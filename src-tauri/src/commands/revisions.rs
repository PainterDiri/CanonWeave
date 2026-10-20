use tauri::State;

use crate::{
    domain::{DraftAcceptResult, PreferenceInput, RevisionCompare, RevisionRecord},
    error::IpcResult,
    revisions,
    state::AppState,
};

#[tauri::command]
#[specta::specta]
pub fn revision_list(
    state: State<'_, AppState>,
    chapter_id: Option<String>,
) -> IpcResult<Vec<RevisionRecord>> {
    state
        .with_project(|loaded| {
            revisions::list(&loaded.conn, &loaded.summary.id, chapter_id.as_deref())
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn revision_compare(
    state: State<'_, AppState>,
    from_id: String,
    to_id: String,
) -> IpcResult<RevisionCompare> {
    state
        .with_project(|loaded| {
            revisions::compare(&loaded.conn, &loaded.summary.id, &from_id, &to_id)
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn revision_rollback(
    state: State<'_, AppState>,
    revision_id: String,
) -> IpcResult<DraftAcceptResult> {
    let mut guard = state.project.lock().map_err(|_| "项目状态锁已损坏".to_string())?;
    let loaded = guard.as_mut().ok_or("尚未打开项目")?;
    revisions::rollback(&mut loaded.conn, &loaded.summary.id, &revision_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn preference_save(
    state: State<'_, AppState>,
    input: PreferenceInput,
) -> IpcResult<()> {
    state
        .with_project(|loaded| crate::preferences::save(&loaded.conn, &loaded.summary.id, &input))
        .map_err(|e| e.to_string())
}
