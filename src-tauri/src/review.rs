use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::json;

use crate::{
    candidates,
    domain::{PolicyApplyResult, ReviewPolicy, ReviewPolicyInput},
    error::{AppError, AppResult},
    now,
};

fn from_row(row: &Row<'_>) -> rusqlite::Result<ReviewPolicy> {
    let sample_rate: f64 = row.get(5)?;
    Ok(ReviewPolicy {
        id: row.get(0)?,
        scope: row.get(1)?,
        scope_id: row.get(2)?,
        mode: row.get(3)?,
        auto_accept_level: row.get(4)?,
        sample_rate: sample_rate.round() as i32,
        lock_hard: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

pub fn get(
    conn: &Connection,
    project_id: &str,
    scope: &str,
    scope_id: Option<&str>,
) -> AppResult<ReviewPolicy> {
    conn.query_row(
        "SELECT id, scope, scope_id, mode, auto_accept_level, sample_rate, lock_hard, updated_at
         FROM review_policy WHERE project_id=?1 AND scope=?2 AND
            ((scope_id IS NULL AND ?3 IS NULL) OR scope_id=?3)
         ORDER BY updated_at DESC LIMIT 1",
        params![project_id, scope, scope_id],
        from_row,
    )
    .optional()?
    .or_else(|| {
        conn.query_row(
            "SELECT id, scope, scope_id, mode, auto_accept_level, sample_rate, lock_hard, updated_at
             FROM review_policy WHERE project_id=?1 AND scope='project'
             ORDER BY updated_at DESC LIMIT 1",
            [project_id],
            from_row,
        )
        .optional()
        .ok()
        .flatten()
    })
    .ok_or_else(|| AppError::Message("项目缺少审核策略".into()))
}

pub fn save(
    conn: &mut Connection,
    project_id: &str,
    input: &ReviewPolicyInput,
) -> AppResult<ReviewPolicy> {
    let (level, sample_rate) = match input.mode.as_str() {
        "strict" => ("none", 0),
        "balanced" => ("L0", 0),
        "fast" => ("L1", 10),
        "draft" => ("none", 0),
        other => return Err(AppError::Message(format!("未知审核策略：{other}"))),
    };
    if !["project", "chapter", "scene"].contains(&input.scope.as_str()) {
        return Err(AppError::Message("未知审核作用域".into()));
    }
    let ts = now();
    let tx = conn.transaction()?;
    let existing: Option<String> = tx
        .query_row(
            "SELECT id FROM review_policy WHERE project_id=?1 AND scope=?2 AND
                ((scope_id IS NULL AND ?3 IS NULL) OR scope_id=?3)
             ORDER BY updated_at DESC LIMIT 1",
            params![project_id, input.scope, input.scope_id],
            |row| row.get(0),
        )
        .optional()?;
    let policy_id = if let Some(id) = existing {
        tx.execute(
            "UPDATE review_policy SET mode=?2, auto_accept_level=?3, sample_rate=?4,
                lock_hard=1, updated_at=?5 WHERE id=?1",
            params![id, input.mode, level, sample_rate, ts],
        )?;
        id
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO review_policy (id, project_id, scope, scope_id, mode,
                auto_accept_level, sample_rate, lock_hard, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,1,?8)",
            params![
                id,
                project_id,
                input.scope,
                input.scope_id,
                input.mode,
                level,
                sample_rate,
                ts
            ],
        )?;
        id
    };
    tx.execute(
        "INSERT INTO audit_log (id, project_id, action, subject, policy_id, detail, created_at)
         VALUES (?1,?2,'review_policy_switch',?3,?4,?5,?6)",
        params![
            uuid::Uuid::new_v4().to_string(),
            project_id,
            format!("{}:{}", input.scope, input.scope_id.clone().unwrap_or_default()),
            policy_id,
            json!({"mode": input.mode, "level": level, "sampleRate": sample_rate}).to_string(),
            ts
        ],
    )?;
    tx.commit()?;
    get(conn, project_id, &input.scope, input.scope_id.as_deref())
}

pub fn apply(
    conn: &mut Connection,
    project_id: &str,
    policy: &ReviewPolicy,
    ids: Option<&[String]>,
) -> AppResult<PolicyApplyResult> {
    if policy.mode == "draft" {
        return Ok(PolicyApplyResult {
            mode: policy.mode.clone(),
            accepted: 0,
            sampled: 0,
            batch_id: None,
            revision_id: None,
            message: "草稿策略只报告硬冲突，不写入权威库".into(),
        });
    }
    let levels: &[&str] = match policy.mode.as_str() {
        "fast" => &["L0", "L1"],
        "balanced" => &["L0"],
        "strict" => &[],
        _ => &[],
    };
    if levels.is_empty() {
        return Ok(PolicyApplyResult {
            mode: policy.mode.clone(),
            accepted: 0,
            sampled: 0,
            batch_id: None,
            revision_id: None,
            message: "严格策略不自动通过任何候选".into(),
        });
    }
    let pending = candidates::list(conn, project_id, Some("pending"), None)?;
    let candidate_ids: Vec<String> = pending
        .into_iter()
        .filter(|candidate| {
            levels.contains(&candidate.risk_level.as_str())
                && ids
                    .map(|ids| ids.contains(&candidate.id))
                    .unwrap_or(true)
        })
        .map(|candidate| candidate.id)
        .collect();
    if candidate_ids.is_empty() {
        return Ok(PolicyApplyResult {
            mode: policy.mode.clone(),
            accepted: 0,
            sampled: 0,
            batch_id: None,
            revision_id: None,
            message: "没有符合当前策略的低风险候选".into(),
        });
    }
    let outcome = candidates::batch_accept_selective(
        conn,
        project_id,
        &candidate_ids,
        None,
        levels,
        Some(&policy.id),
        &policy.mode,
        policy.sample_rate,
    )?;
    Ok(PolicyApplyResult {
        mode: policy.mode.clone(),
        accepted: outcome.accepted,
        sampled: outcome.sampled,
        batch_id: Some(outcome.batch_id),
        revision_id: Some(outcome.revision_id),
        message: format!(
            "按{}策略接受 {} 条，抽样复核 {} 条；L2/L3 保持在队列中",
            match policy.mode.as_str() {
                "fast" => "快速",
                "balanced" => "平衡",
                _ => "当前",
            },
            outcome.accepted,
            outcome.sampled
        ),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{candidates, project};

    use super::*;

    fn setup() -> (tempfile::TempDir, crate::project::LoadedProject) {
        let dir = tempfile::tempdir().unwrap();
        let loaded = project::create(&crate::domain::CreateProjectInput {
            name: "t".into(),
            root_path: dir.path().join("p").to_string_lossy().into_owned(),
            mode: "import".into(),
            writing_language: None,
        })
        .unwrap();
        (dir, loaded)
    }

    #[test]
    fn policies_auto_accept_different_risk_levels_and_record_switches() {
        let (_dir, mut loaded) = setup();
        let pid = loaded.summary.id.clone();
        let l0 = candidates::insert(
            &loaded.conn,
            &pid,
            "style_sample",
            json!({"text":"这是一段作者认可的文字，用于测试自动通过策略。"}),
            Some("ch01｜原文引用"),
            None,
            0.9,
        )
        .unwrap();
        assert_eq!(l0.risk_level, "L0");
        let l1 = candidates::insert(
            &loaded.conn,
            &pid,
            "entity",
            json!({"type":"place","canonicalName":"旧车站"}),
            Some("ch01"),
            None,
            0.9,
        )
        .unwrap();
        assert_eq!(l1.risk_level, "L1");
        let l3 = candidates::insert(
            &loaded.conn,
            &pid,
            "entity",
            json!({"type":"place","canonicalName":"危险地点"}),
            Some("ch02"),
            None,
            0.9,
        )
        .unwrap();
        loaded
            .conn
            .execute(
                "UPDATE candidate SET risk_level='L3' WHERE id=?1",
                [&l3.id],
            )
            .unwrap();
        let balanced = save(
            &mut loaded.conn,
            &pid,
            &ReviewPolicyInput {
                scope: "project".into(),
                scope_id: None,
                mode: "balanced".into(),
            },
        )
        .unwrap();
        let result = apply(&mut loaded.conn, &pid, &balanced, None).unwrap();
        assert_eq!(result.accepted, 1);
        let remaining = candidates::list(&loaded.conn, &pid, Some("pending"), None).unwrap();
        assert!(remaining.iter().any(|c| c.id == l1.id));
        let fast = save(
            &mut loaded.conn,
            &pid,
            &ReviewPolicyInput {
                scope: "project".into(),
                scope_id: None,
                mode: "fast".into(),
            },
        )
        .unwrap();
        let result = apply(&mut loaded.conn, &pid, &fast, None).unwrap();
        assert_eq!(result.accepted, 1);
        let remaining = candidates::list(&loaded.conn, &pid, Some("pending"), None).unwrap();
        assert!(remaining.iter().any(|c| c.id == l3.id));
        let samples: i64 = loaded
            .conn
            .query_row("SELECT COUNT(*) FROM review_sample", [], |row| row.get(0))
            .unwrap();
        assert!(samples >= 2);
        let switches: i64 = loaded
            .conn
            .query_row(
                "SELECT COUNT(*) FROM audit_log WHERE action='review_policy_switch'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(switches, 2);
    }

    #[test]
    fn draft_policy_never_writes() {
        let (_dir, mut loaded) = setup();
        let pid = loaded.summary.id.clone();
        candidates::insert(
            &loaded.conn,
            &pid,
            "style_sample",
            json!({"text":"足够长的测试样本文本，用于草稿策略不写入测试。"}),
            Some("ch01｜引用"),
            None,
            0.9,
        )
        .unwrap();
        let draft = save(
            &mut loaded.conn,
            &pid,
            &ReviewPolicyInput {
                scope: "project".into(),
                scope_id: None,
                mode: "draft".into(),
            },
        )
        .unwrap();
        let result = apply(&mut loaded.conn, &pid, &draft, None).unwrap();
        assert_eq!(result.accepted, 0);
        assert_eq!(
            candidates::list(&loaded.conn, &pid, Some("pending"), None)
                .unwrap()
                .len(),
            1
        );
    }
}
