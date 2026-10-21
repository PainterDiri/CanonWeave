use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::{json, Value};

use crate::{
    domain::{
        KnowledgeDomain, KnowledgeDomainInput, KnowledgeEntry, KnowledgeEntryInput, KnowledgeField,
    },
    error::{AppError, AppResult},
    now,
};

const DEFAULT_DOMAINS: &[(&str, &str, &str, &str, i32, &str)] = &[
    (
        "environment",
        "环境",
        "地点之间的地理关系、空间布置、景色与环境规则。",
        "landmark",
        10,
        r#"[
          {"key":"category","label":"环境类型","placeholder":"城市、建筑群、自然区域或特殊空间"},
          {"key":"geography","label":"地理关系","placeholder":"与哪些地点相邻、位于何处、如何抵达"},
          {"key":"layout","label":"空间布置","placeholder":"入口、核心区域、路线、重要房间"},
          {"key":"landscape","label":"景色与感官","placeholder":"光线、声音、气味、季节和天气"},
          {"key":"rules","label":"环境规则","placeholder":"进入条件、危险、变化机制与例外"}
        ]"#,
    ),
    (
        "item",
        "重要物品",
        "与剧情紧密相关的物品、能力、归属与流转记录。",
        "package",
        20,
        r#"[
          {"key":"category","label":"物品类型","placeholder":"武器、凭证、遗物、媒介或其他"},
          {"key":"function","label":"作用与能力","placeholder":"它能做什么，故事中解决什么问题"},
          {"key":"ownership","label":"归属与流转","placeholder":"当前持有者、曾经持有者、转移条件"},
          {"key":"limits","label":"限制与代价","placeholder":"使用条件、损耗、反噬或失效方式"},
          {"key":"storyRole","label":"剧情作用","placeholder":"与哪些人物、事件和伏笔相关"}
        ]"#,
    ),
];

fn parse_fields(text: &str) -> Vec<KnowledgeField> {
    serde_json::from_str(text).unwrap_or_default()
}

fn domain_from_row(row: &Row<'_>) -> rusqlite::Result<KnowledgeDomain> {
    let schema: String = row.get(9)?;
    Ok(KnowledgeDomain {
        id: row.get(0)?,
        key: row.get(1)?,
        name: row.get(2)?,
        description: row.get(3)?,
        icon: row.get(4)?,
        accent: row.get(5)?,
        is_system: row.get::<_, i32>(6)? == 1,
        sort_order: row.get(7)?,
        field_schema: parse_fields(&schema),
        entry_count: row.get(8)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

const DOMAIN_SELECT: &str = "SELECT d.id, d.key, d.name, d.description, d.icon, d.accent,
    d.is_system, d.sort_order,
    (SELECT COUNT(*) FROM knowledge_entry e WHERE e.domain_id=d.id),
    d.field_schema, d.created_at, d.updated_at
    FROM knowledge_domain d";

pub fn ensure_defaults(conn: &Connection, project_id: &str) -> AppResult<()> {
    let ts = now();
    for (key, name, description, icon, order, schema) in DEFAULT_DOMAINS {
        conn.execute(
            "INSERT INTO knowledge_domain
             (id, project_id, key, name, description, icon, accent, is_system, sort_order,
              field_schema, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,'teal',1,?7,?8,?9,?9)
             ON CONFLICT(project_id, key) DO UPDATE SET
               name=excluded.name, description=excluded.description, icon=excluded.icon,
               sort_order=excluded.sort_order, field_schema=excluded.field_schema,
               updated_at=excluded.updated_at",
            params![
                format!("domain-{project_id}-{key}"),
                project_id,
                key,
                name,
                description,
                icon,
                order,
                schema,
                ts
            ],
        )?;
    }
    Ok(())
}

pub fn domains(conn: &Connection, project_id: &str) -> AppResult<Vec<KnowledgeDomain>> {
    ensure_defaults(conn, project_id)?;
    let mut stmt = conn.prepare(&format!(
        "{DOMAIN_SELECT} WHERE d.project_id=?1 ORDER BY d.sort_order, d.created_at"
    ))?;
    let rows = stmt
        .query_map([project_id], domain_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn create_domain(
    conn: &Connection,
    project_id: &str,
    input: &KnowledgeDomainInput,
) -> AppResult<KnowledgeDomain> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(AppError::Message("知识域名称不能为空".into()));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now();
    let key = format!("custom-{}", &id[..8]);
    let labels: Vec<String> = input
        .fields
        .iter()
        .map(|label| label.trim().to_string())
        .filter(|label| !label.is_empty())
        .collect();
    let labels = if labels.is_empty() {
        vec!["核心设定".into(), "规则与限制".into(), "关联与影响".into()]
    } else {
        labels
    };
    let schema: Vec<KnowledgeField> = labels
        .into_iter()
        .enumerate()
        .map(|(index, label)| KnowledgeField {
            key: format!("field{}", index + 1),
            label: label.clone(),
            placeholder: format!("填写{label}"),
        })
        .collect();
    let sort_order: i32 = conn.query_row(
        "SELECT COALESCE(MAX(sort_order), 20) + 10 FROM knowledge_domain WHERE project_id=?1",
        [project_id],
        |row| row.get(0),
    )?;
    conn.execute(
        "INSERT INTO knowledge_domain
         (id, project_id, key, name, description, icon, accent, is_system, sort_order,
          field_schema, created_at, updated_at)
         VALUES (?1,?2,?3,?4,?5,'sparkles','amber',0,?6,?7,?8,?8)",
        params![
            id,
            project_id,
            key,
            name,
            input.description.trim(),
            sort_order,
            serde_json::to_string(&schema)?,
            ts
        ],
    )?;
    conn.query_row(
        &format!("{DOMAIN_SELECT} WHERE d.project_id=?1 AND d.id=?2"),
        params![project_id, id],
        domain_from_row,
    )
    .map_err(Into::into)
}

fn entry_from_row(row: &Row<'_>) -> rusqlite::Result<KnowledgeEntry> {
    let aliases: String = row.get(7)?;
    let fields: String = row.get(9)?;
    Ok(KnowledgeEntry {
        id: row.get(0)?,
        entity_id: row.get(1)?,
        domain_id: row.get(2)?,
        domain_key: row.get(3)?,
        domain_name: row.get(4)?,
        name: row.get(5)?,
        summary: row.get(6)?,
        aliases: serde_json::from_str(&aliases).unwrap_or_default(),
        status: row.get(8)?,
        fields: serde_json::from_str(&fields).unwrap_or_else(|_| json!({})),
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

const ENTRY_SELECT: &str = "SELECT e.id, e.entity_id, e.domain_id, d.key, d.name, e.name,
    e.summary, e.aliases, e.status, e.fields, e.created_at, e.updated_at
    FROM knowledge_entry e JOIN knowledge_domain d ON d.id=e.domain_id";

pub fn entries(
    conn: &Connection,
    project_id: &str,
    domain_id: Option<&str>,
) -> AppResult<Vec<KnowledgeEntry>> {
    let mut sql = format!("{ENTRY_SELECT} WHERE e.project_id=?1");
    if domain_id.is_some() {
        sql.push_str(" AND e.domain_id=?2");
    }
    sql.push_str(" ORDER BY e.updated_at DESC, e.name");
    let mut stmt = conn.prepare(&sql)?;
    let rows = if let Some(domain_id) = domain_id {
        stmt.query_map(params![project_id, domain_id], entry_from_row)?
            .collect::<Result<Vec<_>, _>>()?
    } else {
        stmt.query_map([project_id], entry_from_row)?
            .collect::<Result<Vec<_>, _>>()?
    };
    Ok(rows)
}

pub fn prompt_context(conn: &Connection, project_id: &str, limit: usize) -> AppResult<String> {
    let mut stmt = conn.prepare(
        "SELECT d.name, e.name, e.summary, e.fields
         FROM knowledge_entry e JOIN knowledge_domain d ON d.id=e.domain_id
         WHERE e.project_id=?1
         ORDER BY d.sort_order, e.updated_at DESC LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![project_id, limit as i32], |row| {
            let fields: String = row.get(3)?;
            let field_text = serde_json::from_str::<Value>(&fields)
                .ok()
                .map(|value| value.to_string())
                .unwrap_or(fields);
            Ok(format!(
                "- [{}] {}：{}｜{}",
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                field_text
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows.join("\n"))
}

fn rebuild_fts(
    conn: &Connection,
    project_id: &str,
    entry_id: &str,
    domain: &KnowledgeDomain,
    name: &str,
    summary: &str,
    fields: &Value,
) -> AppResult<()> {
    conn.execute(
        "DELETE FROM text_fts WHERE project_id=?1 AND source_kind='knowledge' AND source_id=?2",
        params![project_id, entry_id],
    )?;
    let mut lines = vec![format!("知识域：{}", domain.name), format!("名称：{name}")];
    if !summary.trim().is_empty() {
        lines.push(summary.trim().to_string());
    }
    if let Some(values) = fields.as_object() {
        for field in &domain.field_schema {
            if let Some(value) = values.get(&field.key).and_then(Value::as_str) {
                if !value.trim().is_empty() {
                    lines.push(format!("{}：{}", field.label, value.trim()));
                }
            }
        }
    }
    let content = lines.join("\n");
    conn.execute(
        "INSERT INTO text_fts
         (content, tokenized, source_id, project_id, source_kind, chapter_id, branch_id)
         VALUES (?1,?2,?3,?4,'knowledge',NULL,NULL)",
        params![
            content,
            crate::retrieval::tokenize(&content),
            entry_id,
            project_id
        ],
    )?;
    Ok(())
}

pub fn save_entry(
    conn: &mut Connection,
    project_id: &str,
    input: &KnowledgeEntryInput,
) -> AppResult<KnowledgeEntry> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(AppError::Message("知识条目名称不能为空".into()));
    }
    let domain = conn
        .query_row(
            &format!("{DOMAIN_SELECT} WHERE d.project_id=?1 AND d.id=?2"),
            params![project_id, input.domain_id],
            domain_from_row,
        )
        .optional()?
        .ok_or_else(|| AppError::Message("知识域不存在".into()))?;
    if !input.fields.is_object() {
        return Err(AppError::Message("知识条目字段必须是对象".into()));
    }
    let ts = now();
    let tx = conn.transaction()?;
    let entity_type = format!("knowledge:{}", domain.key);
    let description = if input.summary.trim().is_empty() {
        format!("{}知识条目", domain.name)
    } else {
        input.summary.trim().to_string()
    };
    let entity_id = if let Some(id) = &input.id {
        let existing: Option<String> = tx
            .query_row(
                "SELECT entity_id FROM knowledge_entry WHERE project_id=?1 AND id=?2",
                params![project_id, id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        let entity_id = existing.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        tx.execute(
            "INSERT INTO entity
             (id, project_id, type, canonical_name, aliases, status, description, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,'confirmed',?6,?7,?7)
             ON CONFLICT(id) DO UPDATE SET type=excluded.type, canonical_name=excluded.canonical_name,
               aliases=excluded.aliases, description=excluded.description, updated_at=excluded.updated_at",
            params![
                entity_id,
                project_id,
                entity_type,
                name,
                serde_json::to_string(&input.aliases)?,
                description,
                ts
            ],
        )?;
        tx.execute(
            "UPDATE knowledge_entry SET entity_id=?3, name=?4, aliases=?5, summary=?6,
                fields=?7, updated_at=?8 WHERE project_id=?1 AND id=?2",
            params![
                project_id,
                id,
                entity_id,
                name,
                serde_json::to_string(&input.aliases)?,
                input.summary.trim(),
                input.fields.to_string(),
                ts
            ],
        )?;
        entity_id
    } else {
        let entity_id = uuid::Uuid::new_v4().to_string();
        let id = uuid::Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO entity
             (id, project_id, type, canonical_name, aliases, status, description, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,'confirmed',?6,?7,?7)",
            params![
                entity_id,
                project_id,
                entity_type,
                name,
                serde_json::to_string(&input.aliases)?,
                description,
                ts
            ],
        )?;
        tx.execute(
            "INSERT INTO knowledge_entry
             (id, project_id, domain_id, entity_id, name, aliases, summary, fields, status,
              created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'confirmed',?9,?9)",
            params![
                id,
                project_id,
                input.domain_id,
                entity_id,
                name,
                serde_json::to_string(&input.aliases)?,
                input.summary.trim(),
                input.fields.to_string(),
                ts
            ],
        )?;
        entity_id
    };
    let entry_id = if let Some(id) = &input.id {
        id.clone()
    } else {
        tx.query_row(
            "SELECT id FROM knowledge_entry WHERE project_id=?1 AND entity_id=?2",
            params![project_id, entity_id],
            |row| row.get(0),
        )?
    };
    rebuild_fts(
        &tx,
        project_id,
        &entry_id,
        &domain,
        name,
        &input.summary,
        &input.fields,
    )?;
    crate::graphrag::mark_stale(&tx, project_id)?;
    tx.commit()?;
    conn.query_row(
        &format!("{ENTRY_SELECT} WHERE e.project_id=?1 AND e.id=?2"),
        params![project_id, entry_id],
        entry_from_row,
    )
    .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use crate::domain::{CreateProjectInput, KnowledgeDomainInput, KnowledgeEntryInput};
    use crate::project;

    use super::*;

    #[test]
    fn seeds_defaults_and_saves_searchable_custom_entries() {
        let dir = tempfile::tempdir().unwrap();
        let mut loaded = project::create(&CreateProjectInput {
            name: "t".into(),
            root_path: dir.path().join("p").to_string_lossy().into_owned(),
            mode: "idea".into(),
            writing_language: None,
        })
        .unwrap();
        let project_id = loaded.summary.id.clone();
        let defaults = domains(&loaded.conn, &project_id).unwrap();
        assert!(defaults.iter().any(|domain| domain.key == "environment"));
        assert!(defaults.iter().any(|domain| domain.key == "item"));

        let domain = create_domain(
            &loaded.conn,
            &project_id,
            &KnowledgeDomainInput {
                name: "禁墟".into(),
                description: "禁墟能力与克制关系".into(),
                fields: vec!["能力".into(), "克制关系".into()],
            },
        )
        .unwrap();
        let entry = save_entry(
            &mut loaded.conn,
            &project_id,
            &KnowledgeEntryInput {
                id: None,
                domain_id: domain.id.clone(),
                name: "雨墟".into(),
                aliases: vec!["旧雨区".into()],
                summary: "只在暴雨夜开启。".into(),
                fields: json!({"field1":"吞噬声音","field2":"雷火"}),
            },
        )
        .unwrap();
        assert!(entry.entity_id.is_some());
        let hits =
            crate::retrieval::search_for_scene(&loaded.conn, &project_id, "雷火 禁墟", 5, None)
                .unwrap();
        assert!(hits.iter().any(|hit| hit.source_id == entry.id));
        let wiki = crate::wiki::rebuild(&mut loaded.conn, &project_id).unwrap();
        assert!(wiki.pages.iter().any(|page| page.kind == "knowledge"));
    }
}
