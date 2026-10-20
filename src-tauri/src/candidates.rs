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

pub fn classify(
    conn: &Connection,
    project_id: &str,
    kind: &str,
    payload: &Value,
    source_span: Option<&str>,
) -> AppResult<&'static str> {
    let base = risk_for(kind, payload, source_span.is_some());
    if base != "L1" {
        return Ok(base);
    }
    let quote_present = source_span
        .map(|span| span.contains('｜') && !span.ends_with('｜'))
        .unwrap_or(false);
    if !quote_present {
        return Ok("L1");
    }
    match kind {
        "style_sample" | "rule" | "scene_goal" => Ok("L0"),
        "claim" => {
            let subject = payload
                .get("subject")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let truth = payload
                .get("truthLayer")
                .and_then(Value::as_str)
                .unwrap_or("world");
            if truth != "world" {
                return Ok("L2");
            }
            if find_entity(conn, project_id, subject)?.is_some() {
                Ok("L0")
            } else {
                Ok("L1")
            }
        }
        _ => Ok("L1"),
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
    let risk = classify(conn, project_id, kind, &payload, source_span)?;
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
    let risk = classify(
        conn,
        project_id,
        &candidate.kind,
        &payload,
        candidate.source_span.as_deref(),
    )?;
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
    let branch_id = crate::branches::current(conn, project_id)?;
    let tx = conn.savepoint()?;
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
                    version, created_at, updated_at, branch_id)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,'confirmed',1,?14,?14,?15)",
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
                    ts,
                    branch_id
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
                    reveal_time, cause, outcome, source_span, status, created_at, branch_id)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'confirmed',?11,?12)",
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
                    ts,
                    branch_id
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
    crate::graphrag::mark_stale(&tx, project_id)?;
    tx.commit()?;
    let updated = get(conn, project_id, id)?;
    Ok(CandidateAcceptResult {
        candidate: updated,
        created_id,
    })
}

#[derive(Debug, Clone)]
pub struct BatchOutcome {
    pub batch_id: String,
    pub revision_id: String,
    pub accepted: i32,
    pub sampled: i32,
}

fn table_for_kind(kind: &str) -> Option<&'static str> {
    match kind {
        "entity" => Some("entity"),
        "claim" => Some("claim"),
        "event" => Some("event"),
        "relation" => Some("relation"),
        "promise" => Some("promise"),
        "rule" => Some("rule"),
        "style_sample" => Some("style_sample"),
        "scene_goal" => Some("outline_node"),
        _ => None,
    }
}

fn sampled(candidate_id: &str, sample_rate: i32) -> bool {
    if sample_rate <= 0 {
        return false;
    }
    let hash = crate::hash_text(candidate_id);
    (u8::from_str_radix(&hash[..2], 16).unwrap_or(0) as u16)
        < (sample_rate.clamp(0, 100) as u16 * 255 / 100)
}

/// 单事务批量接受。allowed_levels 只允许 L0/L1；L2/L3 永远不会进入该路径。
#[allow(clippy::too_many_arguments)]
pub fn batch_accept_selective(
    conn: &mut Connection,
    project_id: &str,
    ids: &[String],
    chapter_id: Option<&str>,
    allowed_levels: &[&str],
    policy_id: Option<&str>,
    policy_mode: &str,
    sample_rate: i32,
) -> AppResult<BatchOutcome> {
    if allowed_levels
        .iter()
        .any(|level| !matches!(*level, "L0" | "L1"))
    {
        return Err(AppError::Message("自动/批量路径禁止接受 L2/L3".into()));
    }
    let pending = list(conn, project_id, Some("pending"), None)?;
    let eligible: Vec<Candidate> = pending
        .into_iter()
        .filter(|candidate| {
            allowed_levels.contains(&candidate.risk_level.as_str())
                && ids.contains(&candidate.id)
        })
        .collect();
    if eligible.is_empty() {
        return Err(AppError::Message(
            "没有符合当前策略的低风险候选；L2/L3 必须逐条处理".into(),
        ));
    }
    let batch_id = uuid::Uuid::new_v4().to_string();
    let ts = now();
    conn.execute(
        "INSERT INTO review_batch (id, project_id, chapter_id, policy_id, item_count,
            accepted_count, rejected_count, created_at, policy_mode, sample_rate, sampled_count)
         VALUES (?1,?2,?3,?4,?5,0,0,?6,?7,?8,0)",
        params![
            batch_id,
            project_id,
            chapter_id,
            policy_id,
            eligible.len() as i32,
            ts,
            policy_mode,
            sample_rate.clamp(0, 100)
        ],
    )?;
    let mut operations: Vec<Value> = Vec::new();
    let mut sample_count = 0;
    for candidate in &eligible {
        let result = match accept(conn, project_id, &candidate.id) {
            Ok(result) => result,
            Err(error) => {
                for operation in &operations {
                    if let (Some(table), Some(created_id)) = (
                        operation.get("table").and_then(Value::as_str),
                        operation.get("createdId").and_then(Value::as_str),
                    ) {
                        conn.execute(&format!("DELETE FROM {table} WHERE id=?1"), [created_id])
                            .ok();
                    }
                    if let Some(candidate_id) = operation.get("candidateId").and_then(Value::as_str)
                    {
                        conn.execute(
                            "UPDATE candidate SET status='pending', confirmed_at=NULL WHERE project_id=?1 AND id=?2",
                            params![project_id, candidate_id],
                        )
                        .ok();
                    }
                }
                conn.execute("DELETE FROM review_batch WHERE id=?1", [&batch_id])
                    .ok();
                return Err(error);
            }
        };
        operations.push(json!({
            "candidateId": candidate.id,
            "createdId": result.created_id,
            "kind": candidate.kind,
            "table": table_for_kind(&candidate.kind)
        }));
        let is_sampled = sampled(&candidate.id, sample_rate);
        if is_sampled {
            sample_count += 1;
        }
        conn.execute(
            "INSERT INTO review_sample (id, project_id, batch_id, candidate_id, sampled,
                review_status, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                uuid::Uuid::new_v4().to_string(),
                project_id,
                batch_id,
                candidate.id,
                is_sampled as i32,
                if is_sampled { "pending" } else { "not_sampled" },
                ts
            ],
        )?;
    }
    let snapshot_id = uuid::Uuid::new_v4().to_string();
    let revision_id = uuid::Uuid::new_v4().to_string();
    let diff = json!({
        "action": "batch_accept",
        "batchId": batch_id,
        "policyMode": policy_mode,
        "operations": operations
    });
    conn.execute(
        "INSERT INTO snapshot (id, project_id, chapter_id, content, metadata, created_at)
         VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            snapshot_id,
            project_id,
            chapter_id,
            diff.to_string(),
            json!({"kind": "review_batch", "batchId": batch_id}).to_string(),
            ts
        ],
    )?;
    let parent: Option<String> = chapter_id
        .and_then(|id| {
            conn.query_row(
                "SELECT current_revision_id FROM chapter WHERE project_id=?1 AND id=?2",
                params![project_id, id],
                |row| row.get::<_, Option<String>>(0),
            )
            .ok()
            .flatten()
        });
    conn.execute(
        "INSERT INTO revision (id, project_id, parent_id, chapter_id, diff, snapshot_id,
            author_action, created_at) VALUES (?1,?2,?3,?4,?5,?6,'batch_accept',?7)",
        params![
            revision_id,
            project_id,
            parent,
            chapter_id,
            diff.to_string(),
            snapshot_id,
            ts
        ],
    )?;
    conn.execute(
        "UPDATE review_batch SET accepted_count=?2, revision_id=?3, snapshot_id=?4,
            sampled_count=?5 WHERE id=?1",
        params![
            batch_id,
            eligible.len() as i32,
            revision_id,
            snapshot_id,
            sample_count
        ],
    )?;
    conn.execute(
        "INSERT INTO audit_log (id, project_id, action, subject, policy_id, batch_id, detail, created_at)
         VALUES (?1,?2,'batch_accept',?3,?4,?5,?6,?7)",
        params![
            uuid::Uuid::new_v4().to_string(),
            project_id,
            revision_id,
            policy_id,
            batch_id,
            json!({
                "accepted": eligible.len(),
                "sampled": sample_count,
                "mode": policy_mode
            })
            .to_string(),
            ts
        ],
    )?;
    Ok(BatchOutcome {
        batch_id,
        revision_id,
        accepted: eligible.len() as i32,
        sampled: sample_count,
    })
}

pub fn batch_accept_l1(
    conn: &mut Connection,
    project_id: &str,
    ids: &[String],
    chapter_id: Option<&str>,
) -> AppResult<(String, String, i32)> {
    let outcome = batch_accept_selective(
        conn,
        project_id,
        ids,
        chapter_id,
        &["L1"],
        None,
        "balanced",
        0,
    )?;
    Ok((outcome.batch_id, outcome.revision_id, outcome.accepted))
}

pub fn undo_batch(
    conn: &mut Connection,
    project_id: &str,
    batch_id: &str,
) -> AppResult<(String, i32)> {
    let (snapshot_id, undone_at): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT snapshot_id, undone_at FROM review_batch WHERE project_id=?1 AND id=?2",
            params![project_id, batch_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::Message("找不到审核批次".into()))?;
    if undone_at.is_some() {
        return Err(AppError::Message("该批次已经撤销".into()));
    }
    let snapshot_id =
        snapshot_id.ok_or_else(|| AppError::Message("批次缺少可撤销快照".into()))?;
    let content: String = conn.query_row(
        "SELECT content FROM snapshot WHERE project_id=?1 AND id=?2",
        params![project_id, snapshot_id],
        |row| row.get(0),
    )?;
    let diff: Value = serde_json::from_str(&content)?;
    let operations = diff
        .get("operations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if operations.is_empty() {
        return Err(AppError::Message("批次快照没有可回滚操作".into()));
    }
    let ts = now();
    let undo_revision = uuid::Uuid::new_v4().to_string();
    let undo_snapshot = uuid::Uuid::new_v4().to_string();
    let tx = conn.transaction()?;
    let mut restored = 0;
    for operation in &operations {
        let table = operation
            .get("table")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::Message("批次操作缺少表名".into()))?;
        if ![
            "entity",
            "claim",
            "event",
            "relation",
            "promise",
            "rule",
            "style_sample",
            "outline_node",
        ]
        .contains(&table)
        {
            return Err(AppError::Message("批次操作包含非法表名".into()));
        }
        if let (Some(created_id), Some(candidate_id)) = (
            operation.get("createdId").and_then(Value::as_str),
            operation.get("candidateId").and_then(Value::as_str),
        ) {
            tx.execute(&format!("DELETE FROM {table} WHERE id=?1"), [created_id])?;
            tx.execute(
                "UPDATE candidate SET status='pending', confirmed_at=NULL WHERE project_id=?1 AND id=?2",
                params![project_id, candidate_id],
            )?;
            restored += 1;
        }
    }
    let undo_diff = json!({
        "action": "batch_undo",
        "batchId": batch_id,
        "restored": restored,
        "operations": operations
    });
    tx.execute(
        "INSERT INTO snapshot (id, project_id, chapter_id, content, metadata, created_at)
         VALUES (?1,?2,NULL,?3,?4,?5)",
        params![
            undo_snapshot,
            project_id,
            undo_diff.to_string(),
            json!({"kind": "batch_undo", "batchId": batch_id}).to_string(),
            ts
        ],
    )?;
    let batch_revision: Option<String> = tx.query_row(
        "SELECT revision_id FROM review_batch WHERE id=?1",
        [batch_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "INSERT INTO revision (id, project_id, parent_id, chapter_id, diff, snapshot_id,
            author_action, created_at) VALUES (?1,?2,?3,NULL,?4,?5,'batch_undo',?6)",
        params![
            undo_revision,
            project_id,
            batch_revision,
            undo_diff.to_string(),
            undo_snapshot,
            ts
        ],
    )?;
    tx.execute(
        "UPDATE review_batch SET undone_at=?2 WHERE project_id=?1 AND id=?3",
        params![project_id, ts, batch_id],
    )?;
    tx.execute(
        "UPDATE review_sample SET review_status='invalidated' WHERE batch_id=?1",
        [batch_id],
    )?;
    tx.execute(
        "INSERT INTO audit_log (id, project_id, action, subject, batch_id, detail, created_at)
         VALUES (?1,?2,'batch_undo',?3,?4,?5,?6)",
        params![
            uuid::Uuid::new_v4().to_string(),
            project_id,
            undo_revision,
            batch_id,
            json!({"restored": restored}).to_string(),
            ts
        ],
    )?;
    crate::graphrag::mark_stale(&tx, project_id)?;
    tx.commit()?;
    Ok((undo_revision, restored))
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

    #[test]
    fn batch_accept_creates_one_revision_and_skips_l2() {
        let (_dir, mut loaded) = setup();
        let pid = loaded.summary.id.clone();
        let a = insert(
            &loaded.conn,
            &pid,
            "entity",
            json!({"type":"place","canonicalName":"旧车站"}),
            Some("ch01"),
            None,
            0.8,
        )
        .unwrap();
        let b = insert(
            &loaded.conn,
            &pid,
            "rule",
            json!({"text":"旧车站只在雨夜出现"}),
            Some("ch02"),
            None,
            0.8,
        )
        .unwrap();
        let risky = insert(
            &loaded.conn,
            &pid,
            "claim",
            json!({"subject":"父亲","predicate":"死亡","object":"","truthLayer":"world"}),
            Some("ch03"),
            None,
            0.8,
        )
        .unwrap();
        assert_eq!(risky.risk_level, "L2");
        let (_, revision, accepted) = batch_accept_l1(
            &mut loaded.conn,
            &pid,
            &[a.id.clone(), b.id.clone(), risky.id.clone()],
            None,
        )
        .unwrap();
        assert_eq!(accepted, 2);
        let revisions: i64 = loaded
            .conn
            .query_row(
                "SELECT COUNT(*) FROM revision WHERE author_action='batch_accept'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(revisions, 1);
        let risky_status: String = loaded
            .conn
            .query_row("SELECT status FROM candidate WHERE id=?1", [&risky.id], |row| row.get(0))
            .unwrap();
        assert_eq!(risky_status, "pending");
        let batch_revision: String = loaded
            .conn
            .query_row(
                "SELECT revision_id FROM review_batch ORDER BY created_at DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(batch_revision, revision);
    }

    #[test]
    fn batch_undo_restores_created_records_and_candidates() {
        let (_dir, mut loaded) = setup();
        let pid = loaded.summary.id.clone();
        let a = insert(
            &loaded.conn,
            &pid,
            "entity",
            json!({"type":"place","canonicalName":"旧车站"}),
            Some("ch01"),
            None,
            0.8,
        )
        .unwrap();
        let b = insert(
            &loaded.conn,
            &pid,
            "rule",
            json!({"text":"旧车站只在雨夜出现"}),
            Some("ch02"),
            None,
            0.8,
        )
        .unwrap();
        let (batch_id, _, accepted) =
            batch_accept_l1(&mut loaded.conn, &pid, &[a.id.clone(), b.id.clone()], None).unwrap();
        assert_eq!(accepted, 2);
        let (_, restored) = undo_batch(&mut loaded.conn, &pid, &batch_id).unwrap();
        assert_eq!(restored, 2);
        let entities: i64 = loaded
            .conn
            .query_row("SELECT COUNT(*) FROM entity", [], |row| row.get(0))
            .unwrap();
        assert_eq!(entities, 0);
        let pending = list(&loaded.conn, &pid, Some("pending"), None).unwrap();
        assert_eq!(pending.len(), 2);
        assert!(undo_batch(&mut loaded.conn, &pid, &batch_id).is_err());
    }

    #[test]
    fn batch_accepts_one_hundred_items_with_single_revision() {
        let (_dir, mut loaded) = setup();
        let pid = loaded.summary.id.clone();
        let mut ids = Vec::new();
        for index in 0..100 {
            let candidate = insert(
                &loaded.conn,
                &pid,
                "entity",
                json!({"type":"concept","canonicalName":format!("概念{index}")}),
                Some("ch01"),
                None,
                0.8,
            )
            .unwrap();
            ids.push(candidate.id);
        }
        let (_, revision, accepted) =
            batch_accept_l1(&mut loaded.conn, &pid, &ids, None).unwrap();
        assert_eq!(accepted, 100);
        let revisions: i64 = loaded
            .conn
            .query_row(
                "SELECT COUNT(*) FROM revision WHERE author_action='batch_accept'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(revisions, 1);
        let candidates: i64 = loaded
            .conn
            .query_row("SELECT COUNT(*) FROM entity", [], |row| row.get(0))
            .unwrap();
        assert_eq!(candidates, 100);
        assert!(!revision.is_empty());
    }
}
