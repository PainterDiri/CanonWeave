use tauri::State;

use crate::{
    domain::{KnowledgeDomain, KnowledgeDomainInput, KnowledgeEntry, KnowledgeEntryInput},
    error::IpcResult,
    knowledge,
    state::AppState,
};

#[tauri::command]
#[specta::specta]
pub fn knowledge_domains(state: State<'_, AppState>) -> IpcResult<Vec<KnowledgeDomain>> {
    state
        .with_project(|loaded| knowledge::domains(&loaded.conn, &loaded.summary.id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn knowledge_domain_create(
    state: State<'_, AppState>,
    input: KnowledgeDomainInput,
) -> IpcResult<KnowledgeDomain> {
    state
        .with_project(|loaded| knowledge::create_domain(&loaded.conn, &loaded.summary.id, &input))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn knowledge_entries(
    state: State<'_, AppState>,
    domain_id: Option<String>,
) -> IpcResult<Vec<KnowledgeEntry>> {
    state
        .with_project(|loaded| {
            knowledge::entries(&loaded.conn, &loaded.summary.id, domain_id.as_deref())
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn knowledge_entry_save(
    state: State<'_, AppState>,
    input: KnowledgeEntryInput,
) -> IpcResult<KnowledgeEntry> {
    let mut guard = state
        .project
        .lock()
        .map_err(|_| "项目状态锁已损坏".to_string())?;
    let loaded = guard.as_mut().ok_or("尚未打开项目")?;
    knowledge::save_entry(&mut loaded.conn, &loaded.summary.id, &input).map_err(|e| e.to_string())
}
