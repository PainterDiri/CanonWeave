use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rusqlite::params;
use serde_json::{json, Value};
use tauri::{Emitter, State};

use crate::{
    candidates, chat, craft,
    domain::{
        CraftReport, HarnessEvent, HarnessToolTrace, HarnessTurnResult, ProjectGuide, StoryThread,
        StoryThreadInput, WorkflowPipeline,
    },
    error::IpcResult,
    harness::{self, ToolContext, ToolOutcome},
    llm::{
        is_tool_protocol_unsupported, is_transient_model_error, LlmClient, ToolCall, ToolTurnOutput,
    },
    state::AppState,
};

const HISTORY_MESSAGES: usize = 12;

fn emit(
    app: &tauri::AppHandle,
    session_id: &str,
    run_id: &str,
    kind: &str,
    text: &str,
    data: Value,
) {
    let _ = app.emit(
        "harness://event",
        HarnessEvent {
            session_id: session_id.into(),
            run_id: run_id.into(),
            kind: kind.into(),
            text: text.into(),
            data,
        },
    );
}

fn lock_err<T>(_: T) -> String {
    "项目状态锁已损坏".to_string()
}

/// 一轮模型调用：瞬时错误最多重试两次；网关拒绝 tools 参数时切换到文本协议。
async fn model_turn(
    client: &LlmClient,
    messages: &mut [Value],
    tools: &Value,
    use_tools: bool,
    text_mode: &mut bool,
    mode: &str,
    cancel: Arc<AtomicBool>,
    on_delta: &mut (dyn FnMut(&str) + Send),
) -> Result<ToolTurnOutput, String> {
    let mut attempt = 0;
    loop {
        let native = use_tools && !*text_mode;
        let result = if *text_mode {
            client
                .stream_tool_turn(messages, None, cancel.clone(), |_| {})
                .await
        } else {
            client
                .stream_tool_turn(
                    messages,
                    if native { Some(tools) } else { None },
                    cancel.clone(),
                    &mut *on_delta,
                )
                .await
        };
        match result {
            Ok(output) => return Ok(output),
            Err(error) => {
                let message = error.to_string();
                if cancel.load(Ordering::Relaxed) {
                    return Err("已取消".into());
                }
                if native && is_tool_protocol_unsupported(&message) {
                    *text_mode = true;
                    if let Some(system) = messages.first_mut() {
                        let content = system["content"].as_str().unwrap_or_default().to_string();
                        system["content"] =
                            Value::String(content + &harness::text_protocol_instructions(mode));
                    }
                    continue;
                }
                if attempt < 2 && is_transient_model_error(&message) {
                    attempt += 1;
                    tokio::time::sleep(Duration::from_millis(1500 * attempt)).await;
                    continue;
                }
                return Err(message);
            }
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn harness_send(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    content: String,
    mode: String,
) -> IpcResult<HarnessTurnResult> {
    let content = content.trim().to_string();
    if content.is_empty() {
        return Err("请输入要交给 Agent 的任务".into());
    }
    let mode = harness::normalize_mode(&mode).to_string();
    let client = super::seed::require_client(&state).map_err(|e| e.to_string())?;

    // 1. 落盘作者消息、建立 Run、准备上下文（只在这里短暂持锁）。
    let (project_id, user_message, run_id, mut messages) = {
        let guard = state.project.lock().map_err(lock_err)?;
        let loaded = guard.as_ref().ok_or("尚未打开项目")?;
        let pid = loaded.summary.id.clone();
        let conn = &loaded.conn;
        let history = chat::history_window(conn, &pid, &session_id, HISTORY_MESSAGES)
            .map_err(|e| e.to_string())?;
        let session = chat::get(conn, &pid, &session_id).map_err(|e| e.to_string())?;
        let user_message = chat::append_message(conn, &pid, &session_id, "user", &content, "")
            .map_err(|e| e.to_string())?;
        let run_id = harness::create_run(conn, &pid, &session_id, &content, &mode, &client.model)
            .map_err(|e| e.to_string())?;
        let system = harness::system_prompt(conn, &pid, &mode).map_err(|e| e.to_string())?;
        let mut messages = vec![json!({"role":"system","content":system})];
        if !session.summary.trim().is_empty() {
            messages.push(json!({
                "role":"system",
                "content":format!("更早对话的滚动摘要（不是事实来源）：\n{}", session.summary)
            }));
        }
        for message in history {
            if message.role == "user" || message.role == "assistant" {
                messages.push(json!({"role":message.role,"content":message.content}));
            }
        }
        // 作者 @ 引用的路径预先读取，作为本轮附件。
        let mut attachments = String::new();
        for path in harness::mentioned_paths(&content) {
            let call = ToolCall {
                id: String::new(),
                name: "read_path".into(),
                arguments: json!({"path": path}).to_string(),
            };
            let (step_id, _) =
                harness::begin_step(conn, &run_id, &call).map_err(|e| e.to_string())?;
            let ctx = ToolContext {
                conn,
                project_id: &pid,
                session_id: &session_id,
                run_id: &run_id,
                step_id: &step_id,
                mode: &mode,
            };
            let outcome = harness::execute(&ctx, &call);
            let title = format!("附件 @{path}");
            harness::finish_step(conn, &step_id, &title, &outcome).map_err(|e| e.to_string())?;
            attachments.push_str(&format!(
                "\n\n<attachment path=\"{path}\">\n{}\n</attachment>",
                outcome.content
            ));
        }
        messages.push(json!({"role":"user","content":format!("{content}{attachments}")}));
        (pid, user_message, run_id, messages)
    };
    emit(
        &app,
        &session_id,
        &run_id,
        "start",
        "",
        json!({"mode": mode}),
    );

    let cancel = Arc::new(AtomicBool::new(false));
    let request_id = format!("harness:{session_id}");
    state
        .cancellations
        .lock()
        .map_err(lock_err)?
        .insert(request_id.clone(), cancel.clone());

    let tools = harness::tools_schema(&mode);
    let outcome: Result<(String, Value), String> = async {
        let mut text_mode = false;
        let mut final_text = String::new();
        let mut plan = json!([]);
        for turn in 0..harness::MAX_MODEL_TURNS {
            if cancel.load(Ordering::Relaxed) {
                return Err("已取消".into());
            }
            let last_turn = turn + 1 == harness::MAX_MODEL_TURNS;
            if last_turn {
                messages.push(json!({
                    "role":"user",
                    "content":"已达到本轮工具调用上限。请不要再调用工具，基于已读取的信息直接给出总结和下一步。"
                }));
            }
            let app_for_delta = app.clone();
            let (sid, rid) = (session_id.clone(), run_id.clone());
            let mut on_delta = move |delta: &str| {
                emit(&app_for_delta, &sid, &rid, "delta", delta, Value::Null);
            };
            let output = model_turn(
                &client,
                &mut messages,
                &tools,
                !last_turn,
                &mut text_mode,
                &mode,
                cancel.clone(),
                &mut on_delta,
            )
            .await?;
            let (visible, calls) = if text_mode {
                let (visible, calls) = harness::parse_text_tool_calls(&output.content);
                if !visible.is_empty() {
                    emit(&app, &session_id, &run_id, "delta", &visible, Value::Null);
                }
                (visible, if last_turn { vec![] } else { calls })
            } else {
                (output.content.clone(), output.tool_calls.clone())
            };
            if calls.is_empty() {
                // 部分模型偶尔把内部的函数调用标记当正文吐出来；这时工具没有执行，提醒它重新走工具接口。
                if !last_turn && harness::leaked_tool_markup(&visible) {
                    messages.push(json!({"role":"assistant","content":visible}));
                    messages.push(json!({
                        "role":"user",
                        "content":"上一条回复把工具调用写成了文本，工具没有被执行。请通过工具接口重新调用需要的工具，不要在正文里输出调用标记。"
                    }));
                    continue;
                }
                final_text = visible;
                break;
            }
            emit(&app, &session_id, &run_id, "turn_end", &visible, Value::Null);
            if text_mode {
                messages.push(json!({"role":"assistant","content":output.content}));
            } else {
                messages.push(json!({
                    "role":"assistant",
                    "content": if visible.is_empty() { Value::Null } else { Value::String(visible.clone()) },
                    "tool_calls": calls.iter().map(|call| json!({
                        "id": call.id,
                        "type": "function",
                        "function": {"name": call.name, "arguments": call.arguments}
                    })).collect::<Vec<_>>()
                }));
            }
            let mut text_results = String::new();
            let mut terminal = false;
            for (index, call) in calls.iter().enumerate() {
                if cancel.load(Ordering::Relaxed) {
                    return Err("已取消".into());
                }
                let args: Value = crate::llm::parse_json_loose(&call.arguments).unwrap_or_else(|_| json!({}));
                let title = harness::tool_title(&call.name, &args);
                let result_text = if index >= harness::MAX_CALLS_PER_TURN {
                    "错误：单轮工具调用过多，本次调用被跳过。请合并请求。".to_string()
                } else {
                    let (step_id, seq) = {
                        let guard = state.project.lock().map_err(lock_err)?;
                        let loaded = guard.as_ref().ok_or("尚未打开项目")?;
                        harness::begin_step(&loaded.conn, &run_id, call).map_err(|e| e.to_string())?
                    };
                    emit(
                        &app,
                        &session_id,
                        &run_id,
                        "tool_start",
                        &title,
                        json!({"stepId": step_id, "seq": seq, "toolName": call.name, "input": args}),
                    );
                    let outcome = if harness::is_workflow_tool(&call.name) {
                        run_workflow_tool(&app, &state, &mode, &call.name, &args, &user_message.id).await
                    } else {
                        let guard = state.project.lock().map_err(lock_err)?;
                        let loaded = guard.as_ref().ok_or("尚未打开项目")?;
                        let ctx = ToolContext {
                            conn: &loaded.conn,
                            project_id: &project_id,
                            session_id: &session_id,
                            run_id: &run_id,
                            step_id: &step_id,
                            mode: &mode,
                        };
                        harness::execute(&ctx, call)
                    };
                    {
                        let guard = state.project.lock().map_err(lock_err)?;
                        let loaded = guard.as_ref().ok_or("尚未打开项目")?;
                        let conn = &loaded.conn;
                        for candidate_id in &outcome.candidate_ids {
                            conn.execute(
                                "UPDATE candidate SET batch_id=COALESCE(batch_id, ?2), agent_run_id=?2,
                                    agent_step_id=?3 WHERE id=?1",
                                params![candidate_id, run_id, step_id],
                            )
                            .map_err(|e| e.to_string())?;
                        }
                        harness::finish_step(conn, &step_id, &title, &outcome).map_err(|e| e.to_string())?;
                        if let Some(next) = &outcome.plan {
                            plan = next.clone();
                            harness::save_plan(conn, &run_id, &plan).map_err(|e| e.to_string())?;
                            emit(&app, &session_id, &run_id, "plan", "", plan.clone());
                        }
                    }
                    terminal |= outcome.terminal;
                    emit(
                        &app,
                        &session_id,
                        &run_id,
                        "tool_end",
                        &title,
                        serde_json::to_value(HarnessToolTrace {
                            step_id,
                            seq,
                            tool_name: call.name.clone(),
                            title: title.clone(),
                            input: args,
                            summary: outcome.summary.clone(),
                            status: if outcome.failed { "failed" } else { "completed" }.into(),
                            candidate_ids: outcome.candidate_ids.clone(),
                            action: outcome.action.clone().unwrap_or(Value::Null),
                        })
                        .unwrap_or(Value::Null),
                    );
                    outcome.content
                };
                if text_mode {
                    text_results.push_str(&format!(
                        "<tool_result name=\"{}\">\n{}\n</tool_result>\n",
                        call.name, result_text
                    ));
                } else {
                    messages.push(json!({
                        "role":"tool",
                        "tool_call_id": call.id,
                        "content": result_text
                    }));
                }
            }
            if text_mode {
                messages.push(json!({"role":"user","content":text_results}));
            }
            if terminal {
                final_text = if visible.trim().is_empty() {
                    "这里需要你来做决定，请在下方选择一个方向，或直接告诉我你的想法。".into()
                } else {
                    visible
                };
                break;
            }
        }
        if final_text.trim().is_empty() {
            final_text = "本轮已完成工具调用，但模型没有给出总结。请查看上方的执行轨迹和候选。".into();
        }
        Ok((final_text, plan))
    }
    .await;

    if let Ok(mut map) = state.cancellations.lock() {
        map.remove(&request_id);
    }

    let guard = state.project.lock().map_err(lock_err)?;
    let loaded = guard.as_ref().ok_or("尚未打开项目")?;
    let conn = &loaded.conn;
    match outcome {
        Ok((final_text, plan)) => {
            let assistant = chat::append_message(
                conn,
                &project_id,
                &session_id,
                "assistant",
                &final_text,
                &client.model,
            )
            .map_err(|e| e.to_string())?;
            let run_candidates =
                candidates::list_for_run(conn, &project_id, &run_id).map_err(|e| e.to_string())?;
            let candidate_ids: Vec<String> = run_candidates.iter().map(|c| c.id.clone()).collect();
            conn.execute(
                "UPDATE chat_message SET linked_candidate_ids=?2, agent_run_id=?3 WHERE id=?1",
                params![
                    assistant.id,
                    serde_json::to_string(&candidate_ids).unwrap_or_default(),
                    run_id
                ],
            )
            .map_err(|e| e.to_string())?;
            conn.execute(
                "UPDATE candidate SET source_message_id=?2
                 WHERE agent_run_id=?1 AND source_message_id IS NULL",
                params![run_id, user_message.id],
            )
            .map_err(|e| e.to_string())?;
            harness::finish_run(conn, &project_id, &run_id, "completed", "")
                .map_err(|e| e.to_string())?;
            let result = HarnessTurnResult {
                run_id: run_id.clone(),
                session: chat::get(conn, &project_id, &session_id).map_err(|e| e.to_string())?,
                user_message,
                assistant_message: chat::message(conn, &project_id, &assistant.id)
                    .map_err(|e| e.to_string())?,
                traces: harness::traces(conn, &run_id).map_err(|e| e.to_string())?,
                candidates: run_candidates,
                plan,
                actions: harness::run_actions(conn, &run_id).map_err(|e| e.to_string())?,
            };
            emit(&app, &session_id, &run_id, "done", "", Value::Null);
            Ok(result)
        }
        Err(message) => {
            let status = if message == "已取消" {
                "canceled"
            } else {
                "failed"
            };
            let _ = harness::finish_run(conn, &project_id, &run_id, status, &message);
            emit(&app, &session_id, &run_id, "error", &message, Value::Null);
            Err(message)
        }
    }
}

fn workflow_failure(message: impl Into<String>) -> ToolOutcome {
    let message = message.into();
    ToolOutcome {
        content: format!("错误：{message}"),
        summary: message,
        failed: true,
        ..ToolOutcome::default()
    }
}

fn premise_item(premise: &crate::domain::Premise) -> Value {
    json!({
        "kind": "premise",
        "id": premise.id,
        "title": premise.logline,
        "summary": premise.core_conflict,
    })
}

/// 调用模型或长事务的工具。产物都是候选（种子卡、蓝本、卷章、文笔档案）或派生视图，
/// 并以确认卡交还作者。
async fn run_workflow_tool(
    app: &tauri::AppHandle,
    state: &State<'_, AppState>,
    mode: &str,
    name: &str,
    args: &Value,
    source_message_id: &str,
) -> ToolOutcome {
    let tier = harness::find_tool(name)
        .map(|tool| tool.tier)
        .unwrap_or(harness::Tier::Propose);
    if !harness::allowed(mode, tier) {
        return workflow_failure(
            "当前为规划（只读）模式，不能生成候选。请先汇报分析结果，由作者决定是否切换到执笔模式。",
        );
    }
    let text = |key: &str| {
        args.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    match name {
        "generate_seed" => {
            match super::seed::generate_seed_cards(state, &text("logline"), Some(source_message_id)).await {
                Ok(cards) => ToolOutcome {
                    content: format!(
                        "已生成 {} 张种子卡候选（在“灵感与卷章 → 种子卡”可见）：\n{}\n请向作者概括三者的差异，并请作者在确认卡中选定一张。",
                        cards.len(),
                        cards
                            .iter()
                            .enumerate()
                            .map(|(index, card)| format!(
                                "{}. {}｜核心冲突：{}｜类型：{}",
                                index + 1,
                                card.logline,
                                card.core_conflict,
                                card.genre
                            ))
                            .collect::<Vec<_>>()
                            .join("\n")
                    ),
                    summary: format!("{} 个方向", cards.len()),
                    action: Some(json!({
                        "type": "approval",
                        "group": "premise",
                        "title": "选定一张种子卡",
                        "items": cards.iter().map(premise_item).collect::<Vec<_>>()
                    })),
                    ..ToolOutcome::default()
                },
                Err(error) => workflow_failure(error),
            }
        }
        "generate_blueprint" => {
            let input = crate::domain::NarrativeGenerateInput {
                medium: if text("medium") == "game" {
                    "game".into()
                } else {
                    "novel".into()
                },
                structure_strategy: if text("strategy") == "fixed" {
                    "fixed".into()
                } else {
                    "adaptive".into()
                },
                depth: Some(text("depth")).filter(|depth| !depth.is_empty()),
                target_units: args
                    .get("targetUnits")
                    .and_then(Value::as_i64)
                    .map(|value| value as i32),
            };
            match super::blueprint::blueprint_generate(app.clone(), state.clone(), input).await {
                Ok(view) => {
                    let errors = view
                        .diagnostics
                        .iter()
                        .filter(|item| item.severity == "error")
                        .count();
                    let warnings = view
                        .diagnostics
                        .iter()
                        .filter(|item| item.severity == "warning")
                        .count();
                    ToolOutcome {
                        content: format!(
                            "已生成叙事蓝本候选《{}》：{} 条人物/势力/规则，{} 个时间锚点，{} 个逻辑节点；诊断 {errors} 阻断、{warnings} 提醒。\n核心戏剧问题：{}\n{}\n请向作者概括世界观的骨架（能力体系、主要势力、核心人物、暗线），并提醒：蓝本确认后可以让你用 sync_blueprint 把它落入人物页与知识库。",
                            view.blueprint.title,
                            view.elements.len(),
                            view.timeline.len(),
                            view.nodes.len(),
                            view.blueprint.core_dramatic_question,
                            view.diagnostics
                                .iter()
                                .filter(|item| item.severity == "error")
                                .take(6)
                                .map(|item| format!("- 阻断：{}", item.message))
                                .collect::<Vec<_>>()
                                .join("\n")
                        ),
                        summary: format!("{} 个逻辑节点 · {errors} 阻断", view.nodes.len()),
                        action: Some(json!({
                            "type": "approval",
                            "group": "blueprint",
                            "title": "确认叙事蓝本",
                            "items": [{
                                "kind": "blueprint",
                                "id": view.blueprint.id,
                                "title": view.blueprint.title,
                                "summary": view.blueprint.core_dramatic_question,
                                "blocked": errors > 0
                            }]
                        })),
                        ..ToolOutcome::default()
                    }
                }
                Err(error) => workflow_failure(error),
            }
        }
        "generate_outline" => {
            let volumes = args
                .get("volumes")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .map(|item| crate::domain::VolumePlanInput {
                            title: item
                                .get("title")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            chapter_count: item
                                .get("chapterCount")
                                .and_then(Value::as_i64)
                                .unwrap_or(0) as i32,
                        })
                        .collect()
                })
                .unwrap_or_default();
            let mode = match text("mode").as_str() {
                "chapters" => "chapters",
                "volumes" => "volumes",
                _ => "adaptive",
            };
            let input = crate::domain::OutlineGenerateInput {
                mode: mode.into(),
                total_chapters: args
                    .get("totalChapters")
                    .and_then(Value::as_i64)
                    .map(|value| value as i32),
                volumes,
            };
            match super::outline::outline_generate(state.clone(), input).await {
                Ok(nodes) => {
                    let volumes: Vec<&crate::domain::OutlineNode> =
                        nodes.iter().filter(|node| node.level == "volume").collect();
                    let chapters: Vec<&crate::domain::OutlineNode> = nodes
                        .iter()
                        .filter(|node| node.level == "chapter")
                        .collect();
                    let preview = chapters
                        .iter()
                        .take(12)
                        .map(|node| {
                            format!("第{}章《{}》：{}", node.order_no, node.title, node.goal)
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    let confirm_ids: Vec<String> = if volumes.is_empty() {
                        chapters.iter().map(|node| node.id.clone()).collect()
                    } else {
                        volumes.iter().map(|node| node.id.clone()).collect()
                    };
                    ToolOutcome {
                        content: format!(
                            "已生成卷章骨架候选：{} 卷、{} 章（在“灵感与卷章 → 卷章编排”可见）。前 12 章：\n{preview}",
                            volumes.len(),
                            chapters.len()
                        ),
                        summary: format!("{} 卷 · {} 章", volumes.len(), chapters.len()),
                        action: Some(json!({
                            "type": "approval",
                            "group": "outline",
                            "title": "确认卷章骨架",
                            "items": [{
                                "kind": "outline",
                                "ids": confirm_ids,
                                "title": format!("{} 卷 · {} 章", volumes.len(), chapters.len()),
                                "summary": "确认后会建立章节，可在正文页开始写作；也可以先到卷章编排逐章修改。"
                            }]
                        })),
                        ..ToolOutcome::default()
                    }
                }
                Err(error) => workflow_failure(error),
            }
        }
        "recommend_style" => {
            match super::agent::style_candidate_for(
                state,
                &text("instruction"),
                "agent:style:harness",
            )
            .await
            {
                Ok(proposal) => ToolOutcome {
                    content: format!(
                        "已生成文笔档案候选「{}」（预设：{}）。理由：{}",
                        proposal
                            .candidate
                            .payload
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or(&proposal.preset_name),
                        proposal.preset_name,
                        proposal.reason
                    ),
                    summary: proposal.preset_name.clone(),
                    candidate_ids: vec![proposal.candidate.id.clone()],
                    ..ToolOutcome::default()
                },
                Err(error) => workflow_failure(error),
            }
        }
        "rebuild_wiki" => match super::wiki::wiki_rebuild(state.clone()) {
            Ok(result) => ToolOutcome {
                content: format!(
                    "叙事百科已重建：{} 页（人物 {}、地点 {}、规则 {}、伏笔 {}、章节 {}）。",
                    result.pages.len(),
                    result.characters,
                    result.places,
                    result.rules,
                    result.promises,
                    result.chapters
                ),
                summary: format!("{} 页", result.pages.len()),
                ..ToolOutcome::default()
            },
            Err(error) => workflow_failure(error),
        },
        "rebuild_graph" => match super::graph::community_build(state.clone()) {
            Ok(reports) => ToolOutcome {
                content: format!("叙事图已重建：{} 个社区报告。", reports.len()),
                summary: format!("{} 个社区", reports.len()),
                ..ToolOutcome::default()
            },
            Err(error) => workflow_failure(error),
        },
        other => workflow_failure(format!("未知工作流工具：{other}")),
    }
}

#[tauri::command]
#[specta::specta]
pub fn harness_cancel(state: State<'_, AppState>, session_id: String) -> IpcResult<bool> {
    let map = state.cancellations.lock().map_err(lock_err)?;
    match map.get(&format!("harness:{session_id}")) {
        Some(flag) => {
            flag.store(true, Ordering::Relaxed);
            Ok(true)
        }
        None => Ok(false),
    }
}

#[tauri::command]
#[specta::specta]
pub fn harness_traces(
    state: State<'_, AppState>,
    run_id: String,
) -> IpcResult<Vec<HarnessToolTrace>> {
    state
        .with_project(|loaded| {
            crate::agent::run(&loaded.conn, &loaded.summary.id, &run_id)?;
            harness::traces(&loaded.conn, &run_id)
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn harness_paths(state: State<'_, AppState>) -> IpcResult<Vec<String>> {
    state
        .with_project(|loaded| harness::workspace_paths(&loaded.conn, &loaded.summary.id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn project_guide_get(state: State<'_, AppState>) -> IpcResult<ProjectGuide> {
    state
        .with_project(|loaded| craft::guide(&loaded.conn, &loaded.summary.id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn project_guide_save(state: State<'_, AppState>, content: String) -> IpcResult<ProjectGuide> {
    state
        .with_project(|loaded| craft::save_guide(&loaded.conn, &loaded.summary.id, &content))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn story_thread_list(state: State<'_, AppState>) -> IpcResult<Vec<StoryThread>> {
    state
        .with_project(|loaded| craft::threads(&loaded.conn, &loaded.summary.id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn story_thread_save(
    state: State<'_, AppState>,
    input: StoryThreadInput,
) -> IpcResult<StoryThread> {
    state
        .with_project(|loaded| craft::save_thread(&loaded.conn, &loaded.summary.id, &input))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn story_thread_delete(state: State<'_, AppState>, id: String) -> IpcResult<()> {
    state
        .with_project(|loaded| craft::delete_thread(&loaded.conn, &loaded.summary.id, &id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn craft_report(state: State<'_, AppState>) -> IpcResult<CraftReport> {
    state
        .with_project(|loaded| craft::report(&loaded.conn, &loaded.summary.id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn workflow_pipeline(state: State<'_, AppState>) -> IpcResult<WorkflowPipeline> {
    state
        .with_project(|loaded| craft::pipeline(&loaded.conn, &loaded.summary.id))
        .map_err(|e| e.to_string())
}
