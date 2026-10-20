use std::path::Path;

use serde_json::Value;
use tauri::State;

use crate::{
    candidates,
    domain::{ExtractionResult, ImportResult},
    error::IpcResult,
    import,
    llm::{extraction_system_prompt, extraction_user_prompt, parse_json_loose, ChatTurn},
    state::AppState,
};

#[tauri::command]
#[specta::specta]
pub fn document_import(state: State<'_, AppState>, path: String) -> IpcResult<ImportResult> {
    let mut guard = state.project.lock().map_err(|_| "项目状态锁已损坏".to_string())?;
    let loaded = guard.as_mut().ok_or("尚未打开项目")?;
    import::import_document(
        &loaded.root,
        &mut loaded.conn,
        &loaded.summary.id,
        Path::new(&path),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn document_list(
    state: State<'_, AppState>,
) -> IpcResult<Vec<crate::domain::SourceDocument>> {
    state
        .with_project(|loaded| {
            let mut stmt = loaded.conn.prepare(
                "SELECT id, path, hash, type, title, word_count, imported_at FROM source_document
                 WHERE project_id=?1 ORDER BY imported_at DESC",
            )?;
            let rows = stmt
                .query_map([&loaded.summary.id], |row| {
                    Ok(crate::domain::SourceDocument {
                        id: row.get(0)?,
                        path: row.get(1)?,
                        hash: row.get(2)?,
                        doc_type: row.get(3)?,
                        title: row.get(4)?,
                        word_count: row.get(5)?,
                        imported_at: row.get(6)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .map_err(|e| e.to_string())
}

fn exists_entity(conn: &rusqlite::Connection, project_id: &str, name: &str) -> crate::error::AppResult<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM entity WHERE project_id=?1 AND canonical_name=?2)",
        rusqlite::params![project_id, name],
        |row| row.get(0),
    )?)
}

fn has_duplicate(candidates: &[crate::domain::Candidate], kind: &str, key: &str) -> bool {
    candidates.iter().any(|candidate| {
        candidate.kind == kind
            && candidate
                .payload
                .get("canonicalName")
                .and_then(Value::as_str)
                .map(|v| v == key)
                .unwrap_or(false)
    })
}

fn has_claim_duplicate(
    conn: &rusqlite::Connection,
    project_id: &str,
    subject: &str,
    predicate: &str,
    object: &str,
) -> crate::error::AppResult<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM claim WHERE project_id=?1 AND subject=?2 AND predicate=?3 AND object=?4)",
        rusqlite::params![project_id, subject, predicate, object],
        |row| row.get(0),
    )?)
}

#[tauri::command]
#[specta::specta]
pub async fn extraction_run(state: State<'_, AppState>, document_id: String) -> IpcResult<ExtractionResult> {
    let client = super::seed::require_client(&state).map_err(|e| e.to_string())?;
    let (project_id, document, chapters) = {
        let guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
        let loaded = guard.as_ref().ok_or("尚未打开项目")?;
        let document = import::document(&loaded.conn, &loaded.summary.id, &document_id)
            .map_err(|e| e.to_string())?;
        let mut chapters = import::chapter_texts(&loaded.conn, &loaded.summary.id, &document_id)
            .map_err(|e| e.to_string())?;
        chapters.sort_by_key(|(number, _, _)| *number);
        (loaded.summary.id.clone(), document, chapters)
    };
    if chapters.is_empty() {
        return Err("该文档没有可抽取的章节".into());
    }
    let mut created = 0;
    for (number, chapter_id, text) in chapters.iter().take(20) {
        let raw = client
            .complete(
                &[
                    ChatTurn::system(extraction_system_prompt()),
                    ChatTurn::user(extraction_user_prompt(&format!("第 {number} 章"), text)),
                ],
                Some(6000),
            )
            .await
            .map_err(|e| e.to_string())?;
        let extracted: Value = parse_json_loose(&raw).map_err(|e| e.to_string())?;
        let mut guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
        let loaded = guard.as_mut().ok_or("尚未打开项目")?;
        let pending = candidates::list(&loaded.conn, &project_id, Some("pending"), None)
            .map_err(|e| e.to_string())?;
        let span = format!("{}:chapter:{}", document.id, number);
        let _ = chapter_id;
        if let Some(entities) = extracted.get("entities").and_then(Value::as_array) {
            for entity in entities {
                let name = entity
                    .get("canonicalName")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim();
                if name.is_empty()
                    || exists_entity(&loaded.conn, &project_id, name).map_err(|e| e.to_string())?
                    || has_duplicate(&pending, "entity", name)
                {
                    continue;
                }
                let quote = entity.get("quote").and_then(Value::as_str).unwrap_or_default();
                candidates::insert(
                    &loaded.conn,
                    &project_id,
                    "entity",
                    entity.clone(),
                    Some(&format!("{span}｜{quote}")),
                    None,
                    0.65,
                )
                .map_err(|e| e.to_string())?;
                created += 1;
            }
        }
        if let Some(claims) = extracted.get("claims").and_then(Value::as_array) {
            for claim in claims {
                let subject = claim.get("subject").and_then(Value::as_str).unwrap_or_default();
                let predicate = claim.get("predicate").and_then(Value::as_str).unwrap_or_default();
                let object = claim.get("object").and_then(Value::as_str).unwrap_or_default();
                if subject.trim().is_empty() || predicate.trim().is_empty() {
                    continue;
                }
                if has_claim_duplicate(&loaded.conn, &project_id, subject, predicate, object)
                    .map_err(|e| e.to_string())?
                {
                    continue;
                }
                let quote = claim.get("quote").and_then(Value::as_str).unwrap_or_default();
                candidates::insert(
                    &loaded.conn,
                    &project_id,
                    "claim",
                    claim.clone(),
                    Some(&format!("{span}｜{quote}")),
                    None,
                    0.6,
                )
                .map_err(|e| e.to_string())?;
                created += 1;
            }
        }
        for (key, kind, confidence) in [
            ("events", "event", 0.6),
            ("relations", "relation", 0.55),
            ("promises", "promise", 0.6),
        ] {
            if let Some(items) = extracted.get(key).and_then(Value::as_array) {
                for item in items {
                    let quote = item.get("quote").and_then(Value::as_str).unwrap_or_default();
                    candidates::insert(
                        &loaded.conn,
                        &project_id,
                        kind,
                        item.clone(),
                        Some(&format!("{span}｜{quote}")),
                        None,
                        confidence,
                    )
                    .map_err(|e| e.to_string())?;
                    created += 1;
                }
            }
        }
    }
    Ok(ExtractionResult {
        candidates_created: created,
    })
}
