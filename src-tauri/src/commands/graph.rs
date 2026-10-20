use tauri::State;

use crate::{
    branches,
    domain::{
        BranchRecord, CommunityReport, GraphData, ImpactSet, InsightResult, TimelineItem,
    },
    error::IpcResult,
    graphrag,
    llm::{ChatTurn},
    state::AppState,
};

#[tauri::command]
#[specta::specta]
pub fn branch_list(state: State<'_, AppState>) -> IpcResult<Vec<BranchRecord>> {
    state
        .with_project(|loaded| branches::list(&loaded.conn, &loaded.summary.id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn branch_create(
    state: State<'_, AppState>,
    name: String,
    from_branch_id: Option<String>,
) -> IpcResult<BranchRecord> {
    state
        .with_project(|loaded| {
            branches::create(
                &loaded.conn,
                &loaded.summary.id,
                &name,
                from_branch_id.as_deref(),
            )
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn branch_switch(state: State<'_, AppState>, id: String) -> IpcResult<Vec<BranchRecord>> {
    state
        .with_project(|loaded| {
            branches::switch(&loaded.conn, &loaded.summary.id, &id)?;
            branches::list(&loaded.conn, &loaded.summary.id)
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn timeline_data(
    state: State<'_, AppState>,
    branch_id: Option<String>,
) -> IpcResult<Vec<TimelineItem>> {
    state
        .with_project(|loaded| {
            branches::timeline(&loaded.conn, &loaded.summary.id, branch_id.as_deref())
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn graph_data(state: State<'_, AppState>) -> IpcResult<GraphData> {
    state
        .with_project(|loaded| graphrag::graph_data(&loaded.conn, &loaded.summary.id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn community_build(state: State<'_, AppState>) -> IpcResult<Vec<CommunityReport>> {
    state
        .with_project(|loaded| graphrag::build_communities(&loaded.conn, &loaded.summary.id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn community_list(state: State<'_, AppState>) -> IpcResult<Vec<CommunityReport>> {
    state
        .with_project(|loaded| graphrag::list_communities(&loaded.conn, &loaded.summary.id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn impact_analysis(
    state: State<'_, AppState>,
    entity_id: String,
) -> IpcResult<ImpactSet> {
    state
        .with_project(|loaded| {
            graphrag::require_entity(&loaded.conn, &loaded.summary.id, &entity_id)?;
            graphrag::impact(&loaded.conn, &loaded.summary.id, &entity_id)
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn insight_query(
    state: State<'_, AppState>,
    question: String,
) -> IpcResult<InsightResult> {
    if question.trim().is_empty() {
        return Err("请输入一个全书问题".into());
    }
    let client = super::seed::require_client(&state).map_err(|e| e.to_string())?;
    let (project_id, context, sources, community_ids) = {
        let guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
        let loaded = guard.as_ref().ok_or("尚未打开项目")?;
        let (context, sources, ids) =
            graphrag::insight_context(&loaded.conn, &loaded.summary.id, &question)
                .map_err(|e| e.to_string())?;
        (loaded.summary.id.clone(), context, sources, ids)
    };
    if context.trim().is_empty() {
        return Err("还没有社区报告，请先建立叙事图与社区摘要".into());
    }
    let answer = client
        .complete(
            &[
                ChatTurn::system(
                    "你是全书叙事分析助手。根据社区报告回答问题。\n\
                     社区报告是派生摘要，不是权威事实；结论必须引用来源 ID。\n\
                     如果报告不足以回答，明确说明缺少什么，不要补写事实。使用中文。",
                ),
                ChatTurn::user(format!("问题：{question}\n\n社区报告：\n{context}")),
            ],
            Some(1800),
        )
        .await
        .map_err(|e| e.to_string())?;
    let guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
    let loaded = guard.as_ref().ok_or("尚未打开项目")?;
    graphrag::save_insight(
        &loaded.conn,
        &project_id,
        &question,
        &answer,
        &sources,
        &community_ids,
    )
    .map_err(|e| e.to_string())?;
    Ok(InsightResult {
        question,
        answer,
        source_ids: sources,
        community_ids,
    })
}
