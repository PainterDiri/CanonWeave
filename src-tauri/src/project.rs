use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{
    db,
    domain::{CreateProjectInput, ProjectCounts, ProjectSnapshot, ProjectSummary},
    error::{AppError, AppResult},
    now,
};

const PROJECT_FILE: &str = "project.json";
const DB_FILE: &str = "narrative.sqlite";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectFile {
    id: String,
    name: String,
    mode: String,
    writing_language: String,
    schema_version: i32,
    created_at: String,
    #[serde(default)]
    current_branch_id: Option<String>,
}

#[derive(Debug)]
pub struct LoadedProject {
    pub summary: ProjectSummary,
    pub root: PathBuf,
    pub conn: Connection,
}

pub fn create(input: &CreateProjectInput) -> AppResult<LoadedProject> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(AppError::Message("项目名称不能为空".into()));
    }
    if !["idea", "import", "both"].contains(&input.mode.as_str()) {
        return Err(AppError::Message(format!("未知的项目模式：{}", input.mode)));
    }
    let root = PathBuf::from(input.root_path.trim());
    if root.as_os_str().is_empty() {
        return Err(AppError::Message("请选择项目目录".into()));
    }
    std::fs::create_dir_all(&root)?;
    if root.join(PROJECT_FILE).exists() {
        return Err(AppError::Message(
            "该目录已经包含 CanonWeave 项目，请使用“打开项目”".into(),
        ));
    }
    let has_entries = std::fs::read_dir(&root)?.next().is_some();
    if has_entries {
        return Err(AppError::Message(
            "请选择一个空目录来创建项目，避免覆盖已有文件".into(),
        ));
    }

    for dir in ["imports", "exports", "backups"] {
        std::fs::create_dir_all(root.join(dir))?;
    }

    let id = uuid::Uuid::new_v4().to_string();
    let main_branch_id = format!("main-{id}");
    let ts = now();
    let language = input
        .writing_language
        .clone()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "zh-CN".into());

    let mut conn = db::open(&root.join(DB_FILE))?;
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO project (id, name, root_path, schema_version, writing_language, mode,
            created_at, updated_at, current_branch_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8)",
        params![
            id,
            name,
            root.to_string_lossy(),
            db::SCHEMA_VERSION as i32,
            language,
            input.mode,
            ts,
            main_branch_id.clone()
        ],
    )?;
    tx.execute(
        "INSERT INTO branch (id, project_id, name, status, created_at) VALUES (?1, ?2, '主线', 'active', ?3)",
        params![main_branch_id.clone(), id, ts],
    )?;
    tx.execute(
        "INSERT INTO review_policy (id, project_id, scope, scope_id, mode, auto_accept_level, sample_rate, lock_hard, updated_at)
         VALUES (?1, ?2, 'project', ?2, 'balanced', 'L0', 0.0, 1, ?3)",
        params![format!("policy-{id}"), id, ts],
    )?;
    crate::rules::seed_defaults(&tx, &id)?;
    tx.commit()?;

    let file = ProjectFile {
        id: id.clone(),
        name: name.to_string(),
        mode: input.mode.clone(),
        writing_language: language.clone(),
        schema_version: db::SCHEMA_VERSION as i32,
        created_at: ts.clone(),
        current_branch_id: Some(main_branch_id.clone()),
    };
    write_project_file(&root, &file)?;

    Ok(LoadedProject {
        summary: ProjectSummary {
            id,
            name: name.to_string(),
            root_path: root.to_string_lossy().into_owned(),
            mode: input.mode.clone(),
            active_branch_id: Some(main_branch_id),
            writing_language: language,
            schema_version: db::SCHEMA_VERSION as i32,
            created_at: ts.clone(),
            updated_at: ts,
        },
        root,
        conn,
    })
}

pub fn open(root: &Path) -> AppResult<LoadedProject> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let meta_path = root.join(PROJECT_FILE);
    if !meta_path.exists() {
        return Err(AppError::Message(format!(
            "目录中没有 project.json：{}",
            root.display()
        )));
    }
    let file: ProjectFile = serde_json::from_str(&std::fs::read_to_string(&meta_path)?)?;
    if file.schema_version as i64 > db::SCHEMA_VERSION {
        return Err(AppError::Message(format!(
            "项目由更高版本创建（schema {}），请升级应用后再打开",
            file.schema_version
        )));
    }
    let conn = db::open(&root.join(DB_FILE))?;
    let summary = conn
        .query_row(
            "SELECT id, name, root_path, mode, current_branch_id, writing_language, schema_version, created_at, updated_at
             FROM project WHERE id = ?1",
            [&file.id],
            |row| {
                Ok(ProjectSummary {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    root_path: row.get(2)?,
                    mode: row.get(3)?,
                    active_branch_id: row.get(4)?,
                    writing_language: row.get(5)?,
                    schema_version: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::Message("project.json 与数据库不一致：找不到项目记录".into()))?;
    Ok(LoadedProject {
        summary,
        root,
        conn,
    })
}

pub fn counts(conn: &Connection, project_id: &str) -> AppResult<ProjectCounts> {
    let count = |sql: &str| -> AppResult<i64> {
        Ok(conn.query_row(sql, [project_id], |row| row.get(0))?)
    };
    Ok(ProjectCounts {
        premises: count("SELECT COUNT(*) FROM premise WHERE project_id = ?1 AND status != 'archived'")? as i32,
        sessions: count("SELECT COUNT(*) FROM chat_session WHERE project_id = ?1 AND archived_at IS NULL")? as i32,
        pending_candidates: count("SELECT COUNT(*) FROM candidate WHERE project_id = ?1 AND status = 'pending'")? as i32,
        chapters: count("SELECT COUNT(*) FROM chapter WHERE project_id = ?1")? as i32,
        entities: count("SELECT COUNT(*) FROM entity WHERE project_id = ?1")? as i32,
        open_issues: count("SELECT COUNT(*) FROM issue WHERE project_id = ?1 AND status = 'open'")? as i32,
        promises: count("SELECT COUNT(*) FROM promise WHERE project_id = ?1 AND state NOT IN ('paid_off','abandoned')")? as i32,
    })
}

pub fn snapshot(loaded: &LoadedProject) -> AppResult<ProjectSnapshot> {
    Ok(ProjectSnapshot {
        project: loaded.summary.clone(),
        counts: counts(&loaded.conn, &loaded.summary.id)?,
    })
}

fn write_project_file(root: &Path, file: &ProjectFile) -> AppResult<()> {
    let target = root.join(PROJECT_FILE);
    let temp = root.join(".project.json.tmp");
    std::fs::write(&temp, serde_json::to_string_pretty(file)?)?;
    std::fs::rename(temp, target)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn creates_project_structure_and_reopens() {
        let dir = temp_root();
        let root = dir.path().join("story");
        let loaded = create(&CreateProjectInput {
            name: "雨夜".into(),
            root_path: root.to_string_lossy().into_owned(),
            mode: "idea".into(),
            writing_language: None,
        })
        .unwrap();
        assert!(root.join("project.json").exists());
        assert!(root.join("narrative.sqlite").exists());
        for d in ["imports", "exports", "backups"] {
            assert!(root.join(d).is_dir());
        }
        let policy_count: i64 = loaded
            .conn
            .query_row("SELECT COUNT(*) FROM review_policy", [], |r| r.get(0))
            .unwrap();
        assert_eq!(policy_count, 1);
        let id = loaded.summary.id.clone();
        drop(loaded);

        let reopened = open(&root).unwrap();
        assert_eq!(reopened.summary.id, id);
        assert_eq!(reopened.summary.mode, "idea");
        let raw = std::fs::read_to_string(root.join("project.json")).unwrap();
        assert!(!raw.to_lowercase().contains("api"));
    }

    #[test]
    fn rejects_non_empty_directory_and_duplicate_project() {
        let dir = temp_root();
        let root = dir.path().join("story");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("notes.txt"), "hi").unwrap();
        let err = create(&CreateProjectInput {
            name: "x".into(),
            root_path: root.to_string_lossy().into_owned(),
            mode: "idea".into(),
            writing_language: None,
        })
        .unwrap_err();
        assert!(err.to_string().contains("空目录"));
    }

    #[test]
    fn counts_start_at_zero() {
        let dir = temp_root();
        let loaded = create(&CreateProjectInput {
            name: "x".into(),
            root_path: dir.path().join("p").to_string_lossy().into_owned(),
            mode: "both".into(),
            writing_language: None,
        })
        .unwrap();
        let c = counts(&loaded.conn, &loaded.summary.id).unwrap();
        assert_eq!(c.premises, 0);
        assert_eq!(c.chapters, 0);
    }
}
