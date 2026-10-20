use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::Value;

use crate::{
    domain::{
        Candidate, WikiEditInput, WikiPage, WikiRebuildResult,
    },
    error::{AppError, AppResult},
    now,
};

const GENERATOR_VERSION: i32 = 1;

fn page_from_row(row: &Row<'_>) -> rusqlite::Result<WikiPage> {
    let source_ids: String = row.get(6)?;
    let source_spans: String = row.get(7)?;
    Ok(WikiPage {
        id: row.get(0)?,
        entity_id: row.get(1)?,
        kind: row.get(2)?,
        title: row.get(3)?,
        content: row.get(4)?,
        content_hash: row.get(5)?,
        source_ids: serde_json::from_str(&source_ids).unwrap_or_default(),
        source_spans: serde_json::from_str(&source_spans).unwrap_or_default(),
        generator_version: row.get(8)?,
        edited_by_author: row.get(9)?,
        generated_at: row.get(10)?,
        stale_at: row.get(11)?,
    })
}

const SELECT: &str = "SELECT id, entity_id, kind, title, content, content_hash, source_ids,
    source_spans, generator_version, edited_by_author, generated_at, stale_at FROM wiki_page";

pub fn list(conn: &Connection, project_id: &str) -> AppResult<Vec<WikiPage>> {
    let mut stmt = conn.prepare(&format!(
        "{SELECT} WHERE project_id=?1
         ORDER BY CASE kind
           WHEN 'character' THEN 0 WHEN 'place' THEN 1 WHEN 'world_rule' THEN 2
           WHEN 'timeline' THEN 3 WHEN 'promise' THEN 4 ELSE 5 END, title"
    ))?;
    let rows = stmt
        .query_map([project_id], page_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    Ok(rows)
}

pub fn get(conn: &Connection, project_id: &str, id: &str) -> AppResult<WikiPage> {
    conn.query_row(
        &format!("{SELECT} WHERE project_id=?1 AND id=?2"),
        params![project_id, id],
        page_from_row,
    )
    .optional()?
    .ok_or_else(|| AppError::Message("找不到百科页面".into()))
}

#[derive(Debug)]
struct DraftPage {
    id: String,
    entity_id: Option<String>,
    kind: &'static str,
    title: String,
    content: String,
    source_ids: Vec<String>,
    source_spans: Vec<String>,
}

impl DraftPage {
    fn new(kind: &'static str, title: String, body: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            entity_id: None,
            kind,
            title,
            content: format!(
                "派生视图 · 非权威事实。事实变更后本页会标记过期，并可全量重建。\n\n{body}"
            ),
            source_ids: Vec::new(),
            source_spans: Vec::new(),
        }
    }

    fn source(&mut self, source_id: Option<&str>, source_span: Option<&str>) {
        if let Some(source_id) = source_id.map(str::trim).filter(|value| !value.is_empty()) {
            self.source_ids.push(source_id.to_string());
        }
        if let Some(source_span) = source_span.map(str::trim).filter(|value| !value.is_empty()) {
            self.source_spans.push(source_span.to_string());
        }
    }

    fn finish(mut self) -> Self {
        self.source_ids.sort();
        self.source_ids.dedup();
        self.source_spans.sort();
        self.source_spans.dedup();
        self
    }
}

fn json_list(text: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(text).unwrap_or_default()
}

fn claim_lines(
    conn: &Connection,
    project_id: &str,
    entity_id: &str,
    branch_id: Option<&str>,
    page: &mut DraftPage,
) -> AppResult<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT id, subject, predicate, object, truth_layer, valid_from, valid_to,
            source_span, perspective_id
         FROM claim
         WHERE project_id=?1 AND status='confirmed'
           AND (branch_id IS NULL OR branch_id=?2)
           AND (subject_id=?3 OR object_entity_id=?3)
         ORDER BY truth_layer, updated_at DESC",
    )?;
    let rows = stmt
        .query_map(params![project_id, branch_id, entity_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, Option<String>>(8)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    let mut lines = Vec::new();
    for (id, subject, predicate, object, layer, from, to, span, perspective) in rows {
        page.source(Some(&id), Some(&span));
        let range = match (from, to) {
            (Some(from), Some(to)) => format!(" · {from}→{to}"),
            (Some(from), None) => format!(" · 自 {from}"),
            _ => String::new(),
        };
        let perspective = perspective
            .filter(|value| !value.is_empty())
            .map(|value| format!(" · 视角 {value}"))
            .unwrap_or_default();
        lines.push(format!(
            "- [{layer}] {subject} · {predicate} · {object}{range}{perspective}（{id}）"
        ));
    }
    Ok(lines)
}

fn build_character(
    conn: &Connection,
    project_id: &str,
    branch_id: Option<&str>,
    id: &str,
    name: &str,
    aliases: &str,
    description: &str,
) -> AppResult<DraftPage> {
    let mut page = DraftPage::new("character", name.to_string(), String::new());
    page.entity_id = Some(id.to_string());
    page.source(Some(id), None);
    let aliases = json_list(aliases);
    let core = conn
        .query_row(
            "SELECT desire, need, fear, flaw, \"values\", secret, taboo, voice_anchor
             FROM character_core WHERE entity_id=?1",
            [id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .optional()?;
    let mut body = format!(
        "别名：{}\n简介：{}\n",
        if aliases.is_empty() {
            "无".to_string()
        } else {
            aliases.join("、")
        },
        if description.trim().is_empty() {
            "未记录"
        } else {
            description
        }
    );
    if let Some((desire, need, fear, flaw, values, secret, taboo, anchor)) = core {
        body.push_str("\n## 人物核心\n");
        body.push_str(&format!(
            "- 欲望：{}\n- 需求：{}\n- 恐惧：{}\n- 缺陷：{}\n- 价值观：{}\n- 秘密：{}\n- 底线：{}\n",
            empty_as(desire, "未记录"),
            empty_as(need, "未记录"),
            empty_as(fear, "未记录"),
            empty_as(flaw, "未记录"),
            empty_as(json_list(&values).join("、"), "未记录"),
            empty_as(secret, "未记录"),
            empty_as(json_list(&taboo).join("、"), "未记录"),
        ));
        if !anchor.trim().is_empty() {
            body.push_str(&format!("\n## 作者声音锚点\n{}\n", anchor.trim()));
        }
    }
    let mut stmt = conn.prepare(
        "SELECT r.id, r.type, e.canonical_name, r.evidence_id
         FROM relation r JOIN entity e ON e.id=CASE WHEN r.from_id=?2 THEN r.to_id ELSE r.from_id END
         WHERE r.project_id=?1 AND r.status='confirmed' AND (r.from_id=?2 OR r.to_id=?2)
         ORDER BY e.canonical_name",
    )?;
    let relations = stmt
        .query_map(params![project_id, id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    if !relations.is_empty() {
        body.push_str("\n## 关系\n");
        for (relation_id, relation_type, target, evidence) in relations {
            page.source(Some(&relation_id), evidence.as_deref());
            body.push_str(&format!("- {target}：{relation_type}（{relation_id}）\n"));
        }
    }
    let claims = claim_lines(conn, project_id, id, branch_id, &mut page)?;
    if !claims.is_empty() {
        body.push_str("\n## 已确认事实\n");
        body.push_str(&claims.join("\n"));
        body.push('\n');
    }
    let mut stmt = conn.prepare(
        "SELECT id, before_state, event, after_state, evidence_id
         FROM arc_beat WHERE project_id=?1 AND character_id=?2 ORDER BY created_at",
    )?;
    let beats = stmt
        .query_map(params![project_id, id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    if !beats.is_empty() {
        body.push_str("\n## 成长轨迹\n");
        for (beat_id, before, event, after, evidence) in beats {
            page.source(Some(&beat_id), evidence.as_deref());
            body.push_str(&format!("- {before} → {event} → {after}\n"));
        }
    }
    page.content = format!(
        "派生视图 · 非权威事实。事实变更后本页会标记过期，并可全量重建。\n\n{body}"
    );
    Ok(page.finish())
}

fn build_place(
    conn: &Connection,
    project_id: &str,
    branch_id: Option<&str>,
    id: &str,
    name: &str,
    aliases: &str,
    description: &str,
) -> AppResult<DraftPage> {
    let mut page = DraftPage::new("place", name.to_string(), String::new());
    page.entity_id = Some(id.to_string());
    page.source(Some(id), None);
    let mut body = format!(
        "别名：{}\n简介：{}\n",
        empty_as(json_list(aliases).join("、"), "无"),
        empty_as(description.to_string(), "未记录")
    );
    let mut stmt = conn.prepare(
        "SELECT r.id, r.type, e.canonical_name, r.evidence_id
         FROM relation r JOIN entity e ON e.id=CASE WHEN r.from_id=?2 THEN r.to_id ELSE r.from_id END
         WHERE r.project_id=?1 AND r.status='confirmed' AND (r.from_id=?2 OR r.to_id=?2)
         ORDER BY e.canonical_name",
    )?;
    let relations = stmt
        .query_map(params![project_id, id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    if !relations.is_empty() {
        body.push_str("\n## 地理与势力关系\n");
        for (relation_id, relation_type, target, evidence) in relations {
            page.source(Some(&relation_id), evidence.as_deref());
            body.push_str(&format!("- {target}：{relation_type}（{relation_id}）\n"));
        }
    }
    let mut stmt = conn.prepare(
        "SELECT id, title, story_time, reveal_time, outcome, source_span
         FROM event WHERE project_id=?1 AND status='confirmed' AND place_id=?2
           AND (branch_id IS NULL OR branch_id=?3)
         ORDER BY story_time, reveal_time",
    )?;
    let events = stmt
        .query_map(params![project_id, id, branch_id], |row| {
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
    if !events.is_empty() {
        body.push_str("\n## 相关事件\n");
        for (event_id, title, story_time, reveal_time, outcome, span) in events {
            page.source(Some(&event_id), Some(&span));
            body.push_str(&format!(
                "- {title}｜故事时间 {story_time}｜叙述时间 {reveal_time}｜{outcome}（{event_id}）\n"
            ));
        }
    }
    let claims = claim_lines(conn, project_id, id, branch_id, &mut page)?;
    if !claims.is_empty() {
        body.push_str("\n## 已确认事实\n");
        body.push_str(&claims.join("\n"));
        body.push('\n');
    }
    page.content = format!(
        "派生视图 · 非权威事实。事实变更后本页会标记过期，并可全量重建。\n\n{body}"
    );
    Ok(page.finish())
}

fn build_rule(
    conn: &Connection,
    project_id: &str,
    id: &str,
    code: &str,
    description: &str,
    predicate: &str,
    exception_scope: &str,
    layer: &str,
    severity: &str,
) -> AppResult<DraftPage> {
    let mut page = DraftPage::new(
        "world_rule",
        format!("规则 · {code}"),
        String::new(),
    );
    page.source(Some(&format!("rule:{id}")), None);
    let mut body = format!(
        "## 条款\n{}\n\n## 执行层\n- 层级：{layer}\n- 严重度：{severity}\n\n## 条件与例外\n- 条件：{}\n- 例外：{}\n",
        empty_as(description.to_string(), "未填写"),
        pretty_json(predicate),
        pretty_json(exception_scope),
    );
    let mut stmt = conn.prepare(
        "SELECT id, explanation, evidence_spans, certificate FROM issue
         WHERE project_id=?1 AND rule_id=?2 ORDER BY created_at DESC LIMIT 12",
    )?;
    let issues = stmt
        .query_map(params![project_id, id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    if !issues.is_empty() {
        body.push_str("\n## 违例案例\n");
        for (issue_id, explanation, spans, certificate) in issues {
            for span in json_list(&spans) {
                page.source(None, Some(&span));
            }
            page.source(Some(&format!("issue:{issue_id}")), Some(&certificate));
            body.push_str(&format!("- {explanation}（{issue_id}）\n"));
        }
    }
    page.content = format!(
        "派生视图 · 非权威事实。规则原文仍以规则表为权威来源。\n\n{body}"
    );
    Ok(page.finish())
}

fn build_timeline(
    conn: &Connection,
    project_id: &str,
    branch_id: Option<&str>,
) -> AppResult<DraftPage> {
    let mut page = DraftPage::new("timeline", "双时间线".into(), String::new());
    page.source(Some(&format!("project:{project_id}")), None);
    let mut body = String::from("## 事件\n");
    let mut stmt = conn.prepare(
        "SELECT id, title, story_time, reveal_time, branch_id, outcome, source_span
         FROM event WHERE project_id=?1 AND status='confirmed'
           AND (branch_id IS NULL OR branch_id=?2)
         ORDER BY story_time, reveal_time, created_at",
    )?;
    let events = stmt
        .query_map(params![project_id, branch_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    if events.is_empty() {
        body.push_str("- 尚无已确认事件。\n");
    }
    for (id, title, story, reveal, branch, outcome, span) in events {
        page.source(Some(&id), Some(&span));
        body.push_str(&format!(
            "- {title}｜故事 {story}｜叙述 {reveal}｜分支 {}｜{outcome}（{id}）\n",
            branch.as_deref().unwrap_or("共享")
        ));
    }
    body.push_str("\n## 事实有效区间\n");
    let mut stmt = conn.prepare(
        "SELECT id, subject, predicate, object, valid_from, valid_to, source_span
         FROM claim WHERE project_id=?1 AND status='confirmed'
           AND (branch_id IS NULL OR branch_id=?2)
           AND (valid_from IS NOT NULL OR valid_to IS NOT NULL)
         ORDER BY valid_from",
    )?;
    let claims = stmt
        .query_map(params![project_id, branch_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, String>(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    if claims.is_empty() {
        body.push_str("- 尚无带有效区间的事实。\n");
    }
    for (id, subject, predicate, object, from, to, span) in claims {
        page.source(Some(&id), Some(&span));
        body.push_str(&format!(
            "- {subject} · {predicate} · {object}｜{}→{}（{id}）\n",
            from.as_deref().unwrap_or("不限"),
            to.as_deref().unwrap_or("不限")
        ));
    }
    page.content = format!(
        "派生视图 · 非权威事实。事实变更后本页会标记过期，并可全量重建。\n\n{body}"
    );
    Ok(page.finish())
}

fn build_promises(conn: &Connection, project_id: &str) -> AppResult<Vec<DraftPage>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, state, plant_scene_id, target_window, payoff_contract, evidence_ids,
            payoff_state_change
         FROM promise WHERE project_id=?1 ORDER BY updated_at DESC",
    )?;
    let rows = stmt
        .query_map([project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    let mut pages = Vec::new();
    for (id, title, state, scene, window, contract, evidence, payoff_change) in rows {
        let mut page = DraftPage::new("promise", format!("伏笔 · {title}"), String::new());
        page.source(Some(&id), scene.as_deref());
        let mut body = format!(
            "## 状态\n{state}\n\n## 预期窗口\n{}\n\n## 兑现契约\n{}\n\n## 证据\n",
            empty_as(window, "未设定"),
            empty_as(contract, "尚未写清怎样才算真正兑现")
        );
        let evidence = json_list(&evidence);
        if evidence.is_empty() {
            body.push_str("- 尚无证据。\n");
        }
        for item in evidence {
            page.source(None, Some(&item));
            body.push_str(&format!("- {item}\n"));
        }
        if !payoff_change.trim().is_empty() {
            body.push_str(&format!("\n## 已记录的状态变化\n{}\n", payoff_change.trim()));
        }
        body.push_str("\n## 当前风险\n");
        body.push_str(if matches!(state.as_str(), "planted" | "reinforced" | "ready") {
            "- 仍形成叙事债务；到期后审计会提示。\n"
        } else {
            "- 当前未形成未结债务。\n"
        });
        page.content = format!(
            "派生视图 · 非权威事实。伏笔状态与契约仍以伏笔表为权威来源。\n\n{body}"
        );
        pages.push(page.finish());
    }
    Ok(pages)
}

fn build_chapters(
    conn: &Connection,
    project_id: &str,
    branch_id: Option<&str>,
) -> AppResult<Vec<DraftPage>> {
    let mut stmt = conn.prepare(
        "SELECT id, number, title, status, current_revision_id
         FROM chapter WHERE project_id=?1 ORDER BY number",
    )?;
    let chapters = stmt
        .query_map([project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i32>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    let mut pages = Vec::new();
    for (id, number, title, status, revision) in chapters {
        let mut page = DraftPage::new(
            "chapter",
            format!("第 {number} 章 · {title}"),
            String::new(),
        );
        page.source(Some(&id), None);
        let text = crate::scenes::chapter_text(conn, project_id, &id)?;
        page.source(revision.as_deref(), None);
        let mut body = format!(
            "## 状态\n{status}\n\n## 已提交正文摘要\n{}\n",
            summarize_text(&text, 520)
        );
        let mut stmt = conn.prepare(
            "SELECT id, title, outcome, source_span FROM event
             WHERE project_id=?1 AND status='confirmed'
               AND (branch_id IS NULL OR branch_id=?3)
               AND (source_span LIKE ?2 OR title LIKE ?4)
             ORDER BY story_time LIMIT 12",
        )?;
        let token = format!("%{id}%");
        let number_token = format!("%第{number}章%");
        let events = stmt
            .query_map(params![project_id, token, branch_id, number_token], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        body.push_str("\n## 事件与状态变化\n");
        if events.is_empty() {
            body.push_str("- 尚未关联结构化事件；正文摘要仍可作为阅读导航。\n");
        }
        for (event_id, event_title, outcome, span) in events {
            page.source(Some(&event_id), Some(&span));
            body.push_str(&format!("- {event_title}｜{outcome}\n"));
        }
        let mut stmt = conn.prepare(
            "SELECT id, title, state FROM promise
             WHERE project_id=?1 AND target_window LIKE ?2
               AND state NOT IN ('paid_off','abandoned')",
        )?;
        let open_promises = stmt
            .query_map(params![project_id, format!("%{number}%")], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        body.push_str("\n## 未解决问题\n");
        if open_promises.is_empty() {
            body.push_str("- 本章窗口内没有伏笔债务。\n");
        } else {
            for (promise_id, promise_title, state) in open_promises {
                page.source(Some(&promise_id), None);
                body.push_str(&format!("- [{state}] {promise_title}\n"));
            }
        }
        page.content = format!(
            "派生视图 · 非权威事实。章节摘要不能替代正文与原文证据。\n\n{body}"
        );
        pages.push(page.finish());
    }
    Ok(pages)
}

fn summarize_text(text: &str, max_chars: usize) -> String {
    let compact = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if compact.is_empty() {
        return "尚无已提交正文。".into();
    }
    let mut summary: String = compact.chars().take(max_chars).collect();
    if compact.chars().count() > max_chars {
        summary.push('…');
    }
    summary
}

fn empty_as(value: String, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value
    }
}

fn pretty_json(text: &str) -> String {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| serde_json::to_string_pretty(&value).ok())
        .unwrap_or_else(|| text.to_string())
}

pub fn rebuild(conn: &mut Connection, project_id: &str) -> AppResult<WikiRebuildResult> {
    let branch_id = crate::branches::current(conn, project_id)?;
    let mut drafts = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, type, canonical_name, aliases, description FROM entity
             WHERE project_id=?1 AND status='confirmed'
             ORDER BY CASE type WHEN 'character' THEN 0 WHEN 'place' THEN 1 ELSE 2 END, canonical_name",
        )?;
        let entities = stmt
            .query_map([project_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        for (id, entity_type, name, aliases, description) in entities {
            let page = match entity_type.as_str() {
                "character" => Some(build_character(
                    conn,
                    project_id,
                    branch_id.as_deref(),
                    &id,
                    &name,
                    &aliases,
                    &description,
                )?),
                "place" => Some(build_place(
                    conn,
                    project_id,
                    branch_id.as_deref(),
                    &id,
                    &name,
                    &aliases,
                    &description,
                )?),
                _ => None,
            };
            if let Some(page) = page {
                drafts.push(page);
            }
        }
    }
    {
        let mut stmt = conn.prepare(
            "SELECT id, code, description, predicate, exception_scope, constraint_layer, severity
             FROM rule WHERE project_id=?1 ORDER BY code",
        )?;
        let rules = stmt
            .query_map([project_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        for (id, code, description, predicate, exceptions, layer, severity) in rules {
            drafts.push(build_rule(
                conn,
                project_id,
                &id,
                &code,
                &description,
                &predicate,
                &exceptions,
                &layer,
                &severity,
            )?);
        }
    }
    drafts.push(build_timeline(conn, project_id, branch_id.as_deref())?);
    drafts.extend(build_promises(conn, project_id)?);
    drafts.extend(build_chapters(conn, project_id, branch_id.as_deref())?);

    let ts = now();
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM wiki_page WHERE project_id=?1", [project_id])?;
    let mut pages = Vec::new();
    for draft in drafts {
        let hash = crate::hash_text(&format!(
            "{}\n{}\n{}\n{}\n{}",
            draft.kind,
            draft.title,
            draft.content,
            serde_json::to_string(&draft.source_ids)?,
            serde_json::to_string(&draft.source_spans)?
        ));
        tx.execute(
            "INSERT INTO wiki_page (id, project_id, entity_id, kind, title, content, content_hash,
                source_ids, generated_at, stale_at, source_spans, generator_version, edited_by_author)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,NULL,?10,?11,0)",
            params![
                draft.id,
                project_id,
                draft.entity_id,
                draft.kind,
                draft.title,
                draft.content,
                hash,
                serde_json::to_string(&draft.source_ids)?,
                ts,
                serde_json::to_string(&draft.source_spans)?,
                GENERATOR_VERSION
            ],
        )?;
        pages.push(WikiPage {
            id: draft.id,
            entity_id: draft.entity_id,
            kind: draft.kind.into(),
            title: draft.title,
            content: draft.content,
            content_hash: hash,
            source_ids: draft.source_ids,
            source_spans: draft.source_spans,
            generator_version: GENERATOR_VERSION,
            edited_by_author: false,
            generated_at: ts.clone(),
            stale_at: None,
        });
    }
    tx.commit()?;
    pages.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.title.cmp(&b.title)));
    Ok(WikiRebuildResult {
        characters: pages.iter().filter(|page| page.kind == "character").count() as i32,
        places: pages.iter().filter(|page| page.kind == "place").count() as i32,
        rules: pages.iter().filter(|page| page.kind == "world_rule").count() as i32,
        timelines: pages.iter().filter(|page| page.kind == "timeline").count() as i32,
        promises: pages.iter().filter(|page| page.kind == "promise").count() as i32,
        chapters: pages.iter().filter(|page| page.kind == "chapter").count() as i32,
        pages,
    })
}

pub fn propose_edit(
    conn: &Connection,
    project_id: &str,
    input: &WikiEditInput,
) -> AppResult<Candidate> {
    let page = get(conn, project_id, &input.page_id)?;
    if input.title.trim().is_empty() || input.content.trim().is_empty() {
        return Err(AppError::Message("百科页标题和内容不能为空".into()));
    }
    crate::candidates::insert(
        conn,
        project_id,
        "wiki_edit",
        serde_json::json!({
            "pageId": page.id,
            "title": input.title.trim(),
            "content": input.content.trim(),
            "baseHash": page.content_hash,
        }),
        Some(&format!("author:wiki:{}", page.id)),
        None,
        1.0,
    )
}

pub fn apply_confirmed_edit(
    conn: &Connection,
    project_id: &str,
    candidate: &Candidate,
) -> AppResult<String> {
    let page_id = candidate
        .payload
        .get("pageId")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::Message("百科编辑候选缺少页面 ID".into()))?;
    let page = get(conn, project_id, page_id)?;
    let base_hash = candidate
        .payload
        .get("baseHash")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if page.content_hash != base_hash {
        return Err(AppError::Message(
            "百科页面已变化，请重新基于最新内容提出修改".into(),
        ));
    }
    let title = candidate
        .payload
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::Message("百科编辑候选缺少标题".into()))?;
    let content = candidate
        .payload
        .get("content")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::Message("百科编辑候选缺少内容".into()))?;
    let source_id = format!("author:wiki:{}", page.id);
    let mut source_ids = page.source_ids.clone();
    source_ids.push(source_id.clone());
    source_ids.sort();
    source_ids.dedup();
    let hash = crate::hash_text(&format!(
        "{}\n{}\n{}",
        page.kind,
        title.trim(),
        content.trim()
    ));
    conn.execute(
        "UPDATE wiki_page SET title=?3, content=?4, content_hash=?5, source_ids=?6,
            edited_by_author=1, stale_at=NULL WHERE project_id=?1 AND id=?2",
        params![
            project_id,
            page.id,
            title.trim(),
            content.trim(),
            hash,
            serde_json::to_string(&source_ids)?
        ],
    )?;
    Ok(page.id)
}

pub fn context_index(conn: &Connection, project_id: &str) -> AppResult<String> {
    let mut stmt = conn.prepare(
        "SELECT kind, title, stale_at, source_ids FROM wiki_page
         WHERE project_id=?1
         ORDER BY CASE kind
           WHEN 'character' THEN 0 WHEN 'place' THEN 1 WHEN 'world_rule' THEN 2
           WHEN 'timeline' THEN 3 WHEN 'promise' THEN 4 ELSE 5 END, title
         LIMIT 24",
    )?;
    let rows = stmt
        .query_map([project_id], |row| {
            let sources: String = row.get(3)?;
            let source_ids = json_list(&sources);
            Ok(format!(
                "- [{}] {}{}｜来源：{}",
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                if row.get::<_, Option<String>>(2)?.is_some() {
                    "（已过期）"
                } else {
                    ""
                },
                source_ids
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    if rows.is_empty() {
        Ok("尚未编译叙事百科。百科索引只作导航，不得替代原文证据。".into())
    } else {
        Ok(format!(
            "以下为派生导航索引，不是事实源：\n{}",
            rows.join("\n")
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::{candidates, domain::CreateProjectInput, project};

    use super::*;

    fn setup() -> (tempfile::TempDir, project::LoadedProject) {
        let dir = tempfile::tempdir().unwrap();
        let loaded = project::create(&CreateProjectInput {
            name: "wiki".into(),
            root_path: dir.path().join("story").to_string_lossy().into_owned(),
            mode: "both".into(),
            writing_language: None,
        })
        .unwrap();
        (dir, loaded)
    }

    #[test]
    fn rebuild_has_all_page_families_sources_hashes_and_stale_recovery() {
        let (_dir, mut loaded) = setup();
        let pid = loaded.summary.id.clone();
        loaded
            .conn
            .execute_batch(&format!(
                "INSERT INTO entity (id,project_id,type,canonical_name,aliases,status,description,created_at,updated_at)
                 VALUES ('lin','{pid}','character','林澈','[\"阿澈\"]','confirmed','主角','now','now'),
                        ('mom','{pid}','character','母亲','[]','confirmed','','now','now'),
                        ('station','{pid}','place','旧车站','[]','confirmed','','now','now');
                 INSERT INTO character_core (entity_id,desire,fear,voice_anchor,updated_at)
                 VALUES ('lin','找到寄信人','身份崩塌','“先把信给我。”','now');
                 INSERT INTO relation (id,project_id,from_id,to_id,type,status,created_at)
                 VALUES ('rel','{pid}','lin','mom','母子','confirmed','now');
                 INSERT INTO claim (id,project_id,subject,subject_id,predicate,object,truth_layer,source_span,status,version,created_at,updated_at)
                 VALUES ('claim','{pid}','林澈','lin','持有','旧钥匙','world','ch01:p3｜林澈握住旧钥匙','confirmed',1,'now','now');
                 INSERT INTO event (id,project_id,title,participants,place_id,story_time,reveal_time,source_span,status,created_at)
                 VALUES ('event','{pid}','钥匙现形','[\"林澈\"]','station','葬礼夜','第一章','ch01:p3｜钥匙现形','confirmed','now');
                 INSERT INTO chapter (id,project_id,number,title,status,style_epoch,created_at,updated_at)
                 VALUES ('ch1','{pid}',1,'雨夜','planned',1,'now','now');"
            ))
            .unwrap();
        let result = rebuild(&mut loaded.conn, &pid).unwrap();
        assert!(result.characters >= 2);
        assert!(result.places >= 1);
        assert!(result.rules >= 1);
        assert!(result.timelines == 1);
        assert!(result.chapters == 1);
        assert!(result.pages.iter().all(|page| !page.source_ids.is_empty()));
        assert!(result.pages.iter().all(|page| page.content_hash.len() == 64));
        assert!(result.pages.iter().all(|page| page.stale_at.is_none()));

        crate::graphrag::mark_stale(&loaded.conn, &pid).unwrap();
        assert!(list(&loaded.conn, &pid)
            .unwrap()
            .iter()
            .all(|page| page.stale_at.is_some()));
        let rebuilt = rebuild(&mut loaded.conn, &pid).unwrap();
        assert!(rebuilt.pages.iter().all(|page| page.stale_at.is_none()));
    }

    #[test]
    fn author_edit_requires_candidate_and_does_not_touch_facts() {
        let (_dir, mut loaded) = setup();
        let pid = loaded.summary.id.clone();
        loaded
            .conn
            .execute(
                "INSERT INTO entity (id,project_id,type,canonical_name,aliases,status,description,created_at,updated_at)
                 VALUES ('lin',?1,'character','林澈','[]','confirmed','主角','now','now')",
                [&pid],
            )
            .unwrap();
        let page = rebuild(&mut loaded.conn, &pid)
            .unwrap()
            .pages
            .into_iter()
            .find(|page| page.kind == "character")
            .unwrap();
        let candidate = propose_edit(
            &loaded.conn,
            &pid,
            &WikiEditInput {
                page_id: page.id.clone(),
                title: page.title.clone(),
                content: "作者确认后的派生页面".into(),
            },
        )
        .unwrap();
        assert_eq!(candidate.risk_level, "L2");
        candidates::accept(&mut loaded.conn, &pid, &candidate.id).unwrap();
        let updated = get(&loaded.conn, &pid, &page.id).unwrap();
        assert!(updated.content.contains("作者确认"));
        assert!(updated.edited_by_author);
        let fact_count: i64 = loaded
            .conn
            .query_row("SELECT COUNT(*) FROM claim WHERE project_id=?1", [&pid], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(fact_count, 0);
    }
}
