use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::Deserialize;
use serde_json::json;
use tauri::{Emitter, Manager, State};

use crate::{
    candidates, chat, context,
    domain::{
        BeatOption, Candidate, EmotionBeat, GenerationDelta, GenerationError,
        GenerationFinished, GenerationInput, GenerationRunRow, GenerationStartResult, PlotBeatsInput,
        SceneRecord, SensoryPlan,
    },
    error::IpcResult,
    llm::{parse_json_loose, ChatTurn, LlmClient},
    scenes, seed, state::AppState, style,
};

fn generation_system_prompt() -> &'static str {
    "你是中文长篇小说的正文写作器。只输出正文，不输出标题、解释、引用或元信息。\n\
     严格遵守上下文中的 [STYLE CONTRACT] 与 POV 限知；不得改变 L0 硬事实；\n\
     不确定的设定不要补写，改用可感知的动作与场景细节推进。\n\
     篇幅 900 至 1600 字，场景结束时必须发生可记录的状态变化。"
}

#[tauri::command]
#[specta::specta]
pub fn generation_start(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    input: GenerationInput,
) -> IpcResult<GenerationStartResult> {
    if input.mode != "quick" {
        return Err("M2 先支持快速模式；精修模式与 fact_delta 将在 M3 接入".into());
    }
    let config = state.model_store.load();
    let key = crate::model::load_api_key()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "尚未保存 API Key，请先在设置中填写并测试连接".to_string())?;
    let (project_id, scene, chapter, preview, style_version) = {
        let guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
        let loaded = guard.as_ref().ok_or("尚未打开项目")?;
        let scene = scenes::get_scene(&loaded.conn, &loaded.summary.id, &input.scene_id)
            .map_err(|e| e.to_string())?;
        let chapter = scenes::get_chapter(&loaded.conn, &loaded.summary.id, &scene.chapter_id)
            .map_err(|e| e.to_string())?;
        let preview = context::compile(
            &loaded.conn,
            &loaded.summary.id,
            &input.scene_id,
            12000,
            input.style_profile_id.as_deref(),
        )
        .map_err(|e| e.to_string())?;
        let style_version = style::list(&loaded.conn, &loaded.summary.id)
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|profile| {
                Some(&profile.id) == input.style_profile_id.as_ref()
                    || Some(&profile.id) == scene.style_profile_id.as_ref()
            })
            .or_else(|| {
                style::list(&loaded.conn, &loaded.summary.id)
                    .ok()
                    .and_then(|list| list.into_iter().find(|p| p.scope == "project"))
            })
            .map(|profile| profile.version)
            .unwrap_or(0);
        (
            loaded.summary.id.clone(),
            scene,
            chapter,
            preview,
            style_version,
        )
    };
    let run_id = uuid::Uuid::new_v4().to_string();
    let ts = crate::now();
    {
        let guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
        let loaded = guard.as_ref().ok_or("尚未打开项目")?;
        loaded
            .conn
            .execute(
                "INSERT INTO generation_run (id, project_id, chapter_id, scene_id, model, params,
                    context_hash, prompt_version, style_profile_version, generation_mode, review_mode,
                    fact_delta, status, error, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'balanced','{}','running','',?11)",
                rusqlite::params![
                    run_id,
                    project_id,
                    chapter.id,
                    scene.id,
                    config.model,
                    json!({"budget": preview.budget}).to_string(),
                    preview.context_hash,
                    crate::llm::PROMPT_VERSION,
                    style_version,
                    input.mode,
                    ts
                ],
            )
            .map_err(|e| e.to_string())?;
    }
    let cancel = Arc::new(AtomicBool::new(false));
    state
        .cancellations
        .lock()
        .map_err(|_| "取消状态锁已损坏".to_string())?
        .insert(format!("gen:{run_id}"), cancel.clone());

    let rendered = context::render(&preview);
    let extra = input.extra_instruction.unwrap_or_default();
    let run_for_task = run_id.clone();
    let project_for_task = project_id.clone();
    let scene_for_task = scene.id.clone();
    let chapter_for_task = chapter.id.clone();
    let context_hash = preview.context_hash.clone();
    let result_hash = context_hash.clone();
    let budget = preview.budget;
    let model_name = config.model.clone();
    let style_for_task = input.style_profile_id.clone();
    let app_for_task = app.clone();
    tauri::async_runtime::spawn(async move {
        let client = match LlmClient::new(&config, key) {
            Ok(client) => client,
            Err(e) => {
                finish_error(&app_for_task, &run_for_task, &e.to_string());
                return;
            }
        };
        let messages = vec![
            ChatTurn::system(generation_system_prompt()),
            ChatTurn::user(format!(
                "{rendered}\n\n[EXTRA INSTRUCTION]\n{}",
                if extra.trim().is_empty() {
                    "无"
                } else {
                    extra.as_str()
                }
            )),
        ];
        let emitter = app_for_task.clone();
        let run_for_emit = run_for_task.clone();
        let cancel_for_task = cancel.clone();
        let result = client
            .stream(&messages, cancel_for_task.clone(), move |delta| {
                let _ = emitter.emit(
                    "generation://delta",
                    GenerationDelta {
                        run_id: run_for_emit.clone(),
                        delta: delta.to_string(),
                    },
                );
            })
            .await;

        let state = app_for_task.state::<AppState>();
        if let Ok(mut map) = state.cancellations.lock() {
            map.remove(&format!("gen:{run_for_task}"));
        }
        match result {
            Ok(text) => {
                let outcome = (|| -> Result<String, String> {
                    let mut guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
                    let loaded = guard.as_mut().ok_or("尚未打开项目")?;
                    let word_count = text.chars().count();
                    let candidate = candidates::insert(
                        &loaded.conn,
                        &project_for_task,
                        "draft",
                        json!({
                            "text": text,
                            "sceneId": scene_for_task.clone(),
                            "chapterId": chapter_for_task.clone(),
                            "runId": run_for_task.clone(),
                            "mode": "quick",
                            "contextHash": context_hash,
                            "styleProfileId": style_for_task,
                            "wordCount": word_count
                        }),
                        Some(&format!("scene:{scene_for_task}")),
                        None,
                        0.8,
                    )
                    .map_err(|e| e.to_string())?;
                    loaded
                        .conn
                        .execute(
                            "UPDATE generation_run SET status='done', candidate_id=?2 WHERE id=?1",
                            rusqlite::params![run_for_task.clone(), candidate.id],
                        )
                        .map_err(|e| e.to_string())?;
                    let _ = app_for_task.emit(
                        "generation://finished",
                        GenerationFinished {
                            run_id: run_for_task.clone(),
                            candidate_id: Some(candidate.id.clone()),
                            status: "done".into(),
                        },
                    );
                    Ok(candidate.id)
                })();
                if let Err(error) = outcome {
                    finish_error(&app_for_task, &run_for_task, &error);
                }
            }
            Err(error) => {
                let message = if cancel.load(Ordering::Relaxed) {
                    "生成已取消".to_string()
                } else {
                    error.to_string()
                };
                finish_error(&app_for_task, &run_for_task, &message);
            }
        }
    });
    let _ = model_name;
    Ok(GenerationStartResult {
        run_id,
        context_hash: result_hash,
        budget,
    })
}

fn finish_error(app: &tauri::AppHandle, run_id: &str, error: &str) {
    let state = app.state::<AppState>();
    if let Ok(guard) = state.project.lock() {
        if let Some(loaded) = guard.as_ref() {
            let status = if error.contains("取消") { "canceled" } else { "error" };
            let _ = loaded.conn.execute(
                "UPDATE generation_run SET status=?2, error=?3 WHERE id=?1",
                rusqlite::params![run_id, status, error],
            );
        }
    }
    let _ = app.emit(
        "generation://error",
        GenerationError {
            run_id: run_id.to_string(),
            error: error.to_string(),
        },
    );
}

#[tauri::command]
#[specta::specta]
pub fn generation_cancel(state: State<'_, AppState>, run_id: String) -> IpcResult<()> {
    let map = state
        .cancellations
        .lock()
        .map_err(|_| "取消状态锁已损坏".to_string())?;
    if let Some(flag) = map.get(&format!("gen:{run_id}")) {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn generation_runs(state: State<'_, AppState>) -> IpcResult<Vec<GenerationRunRow>> {
    state
        .with_project(|loaded| {
            let mut stmt = loaded.conn.prepare(
                "SELECT id, scene_id, generation_mode, status, model, context_hash, error,
                    created_at, candidate_id FROM generation_run WHERE project_id=?1
                 ORDER BY created_at DESC LIMIT 50",
            )?;
            let rows = stmt
                .query_map([&loaded.summary.id], |row| {
                    Ok(GenerationRunRow {
                        id: row.get(0)?,
                        scene_id: row.get(1)?,
                        mode: row.get(2)?,
                        status: row.get(3)?,
                        model: row.get(4)?,
                        context_hash: row.get(5)?,
                        error: row.get(6)?,
                        created_at: row.get(7)?,
                        candidate_id: row.get(8)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn plot_beats(
    state: State<'_, AppState>,
    input: PlotBeatsInput,
) -> IpcResult<Vec<BeatOption>> {
    if input.instruction.trim().is_empty() {
        return Err("请先说明下一场想发生什么".into());
    }
    let client = super::seed::require_client(&state).map_err(|e| e.to_string())?;
    let (project_id, session, messages, premise) = {
        let guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
        let loaded = guard.as_ref().ok_or("尚未打开项目")?;
        let pid = loaded.summary.id.clone();
        let session = chat::get(&loaded.conn, &pid, &input.session_id).map_err(|e| e.to_string())?;
        let messages = chat::history_window(&loaded.conn, &pid, &input.session_id, 10)
            .map_err(|e| e.to_string())?;
        let premise = seed::active(&loaded.conn, &pid)
            .map_err(|e| e.to_string())?
            .and_then(|p| serde_json::to_value(p).ok());
        (pid, session, messages, premise)
    };
    let mut transcript = String::new();
    for message in messages {
        transcript.push_str(&format!("{}：{}\n", message.role, message.content));
    }
    let raw = client
        .complete(
            &[
                ChatTurn::system(
                    "你是长篇结构编辑。根据作者的要求，给出 3 个差异明显的“下一场”节拍选项，不写整章。\n\
                     只输出 JSON 数组，不要 Markdown。每项：\n\
                     {\"title\":\"节拍名\",\"beat\":\"发生什么\",\"turn\":\"预期如何被打破\",\"cost\":\"代价\",\"sceneGoal\":\"场景目标\",\"sceneConflict\":\"场景冲突\"}\n\
                     三个选项不能只是同一件事的不同措辞。使用中文。",
                ),
                ChatTurn::user(format!(
                    "种子卡：{}\n\n最近对话：\n{}\n\n作者要求：{}",
                    premise
                        .map(|p| p.to_string())
                        .unwrap_or_else(|| "无".into()),
                    transcript,
                    input.instruction.trim()
                )),
            ],
            Some(2500),
        )
        .await
        .map_err(|e| e.to_string())?;
    let beats: Vec<BeatOption> = parse_json_loose(&raw).map_err(|e| e.to_string())?;
    if beats.len() < 3 {
        return Err("模型没有给出 3 个节拍选项，请重试".into());
    }
    let guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
    let loaded = guard.as_ref().ok_or("尚未打开项目")?;
    let assistant = chat::append_message(
        &loaded.conn,
        &project_id,
        &session.id,
        "assistant",
        &raw,
        &client.model,
    )
    .map_err(|e| e.to_string())?;
    for beat in beats.iter().take(3) {
        candidates::insert(
            &loaded.conn,
            &project_id,
            "beat_option",
            serde_json::to_value(beat).map_err(|e| e.to_string())?,
            Some(&format!("chat:{}:{}", session.id, assistant.id)),
            Some(&assistant.id),
            0.7,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(beats.into_iter().take(3).collect())
}

#[tauri::command]
#[specta::specta]
pub fn scene_from_beat(
    state: State<'_, AppState>,
    chapter_id: String,
    beat: BeatOption,
) -> IpcResult<SceneRecord> {
    state
        .with_project(|loaded| {
            scenes::save_scene(
                &loaded.conn,
                &loaded.summary.id,
                &crate::domain::SceneInput {
                    id: None,
                    chapter_id,
                    goal: if beat.scene_goal.is_empty() {
                        beat.title.clone()
                    } else {
                        beat.scene_goal.clone()
                    },
                    conflict: if beat.scene_conflict.is_empty() {
                        beat.cost.clone()
                    } else {
                        beat.scene_conflict.clone()
                    },
                    pov: String::new(),
                    story_time: String::new(),
                    place_id: None,
                    style_profile_id: None,
                    emotion_beat: EmotionBeat {
                        target: beat.title.clone(),
                        intensity: 3,
                        entry: String::new(),
                        exit: String::new(),
                        turn: beat.turn.clone(),
                    },
                    sensory_plan: SensoryPlan {
                        channels: vec!["视觉".into(), "触觉".into()],
                        motifs: vec![],
                        distance: "中景".into(),
                    },
                },
            )
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn chat_continue(
    state: State<'_, AppState>,
    session_id: String,
) -> IpcResult<Candidate> {
    let client = super::seed::require_client(&state).map_err(|e| e.to_string())?;
    let (project_id, session, messages, premise) = {
        let guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
        let loaded = guard.as_ref().ok_or("尚未打开项目")?;
        let pid = loaded.summary.id.clone();
        let session = chat::get(&loaded.conn, &pid, &session_id).map_err(|e| e.to_string())?;
        let messages = chat::history_window(&loaded.conn, &pid, &session_id, 8)
            .map_err(|e| e.to_string())?;
        let premise = seed::active(&loaded.conn, &pid)
            .map_err(|e| e.to_string())?
            .and_then(|p| serde_json::to_value(p).ok());
        (pid, session, messages, premise)
    };
    let mut transcript = String::new();
    for message in &messages {
        transcript.push_str(&format!("{}：{}\n", message.role, message.content));
    }
    let raw = client
        .complete(
            &[
                ChatTurn::system(
                    "根据最近对话续写一段小说草稿，500 至 800 个汉字。\n\
                     只输出正文，不解释，不加标题。保持 POV 限知与人物语气；\n\
                     不要改变已确认事实；结尾留下一个可继续的钩子。",
                ),
                ChatTurn::user(format!(
                    "种子卡：{}\n\n最近对话：\n{}",
                    premise
                        .map(|p| p.to_string())
                        .unwrap_or_else(|| "无".into()),
                    transcript
                )),
            ],
            Some(2400),
        )
        .await
        .map_err(|e| e.to_string())?;
    let guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
    let loaded = guard.as_ref().ok_or("尚未打开项目")?;
    let source_message = messages.last().map(|m| m.id.clone());
    candidates::insert(
        &loaded.conn,
        &project_id,
        "draft",
        json!({
            "text": raw,
            "source": "chat_continue",
            "sessionId": session.id.clone(),
            "wordCount": raw.chars().count(),
            "styleProfileId": null
        }),
        Some(&format!(
            "chat:{}:{}",
            session.id,
            source_message.clone().unwrap_or_default()
        )),
        source_message.as_deref(),
        0.75,
    )
    .map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SceneDraft {
    goal: String,
    conflict: String,
    #[serde(default)]
    pov: String,
    #[serde(default, alias = "story_time")]
    story_time: String,
    #[serde(default)]
    emotion_beat: Option<EmotionBeat>,
    #[serde(default)]
    sensory_plan: Option<SensoryPlan>,
}

#[tauri::command]
#[specta::specta]
pub async fn chat_to_scene(
    state: State<'_, AppState>,
    session_id: String,
    chapter_id: String,
) -> IpcResult<SceneRecord> {
    let client = super::seed::require_client(&state).map_err(|e| e.to_string())?;
    let (project_id, messages) = {
        let guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
        let loaded = guard.as_ref().ok_or("尚未打开项目")?;
        let pid = loaded.summary.id.clone();
        let messages = chat::history_window(&loaded.conn, &pid, &session_id, 10)
            .map_err(|e| e.to_string())?;
        (pid, messages)
    };
    let mut transcript = String::new();
    for message in &messages {
        transcript.push_str(&format!("{}：{}\n", message.role, message.content));
    }
    let raw = client
        .complete(
            &[
                ChatTurn::system(
                    "把最近对话整理成一个可执行的场景契约。只输出 JSON，不要 Markdown：\n\
                     {\"goal\":\"场景目标\",\"conflict\":\"谁阻止、代价\",\"pov\":\"视角人物\",\"storyTime\":\"故事时间\",\n\
                     \"emotionBeat\":{\"target\":\"目标情绪\",\"intensity\":1,\"entry\":\"入场状态\",\"exit\":\"出场状态\",\"turn\":\"转折\"},\n\
                     \"sensoryPlan\":{\"channels\":[\"视觉\",\"触觉\"],\"motifs\":[\"雨\"],\"distance\":\"中景\"}}\n\
                     intensity 必须是 1 至 5。没有依据的字段留空，不要发明世界事实。",
                ),
                ChatTurn::user(transcript),
            ],
            Some(1500),
        )
        .await
        .map_err(|e| e.to_string())?;
    let draft: SceneDraft = parse_json_loose(&raw).map_err(|e| e.to_string())?;
    let guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
    let loaded = guard.as_ref().ok_or("尚未打开项目")?;
    scenes::save_scene(
        &loaded.conn,
        &project_id,
        &crate::domain::SceneInput {
            id: None,
            chapter_id,
            goal: draft.goal,
            conflict: draft.conflict,
            pov: draft.pov,
            story_time: draft.story_time,
            place_id: None,
            style_profile_id: None,
            emotion_beat: draft.emotion_beat.unwrap_or(EmotionBeat {
                target: String::new(),
                intensity: 3,
                entry: String::new(),
                exit: String::new(),
                turn: String::new(),
            }),
            sensory_plan: draft.sensory_plan.unwrap_or(SensoryPlan {
                channels: vec!["视觉".into()],
                motifs: vec![],
                distance: "中景".into(),
            }),
        },
    )
    .map_err(|e| e.to_string())
}

