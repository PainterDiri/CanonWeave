use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::{
    domain::{
        ChapterRecord, DraftAcceptResult, EmotionBeat, SceneInput, SceneRecord, SensoryPlan,
    },
    error::{AppError, AppResult},
    now, retrieval,
};

pub fn chapters(conn: &Connection, project_id: &str) -> AppResult<Vec<ChapterRecord>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.number, c.title, c.status, c.current_revision_id, c.style_profile_id,
            c.style_epoch, (SELECT COUNT(*) FROM scene s WHERE s.chapter_id=c.id), c.updated_at
         FROM chapter c WHERE c.project_id=?1 ORDER BY c.number",
    )?;
    let rows = stmt
        .query_map([project_id], |row| {
            Ok(ChapterRecord {
                id: row.get(0)?,
                number: row.get(1)?,
                title: row.get(2)?,
                status: row.get(3)?,
                current_revision_id: row.get(4)?,
                style_profile_id: row.get(5)?,
                style_epoch: row.get(6)?,
                scene_count: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    Ok(rows)
}

pub fn get_chapter(conn: &Connection, project_id: &str, id: &str) -> AppResult<ChapterRecord> {
    chapters(conn, project_id)?
        .into_iter()
        .find(|chapter| chapter.id == id)
        .ok_or_else(|| AppError::Message("找不到章节".into()))
}

fn scene_from_row(row: &Row<'_>) -> rusqlite::Result<SceneRecord> {
    let emotion: String = row.get(10)?;
    let sensory: String = row.get(11)?;
    Ok(SceneRecord {
        id: row.get(0)?,
        chapter_id: row.get(1)?,
        number: row.get(2)?,
        goal: row.get(3)?,
        conflict: row.get(4)?,
        pov: row.get(5)?,
        story_time: row.get(6)?,
        branch_id: row.get(7)?,
        place_id: row.get(8)?,
        style_profile_id: row.get(9)?,
        emotion_beat: serde_json::from_str(&emotion).unwrap_or(EmotionBeat {
            target: String::new(),
            intensity: 3,
            entry: String::new(),
            exit: String::new(),
            turn: String::new(),
        }),
        sensory_plan: serde_json::from_str(&sensory).unwrap_or(SensoryPlan {
            channels: vec![],
            motifs: vec![],
            distance: String::new(),
        }),
        status: row.get(12)?,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
    })
}

const SELECT: &str = "SELECT id, chapter_id, number, goal, conflict, pov, story_time, branch_id,
    place_id, style_profile_id, emotion_beat, sensory_plan, status, created_at, updated_at FROM scene";

pub fn scenes(
    conn: &Connection,
    project_id: &str,
    chapter_id: Option<&str>,
) -> AppResult<Vec<SceneRecord>> {
    let mut sql = format!("{SELECT} WHERE project_id=?1");
    if chapter_id.is_some() {
        sql.push_str(" AND chapter_id=?2");
    }
    sql.push_str(" ORDER BY chapter_id, number");
    let mut stmt = conn.prepare(&sql)?;
    let rows = if let Some(chapter_id) = chapter_id {
        stmt.query_map(params![project_id, chapter_id], scene_from_row)?
            .collect::<Result<Vec<_>, _>>()?
    } else {
        stmt.query_map([project_id], scene_from_row)?
            .collect::<Result<Vec<_>, _>>()?
    };
    drop(stmt);
    Ok(rows)
}

pub fn get_scene(conn: &Connection, project_id: &str, id: &str) -> AppResult<SceneRecord> {
    conn.query_row(
        &format!("{SELECT} WHERE project_id=?1 AND id=?2"),
        params![project_id, id],
        scene_from_row,
    )
    .optional()?
    .ok_or_else(|| AppError::Message("找不到场景".into()))
}

pub fn save_scene(conn: &Connection, project_id: &str, input: &SceneInput) -> AppResult<SceneRecord> {
    if input.goal.trim().is_empty() {
        return Err(AppError::Message("场景目标不能为空".into()));
    }
    if input.conflict.trim().is_empty() {
        return Err(AppError::Message("场景冲突不能为空".into()));
    }
    get_chapter(conn, project_id, &input.chapter_id)?;
    let ts = now();
    let branch_id = crate::branches::current(conn, project_id)?;
    let id = if let Some(id) = &input.id {
        get_scene(conn, project_id, id)?;
        conn.execute(
            "UPDATE scene SET goal=?3, conflict=?4, pov=?5, story_time=?6, place_id=?7,
                style_profile_id=?8, emotion_beat=?9, sensory_plan=?10, updated_at=?11
             WHERE project_id=?1 AND id=?2",
            params![
                project_id,
                id,
                input.goal.trim(),
                input.conflict.trim(),
                input.pov.trim(),
                input.story_time.trim(),
                input.place_id,
                input.style_profile_id,
                serde_json::to_string(&input.emotion_beat)?,
                serde_json::to_string(&input.sensory_plan)?,
                ts
            ],
        )?;
        id.clone()
    } else {
        let number: i32 = conn.query_row(
            "SELECT COALESCE(MAX(number),0)+1 FROM scene WHERE chapter_id=?1",
            [&input.chapter_id],
            |row| row.get(0),
        )?;
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO scene (id, project_id, chapter_id, number, goal, conflict, pov, story_time,
                branch_id, place_id, style_profile_id, emotion_beat, sensory_plan, status, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?13,?9,?10,?11,?12,'planned',?14,?14)",
            params![
                id,
                project_id,
                input.chapter_id,
                number,
                input.goal.trim(),
                input.conflict.trim(),
                input.pov.trim(),
                input.story_time.trim(),
                input.place_id,
                input.style_profile_id,
                serde_json::to_string(&input.emotion_beat)?,
                serde_json::to_string(&input.sensory_plan)?,
                ts,
                branch_id
            ],
        )?;
        id
    };
    crate::graphrag::mark_stale(conn, project_id)?;
    get_scene(conn, project_id, &id)
}

pub fn scene_contract(
    conn: &Connection,
    project_id: &str,
    scene: &SceneRecord,
) -> AppResult<String> {
    let chapter = get_chapter(conn, project_id, &scene.chapter_id)?;
    Ok(format!(
        "第 {} 章《{}》第 {} 场\n\
         目标：{}\n\
         冲突：{}\n\
         视角：{}\n\
         故事时间：{}\n\
         情绪节拍：{}（强度 {}/5）· 入场：{} · 出场：{} · 转折：{}\n\
         感官计划：{}；母题：{}；镜头距离：{}\n\
         约束：不得改变已确认硬事实；场景结束时必须发生可记录的状态变化。",
        chapter.number,
        chapter.title,
        scene.number,
        scene.goal,
        scene.conflict,
        if scene.pov.is_empty() { "未设定" } else { &scene.pov },
        if scene.story_time.is_empty() {
            "未设定"
        } else {
            &scene.story_time
        },
        if scene.emotion_beat.target.is_empty() {
            "未设定"
        } else {
            &scene.emotion_beat.target
        },
        scene.emotion_beat.intensity,
        scene.emotion_beat.entry,
        scene.emotion_beat.exit,
        scene.emotion_beat.turn,
        if scene.sensory_plan.channels.is_empty() {
            "视觉、触觉".to_string()
        } else {
            scene.sensory_plan.channels.join("、")
        },
        if scene.sensory_plan.motifs.is_empty() {
            "未指定".to_string()
        } else {
            scene.sensory_plan.motifs.join("、")
        },
        if scene.sensory_plan.distance.is_empty() {
            "中景"
        } else {
            &scene.sensory_plan.distance
        }
    ))
}

pub fn chapter_text(conn: &Connection, project_id: &str, chapter_id: &str) -> AppResult<String> {
    let revision: Option<String> = conn
        .query_row(
            "SELECT current_revision_id FROM chapter WHERE project_id=?1 AND id=?2",
            params![project_id, chapter_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    if let Some(revision) = revision {
        if let Some(text) = conn
            .query_row(
                "SELECT snapshot.content FROM revision
                 JOIN snapshot ON snapshot.id=revision.snapshot_id
                 WHERE revision.project_id=?1 AND revision.id=?2",
                params![project_id, revision],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            return Ok(text);
        }
    }
    Ok(conn
        .query_row(
            "SELECT content FROM text_fts WHERE project_id=?1 AND chapter_id=?2 LIMIT 1",
            params![project_id, chapter_id],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or_default())
}

/// 写入一个正文版本：快照 + revision + 章节指针 + 索引，同一事务完成。
pub fn commit_text(
    conn: &mut Connection,
    project_id: &str,
    chapter_id: &str,
    text: &str,
    author_action: &str,
) -> AppResult<DraftAcceptResult> {
    if text.trim().is_empty() {
        return Err(AppError::Message("正文为空，不能提交版本".into()));
    }
    let chapter = get_chapter(conn, project_id, chapter_id)?;
    let before = chapter_text(conn, project_id, chapter_id)?;
    let ts = now();
    let snapshot_id = uuid::Uuid::new_v4().to_string();
    let revision_id = uuid::Uuid::new_v4().to_string();
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO snapshot (id, project_id, chapter_id, content, metadata, created_at)
         VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            snapshot_id,
            project_id,
            chapter_id,
            text,
            serde_json::json!({
                "chapterNumber": chapter.number,
                "title": chapter.title,
                "chars": text.chars().count()
            })
            .to_string(),
            ts
        ],
    )?;
    tx.execute(
        "INSERT INTO revision (id, project_id, parent_id, chapter_id, diff, snapshot_id, author_action, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![
            revision_id,
            project_id,
            chapter.current_revision_id,
            chapter_id,
            serde_json::json!({
                "beforeChars": before.chars().count(),
                "afterChars": text.chars().count(),
                "action": author_action
            })
            .to_string(),
            snapshot_id,
            author_action,
            ts
        ],
    )?;
    tx.execute(
        "UPDATE chapter SET current_revision_id=?3, status='revised', updated_at=?4
         WHERE project_id=?1 AND id=?2",
        params![project_id, chapter_id, revision_id, ts],
    )?;
    tx.execute(
        "UPDATE scene SET status='drafted', updated_at=?2 WHERE chapter_id=?1 AND status='planned'",
        params![chapter_id, ts],
    )?;
    tx.execute(
        "DELETE FROM text_fts WHERE project_id=?1 AND chapter_id=?2 AND source_kind='chapter'",
        params![project_id, chapter_id],
    )?;
    tx.execute(
        "INSERT INTO text_fts (content, tokenized, source_id, project_id, source_kind, chapter_id, branch_id)
         VALUES (?1,?2,?3,?4,'chapter',?5,NULL)",
        params![
            text,
            retrieval::tokenize(text),
            revision_id,
            project_id,
            chapter_id
        ],
    )?;
    crate::graphrag::mark_stale(&tx, project_id)?;
    tx.commit()?;
    Ok(DraftAcceptResult {
        chapter_id: chapter_id.to_string(),
        revision_id,
        snapshot_id,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::project;

    use super::*;

    #[test]
    fn scene_save_and_revision_are_consistent() {
        let dir = tempfile::tempdir().unwrap();
        let mut loaded = project::create(&crate::domain::CreateProjectInput {
            name: "t".into(),
            root_path: dir.path().join("p").to_string_lossy().into_owned(),
            mode: "idea".into(),
            writing_language: None,
        })
        .unwrap();
        let pid = loaded.summary.id.clone();
        let chapter_id = uuid::Uuid::new_v4().to_string();
        loaded
            .conn
            .execute(
                "INSERT INTO chapter (id, project_id, number, title, status, style_epoch, created_at, updated_at)
                 VALUES (?1,?2,1,'雨夜','planned',1,?3,?3)",
                params![chapter_id, pid, now()],
            )
            .unwrap();
        let scene = save_scene(
            &loaded.conn,
            &pid,
            &SceneInput {
                id: None,
                chapter_id: chapter_id.clone(),
                goal: "拿到信".into(),
                conflict: "母亲阻止".into(),
                pov: "林澈".into(),
                story_time: "葬礼夜".into(),
                place_id: None,
                style_profile_id: None,
                emotion_beat: EmotionBeat {
                    target: "压抑".into(),
                    intensity: 4,
                    entry: "麻木".into(),
                    exit: "动摇".into(),
                    turn: "发现信是空白的".into(),
                },
                sensory_plan: SensoryPlan {
                    channels: vec!["嗅觉".into(), "触觉".into()],
                    motifs: vec!["雨".into()],
                    distance: "近景".into(),
                },
            },
        )
        .unwrap();
        assert_eq!(scene.number, 1);
        let contract = scene_contract(&loaded.conn, &pid, &scene).unwrap();
        assert!(contract.contains("情绪节拍"));
        let result = commit_text(
            &mut loaded.conn,
            &pid,
            &chapter_id,
            "雨落在旧灯上。林澈把信攥进掌心。",
            "draft_accept",
        )
        .unwrap();
        assert_eq!(
            chapter_text(&loaded.conn, &pid, &chapter_id).unwrap(),
            "雨落在旧灯上。林澈把信攥进掌心。"
        );
        let meta: String = loaded
            .conn
            .query_row(
                "SELECT metadata FROM snapshot WHERE id=?1",
                [&result.snapshot_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(meta.contains("chars"));
        let _: serde_json::Value = json!(result.revision_id);
    }
}
