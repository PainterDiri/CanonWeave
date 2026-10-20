use serde_json::Value;
use tauri::State;

use crate::{
    chat,
    domain::{
        ChatSessionCreateInput, Premise, PremiseInput, PremiseSources, SeedGenerateInput,
        SeedGenerateResult,
    },
    error::{AppError, IpcResult},
    llm::{parse_json_loose, seed_system_prompt, seed_user_prompt, ChatTurn, LlmClient},
    model, seed,
    state::AppState,
};

fn client(state: &AppState) -> Result<LlmClient, String> {
    let config = state.model_store.load();
    let key = model::load_api_key()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "尚未保存 API Key，请先在设置中填写并测试连接".to_string())?;
    LlmClient::new(&config, key).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn seed_generate(
    state: State<'_, AppState>,
    input: SeedGenerateInput,
) -> IpcResult<SeedGenerateResult> {
    if input.logline.trim().chars().count() < 4 {
        return Err("请至少写一个具体的画面或冲突".into());
    }
    let client = client(&state)?;
    let (project_id, session_id, user_message_id) = {
        let guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
        let loaded = guard.as_ref().ok_or("尚未打开项目")?;
        let pid = loaded.summary.id.clone();
        let sessions = chat::list(&loaded.conn, &pid, Some("seed"), false).map_err(|e| e.to_string())?;
        let session = match sessions.into_iter().next() {
            Some(session) => session,
            None => chat::create(
                &loaded.conn,
                &pid,
                &ChatSessionCreateInput {
                    scope: "seed".into(),
                    mode: "diverge".into(),
                    title: Some("灵感孵化".into()),
                    scene_id: None,
                    branch_id: None,
                    parent_session_id: None,
                },
            )
            .map_err(|e| e.to_string())?,
        };
        let message = chat::append_message(
            &loaded.conn,
            &pid,
            &session.id,
            "user",
            input.logline.trim(),
            "",
        )
        .map_err(|e| e.to_string())?;
        (pid, session.id, message.id)
    };

    let raw = client
        .complete(
            &[
                ChatTurn::system(seed_system_prompt()),
                ChatTurn::user(seed_user_prompt(input.logline.trim())),
            ],
            Some(4096),
        )
        .await
        .map_err(|e| e.to_string())?;
    let directions: Vec<Value> = parse_json_loose(&raw).map_err(|e| e.to_string())?;

    let mut guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
    let loaded = guard.as_mut().ok_or("尚未打开项目")?;
    let assistant = chat::append_message(
        &loaded.conn,
        &project_id,
        &session_id,
        "assistant",
        &raw,
        &client.model,
    )
    .map_err(|e| e.to_string())?;
    let cards = seed::insert_directions(
        &mut loaded.conn,
        &project_id,
        &directions,
        Some(&assistant.id),
    )
    .map_err(|e| e.to_string())?;
    if cards.is_empty() {
        return Err("模型返回的走向缺少一句话故事，请重试".into());
    }
    let _ = user_message_id;
    Ok(SeedGenerateResult { directions: cards })
}

#[tauri::command]
#[specta::specta]
pub fn premise_list(state: State<'_, AppState>) -> IpcResult<Vec<Premise>> {
    state
        .with_project(|loaded| seed::list(&loaded.conn, &loaded.summary.id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn premise_active(state: State<'_, AppState>) -> IpcResult<Option<Premise>> {
    state
        .with_project(|loaded| seed::active(&loaded.conn, &loaded.summary.id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn premise_update(
    state: State<'_, AppState>,
    id: String,
    input: PremiseInput,
) -> IpcResult<Premise> {
    state
        .with_project(|loaded| seed::update(&loaded.conn, &loaded.summary.id, &id, &input))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn premise_confirm(state: State<'_, AppState>, id: String) -> IpcResult<Premise> {
    let mut guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
    let loaded = guard.as_mut().ok_or("尚未打开项目")?;
    seed::confirm(&mut loaded.conn, &loaded.summary.id, &id).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn premise_archive(state: State<'_, AppState>, id: String) -> IpcResult<()> {
    state
        .with_project(|loaded| seed::archive(&loaded.conn, &loaded.summary.id, &id))
        .map_err(|e| e.to_string())
}

/// 供 UI 展示“这条来自哪条消息”，不返回密钥等敏感信息。
#[tauri::command]
#[specta::specta]
pub fn premise_debug_sources(state: State<'_, AppState>, id: String) -> IpcResult<PremiseSources> {
    let premise = state
        .with_project(|loaded| seed::get(&loaded.conn, &loaded.summary.id, &id))
        .map_err(|e| e.to_string())?;
    Ok(PremiseSources {
        field_sources: premise.field_sources,
        source_message_id: premise.source_message_id,
    })
}

pub fn require_client(state: &AppState) -> Result<LlmClient, AppError> {
    let config = state.model_store.load();
    let key = model::load_api_key()?
        .ok_or_else(|| AppError::Message("尚未保存 API Key，请先在设置中填写并测试连接".into()))?;
    LlmClient::new(&config, key)
}
