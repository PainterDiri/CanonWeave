use serde_json::Value;
use tauri::State;

use crate::{
    blueprint,
    domain::{
        NarrativeBlueprintEditInput, NarrativeBlueprintView, NarrativeGenerateInput,
        NarrativeLogicNode,
    },
    error::IpcResult,
    llm::{
        narrative_chain_system_prompt, narrative_chain_user_prompt,
        narrative_foundation_system_prompt, narrative_foundation_user_prompt,
        narrative_repair_system_prompt, narrative_repair_user_prompt, parse_json_loose, ChatTurn,
    },
    seed,
    state::AppState,
};

#[tauri::command]
#[specta::specta]
pub fn blueprint_current(
    state: State<'_, AppState>,
) -> IpcResult<Option<NarrativeBlueprintView>> {
    state
        .with_project(|loaded| blueprint::current(&loaded.conn, &loaded.summary.id))
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn blueprint_candidate(
    state: State<'_, AppState>,
) -> IpcResult<Option<NarrativeBlueprintView>> {
    state
        .with_project(|loaded| blueprint::candidate(&loaded.conn, &loaded.summary.id))
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn blueprint_generate(
    state: State<'_, AppState>,
    input: NarrativeGenerateInput,
) -> IpcResult<NarrativeBlueprintView> {
    if !["novel", "game"].contains(&input.medium.as_str()) {
        return Err("媒介必须是 novel 或 game".into());
    }
    if !["adaptive", "fixed"].contains(&input.structure_strategy.as_str()) {
        return Err("编排策略必须是 adaptive 或 fixed".into());
    }
    if input.structure_strategy == "fixed" && input.target_units.unwrap_or(0) < 1 {
        return Err("固定结构必须填写目标章节或任务数".into());
    }
    let client = super::seed::require_client(&state).map_err(|error| error.to_string())?;
    let (project_id, premise_id, premise) = {
        let guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
        let loaded = guard.as_ref().ok_or("尚未打开项目")?;
        let premise = seed::active(&loaded.conn, &loaded.summary.id)
            .map_err(|error| error.to_string())?
            .ok_or("请先确认种子卡，再建立叙事蓝本")?;
        let value = serde_json::to_value(&premise).map_err(|error| error.to_string())?;
        (loaded.summary.id.clone(), premise.id, value)
    };

    let foundation_raw = client
        .complete(
            &[
                ChatTurn::system(narrative_foundation_system_prompt()),
                ChatTurn::user(narrative_foundation_user_prompt(
                    &premise,
                    &input.medium,
                    &input.structure_strategy,
                    input.target_units,
                )),
            ],
            Some(client.max_tokens.max(9000)),
        )
        .await
        .map_err(|error| error.to_string())?;
    let mut foundation: Value =
        parse_json_loose(&foundation_raw).map_err(|error| format!("叙事地基解析失败：{error}"))?;

    let chain_raw = client
        .complete(
            &[
                ChatTurn::system(narrative_chain_system_prompt()),
                ChatTurn::user(narrative_chain_user_prompt(
                    &foundation,
                    &input.medium,
                    &input.structure_strategy,
                )),
            ],
            Some(client.max_tokens.max(9000)),
        )
        .await
        .map_err(|error| error.to_string())?;
    let mut chain: Value =
        parse_json_loose(&chain_raw).map_err(|error| format!("逻辑链解析失败：{error}"))?;

    let mut last_view = None;
    for attempt in 0..=2 {
        let view = {
            let mut guard = state
                .project
                .lock()
                .map_err(|_| "项目状态锁已损坏".to_string())?;
            let loaded = guard.as_mut().ok_or("尚未打开项目")?;
            blueprint::save_generated(
                &mut loaded.conn,
                &project_id,
                &premise_id,
                &input,
                &foundation,
                &chain,
            )
            .map_err(|error| error.to_string())?
        };
        let errors: Vec<_> = view
            .diagnostics
            .iter()
            .filter(|item| item.severity == "error")
            .cloned()
            .collect();
        if errors.is_empty() || attempt == 2 {
            return Ok(view);
        }
        last_view = Some(view);
        let diagnostics = serde_json::to_value(&errors).map_err(|error| error.to_string())?;
        let repair_raw = match client
            .complete(
                &[
                    ChatTurn::system(narrative_repair_system_prompt()),
                    ChatTurn::user(narrative_repair_user_prompt(
                        &foundation,
                        &chain,
                        &diagnostics,
                    )),
                ],
                Some(client.max_tokens.max(9000)),
            )
            .await
        {
            Ok(raw) => raw,
            Err(_) => return Ok(last_view.expect("last candidate view exists")),
        };
        let repaired: Value = match parse_json_loose(&repair_raw) {
            Ok(value) => value,
            Err(_) => return Ok(last_view.expect("last candidate view exists")),
        };
        let Some(next_foundation) = repaired.get("foundation").cloned() else {
            return Ok(last_view.expect("last candidate view exists"));
        };
        let Some(next_chain) = repaired.get("chain").cloned() else {
            return Ok(last_view.expect("last candidate view exists"));
        };
        foundation = next_foundation;
        chain = next_chain;
    }

    last_view.ok_or_else(|| "叙事蓝本生成失败".into())
}

#[tauri::command]
#[specta::specta]
pub fn blueprint_update(
    state: State<'_, AppState>,
    input: NarrativeBlueprintEditInput,
) -> IpcResult<NarrativeBlueprintView> {
    let mut guard = state
        .project
        .lock()
        .map_err(|_| "项目状态锁已损坏".to_string())?;
    let loaded = guard.as_mut().ok_or("尚未打开项目")?;
    blueprint::update_candidate(&mut loaded.conn, &loaded.summary.id, &input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn blueprint_confirm(
    state: State<'_, AppState>,
    blueprint_id: String,
) -> IpcResult<NarrativeBlueprintView> {
    let mut guard = state
        .project
        .lock()
        .map_err(|_| "项目状态锁已损坏".to_string())?;
    let loaded = guard.as_mut().ok_or("尚未打开项目")?;
    blueprint::confirm(&mut loaded.conn, &loaded.summary.id, &blueprint_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn blueprint_logic_context(
    state: State<'_, AppState>,
    limit: Option<i32>,
) -> IpcResult<String> {
    state
        .with_project(|loaded| {
            blueprint::logic_context(&loaded.conn, &loaded.summary.id, limit.unwrap_or(80))
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn blueprint_nodes(
    state: State<'_, AppState>,
) -> IpcResult<Vec<NarrativeLogicNode>> {
    state
        .with_project(|loaded| {
            Ok(blueprint::current(&loaded.conn, &loaded.summary.id)?
                .map(|view| view.nodes)
                .unwrap_or_default())
        })
        .map_err(|error| error.to_string())
}
