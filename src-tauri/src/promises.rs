use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::Value;

use crate::{
    domain::{PayoffCheck, PromiseInput, PromisePayoffInput, PromiseRecord, PromiseStateInput},
    error::{AppError, AppResult},
    now,
};

fn from_row(row: &Row<'_>) -> rusqlite::Result<PromiseRecord> {
    let evidence: String = row.get(6)?;
    let plant_evidence: String = row.get(9)?;
    let payoff_evidence: String = row.get(10)?;
    Ok(PromiseRecord {
        id: row.get(0)?,
        title: row.get(1)?,
        state: row.get(2)?,
        plant_scene_id: row.get(3)?,
        target_window: row.get(4)?,
        payoff_contract: row.get(5)?,
        evidence_ids: serde_json::from_str(&evidence).unwrap_or_default(),
        plant_evidence_ids: serde_json::from_str(&plant_evidence).unwrap_or_default(),
        payoff_evidence_ids: serde_json::from_str(&payoff_evidence).unwrap_or_default(),
        payoff_state_change: row.get(11)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

const SELECT: &str = "SELECT id, title, state, plant_scene_id, target_window, payoff_contract,
    evidence_ids, created_at, updated_at, plant_evidence_ids, payoff_evidence_ids,
    payoff_state_change FROM promise";

pub fn list(conn: &Connection, project_id: &str) -> AppResult<Vec<PromiseRecord>> {
    let mut stmt = conn.prepare(&format!(
        "{SELECT} WHERE project_id=?1 ORDER BY CASE state WHEN 'ready' THEN 0 WHEN 'reinforced' THEN 1 WHEN 'planted' THEN 2 WHEN 'planned' THEN 3 ELSE 4 END, created_at"
    ))?;
    let rows = stmt
        .query_map([project_id], from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    Ok(rows)
}

pub fn get(conn: &Connection, project_id: &str, id: &str) -> AppResult<PromiseRecord> {
    conn.query_row(
        &format!("{SELECT} WHERE project_id=?1 AND id=?2"),
        params![project_id, id],
        from_row,
    )
    .optional()?
    .ok_or_else(|| AppError::Message("找不到伏笔".into()))
}

pub fn save(conn: &Connection, project_id: &str, input: &PromiseInput) -> AppResult<PromiseRecord> {
    if input.title.trim().is_empty() {
        return Err(AppError::Message("伏笔标题不能为空".into()));
    }
    let ts = now();
    let id = if let Some(id) = &input.id {
        get(conn, project_id, id)?;
        conn.execute(
            "UPDATE promise SET title=?3, target_window=?4, payoff_contract=?5, updated_at=?6
             WHERE project_id=?1 AND id=?2",
            params![
                project_id,
                id,
                input.title.trim(),
                input.target_window.trim(),
                input.payoff_contract.trim(),
                ts
            ],
        )?;
        id.clone()
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO promise (id, project_id, title, state, target_window, payoff_contract,
                evidence_ids, created_at, updated_at) VALUES (?1,?2,?3,'planned',?4,?5,'[]',?6,?6)",
            params![
                id,
                project_id,
                input.title.trim(),
                input.target_window.trim(),
                input.payoff_contract.trim(),
                ts
            ],
        )?;
        id
    };
    crate::graphrag::mark_stale(conn, project_id)?;
    get(conn, project_id, &id)
}

pub fn transition(
    conn: &Connection,
    project_id: &str,
    input: &PromiseStateInput,
) -> AppResult<PromiseRecord> {
    let promise = get(conn, project_id, &input.id)?;
    let allowed = matches!(
        (promise.state.as_str(), input.state.as_str()),
        ("planned", "planted")
            | ("planted", "reinforced")
            | ("planted", "ready")
            | ("reinforced", "ready")
            | ("ready", "paid_off")
            | ("planned", "abandoned")
            | ("planted", "abandoned")
            | ("reinforced", "abandoned")
            | ("ready", "abandoned")
    );
    if !allowed && promise.state != input.state {
        return Err(AppError::Message(format!(
            "不能从 {} 直接转为 {}",
            promise.state, input.state
        )));
    }
    if input.state == "paid_off" {
        return Err(AppError::Message(
            "已兑现必须通过 promise_payoff，提交原埋设证据、新证据定位与读者认知/故事状态变化".into(),
        ));
    }
    let mut evidence: Vec<String> = promise.evidence_ids.clone();
    let mut plant_evidence = promise.plant_evidence_ids.clone();
    if !input.evidence.trim().is_empty() {
        evidence.push(input.evidence.trim().to_string());
        if input.state == "planted" {
            plant_evidence.push(input.evidence.trim().to_string());
        }
    }
    evidence.sort();
    evidence.dedup();
    plant_evidence.sort();
    plant_evidence.dedup();
    conn.execute(
        "UPDATE promise SET state=?3, evidence_ids=?4, plant_evidence_ids=?5, updated_at=?6
         WHERE project_id=?1 AND id=?2",
        params![
            project_id,
            input.id,
            input.state,
            serde_json::to_string(&evidence)?,
            serde_json::to_string(&plant_evidence)?,
            now()
        ],
    )?;
    crate::graphrag::mark_stale(conn, project_id)?;
    get(conn, project_id, &input.id)
}

fn payoff_blockers(promise: &PromiseRecord, input: &PromisePayoffInput) -> Vec<String> {
    let mut blockers = Vec::new();
    if promise.state != "ready" {
        blockers.push("伏笔尚未进入 ready 状态".into());
    }
    if promise.payoff_contract.trim().is_empty() {
        blockers.push("缺少兑现契约：怎样才算真正兑现".into());
    }
    if promise.plant_evidence_ids.is_empty() {
        blockers.push("缺少原埋设证据".into());
    }
    let source = input.source_id.trim();
    if source.is_empty() {
        blockers.push("缺少新证据来源 ID".into());
    } else if !(source.contains(':')
        || source.starts_with("ch")
        || source.starts_with("scene-")
        || source.starts_with("revision-"))
    {
        blockers.push("新证据来源 ID 不可定位，请使用章节、场景、revision 或导入来源".into());
    }
    if input.quote.trim().chars().filter(|ch| !ch.is_whitespace()).count() < 8 {
        blockers.push("新证据需要至少 8 个字的可回查原文片段，不能只写关键词".into());
    }
    if input.state_change.trim().chars().filter(|ch| !ch.is_whitespace()).count() < 4 {
        blockers.push("需要写清读者认知、角色认知或故事状态发生了什么变化".into());
    }
    let canonical = format!("{}｜{}", source, input.quote.trim());
    if promise
        .payoff_evidence_ids
        .iter()
        .chain(promise.plant_evidence_ids.iter())
        .any(|evidence| evidence == &canonical)
    {
        blockers.push("新证据与已有证据相同；兑现必须使用新的可定位片段".into());
    }
    blockers
}

pub fn payoff_check(
    conn: &Connection,
    project_id: &str,
    input: &PromisePayoffInput,
) -> AppResult<PayoffCheck> {
    let promise = get(conn, project_id, &input.id)?;
    let blockers = payoff_blockers(&promise, input);
    Ok(PayoffCheck {
        ready: blockers.is_empty(),
        blockers,
        plant_evidence_ids: promise.plant_evidence_ids,
        payoff_evidence_ids: promise.payoff_evidence_ids,
        state_change: input.state_change.trim().to_string(),
    })
}

pub fn payoff(
    conn: &Connection,
    project_id: &str,
    input: &PromisePayoffInput,
) -> AppResult<PromiseRecord> {
    let promise = get(conn, project_id, &input.id)?;
    let blockers = payoff_blockers(&promise, input);
    if !blockers.is_empty() {
        return Err(AppError::Message(format!(
            "兑现证据不足：{}",
            blockers.join("；")
        )));
    }
    let canonical = format!("{}｜{}", input.source_id.trim(), input.quote.trim());
    let mut evidence = promise.evidence_ids.clone();
    let mut payoff_evidence = promise.payoff_evidence_ids.clone();
    evidence.push(canonical.clone());
    payoff_evidence.push(canonical);
    evidence.sort();
    evidence.dedup();
    payoff_evidence.sort();
    payoff_evidence.dedup();
    conn.execute(
        "UPDATE promise SET state='paid_off', evidence_ids=?3, payoff_evidence_ids=?4,
            payoff_state_change=?5, updated_at=?6 WHERE project_id=?1 AND id=?2",
        params![
            project_id,
            input.id,
            serde_json::to_string(&evidence)?,
            serde_json::to_string(&payoff_evidence)?,
            input.state_change.trim(),
            now()
        ],
    )?;
    crate::graphrag::mark_stale(conn, project_id)?;
    get(conn, project_id, &input.id)
}

#[allow(dead_code)]
pub fn due(conn: &Connection, project_id: &str, current_chapter: i32) -> AppResult<Vec<PromiseRecord>> {
    let all = list(conn, project_id)?;
    Ok(all
        .into_iter()
        .filter(|promise| {
            if !matches!(promise.state.as_str(), "planted" | "reinforced" | "ready") {
                return false;
            }
            let window = promise.target_window.trim();
            if window.is_empty() {
                return false;
            }
            let digits: String = window.chars().filter(|c| c.is_ascii_digit()).collect();
            digits
                .parse::<i32>()
                .map(|target| current_chapter >= target)
                .unwrap_or(false)
        })
        .collect())
}

#[allow(dead_code)]
pub fn evidence_list(conn: &Connection, project_id: &str) -> AppResult<Value> {
    let mut stmt = conn.prepare("SELECT id, title, state, target_window FROM promise WHERE project_id=?1")?;
    let rows = stmt
        .query_map([project_id], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "title": row.get::<_, String>(1)?,
                "state": row.get::<_, String>(2)?,
                "targetWindow": row.get::<_, String>(3)?,
            }))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Value::Array(rows))
}

#[cfg(test)]
mod tests {
    use crate::project;

    use super::*;

    #[test]
    fn payoff_requires_contract_and_evidence() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = project::create(&crate::domain::CreateProjectInput {
            name: "t".into(),
            root_path: dir.path().join("p").to_string_lossy().into_owned(),
            mode: "idea".into(),
            writing_language: None,
        })
        .unwrap();
        let pid = loaded.summary.id.clone();
        let promise = save(
            &loaded.conn,
            &pid,
            &PromiseInput {
                id: None,
                title: "旧钥匙".into(),
                target_window: "第3章".into(),
                payoff_contract: String::new(),
            },
        )
        .unwrap();
        assert!(transition(
            &loaded.conn,
            &pid,
            &PromiseStateInput {
                id: promise.id.clone(),
                state: "paid_off".into(),
                evidence: "ch03".into()
            }
        )
        .is_err());
        let promise = save(
            &loaded.conn,
            &pid,
            &PromiseInput {
                id: Some(promise.id),
                title: "旧钥匙".into(),
                target_window: "第3章".into(),
                payoff_contract: "揭示钥匙属于父亲".into(),
            },
        )
        .unwrap();
        transition(
            &loaded.conn,
            &pid,
            &PromiseStateInput {
                id: promise.id.clone(),
                state: "planted".into(),
                evidence: "ch01:p1｜林澈在父亲外套里摸到旧钥匙".into(),
            },
        )
        .unwrap();
        transition(
            &loaded.conn,
            &pid,
            &PromiseStateInput {
                id: promise.id.clone(),
                state: "ready".into(),
                evidence: "ch02".into(),
            },
        )
        .unwrap();
        let rejected = payoff(
            &loaded.conn,
            &pid,
            &PromisePayoffInput {
                id: promise.id.clone(),
                source_id: "chapter:ch03".into(),
                quote: "钥匙".into(),
                state_change: "林澈知道钥匙属于父亲".into(),
            },
        )
        .unwrap_err();
        assert!(rejected.to_string().contains("8"));
        let paid = payoff(
            &loaded.conn,
            &pid,
            &PromisePayoffInput {
                id: promise.id,
                source_id: "chapter:ch03".into(),
                quote: "林澈把旧钥匙插进抽屉，认出钥匙上刻着父亲的名字。".into(),
                state_change: "林澈改变判断，承认父亲确实留下过钥匙。".into(),
            },
        )
        .unwrap();
        assert_eq!(paid.state, "paid_off");
        assert!(paid.evidence_ids.len() >= 3);
    }
}
