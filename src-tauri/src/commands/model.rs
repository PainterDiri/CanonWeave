use tauri::State;

use crate::{
    domain::{ModelConfig, ModelConfigInput, ModelTestResult},
    error::IpcResult,
    model,
    state::AppState,
};

#[tauri::command]
#[specta::specta]
pub fn model_get(state: State<'_, AppState>) -> ModelConfig {
    state.model_store.load()
}

#[tauri::command]
#[specta::specta]
pub fn model_save(state: State<'_, AppState>, input: ModelConfigInput) -> IpcResult<ModelConfig> {
    let config = state.model_store.save(&input).map_err(|e| e.to_string())?;
    if let Ok(mut guard) = state.model.lock() {
        *guard = config.clone();
    }
    Ok(config)
}

#[tauri::command]
#[specta::specta]
pub async fn model_test(state: State<'_, AppState>) -> IpcResult<ModelTestResult> {
    let config = state.model_store.load();
    Ok(model::test_connection(&config).await)
}
