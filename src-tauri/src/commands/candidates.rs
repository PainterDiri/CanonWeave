use tauri::State;

use crate::{
    candidates,
    domain::{Candidate, CandidateAcceptResult, ReviewQueue},
    error::IpcResult,
    state::AppState,
};

#[tauri::command]
#[specta::specta]
pub fn review_queue(state: State<'_, AppState>) -> IpcResult<ReviewQueue> {
    state
        .with_project(|loaded| candidates::queue(&loaded.conn, &loaded.summary.id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn candidate_list(
    state: State<'_, AppState>,
    status: Option<String>,
    kind: Option<String>,
) -> IpcResult<Vec<Candidate>> {
    state
        .with_project(|loaded| {
            candidates::list(
                &loaded.conn,
                &loaded.summary.id,
                status.as_deref(),
                kind.as_deref(),
            )
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn candidate_accept(state: State<'_, AppState>, id: String) -> IpcResult<CandidateAcceptResult> {
    let mut guard = state.project.lock().map_err(|_| "项目状态锁已损坏".to_string())?;
    let loaded = guard.as_mut().ok_or("尚未打开项目")?;
    candidates::accept(&mut loaded.conn, &loaded.summary.id, &id).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn candidate_reject(state: State<'_, AppState>, id: String) -> IpcResult<Candidate> {
    state
        .with_project(|loaded| candidates::reject(&loaded.conn, &loaded.summary.id, &id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn candidate_update_payload(
    state: State<'_, AppState>,
    id: String,
    payload_json: String,
) -> IpcResult<Candidate> {
    let payload: serde_json::Value =
        serde_json::from_str(&payload_json).map_err(|e| format!("候选内容不是合法 JSON：{e}"))?;
    state
        .with_project(|loaded| {
            candidates::update_payload(&loaded.conn, &loaded.summary.id, &id, payload)
        })
        .map_err(|e| e.to_string())
}
