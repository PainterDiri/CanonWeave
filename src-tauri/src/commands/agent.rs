use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tauri::{Emitter, State};

use crate::{
    agent,
    domain::{
        AgentProgressEvent, AgentRun, AgentRunCreateInput, AgentRunDetail, AgentToolDefinition,
        FoundationReport,
    },
    error::IpcResult,
    llm::{agent_synthesis_system_prompt, agent_synthesis_user_prompt, ChatTurn},
    state::AppState,
};

fn emit_progress(
    app: &tauri::AppHandle,
    detail: &AgentRunDetail,
    status: Option<&str>,
    message: &str,
) {
    let _ = app.emit(
        "agent://progress",
        AgentProgressEvent {
            run_id: detail.run.id.clone(),
            status: status.unwrap_or(&detail.run.status).to_string(),
            current_step: detail.run.current_step,
            total_steps: detail.steps.len() as i32,
            message: message.into(),
        },
    );
}

#[tauri::command]
#[specta::specta]
pub fn agent_tools() -> Vec<AgentToolDefinition> {
    agent::tools()
}

#[tauri::command]
#[specta::specta]
pub fn agent_runs(state: State<'_, AppState>, limit: Option<i32>) -> IpcResult<Vec<AgentRun>> {
    let guard = state
        .project
        .lock()
        .map_err(|_| "项目状态锁已损坏".to_string())?;
    let loaded = guard.as_ref().ok_or("尚未打开项目")?;
    let mut runs = agent::list(
        &loaded.conn,
        &loaded.summary.id,
        limit.unwrap_or(40).clamp(1, 200) as usize,
    )
    .map_err(|error| error.to_string())?;
    let cancellations = state
        .cancellations
        .lock()
        .map_err(|_| "取消状态锁已损坏".to_string())?;
    for run in &mut runs {
        if ["planning", "running"].contains(&run.status.as_str())
            && !cancellations.contains_key(&format!("agent:{}", run.id))
        {
            agent::mark_interrupted(&loaded.conn, &loaded.summary.id, &run.id)
                .map_err(|error| error.to_string())?;
            run.status = "paused".into();
            run.error = "应用关闭或进程中断后已安全暂停；可从最近步骤恢复".into();
        }
    }
    Ok(runs)
}

#[tauri::command]
#[specta::specta]
pub fn agent_run_create(
    state: State<'_, AppState>,
    input: AgentRunCreateInput,
) -> IpcResult<AgentRunDetail> {
    let mut guard = state
        .project
        .lock()
        .map_err(|_| "项目状态锁已损坏".to_string())?;
    let loaded = guard.as_mut().ok_or("尚未打开项目")?;
    let run = agent::create(&mut loaded.conn, &loaded.summary.id, &input)
        .map_err(|error| error.to_string())?;
    agent::detail(&loaded.conn, &loaded.summary.id, &run.id).map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn agent_run_get(state: State<'_, AppState>, id: String) -> IpcResult<AgentRunDetail> {
    state
        .with_project(|loaded| agent::detail(&loaded.conn, &loaded.summary.id, &id))
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn agent_run_pause(state: State<'_, AppState>, id: String) -> IpcResult<AgentRunDetail> {
    state
        .with_project(|loaded| {
            agent::pause(&loaded.conn, &loaded.summary.id, &id)?;
            agent::detail(&loaded.conn, &loaded.summary.id, &id)
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn agent_run_cancel(state: State<'_, AppState>, id: String) -> IpcResult<AgentRunDetail> {
    if let Ok(map) = state.cancellations.lock() {
        if let Some(flag) = map.get(&format!("agent:{id}")) {
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
    state
        .with_project(|loaded| {
            agent::cancel(&loaded.conn, &loaded.summary.id, &id)?;
            agent::detail(&loaded.conn, &loaded.summary.id, &id)
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn agent_run_resume(state: State<'_, AppState>, id: String) -> IpcResult<AgentRunDetail> {
    state
        .with_project(|loaded| {
            agent::resume(&loaded.conn, &loaded.summary.id, &id)?;
            agent::detail(&loaded.conn, &loaded.summary.id, &id)
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn agent_run_retry_step(
    state: State<'_, AppState>,
    run_id: String,
    step_id: String,
) -> IpcResult<AgentRunDetail> {
    state
        .with_project(|loaded| {
            agent::retry_failed(&loaded.conn, &loaded.summary.id, &run_id, &step_id)?;
            agent::detail(&loaded.conn, &loaded.summary.id, &run_id)
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn agent_run_execute(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> IpcResult<AgentRunDetail> {
    let client = super::seed::require_client(&state).map_err(|error| {
        let message = error.to_string();
        if let Ok(guard) = state.project.lock() {
            if let Some(loaded) = guard.as_ref() {
                let _ = agent::fail_step(&loaded.conn, &id, &loaded.summary.id, &message);
            }
        }
        message
    })?;
    let cancel = Arc::new(AtomicBool::new(false));
    let request_id = format!("agent:{id}");
    state
        .cancellations
        .lock()
        .map_err(|_| "取消状态锁已损坏".to_string())?
        .insert(request_id.clone(), cancel.clone());

    let result = async {
        let first = {
            let guard = state
                .project
                .lock()
                .map_err(|_| "项目状态锁已损坏".to_string())?;
            let loaded = guard.as_ref().ok_or("尚未打开项目")?;
            let current = agent::run(&loaded.conn, &loaded.summary.id, &id)
                .map_err(|error| error.to_string())?;
            if current.status == "draft" || current.status == "planning" {
                agent::start(&loaded.conn, &loaded.summary.id, &id)
                    .map_err(|error| error.to_string())?;
            } else if current.status == "paused" {
                return agent::detail(&loaded.conn, &loaded.summary.id, &id)
                    .map_err(|error| error.to_string());
            } else if !["running", "paused"].contains(&current.status.as_str()) {
                return agent::detail(&loaded.conn, &loaded.summary.id, &id)
                    .map_err(|error| error.to_string());
            }
            agent::detail(&loaded.conn, &loaded.summary.id, &id)
                .map_err(|error| error.to_string())?
        };
        emit_progress(&app, &first, Some("running"), "正在读取项目快照");

        for _ in 0..8 {
            let current_status = {
                let guard = state
                    .project
                    .lock()
                    .map_err(|_| "项目状态锁已损坏".to_string())?;
                let loaded = guard.as_ref().ok_or("尚未打开项目")?;
                agent::run(&loaded.conn, &loaded.summary.id, &id)
                    .map_err(|error| error.to_string())?
                    .status
            };
            if current_status == "canceled" {
                return {
                    let guard = state
                        .project
                        .lock()
                        .map_err(|_| "项目状态锁已损坏".to_string())?;
                    let loaded = guard.as_ref().ok_or("尚未打开项目")?;
                    agent::detail(&loaded.conn, &loaded.summary.id, &id)
                        .map_err(|error| error.to_string())
                };
            }

            let context = {
                let guard = state
                    .project
                    .lock()
                    .map_err(|_| "项目状态锁已损坏".to_string())?;
                let loaded = guard.as_ref().ok_or("尚未打开项目")?;
                agent::execute_read_steps(&loaded.conn, &loaded.summary.id, &id)
                    .map_err(|error| error.to_string())?
            };

            match context {
                Some(context) => {
                    {
                        let guard = state
                            .project
                            .lock()
                            .map_err(|_| "项目状态锁已损坏".to_string())?;
                        let loaded = guard.as_ref().ok_or("尚未打开项目")?;
                        let detail = agent::detail(&loaded.conn, &loaded.summary.id, &id)
                            .map_err(|error| error.to_string())?;
                        emit_progress(&app, &detail, Some("running"), "正在整理证据并生成候选");
                    }
                    let run = {
                        let guard = state
                            .project
                            .lock()
                            .map_err(|_| "项目状态锁已损坏".to_string())?;
                        let loaded = guard.as_ref().ok_or("尚未打开项目")?;
                        agent::run(&loaded.conn, &loaded.summary.id, &id)
                            .map_err(|error| error.to_string())?
                    };
                    let turns = [
                        ChatTurn::system(agent_synthesis_system_prompt()),
                        ChatTurn::user(agent_synthesis_user_prompt(
                            &context.context_json,
                            &run.scope,
                            &run.output,
                        )),
                    ];
                    let raw = match client.stream(&turns, cancel.clone(), |_| {}).await {
                        Ok(raw) => raw,
                        Err(error) => {
                            let message = error.to_string();
                            let guard = state
                                .project
                                .lock()
                                .map_err(|_| "项目状态锁已损坏".to_string())?;
                            let loaded = guard.as_ref().ok_or("尚未打开项目")?;
                            let current_status = agent::run(&loaded.conn, &loaded.summary.id, &id)
                                .map_err(|run_error| run_error.to_string())?
                                .status;
                            if current_status == "canceled" {
                                return agent::detail(&loaded.conn, &loaded.summary.id, &id)
                                    .map_err(|detail_error| detail_error.to_string());
                            }
                            let _ =
                                agent::fail_step(&loaded.conn, &id, &loaded.summary.id, &message);
                            if let Ok(detail) = agent::detail(&loaded.conn, &loaded.summary.id, &id)
                            {
                                emit_progress(&app, &detail, Some("failed"), &message);
                            }
                            return Err(message);
                        }
                    };
                    let mut guard = state
                        .project
                        .lock()
                        .map_err(|_| "项目状态锁已损坏".to_string())?;
                    let loaded = guard.as_mut().ok_or("尚未打开项目")?;
                    let status = agent::run(&loaded.conn, &loaded.summary.id, &id)
                        .map_err(|error| error.to_string())?
                        .status;
                    if status == "canceled" {
                        return agent::detail(&loaded.conn, &loaded.summary.id, &id)
                            .map_err(|error| error.to_string());
                    }
                    agent::complete_synthesis(
                        &mut loaded.conn,
                        &loaded.summary.id,
                        &id,
                        &raw,
                        &client.model,
                    )
                    .map_err(|error| error.to_string())?;
                    let detail = agent::detail(&loaded.conn, &loaded.summary.id, &id)
                        .map_err(|error| error.to_string())?;
                    drop(guard);
                    emit_progress(
                        &app,
                        &detail,
                        Some(&status),
                        if status == "paused" {
                            "模型步骤已完成，任务保持暂停"
                        } else {
                            "候选变更已写入待审批区"
                        },
                    );
                    if status == "paused" {
                        return Ok(detail);
                    }
                }
                None => {
                    let detail = {
                        let guard = state
                            .project
                            .lock()
                            .map_err(|_| "项目状态锁已损坏".to_string())?;
                        let loaded = guard.as_ref().ok_or("尚未打开项目")?;
                        agent::finish_read_steps(&loaded.conn, &loaded.summary.id, &id)
                            .map_err(|error| error.to_string())?
                    };
                    emit_progress(
                        &app,
                        &detail,
                        Some(&detail.run.status),
                        "Agent 任务已停在审批边界",
                    );
                    return Ok(detail);
                }
            }
        }
        Err("Agent 执行步数超过上限，已安全停止".into())
    }
    .await;

    if let Ok(mut map) = state.cancellations.lock() {
        map.remove(&request_id);
    }
    result
}

#[tauri::command]
#[specta::specta]
pub fn agent_foundation(state: State<'_, AppState>) -> IpcResult<FoundationReport> {
    state
        .with_project(|loaded| agent::foundation_report(&loaded.conn, &loaded.summary.id))
        .map_err(|error| error.to_string())
}
