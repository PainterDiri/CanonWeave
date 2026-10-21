use tauri::State;

use crate::{
    domain::{
        Candidate, WikiEditInput, WikiLlmCompileResult, WikiPage, WikiPageDependency,
        WikiRebuildResult,
    },
    error::IpcResult,
    graphrag,
    llm::ChatTurn,
    state::AppState,
    wiki,
};

#[tauri::command]
#[specta::specta]
pub fn wiki_list(state: State<'_, AppState>) -> IpcResult<Vec<WikiPage>> {
    state
        .with_project(|loaded| wiki::list(&loaded.conn, &loaded.summary.id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn wiki_rebuild(state: State<'_, AppState>) -> IpcResult<WikiRebuildResult> {
    let mut guard = state
        .project
        .lock()
        .map_err(|_| "项目状态锁已损坏".to_string())?;
    let loaded = guard.as_mut().ok_or("尚未打开项目")?;
    wiki::rebuild(&mut loaded.conn, &loaded.summary.id).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn wiki_dependencies(state: State<'_, AppState>) -> IpcResult<Vec<WikiPageDependency>> {
    state
        .with_project(|loaded| wiki::list_dependencies(&loaded.conn, &loaded.summary.id))
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn wiki_compile_llm(
    state: State<'_, AppState>,
    limit: Option<i32>,
) -> IpcResult<WikiLlmCompileResult> {
    let client = super::seed::require_client(&state).map_err(|error| error.to_string())?;
    let (project_id, plan) = state
        .with_project(|loaded| {
            wiki::chapter_summary_plan(&loaded.conn, &loaded.summary.id, limit.unwrap_or(4))
                .map(|plan| (loaded.summary.id.clone(), plan))
        })
        .map_err(|error| error.to_string())?;
    let skipped = plan.skipped;
    let mut completed = Vec::new();
    let mut failed = Vec::new();
    for job in plan.jobs {
        let chunks = wiki::chunk_chapter_text(&job.text, 7000);
        let chunk_ids = (0..chunks.len())
            .map(|index| format!("chapter:{}:chunk:{}", job.chapter_id, index + 1))
            .collect::<Vec<_>>();
        let mut chunk_summaries = Vec::new();
        let mut chunk_failed = false;
        for (index, chunk) in chunks.iter().enumerate() {
            let source_id = &chunk_ids[index];
            let result = client
                .complete(
                    &[
                        ChatTurn::system(
                            "你是 Narrative Wiki 的章节摘要编译器。只根据给出的章节正文分段，写 140 至 260 字中文事实摘要。\n\
                             保留关键行动、人物目标、关系变化、结果与未解决问题；不评价文笔，不推断正文没有的信息。\n\
                             摘要必须用方括号引用给定来源 ID，例如 [chapter:abc:chunk:1]。只输出摘要正文。",
                        ),
                        ChatTurn::user(format!(
                            "章节：{}\n允许引用的来源 ID：{source_id}\n\n正文分段：\n{chunk}",
                            job.title
                        )),
                    ],
                    Some(900),
                )
                .await;
            match result {
                Ok(summary) => {
                    match graphrag::validate_grounded_summary(
                        &summary,
                        std::slice::from_ref(source_id),
                    ) {
                        Ok(_) => chunk_summaries.push(format!("{source_id}：{summary}")),
                        Err(error) => {
                            failed.push(format!("{} 分段 {}：{error}", job.title, index + 1));
                            chunk_failed = true;
                            break;
                        }
                    }
                }
                Err(error) => {
                    failed.push(format!("{} 分段 {}：{error}", job.title, index + 1));
                    chunk_failed = true;
                    break;
                }
            }
        }
        if chunk_failed || chunk_summaries.is_empty() {
            continue;
        }
        let final_result = if chunk_summaries.len() == 1 {
            Ok(chunk_summaries[0]
                .split_once('：')
                .map(|(_, summary)| summary.to_string())
                .unwrap_or_else(|| chunk_summaries[0].clone()))
        } else {
            client
                .complete(
                    &[
                        ChatTurn::system(
                            "你是 Narrative Wiki 的章节摘要编译器。合并各分段摘要，输出 220 至 420 字中文总摘要。\n\
                             不得加入分段摘要之外的信息；必须保留关键人物、关系变化、结果与未解决问题。\n\
                             每个关键判断后引用对应分段来源 ID，只能使用允许列表中的 ID。只输出摘要正文。",
                        ),
                        ChatTurn::user(format!(
                            "章节：{}\n允许引用的来源 ID：{}\n\n分段摘要：\n{}",
                            job.title,
                            chunk_ids.join("、"),
                            chunk_summaries.join("\n")
                        )),
                    ],
                    Some(1400),
                )
                .await
        };
        match final_result {
            Ok(summary) => match graphrag::validate_grounded_summary(&summary, &chunk_ids) {
                Ok(cited) => completed.push((
                    job.page_id,
                    job.source_hash,
                    job.chapter_id,
                    summary,
                    cited,
                    chunk_ids,
                )),
                Err(error) => failed.push(format!("{}：{error}", job.title)),
            },
            Err(error) => failed.push(format!("{}：{error}", job.title)),
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
        for (page_id, source_hash, chapter_id, summary, cited, chunk_ids) in &completed {
            wiki::apply_llm_chapter_summary(
                &tx,
                &project_id,
                page_id,
                source_hash,
                chapter_id,
                summary,
                cited,
                chunk_ids,
            )
            .map_err(|error| error.to_string())?;
        }
        tx.commit().map_err(|error| error.to_string())?;
    }
    let (pages, remaining) = state
        .with_project(|loaded| {
            let pages = wiki::list(&loaded.conn, &loaded.summary.id)?;
            let remaining =
                wiki::chapter_summary_plan(&loaded.conn, &loaded.summary.id, 1)?.remaining;
            Ok((pages, remaining))
        })
        .map_err(|error| error.to_string())?;
    Ok(WikiLlmCompileResult {
        pages,
        compiled,
        skipped,
        remaining,
        failed,
    })
}

#[tauri::command]
#[specta::specta]
pub fn wiki_propose_edit(state: State<'_, AppState>, input: WikiEditInput) -> IpcResult<Candidate> {
    state
        .with_project(|loaded| wiki::propose_edit(&loaded.conn, &loaded.summary.id, &input))
        .map_err(|e| e.to_string())
}
