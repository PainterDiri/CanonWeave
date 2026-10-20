use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::{
    domain::{BranchRecord, TimelineItem},
    error::{AppError, AppResult},
    now,
};

fn from_row(row: &Row<'_>, current: &str) -> rusqlite::Result<BranchRecord> {
    let id: String = row.get(0)?;
    Ok(BranchRecord {
        is_current: id == current,
        id,
        name: row.get(1)?,
        parent_branch_id: row.get(2)?,
        status: row.get(3)?,
        created_at: row.get(4)?,
    })
}

pub fn current(conn: &Connection, project_id: &str) -> AppResult<Option<String>> {
    Ok(conn.query_row(
        "SELECT current_branch_id FROM project WHERE id=?1",
        [project_id],
        |row| row.get(0),
    )?)
}

pub fn list(conn: &Connection, project_id: &str) -> AppResult<Vec<BranchRecord>> {
    let current = current(conn, project_id)?.unwrap_or_default();
    let mut stmt = conn.prepare(
        "SELECT id, name, parent_branch_id, status, created_at FROM branch
         WHERE project_id=?1 ORDER BY created_at",
    )?;
    let rows = stmt
        .query_map([project_id], |row| from_row(row, &current))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    Ok(rows)
}

pub fn create(
    conn: &Connection,
    project_id: &str,
    name: &str,
    from_branch_id: Option<&str>,
) -> AppResult<BranchRecord> {
    if name.trim().is_empty() {
        return Err(AppError::Message("分支名称不能为空".into()));
    }
    let from = from_branch_id
        .map(str::to_string)
        .or_else(|| current(conn, project_id).ok().flatten());
    if let Some(parent) = &from {
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM branch WHERE project_id=?1 AND id=?2)",
            params![project_id, parent],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(AppError::Message("源分支不存在".into()));
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now();
    conn.execute(
        "INSERT INTO branch (id, project_id, name, parent_branch_id, status, created_at)
         VALUES (?1,?2,?3,?4,'active',?5)",
        params![id, project_id, name.trim(), from, ts],
    )?;
    switch(conn, project_id, &id)?;
    get(conn, project_id, &id)
}

pub fn switch(conn: &Connection, project_id: &str, id: &str) -> AppResult<()> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM branch WHERE project_id=?1 AND id=?2 AND status='active')",
        params![project_id, id],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(AppError::Message("分支不存在或已关闭".into()));
    }
    conn.execute(
        "UPDATE project SET current_branch_id=?2, updated_at=?3 WHERE id=?1",
        params![project_id, id, now()],
    )?;
    conn.execute(
        "INSERT INTO audit_log (id, project_id, action, subject, created_at)
         VALUES (?1,?2,'branch_switch',?3,?4)",
        params![uuid::Uuid::new_v4().to_string(), project_id, id, now()],
    )?;
    Ok(())
}

pub fn get(conn: &Connection, project_id: &str, id: &str) -> AppResult<BranchRecord> {
    let current = current(conn, project_id)?.unwrap_or_default();
    conn.query_row(
        "SELECT id, name, parent_branch_id, status, created_at FROM branch
         WHERE project_id=?1 AND id=?2",
        params![project_id, id],
        |row| from_row(row, &current),
    )
    .optional()?
    .ok_or_else(|| AppError::Message("找不到分支".into()))
}

pub fn timeline(
    conn: &Connection,
    project_id: &str,
    branch_id: Option<&str>,
) -> AppResult<Vec<TimelineItem>> {
    let branch = branch_id
        .map(str::to_string)
        .or_else(|| current(conn, project_id).ok().flatten());
    let mut items = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, title, story_time, reveal_time, branch_id, source_span, cause, outcome
             FROM event WHERE project_id=?1 AND status='confirmed'
               AND (branch_id IS NULL OR branch_id=?2)",
        )?;
        let rows = stmt.query_map(params![project_id, branch], |row| {
            Ok(TimelineItem {
                id: row.get(0)?,
                kind: "event".into(),
                label: row.get(1)?,
                story_time: row.get(2)?,
                reveal_time: row.get(3)?,
                branch_id: row.get(4)?,
                source: row.get(5)?,
                detail: format!("{} / {}", row.get::<_, String>(6)?, row.get::<_, String>(7)?),
            })
        })?;
        for row in rows {
            items.push(row?);
        }
    }
    {
        let mut stmt = conn.prepare(
            "SELECT id, subject, predicate, object, COALESCE(valid_from,''), branch_id, source_span
             FROM claim WHERE project_id=?1 AND status='confirmed'
               AND (branch_id IS NULL OR branch_id=?2)",
        )?;
        let rows = stmt.query_map(params![project_id, branch], |row| {
            Ok(TimelineItem {
                id: row.get(0)?,
                kind: "claim".into(),
                label: format!(
                    "{} {} {}",
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?
                ),
                story_time: row.get(4)?,
                reveal_time: String::new(),
                branch_id: row.get(5)?,
                source: row.get(6)?,
                detail: String::new(),
            })
        })?;
        for row in rows {
            items.push(row?);
        }
    }
    items.sort_by(|a, b| a.story_time.cmp(&b.story_time));
    Ok(items)
}

#[cfg(test)]
mod tests {
    use rusqlite::params;

    use crate::project;

    use super::*;

    #[test]
    fn branch_switch_changes_timeline_visibility() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = project::create(&crate::domain::CreateProjectInput {
            name: "t".into(),
            root_path: dir.path().join("p").to_string_lossy().into_owned(),
            mode: "idea".into(),
            writing_language: None,
        })
        .unwrap();
        let pid = loaded.summary.id.clone();
        let main = current(&loaded.conn, &pid).unwrap().unwrap();
        loaded.conn.execute(
            "INSERT INTO claim (id,project_id,subject,predicate,object,truth_layer,source_span,
                status,version,branch_id,created_at,updated_at)
             VALUES ('shared',?1,'林澈','持有','钥匙','world','ch1','confirmed',1,NULL,'now','now'),
                    ('main-only',?1,'父亲','死亡','','world','ch2','confirmed',1,?2,'now','now')",
            params![pid, main],
        ).unwrap();
        let branch = create(&loaded.conn, &pid, "父亲仍活着", Some(&main)).unwrap();
        let branch_items = timeline(&loaded.conn, &pid, Some(&branch.id)).unwrap();
        assert_eq!(branch_items.len(), 1);
        assert_eq!(branch_items[0].id, "shared");
        switch(&loaded.conn, &pid, &main).unwrap();
        let main_items = timeline(&loaded.conn, &pid, Some(&main)).unwrap();
        assert_eq!(main_items.len(), 2);
    }
}
