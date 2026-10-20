use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::{json, Value};

use crate::{
    domain::{Candidate, CandidateAcceptResult, CandidateCounts, ReviewQueue},
    error::{AppError, AppResult},
    now,
};

fn candidate_from_row(row: &Row<'_>) -> rusqlite::Result<Candidate> {
    let payload: String = row.get(2)?;
    Ok(Candidate {
        id: row.get(0)?,
        kind: row.get(1)?,
        payload: serde_json::from_str(&payload).unwrap_or(json!({})),
        source_span: row.get(3)?,
        source_message_id: row.get(4)?,
        confidence: row.get(5)?,
        risk_level: row.get(6)?,
        status: row.get(7)?,
        batch_id: row.get(8)?,
        created_at: row.get(9)?,
        confirmed_at: row.get(10)?,
    })
}

const SELECT: &str = "SELECT id, kind, payload, source_span, source_message_id, confidence,
    risk_level, status, batch_id, created_at, confirmed_at FROM candidate";

pub fn risk_for(kind: &str, payload: &Value, has_source: bool) -> &'static str {
    if !has_source {
        return "L2";
    }
    match kind {
        "relation" | "promise" | "event" | "claim" | "premise" => {
            if kind == "claim" {
                let predicate = payload
                    .get("predicate")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let high_impact = [
                    "死", "亡", "持有", "位置", "地点", "时间", "关系", "血缘", "身份", "知道",
                    "知情", "destroyed", "dead", "holds", "located",
                ]
                .iter()
                .any(|needle| predicate.contains(needle));
                if high_impact {
                    "L2"
                } else {
                    "L1"
                }
            } else {
                "L2"
            }
        }
        "entity" | "rule" | "style_sample" | "scene_goal" => "L1",
        _ => "L2",
    }
}

pub fn insert(
    conn: &Connection,
    project_id: &str,
    kind: &str,
    payload: Value,
    source_span: Option<&str>,
    source_message_id: Option<&str>,
    confidence: f64,
) -> AppResult<Candidate> {
    let risk = risk_for(kind, &payload, source_span.is_some());
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO candidate (id, project_id, kind, payload, source_span, source_message_id,
            confidence, risk_level, status, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'pending',?9)",
        params![
            id,
            project_id,
            kind,
            serde_json::to_string(&payload)?,
            source_span,
            source_message_id,
            confidence.clamp(0.0, 1.0),
            risk,
            now()
        ],
    )?;
    get(conn, project_id, &id)
}

pub fn get(conn: &Connection, project_id: &str, id: &str) -> AppResult<Candidate> {
    conn.query_row(
        &format!("{SELECT} WHERE project_id=?1 AND id=?2"),
        params![project_id, id],
        candidate_from_row,
    )
    .optional()?
    .ok_or_else(|| AppError::Message("找不到候选条目".into()))
}

pub fn list(
    conn: &Connection,
    project_id: &str,
    status: Option<&str>,
    kind: Option<&str>,
) -> AppResult<Vec<Candidate>> {
    let mut sql = format!("{SELECT} WHERE project_id=?1");
    if status.is_some() {
        sql.push_str(" AND status=?2");
    }
    if kind.is_some() {
        sql.push_str(if status.is_some() {
            " AND kind=?3"
        } else {
            " AND kind=?2"
        });
    }
    sql.push_str(" ORDER BY CASE risk_level WHEN 'L3' THEN 0 WHEN 'L2' THEN 1 WHEN 'L1' THEN 2 ELSE 3 END, created_at DESC LIMIT 500");
    let mut stmt = conn.prepare(&sql)?;
    let rows = match (status, kind) {
        (Some(s), Some(k)) => stmt
            .query_map(params![project_id, s, k], candidate_from_row)?
            .collect::<Result<Vec<_>, _>>()?,
        (Some(s), None) => stmt
            .query_map(params![project_id, s], candidate_from_row)?
            .collect::<Result<Vec<_>, _>>()?,
        (None, Some(k)) => stmt
            .query_map(params![project_id, k], candidate_from_row)?
            .collect::<Result<Vec<_>, _>>()?,
        (None, None) => stmt
            .query_map([project_id], candidate_from_row)?
            .collect::<Result<Vec<_>, _>>()?,
    };
    drop(stmt);
    Ok(rows)
}

pub fn queue(conn: &Connection, project_id: &str) -> AppResult<ReviewQueue> {
    let candidates = list(conn, project_id, Some("pending"), None)?;
    let mut counts = CandidateCounts::default();
    for candidate in &candidates {
        counts.total += 1;
        match candidate.risk_level.as_str() {
            "L0" => counts.l0 += 1,
            "L1" => counts.l1 += 1,
            "L2" => counts.l2 += 1,
            _ => counts.l3 += 1,
        }
    }
    Ok(ReviewQueue { candidates, counts })
}

pub fn reject(conn: &Connection, project_id: &str, id: &str) -> AppResult<Candidate> {
    conn.execute(
        "UPDATE candidate SET status='rejected' WHERE project_id=?1 AND id=?2 AND status='pending'",
        params![project_id, id],
    )?;
    get(conn, project_id, id)
}

pub fn update_payload(
    conn: &Connection,
    project_id: &str,
    id: &str,
    payload: Value,
) -> AppResult<Candidate> {
    let candidate = get(conn, project_id, id)?;
    if candidate.status != "pending" {
        return Err(AppError::Message("只能编辑待确认候选".into()));
    }
    let risk = risk_for(&candidate.kind, &payload, candidate.source_span.is_some());
    conn.execute(
        "UPDATE candidate SET payload=?3, risk_level=?4 WHERE project_id=?1 AND id=?2",
        params![project_id, id, serde_json::to_string(&payload)?, risk],
    )?;
    get(conn, project_id, id)
}

fn text_of(payload: &Value) -> String {
    payload
        .get("text")
        .and_then(Value::as_str)
        .or_else(|| payload.get("description").and_then(Value::as_str))
        .or_else(|| payload.get("object").and_then(Value::as_str))
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn find_entity(conn: &Connection, project_id: &str, name: &str) -> AppResult<Option<String>> {
    if name.trim().is_empty() {
        return Ok(None);
    }
    Ok(conn
        .query_row(
            "SELECT id FROM entity WHERE project_id=?1 AND (canonical_name=?2 OR aliases LIKE ?3) LIMIT 1",
            params![project_id, name, format!("%\"{}\"%", name)],
            |row| row.get(0),
        )
        .optional()?)
}

/// 作者确认候选后，才写入正式表。所有分支在同一事务中完成。
pub fn accept(conn: &mut Connection, project_id: &str, id: &str) -> AppResult<CandidateAcceptResult> {
    let candidate = get(conn, project_id, id)?;
    if candidate.status != "pending" {
        return Err(AppError::Message("该候选已经处理".into()));
    }
    if candidate.risk_level == "L3" {
        return Err(AppError::Message("L3 硬冲突必须逐条处理后才能写入".into()));
    }
    let payload = candidate.payload.clone();
    let ts = now();
    let tx = conn.transaction()?;
    let created_id = match candidate.kind.as_str() {
        "entity" => {
            let id = uuid::Uuid::new_v4().to_string();
            let entity_type = payload
                .get("type")
                .or_else(|| payload.get("entityType"))
                .and_then(Value::as_str)
                .unwrap_or("concept");
            let name = payload
                .get("canonicalName")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| text_of(&payload).chars().take(30).collect());
            if name.trim().is_empty() {
                return Err(AppError::Message("实体缺少名称".into()));
            }
            let aliases = payload
                .get("aliases")
                .cloned()
                .unwrap_or_else(|| json!([]));
            let description = payload
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default();
            tx.execute(
                "INSERT INTO entity (id, project_id, type, canonical_name, aliases, status, description, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,'confirmed',?6,?7,?7)",
                params![
                    id,
                    project_id,
                    entity_type,
                    name,
                    aliases.to_string(),
                    description,
                    ts
                ],
            )?;
            if entity_type == "character" {
                tx.execute(
                    "INSERT INTO character_core (entity_id, updated_at) VALUES (?1, ?2)",
                    params![id, ts],
                )?;
            }
            id
        }
        "claim" => {
            let id = uuid::Uuid::new_v4().to_string();
            let subject = payload
                .get("subject")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            let predicate = payload
                .get("predicate")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            if subject.is_empty() || predicate.is_empty() {
                return Err(AppError::Message("事实候选缺少主体或谓词".into()));
            }
            let object = payload
                .get("object")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let truth_layer = payload
                .get("truthLayer")
                .and_then(Value::as_str)
                .unwrap_or("world");
            if !["world", "belief", "utterance", "narrative", "planned"].contains(&truth_layer) {
                return Err(AppError::Message(format!("未知真值层：{truth_layer}")));
            }
            let perspective = payload.get("perspective").and_then(Value::as_str);
            let subject_id = find_entity(&tx, project_id, subject)?;
            let object_id = find_entity(&tx, project_id, object)?;
            let quote = payload.get("quote").and_then(Value::as_str).unwrap_or_default();
            let span = match (&candidate.source_span, quote.is_empty()) {
                (Some(span), false) => format!("{span}｜{quote}"),
                (Some(span), true) => span.clone(),
                (None, false) => quote.to_string(),
                (None, true) => String::new(),
            };
            tx.execute(
                "INSERT INTO claim (id, project_id, subject, subject_id, predicate, object, object_entity_id,
                    truth_layer, perspective_id, valid_from, valid_to, source_span, confidence, status,
                    version, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,'confirmed',1,?14,?14)",
                params![
                    id,
                    project_id,
                    subject,
                    subject_id,
                    predicate,
                    object,
                    object_id,
                    truth_layer,
                    perspective,
                    payload.get("validFrom").and_then(Value::as_str),
                    payload.get("validTo").and_then(Value::as_str),
                    span,
                    candidate.confidence,
                    ts
                ],
            )?;
            id
        }
        "event" => {
            let id = uuid::Uuid::new_v4().to_string();
            let title = payload
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if title.trim().is_empty() {
                return Err(AppError::Message("事件候选缺少标题".into()));
            }
            tx.execute(
                "INSERT INTO event (id, project_id, title, participants, place_id, story_time,
                    reveal_time, cause, outcome, source_span, status, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'confirmed',?11)",
                params![
                    id,
                    project_id,
                    title,
                    payload.get("participants").cloned().unwrap_or(json!([])).to_string(),
                    payload.get("placeId").and_then(Value::as_str),
                    payload.get("storyTime").and_then(Value::as_str).unwrap_or_default(),
                    payload.get("revealTime").and_then(Value::as_str),
                    payload.get("cause").and_then(Value::as_str).unwrap_or_default(),
                    payload.get("outcome").and_then(Value::as_str).unwrap_or_default(),
                    candidate.source_span.clone().unwrap_or_default(),
                    ts
                ],
            )?;
            id
        }
        "relation" => {
            let id = uuid::Uuid::new_v4().to_string();
            let from_name = payload.get("from").and_then(Value::as_str).unwrap_or_default();
            let to_name = payload.get("to").and_then(Value::as_str).unwrap_or_default();
            let from_id = find_entity(&tx, project_id, from_name)?
                .ok_or_else(|| AppError::Message(format!("关系候选找不到实体：{from_name}")))?;
            let to_id = find_entity(&tx, project_id, to_name)?
                .ok_or_else(|| AppError::Message(format!("关系候选找不到实体：{to_name}")))?;
            let relation_type = payload.get("type").and_then(Value::as_str).unwrap_or_default();
            if relation_type.trim().is_empty() {
                return Err(AppError::Message("关系候选缺少类型".into()));
            }
            tx.execute(
                "INSERT INTO relation (id, project_id, from_id, to_id, type, direction, evidence_id, status, created_at)
                 VALUES (?1,?2,?3,?4,?5,'directed',?6,'confirmed',?7)",
                params![
                    id,
                    project_id,
                    from_id,
                    to_id,
                    relation_type,
                    candidate.source_span,
                    ts
                ],
            )?;
            id
        }
        "promise" => {
            let id = uuid::Uuid::new_v4().to_string();
            let title = payload
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| text_of(&payload).chars().take(30).collect());
            let contract = payload
                .get("contract")
                .or_else(|| payload.get("payoffContract"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            if title.trim().is_empty() {
                return Err(AppError::Message("伏笔候选缺少标题".into()));
            }
            tx.execute(
                "INSERT INTO promise (id, project_id, title, state, target_window, payoff_contract,
                    evidence_ids, created_at, updated_at) VALUES (?1,?2,?3,'planned',?4,?5,?6,?7,?7)",
                params![
                    id,
                    project_id,
                    title,
                    payload.get("window").or_else(|| payload.get("targetWindow")).and_then(Value::as_str).unwrap_or_default(),
                    contract,
                    json!([candidate.source_span]).to_string(),
                    ts
                ],
            )?;
            id
        }
        "rule" => {
            let id = uuid::Uuid::new_v4().to_string();
            let description = text_of(&payload);
            if description.is_empty() {
                return Err(AppError::Message("规则候选缺少内容".into()));
            }
            tx.execute(
                "INSERT INTO rule (id, project_id, code, constraint_layer, severity, predicate,
                    exception_scope, description, enabled, created_at)
                 VALUES (?1,?2,?3,'soft','warn','{}','{}',?4,1,?5)",
                params![
                    id,
                    project_id,
                    format!("rule-{}", &id[..8]),
                    description,
                    ts
                ],
            )?;
            id
        }
        "style_sample" => {
            let id = uuid::Uuid::new_v4().to_string();
            let text = text_of(&payload);
            if text.is_empty() {
                return Err(AppError::Message("文笔样本为空".into()));
            }
            tx.execute(
                "INSERT INTO style_sample (id, project_id, profile_id, text, source_span, source_type,
                    approved, tags, created_at) VALUES (?1,?2,?3,?4,?5,'chat',1,'[]',?6)",
                params![
                    id,
                    project_id,
                    payload.get("profileId").and_then(Value::as_str),
                    text,
                    candidate.source_span,
                    ts
                ],
            )?;
            id
        }
        "scene_goal" => {
            let id = uuid::Uuid::new_v4().to_string();
            let text = text_of(&payload);
            if text.is_empty() {
                return Err(AppError::Message("章节目标为空".into()));
            }
            let next: i32 = tx.query_row(
                "SELECT COALESCE(MAX(order_no), 0) + 1 FROM outline_node WHERE project_id=?1",
                [project_id],
                |row| row.get(0),
            )?;
            tx.execute(
                "INSERT INTO outline_node (id, project_id, level, order_no, title, goal, status, source, created_at, updated_at)
                 VALUES (?1,?2,'chapter',?3,?4,?5,'candidate','chat',?6,?6)",
                params![
                    id,
                    project_id,
                    next,
                    text.chars().take(24).collect::<String>(),
                    text,
                    ts
                ],
            )?;
            id
        }
        other => {
            return Err(AppError::Message(format!(
                "暂不支持确认该类型候选：{other}"
            )))
        }
    };

    tx.execute(
        "UPDATE candidate SET status='accepted', confirmed_at=?3 WHERE project_id=?1 AND id=?2",
        params![project_id, id, ts],
    )?;
    tx.execute(
        "INSERT INTO audit_log (id, project_id, action, subject, detail, created_at)
         VALUES (?1,?2,'candidate_accept',?3,?4,?5)",
        params![
            uuid::Uuid::new_v4().to_string(),
            project_id,
            id,
            json!({"kind": candidate.kind, "risk": candidate.risk_level, "createdId": created_id}).to_string(),
            ts
        ],
    )?;
    tx.commit()?;
    let updated = get(conn, project_id, id)?;
    Ok(CandidateAcceptResult {
        candidate: updated,
        created_id,
    })
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
            mode: "import".into(),
            writing_language: None,
        })
        .unwrap();
        (dir, loaded)
    }

    #[test]
    fn accepts_entity_and_claim_only_after_confirmation() {
        let (_dir, mut loaded) = setup();
        let pid = loaded.summary.id.clone();
        let entity = insert(
            &loaded.conn,
            &pid,
            "entity",
            json!({"type":"character","canonicalName":"林澈","aliases":["阿澈"],"description":"主角"}),
            Some("ch01:p1"),
            None,
            0.9,
        )
        .unwrap();
        assert_eq!(entity.status, "pending");
        accept(&mut loaded.conn, &pid, &entity.id).unwrap();
        let count: i64 = loaded
            .conn
            .query_row("SELECT COUNT(*) FROM entity WHERE canonical_name='林澈'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        let claim = insert(
            &loaded.conn,
            &pid,
            "claim",
            json!({"subject":"林澈","predicate":"持有一把旧钥匙","object":"旧钥匙","truthLayer":"world","quote":"林澈握紧了旧钥匙。"}),
            Some("ch01:p2"),
            None,
            0.8,
        )
        .unwrap();
        accept(&mut loaded.conn, &pid, &claim.id).unwrap();
        let spans: String = loaded
            .conn
            .query_row("SELECT source_span FROM claim", [], |r| r.get(0))
            .unwrap();
        assert!(spans.contains("旧钥匙"));
    }

    #[test]
    fn missing_source_escalates_to_l2() {
        assert_eq!(risk_for("entity", &json!({}), false), "L2");
        assert_eq!(risk_for("entity", &json!({}), true), "L1");
    }
}
