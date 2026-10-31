use std::collections::{HashMap, HashSet, VecDeque};

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    domain::{
        NarrativeBlueprint, NarrativeBlueprintView, NarrativeDiagnostic, NarrativeElement,
        NarrativeGenerateInput, NarrativeLogicEdge, NarrativeLogicNode, NarrativePresentation,
        NarrativeThread, NarrativeTimelineEvent,
    },
    error::{AppError, AppResult},
    now,
};

const BLUEPRINT_SELECT: &str = "SELECT id, premise_id, title, medium, structure_strategy,
    target_units, core_dramatic_question, narrative_promise, opening_state, ending_state,
    theme_question, status, completeness, version, created_at, updated_at
    FROM narrative_blueprint";

fn blueprint_from_row(row: &Row<'_>) -> rusqlite::Result<NarrativeBlueprint> {
    Ok(NarrativeBlueprint {
        id: row.get(0)?,
        premise_id: row.get(1)?,
        title: row.get(2)?,
        medium: row.get(3)?,
        structure_strategy: row.get(4)?,
        target_units: row.get(5)?,
        core_dramatic_question: row.get(6)?,
        narrative_promise: row.get(7)?,
        opening_state: row.get(8)?,
        ending_state: row.get(9)?,
        theme_question: row.get(10)?,
        status: row.get(11)?,
        completeness: row.get(12)?,
        version: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
    })
}

fn element_from_row(row: &Row<'_>) -> rusqlite::Result<NarrativeElement> {
    let constraints: String = row.get(9)?;
    let leverage: String = row.get(10)?;
    let relations: String = row.get(11)?;
    let metadata: String = row.get(12)?;
    Ok(NarrativeElement {
        id: row.get(0)?,
        kind: row.get(1)?,
        name: row.get(2)?,
        role: row.get(3)?,
        summary: row.get(4)?,
        external_goal: row.get(5)?,
        internal_need: row.get(6)?,
        fear: row.get(7)?,
        flaw: row.get(8)?,
        leverage,
        constraints: serde_json::from_str(&constraints).unwrap_or_default(),
        secret: row.get(13)?,
        relations: serde_json::from_str(&relations).unwrap_or_else(|_| json!([])),
        metadata: serde_json::from_str(&metadata).unwrap_or_else(|_| json!({})),
        status: row.get(14)?,
        sort_order: row.get(15)?,
    })
}

fn timeline_from_row(row: &Row<'_>) -> rusqlite::Result<NarrativeTimelineEvent> {
    let participants: String = row.get(5)?;
    Ok(NarrativeTimelineEvent {
        id: row.get(0)?,
        order_no: row.get(1)?,
        title: row.get(2)?,
        story_time: row.get(3)?,
        cause: row.get(4)?,
        participants: serde_json::from_str(&participants).unwrap_or_default(),
        outcome: row.get(6)?,
        consequence: row.get(7)?,
        reveal_at: row.get(8)?,
        status: row.get(9)?,
    })
}

fn thread_from_row(row: &Row<'_>) -> rusqlite::Result<NarrativeThread> {
    Ok(NarrativeThread {
        id: row.get(0)?,
        order_no: row.get(1)?,
        kind: row.get(2)?,
        title: row.get(3)?,
        dramatic_question: row.get(4)?,
        opening_state: row.get(5)?,
        target_state: row.get(6)?,
        stakes: row.get(7)?,
        status: row.get(8)?,
    })
}

fn logic_node_from_row(row: &Row<'_>) -> rusqlite::Result<NarrativeLogicNode> {
    let participants: String = row.get(11)?;
    Ok(NarrativeLogicNode {
        id: row.get(0)?,
        thread_id: row.get(1)?,
        thread_title: row.get(2)?,
        order_no: row.get(3)?,
        kind: row.get(4)?,
        title: row.get(5)?,
        cause: row.get(6)?,
        choice: row.get(7)?,
        cost: row.get(8)?,
        outcome: row.get(9)?,
        state_change: row.get(10)?,
        knowledge_change: row.get(12)?,
        participants: serde_json::from_str(&participants).unwrap_or_default(),
        location: row.get(13)?,
        story_time: row.get(14)?,
        status: row.get(15)?,
    })
}

fn edge_from_row(row: &Row<'_>) -> rusqlite::Result<NarrativeLogicEdge> {
    Ok(NarrativeLogicEdge {
        id: row.get(0)?,
        from_node_id: row.get(1)?,
        from_title: row.get(2)?,
        to_node_id: row.get(3)?,
        to_title: row.get(4)?,
        relation: row.get(5)?,
        condition: row.get(6)?,
        status: row.get(7)?,
    })
}

fn presentation_from_row(row: &Row<'_>) -> rusqlite::Result<NarrativePresentation> {
    Ok(NarrativePresentation {
        id: row.get(0)?,
        logic_node_id: row.get(1)?,
        logic_node_title: row.get(2)?,
        medium: row.get(3)?,
        mode: row.get(4)?,
        pov: row.get(5)?,
        dialogue_ratio: row.get(6)?,
        narration_ratio: row.get(7)?,
        player_agency: row.get(8)?,
        visual_requirements: row.get(9)?,
        audio_requirements: row.get(10)?,
        interaction: row.get(11)?,
        deliverable: row.get(12)?,
        status: row.get(13)?,
    })
}

pub fn current(conn: &Connection, project_id: &str) -> AppResult<Option<NarrativeBlueprintView>> {
    let blueprint = conn
        .query_row(
            &format!(
                "{BLUEPRINT_SELECT} WHERE project_id=?1 AND status='confirmed'
                 ORDER BY updated_at DESC LIMIT 1"
            ),
            [project_id],
            blueprint_from_row,
        )
        .optional()?;
    match blueprint {
        Some(blueprint) => get_view(conn, project_id, &blueprint.id).map(Some),
        None => Ok(None),
    }
}

pub fn candidate(conn: &Connection, project_id: &str) -> AppResult<Option<NarrativeBlueprintView>> {
    let id = conn
        .query_row(
            "SELECT id FROM narrative_blueprint WHERE project_id=?1 AND status='candidate'
             ORDER BY updated_at DESC LIMIT 1",
            [project_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    match id {
        Some(id) => get_view(conn, project_id, &id).map(Some),
        None => Ok(None),
    }
}

pub fn get_view(
    conn: &Connection,
    project_id: &str,
    blueprint_id: &str,
) -> AppResult<NarrativeBlueprintView> {
    let blueprint = conn
        .query_row(
            &format!("{BLUEPRINT_SELECT} WHERE project_id=?1 AND id=?2"),
            params![project_id, blueprint_id],
            blueprint_from_row,
        )
        .optional()?
        .ok_or_else(|| AppError::Message("找不到叙事蓝本".into()))?;

    let mut elements_stmt = conn.prepare(
        "SELECT id, kind, name, role, summary, external_goal, internal_need, fear, flaw,
                constraints_json, leverage, relations_json, metadata_json, secret, status, sort_order
         FROM narrative_element WHERE blueprint_id=?1 ORDER BY sort_order, created_at",
    )?;
    let elements = elements_stmt
        .query_map([blueprint_id], element_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    drop(elements_stmt);

    let mut timeline_stmt = conn.prepare(
        "SELECT id, order_no, title, story_time, cause, participants_json, outcome,
                consequence, reveal_at, status
         FROM narrative_timeline_event WHERE blueprint_id=?1 ORDER BY order_no, created_at",
    )?;
    let timeline = timeline_stmt
        .query_map([blueprint_id], timeline_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    drop(timeline_stmt);

    let mut thread_stmt = conn.prepare(
        "SELECT id, order_no, kind, title, dramatic_question, opening_state, target_state,
                stakes, status
         FROM narrative_thread WHERE blueprint_id=?1 ORDER BY order_no, created_at",
    )?;
    let threads = thread_stmt
        .query_map([blueprint_id], thread_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    drop(thread_stmt);

    let mut node_stmt = conn.prepare(
        "SELECT n.id, n.thread_id, COALESCE(t.title,''), n.order_no, n.kind, n.title,
                n.cause, n.choice, n.cost, n.outcome, n.state_change,
                n.participants_json, n.knowledge_change, n.location, n.story_time, n.status
         FROM narrative_logic_node n
         LEFT JOIN narrative_thread t ON t.id=n.thread_id
         WHERE n.blueprint_id=?1 ORDER BY n.order_no, n.created_at",
    )?;
    let nodes = node_stmt
        .query_map([blueprint_id], logic_node_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    drop(node_stmt);

    let mut edge_stmt = conn.prepare(
        "SELECT e.id, e.from_node_id, a.title, e.to_node_id, b.title,
                e.relation, e.condition, e.status
         FROM narrative_logic_edge e
         JOIN narrative_logic_node a ON a.id=e.from_node_id
         JOIN narrative_logic_node b ON b.id=e.to_node_id
         WHERE e.blueprint_id=?1 ORDER BY a.order_no, b.order_no",
    )?;
    let edges = edge_stmt
        .query_map([blueprint_id], edge_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    drop(edge_stmt);

    let mut presentation_stmt = conn.prepare(
        "SELECT p.id, p.logic_node_id, n.title, p.medium, p.mode, p.pov,
                p.dialogue_ratio, p.narration_ratio, p.player_agency,
                p.visual_requirements, p.audio_requirements, p.interaction,
                p.deliverable, p.status
         FROM narrative_presentation p
         JOIN narrative_logic_node n ON n.id=p.logic_node_id
         WHERE p.blueprint_id=?1 ORDER BY n.order_no, p.created_at",
    )?;
    let presentations = presentation_stmt
        .query_map([blueprint_id], presentation_from_row)?
        .collect::<Result<Vec<_>, _>>()?;

    let diagnostics = diagnose_values(
        &blueprint,
        &elements,
        &timeline,
        &threads,
        &nodes,
        &edges,
        &presentations,
    );
    Ok(NarrativeBlueprintView {
        blueprint,
        elements,
        timeline,
        threads,
        nodes,
        edges,
        presentations,
        diagnostics,
    })
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct FoundationDraft {
    #[serde(default)]
    title: String,
    #[serde(default)]
    core_dramatic_question: String,
    #[serde(default)]
    narrative_promise: String,
    #[serde(default)]
    opening_state: String,
    #[serde(default)]
    ending_state: String,
    #[serde(default)]
    theme_question: String,
    #[serde(default)]
    elements: Vec<ElementDraft>,
    #[serde(default)]
    timeline: Vec<TimelineDraft>,
    #[serde(default)]
    threads: Vec<ThreadDraft>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ElementDraft {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    role: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    external_goal: String,
    #[serde(default)]
    internal_need: String,
    #[serde(default)]
    fear: String,
    #[serde(default)]
    flaw: String,
    #[serde(default)]
    leverage: String,
    #[serde(default)]
    constraints: Vec<String>,
    #[serde(default)]
    secret: String,
    #[serde(default)]
    relations: Value,
    #[serde(default)]
    metadata: Value,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct TimelineDraft {
    #[serde(default)]
    order_no: i32,
    #[serde(default)]
    title: String,
    #[serde(default)]
    story_time: String,
    #[serde(default)]
    cause: String,
    #[serde(default)]
    participants: Vec<String>,
    #[serde(default)]
    outcome: String,
    #[serde(default)]
    consequence: String,
    #[serde(default)]
    reveal_at: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ThreadDraft {
    #[serde(default)]
    order_no: i32,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    dramatic_question: String,
    #[serde(default)]
    opening_state: String,
    #[serde(default)]
    target_state: String,
    #[serde(default)]
    stakes: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct LogicNodeDraft {
    #[serde(default)]
    order_no: i32,
    #[serde(default)]
    thread_index: Option<usize>,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    cause: String,
    #[serde(default)]
    choice: String,
    #[serde(default)]
    cost: String,
    #[serde(default)]
    outcome: String,
    #[serde(default)]
    state_change: String,
    #[serde(default)]
    knowledge_change: String,
    #[serde(default)]
    participants: Vec<String>,
    #[serde(default)]
    location: String,
    #[serde(default)]
    story_time: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct LogicEdgeDraft {
    #[serde(default)]
    from_order: i32,
    #[serde(default)]
    to_order: i32,
    #[serde(default)]
    relation: String,
    #[serde(default)]
    condition: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PresentationDraft {
    #[serde(default)]
    node_order: i32,
    #[serde(default)]
    medium: String,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    pov: String,
    #[serde(default)]
    dialogue_ratio: i32,
    #[serde(default)]
    narration_ratio: i32,
    #[serde(default)]
    player_agency: String,
    #[serde(default)]
    visual_requirements: String,
    #[serde(default)]
    audio_requirements: String,
    #[serde(default)]
    interaction: String,
    #[serde(default)]
    deliverable: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ChainDraft {
    #[serde(default)]
    nodes: Vec<LogicNodeDraft>,
    #[serde(default)]
    edges: Vec<LogicEdgeDraft>,
    #[serde(default)]
    presentations: Vec<PresentationDraft>,
}

fn parse_foundation(value: &Value) -> AppResult<FoundationDraft> {
    serde_json::from_value(value.clone())
        .map_err(|error| AppError::Message(format!("叙事地基解析失败：{error}")))
}

fn parse_chain(value: &Value) -> AppResult<ChainDraft> {
    serde_json::from_value(value.clone())
        .map_err(|error| AppError::Message(format!("逻辑链解析失败：{error}")))
}

fn non_empty(value: &str) -> bool {
    !value.trim().is_empty()
}

pub fn diagnose_values(
    blueprint: &NarrativeBlueprint,
    elements: &[NarrativeElement],
    timeline: &[NarrativeTimelineEvent],
    threads: &[NarrativeThread],
    nodes: &[NarrativeLogicNode],
    edges: &[NarrativeLogicEdge],
    presentations: &[NarrativePresentation],
) -> Vec<NarrativeDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut push = |code: &str, severity: &str, message: &str, path: &str| {
        diagnostics.push(NarrativeDiagnostic {
            code: code.into(),
            severity: severity.into(),
            message: message.into(),
            path: path.into(),
        });
    };

    if !["novel", "game"].contains(&blueprint.medium.as_str()) {
        push(
            "medium.invalid",
            "error",
            "媒介必须是 novel 或 game",
            "blueprint.medium",
        );
    }
    if !["adaptive", "fixed"].contains(&blueprint.structure_strategy.as_str()) {
        push(
            "strategy.invalid",
            "error",
            "编排策略必须是 adaptive 或 fixed",
            "blueprint.structureStrategy",
        );
    }
    if blueprint.structure_strategy == "fixed" && blueprint.target_units.unwrap_or(0) < 1 {
        push(
            "target.missing",
            "error",
            "固定结构必须填写目标章节或任务数",
            "blueprint.targetUnits",
        );
    }
    if !non_empty(&blueprint.core_dramatic_question) {
        push(
            "question.missing",
            "error",
            "缺少全书核心戏剧问题",
            "blueprint.coreDramaticQuestion",
        );
    }
    if !non_empty(&blueprint.opening_state) || !non_empty(&blueprint.ending_state) {
        push(
            "state.missing",
            "error",
            "必须明确开局长板与终局状态的变化",
            "blueprint.openingState",
        );
    }

    let characters = elements
        .iter()
        .filter(|item| item.kind == "character")
        .count();
    let factions = elements
        .iter()
        .filter(|item| item.kind == "faction")
        .count();
    let world_rules = elements
        .iter()
        .filter(|item| item.kind == "world_rule")
        .count();
    let element_names: HashSet<&str> = elements
        .iter()
        .map(|item| item.name.trim())
        .filter(|name| !name.is_empty())
        .collect();
    let mut relation_edges = 0usize;
    if characters < 2 {
        push(
            "characters.thin",
            "error",
            "至少需要两个目标互相冲突的核心人物",
            "elements",
        );
    }
    if factions + world_rules < 2 {
        push(
            "world.thin",
            "error",
            "至少需要两条势力/世界规则约束，避免逻辑悬空",
            "elements",
        );
    }
    for (index, element) in elements.iter().enumerate() {
        if !non_empty(&element.name) || !non_empty(&element.summary) {
            push(
                "element.incomplete",
                "error",
                "人物或设定必须填写名称和可验证摘要",
                &format!("elements[{index}]"),
            );
        }
        if element.kind == "character" {
            if !non_empty(&element.external_goal) || !non_empty(&element.fear) {
                push(
                    "character.motive",
                    "error",
                    "核心人物必须同时有外部目标和恐惧，否则无法形成稳定动机链",
                    &format!("elements[{index}]"),
                );
            }
        }
        if ["faction", "world_rule"].contains(&element.kind.as_str())
            && element.constraints.iter().all(|value| !non_empty(value))
        {
            push(
                "world.constraint",
                "error",
                "势力或世界规则必须写明不能突破的边界/代价",
                &format!("elements[{index}].constraints"),
            );
        }
        if let Some(relations) = element.relations.as_array() {
            for (relation_index, relation) in relations.iter().enumerate() {
                relation_edges += 1;
                let target = relation
                    .get("target")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim();
                if target.is_empty() || !element_names.contains(target) {
                    push(
                        "relation.target",
                        "error",
                        "人物/势力关系指向了未定义的对象",
                        &format!("elements[{index}].relations[{relation_index}].target"),
                    );
                }
                if relation
                    .get("tension")
                    .and_then(Value::as_str)
                    .map(|value| value.trim().is_empty())
                    .unwrap_or(true)
                {
                    push(
                        "relation.tension",
                        "error",
                        "关系必须写明当前张力或冲突，不能只存标签",
                        &format!("elements[{index}].relations[{relation_index}].tension"),
                    );
                }
            }
        }
    }
    if characters >= 2 && relation_edges == 0 {
        push(
            "relations.missing",
            "error",
            "核心人物之间缺少可验证关系链",
            "elements[].relations",
        );
    }
    if timeline.len() < 3 {
        push(
            "timeline.thin",
            "error",
            "时间线至少需要三个有因果关系的节点",
            "timeline",
        );
    }
    let mut timeline_orders = HashSet::new();
    for (index, event) in timeline.iter().enumerate() {
        if event.order_no <= 0 || !timeline_orders.insert(event.order_no) {
            push(
                "timeline.order",
                "error",
                "时间线序号必须唯一且为正数",
                &format!("timeline[{index}].orderNo"),
            );
        }
        for participant in &event.participants {
            if !element_names.contains(participant.trim()) {
                push(
                    "timeline.participant",
                    "warning",
                    "时间线参与者尚未在人物/势力档案中定义",
                    &format!("timeline[{index}].participants"),
                );
            }
        }
        if !non_empty(&event.cause) || !non_empty(&event.outcome) || !non_empty(&event.consequence)
        {
            push(
                "timeline.cause",
                "error",
                "时间线事件必须写清前因、结果和后续影响",
                &format!("timeline[{index}]"),
            );
        }
    }
    if threads.is_empty() {
        push("thread.missing", "error", "至少需要一条叙事主线", "threads");
    }
    if nodes.len() < 6 {
        push(
            "logic.thin",
            "error",
            "逻辑链至少需要六个可验证的状态变化节点",
            "nodes",
        );
    }

    let mut orders = HashSet::new();
    for (index, node) in nodes.iter().enumerate() {
        if node.order_no <= 0 || !orders.insert(node.order_no) {
            push(
                "logic.order",
                "error",
                "逻辑节点序号必须唯一且为正数",
                &format!("nodes[{index}]"),
            );
        }
        for (field, value) in [
            ("cause", &node.cause),
            ("choice", &node.choice),
            ("cost", &node.cost),
            ("outcome", &node.outcome),
            ("stateChange", &node.state_change),
            ("knowledgeChange", &node.knowledge_change),
        ] {
            if !non_empty(value) {
                push(
                    "logic.incomplete",
                    "error",
                    "逻辑节点必须同时写清原因、选择、代价、结果、状态变化和认知变化",
                    &format!("nodes[{index}].{field}"),
                );
            }
        }
        if node.thread_id.is_none() {
            push(
                "logic.thread",
                "error",
                "逻辑节点必须归属叙事线",
                &format!("nodes[{index}]"),
            );
        }
        if node.participants.is_empty() {
            push(
                "logic.participant",
                "error",
                "逻辑节点必须写明行动主体或受影响对象",
                &format!("nodes[{index}].participants"),
            );
        }
        for participant in &node.participants {
            if !element_names.contains(participant.trim()) {
                push(
                    "logic.participant.unknown",
                    "warning",
                    "逻辑节点参与者尚未在人物/势力档案中定义",
                    &format!("nodes[{index}].participants"),
                );
            }
        }
    }

    let ids: HashSet<&str> = nodes.iter().map(|node| node.id.as_str()).collect();
    let mut incoming: HashMap<&str, usize> =
        nodes.iter().map(|node| (node.id.as_str(), 0)).collect();
    let mut outgoing: HashMap<&str, usize> =
        nodes.iter().map(|node| (node.id.as_str(), 0)).collect();
    for (index, edge) in edges.iter().enumerate() {
        if !ids.contains(edge.from_node_id.as_str()) || !ids.contains(edge.to_node_id.as_str()) {
            push(
                "edge.dangling",
                "error",
                "因果边指向不存在的逻辑节点",
                &format!("edges[{index}]"),
            );
        }
        if !non_empty(&edge.relation) {
            push(
                "edge.relation",
                "error",
                "因果边必须说明关系类型",
                &format!("edges[{index}]"),
            );
        }
        *incoming.entry(edge.to_node_id.as_str()).or_default() += 1;
        *outgoing.entry(edge.from_node_id.as_str()).or_default() += 1;
    }
    if nodes.len() > 1 {
        let roots = nodes
            .iter()
            .filter(|node| incoming.get(node.id.as_str()).copied().unwrap_or(0) == 0)
            .count();
        let terminals = nodes
            .iter()
            .filter(|node| outgoing.get(node.id.as_str()).copied().unwrap_or(0) == 0)
            .count();
        if roots != 1 {
            push(
                "graph.roots",
                "error",
                "因果链应只有一个核心起点，当前存在多个无前因节点",
                "edges",
            );
        }
        if terminals == 0 {
            push("graph.terminal", "error", "因果链没有终局节点", "edges");
        }
    }
    if nodes.len() > 1 && edges.len() < nodes.len() - 1 {
        push(
            "graph.disconnected",
            "error",
            "因果链存在断点，无法从起点完整推到终局",
            "edges",
        );
    }
    if let Some(start) = nodes
        .iter()
        .find(|node| incoming.get(node.id.as_str()).copied().unwrap_or(0) == 0)
        .map(|node| node.id.as_str())
    {
        let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
        for edge in edges {
            adjacency
                .entry(edge.from_node_id.as_str())
                .or_default()
                .push(edge.to_node_id.as_str());
        }
        let mut seen = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(start);
        while let Some(id) = queue.pop_front() {
            if !seen.insert(id) {
                continue;
            }
            for next in adjacency.get(id).into_iter().flatten() {
                queue.push_back(next);
            }
        }
        if seen.len() < nodes.len() {
            push(
                "graph.unreachable",
                "error",
                "存在无法从核心起点到达的逻辑节点",
                "edges",
            );
        }
    }

    if blueprint.medium == "game" {
        if presentations.len() < nodes.len() {
            push(
                "presentation.missing",
                "error",
                "游戏蓝本的每个逻辑节点都必须有呈现契约，明确对白、环境、道具或过场动画",
                "presentations",
            );
        }
        for (index, presentation) in presentations.iter().enumerate() {
            if presentation.dialogue_ratio + presentation.narration_ratio > 100 {
                push(
                    "presentation.ratio",
                    "error",
                    "对白与旁白比例之和不能超过 100",
                    &format!("presentations[{index}]"),
                );
            }
            if ![
                "dialogue",
                "environmental",
                "prop_text",
                "cutscene",
                "mixed",
                "silent",
            ]
            .contains(&presentation.mode.as_str())
            {
                push(
                    "presentation.mode",
                    "error",
                    "呈现模式不在允许范围内",
                    &format!("presentations[{index}].mode"),
                );
            }
            if !non_empty(&presentation.deliverable) {
                push(
                    "presentation.deliverable",
                    "error",
                    "游戏呈现契约必须写明可交付的脚本形式",
                    &format!("presentations[{index}].deliverable"),
                );
            }
        }
    }

    diagnostics
}

pub fn save_generated(
    conn: &mut Connection,
    project_id: &str,
    premise_id: &str,
    input: &NarrativeGenerateInput,
    foundation_value: &Value,
    chain_value: &Value,
) -> AppResult<NarrativeBlueprintView> {
    let foundation = parse_foundation(foundation_value)?;
    let chain = parse_chain(chain_value)?;
    if foundation.title.trim().is_empty() {
        return Err(AppError::Message("模型没有返回蓝本标题".into()));
    }
    if chain.nodes.is_empty() {
        return Err(AppError::Message("模型没有返回逻辑链节点".into()));
    }
    if input.medium == "game" && chain.presentations.is_empty() {
        return Err(AppError::Message("游戏蓝本缺少呈现契约".into()));
    }
    let confirmed: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM narrative_blueprint WHERE project_id=?1 AND status='confirmed')",
        [project_id],
        |row| row.get(0),
    )?;
    if confirmed {
        return Err(AppError::Message(
            "已有已确认的叙事蓝本。请先在新分支中建立另一套蓝本，或明确重开规划。".into(),
        ));
    }

    let tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM narrative_blueprint WHERE project_id=?1 AND status='candidate'",
        [project_id],
    )?;
    let ts = now();
    let blueprint_id = uuid::Uuid::new_v4().to_string();
    let target_units = if input.structure_strategy == "fixed" {
        input.target_units
    } else {
        input.target_units
    };
    tx.execute(
        "INSERT INTO narrative_blueprint
         (id, project_id, premise_id, title, medium, structure_strategy, target_units,
          core_dramatic_question, narrative_promise, opening_state, ending_state,
          theme_question, status, completeness, diagnostics, version, created_at, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,'candidate',0,'[]',1,?13,?13)",
        params![
            blueprint_id,
            project_id,
            premise_id,
            foundation.title.trim(),
            input.medium,
            input.structure_strategy,
            target_units,
            foundation.core_dramatic_question.trim(),
            foundation.narrative_promise.trim(),
            foundation.opening_state.trim(),
            foundation.ending_state.trim(),
            foundation.theme_question.trim(),
            ts
        ],
    )?;

    let mut element_ids = HashMap::new();
    for (index, item) in foundation.elements.iter().enumerate() {
        if item.name.trim().is_empty() || item.kind.trim().is_empty() {
            continue;
        }
        let id = uuid::Uuid::new_v4().to_string();
        element_ids.insert(item.name.trim().to_string(), id.clone());
        tx.execute(
            "INSERT INTO narrative_element
             (id, blueprint_id, kind, name, role, summary, external_goal, internal_need,
              fear, flaw, leverage, constraints_json, secret, relations_json,
              metadata_json, status, sort_order, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,'candidate',?16,?17,?17)",
            params![
                id,
                blueprint_id,
                item.kind.trim(),
                item.name.trim(),
                item.role.trim(),
                item.summary.trim(),
                item.external_goal.trim(),
                item.internal_need.trim(),
                item.fear.trim(),
                item.flaw.trim(),
                item.leverage.trim(),
                serde_json::to_string(&item.constraints)?,
                item.secret.trim(),
                serde_json::to_string(&if item.relations.is_null() {
                    json!([])
                } else {
                    item.relations.clone()
                })?,
                serde_json::to_string(&if item.metadata.is_null() {
                    json!({})
                } else {
                    item.metadata.clone()
                })?,
                index as i32,
                ts
            ],
        )?;
    }

    for (index, event) in foundation.timeline.iter().enumerate() {
        let order_no = if event.order_no > 0 {
            event.order_no
        } else {
            (index + 1) as i32
        };
        tx.execute(
            "INSERT INTO narrative_timeline_event
             (id, blueprint_id, order_no, title, story_time, cause, participants_json,
              outcome, consequence, reveal_at, status, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'candidate',?11,?11)",
            params![
                uuid::Uuid::new_v4().to_string(),
                blueprint_id,
                order_no,
                event.title.trim(),
                event.story_time.trim(),
                event.cause.trim(),
                serde_json::to_string(&event.participants)?,
                event.outcome.trim(),
                event.consequence.trim(),
                event.reveal_at.trim(),
                ts
            ],
        )?;
    }

    let mut thread_ids = Vec::new();
    for (index, thread) in foundation.threads.iter().enumerate() {
        let id = uuid::Uuid::new_v4().to_string();
        thread_ids.push(id.clone());
        let order_no = if thread.order_no > 0 {
            thread.order_no
        } else {
            (index + 1) as i32
        };
        tx.execute(
            "INSERT INTO narrative_thread
             (id, blueprint_id, order_no, kind, title, dramatic_question, opening_state,
              target_state, stakes, status, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'candidate',?10,?10)",
            params![
                id,
                blueprint_id,
                order_no,
                if thread.kind.trim().is_empty() {
                    "main"
                } else {
                    thread.kind.trim()
                },
                thread.title.trim(),
                thread.dramatic_question.trim(),
                thread.opening_state.trim(),
                thread.target_state.trim(),
                thread.stakes.trim(),
                ts
            ],
        )?;
    }

    let mut node_ids = HashMap::new();
    for (index, node) in chain.nodes.iter().enumerate() {
        let order_no = if node.order_no > 0 {
            node.order_no
        } else {
            (index + 1) as i32
        };
        let id = uuid::Uuid::new_v4().to_string();
        node_ids.insert(order_no, id.clone());
        let thread_id = node
            .thread_index
            .and_then(|thread_index| thread_ids.get(thread_index).cloned());
        tx.execute(
            "INSERT INTO narrative_logic_node
             (id, blueprint_id, thread_id, order_no, kind, title, cause, choice, cost,
              outcome, state_change, knowledge_change, participants_json, location,
              story_time, status, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,'candidate',?16,?16)",
            params![
                id,
                blueprint_id,
                thread_id,
                order_no,
                if node.kind.trim().is_empty() {
                    "beat"
                } else {
                    node.kind.trim()
                },
                node.title.trim(),
                node.cause.trim(),
                node.choice.trim(),
                node.cost.trim(),
                node.outcome.trim(),
                node.state_change.trim(),
                node.knowledge_change.trim(),
                serde_json::to_string(&node.participants)?,
                node.location.trim(),
                node.story_time.trim(),
                ts
            ],
        )?;
    }

    for edge in &chain.edges {
        let Some(from_node_id) = node_ids.get(&edge.from_order) else {
            continue;
        };
        let Some(to_node_id) = node_ids.get(&edge.to_order) else {
            continue;
        };
        tx.execute(
            "INSERT INTO narrative_logic_edge
             (id, blueprint_id, from_node_id, to_node_id, relation, condition, status, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,'candidate',?7)",
            params![
                uuid::Uuid::new_v4().to_string(),
                blueprint_id,
                from_node_id,
                to_node_id,
                edge.relation.trim(),
                edge.condition.trim(),
                ts
            ],
        )?;
    }

    for presentation in &chain.presentations {
        let Some(logic_node_id) = node_ids.get(&presentation.node_order) else {
            continue;
        };
        let medium = if presentation.medium.trim().is_empty() {
            input.medium.as_str()
        } else {
            presentation.medium.trim()
        };
        tx.execute(
            "INSERT INTO narrative_presentation
             (id, blueprint_id, logic_node_id, medium, mode, pov, dialogue_ratio,
              narration_ratio, player_agency, visual_requirements, audio_requirements,
              interaction, deliverable, status, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,'candidate',?14,?14)",
            params![
                uuid::Uuid::new_v4().to_string(),
                blueprint_id,
                logic_node_id,
                medium,
                if presentation.mode.trim().is_empty() {
                    "mixed"
                } else {
                    presentation.mode.trim()
                },
                presentation.pov.trim(),
                presentation.dialogue_ratio.clamp(0, 100),
                presentation.narration_ratio.clamp(0, 100),
                presentation.player_agency.trim(),
                presentation.visual_requirements.trim(),
                presentation.audio_requirements.trim(),
                presentation.interaction.trim(),
                presentation.deliverable.trim(),
                ts
            ],
        )?;
    }

    tx.commit()?;
    refresh_diagnostics(conn, project_id, &blueprint_id)?;
    let _ = element_ids;
    get_view(conn, project_id, &blueprint_id)
}

pub fn refresh_diagnostics(
    conn: &Connection,
    project_id: &str,
    blueprint_id: &str,
) -> AppResult<Vec<NarrativeDiagnostic>> {
    let view = get_view(conn, project_id, blueprint_id)?;
    let errors = view
        .diagnostics
        .iter()
        .filter(|item| item.severity == "error")
        .count() as i32;
    let warnings = view
        .diagnostics
        .iter()
        .filter(|item| item.severity == "warning")
        .count() as i32;
    let total = view.diagnostics.len().max(1) as f64;
    let completeness = ((total - errors as f64 - warnings as f64 * 0.35) / total * 100.0)
        .round()
        .clamp(0.0, 100.0) as i32;
    conn.execute(
        "UPDATE narrative_blueprint SET completeness=?3, diagnostics=?4, updated_at=?5
         WHERE id=?1 AND project_id=?2",
        params![
            blueprint_id,
            project_id,
            completeness,
            serde_json::to_string(&view.diagnostics)?,
            now()
        ],
    )?;
    Ok(view.diagnostics)
}

pub fn confirm(
    conn: &mut Connection,
    project_id: &str,
    blueprint_id: &str,
) -> AppResult<NarrativeBlueprintView> {
    let view = get_view(conn, project_id, blueprint_id)?;
    let blocking: Vec<&NarrativeDiagnostic> = view
        .diagnostics
        .iter()
        .filter(|item| item.severity == "error")
        .collect();
    if !blocking.is_empty() {
        return Err(AppError::Message(format!(
            "叙事蓝本仍有 {} 项阻断问题，请先修复：{}",
            blocking.len(),
            blocking
                .iter()
                .take(3)
                .map(|item| item.message.as_str())
                .collect::<Vec<_>>()
                .join("；")
        )));
    }
    let tx = conn.transaction()?;
    let ts = now();
    tx.execute(
        "UPDATE narrative_blueprint SET status='archived', updated_at=?3
         WHERE project_id=?1 AND id!=?2 AND status='confirmed'",
        params![project_id, blueprint_id, ts],
    )?;
    tx.execute(
        "UPDATE narrative_blueprint SET status='confirmed', version=version+1, updated_at=?3
         WHERE project_id=?1 AND id=?2",
        params![project_id, blueprint_id, ts],
    )?;
    for table in [
        "narrative_element",
        "narrative_timeline_event",
        "narrative_thread",
        "narrative_logic_node",
        "narrative_logic_edge",
        "narrative_presentation",
    ] {
        tx.execute(
            &format!("UPDATE {table} SET status='confirmed' WHERE blueprint_id=?1"),
            [blueprint_id],
        )?;
    }
    tx.commit()?;
    get_view(conn, project_id, blueprint_id)
}

pub fn chapter_logic_context(
    conn: &Connection,
    project_id: &str,
    chapter_number: i32,
) -> AppResult<String> {
    let Some(view) = current(conn, project_id)? else {
        return Ok("尚未确认叙事蓝本。".into());
    };
    let mapping = conn
        .query_row(
            "SELECT logic_node_ids, presentation_mode, narrative_function
             FROM outline_node
             WHERE project_id=?1 AND level='chapter' AND order_no=?2 AND status!='dropped'
             ORDER BY updated_at DESC LIMIT 1",
            params![project_id, chapter_number],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?;
    let (ids, chapter_mode, chapter_function) = mapping.unwrap_or_default();
    let ids: Vec<String> = serde_json::from_str(&ids).unwrap_or_default();
    let selected: Vec<&crate::domain::NarrativeLogicNode> = if ids.is_empty() {
        Vec::new()
    } else {
        view.nodes
            .iter()
            .filter(|node| ids.iter().any(|id| id == &node.id || id == &node.title))
            .collect()
    };
    let nodes: Vec<&crate::domain::NarrativeLogicNode> = if selected.is_empty() {
        // Legacy skeletons have no mapping. Keep generation possible, but make the missing
        // authorization explicit instead of silently replacing the blueprint with prose.
        view.nodes.iter().take(6).collect()
    } else {
        selected
    };
    let node_ids: HashSet<&str> = nodes.iter().map(|node| node.id.as_str()).collect();
    let participants: HashSet<&str> = nodes
        .iter()
        .flat_map(|node| node.participants.iter().map(String::as_str))
        .collect();
    let relevant_elements: Vec<_> = view
        .elements
        .iter()
        .filter(|element| participants.contains(element.name.as_str()))
        .collect();
    let relevant_edges: Vec<_> = view
        .edges
        .iter()
        .filter(|edge| {
            node_ids.contains(edge.from_node_id.as_str())
                || node_ids.contains(edge.to_node_id.as_str())
        })
        .collect();
    let relevant_presentations: Vec<_> = view
        .presentations
        .iter()
        .filter(|item| node_ids.contains(item.logic_node_id.as_str()))
        .collect();

    let mut parts = vec![format!(
        "核心戏剧问题：{}\n开局长板：{}\n终局状态：{}\n本章编排模式：{}｜章节功能：{}",
        view.blueprint.core_dramatic_question,
        view.blueprint.opening_state,
        view.blueprint.ending_state,
        if chapter_mode.trim().is_empty() {
            "未标注"
        } else {
            chapter_mode.as_str()
        },
        if chapter_function.trim().is_empty() {
            "未标注"
        } else {
            chapter_function.as_str()
        },
    )];
    if ids.is_empty() {
        parts.push(
            "[警告] 本章骨架尚未绑定逻辑节点。以下仅列出前置逻辑，正文不得借此擅自推进终局或新增因果。"
                .into(),
        );
    }
    parts.push("本章必须承接的因果节点：".into());
    for node in nodes {
        parts.push(format!(
            "- [{}] {}｜本线：{}｜前因：{}｜选择：{}｜代价：{}｜结果：{}｜状态变化：{}｜认知变化：{}｜参与者：{}｜地点：{}｜时间：{}",
            node.order_no,
            node.title,
            if node.thread_title.is_empty() { "主线" } else { &node.thread_title },
            node.cause,
            node.choice,
            node.cost,
            node.outcome,
            node.state_change,
            node.knowledge_change,
            node.participants.join("、"),
            node.location,
            node.story_time
        ));
    }
    if !relevant_elements.is_empty() {
        parts.push("本章人物与设定约束：".into());
        for element in relevant_elements {
            parts.push(format!(
                "- [{}] {}｜功能：{}｜外部目标：{}｜内在需要：{}｜恐惧：{}｜缺陷：{}｜筹码：{}｜秘密：{}",
                element.kind,
                element.name,
                element.role,
                element.external_goal,
                element.internal_need,
                element.fear,
                element.flaw,
                element.leverage,
                element.secret
            ));
        }
    }
    if !relevant_edges.is_empty() {
        parts.push("本章上下游因果：".into());
        for edge in relevant_edges {
            parts.push(format!(
                "- {} → {}｜{}｜成立条件：{}",
                edge.from_title,
                edge.to_title,
                edge.relation,
                if edge.condition.is_empty() {
                    "无额外条件"
                } else {
                    &edge.condition
                }
            ));
        }
    }
    if !relevant_presentations.is_empty() {
        parts.push("本章呈现契约：".into());
        for item in relevant_presentations {
            parts.push(format!(
                "- {}｜媒介：{}｜模式：{}｜POV：{}｜对白 {}%｜旁白 {}%｜玩家能动性：{}｜画面：{}｜声音：{}｜交互：{}｜交付：{}",
                item.logic_node_title,
                item.medium,
                item.mode,
                item.pov,
                item.dialogue_ratio,
                item.narration_ratio,
                item.player_agency,
                item.visual_requirements,
                item.audio_requirements,
                item.interaction,
                item.deliverable
            ));
        }
    }
    Ok(parts.join("\n"))
}

pub fn logic_context(conn: &Connection, project_id: &str, limit: i32) -> AppResult<String> {
    let Some(view) = current(conn, project_id)? else {
        return Ok("尚未确认叙事蓝本。".into());
    };
    let mut parts = vec![format!(
        "核心戏剧问题：{}\n开局长板：{}\n终局状态：{}\n媒介：{}｜编排：{}",
        view.blueprint.core_dramatic_question,
        view.blueprint.opening_state,
        view.blueprint.ending_state,
        view.blueprint.medium,
        view.blueprint.structure_strategy
    )];
    parts.push("关键设定与关系：".into());
    for item in view.elements.iter().take(limit.max(1) as usize) {
        parts.push(format!(
            "- [{}] {}｜角色/功能：{}｜目标：{}｜阻力：{}｜秘密：{}",
            item.kind, item.name, item.role, item.external_goal, item.fear, item.secret
        ));
    }
    parts.push("时间线：".into());
    for event in view.timeline.iter().take(limit.max(1) as usize) {
        parts.push(format!(
            "- {}｜时间：{}｜因：{}｜果：{}｜影响：{}",
            event.title, event.story_time, event.cause, event.outcome, event.consequence
        ));
    }
    parts.push("因果链：".into());
    for node in view.nodes.iter().take(limit.max(1) as usize) {
        parts.push(format!(
            "- #{} {}｜因：{}｜选择：{}｜代价：{}｜结果：{}｜状态变化：{}｜认知变化：{}",
            node.order_no,
            node.title,
            node.cause,
            node.choice,
            node.cost,
            node.outcome,
            node.state_change,
            node.knowledge_change
        ));
    }
    Ok(parts.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project;

    pub(super) fn seed_project() -> (project::LoadedProject, String) {
        let dir = tempfile::tempdir().unwrap();
        let mut loaded = project::create(&crate::domain::CreateProjectInput {
            name: "blueprint".into(),
            root_path: dir.path().join("p").to_string_lossy().into_owned(),
            mode: "idea".into(),
            writing_language: None,
        })
        .unwrap();
        let pid = loaded.summary.id.clone();
        let cards = crate::seed::insert_directions(
            &mut loaded.conn,
            &pid,
            &[json!({
                "logline": "雨夜车站",
                "core_conflict": "林澈想救妹妹，但车站要求他支付记忆",
                "genre": "悬疑",
                "theme": "记忆"
            })],
            None,
        )
        .unwrap();
        let premise = crate::seed::confirm(&mut loaded.conn, &pid, &cards[0].id).unwrap();
        (loaded, premise.id)
    }

    pub(super) fn foundation() -> Value {
        json!({
            "title": "雨夜车站",
            "coreDramaticQuestion": "林澈能否在不抹除妹妹存在的前提下关闭车站？",
            "narrativePromise": "每一次接近真相都让主角失去一段可验证的记忆",
            "openingState": "林澈只相信可验证的记录",
            "endingState": "林澈接受无法被记录的承诺",
            "themeQuestion": "记忆是否是人的唯一证据",
            "elements": [
                {"kind":"character","name":"林澈","role":"主角","summary":"守夜人","externalGoal":"救回妹妹","internalNeed":"承认爱无法被证明","fear":"忘记妹妹","flaw":"只相信证据","leverage":"能读取车票","constraints":["不能撒谎"],"secret":"他曾主动抹去一段记忆","relations":[{"target":"顾鸢","type":"旧识/对手","tension":"双方都相信对方掌握妹妹失踪的关键证据"}],"metadata":{}},
                {"kind":"character","name":"顾鸢","role":"对手","summary":"站长","externalGoal":"维持车站","internalNeed":"被原谅","fear":"失去秩序","flaw":"控制欲","leverage":"掌握时刻表","constraints":["不能离开车站"],"secret":"妹妹仍活着","relations":[{"target":"林澈","type":"旧识/对手","tension":"她必须阻止林澈恢复会摧毁车站的记忆"}],"metadata":{}},
                {"kind":"faction","name":"巡夜局","role":"压力来源","summary":"封锁异常车站的机构","externalGoal":"关闭车站","internalNeed":"","fear":"","flaw":"","leverage":"封锁权","constraints":["午夜前行动"],"secret":"","relations":[],"metadata":{}},
                {"kind":"world_rule","name":"记忆票价","role":"世界规则","summary":"通过车站必须支付一段记忆","externalGoal":"","internalNeed":"","fear":"","flaw":"","leverage":"","constraints":["支付不可逆"],"secret":"","relations":[],"metadata":{}}
            ],
            "timeline": [
                {"orderNo":1,"title":"妹妹失踪","storyTime":"十年前","cause":"妹妹进入车站","participants":["林澈","林小雨"],"outcome":"车站封锁","consequence":"林澈成为守夜人","revealAt":"早期"},
                {"orderNo":2,"title":"支付记忆","storyTime":"十年前","cause":"林澈试图追入车站","participants":["林澈","顾鸢"],"outcome":"林澈忘记妹妹的脸","consequence":"他保存所有记录","revealAt":"中段"},
                {"orderNo":3,"title":"时刻表重启","storyTime":"现在","cause":"车票出现","participants":["林澈","顾鸢"],"outcome":"车站再次开启","consequence":"双方目标正面冲突","revealAt":"开篇"}
            ],
            "threads": [
                {"orderNo":1,"kind":"main","title":"车站真相线","dramaticQuestion":"妹妹为何仍在车站","openingState":"未知","targetState":"真相揭示","stakes":"林澈的存在"},
                {"orderNo":2,"kind":"relationship","title":"林澈与顾鸢","dramaticQuestion":"谁该承担代价","openingState":"敌对","targetState":"有限信任","stakes":"两人记忆"}
            ]
        })
    }

    pub(super) fn chain() -> Value {
        json!({
            "nodes": [
                {"orderNo":1,"threadIndex":0,"kind":"inciting","title":"车票出现","cause":"巡夜局即将封站","choice":"林澈登上列车","cost":"暴露能力","outcome":"车站重启","stateChange":"封锁变成倒计时","knowledgeChange":"林澈知道妹妹可能仍在","participants":["林澈"],"location":"旧站台","storyTime":"第一夜"},
                {"orderNo":2,"threadIndex":0,"kind":"reversal","title":"站长拦截","cause":"林澈登车","choice":"与顾鸢交易","cost":"交出记忆记录","outcome":"获得十分钟","stateChange":"证据消失","knowledgeChange":"顾鸢认识妹妹","participants":["林澈","顾鸢"],"location":"站长室","storyTime":"第一夜"},
                {"orderNo":3,"threadIndex":1,"kind":"complication","title":"检票机索价","cause":"交易后时间不足","choice":"接受检票","cost":"忘掉母亲","outcome":"进入内站","stateChange":"核心证据减少","knowledgeChange":"内站由记忆构成","participants":["林澈"],"location":"检票口","storyTime":"第一夜"},
                {"orderNo":4,"threadIndex":0,"kind":"reveal","title":"妹妹的记录","cause":"进入内站","choice":"读取空白车票","cost":"失去姓名","outcome":"得知顾鸢撒谎","stateChange":"敌对升级","knowledgeChange":"妹妹主动留下","participants":["林澈","顾鸢"],"location":"内站","storyTime":"第二夜"},
                {"orderNo":5,"threadIndex":1,"kind":"crisis","title":"关闭时刻","cause":"巡夜局行动","choice":"救妹妹或保住乘客","cost":"必有一方被遗忘","outcome":"林澈公开能力","stateChange":"身份暴露","knowledgeChange":"顾鸢承认共谋","participants":["林澈","顾鸢","巡夜局"],"location":"主站台","storyTime":"第二夜"},
                {"orderNo":6,"threadIndex":1,"kind":"climax","title":"自我定价","cause":"必须支付最高票价","choice":"用自己的存在换回妹妹","cost":"世界忘记林澈","outcome":"妹妹归来","stateChange":"林澈从记录中消失","knowledgeChange":"妹妹保留他的记忆","participants":["林澈","顾鸢","林小雨"],"location":"站门","storyTime":"黎明"},
                {"orderNo":7,"threadIndex":0,"kind":"resolution","title":"无记录的承诺","cause":"代价已经支付","choice":"顾鸢放弃车站","cost":"失去职位","outcome":"车站永久关闭","stateChange":"世界恢复但无人记得英雄","knowledgeChange":"顾鸢保留秘密","participants":["顾鸢"],"location":"废墟","storyTime":"多年后"}
            ],
            "edges": [
                {"fromOrder":1,"toOrder":2,"relation":"导致","condition":"林澈未退出"},
                {"fromOrder":2,"toOrder":3,"relation":"迫使","condition":"交易成立"},
                {"fromOrder":3,"toOrder":4,"relation":"开启","condition":"进入内站"},
                {"fromOrder":4,"toOrder":5,"relation":"触发","condition":"谎言被识破"},
                {"fromOrder":5,"toOrder":6,"relation":"逼迫","condition":"巡夜局抵达"},
                {"fromOrder":6,"toOrder":7,"relation":"收束","condition":"妹妹归来"}
            ],
            "presentations": []
        })
    }

    #[test]
    fn saves_candidate_and_reports_complete_logic_chain() {
        let (mut loaded, premise_id) = seed_project();
        let pid = loaded.summary.id.clone();
        let view = save_generated(
            &mut loaded.conn,
            &pid,
            &premise_id,
            &NarrativeGenerateInput {
                depth: None,
                medium: "novel".into(),
                structure_strategy: "adaptive".into(),
                target_units: None,
            },
            &foundation(),
            &chain(),
        )
        .unwrap();
        assert_eq!(view.nodes.len(), 7);
        assert_eq!(view.edges.len(), 6);
        assert!(view.diagnostics.iter().all(|item| item.severity != "error"));
        let hero = view
            .elements
            .iter()
            .find(|item| item.name == "林澈")
            .unwrap();
        assert_eq!(hero.secret, "他曾主动抹去一段记忆");
        assert_eq!(hero.status, "candidate");
    }

    #[test]
    fn confirms_blueprint_and_returns_context() {
        let (mut loaded, premise_id) = seed_project();
        let pid = loaded.summary.id.clone();
        let view = save_generated(
            &mut loaded.conn,
            &pid,
            &premise_id,
            &NarrativeGenerateInput {
                depth: None,
                medium: "novel".into(),
                structure_strategy: "adaptive".into(),
                target_units: None,
            },
            &foundation(),
            &chain(),
        )
        .unwrap();
        let confirmed = confirm(&mut loaded.conn, &pid, &view.blueprint.id).unwrap();
        assert_eq!(confirmed.blueprint.status, "confirmed");
        assert!(logic_context(&loaded.conn, &pid, 20)
            .unwrap()
            .contains("核心戏剧问题"));
    }
}

pub fn update_candidate(
    conn: &mut Connection,
    project_id: &str,
    input: &crate::domain::NarrativeBlueprintEditInput,
) -> AppResult<NarrativeBlueprintView> {
    if input.title.trim().is_empty() {
        return Err(AppError::Message("蓝本标题不能为空".into()));
    }
    if !["novel", "game"].contains(&input.medium.as_str()) {
        return Err(AppError::Message("媒介必须是 novel 或 game".into()));
    }
    if !["adaptive", "fixed"].contains(&input.structure_strategy.as_str()) {
        return Err(AppError::Message("编排策略必须是 adaptive 或 fixed".into()));
    }
    if input.structure_strategy == "fixed" && input.target_units.unwrap_or(0) < 1 {
        return Err(AppError::Message("固定结构必须填写目标章节或任务数".into()));
    }
    let status: String = conn
        .query_row(
            "SELECT status FROM narrative_blueprint WHERE project_id=?1 AND id=?2",
            params![project_id, input.id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::Message("找不到叙事蓝本".into()))?;
    if status == "confirmed" {
        return Err(AppError::Message(
            "已确认的叙事蓝本不能直接改写。请建立新分支或重开蓝本规划。".into(),
        ));
    }

    let tx = conn.transaction()?;
    let ts = now();
    tx.execute(
        "UPDATE narrative_blueprint SET title=?3, medium=?4, structure_strategy=?5,
            target_units=?6, core_dramatic_question=?7, narrative_promise=?8,
            opening_state=?9, ending_state=?10, theme_question=?11,
            version=version+1, updated_at=?12
         WHERE project_id=?1 AND id=?2",
        params![
            project_id,
            input.id,
            input.title.trim(),
            input.medium,
            input.structure_strategy,
            input.target_units,
            input.core_dramatic_question.trim(),
            input.narrative_promise.trim(),
            input.opening_state.trim(),
            input.ending_state.trim(),
            input.theme_question.trim(),
            ts
        ],
    )?;
    for (index, item) in input.elements.iter().enumerate() {
        tx.execute(
            "UPDATE narrative_element SET kind=?3, name=?4, role=?5, summary=?6,
                external_goal=?7, internal_need=?8, fear=?9, flaw=?10, leverage=?11,
                constraints_json=?12, secret=?13, relations_json=?14, metadata_json=?15,
                sort_order=?16, updated_at=?17
             WHERE id=?1 AND blueprint_id=?2",
            params![
                item.id,
                input.id,
                item.kind.trim(),
                item.name.trim(),
                item.role.trim(),
                item.summary.trim(),
                item.external_goal.trim(),
                item.internal_need.trim(),
                item.fear.trim(),
                item.flaw.trim(),
                item.leverage.trim(),
                serde_json::to_string(&item.constraints)?,
                item.secret.trim(),
                serde_json::to_string(&item.relations)?,
                serde_json::to_string(&item.metadata)?,
                index as i32,
                ts
            ],
        )?;
    }
    for (index, item) in input.timeline.iter().enumerate() {
        tx.execute(
            "UPDATE narrative_timeline_event SET order_no=?3, title=?4, story_time=?5,
                cause=?6, participants_json=?7, outcome=?8, consequence=?9,
                reveal_at=?10, updated_at=?11
             WHERE id=?1 AND blueprint_id=?2",
            params![
                item.id,
                input.id,
                if item.order_no > 0 {
                    item.order_no
                } else {
                    (index + 1) as i32
                },
                item.title.trim(),
                item.story_time.trim(),
                item.cause.trim(),
                serde_json::to_string(&item.participants)?,
                item.outcome.trim(),
                item.consequence.trim(),
                item.reveal_at.trim(),
                ts
            ],
        )?;
    }
    for (index, item) in input.threads.iter().enumerate() {
        tx.execute(
            "UPDATE narrative_thread SET order_no=?3, kind=?4, title=?5,
                dramatic_question=?6, opening_state=?7, target_state=?8, stakes=?9,
                updated_at=?10
             WHERE id=?1 AND blueprint_id=?2",
            params![
                item.id,
                input.id,
                if item.order_no > 0 {
                    item.order_no
                } else {
                    (index + 1) as i32
                },
                item.kind.trim(),
                item.title.trim(),
                item.dramatic_question.trim(),
                item.opening_state.trim(),
                item.target_state.trim(),
                item.stakes.trim(),
                ts
            ],
        )?;
    }
    for (index, item) in input.nodes.iter().enumerate() {
        tx.execute(
            "UPDATE narrative_logic_node SET thread_id=?3, order_no=?4, kind=?5, title=?6,
                cause=?7, choice=?8, cost=?9, outcome=?10, state_change=?11,
                knowledge_change=?12, participants_json=?13, location=?14, story_time=?15,
                updated_at=?16
             WHERE id=?1 AND blueprint_id=?2",
            params![
                item.id,
                input.id,
                item.thread_id,
                if item.order_no > 0 {
                    item.order_no
                } else {
                    (index + 1) as i32
                },
                item.kind.trim(),
                item.title.trim(),
                item.cause.trim(),
                item.choice.trim(),
                item.cost.trim(),
                item.outcome.trim(),
                item.state_change.trim(),
                item.knowledge_change.trim(),
                serde_json::to_string(&item.participants)?,
                item.location.trim(),
                item.story_time.trim(),
                ts
            ],
        )?;
    }
    let node_ids: HashSet<&str> = input.nodes.iter().map(|node| node.id.as_str()).collect();
    let mut edge_pairs = HashSet::new();
    for (index, item) in input.edges.iter().enumerate() {
        if !node_ids.contains(item.from_node_id.as_str())
            || !node_ids.contains(item.to_node_id.as_str())
        {
            return Err(AppError::Message(format!(
                "第 {} 条因果边指向不存在的逻辑节点",
                index + 1
            )));
        }
        if item.from_node_id == item.to_node_id {
            return Err(AppError::Message("因果边不能从节点指向自身".into()));
        }
        if item.relation.trim().is_empty() {
            return Err(AppError::Message("因果边必须填写关系类型".into()));
        }
        if !edge_pairs.insert((item.from_node_id.as_str(), item.to_node_id.as_str())) {
            return Err(AppError::Message("同一对逻辑节点之间不能重复连边".into()));
        }
    }
    tx.execute(
        "DELETE FROM narrative_logic_edge WHERE blueprint_id=?1",
        [&input.id],
    )?;
    for item in &input.edges {
        let id = if item.id.trim().is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            item.id.clone()
        };
        tx.execute(
            "INSERT INTO narrative_logic_edge
             (id, blueprint_id, from_node_id, to_node_id, relation, condition,
              status, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,'candidate',?7)",
            params![
                id,
                input.id,
                item.from_node_id,
                item.to_node_id,
                item.relation.trim(),
                item.condition.trim(),
                ts
            ],
        )?;
    }
    for item in &input.presentations {
        tx.execute(
            "UPDATE narrative_presentation SET medium=?3, mode=?4, pov=?5,
                dialogue_ratio=?6, narration_ratio=?7, player_agency=?8,
                visual_requirements=?9, audio_requirements=?10, interaction=?11,
                deliverable=?12, updated_at=?13
             WHERE id=?1 AND blueprint_id=?2",
            params![
                item.id,
                input.id,
                item.medium.trim(),
                item.mode.trim(),
                item.pov.trim(),
                item.dialogue_ratio.clamp(0, 100),
                item.narration_ratio.clamp(0, 100),
                item.player_agency.trim(),
                item.visual_requirements.trim(),
                item.audio_requirements.trim(),
                item.interaction.trim(),
                item.deliverable.trim(),
                ts
            ],
        )?;
    }
    tx.commit()?;
    refresh_diagnostics(conn, project_id, &input.id)?;
    get_view(conn, project_id, &input.id)
}

#[cfg(test)]
mod edit_tests {
    use super::tests::{chain, foundation, seed_project};
    use super::*;
    use crate::domain::{NarrativeBlueprintEditInput, NarrativeGenerateInput};

    fn edit_input(view: &NarrativeBlueprintView) -> NarrativeBlueprintEditInput {
        NarrativeBlueprintEditInput {
            id: view.blueprint.id.clone(),
            title: view.blueprint.title.clone(),
            medium: view.blueprint.medium.clone(),
            structure_strategy: view.blueprint.structure_strategy.clone(),
            target_units: view.blueprint.target_units,
            core_dramatic_question: view.blueprint.core_dramatic_question.clone(),
            narrative_promise: view.blueprint.narrative_promise.clone(),
            opening_state: view.blueprint.opening_state.clone(),
            ending_state: view.blueprint.ending_state.clone(),
            theme_question: view.blueprint.theme_question.clone(),
            elements: view.elements.clone(),
            timeline: view.timeline.clone(),
            threads: view.threads.clone(),
            nodes: view.nodes.clone(),
            edges: view.edges.clone(),
            presentations: view.presentations.clone(),
        }
    }

    #[test]
    fn author_can_edit_candidate_and_diagnostics_refresh() {
        let (mut loaded, premise_id) = seed_project();
        let pid = loaded.summary.id.clone();
        let view = save_generated(
            &mut loaded.conn,
            &pid,
            &premise_id,
            &NarrativeGenerateInput {
                depth: None,
                medium: "novel".into(),
                structure_strategy: "adaptive".into(),
                target_units: None,
            },
            &foundation(),
            &chain(),
        )
        .unwrap();
        let mut input = edit_input(&view);
        input.title = "雨夜车站 · 作者修订".into();
        input.nodes[0].cost = "永远失去关于妹妹的一张照片".into();
        let updated = update_candidate(&mut loaded.conn, &pid, &input).unwrap();
        assert_eq!(updated.blueprint.title, "雨夜车站 · 作者修订");
        assert_eq!(updated.nodes[0].cost, "永远失去关于妹妹的一张照片");
        assert!(updated.blueprint.version > view.blueprint.version);
    }

    #[test]
    fn confirmed_blueprint_cannot_be_silently_edited() {
        let (mut loaded, premise_id) = seed_project();
        let pid = loaded.summary.id.clone();
        let view = save_generated(
            &mut loaded.conn,
            &pid,
            &premise_id,
            &NarrativeGenerateInput {
                depth: None,
                medium: "novel".into(),
                structure_strategy: "adaptive".into(),
                target_units: None,
            },
            &foundation(),
            &chain(),
        )
        .unwrap();
        confirm(&mut loaded.conn, &pid, &view.blueprint.id).unwrap();
        let mut input = edit_input(&view);
        input.title = "不应生效".into();
        assert!(update_candidate(&mut loaded.conn, &pid, &input).is_err());
    }

    #[test]
    fn chapter_context_uses_bound_logic_nodes() {
        let (mut loaded, premise_id) = seed_project();
        let pid = loaded.summary.id.clone();
        let view = save_generated(
            &mut loaded.conn,
            &pid,
            &premise_id,
            &NarrativeGenerateInput {
                depth: None,
                medium: "novel".into(),
                structure_strategy: "adaptive".into(),
                target_units: None,
            },
            &foundation(),
            &chain(),
        )
        .unwrap();
        confirm(&mut loaded.conn, &pid, &view.blueprint.id).unwrap();
        let plan = crate::outline::OutlinePlanDraft {
            mode: "chapters".into(),
            total_chapters: 1,
            volumes: vec![],
            chapters: vec![crate::outline::OutlineChapterDraft {
                number: 1,
                volume_index: None,
                title: "第一夜".into(),
                goal: "登车".into(),
                conflict: "车站阻止".into(),
                turn: "顾鸢出现".into(),
                payoff: "车票".into(),
                logic_node_ids: vec![view.nodes[0].id.clone(), view.nodes[1].id.clone()],
                presentation_mode: "prose".into(),
                narrative_function: "触发".into(),
                craft: Default::default(),
            }],
        };
        crate::outline::insert_plan(&mut loaded.conn, &pid, &plan, None).unwrap();
        let context = chapter_logic_context(&loaded.conn, &pid, 1).unwrap();
        assert!(context.contains("车票出现"));
        assert!(context.contains("站长拦截"));
        assert!(context.contains("本章必须承接的因果节点"));
    }

    #[test]
    fn author_can_add_and_delete_edges_with_immediate_diagnostics() {
        let (mut loaded, premise_id) = seed_project();
        let pid = loaded.summary.id.clone();
        let view = save_generated(
            &mut loaded.conn,
            &pid,
            &premise_id,
            &NarrativeGenerateInput {
                depth: None,
                medium: "novel".into(),
                structure_strategy: "adaptive".into(),
                target_units: None,
            },
            &foundation(),
            &chain(),
        )
        .unwrap();

        let mut input = edit_input(&view);
        input.edges.push(crate::domain::NarrativeLogicEdge {
            id: String::new(),
            from_node_id: view.nodes[0].id.clone(),
            from_title: view.nodes[0].title.clone(),
            to_node_id: view.nodes[2].id.clone(),
            to_title: view.nodes[2].title.clone(),
            relation: "旁证".into(),
            condition: "林澈先拿到空白车票".into(),
            status: "candidate".into(),
        });
        let added = update_candidate(&mut loaded.conn, &pid, &input).unwrap();
        assert_eq!(added.edges.len(), view.edges.len() + 1);
        assert!(added.edges.iter().any(|edge| {
            edge.from_node_id == view.nodes[0].id
                && edge.to_node_id == view.nodes[2].id
                && edge.relation == "旁证"
        }));

        let mut broken = edit_input(&added);
        broken.edges = broken.edges.into_iter().take(1).collect();
        let broken = update_candidate(&mut loaded.conn, &pid, &broken).unwrap();
        assert_eq!(broken.edges.len(), 1);
        assert!(broken.diagnostics.iter().any(|item| {
            item.severity == "error"
                && ["graph.roots", "graph.disconnected", "graph.unreachable"]
                    .contains(&item.code.as_str())
        }));
    }
}

#[cfg(test)]
mod patch_tests {
    use super::tests::{chain, foundation, seed_project};
    use super::*;
    use crate::domain::NarrativeGenerateInput;
    use crate::llm::ToolCall;

    #[test]
    fn agent_patch_applies_only_after_author_accepts_and_guards_stale_edits() {
        let (mut loaded, premise_id) = seed_project();
        let pid = loaded.summary.id.clone();
        let view = save_generated(
            &mut loaded.conn,
            &pid,
            &premise_id,
            &NarrativeGenerateInput {
                depth: None,
                medium: "novel".into(),
                structure_strategy: "adaptive".into(),
                target_units: None,
            },
            &foundation(),
            &chain(),
        )
        .unwrap();
        let session = crate::chat::create(
            &loaded.conn,
            &pid,
            &crate::domain::ChatSessionCreateInput {
                scope: "plot".into(),
                mode: "plot".into(),
                title: None,
                scene_id: None,
                branch_id: None,
                parent_session_id: None,
            },
        )
        .unwrap();
        let run_id =
            crate::harness::create_run(&loaded.conn, &pid, &session.id, "修订", "write", "m")
                .unwrap();
        let call = ToolCall {
            id: "c".into(),
            name: "propose_blueprint_patch".into(),
            arguments: json!({
                "reason": "年代自洽",
                "changes": [
                    {"target":"element","name":"林澈","field":"secret","value":"他在封箱夜亲手抹去了妹妹的车票"},
                    {"target":"node","order":1,"field":"cost","value":"他忘了自己为何上车"},
                    {"target":"element","name":"不存在","field":"secret","value":"x"}
                ]
            })
            .to_string(),
        };
        let (step_id, _) = crate::harness::begin_step(&loaded.conn, &run_id, &call).unwrap();
        let outcome = crate::harness::execute(
            &crate::harness::ToolContext {
                conn: &loaded.conn,
                project_id: &pid,
                session_id: &session.id,
                run_id: &run_id,
                step_id: &step_id,
                mode: "write",
            },
            &call,
        );
        assert_eq!(outcome.candidate_ids.len(), 1, "{}", outcome.content);
        assert!(outcome.content.contains("找不到条目「不存在」"));
        // 未确认前蓝本不变。
        let unchanged = candidate(&loaded.conn, &pid).unwrap().unwrap();
        let hero = unchanged
            .elements
            .iter()
            .find(|e| e.name == "林澈")
            .unwrap();
        assert_eq!(hero.secret, "他曾主动抹去一段记忆");

        crate::candidates::accept(&mut loaded.conn, &pid, &outcome.candidate_ids[0]).unwrap();
        let patched = candidate(&loaded.conn, &pid).unwrap().unwrap();
        let hero = patched.elements.iter().find(|e| e.name == "林澈").unwrap();
        assert_eq!(hero.secret, "他在封箱夜亲手抹去了妹妹的车票");
        assert_eq!(patched.nodes[0].cost, "他忘了自己为何上车");
        assert_eq!(view.blueprint.id, patched.blueprint.id);
    }
}
