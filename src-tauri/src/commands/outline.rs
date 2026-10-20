use serde_json::Value;
use tauri::State;

use crate::{
    domain::{OutlineNode, OutlineStatus, OutlineUpdateInput},
    error::IpcResult,
    llm::{outline_system_prompt, outline_user_prompt, parse_json_loose, ChatTurn},
    outline, seed,
    state::AppState,
};

#[tauri::command]
#[specta::specta]
pub fn outline_list(state: State<'_, AppState>) -> IpcResult<Vec<OutlineNode>> {
    state
        .with_project(|loaded| outline::list(&loaded.conn, &loaded.summary.id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn outline_generate(state: State<'_, AppState>) -> IpcResult<Vec<OutlineNode>> {
    let client = super::seed::require_client(&state).map_err(|e| e.to_string())?;
    let (project_id, premise) = {
        let guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
        let loaded = guard.as_ref().ok_or("尚未打开项目")?;
        let premise = seed::active(&loaded.conn, &loaded.summary.id)
            .map_err(|e| e.to_string())?
            .ok_or("请先确认种子卡，再生成十章骨架")?;
        (
            loaded.summary.id.clone(),
            serde_json::to_value(premise).map_err(|e| e.to_string())?,
        )
    };
    let raw = client
        .complete(
            &[
                ChatTurn::system(outline_system_prompt()),
                ChatTurn::user(outline_user_prompt(&premise)),
            ],
            Some(6000),
        )
        .await
        .map_err(|e| e.to_string())?;
    let mut items: Vec<Value> = parse_json_loose(&raw).map_err(|e| e.to_string())?;
    if items.len() < 3 {
        return Err("模型返回的骨架少于 3 章，无法开始写作，请重试".into());
    }
    items.truncate(10);
    let mut guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
    let loaded = guard.as_mut().ok_or("尚未打开项目")?;
    outline::insert_generated(&mut loaded.conn, &project_id, &items, None)
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn outline_update(state: State<'_, AppState>, input: OutlineUpdateInput) -> IpcResult<OutlineNode> {
    state
        .with_project(|loaded| outline::update(&loaded.conn, &loaded.summary.id, &input))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn outline_confirm(state: State<'_, AppState>, ids: Vec<String>) -> IpcResult<Vec<OutlineNode>> {
    let mut guard = state.project.lock().map_err(|_| "项目状态锁已损坏".to_string())?;
    let loaded = guard.as_mut().ok_or("尚未打开项目")?;
    outline::confirm(&mut loaded.conn, &loaded.summary.id, &ids).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn outline_drop(state: State<'_, AppState>, id: String) -> IpcResult<()> {
    state
        .with_project(|loaded| outline::drop_node(&loaded.conn, &loaded.summary.id, &id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn outline_status(state: State<'_, AppState>) -> IpcResult<OutlineStatus> {
    state
        .with_project(|loaded| {
            let has = outline::has_skeleton(&loaded.conn, &loaded.summary.id)?;
            let confirmed = outline::confirmed_count(&loaded.conn, &loaded.summary.id)?;
            Ok(OutlineStatus {
                has_skeleton: has,
                confirmed,
            })
        })
        .map_err(|e| e.to_string())
}
