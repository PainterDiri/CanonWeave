use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::{json, Value};

use crate::{
    domain::{Premise, PremiseInput},
    error::{AppError, AppResult},
    now,
};

fn premise_from_row(row: &Row<'_>) -> rusqlite::Result<Premise> {
    let world_hook: String = row.get(11)?;
    let field_sources: String = row.get(19)?;
    let taboo: String = row.get(18)?;
    Ok(Premise {
        id: row.get(0)?,
        logline: row.get(1)?,
        genre: row.get(2)?,
        subgenre: row.get(3)?,
        theme: row.get(4)?,
        emotional_promise: row.get(5)?,
        tone: row.get(6)?,
        core_conflict: row.get(7)?,
        dramatic_question: row.get(25)?,
        antagonistic_force: row.get(26)?,
        world_rule: row.get(27)?,
        ending_direction: row.get(28)?,
        protagonist_desire: row.get(8)?,
        flaw: row.get(9)?,
        secret: row.get(10)?,
        world_hook: serde_json::from_str(&world_hook).unwrap_or_default(),
        pov: row.get(12)?,
        person: row.get(13)?,
        tense: row.get(14)?,
        target_length: row.get(15)?,
        update_rhythm: row.get(16)?,
        ending_intent: row.get(17)?,
        taboo: serde_json::from_str(&taboo).unwrap_or_default(),
        status: row.get(20)?,
        source_message_id: row.get(21)?,
        field_sources: serde_json::from_str(&field_sources).unwrap_or(json!({})),
        version: row.get(22)?,
        created_at: row.get(23)?,
        updated_at: row.get(24)?,
    })
}

const SELECT: &str = "SELECT id, logline, genre, subgenre, theme, emotional_promise, tone,
    core_conflict, protagonist_desire, flaw, secret, world_hook, pov, person, tense,
    target_length, update_rhythm, ending_intent, taboo, field_sources, status,
    source_message_id, version, created_at, updated_at, dramatic_question,
    antagonistic_force, world_rule, ending_direction FROM premise";

pub fn list(conn: &Connection, project_id: &str) -> AppResult<Vec<Premise>> {
    let mut stmt = conn.prepare(&format!(
        "{SELECT} WHERE project_id = ?1 ORDER BY CASE status WHEN 'confirmed' THEN 0 WHEN 'candidate' THEN 1 ELSE 2 END, created_at"
    ))?;
    let rows = stmt
        .query_map([project_id], premise_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn active(conn: &Connection, project_id: &str) -> AppResult<Option<Premise>> {
    Ok(conn
        .query_row(
            &format!("{SELECT} WHERE project_id = ?1 AND status = 'confirmed' LIMIT 1"),
            [project_id],
            premise_from_row,
        )
        .optional()?)
}

pub fn get(conn: &Connection, project_id: &str, id: &str) -> AppResult<Premise> {
    conn.query_row(
        &format!("{SELECT} WHERE project_id = ?1 AND id = ?2"),
        params![project_id, id],
        premise_from_row,
    )
    .optional()?
    .ok_or_else(|| AppError::Message("找不到种子卡".into()))
}

/// 将模型返回的三种走向写入 premise 候选，保留全部字段来源为 inferred。
pub fn insert_directions(
    conn: &mut Connection,
    project_id: &str,
    directions: &[Value],
    source_message_id: Option<&str>,
) -> AppResult<Vec<Premise>> {
    if directions.is_empty() {
        return Err(AppError::Message("模型没有返回任何故事走向".into()));
    }
    if directions.len() > 5 {
        return Err(AppError::Message("模型返回的走向过多".into()));
    }
    let tx = conn.transaction()?;
    let ts = now();
    let mut ids = Vec::new();
    for direction in directions.iter().take(3) {
        let id = uuid::Uuid::new_v4().to_string();
        let value = |key: &str| -> String {
            direction
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string()
        };
        let array = |key: &str| -> String {
            serde_json::to_string(
                &direction
                    .get(key)
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
            )
            .unwrap_or_else(|_| "[]".into())
        };
        let logline = value("logline");
        if logline.is_empty() {
            continue;
        }
        let fields = json!({
            "logline": "inferred",
            "coreConflict": "inferred",
            "genre": "inferred",
            "theme": "inferred",
            "tone": "inferred",
            "dramaticQuestion": "inferred",
            "antagonisticForce": "inferred",
            "worldRule": "inferred",
            "endingDirection": "inferred"
        });
        tx.execute(
            "INSERT INTO premise (id, project_id, logline, genre, subgenre, theme, emotional_promise,
                tone, core_conflict, protagonist_desire, flaw, secret, world_hook, pov, person, tense,
                target_length, update_rhythm, ending_intent, taboo, status, source_message_id,
                field_sources, version, created_at, updated_at, dramatic_question,
                antagonistic_force, world_rule, ending_direction)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                ?17, ?18, ?19, ?20, 'candidate', ?21, ?22, 1, ?23, ?23, ?24, ?25, ?26, ?27)",
            params![
                id,
                project_id,
                logline,
                value("genre"),
                value("subgenre"),
                value("theme"),
                value("emotional_promise"),
                value("tone"),
                value("core_conflict"),
                value("protagonist_desire"),
                value("flaw"),
                value("secret"),
                array("world_hook"),
                value("pov"),
                value("person"),
                value("tense"),
                value("target_length"),
                value("update_rhythm"),
                value("ending_intent"),
                array("taboo"),
                source_message_id,
                fields.to_string(),
                ts,
                value("dramatic_question"),
                value("antagonistic_force"),
                value("world_rule"),
                value("ending_direction")
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
    id: &str,
    input: &PremiseInput,
) -> AppResult<Premise> {
    if input.logline.trim().is_empty() {
        return Err(AppError::Message("一句话故事不能为空".into()));
    }
    if input.core_conflict.trim().is_empty() {
        return Err(AppError::Message("核心冲突不能为空".into()));
    }
    let current = get(conn, project_id, id)?;
    let mut sources = current
        .field_sources
        .as_object()
        .cloned()
        .unwrap_or_default();
    for field in &input.author_fields {
        sources.insert(field.clone(), Value::String("author".into()));
    }
    sources.insert("logline".into(), Value::String("author".into()));
    sources.insert("coreConflict".into(), Value::String("author".into()));
    let ts = now();
    conn.execute(
        "UPDATE premise SET logline=?3, genre=?4, subgenre=?5, theme=?6, emotional_promise=?7,
            tone=?8, core_conflict=?9, protagonist_desire=?10, flaw=?11, secret=?12, world_hook=?13,
            pov=?14, person=?15, tense=?16, target_length=?17, update_rhythm=?18, ending_intent=?19,
            taboo=?20, field_sources=?21, version=version+1, updated_at=?22,
            dramatic_question=?23, antagonistic_force=?24, world_rule=?25, ending_direction=?26
         WHERE project_id=?1 AND id=?2",
        params![
            project_id,
            id,
            input.logline.trim(),
            input.genre.trim(),
            input.subgenre.trim(),
            input.theme.trim(),
            input.emotional_promise.trim(),
            input.tone.trim(),
            input.core_conflict.trim(),
            input.protagonist_desire.trim(),
            input.flaw.trim(),
            input.secret.trim(),
            serde_json::to_string(&input.world_hook)?,
            input.pov.trim(),
            input.person.trim(),
            input.tense.trim(),
            input.target_length.trim(),
            input.update_rhythm.trim(),
            input.ending_intent.trim(),
            serde_json::to_string(&input.taboo)?,
            Value::Object(sources).to_string(),
            ts,
            input.dramatic_question.trim(),
            input.antagonistic_force.trim(),
            input.world_rule.trim(),
            input.ending_direction.trim()
        ],
    )?;
    get(conn, project_id, id)
}

/// 确认种子卡：必填字段校验，其余候选归档而非删除。
pub fn confirm(conn: &mut Connection, project_id: &str, id: &str) -> AppResult<Premise> {
    let card = get(conn, project_id, id)?;
    if card.status == "archived" {
        return Err(AppError::Message("已归档的种子卡不能直接确认".into()));
    }
    if card.logline.trim().is_empty() || card.core_conflict.trim().is_empty() {
        return Err(AppError::Message(
            "种子卡至少需要一句话故事与核心冲突，才能确认".into(),
        ));
    }
    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE premise SET status='archived', updated_at=?2 WHERE project_id=?1 AND id != ?3 AND status='candidate'",
        params![project_id, now(), id],
    )?;
    tx.execute(
        "UPDATE premise SET status='confirmed', version=version+1, updated_at=?3 WHERE project_id=?1 AND id=?2",
        params![project_id, id, now()],
    )?;
    tx.commit()?;
    get(conn, project_id, id)
}

pub fn archive(conn: &Connection, project_id: &str, id: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE premise SET status='archived', updated_at=?3 WHERE project_id=?1 AND id=?2",
        params![project_id, id, now()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::project;

    use super::*;

    fn setup() -> (tempfile::TempDir, crate::project::LoadedProject) {
        let dir = tempfile::tempdir().unwrap();
        let loaded = project::create(&crate::domain::CreateProjectInput {
            name: "t".into(),
            root_path: dir.path().join("p").to_string_lossy().into_owned(),
            mode: "idea".into(),
            writing_language: None,
        })
        .unwrap();
        (dir, loaded)
    }

    #[test]
    fn inserts_and_confirms_one_direction() {
        let (_dir, mut loaded) = setup();
        let directions = vec![
            json!({"logline":"A", "core_conflict":"A conflict", "genre":"悬疑"}),
            json!({"logline":"B", "core_conflict":"B conflict", "genre":"奇幻"}),
        ];
        let cards = insert_directions(&mut loaded.conn, &loaded.summary.id, &directions, None).unwrap();
        assert_eq!(cards.len(), 2);
        let confirmed = confirm(&mut loaded.conn, &loaded.summary.id, &cards[1].id).unwrap();
        assert_eq!(confirmed.status, "confirmed");
        let all = list(&loaded.conn, &loaded.summary.id).unwrap();
        assert_eq!(all.iter().filter(|p| p.status == "confirmed").count(), 1);
        assert_eq!(all.iter().filter(|p| p.status == "archived").count(), 1);
    }

    #[test]
    fn update_marks_author_fields_and_rejects_empty_conflict() {
        let (_dir, mut loaded) = setup();
        let cards = insert_directions(
            &mut loaded.conn,
            &loaded.summary.id,
            &[json!({
                "logline":"A",
                "core_conflict":"C",
                "dramatic_question":"A 能否在失去 B 前救回 C？",
                "antagonistic_force":"守门人通过规则阻止 A",
                "world_rule":"每次开门都会抹去一段记忆",
                "ending_direction":"A 接受失去记忆"
            })],
            None,
        )
        .unwrap();
        let card = &cards[0];
        assert_eq!(card.dramatic_question, "A 能否在失去 B 前救回 C？");
        assert_eq!(card.antagonistic_force, "守门人通过规则阻止 A");
        let input = PremiseInput {
            logline: "新的句子".into(),
            genre: "悬疑".into(),
            subgenre: String::new(),
            theme: String::new(),
            emotional_promise: String::new(),
            tone: String::new(),
            core_conflict: "新的冲突".into(),
            dramatic_question: String::new(),
            antagonistic_force: String::new(),
            world_rule: String::new(),
            ending_direction: String::new(),
            protagonist_desire: String::new(),
            flaw: String::new(),
            secret: String::new(),
            world_hook: vec![],
            pov: String::new(),
            person: String::new(),
            tense: String::new(),
            target_length: String::new(),
            update_rhythm: String::new(),
            ending_intent: String::new(),
            taboo: vec![],
            source_message_id: None,
            author_fields: vec!["genre".into()],
        };
        let updated = update(&loaded.conn, &loaded.summary.id, &card.id, &input).unwrap();
        assert_eq!(updated.field_sources["genre"], "author");
        assert_eq!(updated.field_sources["tone"], "inferred");
        let mut empty = input;
        empty.core_conflict = " ".into();
        assert!(update(&loaded.conn, &loaded.summary.id, &card.id, &empty).is_err());
    }
}
