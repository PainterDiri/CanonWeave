use tauri::State;

use crate::{
    domain::{
        StyleCheck, StyleCompare, StyleMetrics, StylePreset, StyleProfile, StyleProfileInput,
        StyleSample, StyleSampleInput,
    },
    error::IpcResult,
    state::AppState,
    style,
};

#[tauri::command]
#[specta::specta]
pub fn style_preset_list() -> Vec<StylePreset> {
    style::presets().clone()
}

#[tauri::command]
#[specta::specta]
pub fn style_profile_list(state: State<'_, AppState>) -> IpcResult<Vec<StyleProfile>> {
    state
        .with_project(|loaded| style::list(&loaded.conn, &loaded.summary.id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn style_profile_get(state: State<'_, AppState>, id: String) -> IpcResult<StyleProfile> {
    state
        .with_project(|loaded| style::get(&loaded.conn, &loaded.summary.id, &id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn style_profile_save(
    state: State<'_, AppState>,
    input: StyleProfileInput,
) -> IpcResult<StyleProfile> {
    state
        .with_project(|loaded| style::save(&loaded.conn, &loaded.summary.id, &input))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn style_profile_apply_preset(
    state: State<'_, AppState>,
    preset_id: String,
    name: Option<String>,
) -> IpcResult<StyleProfile> {
    state
        .with_project(|loaded| {
            style::save_preset(&loaded.conn, &loaded.summary.id, &preset_id, name.as_deref())
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn style_sample_add(
    state: State<'_, AppState>,
    input: StyleSampleInput,
) -> IpcResult<StyleSample> {
    state
        .with_project(|loaded| style::add_sample(&loaded.conn, &loaded.summary.id, &input))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn style_sample_list(
    state: State<'_, AppState>,
    profile_id: Option<String>,
) -> IpcResult<Vec<StyleSample>> {
    state
        .with_project(|loaded| {
            style::samples(&loaded.conn, &loaded.summary.id, profile_id.as_deref())
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn style_analyze(text: String) -> StyleMetrics {
    style::analyze(&text)
}

#[tauri::command]
#[specta::specta]
pub fn style_contract_text(state: State<'_, AppState>, profile_id: String) -> IpcResult<String> {
    state
        .with_project(|loaded| {
            let profile = style::get(&loaded.conn, &loaded.summary.id, &profile_id)?;
            Ok(style::contract(&profile))
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn style_check(
    state: State<'_, AppState>,
    text: String,
    profile_id: String,
) -> IpcResult<StyleCheck> {
    state
        .with_project(|loaded| {
            let profile = style::get(&loaded.conn, &loaded.summary.id, &profile_id)?;
            Ok(style::check(&text, &profile))
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn style_compare(left_text: String, right_text: String) -> StyleCompare {
    style::compare(&left_text, &right_text)
}

#[tauri::command]
#[specta::specta]
pub fn style_calibrate(state: State<'_, AppState>, profile_id: String) -> IpcResult<StyleProfile> {
    state
        .with_project(|loaded| {
            style::calibrate(&loaded.conn, &loaded.summary.id, &profile_id)
        })
        .map_err(|e| e.to_string())
}
