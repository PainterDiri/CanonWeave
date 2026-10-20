use tauri::State;

use crate::{
    domain::PreferenceSummary,
    error::IpcResult,
    preferences,
    state::AppState,
};

#[tauri::command]
#[specta::specta]
pub fn preference_summary(
    state: State<'_, AppState>,
    scene_id: Option<String>,
) -> IpcResult<PreferenceSummary> {
    state
        .with_project(|loaded| {
            preferences::summary(
                &loaded.conn,
                &loaded.summary.id,
                scene_id.as_deref(),
            )
        })
        .map_err(|e| e.to_string())
}
