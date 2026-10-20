use tauri::State;

use crate::{
    domain::{PayoffCheck, PromiseInput, PromisePayoffInput, PromiseRecord, PromiseStateInput},
    error::IpcResult,
    promises,
    state::AppState,
};

#[tauri::command]
#[specta::specta]
pub fn promise_list(state: State<'_, AppState>) -> IpcResult<Vec<PromiseRecord>> {
    state
        .with_project(|loaded| promises::list(&loaded.conn, &loaded.summary.id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn promise_save(state: State<'_, AppState>, input: PromiseInput) -> IpcResult<PromiseRecord> {
    state
        .with_project(|loaded| promises::save(&loaded.conn, &loaded.summary.id, &input))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn promise_transition(
    state: State<'_, AppState>,
    input: PromiseStateInput,
) -> IpcResult<PromiseRecord> {
    state
        .with_project(|loaded| promises::transition(&loaded.conn, &loaded.summary.id, &input))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn promise_payoff_check(
    state: State<'_, AppState>,
    input: PromisePayoffInput,
) -> IpcResult<PayoffCheck> {
    state
        .with_project(|loaded| {
            promises::payoff_check(&loaded.conn, &loaded.summary.id, &input)
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn promise_payoff(
    state: State<'_, AppState>,
    input: PromisePayoffInput,
) -> IpcResult<PromiseRecord> {
    state
        .with_project(|loaded| promises::payoff(&loaded.conn, &loaded.summary.id, &input))
        .map_err(|e| e.to_string())
}
