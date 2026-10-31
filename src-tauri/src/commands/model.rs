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
    // 只返回非机密的“已保存密钥”标记；真正读取钥匙串留到第一次调用模型时，启动不弹授权框。
    state.model_store.load()
}

#[tauri::command]
#[specta::specta]
pub fn model_save(state: State<'_, AppState>, input: ModelConfigInput) -> IpcResult<ModelConfig> {
    let updated_key = input
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(str::to_owned);
    let config = state.model_store.save(&input).map_err(|e| e.to_string())?;
    if let Some(key) = updated_key {
        state.api_keys.set(key).map_err(|e| e.to_string())?;
    }
    if let Ok(mut guard) = state.model.lock() {
        *guard = config.clone();
    }
    Ok(config)
}

#[tauri::command]
#[specta::specta]
pub async fn model_test(state: State<'_, AppState>) -> IpcResult<ModelTestResult> {
    let config = state.model_store.load();
    let key = state.api_key().map_err(|e| e.to_string())?;
    Ok(model::test_connection(&config, key.as_deref()).await)
}
