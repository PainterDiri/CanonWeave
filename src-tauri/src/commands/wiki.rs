use tauri::State;

use crate::{
    domain::{Candidate, WikiEditInput, WikiPage, WikiRebuildResult},
    error::IpcResult,
    state::AppState,
    wiki,
};

#[tauri::command]
#[specta::specta]
pub fn wiki_list(state: State<'_, AppState>) -> IpcResult<Vec<WikiPage>> {
    state
        .with_project(|loaded| wiki::list(&loaded.conn, &loaded.summary.id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn wiki_rebuild(state: State<'_, AppState>) -> IpcResult<WikiRebuildResult> {
    let mut guard = state.project.lock().map_err(|_| "项目状态锁已损坏".to_string())?;
    let loaded = guard.as_mut().ok_or("尚未打开项目")?;
    wiki::rebuild(&mut loaded.conn, &loaded.summary.id).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn wiki_propose_edit(
    state: State<'_, AppState>,
    input: WikiEditInput,
) -> IpcResult<Candidate> {
    state
        .with_project(|loaded| {
            wiki::propose_edit(&loaded.conn, &loaded.summary.id, &input)
        })
        .map_err(|e| e.to_string())
}
