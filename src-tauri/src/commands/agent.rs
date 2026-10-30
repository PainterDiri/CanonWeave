use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use serde_json::{json, Value};
use tauri::{Emitter, State};

use crate::{
    agent, candidates, chat,
    domain::{
        AgentNextAction, AgentProgressEvent, AgentRun, AgentRunCreateInput, AgentRunDetail,
        AgentToolDefinition, FoundationReport, StyleProfileInput, StyleRecommendation,
    },
    error::IpcResult,
    llm::{agent_synthesis_system_prompt, agent_synthesis_user_prompt, parse_json_loose, ChatTurn},
    seed,
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

#[tauri::command]
#[specta::specta]
pub fn agent_next_actions(state: State<'_, AppState>) -> IpcResult<Vec<AgentNextAction>> {
    state
        .with_project(|loaded| agent::next_actions(&loaded.conn, &loaded.summary.id))
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn agent_style_proposal(
    state: State<'_, AppState>,
    session_id: String,
    instruction: String,
) -> IpcResult<StyleRecommendation> {
    let instruction = instruction.trim().to_string();
    if instruction.is_empty() {
        return Err("请说明你希望的文笔方向".into());
    }
    let project_id = {
        let guard = state
            .project
            .lock()
            .map_err(|_| "项目状态锁已损坏".to_string())?;
        let loaded = guard.as_ref().ok_or("尚未打开项目")?;
        chat::get(&loaded.conn, &loaded.summary.id, &session_id)
            .map_err(|error| error.to_string())?;
        chat::append_message(
            &loaded.conn,
            &loaded.summary.id,
            &session_id,
            "user",
            &instruction,
            "",
        )
        .map_err(|error| error.to_string())?;
        loaded.summary.id.clone()
    };
    let proposal = style_candidate_for(&state, &instruction, &format!("agent:style:{session_id}")).await?;
    let assistant_text = format!(
        "我建议采用「{}」文笔方案。\n\n{}\n\n已生成待确认的文笔档案候选；确认前不会用于任何正文生成。",
        proposal.preset_name, proposal.reason
    );
    let guard = state
        .project
        .lock()
        .map_err(|_| "项目状态锁已损坏".to_string())?;
    let loaded = guard.as_ref().ok_or("尚未打开项目")?;
    let message = chat::append_message(
        &loaded.conn,
        &project_id,
        &session_id,
        "assistant",
        &assistant_text,
        &proposal.model,
    )
    .map_err(|error| error.to_string())?;
    loaded
        .conn
        .execute(
            "UPDATE candidate SET source_message_id=?2 WHERE id=?1",
            rusqlite::params![proposal.candidate.id, message.id],
        )
        .map_err(|error| error.to_string())?;
    loaded
        .conn
        .execute(
            "UPDATE chat_message SET linked_candidate_ids=?2 WHERE id=?1",
            rusqlite::params![
                message.id,
                serde_json::to_string(&[proposal.candidate.id.clone()]).unwrap()
            ],
        )
        .map_err(|error| error.to_string())?;
    let candidate = candidates::get(&loaded.conn, &project_id, &proposal.candidate.id)
        .map_err(|error| error.to_string())?;
    Ok(StyleRecommendation {
        preset_id: proposal.preset_id,
        preset_name: proposal.preset_name,
        reason: proposal.reason,
        message,
        candidate,
    })
}

pub struct StyleProposal {
    pub preset_id: String,
    pub preset_name: String,
    pub reason: String,
    pub model: String,
    pub candidate: crate::domain::Candidate,
}

/// 从预设中选出最匹配的文笔方案，写入 style_profile 候选。不追加会话消息，供 Agent harness 复用。
pub async fn style_candidate_for(
    state: &State<'_, AppState>,
    instruction: &str,
    source_span: &str,
) -> IpcResult<StyleProposal> {
    let client = super::seed::require_client(state).map_err(|error| error.to_string())?;
    let (project_id, premise, blueprint, presets) = {
        let guard = state
            .project
            .lock()
            .map_err(|_| "项目状态锁已损坏".to_string())?;
        let loaded = guard.as_ref().ok_or("尚未打开项目")?;
        let premise =
            seed::active(&loaded.conn, &loaded.summary.id).map_err(|error| error.to_string())?;
        let blueprint = match crate::blueprint::current(&loaded.conn, &loaded.summary.id)
            .map_err(|error| error.to_string())?
        {
            Some(view) => Some(view),
            None => crate::blueprint::candidate(&loaded.conn, &loaded.summary.id)
                .map_err(|error| error.to_string())?,
        };
        let presets = crate::style::presets().to_vec();
        (
            loaded.summary.id.clone(),
            premise.map(|value| serde_json::to_value(value)).transpose(),
            blueprint.map(|value| {
                json!({
                    "title": value.blueprint.title,
                    "dramaticQuestion": value.blueprint.core_dramatic_question,
                    "openingState": value.blueprint.opening_state,
                    "endingState": value.blueprint.ending_state,
                    "elements": value.elements.into_iter().take(12).map(|element| json!({
                        "kind": element.kind,
                        "name": element.name,
                        "role": element.role,
                        "summary": element.summary
                    })).collect::<Vec<_>>()
                })
            }),
            presets,
        )
    };
    let premise = premise.map_err(|error| error.to_string())?;
    let preset_list = presets
        .iter()
        .map(|preset| {
            json!({
                "id": preset.id,
                "name": preset.name,
                "description": preset.description,
                "antiStyle": preset.anti_style
            })
        })
        .collect::<Vec<_>>();
    let raw = client
        .complete(
            &[
                ChatTurn::system(
                    "你是长篇叙事文笔总监。只能从给定预设中选择一个最匹配故事和作者要求的方案。\n\
                     只输出 JSON：{\"presetId\":\"给定 ID\",\"name\":\"档案名\",\"reason\":\"180 字以内，说明句子节奏、感官、对白、心理和避免项如何服务故事\"}。\n\
                     不要输出 Markdown，不要创建不存在的预设。",
                ),
                ChatTurn::user(format!(
                    "作者要求：{instruction}\n种子卡：{}\n叙事蓝本：{}\n可选预设：{}",
                    premise
                        .as_ref()
                        .map(Value::to_string)
                        .unwrap_or_else(|| "尚未确认".into()),
                    blueprint
                        .as_ref()
                        .map(Value::to_string)
                        .unwrap_or_else(|| "尚未确认".into()),
                    serde_json::to_string(&preset_list).unwrap_or_default()
                )),
            ],
            Some(1200),
        )
        .await
        .map_err(|error| error.to_string())?;
    let recommendation: Value = parse_json_loose(&raw).map_err(|error| error.to_string())?;
    let preset_id = recommendation
        .get("presetId")
        .and_then(Value::as_str)
        .ok_or("文笔模型没有返回 presetId")?;
    let preset = presets
        .iter()
        .find(|preset| preset.id == preset_id)
        .ok_or_else(|| format!("文笔模型返回了未知预设：{preset_id}"))?;
    let reason = recommendation
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("根据当前故事气质选择该文笔预设。")
        .trim()
        .to_string();
    let profile_name = recommendation
        .get("name")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&preset.name)
        .trim()
        .to_string();
    let profile = StyleProfileInput {
        id: None,
        scope: "project".into(),
        scope_id: None,
        name: profile_name,
        pov: premise
            .as_ref()
            .and_then(|value| value.get("pov"))
            .and_then(Value::as_str)
            .unwrap_or("第三人称")
            .into(),
        person: premise
            .as_ref()
            .and_then(|value| value.get("person"))
            .and_then(Value::as_str)
            .unwrap_or("第三人称")
            .into(),
        tense: premise
            .as_ref()
            .and_then(|value| value.get("tense"))
            .and_then(Value::as_str)
            .unwrap_or("过去时")
            .into(),
        syntax: preset.syntax.clone(),
        lexicon: preset.lexicon.clone(),
        imagery: preset.imagery.clone(),
        rhetoric: preset.rhetoric.clone(),
        emotion: preset.emotion.clone(),
        ratios: preset.ratios.clone(),
        anti_style: preset.anti_style.clone(),
    };
    let mut payload = serde_json::to_value(&profile).map_err(|error| error.to_string())?;
    if let Some(object) = payload.as_object_mut() {
        object.insert("presetId".into(), Value::String(preset.id.clone()));
        object.insert("agentReason".into(), Value::String(reason.clone()));
    }
    let guard = state
        .project
        .lock()
        .map_err(|_| "项目状态锁已损坏".to_string())?;
    let loaded = guard.as_ref().ok_or("尚未打开项目")?;
    let candidate = candidates::insert(
        &loaded.conn,
        &project_id,
        "style_profile",
        payload,
        Some(source_span),
        None,
        0.8,
    )
    .map_err(|error| error.to_string())?;
    Ok(StyleProposal {
        preset_id: preset.id.clone(),
        preset_name: preset.name.clone(),
        reason,
        model: client.model.clone(),
        candidate,
    })
}
