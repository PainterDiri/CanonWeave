use rusqlite::params;
use tauri::State;

use crate::{
    candidates, context,
    domain::{
        ChapterRecord, ContextPreview, DraftAcceptResult, SceneInput, SceneRecord,
    },
    error::IpcResult,
    scenes, state::AppState,
};

#[tauri::command]
#[specta::specta]
pub fn chapter_list(state: State<'_, AppState>) -> IpcResult<Vec<ChapterRecord>> {
    state
        .with_project(|loaded| scenes::chapters(&loaded.conn, &loaded.summary.id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn scene_list(
    state: State<'_, AppState>,
    chapter_id: Option<String>,
) -> IpcResult<Vec<SceneRecord>> {
    state
        .with_project(|loaded| {
            scenes::scenes(&loaded.conn, &loaded.summary.id, chapter_id.as_deref())
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn scene_save(state: State<'_, AppState>, input: SceneInput) -> IpcResult<SceneRecord> {
    state
        .with_project(|loaded| scenes::save_scene(&loaded.conn, &loaded.summary.id, &input))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn scene_contract_text(state: State<'_, AppState>, scene_id: String) -> IpcResult<String> {
    state
        .with_project(|loaded| {
            let scene = scenes::get_scene(&loaded.conn, &loaded.summary.id, &scene_id)?;
            scenes::scene_contract(&loaded.conn, &loaded.summary.id, &scene)
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn context_preview(
    state: State<'_, AppState>,
    scene_id: String,
    budget: Option<i32>,
    style_profile_id: Option<String>,
) -> IpcResult<ContextPreview> {
    state
        .with_project(|loaded| {
            context::compile(
                &loaded.conn,
                &loaded.summary.id,
                &scene_id,
                budget.unwrap_or(12000),
                style_profile_id.as_deref(),
            )
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn chapter_text_get(state: State<'_, AppState>, chapter_id: String) -> IpcResult<String> {
    state
        .with_project(|loaded| scenes::chapter_text(&loaded.conn, &loaded.summary.id, &chapter_id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn chapter_text_save(
    state: State<'_, AppState>,
    chapter_id: String,
    text: String,
) -> IpcResult<DraftAcceptResult> {
    let mut guard = state.project.lock().map_err(|_| "项目状态锁已损坏".to_string())?;
    let loaded = guard.as_mut().ok_or("尚未打开项目")?;
    scenes::commit_text(
        &mut loaded.conn,
        &loaded.summary.id,
        &chapter_id,
        &text,
        "manual_save",
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn draft_accept(
    state: State<'_, AppState>,
    candidate_id: String,
    chapter_id: String,
) -> IpcResult<DraftAcceptResult> {
    accept_draft(&state, &candidate_id, &chapter_id, true, false)
}

#[tauri::command]
#[specta::specta]
pub fn draft_accept_variant(
    state: State<'_, AppState>,
    candidate_id: String,
    chapter_id: String,
    use_refined: bool,
    acknowledge_fact_changes: bool,
) -> IpcResult<DraftAcceptResult> {
    accept_draft(
        &state,
        &candidate_id,
        &chapter_id,
        use_refined,
        acknowledge_fact_changes,
    )
}

fn accept_draft(
    state: &State<'_, AppState>,
    candidate_id: &str,
    chapter_id: &str,
    use_refined: bool,
    acknowledge_fact_changes: bool,
) -> IpcResult<DraftAcceptResult> {
    let mut guard = state.project.lock().map_err(|_| "项目状态锁已损坏".to_string())?;
    let loaded = guard.as_mut().ok_or("尚未打开项目")?;
    let candidate = candidates::get(&loaded.conn, &loaded.summary.id, candidate_id)
        .map_err(|e| e.to_string())?;
    if !["draft", "draft_refined"].contains(&candidate.kind.as_str()) {
        return Err("该候选不是正文草稿".into());
    }
    if candidate.status != "pending" {
        return Err("该草稿已经处理".into());
    }
    let blocked = candidate
        .payload
        .get("factDelta")
        .and_then(|value| value.get("blocked"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if use_refined && blocked && !acknowledge_fact_changes {
        return Err("精修版本检测到硬事实变化，必须选择原文或明确确认修改".into());
    }
    let text = candidate
        .payload
        .get(if use_refined { "refinedText" } else { "rawText" })
        .or_else(|| candidate.payload.get("text"))
        .and_then(serde_json::Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| "草稿缺少正文".to_string())?;
    let result = scenes::commit_text(
        &mut loaded.conn,
        &loaded.summary.id,
        chapter_id,
        text,
        if candidate.kind == "draft_refined" && acknowledge_fact_changes {
            "refined_accept_acknowledged"
        } else {
            "draft_accept"
        },
    )
    .map_err(|e| e.to_string())?;
    loaded
        .conn
        .execute(
            "UPDATE candidate SET status='accepted', confirmed_at=?3 WHERE id=?1 AND project_id=?2",
            params![candidate_id, loaded.summary.id, crate::now()],
        )
        .map_err(|e| e.to_string())?;
    Ok(result)
}
