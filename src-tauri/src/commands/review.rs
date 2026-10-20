use tauri::State;

use crate::{
    domain::{
        BatchUndoResult, PolicyApplyResult, ReviewPolicy, ReviewPolicyInput, ReviewSampleRow,
    },
    error::IpcResult,
    review,
    state::AppState,
};

#[tauri::command]
#[specta::specta]
pub fn review_policy_get(
    state: State<'_, AppState>,
    scope: Option<String>,
    scope_id: Option<String>,
) -> IpcResult<ReviewPolicy> {
    state
        .with_project(|loaded| {
            review::get(
                &loaded.conn,
                &loaded.summary.id,
                scope.as_deref().unwrap_or("project"),
                scope_id.as_deref(),
            )
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn review_policy_save(
    state: State<'_, AppState>,
    input: ReviewPolicyInput,
) -> IpcResult<ReviewPolicy> {
    let mut guard = state.project.lock().map_err(|_| "项目状态锁已损坏".to_string())?;
    let loaded = guard.as_mut().ok_or("尚未打开项目")?;
    review::save(&mut loaded.conn, &loaded.summary.id, &input).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn review_policy_apply(
    state: State<'_, AppState>,
    candidate_ids: Option<Vec<String>>,
) -> IpcResult<PolicyApplyResult> {
    let mut guard = state.project.lock().map_err(|_| "项目状态锁已损坏".to_string())?;
    let loaded = guard.as_mut().ok_or("尚未打开项目")?;
    let policy = review::get(&loaded.conn, &loaded.summary.id, "project", None)
        .map_err(|e| e.to_string())?;
    review::apply(
        &mut loaded.conn,
        &loaded.summary.id,
        &policy,
        candidate_ids.as_deref(),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn review_batch_undo(
    state: State<'_, AppState>,
    batch_id: String,
) -> IpcResult<BatchUndoResult> {
    let mut guard = state.project.lock().map_err(|_| "项目状态锁已损坏".to_string())?;
    let loaded = guard.as_mut().ok_or("尚未打开项目")?;
    let (revision_id, restored) =
        crate::candidates::undo_batch(&mut loaded.conn, &loaded.summary.id, &batch_id)
            .map_err(|e| e.to_string())?;
    Ok(BatchUndoResult {
        batch_id,
        revision_id,
        restored,
    })
}

#[tauri::command]
#[specta::specta]
pub fn review_sample_list(state: State<'_, AppState>) -> IpcResult<Vec<ReviewSampleRow>> {
    state
        .with_project(|loaded| {
            let mut stmt = loaded.conn.prepare(
                "SELECT id, batch_id, candidate_id, sampled, review_status, created_at
                 FROM review_sample WHERE project_id=?1 ORDER BY created_at DESC LIMIT 200",
            )?;
            let rows = stmt
                .query_map([&loaded.summary.id], |row| {
                    Ok(ReviewSampleRow {
                        id: row.get(0)?,
                        batch_id: row.get(1)?,
                        candidate_id: row.get(2)?,
                        sampled: row.get(3)?,
                        review_status: row.get(4)?,
                        created_at: row.get(5)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .map_err(|e| e.to_string())
}
