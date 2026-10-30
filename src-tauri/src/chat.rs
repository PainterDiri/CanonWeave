use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::{json, Value};

use crate::{
    candidates,
    domain::{Candidate, ChatMessage, ChatSession, ChatSessionCreateInput},
    error::{AppError, AppResult},
    now,
};

fn session_from_row(row: &Row<'_>) -> rusqlite::Result<ChatSession> {
    Ok(ChatSession {
        id: row.get(0)?,
        scope: row.get(1)?,
        scene_id: row.get(2)?,
        branch_id: row.get(3)?,
        parent_session_id: row.get(4)?,
        mode: row.get(5)?,
        title: row.get(6)?,
        model: row.get(7)?,
        status: row.get(8)?,
        summary: row.get(9)?,
        last_message_at: row.get(10)?,
        created_at: row.get(11)?,
        pinned: row.get(12)?,
        archived_at: row.get(13)?,
        pending_candidates: row.get(14)?,
        message_count: row.get(15)?,
    })
}

const SESSION_SELECT: &str = "SELECT s.id, s.scope, s.scene_id, s.branch_id, s.parent_session_id,
    s.mode, s.title, s.model, s.status, s.summary, s.last_message_at, s.created_at, s.pinned,
    s.archived_at,
    (SELECT COUNT(*) FROM candidate c WHERE c.source_message_id IN
        (SELECT m.id FROM chat_message m WHERE m.session_id=s.id) AND c.status='pending'),
    (SELECT COUNT(*) FROM chat_message m WHERE m.session_id=s.id)
    FROM chat_session s";

fn message_from_row(row: &Row<'_>) -> rusqlite::Result<ChatMessage> {
    let linked: String = row.get(9)?;
    Ok(ChatMessage {
        id: row.get(0)?,
        session_id: row.get(1)?,
        seq: row.get(2)?,
        role: row.get(3)?,
        content: row.get(4)?,
        model: row.get(5)?,
        tokens: row.get(6)?,
        created_at: row.get(7)?,
        linked_candidate_ids: serde_json::from_str(&linked).unwrap_or_default(),
        pinned: row.get(10)?,
        agent_run_id: row.get(11)?,
    })
}

const MESSAGE_SELECT: &str = "SELECT id, session_id, seq, role, content, model, tokens,
    created_at, created_at, linked_candidate_ids, pinned, agent_run_id FROM chat_message";

pub fn create(
    conn: &Connection,
    project_id: &str,
    input: &ChatSessionCreateInput,
) -> AppResult<ChatSession> {
    let scopes = ["seed", "plot", "scene", "lookup", "audit"];
    if !scopes.contains(&input.scope.as_str()) {
        return Err(AppError::Message(format!("未知会话范围：{}", input.scope)));
    }
    let modes = ["diverge", "structure", "plot", "direct"];
    if !modes.contains(&input.mode.as_str()) {
        return Err(AppError::Message(format!("未知对话模式：{}", input.mode)));
    }
    if let Some(parent) = &input.parent_session_id {
        get(conn, project_id, parent)?;
    }
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now();
    let default_title = match input.scope.as_str() {
        "seed" => "灵感孵化",
        "plot" => "推进剧情",
        "scene" => "场景讨论",
        "lookup" => "查设定",
        _ => "审计解释",
    };
    conn.execute(
        "INSERT INTO chat_session (id, project_id, scope, scene_id, branch_id, parent_session_id,
            mode, title, status, created_at, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'active',?9,?9)",
        params![
            id,
            project_id,
            input.scope,
            input.scene_id,
            input.branch_id,
            input.parent_session_id,
            input.mode,
            input.title.as_deref().unwrap_or(default_title),
            ts
        ],
    )?;
    get(conn, project_id, &id)
}

pub fn get(conn: &Connection, project_id: &str, id: &str) -> AppResult<ChatSession> {
    conn.query_row(
        &format!("{SESSION_SELECT} WHERE s.project_id=?1 AND s.id=?2"),
        params![project_id, id],
        session_from_row,
    )
    .optional()?
    .ok_or_else(|| AppError::Message("找不到会话".into()))
}

pub fn list(
    conn: &Connection,
    project_id: &str,
    scope: Option<&str>,
    include_archived: bool,
) -> AppResult<Vec<ChatSession>> {
    let mut sql = format!("{SESSION_SELECT} WHERE s.project_id=?1");
    if !include_archived {
        sql.push_str(" AND s.archived_at IS NULL");
    }
    if scope.is_some() {
        sql.push_str(" AND s.scope=?2");
    }
    sql.push_str(" ORDER BY s.pinned DESC, COALESCE(s.last_message_at, s.created_at) DESC LIMIT 200");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = Vec::new();
    {
        let mut mapped = if let Some(scope) = scope {
            stmt.query_map(params![project_id, scope], session_from_row)?
        } else {
            stmt.query_map([project_id], session_from_row)?
        };
        for row in mapped.by_ref() {
            rows.push(row?);
        }
    }
    drop(stmt);
    Ok(rows)
}

pub fn messages(conn: &Connection, project_id: &str, session_id: &str) -> AppResult<Vec<ChatMessage>> {
    get(conn, project_id, session_id)?;
    let mut stmt = conn.prepare(&format!(
        "{MESSAGE_SELECT} WHERE session_id=?1 ORDER BY seq ASC"
    ))?;
    let rows = stmt
        .query_map([session_id], message_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    Ok(rows)
}

pub fn message(conn: &Connection, project_id: &str, message_id: &str) -> AppResult<ChatMessage> {
    let session_id: String = conn
        .query_row(
            "SELECT session_id FROM chat_message WHERE id=?1",
            [message_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::Message("找不到消息".into()))?;
    get(conn, project_id, &session_id)?;
    conn.query_row(
        &format!("{MESSAGE_SELECT} WHERE id=?1"),
        [message_id],
        message_from_row,
    )
    .map_err(Into::into)
}

pub fn append_message(
    conn: &Connection,
    project_id: &str,
    session_id: &str,
    role: &str,
    content: &str,
    model: &str,
) -> AppResult<ChatMessage> {
    let session = get(conn, project_id, session_id)?;
    let seq: i32 = conn.query_row(
        "SELECT COALESCE(MAX(seq), 0) + 1 FROM chat_message WHERE session_id=?1",
        [session_id],
        |row| row.get(0),
    )?;
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now();
    conn.execute(
        "INSERT INTO chat_message (id, session_id, seq, role, content, model, tokens, created_at,
            linked_candidate_ids, pinned) VALUES (?1,?2,?3,?4,?5,?6,0,?7,'[]',0)",
        params![id, session_id, seq, role, content, model, ts],
    )?;
    let mut title = session.title.clone();
    if role == "user"
        && seq == 1
        && (session.title.ends_with("孵化")
            || session.title.ends_with("剧情")
            || session.title.ends_with("讨论")
            || session.title.ends_with("设定")
            || session.title.ends_with("解释"))
    {
        title = content.chars().take(18).collect::<String>();
    }
    conn.execute(
        "UPDATE chat_session SET last_message_at=?2, updated_at=?2, title=?3 WHERE id=?1",
        params![session_id, ts, title],
    )?;
    message(conn, project_id, &id)
}

pub fn update(
    conn: &Connection,
    project_id: &str,
    id: &str,
    title: Option<&str>,
    pinned: Option<bool>,
    mode: Option<&str>,
    scope: Option<&str>,
) -> AppResult<ChatSession> {
    get(conn, project_id, id)?;
    if let Some(title) = title {
        if title.trim().is_empty() {
            return Err(AppError::Message("会话标题不能为空".into()));
        }
        conn.execute(
            "UPDATE chat_session SET title=?2, updated_at=?3 WHERE id=?1",
            params![id, title.trim(), now()],
        )?;
    }
    if let Some(pinned) = pinned {
        conn.execute(
            "UPDATE chat_session SET pinned=?2, updated_at=?3 WHERE id=?1",
            params![id, pinned as i32, now()],
        )?;
    }
    if let Some(mode) = mode {
        if !["diverge", "structure", "plot", "direct"].contains(&mode) {
            return Err(AppError::Message("未知对话模式".into()));
        }
        conn.execute(
            "UPDATE chat_session SET mode=?2, updated_at=?3 WHERE id=?1",
            params![id, mode, now()],
        )?;
    }
    if let Some(scope) = scope {
        if !["seed", "plot", "scene", "lookup", "audit"].contains(&scope) {
            return Err(AppError::Message("未知会话范围".into()));
        }
        conn.execute(
            "UPDATE chat_session SET scope=?2, updated_at=?3 WHERE id=?1",
            params![id, scope, now()],
        )?;
    }
    get(conn, project_id, id)
}

pub fn archive(conn: &Connection, project_id: &str, id: &str) -> AppResult<()> {
    get(conn, project_id, id)?;
    conn.execute(
        "UPDATE chat_session SET archived_at=?2, status='archived', updated_at=?2 WHERE id=?1",
        params![id, now()],
    )?;
    Ok(())
}

pub fn unarchive(conn: &Connection, project_id: &str, id: &str) -> AppResult<()> {
    get(conn, project_id, id)?;
    conn.execute(
        "UPDATE chat_session SET archived_at=NULL, status='active', updated_at=?2 WHERE id=?1",
        params![id, now()],
    )?;
    Ok(())
}

/// 删除会话只级联删除消息；候选与已确认事实保留（source_message_id 没有外键）。
pub fn delete(conn: &Connection, project_id: &str, id: &str) -> AppResult<()> {
    get(conn, project_id, id)?;
    conn.execute("DELETE FROM chat_session WHERE id=?1", [id])?;
    Ok(())
}

pub fn fork(conn: &mut Connection, project_id: &str, id: &str) -> AppResult<ChatSession> {
    let source = get(conn, project_id, id)?;
    let new_id = uuid::Uuid::new_v4().to_string();
    let ts = now();
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO chat_session (id, project_id, scope, scene_id, branch_id, parent_session_id,
            mode, title, model, status, summary, summary_hash, last_message_at, created_at, updated_at, pinned)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'active',?10,'',?11,?12,?12,0)",
        params![
            new_id,
            project_id,
            source.scope,
            source.scene_id,
            source.branch_id,
            source.id,
            source.mode,
            format!("{}（分叉）", source.title),
            source.model,
            source.summary,
            source.last_message_at,
            ts
        ],
    )?;
    let mut stmt = tx.prepare(
        "SELECT seq, role, content, model, tokens, created_at, linked_candidate_ids, pinned
         FROM chat_message WHERE session_id=?1 ORDER BY seq",
    )?;
    let rows: Vec<(i32, String, String, String, i32, String, String, i32)> = stmt
        .query_map([id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
            ))
        })?
        .collect::<Result<_, _>>()?;
    drop(stmt);
    for (seq, role, content, model, tokens, created_at, linked, pinned) in rows {
        tx.execute(
            "INSERT INTO chat_message (id, session_id, seq, role, content, model, tokens, created_at,
                linked_candidate_ids, pinned) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                uuid::Uuid::new_v4().to_string(),
                new_id,
                seq,
                role,
                content,
                model,
                tokens,
                created_at,
                linked,
                pinned
            ],
        )?;
    }
    tx.commit()?;
    get(conn, project_id, &new_id)
}

pub fn set_summary(
    conn: &Connection,
    project_id: &str,
    id: &str,
    summary: &str,
) -> AppResult<ChatSession> {
    get(conn, project_id, id)?;
    let hash = crate::hash_text(summary);
    conn.execute(
        "UPDATE chat_session SET summary=?2, summary_hash=?3, updated_at=?4 WHERE id=?1",
        params![id, summary, hash, now()],
    )?;
    get(conn, project_id, id)
}

pub fn history_window(
    conn: &Connection,
    project_id: &str,
    id: &str,
    limit: usize,
) -> AppResult<Vec<ChatMessage>> {
    let mut all = messages(conn, project_id, id)?;
    let pins: Vec<_> = all.iter().filter(|m| m.pinned).cloned().collect();
    if all.len() > limit {
        all = all.split_off(all.len() - limit);
    }
    let mut merged = pins;
    for message in all {
        if !merged.iter().any(|m| m.id == message.id) {
            merged.push(message);
        }
    }
    merged.sort_by_key(|m| m.seq);
    Ok(merged)
}

pub fn capture(
    conn: &Connection,
    project_id: &str,
    session_id: &str,
    message_id: &str,
    kind: &str,
    text: &str,
) -> AppResult<Candidate> {
    let message = message(conn, project_id, message_id)?;
    if message.session_id != session_id {
        return Err(AppError::Message("消息不属于该会话".into()));
    }
    let text = text.trim();
    if text.is_empty() {
        return Err(AppError::Message("采纳内容不能为空".into()));
    }
    let source_span = format!("chat:{session_id}:{message_id}");
    let (candidate_kind, payload): (&str, Value) = match kind {
        "character" => (
            "entity",
            json!({
                "type": "character",
                "canonicalName": text.lines().next().unwrap_or(text).chars().take(20).collect::<String>(),
                "aliases": [],
                "description": text
            }),
        ),
        "place" => (
            "entity",
            json!({
                "type": "place",
                "canonicalName": text.lines().next().unwrap_or(text).chars().take(20).collect::<String>(),
                "aliases": [],
                "description": text
            }),
        ),
        "rule" => ("rule", json!({"text": text})),
        "promise" => (
            "promise",
            json!({
                "title": text.chars().take(30).collect::<String>(),
                "contract": text,
                "window": ""
            }),
        ),
        "style_sample" => ("style_sample", json!({"text": text})),
        "scene_goal" => ("scene_goal", json!({"text": text})),
        "setting" => ("entity", json!({"type": "concept", "canonicalName": text.chars().take(18).collect::<String>(), "aliases": [], "description": text})),
        other => return Err(AppError::Message(format!("未知采纳类型：{other}"))),
    };
    let candidate = candidates::insert(
        conn,
        project_id,
        candidate_kind,
        payload,
        Some(&source_span),
        Some(message_id),
        0.7,
    )?;
    let mut linked = message.linked_candidate_ids;
    linked.push(candidate.id.clone());
    conn.execute(
        "UPDATE chat_message SET linked_candidate_ids=?2 WHERE id=?1",
        params![message_id, serde_json::to_string(&linked)?],
    )?;
    Ok(candidate)
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
            mode: "both".into(),
            writing_language: None,
        })
        .unwrap();
        (dir, loaded)
    }

    fn session(conn: &Connection, pid: &str) -> ChatSession {
        create(
            conn,
            pid,
            &ChatSessionCreateInput {
                scope: "seed".into(),
                mode: "diverge".into(),
                title: None,
                scene_id: None,
                branch_id: None,
                parent_session_id: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn fork_copies_messages_but_delete_keeps_candidates() {
        let (_dir, mut loaded) = setup();
        let pid = loaded.summary.id.clone();
        let s = session(&loaded.conn, &pid);
        let m = append_message(&loaded.conn, &pid, &s.id, "user", "雨夜里的旧信", "").unwrap();
        capture(&loaded.conn, &pid, &s.id, &m.id, "promise", "旧信是谁寄的").unwrap();
        let forked = fork(&mut loaded.conn, &pid, &s.id).unwrap();
        assert_eq!(messages(&loaded.conn, &pid, &forked.id).unwrap().len(), 1);
        delete(&loaded.conn, &pid, &s.id).unwrap();
        let candidates = candidates::list(&loaded.conn, &pid, None, None).unwrap();
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].source_message_id.as_deref() == Some(m.id.as_str()));
    }
}
