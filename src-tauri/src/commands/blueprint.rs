use serde_json::{json, Value};
use tauri::{Emitter, State};

use crate::{
    blueprint,
    domain::{
        NarrativeBlueprintEditInput, NarrativeBlueprintView, NarrativeGenerateInput,
        NarrativeLogicNode,
    },
    error::IpcResult,
    llm::{
        is_transient_model_error, narrative_chain_system_prompt, narrative_chain_user_prompt,
        narrative_foundation_system_prompt, narrative_foundation_user_prompt,
        narrative_repair_system_prompt, narrative_repair_user_prompt, parse_json_loose,
        world_layer_system_prompt, world_layer_user_prompt, ChatTurn, LlmClient,
    },
    seed,
    state::AppState,
};

async fn complete_blueprint_request(
    client: &LlmClient,
    messages: &[ChatTurn],
    max_tokens: i32,
) -> Result<String, String> {
    let mut last_error = String::new();
    for attempt in 0..3 {
        match client.complete_streaming(messages, max_tokens).await {
            Ok(text) if !text.trim().is_empty() => return Ok(text),
            Ok(_) => {
                last_error = "模型没有返回文本".into();
            }
            Err(error) => {
                let message = error.to_string();
                let stream_unsupported = message.contains("400")
                    || message.to_lowercase().contains("stream")
                    || message.contains("流式");
                if stream_unsupported && !is_transient_model_error(&message) {
                    return client
                        .complete(messages, Some(max_tokens))
                        .await
                        .map_err(|fallback| fallback.to_string());
                }
                if !is_transient_model_error(&message) {
                    return Err(message);
                }
                last_error = message;
            }
        }
        if attempt < 2 {
            tokio::time::sleep(std::time::Duration::from_secs((attempt + 1) as u64)).await;
        }
    }
    Err(format!(
        "{last_error}。已自动重试 3 次仍失败，请检查模型服务状态或稍后再试"
    ))
}

const WORLD_LAYERS: [(&str, &str); 5] = [
    ("world", "世界运行规律与能力体系"),
    ("factions", "势力格局"),
    ("cast", "人物群像"),
    ("synthesis", "总体戏剧结构与暗线"),
    ("critique", "设定漏洞审查与修订"),
];

fn emit_progress(app: &tauri::AppHandle, step: usize, total: usize, label: &str) {
    let _ = app.emit(
        "blueprint://progress",
        json!({"step": step, "total": total, "label": label}),
    );
}

/// 长篇默认按“史诗”规模建地基；作者明确短篇/中篇或篇幅较小时降为标准规模。
fn infer_scale(premise: &Value, requested: Option<&str>) -> &'static str {
    match requested {
        Some("standard") => return "standard",
        Some("epic") => return "epic",
        Some("quick") => return "quick",
        _ => {}
    }
    let length = premise
        .get("targetLength")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if ["短篇", "中篇", "短剧", "单章"]
        .iter()
        .any(|word| length.contains(word))
    {
        return "standard";
    }
    let digits: String = length
        .split('万')
        .next()
        .unwrap_or_default()
        .chars()
        .filter(|ch| ch.is_ascii_digit())
        .collect();
    match digits.parse::<u32>() {
        Ok(wan) if length.contains('万') && wan < 30 => "standard",
        _ => "epic",
    }
}

fn element_key(value: &Value) -> String {
    format!(
        "{}:{}",
        value
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
    )
}

/// 把一层的结果并入故事圣经：同类同名元素替换，事件按标题替换，其余追加。
fn merge_layer(foundation: &mut Value, layer: &Value) {
    for key in [
        "title",
        "coreDramaticQuestion",
        "narrativePromise",
        "openingState",
        "endingState",
        "themeQuestion",
    ] {
        if let Some(text) = layer.get(key).and_then(Value::as_str) {
            if !text.trim().is_empty() {
                foundation[key] = Value::String(text.to_string());
            }
        }
    }
    let merge_list =
        |target: &mut Value, incoming: Option<&Value>, key_of: &dyn Fn(&Value) -> String| {
            let Some(items) = incoming.and_then(Value::as_array) else {
                return;
            };
            if !target.is_array() {
                *target = json!([]);
            }
            let list = target.as_array_mut().expect("array");
            for item in items {
                let key = key_of(item);
                match list.iter().position(|existing| key_of(existing) == key) {
                    Some(index) => list[index] = item.clone(),
                    None => list.push(item.clone()),
                }
            }
        };
    let title_key = |value: &Value| {
        value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    merge_list(
        &mut foundation["elements"],
        layer.get("elements"),
        &element_key,
    );
    merge_list(
        &mut foundation["timeline"],
        layer.get("timeline"),
        &title_key,
    );
    merge_list(&mut foundation["threads"], layer.get("threads"), &title_key);
    for list in ["timeline", "threads"] {
        if let Some(items) = foundation[list].as_array_mut() {
            for (index, item) in items.iter_mut().enumerate() {
                item["orderNo"] = json!(index + 1);
            }
        }
    }
}

/// 分层建立故事圣经：世界 → 势力 → 人物 → 总体结构 → 漏洞审查。
/// 每层都读取前层与作者已确认资料，避免一次请求把世界观压扁成几条摘要。
async fn build_foundation(
    app: &tauri::AppHandle,
    client: &LlmClient,
    premise: &Value,
    medium: &str,
    canon: &str,
    scale: &str,
    total_steps: usize,
) -> Result<Value, String> {
    let mut foundation = json!({
        "title": "",
        "coreDramaticQuestion": "",
        "narrativePromise": "",
        "openingState": "",
        "endingState": "",
        "themeQuestion": "",
        "elements": [],
        "timeline": [],
        "threads": []
    });
    for (index, (layer, label)) in WORLD_LAYERS.iter().enumerate() {
        emit_progress(app, index + 1, total_steps, &format!("正在构建{label}"));
        let raw = complete_blueprint_request(
            client,
            &[
                ChatTurn::system(world_layer_system_prompt(layer, scale)),
                ChatTurn::user(world_layer_user_prompt(
                    layer,
                    premise,
                    medium,
                    canon,
                    &foundation,
                )),
            ],
            client.max_tokens.max(9000),
        )
        .await
        .map_err(|error| format!("{label}生成失败：{error}"))?;
        let parsed: Value = match parse_json_loose(&raw) {
            Ok(value) => value,
            // 审查层失败不影响已建立的圣经；其余层必须成功。
            Err(_) if *layer == "critique" => continue,
            Err(error) => return Err(format!("{label}解析失败：{error}")),
        };
        if *layer == "critique" {
            if let Some(patch) = parsed.get("patch") {
                merge_layer(&mut foundation, patch);
            }
        } else {
            merge_layer(&mut foundation, &parsed);
        }
    }
    Ok(foundation)
}

/// 作者已确认的人物、实体、知识条目与守则：分层生成时必须保留并扩展。
fn canon_context(conn: &rusqlite::Connection, project_id: &str) -> Result<String, String> {
    let mut parts = Vec::new();
    let entities = crate::entities::list(conn, project_id, None).map_err(|e| e.to_string())?;
    for entity in entities.iter().take(80) {
        parts.push(format!(
            "- [{}] {}：{}",
            entity.entity_type, entity.canonical_name, entity.description
        ));
    }
    let knowledge =
        crate::knowledge::prompt_context(conn, project_id, 60).map_err(|e| e.to_string())?;
    if !knowledge.trim().is_empty() {
        parts.push(knowledge);
    }
    let guide = crate::craft::guide(conn, project_id).map_err(|e| e.to_string())?;
    if !guide.content.trim().is_empty() {
        parts.push(format!("作者守则：\n{}", guide.content.trim()));
    }
    Ok(if parts.is_empty() {
        "（暂无）".into()
    } else {
        parts.join("\n")
    })
}

#[tauri::command]
#[specta::specta]
pub fn blueprint_current(state: State<'_, AppState>) -> IpcResult<Option<NarrativeBlueprintView>> {
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
    app: tauri::AppHandle,
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
    let (project_id, premise_id, premise, canon) = {
        let guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
        let loaded = guard.as_ref().ok_or("尚未打开项目")?;
        let premise = seed::active(&loaded.conn, &loaded.summary.id)
            .map_err(|error| error.to_string())?
            .ok_or("请先确认种子卡，再建立叙事蓝本")?;
        let value = serde_json::to_value(&premise).map_err(|error| error.to_string())?;
        let canon = canon_context(&loaded.conn, &loaded.summary.id)?;
        (loaded.summary.id.clone(), premise.id, value, canon)
    };
    let scale = infer_scale(&premise, input.depth.as_deref());
    let total_steps = WORLD_LAYERS.len() + 2;

    let mut foundation = if scale == "quick" {
        let raw = complete_blueprint_request(
            &client,
            &[
                ChatTurn::system(narrative_foundation_system_prompt()),
                ChatTurn::user(narrative_foundation_user_prompt(
                    &premise,
                    &input.medium,
                    &input.structure_strategy,
                    input.target_units,
                )),
            ],
            client.max_tokens.max(9000),
        )
        .await?;
        parse_json_loose(&raw).map_err(|error| format!("叙事地基解析失败：{error}"))?
    } else {
        build_foundation(
            &app,
            &client,
            &premise,
            &input.medium,
            &canon,
            scale,
            total_steps,
        )
        .await?
    };

    emit_progress(&app, total_steps - 1, total_steps, "正在编织因果链");
    let chain_raw = complete_blueprint_request(
        &client,
        &[
            ChatTurn::system(narrative_chain_system_prompt()),
            ChatTurn::user(narrative_chain_user_prompt(
                &foundation,
                &input.medium,
                &input.structure_strategy,
            )),
        ],
        client.max_tokens.max(9000),
    )
    .await?;
    let mut chain: Value =
        parse_json_loose(&chain_raw).map_err(|error| format!("逻辑链解析失败：{error}"))?;

    emit_progress(&app, total_steps, total_steps, "正在诊断与修复");
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
        let repair_raw = match complete_blueprint_request(
            &client,
            &[
                ChatTurn::system(narrative_repair_system_prompt()),
                ChatTurn::user(narrative_repair_user_prompt(
                    &foundation,
                    &chain,
                    &diagnostics,
                )),
            ],
            client.max_tokens.max(9000),
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
pub fn blueprint_nodes(state: State<'_, AppState>) -> IpcResult<Vec<NarrativeLogicNode>> {
    state
        .with_project(|loaded| {
            Ok(blueprint::current(&loaded.conn, &loaded.summary.id)?
                .map(|view| view.nodes)
                .unwrap_or_default())
        })
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_epic_scale_for_long_works() {
        assert_eq!(
            infer_scale(&json!({"targetLength":"300万字"}), None),
            "epic"
        );
        assert_eq!(
            infer_scale(&json!({"targetLength":"12万字"}), None),
            "standard"
        );
        assert_eq!(
            infer_scale(&json!({"targetLength":"中篇"}), None),
            "standard"
        );
        assert_eq!(infer_scale(&json!({}), None), "epic");
        assert_eq!(
            infer_scale(&json!({"targetLength":"300万字"}), Some("quick")),
            "quick"
        );
    }

    #[test]
    fn merges_layers_by_kind_name_and_title() {
        let mut foundation = json!({"elements":[],"timeline":[],"threads":[]});
        merge_layer(
            &mut foundation,
            &json!({"elements":[{"kind":"faction","name":"守夜庭","summary":"旧"}],
                    "timeline":[{"title":"大寂灭","cause":"旧"}]}),
        );
        merge_layer(
            &mut foundation,
            &json!({"title":"雨夜",
                    "elements":[{"kind":"faction","name":"守夜庭","summary":"新"},
                                {"kind":"character","name":"守夜庭","summary":"同名不同类"}],
                    "timeline":[{"title":"大寂灭","cause":"新"},{"title":"开城","cause":"x"}]}),
        );
        let elements = foundation["elements"].as_array().unwrap();
        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0]["summary"], "新");
        let timeline = foundation["timeline"].as_array().unwrap();
        assert_eq!(timeline.len(), 2);
        assert_eq!(timeline[1]["orderNo"], 2);
        assert_eq!(foundation["title"], "雨夜");
    }
}
