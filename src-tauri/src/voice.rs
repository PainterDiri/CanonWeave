use std::collections::HashMap;

use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::{
    domain::{
        CharacterVoiceProfile, CharacterVoiceSample, VoiceCheck, VoiceDimensions, VoiceDrift,
        VoiceSampleInput, VoiceSegment, VoiceThresholdInput, VoiceThresholds,
    },
    error::{AppError, AppResult},
    now,
};

const PRONOUNS: &[&str] = &["我们", "我", "你们", "你", "他们", "他", "她们", "她", "您"];
const ACTIONS: &[&str] = &[
    "走", "跑", "握", "推", "拉", "转", "抬", "低", "站", "坐", "看", "听", "摔", "敲",
    "按", "抓", "放", "退", "靠", "停", "伸手", "回头", "低头", "递", "接", "藏",
];

fn empty_dimensions() -> VoiceDimensions {
    VoiceDimensions {
        avg_sentence_len: 0,
        sentence_variance: 0,
        punctuation_per_100: 0,
        pronoun_ratio: 0,
        dialogue_action_ratio: 0,
        dialogue_ratio: 0,
        preferred_pronoun: "—".into(),
    }
}

fn split_sentences(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if matches!(ch, '。' | '！' | '？' | '!' | '?' | '；' | ';' | '\n') {
            if !current.trim().is_empty() {
                result.push(current.trim().to_string());
            }
            current.clear();
        } else {
            current.push(ch);
        }
    }
    if !current.trim().is_empty() {
        result.push(current.trim().to_string());
    }
    result
}

pub fn analyze(text: &str) -> VoiceDimensions {
    let chars = text.chars().filter(|ch| !ch.is_whitespace()).count() as i32;
    if chars == 0 {
        return empty_dimensions();
    }
    let sentences = split_sentences(text);
    let lengths = sentences
        .iter()
        .map(|sentence| sentence.chars().filter(|ch| !ch.is_whitespace()).count() as i32)
        .collect::<Vec<_>>();
    let avg_sentence_len = if lengths.is_empty() {
        0
    } else {
        (lengths.iter().sum::<i32>() as f64 / lengths.len() as f64).round() as i32
    };
    let sentence_variance = if lengths.is_empty() {
        0
    } else {
        (lengths
            .iter()
            .map(|length| {
                let delta = (*length - avg_sentence_len) as f64;
                delta * delta
            })
            .sum::<f64>()
            / lengths.len() as f64)
            .sqrt()
            .round() as i32
    };
    let punctuation = text
        .chars()
        .filter(|ch| matches!(ch, '，' | ',' | '！' | '!' | '？' | '?' | '…' | '；' | ';'))
        .count() as i32;
    let punctuation_per_100 = (punctuation as f64 * 100.0 / chars as f64).round() as i32;
    let (preferred_pronoun, pronoun_count) = PRONOUNS
        .iter()
        .map(|pronoun| (*pronoun, text.matches(*pronoun).count() as i32))
        .max_by_key(|(_, count)| *count)
        .unwrap_or(("—", 0));
    let pronoun_ratio = if pronoun_count == 0 {
        0
    } else {
        (pronoun_count as f64 * 100.0 / chars as f64).round() as i32
    };
    let dialogue_re = Regex::new(r#"[“"]([^”"]*)[”"]"#).expect("dialogue regex");
    let dialogue_chars: i32 = dialogue_re
        .captures_iter(text)
        .map(|cap| cap.get(1).map(|m| m.as_str().chars().count()).unwrap_or(0) as i32)
        .sum();
    let action_count = ACTIONS
        .iter()
        .map(|word| text.matches(word).count() as i32)
        .sum::<i32>();
    let dialogue_action_ratio = if dialogue_chars == 0 && action_count == 0 {
        0
    } else if action_count == 0 {
        100
    } else {
        (dialogue_chars as f64 * 100.0 / (action_count as f64 * 6.0)).round() as i32
    };
    VoiceDimensions {
        avg_sentence_len,
        sentence_variance,
        punctuation_per_100,
        pronoun_ratio,
        dialogue_action_ratio,
        dialogue_ratio: (dialogue_chars as f64 * 100.0 / chars as f64).round() as i32,
        preferred_pronoun: if pronoun_count == 0 {
            "—".into()
        } else {
            preferred_pronoun.into()
        },
    }
}

fn char_offset(text: &str, byte_offset: usize) -> i32 {
    text[..byte_offset.min(text.len())].chars().count() as i32
}

fn paragraphs(text: &str) -> Vec<(usize, String)> {
    let mut result = Vec::new();
    let mut offset = 0;
    for raw in text.split("\n\n") {
        let trimmed_start = raw.len() - raw.trim_start().len();
        let content = raw.trim();
        if !content.is_empty() {
            result.push((offset + trimmed_start, content.to_string()));
        }
        offset += raw.len() + 2;
    }
    result
}

fn character_names(
    conn: &Connection,
    project_id: &str,
    character_id: &str,
) -> AppResult<Vec<String>> {
    let (name, aliases): (String, String) = conn
        .query_row(
            "SELECT canonical_name, aliases FROM entity
             WHERE project_id=?1 AND id=?2 AND type='character'",
            params![project_id, character_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::Message("找不到人物".into()))?;
    let mut names = serde_json::from_str::<Vec<String>>(&aliases).unwrap_or_default();
    names.push(name);
    Ok(names)
}

fn scene_is_pov(
    conn: &Connection,
    project_id: &str,
    scene_id: Option<&str>,
    names: &[String],
) -> AppResult<bool> {
    let Some(scene_id) = scene_id else {
        return Ok(false);
    };
    let pov: Option<String> = conn
        .query_row(
            "SELECT pov FROM scene WHERE project_id=?1 AND id=?2",
            params![project_id, scene_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(pov
        .map(|pov| names.iter().any(|name| name == &pov))
        .unwrap_or(false))
}

fn character_segments(
    conn: &Connection,
    project_id: &str,
    character_id: &str,
    scene_id: Option<&str>,
    text: &str,
) -> AppResult<Vec<VoiceSegment>> {
    let names = character_names(conn, project_id, character_id)?;
    let include_pov = scene_is_pov(conn, project_id, scene_id, &names)?;
    let dialogue_re = Regex::new(r#"[“"]([^”"]*)[”"]"#).expect("dialogue regex");
    let mut segments = Vec::new();
    for (byte_start, paragraph) in paragraphs(text) {
        let has_name = names.iter().any(|name| paragraph.contains(name));
        if has_name {
            for capture in dialogue_re.find_iter(&paragraph) {
                let excerpt = capture.as_str().trim();
                if excerpt.is_empty() {
                    continue;
                }
                let start = char_offset(text, byte_start + capture.start());
                let end = char_offset(text, byte_start + capture.end());
                segments.push(VoiceSegment {
                    index: segments.len() as i32,
                    kind: "dialogue".into(),
                    start,
                    end,
                    excerpt: excerpt.to_string(),
                    metrics: analyze(excerpt),
                });
            }
        }
        if include_pov {
            let start = char_offset(text, byte_start);
            let end = start + paragraph.chars().count() as i32;
            segments.push(VoiceSegment {
                index: segments.len() as i32,
                kind: "pov".into(),
                start,
                end,
                excerpt: paragraph.clone(),
                metrics: analyze(&paragraph),
            });
        }
    }
    if segments.is_empty() {
        return Err(AppError::Message(
            "没有找到该人物的对话或 POV 片段；请补人物署名，或传入该人物作为 POV 的场景".into(),
        ));
    }
    Ok(segments)
}

#[derive(Debug, Clone, Copy)]
struct Average {
    count: f64,
    avg_sentence_len: f64,
    sentence_variance: f64,
    punctuation_per_100: f64,
    pronoun_ratio: f64,
    dialogue_action_ratio: f64,
    dialogue_ratio: f64,
}

impl Average {
    fn add(&mut self, dimensions: &VoiceDimensions) {
        self.count += 1.0;
        self.avg_sentence_len += dimensions.avg_sentence_len as f64;
        self.sentence_variance += dimensions.sentence_variance as f64;
        self.punctuation_per_100 += dimensions.punctuation_per_100 as f64;
        self.pronoun_ratio += dimensions.pronoun_ratio as f64;
        self.dialogue_action_ratio += dimensions.dialogue_action_ratio as f64;
        self.dialogue_ratio += dimensions.dialogue_ratio as f64;
    }

    fn finish(&self, preferred_pronoun: String) -> VoiceDimensions {
        if self.count <= 0.0 {
            return empty_dimensions();
        }
        VoiceDimensions {
            avg_sentence_len: (self.avg_sentence_len / self.count).round() as i32,
            sentence_variance: (self.sentence_variance / self.count).round() as i32,
            punctuation_per_100: (self.punctuation_per_100 / self.count).round() as i32,
            pronoun_ratio: (self.pronoun_ratio / self.count).round() as i32,
            dialogue_action_ratio: (self.dialogue_action_ratio / self.count).round() as i32,
            dialogue_ratio: (self.dialogue_ratio / self.count).round() as i32,
            preferred_pronoun,
        }
    }
}

fn recompute_profile(
    conn: &Connection,
    project_id: &str,
    character_id: &str,
) -> AppResult<CharacterVoiceProfile> {
    let samples = samples(conn, project_id, character_id)?;
    let mut average = Average {
        count: 0.0,
        avg_sentence_len: 0.0,
        sentence_variance: 0.0,
        punctuation_per_100: 0.0,
        pronoun_ratio: 0.0,
        dialogue_action_ratio: 0.0,
        dialogue_ratio: 0.0,
    };
    let mut pronoun_counts: HashMap<String, i32> = HashMap::new();
    for sample in &samples {
        average.add(&sample.metrics);
        *pronoun_counts
            .entry(sample.metrics.preferred_pronoun.clone())
            .or_default() += 1;
    }
    let preferred = pronoun_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(pronoun, _)| pronoun)
        .unwrap_or_else(|| "—".into());
    let centroid = average.finish(preferred);
    let thresholds = conn
        .query_row(
            "SELECT thresholds FROM character_voice_profile
             WHERE project_id=?1 AND character_id=?2",
            params![project_id, character_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
    let ts = now();
    let version: i32 = conn
        .query_row(
            "SELECT version FROM character_voice_profile
             WHERE project_id=?1 AND character_id=?2",
            params![project_id, character_id],
            |row| row.get(0),
        )
        .optional()?
        .map(|version: i32| version + 1)
        .unwrap_or(1);
    conn.execute(
        "INSERT INTO character_voice_profile
            (character_id, project_id, centroid, thresholds, sample_count, version, updated_at)
         VALUES (?1,?2,?3,?4,?5,1,?6)
         ON CONFLICT(character_id) DO UPDATE SET centroid=excluded.centroid,
            thresholds=excluded.thresholds, sample_count=excluded.sample_count,
            version=?7, updated_at=excluded.updated_at",
        params![
            character_id,
            project_id,
            serde_json::to_string(&centroid)?,
            serde_json::to_string(&thresholds)?,
            samples.len() as i32,
            ts,
            version
        ],
    )?;
    Ok(CharacterVoiceProfile {
        character_id: character_id.into(),
        centroid,
        thresholds,
        sample_count: samples.len() as i32,
        version,
        updated_at: Some(ts),
    })
}

fn sample_from_row(row: &Row<'_>) -> rusqlite::Result<CharacterVoiceSample> {
    let metrics: String = row.get(6)?;
    Ok(CharacterVoiceSample {
        id: row.get(0)?,
        character_id: row.get(1)?,
        text: row.get(2)?,
        source_span: row.get(3)?,
        source_type: row.get(4)?,
        metrics: serde_json::from_str(&metrics).unwrap_or_else(|_| empty_dimensions()),
        created_at: row.get(5)?,
    })
}

pub fn samples(
    conn: &Connection,
    project_id: &str,
    character_id: &str,
) -> AppResult<Vec<CharacterVoiceSample>> {
    let mut stmt = conn.prepare(
        "SELECT id, character_id, text, source_span, source_type, created_at, metrics
         FROM character_voice_sample
         WHERE project_id=?1 AND character_id=?2 AND approved=1
         ORDER BY created_at DESC",
    )?;
    let rows = stmt
        .query_map(params![project_id, character_id], sample_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    Ok(rows)
}

pub fn add_sample(
    conn: &Connection,
    project_id: &str,
    input: &VoiceSampleInput,
) -> AppResult<CharacterVoiceSample> {
    character_names(conn, project_id, &input.character_id)?;
    if input.text.chars().filter(|ch| !ch.is_whitespace()).count() < 20 {
        return Err(AppError::Message("声音样本至少需要 20 个非空白字符".into()));
    }
    if input.source_span.trim().is_empty() {
        return Err(AppError::Message("声音样本必须带可追溯来源".into()));
    }
    if !["author", "licensed", "public_domain", "import", "chat"]
        .contains(&input.source_type.as_str())
    {
        return Err(AppError::Message("未知声音样本来源类型".into()));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now();
    let metrics = analyze(&input.text);
    conn.execute(
        "INSERT INTO character_voice_sample
            (id, project_id, character_id, text, source_span, source_type, approved, metrics, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,1,?7,?8)",
        params![
            id,
            project_id,
            input.character_id,
            input.text.trim(),
            input.source_span.trim(),
            input.source_type,
            serde_json::to_string(&metrics)?,
            ts
        ],
    )?;
    recompute_profile(conn, project_id, &input.character_id)?;
    conn.query_row(
        "SELECT id, character_id, text, source_span, source_type, created_at, metrics
         FROM character_voice_sample WHERE id=?1",
        [&id],
        sample_from_row,
    )
    .map_err(Into::into)
}

pub fn profile(
    conn: &Connection,
    project_id: &str,
    character_id: &str,
) -> AppResult<CharacterVoiceProfile> {
    character_names(conn, project_id, character_id)?;
    if let Some((centroid, thresholds, sample_count, version, updated_at)) = conn
        .query_row(
            "SELECT centroid, thresholds, sample_count, version, updated_at
             FROM character_voice_profile WHERE project_id=?1 AND character_id=?2",
            params![project_id, character_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i32>(2)?,
                    row.get::<_, i32>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()?
    {
        return Ok(CharacterVoiceProfile {
            character_id: character_id.into(),
            centroid: serde_json::from_str(&centroid).unwrap_or_else(|_| empty_dimensions()),
            thresholds: serde_json::from_str(&thresholds).unwrap_or_default(),
            sample_count,
            version,
            updated_at: Some(updated_at),
        });
    }
    Ok(CharacterVoiceProfile {
        character_id: character_id.into(),
        centroid: empty_dimensions(),
        thresholds: VoiceThresholds::default(),
        sample_count: 0,
        version: 0,
        updated_at: None,
    })
}

pub fn save_thresholds(
    conn: &Connection,
    project_id: &str,
    input: &VoiceThresholdInput,
) -> AppResult<CharacterVoiceProfile> {
    character_names(conn, project_id, &input.character_id)?;
    let thresholds = VoiceThresholds {
        sentence_len: input.thresholds.sentence_len.clamp(2, 80),
        sentence_variance: input.thresholds.sentence_variance.clamp(2, 100),
        punctuation: input.thresholds.punctuation.clamp(1, 40),
        pronoun: input.thresholds.pronoun.clamp(1, 40),
        dialogue_action: input.thresholds.dialogue_action.clamp(5, 200),
    };
    let current_profile = profile(conn, project_id, &input.character_id)?;
    conn.execute(
        "INSERT INTO character_voice_profile
            (character_id, project_id, centroid, thresholds, sample_count, version, updated_at)
         VALUES (?1,?2,?3,?4,?5,1,?6)
         ON CONFLICT(character_id) DO UPDATE SET thresholds=excluded.thresholds,
            version=version+1, updated_at=excluded.updated_at",
        params![
            input.character_id,
            project_id,
            serde_json::to_string(&current_profile.centroid)?,
            serde_json::to_string(&thresholds)?,
            current_profile.sample_count,
            now()
        ],
    )?;
    profile(conn, project_id, &input.character_id)
}

pub fn check(
    conn: &Connection,
    project_id: &str,
    character_id: &str,
    text: &str,
    scene_id: Option<&str>,
) -> AppResult<VoiceCheck> {
    let profile = profile(conn, project_id, character_id)?;
    if profile.sample_count == 0 {
        return Err(AppError::Message(
            "该人物还没有作者认可的声音样本，不能建立声音质心".into(),
        ));
    }
    let segments = character_segments(conn, project_id, character_id, scene_id, text)?;
    let mut drifts = Vec::new();
    for segment in &segments {
        let mut compare = |dimension: &str,
                           current: i32,
                           target: i32,
                           threshold: i32,
                           suggestion: &str| {
            let delta = current - target;
            if delta.abs() > threshold {
                drifts.push(VoiceDrift {
                    dimension: dimension.into(),
                    current,
                    target,
                    delta,
                    severity: if delta.abs() > threshold * 2 {
                        "major"
                    } else {
                        "minor"
                    }
                    .into(),
                    segment_index: segment.index,
                    start: segment.start,
                    end: segment.end,
                    excerpt: segment.excerpt.chars().take(120).collect(),
                    suggestion: suggestion.into(),
                });
            }
        };
        compare(
            "句长",
            segment.metrics.avg_sentence_len,
            profile.centroid.avg_sentence_len,
            profile.thresholds.sentence_len,
            "对比该人物的认可样本，调整句子呼吸与停顿",
        );
        compare(
            "句长波动",
            segment.metrics.sentence_variance,
            profile.centroid.sentence_variance,
            profile.thresholds.sentence_variance,
            "让长短句分布重新接近人物稳定语感",
        );
        compare(
            "标点频率",
            segment.metrics.punctuation_per_100,
            profile.centroid.punctuation_per_100,
            profile.thresholds.punctuation,
            "检查逗号、问号和省略号是否过密或过少",
        );
        compare(
            "代词偏好",
            segment.metrics.pronoun_ratio,
            profile.centroid.pronoun_ratio,
            profile.thresholds.pronoun,
            "检查人物是否突然改变自称或称呼方式",
        );
        compare(
            "对话动作比",
            segment.metrics.dialogue_action_ratio,
            profile.centroid.dialogue_action_ratio,
            profile.thresholds.dialogue_action,
            "平衡台词、动作与停顿，避免只剩解释性对白",
        );
    }
    Ok(VoiceCheck {
        character_id: character_id.into(),
        centroid: profile.centroid,
        segments,
        voice_drift: !drifts.is_empty(),
        drifts,
    })
}

pub fn context_anchor(
    conn: &Connection,
    project_id: &str,
    character_id: &str,
    max_chars: usize,
) -> AppResult<String> {
    let mut result = String::new();
    for sample in samples(conn, project_id, character_id)?.into_iter().take(3) {
        if result.chars().count() >= max_chars {
            break;
        }
        let remaining = max_chars.saturating_sub(result.chars().count());
        let excerpt: String = sample.text.chars().take(remaining).collect();
        result.push_str(&format!(
            "- {}｜{}｜句长 {}｜标点 {}/百字｜代词 {}\n",
            sample.source_type,
            sample.source_span,
            sample.metrics.avg_sentence_len,
            sample.metrics.punctuation_per_100,
            sample.metrics.preferred_pronoun
        ));
        result.push_str(&excerpt);
        result.push('\n');
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use crate::{domain::CreateProjectInput, project};

    use super::*;

    fn setup() -> (tempfile::TempDir, project::LoadedProject) {
        let dir = tempfile::tempdir().unwrap();
        let loaded = project::create(&CreateProjectInput {
            name: "voice".into(),
            root_path: dir.path().join("story").to_string_lossy().into_owned(),
            mode: "idea".into(),
            writing_language: None,
        })
        .unwrap();
        loaded
            .conn
            .execute(
                "INSERT INTO entity (id,project_id,type,canonical_name,aliases,status,description,created_at,updated_at)
                 VALUES ('lin',?1,'character','林澈','[\"阿澈\"]','confirmed','','now','now')",
                [&loaded.summary.id],
            )
            .unwrap();
        (dir, loaded)
    }

    #[test]
    fn samples_build_centroid_and_trace_explainable_drift_to_fragment() {
        let (_dir, loaded) = setup();
        let pid = loaded.summary.id.clone();
        for (index, text) in [
            "林澈说：“先别下结论。把信给我。”他把信封压在灯下，指尖停了一秒。",
            "林澈说：“不。你先把日期说清楚。”他没有抬头，只把旧钥匙推到桌边。",
        ]
        .into_iter()
        .enumerate()
        {
            add_sample(
                &loaded.conn,
                &pid,
                &VoiceSampleInput {
                    character_id: "lin".into(),
                    text: text.into(),
                    source_span: format!("author:sample:{index}"),
                    source_type: "author".into(),
                },
            )
            .unwrap();
        }
        let checked = check(
            &loaded.conn,
            &pid,
            "lin",
            "林澈说：“这绝对不可能，我早就知道所有人都在欺骗我，而且这封来自未来的信一定隐藏着足以摧毁整个家族的惊人秘密，不管他们如何解释，我都会在今晚把所有真相彻底查清并且让每一个人承担后果。”",
            None,
        )
        .unwrap();
        assert!(checked.voice_drift);
        assert!(!checked.segments.is_empty());
        let drift = checked.drifts.first().unwrap();
        assert!(drift.excerpt.contains("林澈说") || drift.excerpt.contains("绝对不可能"));
        assert!(drift.end > drift.start);
    }

    #[test]
    fn sample_source_and_minimum_length_are_enforced() {
        let (_dir, loaded) = setup();
        let pid = loaded.summary.id.clone();
        let error = add_sample(
            &loaded.conn,
            &pid,
            &VoiceSampleInput {
                character_id: "lin".into(),
                text: "太短。".into(),
                source_span: String::new(),
                source_type: "author".into(),
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("20"));
    }
}
