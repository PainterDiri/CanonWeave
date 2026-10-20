use tauri::State;

use crate::{
    domain::{
        CharacterVoiceProfile, CharacterVoiceSample, VoiceCheck, VoiceSampleInput,
        VoiceThresholdInput,
    },
    error::IpcResult,
    state::AppState,
    voice,
};

#[tauri::command]
#[specta::specta]
pub fn voice_sample_list(
    state: State<'_, AppState>,
    character_id: String,
) -> IpcResult<Vec<CharacterVoiceSample>> {
    state
        .with_project(|loaded| {
            voice::samples(&loaded.conn, &loaded.summary.id, &character_id)
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn voice_sample_add(
    state: State<'_, AppState>,
    input: VoiceSampleInput,
) -> IpcResult<CharacterVoiceSample> {
    state
        .with_project(|loaded| voice::add_sample(&loaded.conn, &loaded.summary.id, &input))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn voice_profile_get(
    state: State<'_, AppState>,
    character_id: String,
) -> IpcResult<CharacterVoiceProfile> {
    state
        .with_project(|loaded| voice::profile(&loaded.conn, &loaded.summary.id, &character_id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn voice_thresholds_save(
    state: State<'_, AppState>,
    input: VoiceThresholdInput,
) -> IpcResult<CharacterVoiceProfile> {
    state
        .with_project(|loaded| voice::save_thresholds(&loaded.conn, &loaded.summary.id, &input))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn voice_check(
    state: State<'_, AppState>,
    character_id: String,
    text: String,
    scene_id: Option<String>,
) -> IpcResult<VoiceCheck> {
    state
        .with_project(|loaded| {
            voice::check(
                &loaded.conn,
                &loaded.summary.id,
                &character_id,
                &text,
                scene_id.as_deref(),
            )
        })
        .map_err(|e| e.to_string())
}
