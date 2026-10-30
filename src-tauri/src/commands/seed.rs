use serde_json::Value;
use tauri::State;

use crate::{
    chat,
    domain::{
        ChatSessionCreateInput, Premise, PremiseInput, PremiseSources, SeedGenerateInput,
        SeedGenerateResult,
    },
    error::{AppError, IpcResult},
    llm::{
        parse_json_loose, seed_compact_system_prompt, seed_system_prompt, seed_user_prompt,
        ChatTurn, LlmClient,
    },
    seed,
    state::AppState,
};

fn client(state: &AppState) -> Result<LlmClient, String> {
    let config = state.model_store.load();
    let key = state
        .api_key()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "尚未保存 API Key，请先在设置中填写并测试连接".to_string())?;
    LlmClient::new(&config, key).map_err(|e| e.to_string())
}

async fn generate_directions(
    client: &LlmClient,
    logline: &str,
    current: Option<&Value>,
) -> Result<String, String> {
    let refinement = current
        .map(|premise| {
            format!(
                "\n\n当前种子卡：\n{premise}\n\n作者这次的修改意见：\n{logline}\n\n\
                 请保留作者明确满意的部分，只针对反馈重做不足字段，并给出三个可替换的强化方向。"
            )
        })
        .unwrap_or_else(|| format!("\n\n作者灵感：\n{logline}"));
    let budget = client.max_tokens.max(4096);
    let first = client
        .complete_with_meta(
            &[
                ChatTurn::system(seed_system_prompt()),
                ChatTurn::user(if current.is_some() {
                    refinement.clone()
                } else {
                    seed_user_prompt(logline)
                }),
            ],
            Some(budget),
        )
        .await
        .map_err(|e| e.to_string())?;
    if !first.truncated {
        if let Ok(directions) = parse_json_loose::<Vec<Value>>(&first.text) {
            if directions.len() >= 3 {
                return Ok(first.text);
            }
        }
    }

    let second = client
        .complete_with_meta(
            &[
                ChatTurn::system(seed_compact_system_prompt()),
                ChatTurn::user(format!(
                    "{refinement}\n\n请只输出 3 项精简 JSON，确保数组完整闭合。"
                )),
            ],
            Some(budget),
        )
        .await
        .map_err(|e| e.to_string())?;
    if second.truncated {
        return Err(format!(
            "模型输出在 JSON 完成前达到 token 上限（{}）。已自动尝试精简重试；请在设置中提高 Max tokens，或换用支持更长输出的模型。",
            second.finish_reason.as_deref().unwrap_or("length")
        ));
    }
    let directions = parse_json_loose::<Vec<Value>>(&second.text).map_err(|error| {
        format!("模型两次都没有返回完整的三种走向。请重试或换用更稳定的模型。原始错误：{error}")
    })?;
    if directions.len() < 3 {
        return Err(format!(
            "模型只返回了 {} 个完整走向，请重试或换用更稳定的模型",
            directions.len()
        ));
    }
    Ok(second.text)
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
    let (project_id, session_id, user_message_id, current_premise) = {
        let guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
        let loaded = guard.as_ref().ok_or("尚未打开项目")?;
        let pid = loaded.summary.id.clone();
        let session = match input.session_id.as_deref() {
            Some(session_id) => {
                chat::get(&loaded.conn, &pid, session_id).map_err(|e| e.to_string())?
            }
            None => {
                let sessions = chat::list(&loaded.conn, &pid, Some("seed"), false)
                    .map_err(|e| e.to_string())?;
                match sessions.into_iter().next() {
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
                }
            }
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
        let current_premise = seed::list(&loaded.conn, &pid)
            .map_err(|e| e.to_string())?
            .into_iter()
            .next()
            .map(|premise| serde_json::to_value(premise))
            .transpose()
            .map_err(|e| e.to_string())?;
        (pid, session.id, message.id, current_premise)
    };

    let raw = generate_directions(&client, input.logline.trim(), current_premise.as_ref()).await?;
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

/// Agent harness 调用：生成三个方向并写入种子卡候选，不往会话里追加消息。
pub async fn generate_seed_cards(
    state: &State<'_, AppState>,
    logline: &str,
    source_message_id: Option<&str>,
) -> IpcResult<Vec<Premise>> {
    let logline = logline.trim();
    if logline.chars().count() < 4 {
        return Err("请至少写一个具体的画面或冲突".into());
    }
    let client = client(state)?;
    let current_premise = {
        let guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
        let loaded = guard.as_ref().ok_or("尚未打开项目")?;
        seed::list(&loaded.conn, &loaded.summary.id)
            .map_err(|e| e.to_string())?
            .into_iter()
            .next()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|e| e.to_string())?
    };
    let raw = generate_directions(&client, logline, current_premise.as_ref()).await?;
    let directions: Vec<Value> = parse_json_loose(&raw).map_err(|e| e.to_string())?;
    let mut guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
    let loaded = guard.as_mut().ok_or("尚未打开项目")?;
    let cards = seed::insert_directions(&mut loaded.conn, &loaded.summary.id, &directions, source_message_id)
        .map_err(|e| e.to_string())?;
    if cards.is_empty() {
        return Err("模型返回的走向缺少一句话故事，请重试".into());
    }
    Ok(cards)
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
    let key = state
        .api_key()?
        .ok_or_else(|| AppError::Message("尚未保存 API Key，请先在设置中填写并测试连接".into()))?;
    LlmClient::new(&config, key)
}
