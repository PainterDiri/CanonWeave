use tauri::State;

use crate::{domain::HybridSearchResult, error::IpcResult, retrieval, state::AppState};

#[tauri::command]
#[specta::specta]
pub async fn hybrid_search(
    state: State<'_, AppState>,
    query: String,
    limit: Option<i32>,
) -> IpcResult<HybridSearchResult> {
    if query.trim().is_empty() {
        return Err("检索问题不能为空".into());
    }
    let (semantic, embedding_error) = super::embedding::semantic_query(&state, query.trim()).await;
    state
        .with_project(|loaded| {
            retrieval::hybrid_search(
                &loaded.conn,
                &loaded.summary.id,
                query.trim(),
                limit.unwrap_or(12).clamp(1, 50) as usize,
                semantic.as_ref(),
                embedding_error.clone(),
            )
        })
        .map_err(|error| error.to_string())
}
