use std::collections::HashMap;

use rusqlite::{params, Connection, Row};
use serde_json::json;

use crate::{
    domain::{
        AuditIssue, AuditRunResult, ConflictCertificate, RuleRecord,
    },
    error::{AppError, AppResult},
    now, solver,
};

#[derive(Debug, Clone)]
struct ClaimFact {
    id: String,
    subject: String,
    predicate: String,
    object: String,
    valid_from: String,
    source_span: String,
    truth_layer: String,
    perspective_id: Option<String>,
}

#[derive(Debug, Clone)]
struct EventFact {
    id: String,
    title: String,
    participants: Vec<String>,
    place_id: Option<String>,
    story_time: String,
    cause: String,
    outcome: String,
    source_span: String,
}

pub fn seed_defaults(conn: &Connection, project_id: &str) -> AppResult<()> {
    let rows = [
        (
            "dead_character_acts",
            "hard",
            "block",
            "character_lifecycle",
            "死亡角色在故事时间之后以现实行动出现",
        ),
        (
            "character_two_places",
            "hard",
            "block",
            "exclusive_location",
            "同一角色在同一故事时间出现在两个地点",
        ),
        (
            "item_destroyed_then_held",
            "hard",
            "block",
            "exclusive_item",
            "物品被销毁或交出后仍被持有",
        ),
        (
            "relation_direction_conflict",
            "hard",
            "block",
            "relation_direction",
            "关系方向或上下位关系互相冲突",
        ),
        (
            "pov_knowledge_leak",
            "hard",
            "warn",
            "knowledge_boundary",
            "POV 使用尚未获得、未目睹且未被告知的信息",
        ),
        (
            "promise_overdue",
            "soft",
            "warn",
            "promise_debt",
            "伏笔超过计划窗口仍未推进",
        ),
    ];
    for (code, layer, severity, rule_type, description) in rows {
        conn.execute(
            "INSERT OR IGNORE INTO rule (id, project_id, code, constraint_layer, severity,
                predicate, exception_scope, description, enabled, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,'{}',?7,1,?8)",
            params![
                uuid::Uuid::new_v4().to_string(),
                project_id,
                code,
                layer,
                severity,
                json!({"type": rule_type}).to_string(),
                description,
                now()
            ],
        )?;
    }
    Ok(())
}

fn rule_from_row(row: &Row<'_>) -> rusqlite::Result<RuleRecord> {
    let predicate: String = row.get(4)?;
    let scope: String = row.get(5)?;
    Ok(RuleRecord {
        id: row.get(0)?,
        code: row.get(1)?,
        constraint_layer: row.get(2)?,
        severity: row.get(3)?,
        predicate: serde_json::from_str(&predicate).unwrap_or(json!({})),
        exception_scope: serde_json::from_str(&scope).unwrap_or(json!({})),
        description: row.get(6)?,
        enabled: row.get(7)?,
    })
}

pub fn list(conn: &Connection, project_id: &str) -> AppResult<Vec<RuleRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, code, constraint_layer, severity, predicate, exception_scope, description, enabled
         FROM rule WHERE project_id=?1 ORDER BY constraint_layer, code",
    )?;
    let rows = stmt
        .query_map([project_id], rule_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    Ok(rows)
}

fn claims(conn: &Connection, project_id: &str) -> AppResult<Vec<ClaimFact>> {
    let mut stmt = conn.prepare(
        "SELECT id, subject, predicate, object, COALESCE(valid_from,''), source_span,
            truth_layer, perspective_id
         FROM claim WHERE project_id=?1 AND status='confirmed'
           AND (branch_id IS NULL OR branch_id=(SELECT current_branch_id FROM project WHERE id=?1))",
    )?;
    let rows = stmt
        .query_map([project_id], |row| {
            Ok(ClaimFact {
                id: row.get(0)?,
                subject: row.get(1)?,
                predicate: row.get(2)?,
                object: row.get(3)?,
                valid_from: row.get(4)?,
                source_span: row.get(5)?,
                truth_layer: row.get(6)?,
                perspective_id: row.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    Ok(rows)
}

fn events(conn: &Connection, project_id: &str) -> AppResult<Vec<EventFact>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, participants, place_id, story_time, cause, outcome, source_span
         FROM event WHERE project_id=?1 AND status='confirmed'
           AND (branch_id IS NULL OR branch_id=(SELECT current_branch_id FROM project WHERE id=?1))
         ORDER BY story_time",
    )?;
    let rows = stmt
        .query_map([project_id], |row| {
            let participants: String = row.get(2)?;
            Ok(EventFact {
                id: row.get(0)?,
                title: row.get(1)?,
                participants: serde_json::from_str(&participants).unwrap_or_default(),
                place_id: row.get(3)?,
                story_time: row.get(4)?,
                cause: row.get(5)?,
                outcome: row.get(6)?,
                source_span: row.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    Ok(rows)
}

fn time_value(value: &str) -> Option<i64> {
    let digits: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}

fn ordered_later(later: &str, earlier: &str) -> bool {
    match (time_value(later), time_value(earlier)) {
        (Some(a), Some(b)) => a >= b,
        _ if later.is_empty() || earlier.is_empty() => false,
        _ => later >= earlier,
    }
}

fn is_flashback(event: &EventFact) -> bool {
    let text = format!("{} {} {}", event.title, event.cause, event.outcome);
    ["回忆", "闪回", "梦", "幻", "讲述", "转述"]
        .iter()
        .any(|word| text.contains(word))
}

fn fingerprint(code: &str, ids: &[String]) -> String {
    let mut sorted = ids.to_vec();
    sorted.sort();
    crate::hash_text(&format!("{code}:{}", sorted.join(",")))
}

fn exception_exists(
    conn: &Connection,
    project_id: &str,
    rule_id: &str,
    fp: &str,
) -> AppResult<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM rule_exception WHERE project_id=?1 AND rule_id=?2 AND fingerprint=?3 AND permanent=1)",
        params![project_id, rule_id, fp],
        |row| row.get(0),
    )?)
}

fn issue_exists(conn: &Connection, project_id: &str, fp: &str) -> AppResult<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM issue WHERE project_id=?1 AND fingerprint=?2 AND status IN ('open','deferred'))",
        params![project_id, fp],
        |row| row.get(0),
    )?)
}

#[allow(clippy::too_many_arguments)]
fn insert_issue(
    conn: &Connection,
    project_id: &str,
    rule: &RuleRecord,
    claim_ids: Vec<String>,
    evidence_spans: Vec<String>,
    explanation: String,
    solver_core: Vec<String>,
    risk_level: &str,
) -> AppResult<Option<AuditIssue>> {
    let fp = fingerprint(&rule.code, &claim_ids);
    if issue_exists(conn, project_id, &fp)? || exception_exists(conn, project_id, &rule.id, &fp)? {
        return Ok(None);
    }
    let certificate = ConflictCertificate {
        rule_id: rule.code.clone(),
        severity: rule.severity.clone(),
        claim_ids: claim_ids.clone(),
        evidence_spans: evidence_spans.clone(),
        solver_core: solver_core.clone(),
        explanation: explanation.clone(),
        suggested_actions: vec![
            "修改正文或事实".into(),
            "标记为例外并记录原因".into(),
            "建立新分支".into(),
        ],
    };
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now();
    conn.execute(
        "INSERT INTO issue (id, project_id, rule_id, severity, claim_ids, evidence_spans,
            explanation, certificate, solver_core, metric_snapshot, risk_level, status,
            created_at, fingerprint) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'{}',?10,'open',?11,?12)",
        params![
            id,
            project_id,
            rule.id,
            rule.severity,
            serde_json::to_string(&claim_ids)?,
            serde_json::to_string(&evidence_spans)?,
            explanation,
            serde_json::to_string(&certificate)?,
            serde_json::to_string(&solver_core)?,
            risk_level,
            ts,
            fp
        ],
    )?;
    Ok(Some(AuditIssue {
        id,
        rule_id: rule.id.clone(),
        rule_code: rule.code.clone(),
        constraint_layer: rule.constraint_layer.clone(),
        severity: rule.severity.clone(),
        claim_ids,
        evidence_spans,
        explanation,
        certificate,
        solver_core,
        risk_level: risk_level.into(),
        fingerprint: fp,
        status: "open".into(),
        deferred_until: None,
        created_at: ts,
    }))
}

fn check_dead_acts(
    conn: &Connection,
    project_id: &str,
    rule: &RuleRecord,
) -> AppResult<Vec<AuditIssue>> {
    let claims = claims(conn, project_id)?;
    let events = events(conn, project_id)?;
    let deaths: Vec<&ClaimFact> = claims
        .iter()
        .filter(|claim| {
            claim.truth_layer == "world"
                && (claim.predicate.contains("死")
                    || claim.predicate.contains("亡")
                    || claim.predicate.to_lowercase().contains("dead"))
        })
        .collect();
    let mut issues = Vec::new();
    for death in deaths {
        for event in &events {
            if !event
                .participants
                .iter()
                .any(|participant| participant == &death.subject)
                || is_flashback(event)
            {
                continue;
            }
            let action_after = if death.valid_from.is_empty() || event.story_time.is_empty() {
                true
            } else {
                ordered_later(&event.story_time, &death.valid_from)
            };
            if !action_after {
                continue;
            }
            let core =
                solver::dead_after_death_core(&death.id, &event.id, &rule.code).unwrap_or_default();
            let explanation = format!(
                "{} 在故事时间 {} 已死亡，但事件《{}》发生在 {}，且未标记为回忆。",
                death.subject,
                if death.valid_from.is_empty() {
                    "未知"
                } else {
                    &death.valid_from
                },
                event.title,
                if event.story_time.is_empty() {
                    "未知"
                } else {
                    &event.story_time
                }
            );
            if let Some(issue) = insert_issue(
                conn,
                project_id,
                rule,
                vec![death.id.clone(), event.id.clone()],
                vec![death.source_span.clone(), event.source_span.clone()],
                explanation,
                core,
                if rule.severity == "block" { "L3" } else { "L2" },
            )? {
                issues.push(issue);
            }
        }
    }
    Ok(issues)
}

fn check_two_places(
    conn: &Connection,
    project_id: &str,
    rule: &RuleRecord,
) -> AppResult<Vec<AuditIssue>> {
    let events = events(conn, project_id)?;
    let mut by_time: HashMap<String, Vec<&EventFact>> = HashMap::new();
    for event in &events {
        if !event.story_time.is_empty() {
            by_time.entry(event.story_time.clone()).or_default().push(event);
        }
    }
    let mut issues = Vec::new();
    for group in by_time.values() {
        for i in 0..group.len() {
            for j in (i + 1)..group.len() {
                let a = group[i];
                let b = group[j];
                let shared: Vec<&String> = a
                    .participants
                    .iter()
                    .filter(|p| b.participants.contains(*p))
                    .collect();
                if shared.is_empty() {
                    continue;
                }
                let Some(place_a) = &a.place_id else { continue };
                let Some(place_b) = &b.place_id else { continue };
                if place_a == place_b {
                    continue;
                }
                let core = solver::two_places_core(&a.id, &b.id).unwrap_or_default();
                for participant in shared {
                    if let Some(issue) = insert_issue(
                        conn,
                        project_id,
                        rule,
                        vec![a.id.clone(), b.id.clone()],
                        vec![a.source_span.clone(), b.source_span.clone()],
                        format!(
                            "{participant} 在同一故事时间 {} 分别出现在 {} 与 {}。",
                            a.story_time, place_a, place_b
                        ),
                        core.clone(),
                        "L3",
                    )? {
                        issues.push(issue);
                    }
                }
            }
        }
    }
    Ok(issues)
}

fn check_item_lifecycle(
    conn: &Connection,
    project_id: &str,
    rule: &RuleRecord,
) -> AppResult<Vec<AuditIssue>> {
    let claims = claims(conn, project_id)?;
    let destroyed: Vec<&ClaimFact> = claims
        .iter()
        .filter(|claim| {
            claim.predicate.contains("销毁")
                || claim.predicate.contains("毁")
                || claim.predicate.to_lowercase().contains("destroy")
        })
        .collect();
    let held: Vec<&ClaimFact> = claims
        .iter()
        .filter(|claim| {
            claim.predicate.contains("持有")
                || claim.predicate.contains("握")
                || claim.predicate.contains("拥有")
                || claim.predicate.to_lowercase().contains("hold")
        })
        .collect();
    let mut issues = Vec::new();
    for destroy in destroyed {
        for possession in &held {
            let item = if destroy.subject.trim().is_empty() {
                &destroy.object
            } else {
                &destroy.subject
            };
            if item != &possession.object && item != &possession.subject {
                continue;
            }
            if !destroy.valid_from.is_empty()
                && !possession.valid_from.is_empty()
                && !ordered_later(&possession.valid_from, &destroy.valid_from)
            {
                continue;
            }
            let core = solver::item_after_destroy_core(&destroy.id, &possession.id)
                .unwrap_or_default();
            if let Some(issue) = insert_issue(
                conn,
                project_id,
                rule,
                vec![destroy.id.clone(), possession.id.clone()],
                vec![destroy.source_span.clone(), possession.source_span.clone()],
                format!(
                    "物品「{}」已在 {} 被销毁或交出，但 {} 仍在 {} 持有。",
                    item,
                    destroy.valid_from,
                    possession.subject,
                    possession.valid_from
                ),
                core,
                "L3",
            )? {
                issues.push(issue);
            }
        }
    }
    Ok(issues)
}

fn check_relation_direction(
    conn: &Connection,
    project_id: &str,
    rule: &RuleRecord,
) -> AppResult<Vec<AuditIssue>> {
    let mut stmt = conn.prepare(
        "SELECT r1.id, r1.from_id, r1.to_id, r1.type, r2.id, r2.type
         FROM relation r1 JOIN relation r2
           ON r2.project_id=r1.project_id AND r2.from_id=r1.to_id AND r2.to_id=r1.from_id
         WHERE r1.project_id=?1 AND r1.status='confirmed' AND r2.status='confirmed'
           AND r1.id < r2.id",
    )?;
    let rows = stmt
        .query_map([project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    let mut issues = Vec::new();
    for (id_a, from, to, type_a, id_b, type_b) in rows {
        let directional = ["父", "母", "师", "上司", "主", "下属"];
        if !directional
            .iter()
            .any(|word| type_a.contains(word) || type_b.contains(word))
        {
            continue;
        }
        if let Some(issue) = insert_issue(
            conn,
            project_id,
            rule,
            vec![id_a, id_b],
            vec![format!("relation:{from}->{to}"), format!("relation:{to}->{from}")],
            format!(
                "同一对角色同时存在「{}」与「{}」的相反方向关系，需确认哪条是角色认知或误传。",
                type_a, type_b
            ),
            vec![],
            "L2",
        )? {
            issues.push(issue);
        }
    }
    Ok(issues)
}

fn check_pov_leak(
    conn: &Connection,
    project_id: &str,
    rule: &RuleRecord,
) -> AppResult<Vec<AuditIssue>> {
    let claims = claims(conn, project_id)?;
    let planned: Vec<&ClaimFact> = claims
        .iter()
        .filter(|claim| claim.truth_layer == "planned" && claim.object.chars().count() >= 2)
        .collect();
    if planned.is_empty() {
        return Ok(vec![]);
    }
    let mut stmt = conn.prepare(
        "SELECT s.id, s.pov, s.chapter_id, t.content FROM scene s
         JOIN text_fts t ON t.chapter_id=s.chapter_id
         WHERE s.project_id=?1",
    )?;
    let scenes = stmt
        .query_map([project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    let mut issues = Vec::new();
    for claim in planned {
        let known: bool = claims.iter().any(|candidate| {
            candidate.object == claim.object
                && candidate.predicate.contains("知道")
                && candidate
                    .perspective_id
                    .as_deref()
                    .map(|p| p == claim.perspective_id.as_deref().unwrap_or_default())
                    .unwrap_or(false)
        });
        if known {
            continue;
        }
        for (scene_id, pov, chapter_id, text) in &scenes {
            if pov.is_empty() || claim.perspective_id.as_deref() == Some(pov.as_str()) {
                continue;
            }
            let Some(index) = text.find(&claim.object) else { continue };
            let start = text[..index].char_indices().rev().nth(30).map(|(i, _)| i).unwrap_or(0);
            let end = (index + claim.object.len() + 60).min(text.len());
            let excerpt = text[start..end].replace('\n', " ");
            if let Some(issue) = insert_issue(
                conn,
                project_id,
                rule,
                vec![claim.id.clone(), scene_id.clone()],
                vec![claim.source_span.clone(), format!("chapter:{chapter_id}｜{excerpt}")],
                format!(
                    "POV「{}」的叙述出现了计划后揭示的信息「{}」，但没有对应的知情或目睹记录。",
                    pov, claim.object
                ),
                vec![],
                "L2",
            )? {
                issues.push(issue);
            }
        }
    }
    Ok(issues)
}

fn check_overdue_promises(
    conn: &Connection,
    project_id: &str,
    rule: &RuleRecord,
) -> AppResult<Vec<AuditIssue>> {
    let current: i32 = conn.query_row(
        "SELECT COALESCE(MAX(number),0) FROM chapter WHERE project_id=?1",
        [project_id],
        |row| row.get(0),
    )?;
    let mut stmt = conn.prepare(
        "SELECT id, title, target_window, evidence_ids FROM promise
         WHERE project_id=?1 AND state NOT IN ('paid_off','abandoned')",
    )?;
    let rows = stmt
        .query_map([project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    let mut issues = Vec::new();
    for (id, title, window, evidence) in rows {
        let Some(target) = time_value(&window) else { continue };
        if current < target as i32 {
            continue;
        }
        let evidence: Vec<String> = serde_json::from_str(&evidence).unwrap_or_default();
        if let Some(issue) = insert_issue(
            conn,
            project_id,
            rule,
            vec![id],
            evidence,
            format!(
                "伏笔「{title}」计划在「{window}」兑现，当前已到第 {current} 章仍未推进。"
            ),
            vec![],
            "L2",
        )? {
            issues.push(issue);
        }
    }
    Ok(issues)
}

pub fn run_audit(conn: &Connection, project_id: &str) -> AppResult<AuditRunResult> {
    let rules = list(conn, project_id)?;
    let mut issues = Vec::new();
    for rule in rules.iter().filter(|rule| rule.enabled) {
        let found = match rule.code.as_str() {
            "dead_character_acts" => check_dead_acts(conn, project_id, rule)?,
            "character_two_places" => check_two_places(conn, project_id, rule)?,
            "item_destroyed_then_held" => check_item_lifecycle(conn, project_id, rule)?,
            "relation_direction_conflict" => check_relation_direction(conn, project_id, rule)?,
            "pov_knowledge_leak" => check_pov_leak(conn, project_id, rule)?,
            "promise_overdue" => check_overdue_promises(conn, project_id, rule)?,
            _ => vec![],
        };
        issues.extend(found);
    }
    Ok(AuditRunResult {
        issues_created: issues.len() as i32,
        issues,
    })
}

pub fn issues(conn: &Connection, project_id: &str, status: Option<&str>) -> AppResult<Vec<AuditIssue>> {
    let sql = if status.is_some() {
        "SELECT i.id, i.rule_id, r.code, r.constraint_layer, i.severity, i.claim_ids,
            i.evidence_spans, i.explanation, i.certificate, i.solver_core, i.risk_level,
            i.fingerprint, i.status, i.deferred_until, i.created_at
         FROM issue i JOIN rule r ON r.id=i.rule_id
         WHERE i.project_id=?1 AND i.status=?2 ORDER BY i.created_at DESC"
    } else {
        "SELECT i.id, i.rule_id, r.code, r.constraint_layer, i.severity, i.claim_ids,
            i.evidence_spans, i.explanation, i.certificate, i.solver_core, i.risk_level,
            i.fingerprint, i.status, i.deferred_until, i.created_at
         FROM issue i JOIN rule r ON r.id=i.rule_id
         WHERE i.project_id=?1 ORDER BY i.created_at DESC"
    };
    let mut stmt = conn.prepare(sql)?;
    let map = |row: &Row<'_>| {
        let claim_ids: String = row.get(5)?;
        let evidence: String = row.get(6)?;
        let certificate: String = row.get(8)?;
        let solver_core: String = row.get(9)?;
        Ok(AuditIssue {
            id: row.get(0)?,
            rule_id: row.get(1)?,
            rule_code: row.get(2)?,
            constraint_layer: row.get(3)?,
            severity: row.get(4)?,
            claim_ids: serde_json::from_str(&claim_ids).unwrap_or_default(),
            evidence_spans: serde_json::from_str(&evidence).unwrap_or_default(),
            explanation: row.get(7)?,
            certificate: serde_json::from_str(&certificate).unwrap_or(ConflictCertificate {
                rule_id: String::new(),
                severity: String::new(),
                claim_ids: vec![],
                evidence_spans: vec![],
                solver_core: vec![],
                explanation: String::new(),
                suggested_actions: vec![],
            }),
            solver_core: serde_json::from_str(&solver_core).unwrap_or_default(),
            risk_level: row.get(10)?,
            fingerprint: row.get(11)?,
            status: row.get(12)?,
            deferred_until: row.get(13)?,
            created_at: row.get(14)?,
        })
    };
    let rows = if let Some(status) = status {
        stmt.query_map(params![project_id, status], map)?
            .collect::<Result<Vec<_>, _>>()?
    } else {
        stmt.query_map([project_id], map)?
            .collect::<Result<Vec<_>, _>>()?
    };
    drop(stmt);
    Ok(rows)
}

pub fn issue_action(
    conn: &Connection,
    project_id: &str,
    id: &str,
    action: &str,
    reason: &str,
) -> AppResult<AuditIssue> {
    let issue: AuditIssue = issues(conn, project_id, None)?
        .into_iter()
        .find(|issue| issue.id == id)
        .ok_or_else(|| AppError::Message("找不到审计问题".into()))?;
    let ts = now();
    match action {
        "defer" => {
            conn.execute(
                "UPDATE issue SET status='deferred', deferred_until=?3 WHERE project_id=?1 AND id=?2",
                params![project_id, id, ts],
            )?;
        }
        "resolve" => {
            conn.execute(
                "UPDATE issue SET status='resolved' WHERE project_id=?1 AND id=?2",
                params![project_id, id],
            )?;
        }
        "exception_once" | "exception_permanent" => {
            if reason.trim().is_empty() {
                return Err(AppError::Message("例外必须记录原因".into()));
            }
            conn.execute(
                "INSERT INTO rule_exception (id, project_id, rule_id, fingerprint, reason, permanent, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    project_id,
                    issue.rule_id,
                    issue.fingerprint,
                    reason.trim(),
                    (action == "exception_permanent") as i32,
                    ts
                ],
            )?;
            conn.execute(
                "UPDATE issue SET status='exception' WHERE project_id=?1 AND id=?2",
                params![project_id, id],
            )?;
        }
        "new_branch" => {
            conn.execute(
                "UPDATE issue SET status='branched' WHERE project_id=?1 AND id=?2",
                params![project_id, id],
            )?;
        }
        other => return Err(AppError::Message(format!("未知处理动作：{other}"))),
    }
    conn.execute(
        "INSERT INTO audit_log (id, project_id, action, subject, detail, created_at)
         VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            uuid::Uuid::new_v4().to_string(),
            project_id,
            format!("issue_{action}"),
            id,
            json!({"reason": reason, "rule": issue.rule_code}).to_string(),
            ts
        ],
    )?;
    issues(conn, project_id, None)?
        .into_iter()
        .find(|issue| issue.id == id)
        .ok_or_else(|| AppError::Message("问题状态更新失败".into()))
}

#[cfg(test)]
mod tests {
    use rusqlite::params;

    use crate::{project, scenes};

    use super::*;

    fn setup() -> (tempfile::TempDir, crate::project::LoadedProject) {
        let dir = tempfile::tempdir().unwrap();
        let mut loaded = project::create(&crate::domain::CreateProjectInput {
            name: "t".into(),
            root_path: dir.path().join("p").to_string_lossy().into_owned(),
            mode: "both".into(),
            writing_language: None,
        })
        .unwrap();
        seed_defaults(&loaded.conn, &loaded.summary.id).unwrap();
        (dir, loaded)
    }

    fn claim(
        conn: &Connection,
        pid: &str,
        subject: &str,
        predicate: &str,
        object: &str,
        time: &str,
    ) {
        conn.execute(
            "INSERT INTO claim (id,project_id,subject,predicate,object,truth_layer,valid_from,
                source_span,status,version,created_at,updated_at)
             VALUES (?1,?2,?3,?4,?5,'world',?6,?7,'confirmed',1,'now','now')",
            params![
                uuid::Uuid::new_v4().to_string(),
                pid,
                subject,
                predicate,
                object,
                time,
                format!("ch:{subject}")
            ],
        )
        .unwrap();
    }

    fn event(
        conn: &Connection,
        pid: &str,
        title: &str,
        participants: &[&str],
        place: &str,
        time: &str,
    ) {
        conn.execute(
            "INSERT INTO event (id,project_id,title,participants,place_id,story_time,source_span,
                status,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,'confirmed','now')",
            params![
                uuid::Uuid::new_v4().to_string(),
                pid,
                title,
                serde_json::to_string(participants).unwrap(),
                place,
                time,
                format!("ch:{title}")
            ],
        )
        .unwrap();
    }

    #[test]
    fn detects_death_two_places_and_item_conflicts_with_cores() {
        let (_dir, loaded) = setup();
        let pid = loaded.summary.id.clone();
        claim(&loaded.conn, &pid, "林澈", "死亡", "", "3");
        claim(&loaded.conn, &pid, "黄铜钥匙", "被销毁", "", "2");
        claim(&loaded.conn, &pid, "林澈", "持有", "黄铜钥匙", "5");
        event(&loaded.conn, &pid, "雨夜归来", &["林澈"], "旧车站", "9");
        event(&loaded.conn, &pid, "葬礼", &["林澈"], "灵堂", "9");
        let result = run_audit(&loaded.conn, &pid).unwrap();
        assert!(result.issues.iter().any(|i| i.rule_code == "dead_character_acts" && i.risk_level == "L3"));
        assert!(result.issues.iter().any(|i| i.rule_code == "character_two_places"));
        assert!(result.issues.iter().any(|i| i.rule_code == "item_destroyed_then_held"));
        assert!(result
            .issues
            .iter()
            .any(|i| i.rule_code == "dead_character_acts" && !i.solver_core.is_empty()));
    }

    #[test]
    fn exceptions_suppress_same_fingerprint() {
        let (_dir, loaded) = setup();
        let pid = loaded.summary.id.clone();
        claim(&loaded.conn, &pid, "林澈", "死亡", "", "1");
        event(&loaded.conn, &pid, "归来", &["林澈"], "车站", "4");
        let result = run_audit(&loaded.conn, &pid).unwrap();
        let issue = result.issues.first().unwrap();
        issue_action(&loaded.conn, &pid, &issue.id, "exception_permanent", "这是鬼魂视角").unwrap();
        let again = run_audit(&loaded.conn, &pid).unwrap();
        assert_eq!(again.issues_created, 0);
    }

    #[test]
    fn overdue_promise_creates_soft_issue() {
        let (_dir, loaded) = setup();
        let pid = loaded.summary.id.clone();
        loaded
            .conn
            .execute(
                "INSERT INTO chapter (id,project_id,number,title,status,style_epoch,created_at,updated_at)
                 VALUES (?1,?2,5,'第五章','planned',1,'now','now')",
                params![uuid::Uuid::new_v4().to_string(), pid],
            )
            .unwrap();
        loaded
            .conn
            .execute(
                "INSERT INTO promise (id,project_id,title,state,target_window,payoff_contract,evidence_ids,created_at,updated_at)
                 VALUES (?1,?2,'旧钥匙','planted','第3章','揭示父亲身份','[\"ch1\"]','now','now')",
                params![uuid::Uuid::new_v4().to_string(), pid],
            )
            .unwrap();
        let result = run_audit(&loaded.conn, &pid).unwrap();
        assert!(result.issues.iter().any(|i| i.rule_code == "promise_overdue"));
    }

    #[test]
    fn audit_finds_no_false_death_conflict_for_flashback() {
        let (_dir, loaded) = setup();
        let pid = loaded.summary.id.clone();
        claim(&loaded.conn, &pid, "林澈", "死亡", "", "1");
        event(&loaded.conn, &pid, "回忆中的告别", &["林澈"], "灵堂", "9");
        let result = run_audit(&loaded.conn, &pid).unwrap();
        assert!(!result
            .issues
            .iter()
            .any(|i| i.rule_code == "dead_character_acts"));
        let _ = scenes::chapters(&loaded.conn, &pid);
    }

    #[test]
    fn detects_pov_knowledge_leak_from_planned_claim() {
        let (_dir, loaded) = setup();
        let pid = loaded.summary.id.clone();
        loaded
            .conn
            .execute(
                "INSERT INTO claim (id,project_id,subject,predicate,object,truth_layer,
                    perspective_id,source_span,status,version,created_at,updated_at)
                 VALUES (?1,?2,'父亲','仍然活着','父亲仍活着','planned','村长','ch18','confirmed',1,'now','now')",
                params![uuid::Uuid::new_v4().to_string(), pid],
            )
            .unwrap();
        let chapter_id = uuid::Uuid::new_v4().to_string();
        loaded
            .conn
            .execute(
                "INSERT INTO chapter (id,project_id,number,title,status,style_epoch,created_at,updated_at)
                 VALUES (?1,?2,2,'信','planned',1,'now','now')",
                params![chapter_id, pid],
            )
            .unwrap();
        loaded
            .conn
            .execute(
                "INSERT INTO scene (id,project_id,chapter_id,number,goal,conflict,pov,status,emotion_beat,sensory_plan,created_at,updated_at)
                 VALUES (?1,?2,?3,1,'翻信','母亲阻止','林澈','planned','{}','{}','now','now')",
                params![uuid::Uuid::new_v4().to_string(), pid, chapter_id],
            )
            .unwrap();
        loaded
            .conn
            .execute(
                "INSERT INTO text_fts (content,tokenized,source_id,project_id,source_kind,chapter_id,branch_id)
                 VALUES ('林澈低声说，父亲仍活着。','林澈 低声 说 父亲 仍 活着','idx',?1,'chapter',?2,NULL)",
                params![pid, chapter_id],
            )
            .unwrap();
        let result = run_audit(&loaded.conn, &pid).unwrap();
        assert!(result
            .issues
            .iter()
            .any(|issue| issue.rule_code == "pov_knowledge_leak"));
    }
}
