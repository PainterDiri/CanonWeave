use std::path::Path;

use rusqlite::Connection;

use crate::error::{AppError, AppResult};

pub const SCHEMA_VERSION: i64 = 10;

const MIGRATIONS: &[(i64, &str, &str)] = &[
    (1, "init", include_str!("../../migrations/001_init.sql")),
    (
        2,
        "generation_link",
        include_str!("../../migrations/002_generation_link.sql"),
    ),
    (
        3,
        "audit_refinement",
        include_str!("../../migrations/003_audit_refinement.sql"),
    ),
    (
        4,
        "graph_branches",
        include_str!("../../migrations/004_graph_branches.sql"),
    ),
    (
        5,
        "m5_compiler",
        include_str!("../../migrations/005_m5_compiler.sql"),
    ),
    (
        6,
        "structure_knowledge",
        include_str!("../../migrations/006_structure_knowledge.sql"),
    ),
    (
        7,
        "narrative_blueprint",
        include_str!("../../migrations/007_narrative_blueprint.sql"),
    ),
    (
        8,
        "agent_runtime",
        include_str!("../../migrations/008_agent_runtime.sql"),
    ),
    (
        9,
        "optional_embeddings",
        include_str!("../../migrations/009_optional_embeddings.sql"),
    ),
    (
        10,
        "graph_wiki_hierarchy",
        include_str!("../../migrations/010_graph_wiki_hierarchy.sql"),
    ),
];

pub fn open(path: &Path) -> AppResult<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut conn = Connection::open(path)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrate(&mut conn)?;
    Ok(conn)
}

pub fn migrate(conn: &mut Connection) -> AppResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migration (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL
        );",
    )?;

    for (version, name, sql) in MIGRATIONS {
        let applied: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migration WHERE version = ?1)",
            [version],
            |row| row.get(0),
        )?;
        if applied {
            continue;
        }
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.execute(
            "INSERT INTO schema_migration (version, name, applied_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![version, name, crate::now()],
        )?;
        tx.commit()?;
    }

    let current: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migration",
        [],
        |row| row.get(0),
    )?;
    if current != SCHEMA_VERSION {
        return Err(AppError::Message(format!(
            "项目数据库版本 {current} 与当前应用版本 {SCHEMA_VERSION} 不一致"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_schema_and_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        migrate(&mut conn).unwrap();
        let tables: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('project','claim','candidate','chat_session','text_fts')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tables, 5);
    }

    #[test]
    fn fts5_is_available() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE VIRTUAL TABLE t USING fts5(content, tokenize='unicode61');
             INSERT INTO t(content) VALUES ('雨落在旧灯上');
             SELECT * FROM t WHERE t MATCH '旧灯';",
        )
        .unwrap();
    }

    #[test]
    fn upgrades_v7_project_through_agent_and_embedding_without_losing_candidates() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_migration (
               version INTEGER PRIMARY KEY,
               name TEXT NOT NULL,
               applied_at TEXT NOT NULL
             );",
        )
        .unwrap();
        for (version, name, sql) in MIGRATIONS.iter().take(7) {
            conn.execute_batch(sql).unwrap();
            conn.execute(
                "INSERT INTO schema_migration (version, name, applied_at) VALUES (?1,?2,'now')",
                rusqlite::params![version, name],
            )
            .unwrap();
        }
        conn.execute_batch(
            "INSERT INTO project
             (id,name,root_path,schema_version,writing_language,mode,created_at,updated_at)
             VALUES ('p','旧项目','/tmp/old',7,'zh-CN','both','now','now');
             INSERT INTO candidate
             (id,project_id,kind,payload,confidence,risk_level,status,created_at)
             VALUES ('c','p','entity','{}',0.5,'L1','pending','now');
             INSERT INTO wiki_page
             (id,project_id,entity_id,kind,title,content,content_hash,source_ids,generated_at,stale_at)
             VALUES ('w','p',NULL,'timeline','旧时间线','旧内容','wh','[]','now',NULL);",
        )
        .unwrap();

        migrate(&mut conn).unwrap();
        let candidate_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM candidate WHERE id='c'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(candidate_count, 1);
        let agent_tables: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type='table' AND name IN ('agent_run','agent_step','agent_evidence')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(agent_tables, 3);
        let schema: i32 = conn
            .query_row(
                "SELECT schema_version FROM project WHERE id='p'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(schema, 10);
        let embedding_columns: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('embedding')
                 WHERE name IN ('content','content_hash','status','branch_id','updated_at')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(embedding_columns, 5);
        let graph_columns: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('graph_community')
                 WHERE name IN ('parent_id','community_key','source_hash','summary_mode')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(graph_columns, 4);
        let wiki_tables: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type='table' AND name='wiki_page_dependency'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(wiki_tables, 1);
        let (page_key, source_hash): (String, String) = conn
            .query_row(
                "SELECT page_key, source_hash FROM wiki_page WHERE id='w'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(page_key, "w");
        assert_eq!(source_hash, "wh");
    }
}
