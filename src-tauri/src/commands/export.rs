use tauri::State;

use crate::{
    domain::{ExportInput, ExportResult},
    error::IpcResult,
    export,
    state::AppState,
};

#[tauri::command]
#[specta::specta]
pub fn export_project(
    state: State<'_, AppState>,
    input: ExportInput,
) -> IpcResult<ExportResult> {
    state
        .with_project(|loaded| {
            export::export(
                &loaded.conn,
                &loaded.root,
                &loaded.summary.id,
                &input,
            )
        })
        .map_err(|e| e.to_string())
}
