use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::json;

use crate::{
    domain::{CharacterDetail, CharacterInput, Entity},
    error::{AppError, AppResult},
    now,
};

fn entity_from_row(row: &Row<'_>) -> rusqlite::Result<Entity> {
    let aliases: String = row.get(3)?;
    Ok(Entity {
        id: row.get(0)?,
        entity_type: row.get(1)?,
        canonical_name: row.get(2)?,
        aliases: serde_json::from_str(&aliases).unwrap_or_default(),
        status: row.get(4)?,
        description: row.get(5)?,
    })
}

pub fn list(conn: &Connection, project_id: &str, entity_type: Option<&str>) -> AppResult<Vec<Entity>> {
    let mut sql = "SELECT id, type, canonical_name, aliases, status, description FROM entity WHERE project_id=?1"
        .to_string();
    if entity_type.is_some() {
        sql.push_str(" AND type=?2");
    }
    sql.push_str(" ORDER BY canonical_name");
    let mut stmt = conn.prepare(&sql)?;
    let rows = if let Some(entity_type) = entity_type {
        stmt.query_map(params![project_id, entity_type], entity_from_row)?
            .collect::<Result<Vec<_>, _>>()?
    } else {
        stmt.query_map([project_id], entity_from_row)?
            .collect::<Result<Vec<_>, _>>()?
    };
    Ok(rows)
}

pub fn get_entity(conn: &Connection, project_id: &str, id: &str) -> AppResult<Entity> {
    conn.query_row(
        "SELECT id, type, canonical_name, aliases, status, description FROM entity
         WHERE project_id=?1 AND id=?2",
        params![project_id, id],
        entity_from_row,
    )
    .optional()?
    .ok_or_else(|| AppError::Message("找不到实体".into()))
}

pub fn characters(conn: &Connection, project_id: &str) -> AppResult<Vec<CharacterDetail>> {
    let mut stmt = conn.prepare(
        "SELECT e.id, e.type, e.canonical_name, e.aliases, e.status, e.description,
            COALESCE(c.desire,''), COALESCE(c.need,''), COALESCE(c.fear,''), COALESCE(c.flaw,''),
            COALESCE(c.\"values\",'[]'), COALESCE(c.secret,''), COALESCE(c.taboo,'[]'),
            COALESCE(c.voice_anchor,'')
         FROM entity e LEFT JOIN character_core c ON c.entity_id=e.id
         WHERE e.project_id=?1 AND e.type='character'
         ORDER BY e.canonical_name",
    )?;
    let rows = stmt
        .query_map([project_id], |row| {
            let aliases: String = row.get(3)?;
            let values: String = row.get(10)?;
            let taboo: String = row.get(12)?;
            Ok(CharacterDetail {
                entity: Entity {
                    id: row.get(0)?,
                    entity_type: row.get(1)?,
                    canonical_name: row.get(2)?,
                    aliases: serde_json::from_str(&aliases).unwrap_or_default(),
                    status: row.get(4)?,
                    description: row.get(5)?,
                },
                desire: row.get(6)?,
                need: row.get(7)?,
                fear: row.get(8)?,
                flaw: row.get(9)?,
                values: serde_json::from_str(&values).unwrap_or_default(),
                secret: row.get(11)?,
                taboo: serde_json::from_str(&taboo).unwrap_or_default(),
                voice_anchor: row.get(13)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn save_character(
    conn: &mut Connection,
    project_id: &str,
    input: &CharacterInput,
) -> AppResult<CharacterDetail> {
    if input.canonical_name.trim().is_empty() {
        return Err(AppError::Message("人物名称不能为空".into()));
    }
    let ts = now();
    let tx = conn.transaction()?;
    let entity_id = if let Some(id) = &input.entity_id {
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM entity WHERE project_id=?1 AND id=?2 AND type='character')",
            params![project_id, id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(AppError::Message("人物不存在".into()));
        }
        tx.execute(
            "UPDATE entity SET canonical_name=?3, aliases=?4, description=?5, updated_at=?6
             WHERE project_id=?1 AND id=?2",
            params![
                project_id,
                id,
                input.canonical_name.trim(),
                serde_json::to_string(&input.aliases)?,
                input.description.trim(),
                ts
            ],
        )?;
        id.clone()
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO entity (id, project_id, type, canonical_name, aliases, status, description, created_at, updated_at)
             VALUES (?1,?2,'character',?3,?4,'confirmed',?5,?6,?6)",
            params![
                id,
                project_id,
                input.canonical_name.trim(),
                serde_json::to_string(&input.aliases)?,
                input.description.trim(),
                ts
            ],
        )?;
        id
    };
    tx.execute(
        "INSERT INTO character_core (entity_id, desire, need, fear, flaw, \"values\", secret, taboo,
            voice_anchor, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
         ON CONFLICT(entity_id) DO UPDATE SET desire=excluded.desire, need=excluded.need,
            fear=excluded.fear, flaw=excluded.flaw, \"values\"=excluded.\"values\",
            secret=excluded.secret, taboo=excluded.taboo, voice_anchor=excluded.voice_anchor,
            updated_at=excluded.updated_at",
        params![
            entity_id,
            input.desire.trim(),
            input.need.trim(),
            input.fear.trim(),
            input.flaw.trim(),
            serde_json::to_string(&input.values)?,
            input.secret.trim(),
            serde_json::to_string(&input.taboo)?,
            input.voice_anchor.trim(),
            ts
        ],
    )?;
    tx.commit()?;
    get_character(conn, project_id, &entity_id)
}

pub fn get_character(conn: &Connection, project_id: &str, id: &str) -> AppResult<CharacterDetail> {
    let entity = get_entity(conn, project_id, id)?;
    if entity.entity_type != "character" {
        return Err(AppError::Message("该实体不是人物".into()));
    }
    let core = conn
        .query_row(
            "SELECT desire, need, fear, flaw, \"values\", secret, taboo, voice_anchor
             FROM character_core WHERE entity_id=?1",
            [id],
            |row| {
                let values: String = row.get(4)?;
                let taboo: String = row.get(6)?;
                Ok(CharacterDetail {
                    entity: entity.clone(),
                    desire: row.get(0)?,
                    need: row.get(1)?,
                    fear: row.get(2)?,
                    flaw: row.get(3)?,
                    values: serde_json::from_str(&values).unwrap_or_default(),
                    secret: row.get(5)?,
                    taboo: serde_json::from_str(&taboo).unwrap_or_default(),
                    voice_anchor: row.get(7)?,
                })
            },
        )
        .optional()?;
    Ok(core.unwrap_or(CharacterDetail {
        entity: entity.clone(),
        desire: String::new(),
        need: String::new(),
        fear: String::new(),
        flaw: String::new(),
        values: vec![],
        secret: String::new(),
        taboo: vec![],
        voice_anchor: String::new(),
    }))
}

pub fn ensure_character(
    conn: &Connection,
    project_id: &str,
    name: &str,
) -> AppResult<String> {
    if let Some(id) = conn
        .query_row(
            "SELECT id FROM entity WHERE project_id=?1 AND canonical_name=?2",
            params![project_id, name],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    {
        return Ok(id);
    }
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now();
    conn.execute(
        "INSERT INTO entity (id, project_id, type, canonical_name, aliases, status, description, created_at, updated_at)
         VALUES (?1,?2,'character',?3,?4,'confirmed','',?5,?5)",
        params![id, project_id, name, json!([]).to_string(), ts],
    )?;
    conn.execute(
        "INSERT INTO character_core (entity_id, updated_at) VALUES (?1,?2)",
        params![id, ts],
    )?;
    Ok(id)
}
