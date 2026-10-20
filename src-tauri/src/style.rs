use std::sync::OnceLock;

use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::{
    domain::{
        StyleCheck, StyleCompare, StyleDrift, StyleEmotion, StyleImagery, StyleLexicon,
        StyleMetrics, StylePreset, StyleProfile, StyleProfileInput, StyleRatios, StyleRhetoric,
        StyleSample, StyleSampleInput, StyleSyntax, TellIssue,
    },
    error::{AppError, AppResult},
    now,
};

const CLICHES: &[&str] = &[
    "空气仿佛凝固",
    "心中一震",
    "嘴角勾起",
    "眼神复杂",
    "五味杂陈",
    "倒吸一口凉气",
    "心头一紧",
    "不由得",
    "不知为何",
    "说不出话",
    "眼神暗了暗",
];

const SENSORY: &[&str] = &[
    "看", "听", "闻", "尝", "触", "冷", "热", "雨", "光", "声", "味", "痛", "香", "风",
    "湿", "亮", "暗", "腥", "苦", "甜", "粗糙", "冰凉", "滚烫", "刺鼻", "轰鸣", "沉默",
];

const ADVERBS: &[&str] = &[
    "很", "更", "已经", "忽然", "突然", "立刻", "仿佛", "似乎", "好像", "终于", "仍然",
    "依然", "只是", "竟然", "居然", "缓缓", "慢慢", "轻轻",
];

const EMOTIONS: &[&str] = &[
    "愤怒", "悲伤", "害怕", "开心", "难过", "紧张", "恐惧", "绝望", "兴奋", "痛苦", "羞愧",
    "委屈", "焦虑", "厌恶", "欣喜",
];

const ACTIONS: &[&str] = &[
    "走", "跑", "握", "推", "拉", "转", "抬", "低", "站", "坐", "看", "听", "摔", "敲", "按",
    "抓", "放", "退", "靠", "停", "伸手", "回头", "低头",
];

fn split_sentences(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if matches!(ch, '。' | '！' | '？' | '!' | '?' | '；' | ';' | '\n') {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                result.push(trimmed.to_string());
            }
            current.clear();
        } else {
            current.push(ch);
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        result.push(trimmed.to_string());
    }
    result
}

fn count_occurrences(text: &str, words: &[&str]) -> i32 {
    words
        .iter()
        .map(|word| text.matches(word).count() as i32)
        .sum()
}

fn per_thousand(count: i32, chars: i32) -> i32 {
    if chars <= 0 {
        0
    } else {
        ((count as f64) * 1000.0 / chars as f64).round() as i32
    }
}

fn percentile(sorted: &[i32], p: f64) -> i32 {
    if sorted.is_empty() {
        return 0;
    }
    let index = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[index]
}

pub fn analyze(text: &str) -> StyleMetrics {
    let chars = text.chars().filter(|c| !c.is_whitespace()).count() as i32;
    let sentences = split_sentences(text);
    let lengths: Vec<i32> = sentences
        .iter()
        .map(|sentence| sentence.chars().filter(|c| !c.is_whitespace()).count() as i32)
        .collect();
    let avg = if lengths.is_empty() {
        0
    } else {
        (lengths.iter().sum::<i32>() as f64 / lengths.len() as f64).round() as i32
    };
    let mut sorted = lengths.clone();
    sorted.sort_unstable();
    let variance = if lengths.is_empty() {
        0
    } else {
        (lengths
            .iter()
            .map(|len| {
                let diff = (*len - avg) as f64;
                diff * diff
            })
            .sum::<f64>()
            / lengths.len() as f64)
            .sqrt()
            .round() as i32
    };
    let paragraphs: Vec<&str> = text
        .split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    let paragraph_chars: Vec<i32> = paragraphs
        .iter()
        .map(|p| p.chars().filter(|c| !c.is_whitespace()).count() as i32)
        .collect();

    let dialogue_re = Regex::new(r#"[“"]([^”"]*)[”"]"#).expect("dialogue regex");
    let dialogue_chars: i32 = dialogue_re
        .captures_iter(text)
        .map(|cap| cap.get(1).map(|m| m.as_str().chars().count()).unwrap_or(0) as i32)
        .sum();
    let dialogue_ratio = if chars > 0 {
        ((dialogue_chars as f64) * 100.0 / chars as f64).round() as i32
    } else {
        0
    };

    let sensory = count_occurrences(text, SENSORY);
    let adverb = count_occurrences(text, ADVERBS) + text.matches('地').count() as i32;
    let metaphor = ["像", "仿佛", "如同", "犹如", "似的"]
        .iter()
        .map(|word| text.matches(word).count() as i32)
        .sum();
    let cliche_hits: Vec<String> = CLICHES
        .iter()
        .filter(|cliche| text.contains(**cliche))
        .map(|cliche| (*cliche).to_string())
        .collect();

    let mut opening_counts: std::collections::HashMap<String, (i32, String)> =
        std::collections::HashMap::new();
    for sentence in &sentences {
        let opening: String = sentence.chars().take(2).collect();
        if opening.chars().count() < 2 {
            continue;
        }
        let entry = opening_counts
            .entry(opening)
            .or_insert_with(|| (0, sentence.clone()));
        entry.0 += 1;
    }
    let mut repeated_openings: Vec<(String, i32)> = opening_counts
        .into_iter()
        .filter(|(_, (count, _))| *count >= 3)
        .map(|(opening, (count, _))| (opening, count))
        .collect();
    repeated_openings.sort_by(|a, b| b.1.cmp(&a.1));
    let repeated_openings = repeated_openings
        .into_iter()
        .take(3)
        .map(|(opening, _)| opening)
        .collect();

    let mut tell_without_show = Vec::new();
    for sentence in &sentences {
        for emotion in EMOTIONS {
            if !sentence.contains(emotion) {
                continue;
            }
            let has_support = SENSORY.iter().any(|word| sentence.contains(word))
                || ACTIONS.iter().any(|word| sentence.contains(word))
                || quote_re().is_match(sentence);
            if !has_support {
                tell_without_show.push(TellIssue {
                    excerpt: sentence.chars().take(80).collect(),
                    emotion: (*emotion).to_string(),
                    reason: "只有情绪判断，没有动作、感官或环境承接".into(),
                });
            }
            break;
        }
        if tell_without_show.len() >= 6 {
            break;
        }
    }

    StyleMetrics {
        chars,
        sentences: sentences.len() as i32,
        paragraphs: paragraphs.len() as i32,
        avg_sentence_len: avg,
        p90_sentence_len: percentile(&sorted, 0.9),
        sentence_variance: variance,
        avg_paragraph_len: if paragraph_chars.is_empty() {
            0
        } else {
            (paragraph_chars.iter().sum::<i32>() as f64 / paragraph_chars.len() as f64).round()
                as i32
        },
        dialogue_ratio,
        description_ratio: (100 - dialogue_ratio).max(0),
        sensory_per_1000: per_thousand(sensory, chars),
        adverb_per_1000: per_thousand(adverb, chars),
        metaphor_per_1000: per_thousand(metaphor, chars),
        cliche_hits,
        repeated_openings,
        tell_without_show,
    }
}

static REGEX_QUOTE: OnceLock<Regex> = OnceLock::new();
fn quote_re() -> &'static Regex {
    REGEX_QUOTE.get_or_init(|| Regex::new(r#"[“"][^”"]*[”"]"#).expect("quote regex"))
}

pub fn contract(profile: &StyleProfile) -> String {
    let s = &profile.syntax;
    let l = &profile.lexicon;
    let i = &profile.imagery;
    let r = &profile.rhetoric;
    let e = &profile.emotion;
    let ratios = &profile.ratios;
    format!(
        "[STYLE CONTRACT v{}]\n\
         视角：{}，{}，{}，心理距离 {}/5\n\
         句法：平均 {} 至 {} 字，短句比例约 {}%，段落 {} 至 {} 句\n\
         词汇：具体名词优先，语域「{}」，副词密度每千字不超过 {} 次{}\n\
         意象：{}；每场聚焦 1 至 2 个母题，感官词每千字约 {} 次\n\
         修辞：每千字约 {} 个比喻，反讽 {}%，留白 {}%\n\
         情绪：内心戏 {}%，外显 {}%，强度 {} 至 {}/5\n\
         配比：动作 {} / 对话 {} / 描写 {} / 内心 {}\n\
         禁例：{}\n\
         要求：用具体动作、感官与空间关系呈现情绪；不得改变已给出的硬事实。",
        profile.version,
        profile.pov,
        profile.person,
        profile.tense,
        (10 - e.interiority / 10).clamp(1, 5),
        s.avg_sentence_min,
        s.avg_sentence_max,
        s.short_ratio,
        s.paragraph_sentences_min,
        s.paragraph_sentences_max,
        l.register,
        l.adverb_density_max,
        if l.banned_words.is_empty() {
            String::new()
        } else {
            format!("；避免：{}", l.banned_words.join("、"))
        },
        if i.motifs.is_empty() {
            "作者未指定固定母题".to_string()
        } else {
            i.motifs.join("、")
        },
        i.sensory_per_1000,
        r.metaphor_per_1000,
        r.irony,
        r.minimalism,
        e.interiority,
        e.externality,
        e.intensity_min,
        e.intensity_max,
        ratios.action,
        ratios.dialogue,
        ratios.description,
        ratios.interiority,
        if profile.anti_style.is_empty() {
            "无额外禁例".to_string()
        } else {
            profile.anti_style.join("、")
        }
    )
}

pub fn check(text: &str, profile: &StyleProfile) -> StyleCheck {
    let metrics = analyze(text);
    let sentences = split_sentences(text);
    let longest = sentences
        .iter()
        .max_by_key(|s| s.chars().count())
        .cloned()
        .unwrap_or_default();
    let shortest = sentences
        .iter()
        .min_by_key(|s| s.chars().count())
        .cloned()
        .unwrap_or_default();
    let mut drifts = Vec::new();

    let target_min = profile.syntax.avg_sentence_min;
    let target_max = profile.syntax.avg_sentence_max;
    if metrics.avg_sentence_len < target_min || metrics.avg_sentence_len > target_max {
        let diff = if metrics.avg_sentence_len < target_min {
            target_min - metrics.avg_sentence_len
        } else {
            metrics.avg_sentence_len - target_max
        };
        drifts.push(StyleDrift {
            dimension: "句长".into(),
            current: format!("平均 {} 字", metrics.avg_sentence_len),
            target: format!("{target_min} 至 {target_max} 字"),
            delta: format!("偏离 {diff} 字"),
            severity: if diff > 8 { "major" } else { "minor" }.into(),
            excerpt: if metrics.avg_sentence_len > target_max {
                longest.chars().take(100).collect()
            } else {
                shortest.chars().take(100).collect()
            },
            suggestion: if metrics.avg_sentence_len > target_max {
                "拆开连续长句，在动作转折处断句".into()
            } else {
                "合并碎片化短句，补足动作与环境承接".into()
            },
        });
    }

    let target_dialogue = profile.ratios.dialogue;
    let dialogue_diff = (metrics.dialogue_ratio - target_dialogue).abs();
    if dialogue_diff >= 12 {
        drifts.push(StyleDrift {
            dimension: "对话配比".into(),
            current: format!("{}%", metrics.dialogue_ratio),
            target: format!("约 {}%", target_dialogue),
            delta: format!("偏离 {dialogue_diff} 个百分点"),
            severity: if dialogue_diff >= 22 { "major" } else { "minor" }.into(),
            excerpt: sentences.first().cloned().unwrap_or_default(),
            suggestion: if metrics.dialogue_ratio > target_dialogue {
                "把部分对话改写成动作与停顿，减少解释性台词".into()
            } else {
                "让人物在冲突处直接开口，减少叙述转述".into()
            },
        });
    }

    let target_sensory = profile.imagery.sensory_per_1000;
    let sensory_tolerance = (target_sensory / 2).max(4);
    if (metrics.sensory_per_1000 - target_sensory).abs() > sensory_tolerance {
        drifts.push(StyleDrift {
            dimension: "感官密度".into(),
            current: format!("{}/千字", metrics.sensory_per_1000),
            target: format!("约 {}/千字", target_sensory),
            delta: format!("偏离 {}", metrics.sensory_per_1000 - target_sensory),
            severity: "minor".into(),
            excerpt: sentences
                .iter()
                .find(|sentence| SENSORY.iter().any(|word| sentence.contains(word)))
                .cloned()
                .unwrap_or_else(|| text.chars().take(80).collect()),
            suggestion: "补一个可感知的动作或环境细节，避免只写判断".into(),
        });
    }

    if metrics.adverb_per_1000 > profile.lexicon.adverb_density_max {
        drifts.push(StyleDrift {
            dimension: "副词密度".into(),
            current: format!("{}/千字", metrics.adverb_per_1000),
            target: format!("≤ {}/千字", profile.lexicon.adverb_density_max),
            delta: format!("超出 {}", metrics.adverb_per_1000 - profile.lexicon.adverb_density_max),
            severity: "minor".into(),
            excerpt: sentences
                .iter()
                .find(|sentence| ADVERBS.iter().any(|word| sentence.contains(word)))
                .cloned()
                .unwrap_or_default(),
            suggestion: "删去可有可无的程度副词，让动词承担力度".into(),
        });
    }

    for cliche in &metrics.cliche_hits {
        let excerpt = sentences
            .iter()
            .find(|sentence| sentence.contains(cliche))
            .cloned()
            .unwrap_or_default();
        drifts.push(StyleDrift {
            dimension: "陈词与 AI 腔".into(),
            current: cliche.clone(),
            target: "作者禁例".into(),
            delta: "命中词表".into(),
            severity: "major".into(),
            excerpt: excerpt.chars().take(100).collect(),
            suggestion: "改用具体动作、物件或身体反应替代套话".into(),
        });
    }

    if let Some(issue) = metrics.tell_without_show.first() {
        drifts.push(StyleDrift {
            dimension: "画面感".into(),
            current: "tell_without_show".into(),
            target: "动作 / 感官 / 空间关系".into(),
            delta: format!("{} 处抽象情绪", metrics.tell_without_show.len()),
            severity: "minor".into(),
            excerpt: issue.excerpt.clone(),
            suggestion: issue.reason.clone(),
        });
    }

    if !metrics.repeated_openings.is_empty() {
        drifts.push(StyleDrift {
            dimension: "句式开头".into(),
            current: metrics.repeated_openings.join("、"),
            target: "开头有变化".into(),
            delta: "同一开头重复 ≥3 次".into(),
            severity: "minor".into(),
            excerpt: sentences.first().cloned().unwrap_or_default(),
            suggestion: "调整语序，或从物件、声音、动作切入".into(),
        });
    }

    StyleCheck {
        style_drift: !drifts.is_empty(),
        drifts,
        metrics,
    }
}

pub fn compare(left_text: &str, right_text: &str) -> StyleCompare {
    let left = analyze(left_text);
    let right = analyze(right_text);
    let mut deltas = Vec::new();
    let mut push = |dimension: &str, l: i32, r: i32, unit: &str, target: &str| {
        if l != r {
            deltas.push(StyleDrift {
                dimension: dimension.into(),
                current: format!("A：{l}{unit}"),
                target: format!("B：{r}{unit}"),
                delta: format!("{:+.0}%", if l == 0 { 0.0 } else { (r - l) as f64 * 100.0 / l as f64 }),
                severity: "info".into(),
                excerpt: target.into(),
                suggestion: String::new(),
            });
        }
    };
    push("平均句长", left.avg_sentence_len, right.avg_sentence_len, " 字", "");
    push("对话占比", left.dialogue_ratio, right.dialogue_ratio, "%", "");
    push("感官密度", left.sensory_per_1000, right.sensory_per_1000, "/千字", "");
    push("比喻密度", left.metaphor_per_1000, right.metaphor_per_1000, "/千字", "");
    push("副词密度", left.adverb_per_1000, right.adverb_per_1000, "/千字", "");
    StyleCompare { left, right, deltas }
}

pub fn presets() -> &'static Vec<StylePreset> {
    static PRESETS: OnceLock<Vec<StylePreset>> = OnceLock::new();
    PRESETS.get_or_init(|| {
        vec![
            preset(
                "cold_restrained",
                "冷峻克制",
                "短句控制情绪，意象少而准，不解释动机。",
                (16, 24, 38),
                (70, "克制", 6, 8),
                (2, vec!["雨", "旧灯"]),
                (2, 10, 60),
                (45, 55, 2, 4),
                (35, 18, 37, 10),
                vec!["总结式抒情", "解释动机", "四字排比"],
            ),
            preset(
                "hot_blood",
                "热血爽文",
                "短句高频推进，动作明确，情绪外显。",
                (12, 20, 58),
                (55, "直接", 8, 12),
                (5, vec!["火光", "拳风"]),
                (4, 8, 35),
                (35, 65, 3, 5),
                (50, 25, 15, 10),
                vec!["长篇内心独白", "拖沓环境描写"],
            ),
            preset(
                "classical",
                "古典雅致",
                "句法舒展，语域文雅，重视留白与意象层叠。",
                (24, 36, 18),
                (80, "文雅", 5, 6),
                (9, vec!["月", "竹", "旧纸"]),
                (5, 20, 50),
                (55, 40, 2, 4),
                (20, 25, 40, 15),
                vec!["网络热词", "现代口语", "四字堆砌"],
            ),
            preset(
                "suspense_cold",
                "悬疑冷感",
                "信息控制优先，感官指向异常细节，少解释。",
                (14, 24, 45),
                (75, "冷静", 5, 8),
                (3, vec!["旧信", "门缝", "湿墙"]),
                (1, 15, 60),
                (50, 45, 2, 4),
                (30, 25, 35, 10),
                vec!["提前解释谜底", "全知视角泄露"],
            ),
            preset(
                "warm_healing",
                "温柔治愈",
                "感官柔和，留出呼吸感，让关系在细节里发生。",
                (18, 28, 32),
                (50, "温和", 10, 12),
                (7, vec!["热水", "旧毛毯", "晨光"]),
                (3, 5, 40),
                (30, 45, 2, 4),
                (25, 30, 30, 15),
                vec!["说教", "强行圆满"],
            ),
            preset(
                "black_humor",
                "黑色幽默",
                "反讽与错位构成笑点，情绪不直接落地。",
                (15, 26, 42),
                (65, "冷幽默", 8, 10),
                (3, vec!["账单", "旧钟"]),
                (4, 50, 30),
                (45, 50, 3, 4),
                (30, 30, 25, 15),
                vec!["解释笑点", "总结式升华"],
            ),
            preset(
                "cinematic_hardboiled",
                "电影感硬派",
                "强动作与空间调度，少心理描写，镜头冷静。",
                (12, 20, 52),
                (65, "冷硬", 4, 6),
                (10, vec!["霓虹", "雨棚", "车门"]),
                (2, 10, 65),
                (45, 55, 3, 5),
                (50, 20, 25, 5),
                vec!["解释动机", "总结式抒情"],
            ),
        ]
    })
}

#[allow(clippy::too_many_arguments)]
fn preset(
    id: &str,
    name: &str,
    description: &str,
    syntax: (i32, i32, i32),
    lexicon: (i32, &str, i32, i32),
    imagery: (i32, Vec<&str>),
    rhetoric: (i32, i32, i32),
    emotion: (i32, i32, i32, i32),
    ratios: (i32, i32, i32, i32),
    anti: Vec<&str>,
) -> StylePreset {
    StylePreset {
        id: id.into(),
        name: name.into(),
        description: description.into(),
        syntax: StyleSyntax {
            avg_sentence_min: syntax.0,
            avg_sentence_max: syntax.1,
            short_ratio: syntax.2,
            paragraph_sentences_min: 3,
            paragraph_sentences_max: 5,
        },
        lexicon: StyleLexicon {
            concrete_ratio: lexicon.0,
            register: lexicon.1.into(),
            adverb_density_max: lexicon.2,
            banned_words: vec![lexicon.3.to_string()],
        },
        imagery: StyleImagery {
            sensory_channels: vec!["视觉".into(), "触觉".into(), "听觉".into()],
            motifs: imagery.1.into_iter().map(str::to_string).collect(),
            sensory_per_1000: imagery.0,
        },
        rhetoric: StyleRhetoric {
            metaphor_per_1000: rhetoric.0,
            irony: rhetoric.1,
            minimalism: rhetoric.2,
        },
        emotion: StyleEmotion {
            interiority: emotion.0,
            externality: emotion.1,
            intensity_min: emotion.2,
            intensity_max: emotion.3,
        },
        ratios: StyleRatios {
            action: ratios.0,
            dialogue: ratios.1,
            description: ratios.2,
            interiority: ratios.3,
        },
        anti_style: anti.into_iter().map(str::to_string).collect(),
    }
}

fn profile_from_row(row: &Row<'_>) -> rusqlite::Result<StyleProfile> {
    let syntax: String = row.get(7)?;
    let lexicon: String = row.get(8)?;
    let imagery: String = row.get(9)?;
    let rhetoric: String = row.get(10)?;
    let emotion: String = row.get(11)?;
    let ratios: String = row.get(12)?;
    let anti: String = row.get(13)?;
    Ok(StyleProfile {
        id: row.get(0)?,
        scope: row.get(1)?,
        scope_id: row.get(2)?,
        name: row.get(3)?,
        pov: row.get(4)?,
        person: row.get(5)?,
        tense: row.get(6)?,
        syntax: serde_json::from_str(&syntax).unwrap_or_else(|_| presets()[0].syntax.clone()),
        lexicon: serde_json::from_str(&lexicon).unwrap_or_else(|_| presets()[0].lexicon.clone()),
        imagery: serde_json::from_str(&imagery).unwrap_or_else(|_| presets()[0].imagery.clone()),
        rhetoric: serde_json::from_str(&rhetoric).unwrap_or_else(|_| presets()[0].rhetoric.clone()),
        emotion: serde_json::from_str(&emotion).unwrap_or_else(|_| presets()[0].emotion.clone()),
        ratios: serde_json::from_str(&ratios).unwrap_or_else(|_| presets()[0].ratios.clone()),
        anti_style: serde_json::from_str(&anti).unwrap_or_default(),
        version: row.get(14)?,
        status: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
    })
}

const SELECT: &str = "SELECT id, scope, scope_id, name, pov, person, tense, syntax, lexicon,
    imagery, rhetoric, emotion, ratios, anti_style, version, status, created_at, updated_at
    FROM style_profile";

pub fn list(conn: &Connection, project_id: &str) -> AppResult<Vec<StyleProfile>> {
    let mut stmt = conn.prepare(&format!("{SELECT} WHERE project_id=?1 ORDER BY updated_at DESC"))?;
    let rows = stmt
        .query_map([project_id], profile_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    Ok(rows)
}

pub fn get(conn: &Connection, project_id: &str, id: &str) -> AppResult<StyleProfile> {
    conn.query_row(
        &format!("{SELECT} WHERE project_id=?1 AND id=?2"),
        params![project_id, id],
        profile_from_row,
    )
    .optional()?
    .ok_or_else(|| AppError::Message("找不到文笔档案".into()))
}

pub fn save(
    conn: &Connection,
    project_id: &str,
    input: &StyleProfileInput,
) -> AppResult<StyleProfile> {
    if input.name.trim().is_empty() {
        return Err(AppError::Message("文笔档案需要名称".into()));
    }
    if !["project", "chapter", "character", "scene_type", "scene"].contains(&input.scope.as_str()) {
        return Err(AppError::Message("未知文笔作用域".into()));
    }
    if input.scope != "project" && input.scope_id.as_deref().unwrap_or("").is_empty() {
        return Err(AppError::Message("非项目级文笔档案需要作用域对象".into()));
    }
    let ts = now();
    let id = if let Some(id) = &input.id {
        get(conn, project_id, id)?;
        conn.execute(
            "UPDATE style_profile SET scope=?3, scope_id=?4, name=?5, pov=?6, person=?7, tense=?8,
                syntax=?9, lexicon=?10, imagery=?11, rhetoric=?12, emotion=?13, ratios=?14,
                anti_style=?15, version=version+1, updated_at=?16
             WHERE project_id=?1 AND id=?2",
            params![
                project_id,
                id,
                input.scope,
                input.scope_id,
                input.name.trim(),
                input.pov,
                input.person,
                input.tense,
                serde_json::to_string(&input.syntax)?,
                serde_json::to_string(&input.lexicon)?,
                serde_json::to_string(&input.imagery)?,
                serde_json::to_string(&input.rhetoric)?,
                serde_json::to_string(&input.emotion)?,
                serde_json::to_string(&input.ratios)?,
                serde_json::to_string(&input.anti_style)?,
                ts
            ],
        )?;
        id.clone()
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO style_profile (id, project_id, scope, scope_id, name, pov, person, tense,
                syntax, lexicon, imagery, rhetoric, emotion, ratios, anti_style, version, status,
                created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,1,'confirmed',?16,?16)",
            params![
                id,
                project_id,
                input.scope,
                input.scope_id,
                input.name.trim(),
                input.pov,
                input.person,
                input.tense,
                serde_json::to_string(&input.syntax)?,
                serde_json::to_string(&input.lexicon)?,
                serde_json::to_string(&input.imagery)?,
                serde_json::to_string(&input.rhetoric)?,
                serde_json::to_string(&input.emotion)?,
                serde_json::to_string(&input.ratios)?,
                serde_json::to_string(&input.anti_style)?,
                ts
            ],
        )?;
        id
    };
    get(conn, project_id, &id)
}

pub fn save_preset(
    conn: &Connection,
    project_id: &str,
    preset_id: &str,
    name: Option<&str>,
) -> AppResult<StyleProfile> {
    let preset = presets()
        .iter()
        .find(|preset| preset.id == preset_id)
        .ok_or_else(|| AppError::Message("未知文笔预设".into()))?;
    save(
        conn,
        project_id,
        &StyleProfileInput {
            id: None,
            scope: "project".into(),
            scope_id: None,
            name: name.unwrap_or(&preset.name).to_string(),
            pov: "第三人称限知".into(),
            person: "第三人称".into(),
            tense: "过去时".into(),
            syntax: preset.syntax.clone(),
            lexicon: preset.lexicon.clone(),
            imagery: preset.imagery.clone(),
            rhetoric: preset.rhetoric.clone(),
            emotion: preset.emotion.clone(),
            ratios: preset.ratios.clone(),
            anti_style: preset.anti_style.clone(),
        },
    )
}

pub fn samples(conn: &Connection, project_id: &str, profile_id: Option<&str>) -> AppResult<Vec<StyleSample>> {
    let mut sql = "SELECT id, profile_id, text, source_span, source_type, approved, tags, created_at
        FROM style_sample WHERE project_id=?1".to_string();
    if profile_id.is_some() {
        sql.push_str(" AND profile_id=?2");
    }
    sql.push_str(" ORDER BY created_at DESC");
    let mut stmt = conn.prepare(&sql)?;
    let map = |row: &Row<'_>| {
        let tags: String = row.get(6)?;
        Ok(StyleSample {
            id: row.get(0)?,
            profile_id: row.get(1)?,
            text: row.get(2)?,
            source_span: row.get(3)?,
            source_type: row.get(4)?,
            approved: row.get(5)?,
            tags: serde_json::from_str(&tags).unwrap_or_default(),
            created_at: row.get(7)?,
        })
    };
    let rows = if let Some(profile_id) = profile_id {
        stmt.query_map(params![project_id, profile_id], map)?
            .collect::<Result<Vec<_>, _>>()?
    } else {
        stmt.query_map([project_id], map)?
            .collect::<Result<Vec<_>, _>>()?
    };
    drop(stmt);
    Ok(rows)
}

pub fn add_sample(
    conn: &Connection,
    project_id: &str,
    input: &StyleSampleInput,
) -> AppResult<StyleSample> {
    if input.text.trim().chars().count() < 40 {
        return Err(AppError::Message("样本太短，请至少粘贴 40 字".into()));
    }
    let allowed = ["author", "licensed", "public_domain", "chat", "import"];
    if !allowed.contains(&input.source_type.as_str()) {
        return Err(AppError::Message(
            "样本来源必须是作者原创、已授权、公版、对话或导入稿件；不支持按在世作家姓名仿写".into(),
        ));
    }
    if input.source_span.trim().is_empty() {
        return Err(AppError::Message("样本必须带来源标记".into()));
    }
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO style_sample (id, project_id, profile_id, text, source_span, source_type,
            approved, tags, created_at) VALUES (?1,?2,?3,?4,?5,?6,1,?7,?8)",
        params![
            id,
            project_id,
            input.profile_id,
            input.text.trim(),
            input.source_span.trim(),
            input.source_type,
            serde_json::to_string(&input.tags)?,
            now()
        ],
    )?;
    Ok(samples(conn, project_id, input.profile_id.as_deref())?
        .into_iter()
        .find(|sample| sample.id == id)
        .ok_or_else(|| AppError::Message("样本写入失败".into()))?)
}

pub fn calibrate(
    conn: &Connection,
    project_id: &str,
    profile_id: &str,
) -> AppResult<StyleProfile> {
    let profile = get(conn, project_id, profile_id)?;
    let samples = samples(conn, project_id, Some(profile_id))?;
    let joined = samples
        .iter()
        .map(|sample| sample.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    if joined.chars().count() < 120 {
        return Err(AppError::Message(
            "至少需要 3 段、合计 120 字以上的作者样本才能校准".into(),
        ));
    }
    let metrics = analyze(&joined);
    let syntax = StyleSyntax {
        avg_sentence_min: (metrics.avg_sentence_len - 3).max(6),
        avg_sentence_max: metrics.avg_sentence_len + 3,
        short_ratio: profile.syntax.short_ratio,
        paragraph_sentences_min: profile.syntax.paragraph_sentences_min,
        paragraph_sentences_max: profile.syntax.paragraph_sentences_max,
    };
    let lexicon = StyleLexicon {
        adverb_density_max: (metrics.adverb_per_1000 + 2).max(3),
        ..profile.lexicon.clone()
    };
    let imagery = StyleImagery {
        sensory_per_1000: metrics.sensory_per_1000.max(4),
        ..profile.imagery.clone()
    };
    let rhetoric = StyleRhetoric {
        metaphor_per_1000: metrics.metaphor_per_1000.max(1),
        ..profile.rhetoric.clone()
    };
    let ratios = StyleRatios {
        dialogue: metrics.dialogue_ratio.clamp(5, 70),
        description: (100 - metrics.dialogue_ratio).clamp(10, 90),
        ..profile.ratios.clone()
    };
    save(
        conn,
        project_id,
        &StyleProfileInput {
            id: Some(profile.id),
            scope: profile.scope,
            scope_id: profile.scope_id,
            name: profile.name,
            pov: profile.pov,
            person: profile.person,
            tense: profile.tense,
            syntax,
            lexicon,
            imagery,
            rhetoric,
            emotion: profile.emotion,
            ratios,
            anti_style: profile.anti_style,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project;

    fn setup() -> (tempfile::TempDir, crate::project::LoadedProject) {
        let dir = tempfile::tempdir().unwrap();
        let loaded = project::create(&crate::domain::CreateProjectInput {
            name: "t".into(),
            root_path: dir.path().join("p").to_string_lossy().into_owned(),
            mode: "idea".into(),
            writing_language: None,
        })
        .unwrap();
        (dir, loaded)
    }

    #[test]
    fn metrics_expose_sentence_and_dialogue_differences() {
        let long = "雨落在旧灯上，林澈把信摊开，纸页的边缘被水汽泡得发软。母亲站在门口，没有进来，也没有说话。\n\n他听见楼下有人在喊父亲的名字，声音隔着雨，像从很久以前传过来。";
        let short = "雨停了。\n\n林澈推门。\n\n“走。”母亲说。\n\n他没有回头。";
        let a = analyze(long);
        let b = analyze(short);
        assert!(a.avg_sentence_len > b.avg_sentence_len);
        assert_ne!(a.dialogue_ratio, b.dialogue_ratio);
        assert!(compare(long, short).deltas.iter().any(|d| d.dimension == "平均句长"));
    }

    #[test]
    fn cliche_and_tell_without_show_are_flagged() {
        let metrics = analyze("他很愤怒。空气仿佛凝固。");
        assert!(metrics.cliche_hits.iter().any(|hit| hit == "空气仿佛凝固"));
        assert!(!metrics.tell_without_show.is_empty());
    }

    #[test]
    fn presets_and_contract_save() {
        let (_dir, loaded) = setup();
        assert_eq!(presets().len(), 7);
        let profile = save_preset(&loaded.conn, &loaded.summary.id, "cold_restrained", None).unwrap();
        let text = contract(&profile);
        assert!(text.contains("STYLE CONTRACT"));
        assert!(text.contains("禁例"));
        let again = save(
            &loaded.conn,
            &loaded.summary.id,
            &StyleProfileInput {
                id: Some(profile.id),
                scope: profile.scope,
                scope_id: profile.scope_id,
                name: "冷峻克制 v2".into(),
                pov: profile.pov,
                person: profile.person,
                tense: profile.tense,
                syntax: profile.syntax,
                lexicon: profile.lexicon,
                imagery: profile.imagery,
                rhetoric: profile.rhetoric,
                emotion: profile.emotion,
                ratios: profile.ratios,
                anti_style: profile.anti_style,
            },
        )
        .unwrap();
        assert_eq!(again.version, 2);
    }

    #[test]
    fn rejects_unattributed_samples() {
        let (_dir, loaded) = setup();
        let err = add_sample(
            &loaded.conn,
            &loaded.summary.id,
            &StyleSampleInput {
                profile_id: None,
                text: "这是一段足够长的测试样本，用来确保来源校验生效。".repeat(4),
                source_span: "".into(),
                source_type: "author".into(),
                tags: vec![],
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("来源"));
    }

    #[test]
    fn sorts_sentence_lengths() {
        let mut values = vec![3, 5, 8, 13, 21];
        values.sort_unstable();
        assert_eq!(percentile(&values, 0.9), 21);
        assert_eq!(values.first().copied(), Some(3));
    }
}
