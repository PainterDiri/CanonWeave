use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tauri::{Emitter, State};

use crate::{
    agent, candidates, chat,
    domain::{
        Candidate, CandidateCaptureInput, ChatDeltaEvent, ChatErrorEvent, ChatFinishedEvent,
        ChatMessage, ChatSendInput, ChatSendResult, ChatSession, ChatSessionCreateInput,
    },
    error::IpcResult,
    llm::{chat_system_prompt, ChatTurn},
    seed,
    state::AppState,
};

#[tauri::command]
#[specta::specta]
pub fn chat_session_create(
    state: State<'_, AppState>,
    input: ChatSessionCreateInput,
) -> IpcResult<ChatSession> {
    state
        .with_project(|loaded| chat::create(&loaded.conn, &loaded.summary.id, &input))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn chat_session_list(
    state: State<'_, AppState>,
    scope: Option<String>,
    include_archived: bool,
) -> IpcResult<Vec<ChatSession>> {
    state
        .with_project(|loaded| {
            chat::list(
                &loaded.conn,
                &loaded.summary.id,
                scope.as_deref(),
                include_archived,
            )
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn chat_session_get(state: State<'_, AppState>, id: String) -> IpcResult<ChatSession> {
    state
        .with_project(|loaded| chat::get(&loaded.conn, &loaded.summary.id, &id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn chat_messages_list(
    sys: State<'_, AppState>,
    session_id: String,
) -> IpcResult<Vec<ChatMessage>> {
    sys.with_project(|loaded| chat::messages(&loaded.conn, &loaded.summary.id, &session_id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn chat_session_update(
    state: State<'_, AppState>,
    id: String,
    title: Option<String>,
    pinned: Option<bool>,
    mode: Option<String>,
    scope: Option<String>,
) -> IpcResult<ChatSession> {
    state
        .with_project(|loaded| {
            chat::update(
                &loaded.conn,
                &loaded.summary.id,
                &id,
                title.as_deref(),
                pinned,
                mode.as_deref(),
                scope.as_deref(),
            )
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn chat_session_archive(state: State<'_, AppState>, id: String) -> IpcResult<()> {
    state
        .with_project(|loaded| chat::archive(&loaded.conn, &loaded.summary.id, &id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn chat_session_unarchive(state: State<'_, AppState>, id: String) -> IpcResult<()> {
    state
        .with_project(|loaded| chat::unarchive(&loaded.conn, &loaded.summary.id, &id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn chat_session_delete(state: State<'_, AppState>, id: String) -> IpcResult<()> {
    state
        .with_project(|loaded| chat::delete(&loaded.conn, &loaded.summary.id, &id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn chat_session_fork(state: State<'_, AppState>, id: String) -> IpcResult<ChatSession> {
    let mut guard = state
        .project
        .lock()
        .map_err(|_| "项目状态锁已损坏".to_string())?;
    let loaded = guard.as_mut().ok_or("尚未打开项目")?;
    chat::fork(&mut loaded.conn, &loaded.summary.id, &id).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn chat_capture(
    sys: State<'_, AppState>,
    input: CandidateCaptureInput,
) -> IpcResult<Candidate> {
    sys.with_project(|loaded| {
        chat::capture(
            &loaded.conn,
            &loaded.summary.id,
            &input.session_id,
            &input.message_id,
            &input.kind,
            &input.text,
        )
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn chat_summary(
    state: State<'_, AppState>,
    session_id: String,
) -> IpcResult<ChatSession> {
    let client = super::seed::require_client(&state).map_err(|e| e.to_string())?;
    let (project_id, messages, current) = {
        let guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
        let loaded = guard.as_ref().ok_or("尚未打开项目")?;
        let pid = loaded.summary.id.clone();
        let messages =
            chat::messages(&loaded.conn, &pid, &session_id).map_err(|e| e.to_string())?;
        let current = chat::get(&loaded.conn, &pid, &session_id).map_err(|e| e.to_string())?;
        (pid, messages, current)
    };
    if messages.is_empty() {
        return Err("会话还没有消息".into());
    }
    let recent: Vec<_> = messages.iter().rev().take(16).rev().collect();
    let mut transcript = String::new();
    for message in recent {
        transcript.push_str(&format!("{}：{}\n", message.role, message.content));
    }
    let raw = client
        .complete(
            &[
                ChatTurn::system(
                    "把以下创作对话压缩成一份滚动摘要。保留：作者已明确的决定、尚未解决的冲突、被否定的方向、下一步。不要新增设定，不要输出建议。300 字以内。",
                ),
                ChatTurn::user(transcript),
            ],
            Some(800),
        )
        .await
        .map_err(|e| e.to_string())?;
    let guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
    let loaded = guard.as_ref().ok_or("尚未打开项目")?;
    let mut summary = raw.trim().to_string();
    if !current.summary.is_empty() {
        summary = format!("{}\n{}", current.summary, summary);
    }
    chat::set_summary(&loaded.conn, &project_id, &session_id, &summary).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn chat_send(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    input: ChatSendInput,
) -> IpcResult<ChatSendResult> {
    if input.content.trim().is_empty() {
        return Err("消息不能为空".into());
    }
    let client = super::seed::require_client(&state).map_err(|e| e.to_string())?;
    let request_id = uuid::Uuid::new_v4().to_string();
    let cancel = Arc::new(AtomicBool::new(false));
    let (project_id, session, messages, premise, project_context) = {
        let guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
        let loaded = guard.as_ref().ok_or("尚未打开项目")?;
        let pid = loaded.summary.id.clone();
        let session =
            chat::get(&loaded.conn, &pid, &input.session_id).map_err(|e| e.to_string())?;
        let user_message = chat::append_message(
            &loaded.conn,
            &pid,
            &input.session_id,
            "user",
            input.content.trim(),
            "",
        )
        .map_err(|e| e.to_string())?;
        let messages = chat::history_window(&loaded.conn, &pid, &input.session_id, 12)
            .map_err(|e| e.to_string())?;
        let premise = seed::list(&loaded.conn, &pid)
            .map_err(|e| e.to_string())?
            .into_iter()
            .next()
            .and_then(|p| serde_json::to_value(p).ok());
        let knowledge_context = crate::knowledge::prompt_context(&loaded.conn, &pid, 30)
            .map_err(|error| error.to_string())?;
        let open_issues: i64 = loaded
            .conn
            .query_row(
                "SELECT COUNT(*) FROM issue WHERE project_id=?1 AND status='open'",
                [&pid],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        let pending_candidates: i64 = loaded
            .conn
            .query_row(
                "SELECT COUNT(*) FROM candidate WHERE project_id=?1 AND status='pending'",
                [&pid],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        let next_actions =
            agent::next_actions(&loaded.conn, &pid).map_err(|error| error.to_string())?;
        let next_action_context = next_actions
            .iter()
            .take(3)
            .map(|action| {
                format!(
                    "- {}：{}（动作 {}）",
                    action.label, action.description, action.action
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let mut promise_stmt = loaded
            .conn
            .prepare(
                "SELECT title, state, target_window FROM promise
                 WHERE project_id=?1 AND state NOT IN ('paid_off','abandoned')
                 ORDER BY updated_at DESC LIMIT 8",
            )
            .map_err(|error| error.to_string())?;
        let promises = promise_stmt
            .query_map([&pid], |row| {
                Ok(format!(
                    "- [{}] {}｜窗口：{}",
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(2)?
                ))
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        drop(promise_stmt);
        let project_context = format!(
            "待处理冲突：{open_issues}\n待确认候选：{pending_candidates}\n\
             未结伏笔：\n{}\n作者确认的知识库：\n{}\n系统建议的下一步（优先从这些动作中引导，但以作者当前问题为主）：\n{}",
            if promises.is_empty() {
                "无".to_string()
            } else {
                promises.join("\n")
            },
            if knowledge_context.is_empty() {
                "无".to_string()
            } else {
                knowledge_context
            },
            if next_action_context.is_empty() {
                "无".to_string()
            } else {
                next_action_context
            }
        );
        let _ = user_message;
        (pid, session, messages, premise, project_context)
    };
    state
        .cancellations
        .lock()
        .map_err(|_| "取消状态锁已损坏".to_string())?
        .insert(request_id.clone(), cancel.clone());

    let constraints: Vec<String> = messages
        .iter()
        .filter(|m| m.pinned)
        .map(|m| m.content.clone())
        .collect();
    let mut turns = vec![ChatTurn::system(chat_system_prompt(
        &session.mode,
        premise.as_ref(),
        &constraints,
    ))];
    if !session.summary.is_empty() {
        turns.push(ChatTurn::system(format!(
            "早前对话摘要（只用于压缩上下文，不是事实）：\n{}",
            session.summary
        )));
    }
    turns.push(ChatTurn::system(format!(
        "当前项目的只读快照。它帮助你定位任务，不替代原文与作者确认事实：\n{project_context}"
    )));
    for message in &messages {
        if message.role == "user" || message.role == "assistant" {
            turns.push(ChatTurn {
                role: message.role.clone(),
                content: message.content.clone(),
            });
        }
    }

    let app_for_delta = app.clone();
    let session_for_delta = input.session_id.clone();
    let request_for_delta = request_id.clone();
    let stream_result = client
        .stream(&turns, cancel, move |delta| {
            let _ = app_for_delta.emit(
                "chat://delta",
                ChatDeltaEvent {
                    session_id: session_for_delta.clone(),
                    request_id: request_for_delta.clone(),
                    delta: delta.to_string(),
                },
            );
        })
        .await;

    if let Ok(mut map) = state.cancellations.lock() {
        map.remove(&request_id);
    }

    match stream_result {
        Ok(text) => {
            let guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
            let loaded = guard.as_ref().ok_or("尚未打开项目")?;
            let assistant = chat::append_message(
                &loaded.conn,
                &project_id,
                &input.session_id,
                "assistant",
                &text,
                &client.model,
            )
            .map_err(|e| e.to_string())?;
            let user_message = chat::messages(&loaded.conn, &project_id, &input.session_id)
                .map_err(|e| e.to_string())?
                .into_iter()
                .rev()
                .find(|m| m.role == "user")
                .ok_or("找不到刚发送的消息")?;
            let session = chat::get(&loaded.conn, &project_id, &input.session_id)
                .map_err(|e| e.to_string())?;
            let _ = app.emit(
                "chat://finished",
                ChatFinishedEvent {
                    session_id: input.session_id.clone(),
                    request_id: request_id.clone(),
                    message: assistant.clone(),
                },
            );
            Ok(ChatSendResult {
                user_message,
                assistant_message: assistant,
                session,
            })
        }
        Err(error) => {
            let message = error.to_string();
            let _ = app.emit(
                "chat://error",
                ChatErrorEvent {
                    session_id: input.session_id,
                    request_id,
                    error: message.clone(),
                },
            );
            Err(message)
        }
    }
}

#[tauri::command]
#[specta::specta]
pub fn chat_cancel(state: State<'_, AppState>, request_id: String) -> IpcResult<()> {
    let map = state
        .cancellations
        .lock()
        .map_err(|_| "取消状态锁已损坏".to_string())?;
    if let Some(flag) = map.get(&request_id) {
        flag.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn chat_candidates(
    state: State<'_, AppState>,
    session_id: String,
) -> IpcResult<Vec<Candidate>> {
    state
        .with_project(|loaded| {
            let pid = &loaded.summary.id;
            let all = candidates::list(&loaded.conn, pid, None, None)?;
            let message_ids: Vec<String> = chat::messages(&loaded.conn, pid, &session_id)?
                .into_iter()
                .map(|m| m.id)
                .collect();
            Ok(all
                .into_iter()
                .filter(|c| {
                    c.source_message_id
                        .as_ref()
                        .map(|id| message_ids.contains(id))
                        .unwrap_or(false)
                })
                .collect())
        })
        .map_err(|e| e.to_string())
}

fn append_message(
    state: &AppState,
    session_id: &str,
    role: &str,
    content: &str,
    model: &str,
) -> IpcResult<ChatMessage> {
    if content.trim().is_empty() {
        return Err("消息不能为空".into());
    }
    if !["user", "assistant"].contains(&role) {
        return Err("只能追加用户或助手消息".into());
    }
    state
        .with_project(|loaded| {
            chat::append_message(
                &loaded.conn,
                &loaded.summary.id,
                session_id,
                role,
                content.trim(),
                model,
            )
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn chat_append_user(
    state: State<'_, AppState>,
    session_id: String,
    content: String,
) -> IpcResult<ChatMessage> {
    append_message(&state, &session_id, "user", &content, "")
}

#[tauri::command]
#[specta::specta]
pub fn chat_append_assistant(
    state: State<'_, AppState>,
    session_id: String,
    content: String,
) -> IpcResult<ChatMessage> {
    append_message(&state, &session_id, "assistant", &content, "agent-workflow")
}
