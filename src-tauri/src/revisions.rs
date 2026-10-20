use rusqlite::{params, Connection};

use crate::{
    domain::{DraftAcceptResult, RevisionCompare, RevisionRecord},
    error::{AppError, AppResult},
    scenes,
};

pub fn list(
    conn: &Connection,
    project_id: &str,
    chapter_id: Option<&str>,
) -> AppResult<Vec<RevisionRecord>> {
    let sql = if chapter_id.is_some() {
        "SELECT id,parent_id,chapter_id,diff,snapshot_id,author_action,created_at
         FROM revision WHERE project_id=?1 AND chapter_id=?2 ORDER BY created_at DESC"
    } else {
        "SELECT id,parent_id,chapter_id,diff,snapshot_id,author_action,created_at
         FROM revision WHERE project_id=?1 ORDER BY created_at DESC LIMIT 200"
    };
    let mut stmt = conn.prepare(sql)?;
    let map = |row: &rusqlite::Row<'_>| {
        let diff: String = row.get(3)?;
        Ok(RevisionRecord {
            id: row.get(0)?,
            parent_id: row.get(1)?,
            chapter_id: row.get(2)?,
            diff: serde_json::from_str(&diff).unwrap_or(serde_json::json!({})),
            snapshot_id: row.get(4)?,
            author_action: row.get(5)?,
            created_at: row.get(6)?,
        })
    };
    let rows = if let Some(chapter_id) = chapter_id {
        stmt.query_map(params![project_id, chapter_id], map)?
            .collect::<Result<Vec<_>, _>>()?
    } else {
        stmt.query_map([project_id], map)?
            .collect::<Result<Vec<_>, _>>()?
    };
    drop(stmt);
    Ok(rows)
}
fn snapshot_text(conn: &Connection, project_id: &str, revision_id: &str) -> AppResult<String> {
    conn.query_row(
        "SELECT s.content FROM revision r JOIN snapshot s ON s.id=r.snapshot_id
         WHERE r.project_id=?1 AND r.id=?2",
        params![project_id, revision_id],
        |row| row.get(0),
    )
    .map_err(|_| AppError::Message("找不到该版本的正文快照".into()))
}

pub fn compare(
    conn: &Connection,
    project_id: &str,
    from_id: &str,
    to_id: &str,
) -> AppResult<RevisionCompare> {
    let from_text = snapshot_text(conn, project_id, from_id)?;
    let to_text = snapshot_text(conn, project_id, to_id)?;
    let from_chars = from_text.chars().count() as i32;
    let to_chars = to_text.chars().count() as i32;
    let preview = format!(
        "版本 A：{} 字\n版本 B：{} 字\n字数变化：{:+}\n\n[A 开头]\n{}\n\n[B 开头]\n{}",
        from_chars,
        to_chars,
        to_chars - from_chars,
        from_text.chars().take(300).collect::<String>(),
        to_text.chars().take(300).collect::<String>()
    );
    Ok(RevisionCompare {
        from_text,
        to_text,
        from_chars,
        to_chars,
        preview,
    })
}

pub fn rollback(
    conn: &mut Connection,
    project_id: &str,
    revision_id: &str,
) -> AppResult<DraftAcceptResult> {
    let chapter_id: Option<String> = conn.query_row(
        "SELECT chapter_id FROM revision WHERE project_id=?1 AND id=?2",
        params![project_id, revision_id],
        |row| row.get(0),
    )?;
    let chapter_id =
        chapter_id.ok_or_else(|| AppError::Message("该 revision 没有可回滚的章节".into()))?;
    let text = snapshot_text(conn, project_id, revision_id)?;
    scenes::commit_text(
        conn,
        project_id,
        &chapter_id,
        &text,
        &format!("rollback:{revision_id}"),
    )
}
