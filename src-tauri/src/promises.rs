use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::Value;

use crate::{
    domain::{PromiseInput, PromiseRecord, PromiseStateInput},
    error::{AppError, AppResult},
    now,
};

fn from_row(row: &Row<'_>) -> rusqlite::Result<PromiseRecord> {
    let evidence: String = row.get(6)?;
    Ok(PromiseRecord {
        id: row.get(0)?,
        title: row.get(1)?,
        state: row.get(2)?,
        plant_scene_id: row.get(3)?,
        target_window: row.get(4)?,
        payoff_contract: row.get(5)?,
        evidence_ids: serde_json::from_str(&evidence).unwrap_or_default(),
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

const SELECT: &str = "SELECT id, title, state, plant_scene_id, target_window, payoff_contract,
    evidence_ids, created_at, updated_at FROM promise";

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
        if promise.payoff_contract.trim().is_empty() {
            return Err(AppError::Message(
                "兑现前必须写清兑现契约：怎样才算真正兑现".into(),
            ));
        }
        if input.evidence.trim().is_empty() {
            return Err(AppError::Message(
                "标记已兑现至少需要一条新证据；只出现关键词不算兑现".into(),
            ));
        }
    }
    let mut evidence: Vec<String> = promise.evidence_ids.clone();
    if !input.evidence.trim().is_empty() {
        evidence.push(input.evidence.trim().to_string());
    }
    conn.execute(
        "UPDATE promise SET state=?3, evidence_ids=?4, updated_at=?5 WHERE project_id=?1 AND id=?2",
        params![
            project_id,
            input.id,
            input.state,
            serde_json::to_string(&evidence)?,
            now()
        ],
    )?;
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
                evidence: "ch01".into(),
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
        let paid = transition(
            &loaded.conn,
            &pid,
            &PromiseStateInput {
                id: promise.id,
                state: "paid_off".into(),
                evidence: "ch03".into(),
            },
        )
        .unwrap();
        assert_eq!(paid.state, "paid_off");
        assert!(paid.evidence_ids.len() >= 3);
    }
}
