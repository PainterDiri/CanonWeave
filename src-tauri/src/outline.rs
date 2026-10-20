use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::Value;

use crate::{
    domain::{OutlineNode, OutlineUpdateInput},
    error::{AppError, AppResult},
    now,
};

fn node_from_row(row: &Row<'_>) -> rusqlite::Result<OutlineNode> {
    Ok(OutlineNode {
        id: row.get(0)?,
        parent_id: row.get(1)?,
        level: row.get(2)?,
        order_no: row.get(3)?,
        title: row.get(4)?,
        goal: row.get(5)?,
        conflict: row.get(6)?,
        turn: row.get(7)?,
        payoff: row.get(8)?,
        status: row.get(9)?,
        branch_id: row.get(10)?,
        source: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

const SELECT: &str = "SELECT id, parent_id, level, order_no, title, goal, conflict, turn,
    payoff, status, branch_id, source, created_at, updated_at FROM outline_node";

pub fn list(conn: &Connection, project_id: &str) -> AppResult<Vec<OutlineNode>> {
    let mut stmt = conn.prepare(&format!(
        "{SELECT} WHERE project_id=?1 AND status != 'dropped' ORDER BY order_no"
    ))?;
    let rows = stmt
        .query_map([project_id], node_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    Ok(rows)
}

pub fn get(conn: &Connection, project_id: &str, id: &str) -> AppResult<OutlineNode> {
    conn.query_row(
        &format!("{SELECT} WHERE project_id=?1 AND id=?2"),
        params![project_id, id],
        node_from_row,
    )
    .optional()?
    .ok_or_else(|| AppError::Message("找不到骨架节点".into()))
}

pub fn has_skeleton(conn: &Connection, project_id: &str) -> AppResult<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM outline_node WHERE project_id=?1 AND level='chapter' AND status != 'dropped')",
        [project_id],
        |row| row.get(0),
    )?)
}

pub fn insert_generated(
    conn: &mut Connection,
    project_id: &str,
    items: &[Value],
    branch_id: Option<&str>,
) -> AppResult<Vec<OutlineNode>> {
    if items.is_empty() {
        return Err(AppError::Message("模型没有返回章节骨架".into()));
    }
    let tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM outline_node WHERE project_id=?1 AND source='ai' AND status='candidate'",
        [project_id],
    )?;
    let ts = now();
    let mut ids = Vec::new();
    for (index, item) in items.iter().take(20).enumerate() {
        let order = item
            .get("order")
            .and_then(Value::as_i64)
            .unwrap_or((index + 1) as i64) as i32;
        let title = item
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("未命名章节");
        let id = uuid::Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO outline_node (id, project_id, level, order_no, title, goal, conflict,
                turn, payoff, status, branch_id, source, created_at, updated_at)
             VALUES (?1,?2,'chapter',?3,?4,?5,?6,?7,?8,'candidate',?9,'ai',?10,?10)",
            params![
                id,
                project_id,
                order,
                title,
                item.get("goal").and_then(Value::as_str).unwrap_or_default(),
                item.get("conflict").and_then(Value::as_str).unwrap_or_default(),
                item.get("turn").and_then(Value::as_str).unwrap_or_default(),
                item.get("payoff").and_then(Value::as_str).unwrap_or_default(),
                branch_id,
                ts
            ],
        )?;
        ids.push(id);
    }
    tx.commit()?;
    ids.iter()
        .map(|id| get(conn, project_id, id))
        .collect::<AppResult<Vec<_>>>()
}

pub fn update(
    conn: &Connection,
    project_id: &str,
    input: &OutlineUpdateInput,
) -> AppResult<OutlineNode> {
    if input.title.trim().is_empty() {
        return Err(AppError::Message("章节标题不能为空".into()));
    }
    get(conn, project_id, &input.id)?;
    conn.execute(
        "UPDATE outline_node SET title=?3, goal=?4, conflict=?5, turn=?6, payoff=?7,
            status=CASE WHEN status='candidate' THEN 'candidate' ELSE status END,
            source='author', updated_at=?8
         WHERE project_id=?1 AND id=?2",
        params![
            project_id,
            input.id,
            input.title.trim(),
            input.goal.trim(),
            input.conflict.trim(),
            input.turn.trim(),
            input.payoff.trim(),
            now()
        ],
    )?;
    get(conn, project_id, &input.id)
}

/// 确认骨架节点，并同步建立章节行；重复确认不会产生重复章节。
pub fn confirm(conn: &mut Connection, project_id: &str, ids: &[String]) -> AppResult<Vec<OutlineNode>> {
    if ids.is_empty() {
        return Err(AppError::Message("请选择要确认的章节".into()));
    }
    let tx = conn.transaction()?;
    let ts = now();
    for id in ids {
        let node: OutlineNode = tx
            .query_row(
                &format!("{SELECT} WHERE project_id=?1 AND id=?2"),
                params![project_id, id],
                node_from_row,
            )
            .optional()?
            .ok_or_else(|| AppError::Message("找不到骨架节点".into()))?;
        if node.status == "dropped" {
            continue;
        }
        tx.execute(
            "UPDATE outline_node SET status='confirmed', updated_at=?3 WHERE project_id=?1 AND id=?2",
            params![project_id, id, ts],
        )?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM chapter WHERE project_id=?1 AND number=?2)",
            params![project_id, node.order_no],
            |row| row.get(0),
        )?;
        if !exists {
            tx.execute(
                "INSERT INTO chapter (id, project_id, number, title, status, style_epoch, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,'planned',1,?5,?5)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    project_id,
                    node.order_no,
                    node.title,
                    ts
                ],
            )?;
        } else {
            tx.execute(
                "UPDATE chapter SET title=?3, updated_at=?4 WHERE project_id=?1 AND number=?2 AND status='planned'",
                params![project_id, node.order_no, node.title, ts],
            )?;
        }
    }
    tx.commit()?;
    ids.iter()
        .map(|id| get(conn, project_id, id))
        .collect::<AppResult<Vec<_>>>()
}

pub fn drop_node(conn: &Connection, project_id: &str, id: &str) -> AppResult<()> {
    get(conn, project_id, id)?;
    conn.execute(
        "UPDATE outline_node SET status='dropped', updated_at=?3 WHERE project_id=?1 AND id=?2",
        params![project_id, id, now()],
    )?;
    Ok(())
}

pub fn confirmed_count(conn: &Connection, project_id: &str) -> AppResult<i32> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM outline_node WHERE project_id=?1 AND level='chapter' AND status='confirmed'",
        [project_id],
        |row| row.get(0),
    )?)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::project;

    use super::*;

    #[test]
    fn confirm_creates_chapters_once() {
        let dir = tempfile::tempdir().unwrap();
        let mut loaded = project::create(&crate::domain::CreateProjectInput {
            name: "t".into(),
            root_path: dir.path().join("p").to_string_lossy().into_owned(),
            mode: "idea".into(),
            writing_language: None,
        })
        .unwrap();
        let pid = loaded.summary.id.clone();
        let nodes = insert_generated(
            &mut loaded.conn,
            &pid,
            &[
                json!({"order":1,"title":"雨夜","goal":"拿到信","conflict":"守夜人","turn":"信是空的","payoff":"旧钥匙"}),
                json!({"order":2,"title":"旧钥匙","goal":"开门","conflict":"父亲","turn":"父亲活着","payoff":"身份"}),
            ],
            None,
        )
        .unwrap();
        assert_eq!(nodes.len(), 2);
        confirm(
            &mut loaded.conn,
            &pid,
            &nodes.iter().map(|n| n.id.clone()).collect::<Vec<_>>(),
        )
        .unwrap();
        confirm(&mut loaded.conn, &pid, &[nodes[0].id.clone()]).unwrap();
        let chapters: i64 = loaded
            .conn
            .query_row("SELECT COUNT(*) FROM chapter", [], |r| r.get(0))
            .unwrap();
        assert_eq!(chapters, 2);
        assert_eq!(confirmed_count(&loaded.conn, &pid).unwrap(), 2);
    }
}
