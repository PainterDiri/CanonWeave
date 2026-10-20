use tauri::State;

use crate::{
    domain::{MotifCheck, MotifRecord},
    error::IpcResult,
    motifs,
    state::AppState,
};

#[tauri::command]
#[specta::specta]
pub fn motif_list(state: State<'_, AppState>) -> IpcResult<Vec<MotifRecord>> {
    state
        .with_project(|loaded| motifs::list(&loaded.conn, &loaded.summary.id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn motif_density_check(
    state: State<'_, AppState>,
    scene_id: String,
    text: String,
) -> IpcResult<MotifCheck> {
    state
        .with_project(|loaded| {
            motifs::check(&loaded.conn, &loaded.summary.id, &scene_id, &text)
        })
        .map_err(|e| e.to_string())
}
