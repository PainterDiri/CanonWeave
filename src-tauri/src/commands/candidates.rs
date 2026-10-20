use tauri::State;

use crate::{
    candidates,
    domain::{
        BatchPreview, BatchResult, Candidate, CandidateAcceptResult, ReviewQueue,
    },
    error::IpcResult,
    state::AppState,
};

#[tauri::command]
#[specta::specta]
pub fn review_queue(state: State<'_, AppState>) -> IpcResult<ReviewQueue> {
    state
        .with_project(|loaded| candidates::queue(&loaded.conn, &loaded.summary.id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn candidate_list(
    state: State<'_, AppState>,
    status: Option<String>,
    kind: Option<String>,
) -> IpcResult<Vec<Candidate>> {
    state
        .with_project(|loaded| {
            candidates::list(
                &loaded.conn,
                &loaded.summary.id,
                status.as_deref(),
                kind.as_deref(),
            )
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn candidate_accept(state: State<'_, AppState>, id: String) -> IpcResult<CandidateAcceptResult> {
    let mut guard = state.project.lock().map_err(|_| "项目状态锁已损坏".to_string())?;
    let loaded = guard.as_mut().ok_or("尚未打开项目")?;
    candidates::accept(&mut loaded.conn, &loaded.summary.id, &id).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn candidate_reject(state: State<'_, AppState>, id: String) -> IpcResult<Candidate> {
    state
        .with_project(|loaded| candidates::reject(&loaded.conn, &loaded.summary.id, &id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn candidate_update_payload(
    state: State<'_, AppState>,
    id: String,
    payload_json: String,
) -> IpcResult<Candidate> {
    let payload: serde_json::Value =
        serde_json::from_str(&payload_json).map_err(|e| format!("候选内容不是合法 JSON：{e}"))?;
    state
        .with_project(|loaded| {
            candidates::update_payload(&loaded.conn, &loaded.summary.id, &id, payload)
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn review_batch_preview(
    state: State<'_, AppState>,
    ids: Option<Vec<String>>,
) -> IpcResult<BatchPreview> {
    state
        .with_project(|loaded| {
            let pending = candidates::list(&loaded.conn, &loaded.summary.id, Some("pending"), None)?;
            let selected: Vec<_> = pending
                .into_iter()
                .filter(|candidate| {
                    ids.as_ref()
                        .map(|ids| ids.contains(&candidate.id))
                        .unwrap_or(true)
                })
                .collect();
            let eligible: Vec<String> = selected
                .iter()
                .filter(|candidate| candidate.risk_level == "L1")
                .map(|candidate| candidate.id.clone())
                .collect();
            let l1 = selected.iter().filter(|c| c.risk_level == "L1").count() as i32;
            let l2 = selected.iter().filter(|c| c.risk_level == "L2").count() as i32;
            let l3 = selected.iter().filter(|c| c.risk_level == "L3").count() as i32;
            let blocked = selected
                .iter()
                .filter(|candidate| candidate.risk_level != "L1")
                .map(|candidate| candidate.id.clone())
                .collect();
            Ok(BatchPreview {
                count: eligible.len() as i32,
                ids: eligible,
                l1,
                l2,
                l3,
                blocked,
            })
        })
        .map_err(|e| e.to_string())
}

/// 批量接受只处理 L1。创建单一 revision 记录批次；接受动作逐条仍保持证据链。
#[tauri::command]
#[specta::specta]
pub fn review_batch_apply(
    state: State<'_, AppState>,
    ids: Vec<String>,
    chapter_id: Option<String>,
) -> IpcResult<BatchResult> {
    let mut guard = state.project.lock().map_err(|_| "项目状态锁已损坏".to_string())?;
    let loaded = guard.as_mut().ok_or("尚未打开项目")?;
    let (batch_id, revision_id, accepted) = candidates::batch_accept_l1(
        &mut loaded.conn,
        &loaded.summary.id,
        &ids,
        chapter_id.as_deref(),
    )
    .map_err(|e| e.to_string())?;
    Ok(BatchResult {
        batch_id,
        revision_id,
        accepted,
        rejected: 0,
    })
}
