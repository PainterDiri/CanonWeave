use serde_json::Value;
use tauri::State;

use crate::{
    domain::{
        NarrativeBlueprintView, NarrativeLogicNode, OutlineGenerateInput, OutlineNode,
        OutlineStatus, OutlineUpdateInput, VolumePlanInput,
    },
    error::IpcResult,
    knowledge,
    blueprint,
    llm::{
        outline_adaptive_system_prompt, outline_adaptive_user_prompt, outline_chapter_system_prompt,
        outline_chapter_user_prompt, outline_volume_system_prompt, outline_volume_user_prompt,
        parse_json_loose, ChatTurn, LlmClient,
    },
    outline::{self, OutlineChapterDraft, OutlinePlanDraft, OutlineVolumeDraft},
    seed,
    state::AppState,
};

const MAX_VOLUMES: i32 = 40;
const MAX_CHAPTERS: i32 = 1200;
const CHAPTER_BATCH: i32 = 16;

#[derive(Debug, Clone)]
struct VolumeRequest {
    index: usize,
    title: Option<String>,
    chapter_count: i32,
    start_chapter: i32,
}

#[derive(Debug)]
struct NormalizedPlan {
    mode: String,
    total_chapters: i32,
    volumes: Vec<VolumeRequest>,
}

fn normalize_plan(input: &OutlineGenerateInput) -> Result<NormalizedPlan, String> {
    match input.mode.as_str() {
        "adaptive" => Ok(NormalizedPlan {
            mode: "adaptive".into(),
            total_chapters: 0,
            volumes: vec![],
        }),
        "chapters" => {
            let total = input.total_chapters.unwrap_or(0);
            if !(1..=MAX_CHAPTERS).contains(&total) {
                return Err(format!("章节数必须在 1 至 {MAX_CHAPTERS} 之间"));
            }
            Ok(NormalizedPlan {
                mode: "chapters".into(),
                total_chapters: total,
                volumes: vec![],
            })
        }
        "volumes" => {
            if input.volumes.is_empty() {
                return Err("分卷模式至少需要一卷".into());
            }
            if input.volumes.len() > MAX_VOLUMES as usize {
                return Err(format!("最多支持 {MAX_VOLUMES} 卷"));
            }
            let mut start = 1;
            let mut volumes = Vec::new();
            for (index, volume) in input.volumes.iter().enumerate() {
                if volume.chapter_count < 1 {
                    return Err(format!("第 {} 卷的章节数必须大于 0", index + 1));
                }
                volumes.push(VolumeRequest {
                    index,
                    title: normalize_title(volume),
                    chapter_count: volume.chapter_count,
                    start_chapter: start,
                });
                start += volume.chapter_count;
            }
            let total = start - 1;
            if total > MAX_CHAPTERS {
                return Err(format!("全书章节总数不能超过 {MAX_CHAPTERS}"));
            }
            Ok(NormalizedPlan {
                mode: "volumes".into(),
                total_chapters: total,
                volumes,
            })
        }
        _ => Err("未知的骨架模式，请选择 chapters 或 volumes".into()),
    }
}

fn normalize_title(volume: &VolumePlanInput) -> Option<String> {
    volume
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_string)
}

fn value_string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn value_string_list(value: &Value, key: &str) -> Vec<String> {
    match value.get(key) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect(),
        Some(Value::String(raw)) => raw
            .split([',', '，', '、'])
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn resolve_logic_marker<'a>(
    marker: &str,
    nodes: &'a [NarrativeLogicNode],
) -> Option<&'a NarrativeLogicNode> {
    let marker = marker
        .trim()
        .trim_start_matches('#')
        .trim_start_matches('第')
        .trim_end_matches('章')
        .trim();
    let numeric = marker.parse::<i32>().ok();
    nodes.iter().find(|node| {
        node.id == marker
            || node.title == marker
            || numeric == Some(node.order_no)
            || marker.starts_with(&format!("{} ", node.order_no))
            || marker.starts_with(&format!("{}.", node.order_no))
            || marker.starts_with(&format!("{}、", node.order_no))
            || (!node.title.is_empty() && marker.contains(&node.title))
    })
}

fn bind_logic_nodes(
    plan: &mut OutlinePlanDraft,
    view: &NarrativeBlueprintView,
) -> Result<(), String> {
    for chapter in &mut plan.chapters {
        let mut resolved = Vec::new();
        for marker in &chapter.logic_node_ids {
            let Some(node) = resolve_logic_marker(marker, &view.nodes) else {
                return Err(format!(
                    "第 {} 章引用了不存在的逻辑节点“{}”。请重新生成章节编排。",
                    chapter.number, marker
                ));
            };
            if !resolved.contains(&node.id) {
                resolved.push(node.id.clone());
            }
        }
        if resolved.is_empty() {
            return Err(format!(
                "第 {} 章没有承接任何逻辑节点。章节必须从已确认因果链派生，请重新生成。",
                chapter.number
            ));
        }
        let presentations: Vec<_> = view
            .presentations
            .iter()
            .filter(|item| resolved.contains(&item.logic_node_id))
            .collect();
        if chapter.presentation_mode.trim().is_empty() {
            chapter.presentation_mode = if view.blueprint.medium == "game" {
                if presentations.len() > 1 {
                    "mixed".into()
                } else {
                    presentations
                        .first()
                        .map(|item| item.mode.clone())
                        .unwrap_or_else(|| "mixed".into())
                }
            } else {
                "prose".into()
            };
        }
        if chapter.narrative_function.trim().is_empty() {
            chapter.narrative_function = resolved
                .iter()
                .filter_map(|id| view.nodes.iter().find(|node| &node.id == id))
                .map(|node| node.kind.clone())
                .collect::<Vec<_>>()
                .join(" / ");
        }
        chapter.logic_node_ids = resolved;
    }
    Ok(())
}

async fn generate_volume_headers(
    client: &LlmClient,
    premise: &Value,
    plan: &NormalizedPlan,
) -> Result<Vec<OutlineVolumeDraft>, String> {
    let requests = plan
        .volumes
        .iter()
        .map(|volume| {
            format!(
                "{}. 卷名：{}；章节数：{}；本卷从全书第 {} 章开始",
                volume.index + 1,
                volume.title.as_deref().unwrap_or("由你命名"),
                volume.chapter_count,
                volume.start_chapter
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let raw = client
        .complete(
            &[
                ChatTurn::system(outline_volume_system_prompt()),
                ChatTurn::user(outline_volume_user_prompt(premise, &requests)),
            ],
            Some(client.max_tokens.max(6000)),
        )
        .await
        .map_err(|error| error.to_string())?;
    let mut rows: Vec<Value> =
        parse_json_loose(&raw).map_err(|error| format!("卷级骨架解析失败：{error}"))?;
    if rows.len() != plan.volumes.len() {
        return Err(format!(
            "模型返回了 {} 卷，但作者定义了 {} 卷，请重试",
            rows.len(),
            plan.volumes.len()
        ));
    }
    rows.sort_by_key(|row| row.get("index").and_then(Value::as_i64).unwrap_or(i64::MAX));
    Ok(plan
        .volumes
        .iter()
        .zip(rows)
        .map(|(request, row)| OutlineVolumeDraft {
            title: request
                .title
                .clone()
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| {
                    let title = value_string(&row, "title");
                    if title.is_empty() {
                        format!("第 {} 卷", request.index + 1)
                    } else {
                        title
                    }
                }),
            goal: value_string(&row, "goal"),
            conflict: value_string(&row, "conflict"),
            turn: value_string(&row, "turn"),
            payoff: value_string(&row, "payoff"),
        })
        .collect())
}

async fn generate_chapter_batch(
    client: &LlmClient,
    premise: &Value,
    structure_context: &str,
    volume_context: &str,
    start: i32,
    end: i32,
    previous_context: &str,
) -> Result<Vec<OutlineChapterDraft>, String> {
    let raw = client
        .complete(
            &[
                ChatTurn::system(outline_chapter_system_prompt()),
                ChatTurn::user(outline_chapter_user_prompt(
                    premise,
                    structure_context,
                    volume_context,
                    start,
                    end,
                    previous_context,
                )),
            ],
            Some(client.max_tokens.max(4096)),
        )
        .await
        .map_err(|error| error.to_string())?;
    let rows: Vec<Value> =
        parse_json_loose(&raw).map_err(|error| format!("章节骨架解析失败：{error}"))?;
    let expected = (end - start + 1) as usize;
    if rows.len() < expected {
        return Err(format!(
            "第 {start} 至 {end} 章只返回了 {} 章，请重试",
            rows.len()
        ));
    }
    Ok(rows
        .into_iter()
        .take(expected)
        .enumerate()
        .map(|(offset, row)| {
            let number = start + offset as i32;
            let title = value_string(&row, "title");
            OutlineChapterDraft {
                number,
                volume_index: None,
                title: if title.is_empty() {
                    format!("第 {number} 章")
                } else {
                    title
                },
                goal: value_string(&row, "goal"),
                conflict: value_string(&row, "conflict"),
                turn: value_string(&row, "turn"),
                payoff: value_string(&row, "payoff"),
                logic_node_ids: value_string_list(&row, "logicNodeIds"),
                presentation_mode: value_string(&row, "presentationMode"),
                narrative_function: value_string(&row, "narrativeFunction"),
            }
        })
        .collect())
}

async fn generate_plan(
    client: &LlmClient,
    premise: &Value,
    plan: NormalizedPlan,
) -> Result<OutlinePlanDraft, String> {
    let structure_context = if plan.mode == "chapters" {
        format!("单卷直排，共 {} 章", plan.total_chapters)
    } else {
        plan.volumes
            .iter()
            .map(|volume| {
                format!(
                    "第 {} 卷：第 {} 至 {} 章，共 {} 章",
                    volume.index + 1,
                    volume.start_chapter,
                    volume.start_chapter + volume.chapter_count - 1,
                    volume.chapter_count
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    if plan.mode == "chapters" {
        let mut chapters = Vec::new();
        let mut previous = "这是全书第一章。".to_string();
        let mut start = 1;
        while start <= plan.total_chapters {
            let end = (start + CHAPTER_BATCH - 1).min(plan.total_chapters);
            let batch = generate_chapter_batch(
                client,
                premise,
                &structure_context,
                "不分卷。",
                start,
                end,
                &previous,
            )
            .await?;
            previous = batch
                .iter()
                .rev()
                .take(3)
                .map(|chapter| {
                    format!(
                        "第 {} 章《{}》：{}；转折：{}",
                        chapter.number, chapter.title, chapter.goal, chapter.turn
                    )
                })
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n");
            chapters.extend(batch);
            start = end + 1;
        }
        return Ok(OutlinePlanDraft {
            mode: plan.mode,
            total_chapters: plan.total_chapters,
            volumes: vec![],
            chapters,
        });
    }

    let volume_headers = generate_volume_headers(client, premise, &plan).await?;
    let mut chapters = Vec::new();
    for (index, (request, volume)) in plan.volumes.iter().zip(volume_headers.iter()).enumerate() {
        let volume_context = format!(
            "第 {} 卷《{}》\n目标：{}\n冲突：{}\n转折：{}\n卷末钩子：{}",
            index + 1,
            volume.title,
            volume.goal,
            volume.conflict,
            volume.turn,
            volume.payoff
        );
        let mut previous = format!(
            "本卷从全书第 {} 章开始。上一卷收束：{}",
            request.start_chapter,
            if index == 0 {
                "这是全书第一卷。".to_string()
            } else {
                volume_headers[index - 1].payoff.clone()
            }
        );
        let end_chapter = request.start_chapter + request.chapter_count - 1;
        let mut start = request.start_chapter;
        while start <= end_chapter {
            let end = (start + CHAPTER_BATCH - 1).min(end_chapter);
            let mut batch = generate_chapter_batch(
                client,
                premise,
                &structure_context,
                &volume_context,
                start,
                end,
                &previous,
            )
            .await?;
            for chapter in &mut batch {
                chapter.volume_index = Some(index);
            }
            previous = batch
                .iter()
                .rev()
                .take(3)
                .map(|chapter| {
                    format!(
                        "第 {} 章《{}》：{}；转折：{}",
                        chapter.number, chapter.title, chapter.goal, chapter.turn
                    )
                })
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n");
            chapters.extend(batch);
            start = end + 1;
        }
    }
    Ok(OutlinePlanDraft {
        mode: plan.mode,
        total_chapters: plan.total_chapters,
        volumes: volume_headers,
        chapters,
    })
}

#[tauri::command]
#[specta::specta]
pub fn outline_list(state: State<'_, AppState>) -> IpcResult<Vec<OutlineNode>> {
    state
        .with_project(|loaded| outline::list(&loaded.conn, &loaded.summary.id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn outline_generate(
    state: State<'_, AppState>,
    input: OutlineGenerateInput,
) -> IpcResult<Vec<OutlineNode>> {
    let normalized = normalize_plan(&input).map_err(|e| e.to_string())?;
    let client = super::seed::require_client(&state).map_err(|e| e.to_string())?;
    let (project_id, premise, blueprint_context) = {
        let guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
        let loaded = guard.as_ref().ok_or("尚未打开项目")?;
        let premise = seed::active(&loaded.conn, &loaded.summary.id)
            .map_err(|e| e.to_string())?
            .ok_or("请先确认种子卡，再规划卷章")?;
        let mut premise = serde_json::to_value(premise).map_err(|e| e.to_string())?;
        let knowledge_context = knowledge::prompt_context(&loaded.conn, &loaded.summary.id, 80)
            .map_err(|e| e.to_string())?;
        if !knowledge_context.is_empty() {
            premise["knowledgeBase"] = Value::String(knowledge_context);
        }
        let blueprint_context = if blueprint::current(&loaded.conn, &loaded.summary.id)
            .map_err(|e| e.to_string())?
            .is_some()
        {
            blueprint::logic_context(&loaded.conn, &loaded.summary.id, 120)
                .map_err(|e| e.to_string())?
        } else {
            String::new()
        };
        (loaded.summary.id.clone(), premise, blueprint_context)
    };
    let normalized = if normalized.mode == "adaptive" {
        if blueprint_context.is_empty() {
            return Err("自适应编排需要先确认叙事蓝本".into());
        }
        let raw = client
            .complete(
                &[
                    ChatTurn::system(outline_adaptive_system_prompt()),
                    ChatTurn::user(outline_adaptive_user_prompt(
                        &premise,
                        &blueprint_context,
                    )),
                ],
                Some(client.max_tokens.max(5000)),
            )
            .await
            .map_err(|error| error.to_string())?;
        let value: Value = parse_json_loose(&raw)
            .map_err(|error| format!("自适应结构解析失败：{error}"))?;
        let total = value
            .get("totalChapters")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32;
        if !(24..=MAX_CHAPTERS).contains(&total) {
            return Err("自适应规划没有给出有效章节总数，请重试".into());
        }
        let volumes = value
            .get("volumes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|item| {
                Some(VolumePlanInput {
                    title: item
                        .get("title")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    chapter_count: item.get("chapterCount").and_then(Value::as_i64)? as i32,
                })
            })
            .collect::<Vec<_>>();
        if volumes.is_empty() {
            NormalizedPlan {
                mode: "chapters".into(),
                total_chapters: total,
                volumes: vec![],
            }
        } else {
            normalize_plan(&OutlineGenerateInput {
                mode: "volumes".into(),
                total_chapters: None,
                volumes,
            })
            .map_err(|error| error.to_string())?
        }
    } else {
        if blueprint_context.is_empty() {
            return Err("策划章节或任务前请先确认叙事蓝本".into());
        }
        normalized
    };
    let mut premise = premise;
    if !blueprint_context.is_empty() {
        premise["narrativeBlueprint"] = Value::String(blueprint_context);
    }
    let mut plan = generate_plan(&client, &premise, normalized).await?;
    let mut guard = state.project.lock().map_err(|_| "项目状态锁已损坏")?;
    let loaded = guard.as_mut().ok_or("尚未打开项目")?;
    let view = blueprint::current(&loaded.conn, &project_id)
        .map_err(|error| error.to_string())?
        .ok_or("策划章节或任务前请先确认叙事蓝本")?;
    bind_logic_nodes(&mut plan, &view)?;
    let branch_id =
        crate::branches::current(&loaded.conn, &project_id).map_err(|error| error.to_string())?;
    outline::insert_plan(&mut loaded.conn, &project_id, &plan, branch_id.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn outline_update(
    state: State<'_, AppState>,
    input: OutlineUpdateInput,
) -> IpcResult<OutlineNode> {
    state
        .with_project(|loaded| outline::update(&loaded.conn, &loaded.summary.id, &input))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn outline_confirm(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> IpcResult<Vec<OutlineNode>> {
    let mut guard = state
        .project
        .lock()
        .map_err(|_| "项目状态锁已损坏".to_string())?;
    let loaded = guard.as_mut().ok_or("尚未打开项目")?;
    outline::confirm(&mut loaded.conn, &loaded.summary.id, &ids).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn outline_drop(state: State<'_, AppState>, id: String) -> IpcResult<()> {
    state
        .with_project(|loaded| outline::drop_node(&loaded.conn, &loaded.summary.id, &id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn outline_status(state: State<'_, AppState>) -> IpcResult<OutlineStatus> {
    state
        .with_project(|loaded| {
            let has = outline::has_skeleton(&loaded.conn, &loaded.summary.id)?;
            let confirmed = outline::confirmed_count(&loaded.conn, &loaded.summary.id)?;
            Ok(OutlineStatus {
                has_skeleton: has,
                confirmed,
            })
        })
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_flat_and_volume_structures() {
        let flat = normalize_plan(&OutlineGenerateInput {
            mode: "chapters".into(),
            total_chapters: Some(37),
            volumes: vec![],
        })
        .unwrap();
        assert_eq!(flat.total_chapters, 37);
        assert!(flat.volumes.is_empty());

        let volumes = normalize_plan(&OutlineGenerateInput {
            mode: "volumes".into(),
            total_chapters: None,
            volumes: vec![
                VolumePlanInput {
                    title: Some("第一卷".into()),
                    chapter_count: 80,
                },
                VolumePlanInput {
                    title: None,
                    chapter_count: 120,
                },
            ],
        })
        .unwrap();
        assert_eq!(volumes.total_chapters, 200);
        assert_eq!(volumes.volumes[1].start_chapter, 81);
        assert_eq!(volumes.volumes[0].title.as_deref(), Some("第一卷"));
    }

    fn logic_node(order: i32, title: &str) -> NarrativeLogicNode {
        NarrativeLogicNode {
            id: format!("node-{order}"),
            thread_id: None,
            thread_title: "主线".into(),
            order_no: order,
            kind: "reveal".into(),
            title: title.into(),
            cause: "前因".into(),
            choice: "选择".into(),
            cost: "代价".into(),
            outcome: "结果".into(),
            state_change: "状态".into(),
            knowledge_change: "认知".into(),
            participants: vec![],
            location: String::new(),
            story_time: String::new(),
            status: "confirmed".into(),
        }
    }

    #[test]
    fn resolves_logic_nodes_by_number_title_and_id() {
        let nodes = vec![logic_node(1, "车票出现"), logic_node(2, "站长拦截")];
        assert_eq!(resolve_logic_marker("1", &nodes).unwrap().title, "车票出现");
        assert_eq!(resolve_logic_marker("#2", &nodes).unwrap().title, "站长拦截");
        assert_eq!(resolve_logic_marker("站长拦截", &nodes).unwrap().order_no, 2);
        assert_eq!(resolve_logic_marker("node-1", &nodes).unwrap().order_no, 1);
        assert!(resolve_logic_marker("不存在", &nodes).is_none());
    }

    #[test]
    fn rejects_invalid_structure_sizes() {
        assert!(normalize_plan(&OutlineGenerateInput {
            mode: "chapters".into(),
            total_chapters: Some(0),
            volumes: vec![],
        })
        .is_err());
        assert!(normalize_plan(&OutlineGenerateInput {
            mode: "volumes".into(),
            total_chapters: None,
            volumes: vec![],
        })
        .is_err());
    }
}
