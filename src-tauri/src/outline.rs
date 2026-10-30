use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension, Row};
#[cfg(test)]
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
        logic_node_ids: serde_json::from_str(&row.get::<_, String>(9)?).unwrap_or_default(),
        presentation_mode: row.get(10)?,
        narrative_function: row.get(11)?,
        status: row.get(12)?,
        branch_id: row.get(13)?,
        source: row.get(14)?,
        created_at: row.get(15)?,
        updated_at: row.get(16)?,
        value_shift: row.get(17)?,
        crisis: row.get(18)?,
        hook: row.get(19)?,
        reveal: row.get(20)?,
    })
}

const SELECT: &str = "SELECT id, parent_id, level, order_no, title, goal, conflict, turn,
    payoff, logic_node_ids, presentation_mode, narrative_function, status, branch_id,
    source, created_at, updated_at, value_shift, crisis, hook, reveal FROM outline_node";

pub fn list(conn: &Connection, project_id: &str) -> AppResult<Vec<OutlineNode>> {
    let mut stmt = conn.prepare(&format!(
        "{SELECT} WHERE project_id=?1 AND status != 'dropped'"
    ))?;
    let mut rows = stmt
        .query_map([project_id], node_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    let volume_order: HashMap<String, i32> = rows
        .iter()
        .filter(|node| node.level == "volume")
        .map(|node| (node.id.clone(), node.order_no))
        .collect();
    rows.sort_by_key(|node| {
        let parent_order = node
            .parent_id
            .as_ref()
            .and_then(|id| volume_order.get(id))
            .copied()
            .unwrap_or(node.order_no);
        let rank = if node.level == "volume" { 0 } else { 1 };
        (parent_order, rank, node.order_no)
    });
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

/// 章节工艺卡：Story Grid 式价值转变 + 危机抉择 + 章末钩子 + 读者认知揭示。
#[derive(Debug, Clone, Default)]
pub struct ChapterCraft {
    pub value_shift: String,
    pub crisis: String,
    pub hook: String,
    pub reveal: String,
}

#[derive(Debug, Clone)]
pub struct OutlineChapterDraft {
    pub number: i32,
    pub volume_index: Option<usize>,
    pub title: String,
    pub goal: String,
    pub conflict: String,
    pub turn: String,
    pub payoff: String,
    pub logic_node_ids: Vec<String>,
    pub presentation_mode: String,
    pub narrative_function: String,
    pub craft: ChapterCraft,
}

#[derive(Debug, Clone)]
pub struct OutlineVolumeDraft {
    pub title: String,
    pub goal: String,
    pub conflict: String,
    pub turn: String,
    pub payoff: String,
}

#[derive(Debug, Clone)]
pub struct OutlinePlanDraft {
    pub mode: String,
    pub total_chapters: i32,
    pub volumes: Vec<OutlineVolumeDraft>,
    pub chapters: Vec<OutlineChapterDraft>,
}

/// 保存作者定义的整体结构。已确认骨架不在这里覆盖，避免静默改写正在创作的故事。
pub fn insert_plan(
    conn: &mut Connection,
    project_id: &str,
    plan: &OutlinePlanDraft,
    branch_id: Option<&str>,
) -> AppResult<Vec<OutlineNode>> {
    if plan.chapters.is_empty() {
        return Err(AppError::Message("骨架不能为空".into()));
    }
    let confirmed: bool = conn.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM outline_node
           WHERE project_id=?1 AND level='chapter' AND status='confirmed' AND source!='import'
         )",
        [project_id],
        |row| row.get(0),
    )?;
    if confirmed {
        return Err(AppError::Message(
            "已有已确认的卷章骨架。请直接编辑现有骨架，或在新分支中建立另一套结构。".into(),
        ));
    }
    let tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM outline_node WHERE project_id=?1 AND source='ai' AND status='candidate'",
        [project_id],
    )?;
    let ts = now();
    let mut volume_ids = Vec::new();
    for (index, volume) in plan.volumes.iter().enumerate() {
        let id = uuid::Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO outline_node
             (id, project_id, parent_id, level, order_no, title, goal, conflict, turn, payoff,
              status, branch_id, source, created_at, updated_at)
             VALUES (?1,?2,NULL,'volume',?3,?4,?5,?6,?7,?8,'candidate',?9,'ai',?10,?10)",
            params![
                id,
                project_id,
                (index + 1) as i32,
                volume.title,
                volume.goal,
                volume.conflict,
                volume.turn,
                volume.payoff,
                branch_id,
                ts
            ],
        )?;
        volume_ids.push(id);
    }
    for chapter in &plan.chapters {
        let parent_id = chapter
            .volume_index
            .and_then(|index| volume_ids.get(index))
            .cloned();
        tx.execute(
            "INSERT INTO outline_node
             (id, project_id, parent_id, level, order_no, title, goal, conflict, turn, payoff,
              status, branch_id, source, logic_node_ids, presentation_mode,
              narrative_function, created_at, updated_at, value_shift, crisis, hook, reveal)
             VALUES (?1,?2,?3,'chapter',?4,?5,?6,?7,?8,?9,'candidate',?10,'ai',?11,?12,?13,?14,?14,
                     ?15,?16,?17,?18)",
            params![
                uuid::Uuid::new_v4().to_string(),
                project_id,
                parent_id,
                chapter.number,
                chapter.title,
                chapter.goal,
                chapter.conflict,
                chapter.turn,
                chapter.payoff,
                branch_id,
                serde_json::to_string(&chapter.logic_node_ids).unwrap_or_else(|_| "[]".into()),
                chapter.presentation_mode,
                chapter.narrative_function,
                ts,
                chapter.craft.value_shift,
                chapter.craft.crisis,
                chapter.craft.hook,
                chapter.craft.reveal
            ],
        )?;
    }
    tx.execute(
        "INSERT INTO story_structure (project_id, mode, total_chapters, spec, updated_at)
         VALUES (?1,?2,?3,?4,?5)
         ON CONFLICT(project_id) DO UPDATE SET mode=excluded.mode,
           total_chapters=excluded.total_chapters, spec=excluded.spec, updated_at=excluded.updated_at",
        params![
            project_id,
            plan.mode,
            plan.total_chapters,
            serde_json::json!({
                "volumes": plan.volumes.iter().map(|volume| {
                    serde_json::json!({"title": volume.title})
                }).collect::<Vec<_>>()
            })
            .to_string(),
            ts
        ],
    )?;
    tx.commit()?;
    list(conn, project_id)
}

#[cfg(test)]
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
                item.get("conflict")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                item.get("turn").and_then(Value::as_str).unwrap_or_default(),
                item.get("payoff")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
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
    let current = get(conn, project_id, &input.id)?;
    let keep = |value: &Option<String>, fallback: &str| {
        value
            .as_deref()
            .map(str::trim)
            .unwrap_or(fallback)
            .to_string()
    };
    conn.execute(
        "UPDATE outline_node SET title=?3, goal=?4, conflict=?5, turn=?6, payoff=?7,
            value_shift=?9, crisis=?10, hook=?11, reveal=?12,
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
            now(),
            keep(&input.value_shift, &current.value_shift),
            keep(&input.crisis, &current.crisis),
            keep(&input.hook, &current.hook),
            keep(&input.reveal, &current.reveal)
        ],
    )?;
    get(conn, project_id, &input.id)
}

/// 确认骨架节点，并同步建立章节行；重复确认不会产生重复章节。
pub fn confirm(
    conn: &mut Connection,
    project_id: &str,
    ids: &[String],
) -> AppResult<Vec<OutlineNode>> {
    if ids.is_empty() {
        return Err(AppError::Message("请选择要确认的章节".into()));
    }
    let tx = conn.transaction()?;
    let ts = now();
    let mut target_ids = HashSet::new();
    for id in ids {
        let node: OutlineNode = tx
            .query_row(
                &format!("{SELECT} WHERE project_id=?1 AND id=?2"),
                params![project_id, id],
                node_from_row,
            )
            .optional()?
            .ok_or_else(|| AppError::Message("找不到骨架节点".into()))?;
        if node.level == "volume" {
            target_ids.insert(node.id.clone());
            let mut stmt = tx.prepare(
                "SELECT id FROM outline_node WHERE project_id=?1 AND parent_id=?2 AND status!='dropped'",
            )?;
            for child in
                stmt.query_map(params![project_id, node.id], |row| row.get::<_, String>(0))?
            {
                target_ids.insert(child?);
            }
        } else {
            if let Some(parent_id) = node.parent_id.clone() {
                target_ids.insert(parent_id);
            }
            target_ids.insert(node.id.clone());
        }
    }
    let mut ordered = target_ids.into_iter().collect::<Vec<_>>();
    ordered.sort_by_key(|id| {
        get(&tx, project_id, id)
            .map(|node| {
                let parent = node
                    .parent_id
                    .as_ref()
                    .and_then(|parent_id| get(&tx, project_id, parent_id).ok())
                    .map(|parent| parent.order_no)
                    .unwrap_or(node.order_no);
                (
                    parent,
                    if node.level == "volume" { 0 } else { 1 },
                    node.order_no,
                )
            })
            .unwrap_or((i32::MAX, 2, i32::MAX))
    });
    for id in &ordered {
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
        if node.level == "volume" {
            continue;
        }
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM chapter WHERE project_id=?1 AND number=?2)",
            params![project_id, node.order_no],
            |row| row.get(0),
        )?;
        if !exists {
            tx.execute(
                "INSERT INTO chapter
                 (id, project_id, number, volume_id, title, status, style_epoch, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,'planned',1,?6,?6)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    project_id,
                    node.order_no,
                    node.parent_id,
                    node.title,
                    ts
                ],
            )?;
        } else {
            tx.execute(
                "UPDATE chapter SET title=?3, volume_id=?4, updated_at=?5
                 WHERE project_id=?1 AND number=?2 AND status='planned'",
                params![project_id, node.order_no, node.title, node.parent_id, ts],
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

    #[test]
    fn volume_plan_confirms_nested_chapters_and_keeps_volume_id() {
        let dir = tempfile::tempdir().unwrap();
        let mut loaded = project::create(&crate::domain::CreateProjectInput {
            name: "t".into(),
            root_path: dir.path().join("p").to_string_lossy().into_owned(),
            mode: "idea".into(),
            writing_language: None,
        })
        .unwrap();
        let pid = loaded.summary.id.clone();
        let plan = OutlinePlanDraft {
            mode: "volumes".into(),
            total_chapters: 3,
            volumes: vec![
                OutlineVolumeDraft {
                    title: "第一卷".into(),
                    goal: "进入禁墟".into(),
                    conflict: "残缺契约".into(),
                    turn: "禁墟主动回应".into(),
                    payoff: "留下神道徽记".into(),
                },
                OutlineVolumeDraft {
                    title: "第二卷".into(),
                    goal: "追查神道".into(),
                    conflict: "旧盟友背叛".into(),
                    turn: "灾厄并非敌人".into(),
                    payoff: "开启下一次灾厄潮".into(),
                },
            ],
            chapters: vec![
                OutlineChapterDraft {
                    number: 1,
                    volume_index: Some(0),
                    title: "雨墟".into(),
                    goal: "找到入口".into(),
                    conflict: "雨势吞没道路".into(),
                    turn: "入口在灯下".into(),
                    payoff: "旧灯".into(),
                    logic_node_ids: vec![],
                    presentation_mode: "prose".into(),
                    narrative_function: "".into(),
                    craft: Default::default(),
                },
                OutlineChapterDraft {
                    number: 2,
                    volume_index: Some(0),
                    title: "无字契".into(),
                    goal: "读懂契约".into(),
                    conflict: "文字会消失".into(),
                    turn: "契约认主".into(),
                    payoff: "禁墟核心".into(),
                    logic_node_ids: vec![],
                    presentation_mode: "prose".into(),
                    narrative_function: "".into(),
                    craft: Default::default(),
                },
                OutlineChapterDraft {
                    number: 3,
                    volume_index: Some(1),
                    title: "神道回声".into(),
                    goal: "确认神道来源".into(),
                    conflict: "记忆被篡改".into(),
                    turn: "灾厄曾是守门人".into(),
                    payoff: "旧盟友".into(),
                    logic_node_ids: vec![],
                    presentation_mode: "prose".into(),
                    narrative_function: "".into(),
                    craft: Default::default(),
                },
            ],
        };
        let nodes = insert_plan(&mut loaded.conn, &pid, &plan, None).unwrap();
        let first_volume = nodes.iter().find(|node| node.title == "第一卷").unwrap();
        confirm(&mut loaded.conn, &pid, &[first_volume.id.clone()]).unwrap();
        let chapters = crate::scenes::chapters(&loaded.conn, &pid).unwrap();
        assert_eq!(chapters.len(), 2);
        assert!(chapters
            .iter()
            .all(|chapter| chapter.volume_title.as_deref() == Some("第一卷")));
        assert_eq!(confirmed_count(&loaded.conn, &pid).unwrap(), 2);
    }
}
