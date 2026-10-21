use tauri::State;

use crate::{
    branches,
    domain::{
        BranchRecord, CommunityReport, CommunitySummaryResult, GraphData, ImpactSet, InsightResult,
        TimelineItem,
    },
    error::IpcResult,
    graphrag,
    llm::ChatTurn,
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
pub async fn community_summarize(state: State<'_, AppState>) -> IpcResult<CommunitySummaryResult> {
    let client = super::seed::require_client(&state).map_err(|error| error.to_string())?;
    let (project_id, pending, preserved, requires_llm) = {
        let guard = state
            .project
            .lock()
            .map_err(|_| "项目状态锁已损坏".to_string())?;
        let loaded = guard.as_ref().ok_or("尚未打开项目")?;
        let reports = graphrag::list_communities(&loaded.conn, &loaded.summary.id)
            .map_err(|error| error.to_string())?;
        let requires_llm = reports
            .iter()
            .filter(|report| report.summary_mode != "llm" || report.stale_at.is_some())
            .count() as i32;
        let preserved = reports.len() as i32 - requires_llm;
        let pending = reports
            .into_iter()
            .filter(|report| report.summary_mode != "llm" || report.stale_at.is_some())
            .take(8)
            .map(|report: CommunityReport| {
                graphrag::community_summary_input(&loaded.conn, &loaded.summary.id, &report.id)
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        (loaded.summary.id.clone(), pending, preserved, requires_llm)
    };
    let mut completed = Vec::new();
    let mut failed = Vec::new();
    for input in pending {
        let prompt = format!(
            "社区标题：{}\n层级：L{}\n允许引用的来源 ID：{}\n\n来源证据：\n{}",
            input.report.title,
            input.report.level,
            input.source_ids.join("、"),
            input.evidence
        );
        let result = client
            .complete(
                &[
                    ChatTurn::system(
                        "你是 Narrative Wiki 的社区摘要编译器。只能依据用户提供的来源证据写 180 至 420 字中文摘要。\n\
                         必须区分已确认事实、关系结构与叙事风险，不得补写来源中没有的事实，不得把摘要说成权威事实。\n\
                         每个关键判断后必须用方括号引用至少一个“允许引用的来源 ID”，例如 [claim-id]。\n\
                         不要引用实体名称或不在允许列表中的 ID。只输出摘要正文。",
                    ),
                    ChatTurn::user(prompt),
                ],
                Some(1800),
            )
            .await;
        match result {
            Ok(summary) => match graphrag::validate_grounded_summary(&summary, &input.source_ids) {
                Ok(cited) => {
                    completed.push((input.report.id, input.report.source_hash, summary, cited))
                }
                Err(error) => failed.push(format!("{}：{error}", input.report.title)),
            },
            Err(error) => failed.push(format!("{}：{error}", input.report.title)),
        }
    }
    let compiled = completed.len() as i32;
    {
        let guard = state
            .project
            .lock()
            .map_err(|_| "项目状态锁已损坏".to_string())?;
        let loaded = guard.as_ref().ok_or("尚未打开项目")?;
        let tx = loaded
            .conn
            .unchecked_transaction()
            .map_err(|error| error.to_string())?;
        for (id, source_hash, summary, cited) in &completed {
            graphrag::apply_llm_summary(&tx, &project_id, id, source_hash, summary, cited)
                .map_err(|error| error.to_string())?;
        }
        tx.commit().map_err(|error| error.to_string())?;
    }
    let reports = state
        .with_project(|loaded| graphrag::list_communities(&loaded.conn, &loaded.summary.id))
        .map_err(|error| error.to_string())?;
    let remaining = (requires_llm - compiled).max(0);
    Ok(CommunitySummaryResult {
        reports,
        compiled,
        preserved,
        remaining,
        failed,
    })
}

#[tauri::command]
#[specta::specta]
pub fn impact_analysis(state: State<'_, AppState>, entity_id: String) -> IpcResult<ImpactSet> {
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
    let (semantic, embedding_error) = super::embedding::semantic_query(&state, &question).await;
    let (project_id, insight) = {
        let guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
        let loaded = guard.as_ref().ok_or("尚未打开项目")?;
        let insight = graphrag::insight_context(
            &loaded.conn,
            &loaded.summary.id,
            &question,
            semantic.as_ref(),
            embedding_error,
        )
        .map_err(|e| e.to_string())?;
        (loaded.summary.id.clone(), insight)
    };
    if insight.context.trim().is_empty() {
        return Err("没有召回可用证据。请先确认真实事实或建立叙事图与社区摘要".into());
    }
    let answer = client
        .complete(
            &[
                ChatTurn::system(
                    "你是全书叙事分析助手。根据实体链接、PPR 子图、混合检索证据和社区报告回答问题。\n\
                     社区报告与检索片段都是派生视图，不是权威事实；不得编造原文、来源或因果。\n\
                     每个关键判断后使用方括号引用来源 ID；可以使用社区 ID 标识跨社区结论。\n\
                     如果证据不足，明确说明缺少什么。使用中文。",
                ),
                ChatTurn::user(format!(
                    "问题：{question}\n\n检索模式：{}\n\n证据与社区上下文：\n{}",
                    insight.retrieval_mode, insight.context
                )),
            ],
            Some(1800),
        )
        .await
        .map_err(|e| e.to_string())?;
    let answer = if answer.contains('[') {
        answer
    } else {
        format!(
            "{}\n\n检索来源：{}",
            answer.trim(),
            insight.source_ids.join("、")
        )
    };
    let guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
    let loaded = guard.as_ref().ok_or("尚未打开项目")?;
    graphrag::save_insight(
        &loaded.conn,
        &project_id,
        &question,
        &answer,
        &insight.source_ids,
        &insight.community_ids,
    )
    .map_err(|e| e.to_string())?;
    Ok(InsightResult {
        question,
        answer,
        source_ids: insight.source_ids,
        community_ids: insight.community_ids,
        entity_ids: insight.entity_ids,
        retrieval_mode: insight.retrieval_mode,
        embedding_error: insight.embedding_error,
    })
}
