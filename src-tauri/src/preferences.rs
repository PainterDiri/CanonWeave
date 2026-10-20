use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use crate::{
    domain::{PreferenceInput, PreferenceSummary, PreferenceTarget},
    error::{AppError, AppResult},
    style,
};

fn target_from_metrics(metrics: &crate::domain::StyleMetrics) -> PreferenceTarget {
    PreferenceTarget {
        avg_sentence_len: metrics.avg_sentence_len,
        dialogue_ratio: metrics.dialogue_ratio,
        sensory_per_1000: metrics.sensory_per_1000,
        metaphor_per_1000: metrics.metaphor_per_1000,
        adverb_per_1000: metrics.adverb_per_1000,
    }
}

fn run_text(conn: &Connection, project_id: &str, run_id: &str) -> AppResult<Option<String>> {
    conn.query_row(
        "SELECT CASE WHEN refined_text <> '' THEN refined_text ELSE raw_text END
         FROM generation_run WHERE project_id=?1 AND id=?2",
        params![project_id, run_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

pub fn save(
    conn: &Connection,
    project_id: &str,
    input: &PreferenceInput,
) -> AppResult<()> {
    if input.left_run_id == input.right_run_id {
        return Err(AppError::Message("A/B 两侧必须是不同版本".into()));
    }
    if input.chosen_run_id != input.left_run_id && input.chosen_run_id != input.right_run_id {
        return Err(AppError::Message("选择的版本必须是 A 或 B".into()));
    }
    let left_text = input
        .left_text
        .clone()
        .or(run_text(conn, project_id, &input.left_run_id)?)
        .unwrap_or_default();
    let right_text = input
        .right_text
        .clone()
        .or(run_text(conn, project_id, &input.right_run_id)?)
        .unwrap_or_default();
    if left_text.trim().is_empty() || right_text.trim().is_empty() {
        return Err(AppError::Message("A/B 偏好需要两侧正文或可读取的生成运行".into()));
    }
    let left = style::analyze(&left_text);
    let right = style::analyze(&right_text);
    let chosen = if input.chosen_run_id == input.left_run_id {
        &left
    } else {
        &right
    };
    let other = if input.chosen_run_id == input.left_run_id {
        &right
    } else {
        &left
    };
    let target = target_from_metrics(chosen);
    let details = json!({
        "target": target,
        "chosenMetrics": chosen,
        "otherMetrics": other,
        "chosenSide": if input.chosen_run_id == input.left_run_id { "left" } else { "right" },
        "algorithm": "local_metric_preference_v1"
    });
    conn.execute(
        "INSERT INTO preference (id, project_id, scene_id, left_run_id, right_run_id,
            chosen_run_id, reason, dimension_scores, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![
            uuid::Uuid::new_v4().to_string(),
            project_id,
            input.scene_id,
            input.left_run_id,
            input.right_run_id,
            input.chosen_run_id,
            input.reason.trim(),
            serde_json::to_string(&details)?,
            crate::now()
        ],
    )?;
    Ok(())
}

pub fn targets(
    conn: &Connection,
    project_id: &str,
    scene_id: Option<&str>,
) -> AppResult<Option<PreferenceTarget>> {
    let sql = if scene_id.is_some() {
        "SELECT dimension_scores FROM preference
         WHERE project_id=?1 AND (scene_id=?2 OR scene_id IS NULL)
         ORDER BY created_at DESC LIMIT 40"
    } else {
        "SELECT dimension_scores FROM preference
         WHERE project_id=?1 ORDER BY created_at DESC LIMIT 40"
    };
    let mut stmt = conn.prepare(sql)?;
    let map = |row: &rusqlite::Row<'_>| row.get::<_, String>(0);
    let rows = if let Some(scene_id) = scene_id {
        stmt.query_map(params![project_id, scene_id], map)?
            .collect::<Result<Vec<_>, _>>()?
    } else {
        stmt.query_map([project_id], map)?
            .collect::<Result<Vec<_>, _>>()?
    };
    drop(stmt);
    let mut target = PreferenceTarget {
        avg_sentence_len: 0,
        dialogue_ratio: 0,
        sensory_per_1000: 0,
        metaphor_per_1000: 0,
        adverb_per_1000: 0,
    };
    let mut count = 0.0;
    for raw in rows {
        let value: Value = match serde_json::from_str(&raw) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let Some(item) = value.get("target") else {
            continue;
        };
        target.avg_sentence_len += item
            .get("avgSentenceLen")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32;
        target.dialogue_ratio += item
            .get("dialogueRatio")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32;
        target.sensory_per_1000 += item
            .get("sensoryPer1000")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32;
        target.metaphor_per_1000 += item
            .get("metaphorPer1000")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32;
        target.adverb_per_1000 += item
            .get("adverbPer1000")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32;
        count += 1.0;
    }
    if count <= 0.0 {
        return Ok(None);
    }
    target.avg_sentence_len = (target.avg_sentence_len as f64 / count).round() as i32;
    target.dialogue_ratio = (target.dialogue_ratio as f64 / count).round() as i32;
    target.sensory_per_1000 = (target.sensory_per_1000 as f64 / count).round() as i32;
    target.metaphor_per_1000 = (target.metaphor_per_1000 as f64 / count).round() as i32;
    target.adverb_per_1000 = (target.adverb_per_1000 as f64 / count).round() as i32;
    Ok(Some(target))
}

pub fn summary(
    conn: &Connection,
    project_id: &str,
    scene_id: Option<&str>,
) -> AppResult<PreferenceSummary> {
    let choices: i32 = conn.query_row(
        "SELECT COUNT(*) FROM preference WHERE project_id=?1",
        [project_id],
        |row| row.get(0),
    )?;
    Ok(PreferenceSummary {
        choices,
        target: targets(conn, project_id, scene_id)?,
    })
}

pub fn score_with_target(text: &str, target: &PreferenceTarget) -> f64 {
    let metrics = style::analyze(text);
    let dimensions = [
        (
            metrics.avg_sentence_len,
            target.avg_sentence_len,
            target.avg_sentence_len.max(12) as f64,
        ),
        (
            metrics.dialogue_ratio,
            target.dialogue_ratio,
            target.dialogue_ratio.max(20) as f64,
        ),
        (
            metrics.sensory_per_1000,
            target.sensory_per_1000,
            target.sensory_per_1000.max(8) as f64,
        ),
        (
            metrics.metaphor_per_1000,
            target.metaphor_per_1000,
            target.metaphor_per_1000.max(3) as f64,
        ),
        (
            metrics.adverb_per_1000,
            target.adverb_per_1000,
            target.adverb_per_1000.max(6) as f64,
        ),
    ];
    let deviation = dimensions
        .iter()
        .map(|(current, target, scale)| {
            ((*current - *target).abs() as f64 / scale).min(1.0)
        })
        .sum::<f64>()
        / dimensions.len() as f64;
    ((1.0 - deviation) * 100.0).clamp(0.0, 100.0)
}

#[cfg(test)]
mod tests {
    use crate::{domain::CreateProjectInput, project};

    use super::*;

    #[test]
    fn choice_records_target_and_scores_closer_text_higher() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = project::create(&CreateProjectInput {
            name: "preference".into(),
            root_path: dir.path().join("story").to_string_lossy().into_owned(),
            mode: "idea".into(),
            writing_language: None,
        })
        .unwrap();
        let pid = loaded.summary.id.clone();
        save(
            &loaded.conn,
            &pid,
            &PreferenceInput {
                scene_id: None,
                left_run_id: "left".into(),
                right_run_id: "right".into(),
                chosen_run_id: "left".into(),
                reason: "更接近想要的短句与对话".into(),
                left_text: Some("雨落下来。他停住。门外没有人。".into()),
                right_text: Some(
                    "雨落在远处漆黑而漫长的街道上，他缓慢地停下来，却始终没有回头。"
                        .into(),
                ),
            },
        )
        .unwrap();
        let summary = summary(&loaded.conn, &pid, None).unwrap();
        assert_eq!(summary.choices, 1);
        assert!(summary.target.is_some());
        let target = summary.target.unwrap();
        let chosen_score = score_with_target("灯灭了。他听见脚步。门开了。", &target);
        let other_score = score_with_target(
            "灯光在漫长的沉默之中缓慢地熄灭，他听见远方传来若隐若现而无法辨认的脚步声。",
            &target,
        );
        assert!(chosen_score > other_score);

        loaded
            .conn
            .execute(
                "INSERT INTO text_fts
                    (content,tokenized,source_id,project_id,source_kind,chapter_id,branch_id)
                 VALUES (?1,?2,'source:pref',?3,'chapter',NULL,NULL)",
                params![
                    "雨落在一条漫长而空寂的旧街上，他缓慢停住，始终没有回头。",
                    crate::retrieval::tokenize(
                        "雨落在一条漫长而空寂的旧街上，他缓慢停住，始终没有回头。",
                    ),
                    pid
                ],
            )
            .unwrap();
        let ranked = crate::retrieval::search_for_scene(
            &loaded.conn,
            &pid,
            "雨",
            4,
            None,
        )
        .unwrap();
        assert!(ranked[0].score > 0 && ranked[0].score < 100);
    }
}
