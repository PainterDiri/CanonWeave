use tauri::State;

use crate::{
    domain::{CharacterDetail, CharacterInput, Entity},
    entities,
    error::IpcResult,
    state::AppState,
};

#[tauri::command]
#[specta::specta]
pub fn entity_list(
    state: State<'_, AppState>,
    entity_type: Option<String>,
) -> IpcResult<Vec<Entity>> {
    state
        .with_project(|loaded| {
            entities::list(&loaded.conn, &loaded.summary.id, entity_type.as_deref())
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn character_list(state: State<'_, AppState>) -> IpcResult<Vec<CharacterDetail>> {
    state
        .with_project(|loaded| entities::characters(&loaded.conn, &loaded.summary.id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn character_save(
    state: State<'_, AppState>,
    input: CharacterInput,
) -> IpcResult<CharacterDetail> {
    let mut guard = state.project.lock().map_err(|_| "项目状态锁已损坏".to_string())?;
    let loaded = guard.as_mut().ok_or("尚未打开项目")?;
    entities::save_character(&mut loaded.conn, &loaded.summary.id, &input).map_err(|e| e.to_string())
}
