use tauri::State;

use crate::{
    domain::{CreateProjectInput, ProjectSnapshot},
    error::IpcResult,
    project,
    state::AppState,
};

#[tauri::command]
#[specta::specta]
pub fn project_create(
    state: State<'_, AppState>,
    input: CreateProjectInput,
) -> IpcResult<ProjectSnapshot> {
    let loaded = project::create(&input).map_err(|e| e.to_string())?;
    let snapshot = project::snapshot(&loaded).map_err(|e| e.to_string())?;
    let mut guard = state
        .project
        .lock()
        .map_err(|_| "项目状态锁已损坏".to_string())?;
    *guard = Some(loaded);
    Ok(snapshot)
}

#[tauri::command]
#[specta::specta]
pub fn project_open(state: State<'_, AppState>, root_path: String) -> IpcResult<ProjectSnapshot> {
    let loaded = project::open(std::path::Path::new(&root_path)).map_err(|e| e.to_string())?;
    let snapshot = project::snapshot(&loaded).map_err(|e| e.to_string())?;
    let mut guard = state
        .project
        .lock()
        .map_err(|_| "项目状态锁已损坏".to_string())?;
    *guard = Some(loaded);
    Ok(snapshot)
}

#[tauri::command]
#[specta::specta]
pub fn project_close(state: State<'_, AppState>) -> IpcResult<()> {
    let mut guard = state
        .project
        .lock()
        .map_err(|_| "项目状态锁已损坏".to_string())?;
    *guard = None;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn project_state(state: State<'_, AppState>) -> IpcResult<Option<ProjectSnapshot>> {
    let guard = state
        .project
        .lock()
        .map_err(|_| "项目状态锁已损坏".to_string())?;
    guard
        .as_ref()
        .map(project::snapshot)
        .transpose()
        .map_err(|e| e.to_string())
}
