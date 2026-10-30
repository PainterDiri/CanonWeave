use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::{json, Value};

use crate::{
    domain::{
        AgentEvidence, AgentNextAction, AgentRun, AgentRunCreateInput, AgentRunDetail, AgentStep,
        AgentToolDefinition, FoundationReport,
    },
    error::{AppError, AppResult},
    now,
};

const RUN_SELECT: &str = "SELECT r.id, r.session_id, r.mode, r.scope, r.output, r.goal,
    r.status, r.plan_json, r.current_step, r.approval_policy, r.model, r.context_hash,
    r.error, r.started_at, r.finished_at, r.created_at, r.updated_at,
    (SELECT COUNT(*) FROM candidate c WHERE c.agent_run_id=r.id AND c.status='pending'),
    (SELECT COUNT(*) FROM candidate c WHERE c.agent_run_id=r.id)
    FROM agent_run r";

const STEP_SELECT: &str = "SELECT id, run_id, seq, kind, tool_name, input_json, output_json,
    status, risk_level, requires_approval, candidate_ids, evidence_json, error,
    started_at, finished_at FROM agent_step";

fn tool(
    name: &str,
    description: &str,
    schema: Value,
    access: &str,
    risk_level: &str,
    requires_approval: bool,
    retryable: bool,
    output_target: &str,
) -> AgentToolDefinition {
    AgentToolDefinition {
        name: name.into(),
        description: description.into(),
        parameter_schema: schema,
        access: access.into(),
        risk_level: risk_level.into(),
        requires_approval,
        retryable,
        output_target: output_target.into(),
    }
}

pub fn tools() -> Vec<AgentToolDefinition> {
    let empty = json!({"type":"object","properties":{},"additionalProperties":false});
    vec![
        tool(
            "project_inventory",
            "读取项目结构、人物、知识、伏笔、章节与可回查原文片段。",
            empty.clone(),
            "read",
            "L0",
            false,
            true,
            "context",
        ),
        tool(
            "search_fts",
            "用任务目标检索现有正文、导入稿和知识条目。",
            json!({
                "type":"object",
                "properties":{"query":{"type":"string"},"limit":{"type":"integer"}},
                "required":["query"]
            }),
            "read",
            "L0",
            false,
            true,
            "context",
        ),
        tool(
            "search_hybrid",
            "同时使用 FTS、实体图谱与可用向量召回项目证据。",
            json!({
                "type":"object",
                "properties":{"query":{"type":"string"},"limit":{"type":"integer"}},
                "required":["query"]
            }),
            "read",
            "L0",
            false,
            true,
            "context",
        ),
        tool(
            "get_premise",
            "读取当前种子卡与作者已确认的创作契约。",
            empty.clone(),
            "read",
            "L0",
            false,
            true,
            "context",
        ),
        tool(
            "get_blueprint",
            "读取当前已确认或待审的叙事蓝本、地基、时间线与逻辑链。",
            empty.clone(),
            "read",
            "L0",
            false,
            true,
            "context",
        ),
        tool(
            "get_outline_plan",
            "读取卷章骨架、逻辑节点绑定和确认状态。",
            empty.clone(),
            "read",
            "L0",
            false,
            true,
            "context",
        ),
        tool(
            "get_style_profiles",
            "读取项目文笔档案与作用域。",
            empty.clone(),
            "read",
            "L0",
            false,
            true,
            "context",
        ),
        tool(
            "recommend_next_actions",
            "根据灵感、地基、蓝本、卷章、文笔和正文状态计算下一步建议。",
            empty.clone(),
            "read",
            "L0",
            false,
            true,
            "context",
        ),
        tool(
            "graph_ppr",
            "从种子实体出发运行 Personalized PageRank，召回多跳关系。",
            json!({
                "type":"object",
                "properties":{"seeds":{"type":"array","items":{"type":"string"}}},
                "required":["seeds"]
            }),
            "read",
            "L0",
            false,
            true,
            "context",
        ),
        tool(
            "get_entity",
            "按实体 ID 读取人物、地点、物品或知识实体。",
            json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}),
            "read",
            "L0",
            false,
            true,
            "context",
        ),
        tool(
            "get_relationships",
            "读取指定实体的一跳关系。",
            json!({"type":"object","properties":{"entityId":{"type":"string"}},"required":["entityId"]}),
            "read",
            "L0",
            false,
            true,
            "context",
        ),
        tool(
            "get_timeline",
            "读取当前项目的双时间线事件。",
            empty.clone(),
            "read",
            "L0",
            false,
            true,
            "context",
        ),
        tool(
            "get_open_promises",
            "读取尚未兑现或放弃的伏笔。",
            empty.clone(),
            "read",
            "L0",
            false,
            true,
            "context",
        ),
        tool(
            "audit_consistency",
            "运行现有确定性一致性巡检；问题写入派生审计队列。",
            empty.clone(),
            "read",
            "L1",
            false,
            true,
            "issues",
        ),
        tool(
            "impact_analysis",
            "计算实体变更会影响的事实、关系、事件、场景和伏笔。",
            json!({"type":"object","properties":{"entityId":{"type":"string"}},"required":["entityId"]}),
            "read",
            "L0",
            false,
            true,
            "context",
        ),
        tool(
            "wiki_index",
            "读取 Narrative Wiki 导航索引和来源 ID。",
            empty.clone(),
            "read",
            "L0",
            false,
            true,
            "context",
        ),
        tool(
            "read_knowledge_entry",
            "按知识条目 ID 读取作者确认内容。",
            json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}),
            "read",
            "L0",
            false,
            true,
            "context",
        ),
        tool(
            "generate_premise_directions",
            "根据作者的一句子或修改意见生成三套差异化的灵感方向候选。",
            json!({"type":"object","properties":{"instruction":{"type":"string"}},"required":["instruction"]}),
            "write",
            "L1",
            true,
            true,
            "candidate",
        ),
        tool(
            "generate_narrative_blueprint",
            "从已确认种子卡生成叙事地基、因果链和人物/势力/世界规则候选。",
            json!({"type":"object","properties":{"medium":{"type":"string"},"structureStrategy":{"type":"string"},"targetUnits":{"type":"integer"}}}),
            "write",
            "L2",
            true,
            true,
            "candidate",
        ),
        tool(
            "generate_outline_plan",
            "从已确认蓝本生成卷章骨架候选，可按自适应或作者指定结构规划。",
            json!({"type":"object","properties":{"mode":{"type":"string"},"totalChapters":{"type":"integer"}}}),
            "write",
            "L2",
            true,
            true,
            "candidate",
        ),
        tool(
            "propose_style_profile",
            "根据故事气质和作者要求推荐文笔预设，并生成待确认的文笔档案候选。",
            json!({"type":"object","properties":{"instruction":{"type":"string"}},"required":["instruction"]}),
            "write",
            "L2",
            true,
            true,
            "candidate",
        ),
        tool(
            "rebuild_wiki",
            "重建 Narrative Wiki 派生页面，保留作者修订并刷新页面依赖。",
            empty.clone(),
            "read",
            "L0",
            false,
            true,
            "context",
        ),
        tool(
            "build_community_reports",
            "增量重建分层 GraphRAG 社区报告。",
            empty.clone(),
            "read",
            "L0",
            false,
            true,
            "context",
        ),
        tool(
            "propose_entity",
            "提出人物、势力、地点、物品或概念候选；不能直接写入权威实体。",
            json!({"type":"object","properties":{"canonicalName":{"type":"string"},"type":{"type":"string"},"sourceSpan":{"type":"string"},"quote":{"type":"string"}},"required":["canonicalName","type","sourceSpan","quote"]}),
            "write",
            "L1",
            true,
            true,
            "candidate",
        ),
        tool(
            "propose_relationship",
            "提出实体关系候选；不能直接写入权威关系。",
            json!({"type":"object","properties":{"from":{"type":"string"},"to":{"type":"string"},"type":{"type":"string"},"sourceSpan":{"type":"string"},"quote":{"type":"string"}},"required":["from","to","type","sourceSpan","quote"]}),
            "write",
            "L2",
            true,
            true,
            "candidate",
        ),
        tool(
            "propose_claim",
            "提出带真值层的事实候选；不能直接覆盖权威事实。",
            json!({"type":"object","properties":{"subject":{"type":"string"},"predicate":{"type":"string"},"object":{"type":"string"},"truthLayer":{"type":"string"},"sourceSpan":{"type":"string"},"quote":{"type":"string"}},"required":["subject","predicate","truthLayer","sourceSpan","quote"]}),
            "write",
            "L2",
            true,
            true,
            "candidate",
        ),
        tool(
            "propose_timeline_event",
            "提出时间线事件候选；不能直接写入权威事件。",
            json!({"type":"object","properties":{"title":{"type":"string"},"storyTime":{"type":"string"},"sourceSpan":{"type":"string"},"quote":{"type":"string"}},"required":["title","storyTime","sourceSpan","quote"]}),
            "write",
            "L2",
            true,
            true,
            "candidate",
        ),
        tool(
            "propose_promise",
            "提出伏笔候选及其兑现契约。",
            json!({"type":"object","properties":{"title":{"type":"string"},"window":{"type":"string"},"contract":{"type":"string"},"sourceSpan":{"type":"string"},"quote":{"type":"string"}},"required":["title","contract","sourceSpan","quote"]}),
            "write",
            "L2",
            true,
            true,
            "candidate",
        ),
        tool(
            "propose_knowledge_entry",
            "提出知识条目候选，确认后才会进入知识库、图谱和 FTS。",
            json!({"type":"object","properties":{"domainId":{"type":"string"},"domainKey":{"type":"string"},"name":{"type":"string"},"summary":{"type":"string"},"sourceSpan":{"type":"string"},"quote":{"type":"string"}},"required":["name","sourceSpan","quote"]}),
            "write",
            "L2",
            true,
            true,
            "candidate",
        ),
        tool(
            "propose_outline_patch",
            "提出卷或章节骨架补丁，与知识条目可在同一审批组审阅。",
            json!({"type":"object","properties":{"level":{"type":"string"},"orderNo":{"type":"integer"},"title":{"type":"string"},"sourceSpan":{"type":"string"},"quote":{"type":"string"}},"required":["level","title","sourceSpan","quote"]}),
            "write",
            "L2",
            true,
            true,
            "candidate",
        ),
        tool(
            "propose_wiki_page",
            "提出 Wiki 派生页修改候选；不得成为权威事实。",
            json!({"type":"object","properties":{"title":{"type":"string"},"content":{"type":"string"},"sourceSpan":{"type":"string"},"quote":{"type":"string"}},"required":["title","content","sourceSpan","quote"]}),
            "write",
            "L2",
            true,
            true,
            "candidate",
        ),
    ]
}

fn run_from_row(row: &Row<'_>) -> rusqlite::Result<AgentRun> {
    let plan: String = row.get(7)?;
    Ok(AgentRun {
        id: row.get(0)?,
        session_id: row.get(1)?,
        mode: row.get(2)?,
        scope: row.get(3)?,
        output: row.get(4)?,
        goal: row.get(5)?,
        status: row.get(6)?,
        plan: serde_json::from_str(&plan).unwrap_or_else(|_| json!([])),
        current_step: row.get(8)?,
        approval_policy: row.get(9)?,
        model: row.get(10)?,
        context_hash: row.get(11)?,
        error: row.get(12)?,
        started_at: row.get(13)?,
        finished_at: row.get(14)?,
        created_at: row.get(15)?,
        updated_at: row.get(16)?,
        pending_candidates: row.get(17)?,
        total_candidates: row.get(18)?,
    })
}

fn step_from_row(row: &Row<'_>) -> rusqlite::Result<AgentStep> {
    let input: String = row.get(5)?;
    let output: String = row.get(6)?;
    let candidate_ids: String = row.get(10)?;
    let evidence: String = row.get(11)?;
    Ok(AgentStep {
        id: row.get(0)?,
        run_id: row.get(1)?,
        seq: row.get(2)?,
        kind: row.get(3)?,
        tool_name: row.get(4)?,
        input: serde_json::from_str(&input).unwrap_or_else(|_| json!({})),
        output: serde_json::from_str(&output).unwrap_or_else(|_| json!({})),
        status: row.get(7)?,
        risk_level: row.get(8)?,
        requires_approval: row.get::<_, i32>(9)? == 1,
        candidate_ids: serde_json::from_str(&candidate_ids).unwrap_or_default(),
        evidence: serde_json::from_str(&evidence).unwrap_or_else(|_| json!([])),
        error: row.get(12)?,
        started_at: row.get(13)?,
        finished_at: row.get(14)?,
    })
}

fn evidence_from_row(row: &Row<'_>) -> rusqlite::Result<AgentEvidence> {
    Ok(AgentEvidence {
        id: row.get(0)?,
        run_id: row.get(1)?,
        step_id: row.get(2)?,
        source_kind: row.get(3)?,
        source_id: row.get(4)?,
        source_span: row.get(5)?,
        quote: row.get(6)?,
        created_at: row.get(7)?,
    })
}

pub fn run(conn: &Connection, project_id: &str, id: &str) -> AppResult<AgentRun> {
    conn.query_row(
        &format!("{RUN_SELECT} WHERE r.project_id=?1 AND r.id=?2"),
        params![project_id, id],
        run_from_row,
    )
    .optional()?
    .ok_or_else(|| AppError::Message("找不到 Agent Run".into()))
}

pub fn steps(conn: &Connection, run_id: &str) -> AppResult<Vec<AgentStep>> {
    let mut stmt = conn.prepare(&format!("{STEP_SELECT} WHERE run_id=?1 ORDER BY seq"))?;
    let rows = stmt
        .query_map([run_id], step_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn evidence(conn: &Connection, run_id: &str) -> AppResult<Vec<AgentEvidence>> {
    let mut stmt = conn.prepare(
        "SELECT id, run_id, step_id, source_kind, source_id, source_span, quote, created_at
         FROM agent_evidence WHERE run_id=?1 ORDER BY created_at, id",
    )?;
    let rows = stmt
        .query_map([run_id], evidence_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn detail(conn: &Connection, project_id: &str, id: &str) -> AppResult<AgentRunDetail> {
    let run = run(conn, project_id, id)?;
    Ok(AgentRunDetail {
        steps: steps(conn, id)?,
        candidates: crate::candidates::list_for_run(conn, project_id, id)?,
        evidence: evidence(conn, id)?,
        run,
    })
}

pub fn list(conn: &Connection, project_id: &str, limit: usize) -> AppResult<Vec<AgentRun>> {
    let mut stmt = conn.prepare(&format!(
        "{RUN_SELECT} WHERE r.project_id=?1 AND r.mode != 'harness'
         ORDER BY CASE r.status
           WHEN 'running' THEN 0 WHEN 'planning' THEN 1 WHEN 'waiting_approval' THEN 2
           WHEN 'paused' THEN 3 WHEN 'failed' THEN 4 ELSE 5 END,
           r.updated_at DESC LIMIT ?2"
    ))?;
    let rows = stmt
        .query_map(params![project_id, limit as i32], run_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn mark_interrupted(conn: &Connection, project_id: &str, id: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE agent_run
         SET status='paused', error='应用关闭或进程中断后已安全暂停；可从最近步骤恢复',
             updated_at=?3
         WHERE project_id=?1 AND id=?2 AND status IN ('planning','running')",
        params![project_id, id, now()],
    )?;
    Ok(())
}

fn plan_item(
    seq: i32,
    kind: &str,
    tool_name: &str,
    title: &str,
    risk_level: &str,
    requires_approval: bool,
) -> Value {
    json!({
        "seq": seq,
        "kind": kind,
        "toolName": tool_name,
        "title": title,
        "status": "pending",
        "riskLevel": risk_level,
        "requiresApproval": requires_approval
    })
}

pub fn create(
    conn: &mut Connection,
    project_id: &str,
    input: &AgentRunCreateInput,
) -> AppResult<AgentRun> {
    let goal = input.goal.trim();
    if goal.is_empty() {
        return Err(AppError::Message("Agent 任务目标不能为空".into()));
    }
    if !["analyze", "propose"].contains(&input.mode.as_str()) {
        return Err(AppError::Message(
            "任务模式必须是 analyze 或 propose".into(),
        ));
    }
    if !["project", "volume", "chapter", "library"].contains(&input.scope.as_str()) {
        return Err(AppError::Message("未知的任务范围".into()));
    }
    if ![
        "all",
        "characters",
        "timeline",
        "promises",
        "knowledge",
        "outline",
    ]
    .contains(&input.output.as_str())
    {
        return Err(AppError::Message("未知的期望输出".into()));
    }
    if let Some(session_id) = input.session_id.as_deref() {
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM chat_session WHERE project_id=?1 AND id=?2)",
            params![project_id, session_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(AppError::Message("关联的会话不存在".into()));
        }
    }

    let id = uuid::Uuid::new_v4().to_string();
    let ts = now();
    let plan = if input.mode == "analyze" {
        vec![
            plan_item(
                1,
                "tool",
                "project_inventory",
                "读取项目清单与原文证据",
                "L0",
                false,
            ),
            plan_item(
                2,
                "tool",
                "search_fts",
                "检索与目标相关的正文和资料",
                "L0",
                false,
            ),
            plan_item(
                3,
                "synthesis",
                "model_synthesis",
                "归纳可核实的项目现状",
                "L0",
                false,
            ),
            plan_item(
                4,
                "check",
                "candidate_impact",
                "检查候选与权威资料的冲突",
                "L0",
                false,
            ),
            plan_item(
                5,
                "check",
                "foundation_check",
                "检查叙事地基与影响",
                "L0",
                false,
            ),
        ]
    } else {
        vec![
            plan_item(
                1,
                "tool",
                "project_inventory",
                "读取项目清单与原文证据",
                "L0",
                false,
            ),
            plan_item(
                2,
                "tool",
                "search_fts",
                "检索与目标相关的正文和资料",
                "L0",
                false,
            ),
            plan_item(
                3,
                "synthesis",
                "model_synthesis",
                "去重并生成候选变更组",
                "L2",
                true,
            ),
            plan_item(
                4,
                "check",
                "candidate_impact",
                "检查候选冲突与影响范围",
                "L0",
                false,
            ),
            plan_item(
                5,
                "check",
                "foundation_check",
                "检查叙事地基与候选影响",
                "L0",
                false,
            ),
        ]
    };
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO agent_run
         (id, project_id, session_id, mode, scope, output, goal, status, plan_json,
          current_step, approval_policy, created_at, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,'draft',?8,0,'manual',?9,?9)",
        params![
            id,
            project_id,
            input.session_id,
            input.mode,
            input.scope,
            input.output,
            goal,
            serde_json::to_string(&plan)?,
            ts
        ],
    )?;
    for item in plan {
        let seq = item.get("seq").and_then(Value::as_i64).unwrap_or(0) as i32;
        tx.execute(
            "INSERT INTO agent_step
             (id, run_id, seq, kind, tool_name, input_json, status, risk_level,
              requires_approval)
             VALUES (?1,?2,?3,?4,?5,'{}','pending',?6,?7)",
            params![
                uuid::Uuid::new_v4().to_string(),
                id,
                seq,
                item.get("kind").and_then(Value::as_str).unwrap_or("tool"),
                item.get("toolName")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                item.get("riskLevel")
                    .and_then(Value::as_str)
                    .unwrap_or("L0"),
                i32::from(
                    item.get("requiresApproval")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                )
            ],
        )?;
    }
    tx.execute(
        "INSERT INTO audit_log (id, project_id, action, subject, detail, created_at)
         VALUES (?1,?2,'agent_run_create',?3,?4,?5)",
        params![
            uuid::Uuid::new_v4().to_string(),
            project_id,
            id,
            json!({"goal":goal,"mode":input.mode,"scope":input.scope,"output":input.output})
                .to_string(),
            ts
        ],
    )?;
    tx.commit()?;
    run(conn, project_id, &id)
}

pub fn start(conn: &Connection, project_id: &str, id: &str) -> AppResult<AgentRun> {
    let current = run(conn, project_id, id)?;
    if !["draft", "planning", "running"].contains(&current.status.as_str()) {
        return Err(AppError::Message(format!(
            "当前状态 {} 不能开始执行",
            current.status
        )));
    }
    let ts = now();
    conn.execute(
        "UPDATE agent_run SET status='running', error='', started_at=COALESCE(started_at,?3),
         updated_at=?3 WHERE project_id=?1 AND id=?2",
        params![project_id, id, ts],
    )?;
    sync_plan(conn, id)?;
    run(conn, project_id, id)
}

pub fn pause(conn: &Connection, project_id: &str, id: &str) -> AppResult<AgentRun> {
    let current = run(conn, project_id, id)?;
    if !["planning", "running"].contains(&current.status.as_str()) {
        return Err(AppError::Message(
            "只有正在运行的 Agent 任务可以暂停".into(),
        ));
    }
    conn.execute(
        "UPDATE agent_run SET status='paused', updated_at=?3 WHERE project_id=?1 AND id=?2",
        params![project_id, id, now()],
    )?;
    sync_plan(conn, id)?;
    run(conn, project_id, id)
}

pub fn cancel(conn: &Connection, project_id: &str, id: &str) -> AppResult<AgentRun> {
    let current = run(conn, project_id, id)?;
    if ["completed", "canceled"].contains(&current.status.as_str()) {
        return Ok(current);
    }
    let ts = now();
    conn.execute(
        "UPDATE agent_run SET status='canceled', finished_at=?3, updated_at=?3
         WHERE project_id=?1 AND id=?2",
        params![project_id, id, ts],
    )?;
    conn.execute(
        "UPDATE agent_step SET status='canceled', finished_at=?2
         WHERE run_id=?1 AND status IN ('pending','running')",
        params![id, ts],
    )?;
    sync_plan(conn, id)?;
    run(conn, project_id, id)
}

pub fn resume(conn: &Connection, project_id: &str, id: &str) -> AppResult<AgentRun> {
    let current = run(conn, project_id, id)?;
    if !["paused", "failed"].contains(&current.status.as_str()) {
        return Err(AppError::Message("当前任务不需要恢复".into()));
    }
    let ts = now();
    conn.execute(
        "UPDATE agent_step SET status='pending', error='', started_at=NULL, finished_at=NULL
         WHERE run_id=?1 AND status='failed'",
        [id],
    )?;
    conn.execute(
        "UPDATE agent_run SET status='running', error='', finished_at=NULL,
         started_at=COALESCE(started_at,?3), updated_at=?3
         WHERE project_id=?1 AND id=?2",
        params![project_id, id, ts],
    )?;
    sync_plan(conn, id)?;
    run(conn, project_id, id)
}

pub fn retry_failed(
    conn: &Connection,
    project_id: &str,
    id: &str,
    step_id: &str,
) -> AppResult<AgentRun> {
    let status: Option<String> = conn
        .query_row(
            "SELECT status FROM agent_step WHERE run_id=?1 AND id=?2",
            params![id, step_id],
            |row| row.get(0),
        )
        .optional()?;
    let status = status.ok_or_else(|| AppError::Message("找不到失败步骤".into()))?;
    if status != "failed" {
        return Err(AppError::Message("只能重试失败步骤".into()));
    }
    conn.execute(
        "UPDATE agent_step SET status='pending', error='', started_at=NULL, finished_at=NULL
         WHERE run_id=?1 AND id=?2",
        params![id, step_id],
    )?;
    conn.execute(
        "UPDATE agent_run SET status='running', error='', finished_at=NULL, updated_at=?3
         WHERE project_id=?1 AND id=?2",
        params![project_id, id, now()],
    )?;
    sync_plan(conn, id)?;
    run(conn, project_id, id)
}

fn sync_plan(conn: &Connection, run_id: &str) -> AppResult<()> {
    let steps = steps(conn, run_id)?;
    let plan: Vec<Value> = steps
        .iter()
        .map(|step| {
            json!({
                "seq": step.seq,
                "kind": step.kind,
                "toolName": step.tool_name,
                "title": plan_title(step.seq, &step.tool_name),
                "status": step.status,
                "riskLevel": step.risk_level,
                "requiresApproval": step.requires_approval
            })
        })
        .collect();
    conn.execute(
        "UPDATE agent_run SET plan_json=?2, current_step=?3, updated_at=?4 WHERE id=?1",
        params![
            run_id,
            serde_json::to_string(&plan)?,
            steps
                .iter()
                .filter(|step| step.status == "completed")
                .map(|step| step.seq)
                .max()
                .unwrap_or(0),
            now()
        ],
    )?;
    Ok(())
}

fn plan_title(seq: i32, tool_name: &str) -> String {
    match tool_name {
        "project_inventory" => "读取项目清单与原文证据",
        "search_fts" => "检索相关正文与资料",
        "model_synthesis" => "生成结构化观察与候选",
        "candidate_impact" => "检查候选冲突与影响范围",
        "foundation_check" => "检查叙事地基与影响",
        _ => match seq {
            1 => "准备任务",
            _ => "执行 Agent 步骤",
        },
    }
    .into()
}

fn mark_step_running(conn: &Connection, step: &AgentStep) -> AppResult<()> {
    conn.execute(
        "UPDATE agent_step SET status='running', started_at=?2, finished_at=NULL, error=''
         WHERE id=?1",
        params![step.id, now()],
    )?;
    Ok(())
}

fn mark_step_completed(conn: &Connection, step: &AgentStep, output: Value) -> AppResult<()> {
    conn.execute(
        "UPDATE agent_step SET status='completed', output_json=?2, finished_at=?3
         WHERE id=?1",
        params![step.id, output.to_string(), now()],
    )?;
    Ok(())
}

fn mark_step_skipped(conn: &Connection, step: &AgentStep, output: Value) -> AppResult<()> {
    conn.execute(
        "UPDATE agent_step SET status='skipped', output_json=?2, finished_at=?3
         WHERE id=?1",
        params![step.id, output.to_string(), now()],
    )?;
    Ok(())
}

pub fn fail_step(conn: &Connection, run_id: &str, project_id: &str, error: &str) -> AppResult<()> {
    let ts = now();
    let changed = conn.execute(
        "UPDATE agent_step SET status='failed', error=?2, finished_at=?3
         WHERE run_id=?1 AND status='running'",
        params![run_id, error, ts],
    )?;
    if changed == 0 {
        conn.execute(
            "UPDATE agent_step SET status='failed', error=?2, finished_at=?3
             WHERE id=(
               SELECT id FROM agent_step WHERE run_id=?1 AND status='pending'
               ORDER BY seq LIMIT 1
             )",
            params![run_id, error, ts],
        )?;
    }
    conn.execute(
        "UPDATE agent_run SET status='failed', error=?3, finished_at=?4, updated_at=?4
         WHERE project_id=?1 AND id=?2",
        params![project_id, run_id, error, ts],
    )?;
    sync_plan(conn, run_id)?;
    Ok(())
}

fn truncate(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

fn project_inventory(conn: &Connection, project_id: &str) -> AppResult<Value> {
    let project = conn.query_row(
        "SELECT name, mode, writing_language FROM project WHERE id=?1",
        [project_id],
        |row| {
            Ok(json!({
                "name": row.get::<_, String>(0)?,
                "mode": row.get::<_, String>(1)?,
                "writingLanguage": row.get::<_, String>(2)?
            }))
        },
    )?;
    let counts = crate::project::counts(conn, project_id)?;
    let premise = crate::seed::active(conn, project_id)?
        .map(|value| serde_json::to_value(value))
        .transpose()?;
    let blueprint = match crate::blueprint::current(conn, project_id)? {
        Some(view) => Some(view),
        None => crate::blueprint::candidate(conn, project_id)?,
    }
    .map(|view| {
        json!({
            "id": view.blueprint.id,
            "status": view.blueprint.status,
            "title": view.blueprint.title,
            "coreDramaticQuestion": view.blueprint.core_dramatic_question,
            "openingState": view.blueprint.opening_state,
            "endingState": view.blueprint.ending_state,
            "elements": view.elements.into_iter().map(|element| json!({
                "kind": element.kind,
                "name": element.name,
                "role": element.role,
                "summary": element.summary,
                "constraints": element.constraints,
                "relations": element.relations
            })).collect::<Vec<_>>(),
            "timeline": view.timeline.into_iter().map(|event| json!({
                "title": event.title,
                "storyTime": event.story_time,
                "participants": event.participants,
                "cause": event.cause,
                "outcome": event.outcome,
                "consequence": event.consequence
            })).collect::<Vec<_>>()
        })
    });

    let mut entities = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, type, canonical_name, aliases, description FROM entity
             WHERE project_id=?1 AND status='confirmed'
             ORDER BY updated_at DESC LIMIT 80",
        )?;
        let rows = stmt.query_map([project_id], |row| {
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "type": row.get::<_, String>(1)?,
                "name": row.get::<_, String>(2)?,
                "aliases": serde_json::from_str::<Value>(&row.get::<_, String>(3)?).unwrap_or_else(|_| json!([])),
                "description": row.get::<_, String>(4)?
            }))
        })?;
        for row in rows {
            entities.push(row?);
        }
    }
    let mut knowledge = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT d.id, d.key, d.name, e.id, e.name, e.summary, e.fields
             FROM knowledge_entry e JOIN knowledge_domain d ON d.id=e.domain_id
             WHERE e.project_id=?1 AND e.status='confirmed'
             ORDER BY e.updated_at DESC LIMIT 60",
        )?;
        let rows = stmt.query_map([project_id], |row| {
            Ok(json!({
                "domainId": row.get::<_, String>(0)?,
                "domainKey": row.get::<_, String>(1)?,
                "domainName": row.get::<_, String>(2)?,
                "id": row.get::<_, String>(3)?,
                "name": row.get::<_, String>(4)?,
                "summary": row.get::<_, String>(5)?,
                "fields": serde_json::from_str::<Value>(&row.get::<_, String>(6)?).unwrap_or_else(|_| json!({}))
            }))
        })?;
        for row in rows {
            knowledge.push(row?);
        }
    }
    let mut promises = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, title, state, target_window, payoff_contract FROM promise
             WHERE project_id=?1 AND state NOT IN ('paid_off','abandoned')
             ORDER BY updated_at DESC LIMIT 40",
        )?;
        let rows = stmt.query_map([project_id], |row| {
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "title": row.get::<_, String>(1)?,
                "state": row.get::<_, String>(2)?,
                "targetWindow": row.get::<_, String>(3)?,
                "contract": row.get::<_, String>(4)?
            }))
        })?;
        for row in rows {
            promises.push(row?);
        }
    }
    let mut chapters = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, number, title, status, current_revision_id FROM chapter
             WHERE project_id=?1 ORDER BY number LIMIT 120",
        )?;
        let rows = stmt.query_map([project_id], |row| {
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "number": row.get::<_, i32>(1)?,
                "title": row.get::<_, String>(2)?,
                "status": row.get::<_, String>(3)?,
                "hasRevision": row.get::<_, Option<String>>(4)?.is_some()
            }))
        })?;
        for row in rows {
            chapters.push(row?);
        }
    }

    let mut evidence_sources = Vec::new();
    let mut seen_sources = HashSet::new();
    if let Some(premise) = &premise {
        let quote = truncate(
            &format!(
                "{}｜{}｜{}",
                premise["logline"].as_str().unwrap_or_default(),
                premise["coreConflict"].as_str().unwrap_or_default(),
                premise["dramaticQuestion"].as_str().unwrap_or_default()
            ),
            700,
        )
        .trim_matches('｜')
        .to_string();
        if !quote.is_empty() {
            evidence_sources.push(json!({
                "sourceKind":"premise",
                "sourceId":premise["id"],
                "sourceSpan":format!("premise:{}", premise["id"].as_str().unwrap_or_default()),
                "quote":quote
            }));
        }
    }
    for entity in &entities {
        let quote = entity["description"].as_str().unwrap_or_default().trim();
        if quote.is_empty() {
            continue;
        }
        evidence_sources.push(json!({
            "sourceKind":"entity",
            "sourceId":entity["id"],
            "sourceSpan":format!("entity:{}", entity["id"].as_str().unwrap_or_default()),
            "quote":truncate(quote, 700)
        }));
    }
    for entry in &knowledge {
        let quote = format!(
            "{} {}",
            entry["summary"].as_str().unwrap_or_default(),
            entry["fields"]
        );
        evidence_sources.push(json!({
            "sourceKind":"knowledge",
            "sourceId":entry["id"],
            "sourceSpan":format!("knowledge:{}", entry["id"].as_str().unwrap_or_default()),
            "quote":truncate(quote.trim(), 700)
        }));
    }
    {
        let mut stmt = conn.prepare(
            "SELECT source_id, source_kind, chapter_id, content FROM text_fts
             WHERE project_id=?1 ORDER BY rowid DESC LIMIT 80",
        )?;
        let rows = stmt.query_map([project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        for row in rows {
            let (source_id, source_kind, chapter_id, content) = row?;
            if !seen_sources.insert((source_kind.clone(), source_id.clone())) {
                continue;
            }
            evidence_sources.push(json!({
                "sourceKind":source_kind,
                "sourceId":source_id,
                "chapterId":chapter_id,
                "sourceSpan":format!("{source_kind}:{source_id}"),
                "quote":truncate(content.trim(), 700)
            }));
            if evidence_sources.len() >= 60 {
                break;
            }
        }
    }

    Ok(json!({
        "project": project,
        "counts": counts,
        "premise": premise,
        "blueprint": blueprint,
        "entities": entities,
        "knowledge": knowledge,
        "promises": promises,
        "chapters": chapters,
        "evidenceSources": evidence_sources
    }))
}

fn search_tool(conn: &Connection, project_id: &str, input: &Value, goal: &str) -> AppResult<Value> {
    let query = input
        .get("query")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(goal);
    let limit = input
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(20)
        .clamp(1, 50) as usize;
    let hits = crate::retrieval::search_for_scene(conn, project_id, query, limit, None)?;
    let evidence_sources: Vec<Value> = hits
        .iter()
        .map(|hit| {
            json!({
                "sourceKind":"fts",
                "sourceId":hit.source_id,
                "chapterId":hit.chapter_id,
                "sourceSpan":format!("fts:{}", hit.source_id),
                "quote":hit.excerpt,
                "score":hit.score
            })
        })
        .collect();
    Ok(json!({
        "query": query,
        "hits": hits,
        "evidenceSources": evidence_sources
    }))
}

fn candidate_impact(conn: &Connection, project_id: &str, run_id: &str) -> AppResult<Value> {
    let candidates = crate::candidates::list_for_run(conn, project_id, run_id)?;
    let pending_entities: HashSet<String> = candidates
        .iter()
        .filter(|candidate| candidate.kind == "entity")
        .filter_map(|candidate| {
            candidate
                .payload
                .get("canonicalName")
                .and_then(Value::as_str)
                .map(|value| value.trim().to_string())
        })
        .collect();
    let mut items = Vec::new();
    for candidate in &candidates {
        let payload = &candidate.payload;
        let title = proposal_title(&candidate.kind, payload);
        match candidate.kind.as_str() {
            "claim" => {
                let subject = payload
                    .get("subject")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let predicate = payload
                    .get("predicate")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let object = payload
                    .get("object")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let truth = payload
                    .get("truthLayer")
                    .and_then(Value::as_str)
                    .unwrap_or("world");
                let conflicting: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM claim
                     WHERE project_id=?1 AND status='confirmed'
                       AND subject=?2 AND predicate=?3 AND truth_layer=?4
                       AND COALESCE(object,'')!=?5",
                    params![project_id, subject, predicate, truth, object],
                    |row| row.get(0),
                )?;
                if conflicting > 0 {
                    items.push(json!({
                        "candidateId":candidate.id,
                        "kind":candidate.kind,
                        "title":title,
                        "severity":"warning",
                        "message":format!("同一主体与谓词已有 {conflicting} 条不同对象的确认事实；确认前需核对真值层和时间范围。")
                    }));
                }
            }
            "relation" => {
                for field in ["from", "to"] {
                    let name = payload
                        .get(field)
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .trim();
                    let exists = crate::retrieval::entity_id_by_name(conn, project_id, name)?
                        .is_some()
                        || pending_entities.contains(name);
                    if !exists {
                        items.push(json!({
                            "candidateId":candidate.id,
                            "kind":candidate.kind,
                            "title":title,
                            "severity":"warning",
                            "message":format!("关系目标“{name}”既不是已确认实体，也不在本次实体候选中；确认关系前需先建立实体。")
                        }));
                    }
                }
            }
            "knowledge_entry" => {
                let name = payload
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let existing: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM knowledge_entry
                     WHERE project_id=?1 AND name=?2 AND status='confirmed'",
                    params![project_id, name],
                    |row| row.get(0),
                )?;
                if existing > 0 {
                    items.push(json!({
                        "candidateId":candidate.id,
                        "kind":candidate.kind,
                        "title":title,
                        "severity":"warning",
                        "message":"知识库已有同名条目；应改为补充别名或提出明确的差异修订。"
                    }));
                }
            }
            "outline_patch" => {
                let level = payload
                    .get("level")
                    .and_then(Value::as_str)
                    .unwrap_or("chapter");
                let order_no = payload.get("orderNo").and_then(Value::as_i64).unwrap_or(-1);
                let occupied: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM outline_node
                     WHERE project_id=?1 AND level=?2 AND order_no=?3
                       AND status!='dropped'",
                    params![project_id, level, order_no],
                    |row| row.get(0),
                )?;
                if occupied > 0 {
                    items.push(json!({
                        "candidateId":candidate.id,
                        "kind":candidate.kind,
                        "title":title,
                        "severity":"warning",
                        "message":"目标卷章位置已有节点；应用补丁前需要作者决定替换、插入或另行编号。"
                    }));
                }
            }
            _ => {}
        }
    }
    Ok(json!({
        "candidateCount":candidates.len(),
        "items":items,
        "warningCount":items.len()
    }))
}

fn execute_read_tool(
    conn: &Connection,
    project_id: &str,
    run: &AgentRun,
    step: &AgentStep,
) -> AppResult<Value> {
    match step.tool_name.as_str() {
        "project_inventory" => project_inventory(conn, project_id),
        "search_fts" => search_tool(conn, project_id, &step.input, &run.goal),
        "search_hybrid" => {
            let query = step
                .input
                .get("query")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(&run.goal);
            let limit = step
                .input
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(20)
                .clamp(1, 50) as usize;
            Ok(serde_json::to_value(crate::retrieval::hybrid_search(
                conn, project_id, query, limit, None, None,
            )?)?)
        }
        "get_premise" => Ok(serde_json::to_value(crate::seed::active(
            conn, project_id,
        )?)?),
        "get_blueprint" => {
            let blueprint = match crate::blueprint::current(conn, project_id)? {
                Some(view) => Some(view),
                None => crate::blueprint::candidate(conn, project_id)?,
            };
            Ok(serde_json::to_value(blueprint)?)
        }
        "get_outline_plan" => Ok(serde_json::to_value(crate::outline::list(
            conn, project_id,
        )?)?),
        "get_style_profiles" => Ok(serde_json::to_value(crate::style::list(conn, project_id)?)?),
        "recommend_next_actions" => Ok(serde_json::to_value(next_actions(conn, project_id)?)?),
        "rebuild_wiki" => {
            // Rebuild remains a derived-view operation and never changes authoritative story facts.
            // The read loop only has a shared connection, so defer rebuild requests to the explicit
            // workflow action while still exposing its current status here.
            let pages = crate::wiki::list(conn, project_id)?;
            Ok(json!({
                "pageCount": pages.len(),
                "staleCount": pages.iter().filter(|page| page.stale_at.is_some()).count(),
                "requiresExplicitAction": true
            }))
        }
        "build_community_reports" => {
            let reports = crate::graphrag::list_communities(conn, project_id)?;
            Ok(json!({
                "communityCount": reports.len(),
                "requiresExplicitAction": reports.is_empty()
            }))
        }
        "foundation_check" => {
            serde_json::to_value(crate::foundation::evaluate(conn, project_id)?).map_err(Into::into)
        }
        "candidate_impact" => candidate_impact(conn, project_id, &run.id),
        "graph_ppr" => {
            let names: Vec<String> = step
                .input
                .get("seeds")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            let mut seeds = Vec::new();
            for name in names {
                if let Some(id) = crate::retrieval::entity_id_by_name(conn, project_id, &name)? {
                    seeds.push(id);
                }
            }
            let scores = crate::retrieval::ppr(conn, project_id, &seeds, 0.85)?;
            let mut ranked: Vec<_> = scores.into_iter().collect();
            ranked.sort_by(|left, right| {
                right
                    .1
                    .partial_cmp(&left.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let mut nodes = Vec::new();
            for (id, score) in ranked.into_iter().take(30) {
                if let Ok(entity) = crate::entities::get_entity(conn, project_id, &id) {
                    nodes.push(json!({
                        "id":id,
                        "name":entity.canonical_name,
                        "type":entity.entity_type,
                        "score":score
                    }));
                }
            }
            Ok(json!({"seedEntityIds":seeds,"nodes":nodes}))
        }
        "get_entity" => {
            let id = step
                .input
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::Message("get_entity 缺少 id".into()))?;
            Ok(serde_json::to_value(crate::entities::get_entity(
                conn, project_id, id,
            )?)?)
        }
        "get_relationships" => {
            let entity_id = step
                .input
                .get("entityId")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::Message("get_relationships 缺少 entityId".into()))?;
            let mut stmt = conn.prepare(
                "SELECT r.id, r.from_id, a.canonical_name, r.to_id, b.canonical_name,
                    r.type, r.evidence_id
                 FROM relation r
                 JOIN entity a ON a.id=r.from_id JOIN entity b ON b.id=r.to_id
                 WHERE r.project_id=?1 AND r.status='confirmed'
                   AND (r.from_id=?2 OR r.to_id=?2)
                 ORDER BY r.created_at DESC",
            )?;
            let rows = stmt.query_map(params![project_id, entity_id], |row| {
                Ok(json!({
                    "id":row.get::<_,String>(0)?,
                    "fromId":row.get::<_,String>(1)?,
                    "from":row.get::<_,String>(2)?,
                    "toId":row.get::<_,String>(3)?,
                    "to":row.get::<_,String>(4)?,
                    "type":row.get::<_,String>(5)?,
                    "evidence":row.get::<_,Option<String>>(6)?
                }))
            })?;
            Ok(Value::Array(rows.collect::<Result<Vec<_>, _>>()?))
        }
        "get_timeline" => {
            let mut stmt = conn.prepare(
                "SELECT id, title, story_time, reveal_time, participants, cause, outcome, source_span
                 FROM event WHERE project_id=?1 AND status='confirmed'
                 ORDER BY story_time, created_at",
            )?;
            let rows = stmt.query_map([project_id], |row| {
                Ok(json!({
                    "id":row.get::<_,String>(0)?,
                    "title":row.get::<_,String>(1)?,
                    "storyTime":row.get::<_,String>(2)?,
                    "revealTime":row.get::<_,Option<String>>(3)?,
                    "participants":serde_json::from_str::<Value>(&row.get::<_,String>(4)?).unwrap_or_else(|_| json!([])),
                    "cause":row.get::<_,String>(5)?,
                    "outcome":row.get::<_,String>(6)?,
                    "sourceSpan":row.get::<_,String>(7)?
                }))
            })?;
            Ok(Value::Array(rows.collect::<Result<Vec<_>, _>>()?))
        }
        "get_open_promises" => Ok(serde_json::to_value(
            crate::promises::list(conn, project_id)?
                .into_iter()
                .filter(|promise| !["paid_off", "abandoned"].contains(&promise.state.as_str()))
                .collect::<Vec<_>>(),
        )?),
        "audit_consistency" => Ok(serde_json::to_value(crate::rules::run_audit(
            conn, project_id,
        )?)?),
        "impact_analysis" => {
            let entity_id = step
                .input
                .get("entityId")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::Message("impact_analysis 缺少 entityId".into()))?;
            Ok(serde_json::to_value(crate::graphrag::impact(
                conn, project_id, entity_id,
            )?)?)
        }
        "wiki_index" => {
            let pages = crate::wiki::list(conn, project_id)?;
            Ok(json!(pages
                .into_iter()
                .map(|page| json!({
                    "id":page.id,
                    "title":page.title,
                    "kind":page.kind,
                    "sourceIds":page.source_ids,
                    "staleAt":page.stale_at
                }))
                .collect::<Vec<_>>()))
        }
        "read_knowledge_entry" => {
            let id = step
                .input
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::Message("read_knowledge_entry 缺少 id".into()))?;
            Ok(serde_json::to_value(
                crate::knowledge::entries(conn, project_id, None)?
                    .into_iter()
                    .find(|entry| entry.id == id)
                    .ok_or_else(|| AppError::Message("找不到知识条目".into()))?,
            )?)
        }
        other => Err(AppError::Message(format!(
            "工具 {other} 不能由只读执行器直接调用"
        ))),
    }
}

#[derive(Debug, Clone)]
pub struct ExtractionContext {
    pub context_json: Value,
}

fn evidence_corpus(conn: &Connection, run_id: &str, context: &Value) -> AppResult<String> {
    let mut corpus = String::new();
    for step in steps(conn, run_id)? {
        if let Some(items) = step.output.get("evidenceSources").and_then(Value::as_array) {
            for item in items {
                if let Some(quote) = item.get("quote").and_then(Value::as_str) {
                    corpus.push_str(quote);
                    corpus.push('\n');
                }
            }
        }
    }
    if let Some(items) = context
        .get("inventory")
        .and_then(|value| value.get("evidenceSources"))
        .and_then(Value::as_array)
    {
        for item in items {
            if let Some(quote) = item.get("quote").and_then(Value::as_str) {
                corpus.push_str(quote);
                corpus.push('\n');
            }
        }
    }
    Ok(corpus)
}

pub fn execute_read_steps(
    conn: &Connection,
    project_id: &str,
    run_id: &str,
) -> AppResult<Option<ExtractionContext>> {
    let mut current_run = run(conn, project_id, run_id)?;
    if current_run.status != "running" {
        return Err(AppError::Message("Agent 任务当前不在运行状态".into()));
    }
    let mut skipped_search = false;

    loop {
        let current_steps = steps(conn, run_id)?;
        let inventory_has_evidence = current_steps
            .iter()
            .find(|step| step.tool_name == "project_inventory" && step.status == "completed")
            .and_then(|step| step.output.get("evidenceSources"))
            .and_then(Value::as_array)
            .is_some_and(|items| !items.is_empty());
        let Some(step) = current_steps
            .iter()
            .find(|step| step.status == "pending" && step.tool_name != "model_synthesis")
            .cloned()
        else {
            break;
        };
        if step.tool_name == "search_fts" && !inventory_has_evidence {
            mark_step_skipped(
                conn,
                &step,
                json!({"reason":"项目清单中没有可检索的正文或知识片段"}),
            )?;
            skipped_search = true;
            continue;
        }
        mark_step_running(conn, &step)?;
        let input = if step.tool_name == "search_fts" {
            json!({"query":current_run.goal,"limit":24})
        } else {
            step.input.clone()
        };
        let executing_step = AgentStep {
            input: input.clone(),
            ..step.clone()
        };
        match execute_read_tool(conn, project_id, &current_run, &executing_step) {
            Ok(mut output) => {
                if let Some(object) = output.as_object_mut() {
                    object.insert("readOnly".into(), Value::Bool(true));
                    if skipped_search {
                        object.insert(
                            "planAdaptation".into(),
                            Value::String("项目没有可检索文本，已跳过全文检索".into()),
                        );
                    }
                }
                mark_step_completed(conn, &step, output)?;
            }
            Err(error) => {
                fail_step(conn, run_id, project_id, &error.to_string())?;
                return Err(error);
            }
        }
        sync_plan(conn, run_id)?;
        current_run = run(conn, project_id, run_id)?;
        if current_run.status != "running" {
            break;
        }
    }

    let current_steps = steps(conn, run_id)?;
    if current_run.status != "running" {
        return Ok(None);
    }
    let has_model = current_steps
        .iter()
        .any(|step| step.tool_name == "model_synthesis");
    if !has_model
        || current_steps
            .iter()
            .any(|step| step.tool_name == "model_synthesis" && step.status == "failed")
    {
        return Ok(None);
    }
    if current_steps
        .iter()
        .all(|step| step.tool_name != "model_synthesis" || step.status == "completed")
    {
        return Ok(None);
    }

    let inventory = current_steps
        .iter()
        .find(|step| step.tool_name == "project_inventory")
        .map(|step| step.output.clone())
        .unwrap_or_else(|| json!({}));
    let search = current_steps
        .iter()
        .find(|step| step.tool_name == "search_fts")
        .map(|step| step.output.clone())
        .unwrap_or_else(|| json!({}));
    let mut context = json!({
        "scope":current_run.scope,
        "output":current_run.output,
        "inventory":inventory,
        "search":search
    });
    context["contextHash"] = Value::String(crate::hash_text(&context.to_string()));
    let context_hash = crate::hash_text(&context.to_string());
    conn.execute(
        "UPDATE agent_run SET context_hash=?2, updated_at=?3 WHERE id=?1",
        params![run_id, context_hash, now()],
    )?;
    Ok(Some(ExtractionContext {
        context_json: context,
    }))
}

fn normalize_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join("")
}

fn evidence_verified(quote: &str, corpus: &str) -> bool {
    let quote = normalize_text(quote);
    if quote.chars().count() < 4 {
        return false;
    }
    normalize_text(corpus).contains(&quote)
}

fn allowed_kinds(output: &str) -> Vec<&'static str> {
    match output {
        "characters" => vec!["entity", "relation"],
        "timeline" => vec!["event"],
        "promises" => vec!["promise"],
        "knowledge" => vec!["knowledge_entry"],
        "outline" => vec!["outline_patch", "knowledge_entry"],
        _ => vec![
            "entity",
            "relation",
            "claim",
            "event",
            "promise",
            "knowledge_entry",
            "outline_patch",
        ],
    }
}

pub(crate) fn proposal_title(kind: &str, payload: &Value) -> String {
    let field = match kind {
        "entity" => "canonicalName",
        "relation" => "type",
        "claim" => "predicate",
        "event" | "promise" | "knowledge_entry" | "outline_patch" => "title",
        _ => "name",
    };
    payload
        .get(field)
        .or_else(|| payload.get("name"))
        .and_then(Value::as_str)
        .unwrap_or(kind)
        .to_string()
}

pub(crate) fn validate_proposal(kind: &str, payload: &Value) -> Result<(), String> {
    match kind {
        "entity" => {
            for field in ["canonicalName", "type"] {
                if payload
                    .get(field)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .is_none()
                {
                    return Err(format!("entity 缺少 {field}"));
                }
            }
        }
        "relation" => {
            for field in ["from", "to", "type"] {
                if payload
                    .get(field)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .is_none()
                {
                    return Err(format!("relation 缺少 {field}"));
                }
            }
        }
        "claim" => {
            for field in ["subject", "predicate", "truthLayer"] {
                if payload
                    .get(field)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .is_none()
                {
                    return Err(format!("claim 缺少 {field}"));
                }
            }
            let truth = payload
                .get("truthLayer")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !["world", "belief", "utterance", "narrative", "planned"].contains(&truth) {
                return Err(format!("claim 的 truthLayer 无效：{truth}"));
            }
        }
        "event" => {
            if payload
                .get("title")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default()
                .is_empty()
            {
                return Err("event 缺少 title".into());
            }
        }
        "promise" => {
            if payload
                .get("title")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default()
                .is_empty()
            {
                return Err("promise 缺少 title".into());
            }
        }
        "knowledge_entry" => {
            if payload
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default()
                .is_empty()
            {
                return Err("knowledge_entry 缺少 name".into());
            }
        }
        "outline_patch" => {
            if payload
                .get("title")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default()
                .is_empty()
            {
                return Err("outline_patch 缺少 title".into());
            }
        }
        other => return Err(format!("不支持候选类型：{other}")),
    }
    Ok(())
}

pub(crate) fn duplicate_proposal(
    conn: &Connection,
    project_id: &str,
    kind: &str,
    payload: &Value,
) -> AppResult<bool> {
    let text = |field: &str| {
        payload
            .get(field)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    let exists = match kind {
        "entity" => {
            let name = text("canonicalName");
            conn.query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM entity WHERE project_id=?1 AND canonical_name=?2
                   UNION ALL
                   SELECT 1 FROM candidate
                   WHERE project_id=?1 AND kind='entity' AND status='pending'
                     AND json_extract(payload,'$.canonicalName')=?2
                 )",
                params![project_id, name],
                |row| row.get::<_, bool>(0),
            )?
        }
        "relation" => conn.query_row(
            "SELECT EXISTS(
               SELECT 1 FROM relation r
               JOIN entity a ON a.id=r.from_id JOIN entity b ON b.id=r.to_id
               WHERE r.project_id=?1 AND r.status='confirmed'
                 AND a.canonical_name=?2 AND b.canonical_name=?3 AND r.type=?4
               UNION ALL
               SELECT 1 FROM candidate
               WHERE project_id=?1 AND kind='relation' AND status='pending'
                 AND json_extract(payload,'$.from')=?2
                 AND json_extract(payload,'$.to')=?3
                 AND json_extract(payload,'$.type')=?4
             )",
            params![project_id, text("from"), text("to"), text("type")],
            |row| row.get::<_, bool>(0),
        )?,
        "claim" => conn.query_row(
            "SELECT EXISTS(
               SELECT 1 FROM claim
               WHERE project_id=?1 AND subject=?2 AND predicate=?3
                 AND COALESCE(object,'')=?4 AND truth_layer=?5
               UNION ALL
               SELECT 1 FROM candidate
               WHERE project_id=?1 AND kind='claim' AND status='pending'
                 AND json_extract(payload,'$.subject')=?2
                 AND json_extract(payload,'$.predicate')=?3
                 AND COALESCE(json_extract(payload,'$.object'),'')=?4
                 AND json_extract(payload,'$.truthLayer')=?5
             )",
            params![
                project_id,
                text("subject"),
                text("predicate"),
                text("object"),
                text("truthLayer")
            ],
            |row| row.get::<_, bool>(0),
        )?,
        "event" => conn.query_row(
            "SELECT EXISTS(
               SELECT 1 FROM event WHERE project_id=?1 AND title=?2 AND story_time=?3
               UNION ALL
               SELECT 1 FROM candidate
               WHERE project_id=?1 AND kind='event' AND status='pending'
                 AND json_extract(payload,'$.title')=?2
                 AND COALESCE(json_extract(payload,'$.storyTime'),'')=?3
             )",
            params![project_id, text("title"), text("storyTime")],
            |row| row.get::<_, bool>(0),
        )?,
        "promise" => conn.query_row(
            "SELECT EXISTS(
               SELECT 1 FROM promise WHERE project_id=?1 AND title=?2
               UNION ALL
               SELECT 1 FROM candidate
               WHERE project_id=?1 AND kind='promise' AND status='pending'
                 AND json_extract(payload,'$.title')=?2
             )",
            params![project_id, text("title")],
            |row| row.get::<_, bool>(0),
        )?,
        "knowledge_entry" => conn.query_row(
            "SELECT EXISTS(
               SELECT 1 FROM knowledge_entry WHERE project_id=?1 AND name=?2
               UNION ALL
               SELECT 1 FROM candidate
               WHERE project_id=?1 AND kind='knowledge_entry' AND status='pending'
                 AND json_extract(payload,'$.name')=?2
             )",
            params![project_id, text("name")],
            |row| row.get::<_, bool>(0),
        )?,
        "outline_patch" => conn.query_row(
            "SELECT EXISTS(
               SELECT 1 FROM outline_node
               WHERE project_id=?1 AND level=?2 AND order_no=?3
                 AND title=?4
               UNION ALL
               SELECT 1 FROM candidate
               WHERE project_id=?1 AND kind='outline_patch' AND status='pending'
                 AND json_extract(payload,'$.level')=?2
                 AND json_extract(payload,'$.title')=?4
             )",
            params![
                project_id,
                text("level"),
                payload.get("orderNo").and_then(Value::as_i64).unwrap_or(-1),
                text("title")
            ],
            |row| row.get::<_, bool>(0),
        )?,
        _ => false,
    };
    Ok(exists)
}

pub fn complete_synthesis(
    conn: &mut Connection,
    project_id: &str,
    run_id: &str,
    raw: &str,
    model: &str,
) -> AppResult<Value> {
    let run = run(conn, project_id, run_id)?;
    if !["running", "paused"].contains(&run.status.as_str()) {
        return Err(AppError::Message("Agent 任务当前不在运行状态".into()));
    }
    let step = steps(conn, run_id)?
        .into_iter()
        .find(|step| step.tool_name == "model_synthesis")
        .ok_or_else(|| AppError::Message("任务没有结构化生成步骤".into()))?;
    mark_step_running(conn, &step)?;
    let parsed: Value = crate::llm::parse_json_loose(raw)?;
    let summary = parsed
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or("已完成项目分析。")
        .trim()
        .to_string();
    let candidates = if run.mode == "propose" {
        parsed
            .get("candidates")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    } else {
        vec![]
    };
    let kinds = allowed_kinds(&run.output);
    let corpus = evidence_corpus(conn, run_id, &json!({}))?;
    let mut created = Vec::new();
    let mut skipped = Vec::new();
    let mut created_ids = Vec::new();
    let mut step_evidence = Vec::new();
    let tx = conn.transaction()?;
    for proposal in candidates.into_iter().take(120) {
        let kind = proposal
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let Some(payload) = proposal.get("payload") else {
            skipped.push(json!({"kind":kind,"reason":"缺少 payload"}));
            continue;
        };
        if !kinds.contains(&kind) {
            skipped.push(json!({"kind":kind,"reason":"不在本次期望输出范围内"}));
            continue;
        }
        if let Err(reason) = validate_proposal(kind, payload) {
            skipped.push(json!({"kind":kind,"reason":reason}));
            continue;
        }
        let quote = proposal
            .get("quote")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        let source_span = proposal
            .get("sourceSpan")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        if source_span.is_empty() {
            skipped.push(
                json!({"kind":kind,"title":proposal_title(kind,payload),"reason":"缺少来源定位"}),
            );
            continue;
        }
        if !evidence_verified(quote, &corpus) {
            skipped.push(json!({"kind":kind,"title":proposal_title(kind,payload),"reason":"原文片段无法在只读快照中复核"}));
            continue;
        }
        if duplicate_proposal(&tx, project_id, kind, payload)? {
            skipped.push(json!({"kind":kind,"title":proposal_title(kind,payload),"reason":"与已有权威记录或待审候选重复"}));
            continue;
        }
        let confidence = proposal
            .get("confidence")
            .and_then(Value::as_f64)
            .unwrap_or(0.65);
        let candidate = crate::candidates::insert(
            &tx,
            project_id,
            kind,
            payload.clone(),
            Some(&format!("{source_span}｜{quote}")),
            None,
            confidence,
        )?;
        tx.execute(
            "UPDATE candidate SET batch_id=?3, agent_run_id=?4, agent_step_id=?5
             WHERE project_id=?1 AND id=?2",
            params![project_id, candidate.id, run_id, run_id, step.id],
        )?;
        let source_kind = source_span
            .split_once(':')
            .map(|(kind, _)| kind)
            .unwrap_or("source");
        let source_id = source_span
            .split_once(':')
            .map(|(_, id)| id)
            .unwrap_or(source_span);
        let evidence_id = uuid::Uuid::new_v4().to_string();
        let evidence_row = json!({
            "id":evidence_id,
            "candidateId":candidate.id,
            "sourceKind":source_kind,
            "sourceId":source_id,
            "sourceSpan":source_span,
            "quote":quote
        });
        tx.execute(
            "INSERT INTO agent_evidence
             (id, run_id, step_id, source_kind, source_id, source_span, quote, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                evidence_id,
                run_id,
                step.id,
                source_kind,
                source_id,
                source_span,
                quote,
                now()
            ],
        )?;
        step_evidence.push(evidence_row);
        created_ids.push(candidate.id.clone());
        created.push(json!({
            "candidateId":candidate.id,
            "kind":kind,
            "title":proposal_title(kind,payload),
            "riskLevel":candidate.risk_level
        }));
    }
    let output = json!({
        "summary":summary,
        "created":created,
        "skipped":skipped,
        "candidateCount":created_ids.len(),
        "evidenceCount":step_evidence.len()
    });
    tx.execute(
        "UPDATE agent_step SET status='completed', output_json=?2, candidate_ids=?3,
         evidence_json=?4, finished_at=?5 WHERE id=?1",
        params![
            step.id,
            output.to_string(),
            serde_json::to_string(&created_ids)?,
            serde_json::to_string(&step_evidence)?,
            now()
        ],
    )?;
    tx.execute(
        "UPDATE agent_run SET model=?3, updated_at=?4 WHERE project_id=?1 AND id=?2",
        params![project_id, run_id, model, now()],
    )?;
    tx.commit()?;
    sync_plan(conn, run_id)?;
    Ok(output)
}

pub fn finish_read_steps(
    conn: &Connection,
    project_id: &str,
    run_id: &str,
) -> AppResult<AgentRunDetail> {
    let current = run(conn, project_id, run_id)?;
    if current.status == "failed" {
        return detail(conn, project_id, run_id);
    }
    let pending_steps = steps(conn, run_id)?
        .into_iter()
        .filter(|step| step.status == "pending")
        .count();
    if pending_steps > 0 {
        return detail(conn, project_id, run_id);
    }
    let pending_candidates: i64 = conn.query_row(
        "SELECT COUNT(*) FROM candidate WHERE agent_run_id=?1 AND status='pending'",
        [run_id],
        |row| row.get(0),
    )?;
    let status = if current.mode == "analyze" || pending_candidates == 0 {
        "completed"
    } else {
        "waiting_approval"
    };
    let ts = now();
    conn.execute(
        "UPDATE agent_run SET status=?3, finished_at=?4, updated_at=?5
         WHERE project_id=?1 AND id=?2 AND status='running'",
        params![
            project_id,
            run_id,
            status,
            if status == "completed" {
                Some(ts.clone())
            } else {
                None
            },
            ts
        ],
    )?;
    sync_plan(conn, run_id)?;
    detail(conn, project_id, run_id)
}

pub fn refresh_review_status(conn: &Connection, project_id: &str, run_id: &str) -> AppResult<()> {
    let run = match run(conn, project_id, run_id) {
        Ok(run) => run,
        Err(_) => return Ok(()),
    };
    if run.status != "waiting_approval" {
        return Ok(());
    }
    if run.pending_candidates == 0 {
        let ts = now();
        conn.execute(
            "UPDATE agent_run SET status='completed', finished_at=?3, updated_at=?3
             WHERE project_id=?1 AND id=?2",
            params![project_id, run_id, ts],
        )?;
    }
    Ok(())
}

pub fn foundation_report(conn: &Connection, project_id: &str) -> AppResult<FoundationReport> {
    crate::foundation::evaluate(conn, project_id)
}

pub fn next_actions(conn: &Connection, project_id: &str) -> AppResult<Vec<AgentNextAction>> {
    let mut actions = Vec::new();
    let pending_candidates: i32 = conn.query_row(
        "SELECT COUNT(*) FROM candidate WHERE project_id=?1 AND status='pending'",
        [project_id],
        |row| row.get(0),
    )?;
    if pending_candidates > 0 {
        actions.push(AgentNextAction {
            key: "review-pending".into(),
            label: format!("先处理 {pending_candidates} 个待确认候选"),
            description:
                "候选和生成草稿尚未进入权威资料；先审批可以避免下一步基于未决定内容继续规划。"
                    .into(),
            prompt: "请帮我逐项解释当前待确认候选会怎样影响故事，并告诉我哪些应该先处理。".into(),
            action: "review_candidates".into(),
            priority: 0,
        });
    }

    let premise = crate::seed::list(conn, project_id)?.into_iter().next();
    let blueprint_current = crate::blueprint::current(conn, project_id)?;
    let blueprint_candidate = crate::blueprint::candidate(conn, project_id)?;
    let foundation = crate::foundation::evaluate(conn, project_id)?;
    let has_skeleton: bool = conn.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM outline_node
           WHERE project_id=?1 AND level='chapter' AND status!='dropped'
         )",
        [project_id],
        |row| row.get(0),
    )?;
    let confirmed_outline: i32 = conn.query_row(
        "SELECT COUNT(*) FROM outline_node
         WHERE project_id=?1 AND level='chapter' AND status='confirmed'",
        [project_id],
        |row| row.get(0),
    )?;
    let style_count: i32 = conn.query_row(
        "SELECT COUNT(*) FROM style_profile WHERE project_id=?1 AND status='confirmed'",
        [project_id],
        |row| row.get(0),
    )?;
    let (chapter_count, drafted_count): (i32, i32) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(CASE WHEN current_revision_id IS NOT NULL THEN 1 ELSE 0 END),0)
         FROM chapter WHERE project_id=?1",
        [project_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    match &premise {
        None => actions.push(AgentNextAction {
            key: "shape-premise".into(),
            label: "从一句话孵化故事方向".into(),
            description: "先给 Agent 一个画面、人物困境或一句 logline，它会生成三套因果发动机不同的方向。".into(),
            prompt: "我想先孵化故事。请先问我三个最关键的问题，再根据回答生成三套差异化的灵感方向。".into(),
            action: "seed_generate".into(),
            priority: 1,
        }),
        Some(premise) if premise.status != "confirmed" => actions.push(AgentNextAction {
            key: "refine-premise".into(),
            label: "继续微调当前灵感".into(),
            description: "第一版灵感通常只是起点；继续对话补强主角欲望、阻力、代价和核心戏剧问题。".into(),
            prompt: format!(
                "请基于当前灵感继续深挖，不要直接确认。重点检查主角欲望、结构性阻力、不可逆代价和核心戏剧问题，并给我 3 个可选择的强化方向：{}",
                premise.logline
            ),
            action: "chat".into(),
            priority: 1,
        }),
        Some(_) if blueprint_current.is_none() => {
            if blueprint_candidate.is_some() {
                actions.push(AgentNextAction {
                    key: "review-blueprint".into(),
                    label: "审阅并完善叙事蓝本".into(),
                    description: "叙事蓝本仍是候选。先检查核心戏剧问题、人物冲突、世界规则代价和因果链是否形成闭环。".into(),
                    prompt: "请审阅当前叙事蓝本候选，指出最影响后续卷章的三处因果或人物动机问题，并给出修改建议。".into(),
                    action: "chat".into(),
                    priority: 1,
                });
            } else {
                actions.push(AgentNextAction {
                    key: "generate-blueprint".into(),
                    label: "建立叙事蓝本与因果链".into(),
                    description: "从已确认种子卡生成人物、势力、规则、时间线和可验证的逻辑链候选。".into(),
                    prompt: "请为当前已确认种子卡建立叙事地基和主逻辑链，重点补齐主角与对手、关系张力、能力代价和核心戏剧问题。".into(),
                    action: "blueprint_generate".into(),
                    priority: 1,
                });
            }
        }
        Some(_) if !foundation.ready => actions.push(AgentNextAction {
            key: "repair-foundation".into(),
            label: format!("补齐 {} 项叙事地基", foundation.error_count),
            description: "这些是卷章规划前的阻断项，补齐后再让 Agent 规划结构。".into(),
            prompt: "请根据当前叙事地基检查结果，按影响程度列出必须先补的内容，并直接提出可确认的修订方案。".into(),
            action: "chat".into(),
            priority: 1,
        }),
        Some(_) if !has_skeleton => actions.push(AgentNextAction {
            key: "generate-outline".into(),
            label: "规划卷章骨架".into(),
            description: "让已确认蓝本的逻辑节点映射为卷章候选；不会自动确认章节结构。".into(),
            prompt: "请根据已确认叙事蓝本和因果链规划正式卷章骨架，优先保持逻辑节点闭环，不要套用固定十章。".into(),
            action: "outline_generate".into(),
            priority: 1,
        }),
        Some(_) if confirmed_outline == 0 => actions.push(AgentNextAction {
            key: "review-outline".into(),
            label: "审阅卷章候选".into(),
            description: "卷章骨架仍是候选；确认后正文工坊才会按这些章节开始写作。".into(),
            prompt: "请检查当前卷章候选是否完整承接逻辑链，指出节奏、因果断点和角色行动缺失，并给出修改建议。".into(),
            action: "open_seed".into(),
            priority: 1,
        }),
        Some(_) if style_count == 0 => actions.push(AgentNextAction {
            key: "shape-style".into(),
            label: "确定项目文笔方向".into(),
            description: "根据故事气质选择文笔预设，并生成待确认的文笔档案候选。".into(),
            prompt: "请根据当前故事的核心冲突、情绪基调和叙事视角，为我推荐一套具体可执行的文笔方案。".into(),
            action: "style_proposal".into(),
            priority: 1,
        }),
        Some(_) if chapter_count == 0 => actions.push(AgentNextAction {
            key: "confirm-outline-first".into(),
            label: "先确认至少一个章节".into(),
            description: "正文需要已确认章节作为事务边界；先完成卷章确认，再开始写作。".into(),
            prompt: "请帮我检查第一卷开头几章是否已经可以确认，并说明确认后第一场应该写什么。".into(),
            action: "open_seed".into(),
            priority: 1,
        }),
        Some(_) if drafted_count == 0 => actions.push(AgentNextAction {
            key: "plan-first-chapter".into(),
            label: "准备第一章开场".into(),
            description: "让 Agent 从本章绑定的逻辑节点、人物目标和伏笔债务整理出可直接写作的场景任务。".into(),
            prompt: "请为第一章准备开场方案：明确 POV、目标、冲突、转折、代价、要埋下的伏笔，以及不能提前泄露的信息。".into(),
            action: "chat".into(),
            priority: 1,
        }),
        Some(_) => actions.push(AgentNextAction {
            key: "advance-next-chapter".into(),
            label: "推进下一章".into(),
            description: "根据当前正文进度、未结伏笔和逻辑链给出下一章的具体推进方案。".into(),
            prompt: "请根据当前正文进度和逻辑链，告诉我下一章必须推进什么、不能提前解决什么，并给出三个可选开场冲突。".into(),
            action: "chat".into(),
            priority: 1,
        }),
    }

    if !foundation.ready && actions.len() < 3 && blueprint_current.is_some() {
        actions.push(AgentNextAction {
            key: "foundation-risk".into(),
            label: "检查叙事地基风险".into(),
            description: format!(
                "当前有 {} 项地基阻断问题；继续规划或写作前应先了解影响。",
                foundation.error_count
            ),
            prompt: "请解释当前叙事地基阻断问题会怎样影响卷章或正文，并按优先级给出修复顺序。"
                .into(),
            action: "chat".into(),
            priority: 2,
        });
    }
    if actions.len() < 3 {
        actions.push(AgentNextAction {
            key: "next-step-review".into(),
            label: "让 AI 判断下一步".into(),
            description: "结合当前项目状态、待审内容和叙事债务，给出一个最具体的下一步。".into(),
            prompt: "请判断我现在最应该做的一件事，并给出执行步骤、完成标准和不要提前做的事。"
                .into(),
            action: "chat".into(),
            priority: 2,
        });
    }
    actions.sort_by_key(|action| action.priority);
    Ok(actions)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{
        domain::{CreateProjectInput, KnowledgeDomainInput},
        project,
    };

    use super::*;

    fn project_fixture() -> (tempfile::TempDir, project::LoadedProject) {
        let dir = tempfile::tempdir().unwrap();
        let loaded = project::create(&CreateProjectInput {
            name: "agent".into(),
            root_path: dir.path().join("p").to_string_lossy().into_owned(),
            mode: "both".into(),
            writing_language: None,
        })
        .unwrap();
        let pid = loaded.summary.id.clone();
        loaded
            .conn
            .execute(
                "INSERT INTO text_fts
                 (content, tokenized, source_id, project_id, source_kind, chapter_id, branch_id)
                 VALUES (?1,?2,'source-1',?3,'import',NULL,NULL)",
                params![
                    "林澈在旧车站找到黄铜钥匙。守夜人要求他交出钥匙。",
                    crate::retrieval::tokenize("林澈在旧车站找到黄铜钥匙。守夜人要求他交出钥匙。"),
                    pid
                ],
            )
            .unwrap();
        (dir, loaded)
    }

    fn run_create() -> AgentRunCreateInput {
        AgentRunCreateInput {
            goal: "分析旧稿并整理人物、关系和时间线".into(),
            mode: "propose".into(),
            scope: "project".into(),
            output: "all".into(),
            session_id: None,
        }
    }

    #[test]
    fn run_and_steps_persist_after_reopen() {
        let (dir, mut loaded) = project_fixture();
        let pid = loaded.summary.id.clone();
        let created = create(&mut loaded.conn, &pid, &run_create()).unwrap();
        let root = dir.path().join("p");
        drop(loaded);
        let reopened = project::open(&root).unwrap();
        let persisted = run(&reopened.conn, &pid, &created.id).unwrap();
        assert_eq!(persisted.status, "draft");
        assert_eq!(steps(&reopened.conn, &created.id).unwrap().len(), 5);
    }

    #[test]
    fn registry_enforces_read_and_candidate_write_policy() {
        let registry = tools();
        assert!(registry.iter().any(|tool| {
            tool.name == "project_inventory" && tool.access == "read" && !tool.requires_approval
        }));
        assert!(registry.iter().any(|tool| {
            tool.name == "propose_entity"
                && tool.access == "write"
                && tool.requires_approval
                && tool.output_target == "candidate"
        }));
        assert!(registry.iter().any(|tool| {
            tool.name == "generate_narrative_blueprint"
                && tool.access == "write"
                && tool.requires_approval
                && tool.output_target == "candidate"
        }));
        assert!(registry
            .iter()
            .any(|tool| tool.name == "recommend_next_actions" && tool.access == "read"));
        assert!(!registry
            .iter()
            .any(|tool| tool.name.contains("accept") || tool.name.contains("confirm")));
    }

    #[test]
    fn next_actions_follow_stage_and_pending_candidates() {
        let (_dir, loaded) = project_fixture();
        let pid = loaded.summary.id.clone();
        let initial = next_actions(&loaded.conn, &pid).unwrap();
        assert!(initial
            .iter()
            .any(|action| action.action == "seed_generate"));
        crate::candidates::insert(
            &loaded.conn,
            &pid,
            "style_profile",
            json!({
                "scope":"project",
                "name":"待确认文笔",
                "pov":"第三人称",
                "person":"第三人称",
                "tense":"过去时",
                "syntax":{"avgSentenceMin":10,"avgSentenceMax":22,"shortRatio":45,"paragraphSentencesMin":3,"paragraphSentencesMax":5},
                "lexicon":{"concreteRatio":60,"register":"冷峻","adverbDensityMax":5,"bannedWords":[]},
                "imagery":{"sensoryChannels":["视觉"],"motifs":["雨"],"sensoryPer1000":8},
                "rhetoric":{"metaphorPer1000":3,"irony":30,"minimalism":40},
                "emotion":{"interiority":40,"externality":40,"intensityMin":2,"intensityMax":4},
                "ratios":{"action":40,"dialogue":30,"description":20,"interiority":10},
                "antiStyle":[]
            }),
            Some("agent:test"),
            None,
            0.7,
        )
        .unwrap();
        let actions = next_actions(&loaded.conn, &pid).unwrap();
        assert_eq!(actions[0].action, "review_candidates");
        assert!(actions[0].label.contains('1'));
    }

    #[test]
    fn read_tools_execute_without_writing_authoritative_entities() {
        let (_dir, loaded) = project_fixture();
        let pid = loaded.summary.id.clone();
        let mut conn = loaded.conn;
        let created = create(&mut conn, &pid, &run_create()).unwrap();
        start(&conn, &pid, &created.id).unwrap();
        let before: i64 = conn
            .query_row("SELECT COUNT(*) FROM entity", [], |row| row.get(0))
            .unwrap();
        let context = execute_read_steps(&conn, &pid, &created.id)
            .unwrap()
            .unwrap();
        assert!(!context.context_json["contextHash"]
            .as_str()
            .unwrap_or_default()
            .is_empty());
        assert!(
            context.context_json["inventory"]["evidenceSources"]
                .as_array()
                .unwrap()
                .len()
                > 0
        );
        let after: i64 = conn
            .query_row("SELECT COUNT(*) FROM entity", [], |row| row.get(0))
            .unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn synthesis_creates_linked_pending_candidates_and_evidence() {
        let (_dir, loaded) = project_fixture();
        let pid = loaded.summary.id.clone();
        let mut conn = loaded.conn;
        let created = create(&mut conn, &pid, &run_create()).unwrap();
        start(&conn, &pid, &created.id).unwrap();
        execute_read_steps(&conn, &pid, &created.id).unwrap();
        let raw = json!({
            "summary":"识别到一个主角和一件关键物品。",
            "candidates":[
                {
                    "kind":"entity",
                    "payload":{"type":"character","canonicalName":"林澈","aliases":[],"description":"在旧车站寻找钥匙的人"},
                    "sourceSpan":"import:source-1",
                    "quote":"林澈在旧车站找到黄铜钥匙。",
                    "confidence":0.9
                },
                {
                    "kind":"promise",
                    "payload":{"title":"黄铜钥匙的来历","contract":"揭示钥匙由谁留下，以及它为何能让守夜人退让。","window":"第一卷"},
                    "sourceSpan":"import:source-1",
                    "quote":"守夜人要求他交出钥匙。",
                    "confidence":0.78
                }
            ]
        })
        .to_string();
        complete_synthesis(&mut conn, &pid, &created.id, &raw, "test-model").unwrap();
        execute_read_steps(&conn, &pid, &created.id).unwrap();
        let result = finish_read_steps(&conn, &pid, &created.id).unwrap();
        assert_eq!(result.run.status, "waiting_approval");
        assert_eq!(result.candidates.len(), 2);
        assert!(result
            .candidates
            .iter()
            .all(|candidate| candidate.status == "pending"));
        assert!(result
            .candidates
            .iter()
            .all(|candidate| candidate.agent_run_id.as_deref() == Some(created.id.as_str())));
        assert_eq!(result.evidence.len(), 2);
        let entities: i64 = conn
            .query_row("SELECT COUNT(*) FROM entity", [], |row| row.get(0))
            .unwrap();
        assert_eq!(entities, 0);
    }

    #[test]
    fn pause_resume_and_retry_are_persisted() {
        let (_dir, loaded) = project_fixture();
        let pid = loaded.summary.id.clone();
        let mut conn = loaded.conn;
        let created = create(&mut conn, &pid, &run_create()).unwrap();
        start(&conn, &pid, &created.id).unwrap();
        assert_eq!(pause(&conn, &pid, &created.id).unwrap().status, "paused");
        assert_eq!(resume(&conn, &pid, &created.id).unwrap().status, "running");
        let first = steps(&conn, &created.id).unwrap()[0].clone();
        conn.execute(
            "UPDATE agent_step SET status='failed', error='temporary' WHERE id=?1",
            [&first.id],
        )
        .unwrap();
        conn.execute(
            "UPDATE agent_run SET status='failed', error='temporary' WHERE id=?1",
            [&created.id],
        )
        .unwrap();
        assert_eq!(
            retry_failed(&conn, &pid, &created.id, &first.id)
                .unwrap()
                .status,
            "running"
        );
        assert_eq!(steps(&conn, &created.id).unwrap()[0].status, "pending");
    }

    #[test]
    fn interrupted_runs_are_paused_for_safe_recovery() {
        let (_dir, loaded) = project_fixture();
        let pid = loaded.summary.id.clone();
        let mut conn = loaded.conn;
        let created = create(&mut conn, &pid, &run_create()).unwrap();
        start(&conn, &pid, &created.id).unwrap();
        mark_interrupted(&conn, &pid, &created.id).unwrap();
        let recovered = run(&conn, &pid, &created.id).unwrap();
        assert_eq!(recovered.status, "paused");
        assert!(recovered.error.contains("安全暂停"));
        assert_eq!(resume(&conn, &pid, &created.id).unwrap().status, "running");
    }

    #[test]
    fn unverified_quotes_are_skipped() {
        let (_dir, loaded) = project_fixture();
        let pid = loaded.summary.id.clone();
        let mut conn = loaded.conn;
        let created = create(&mut conn, &pid, &run_create()).unwrap();
        start(&conn, &pid, &created.id).unwrap();
        execute_read_steps(&conn, &pid, &created.id).unwrap();
        let raw = json!({
            "summary":"模型臆造。",
            "candidates":[{
                "kind":"entity",
                "payload":{"type":"character","canonicalName":"不存在的人"},
                "sourceSpan":"import:source-1",
                "quote":"这句话不在原稿里。"
            }]
        })
        .to_string();
        let output = complete_synthesis(&mut conn, &pid, &created.id, &raw, "test-model").unwrap();
        assert_eq!(output["candidateCount"], 0);
        assert_eq!(output["skipped"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn knowledge_candidate_only_becomes_authoritative_after_accept() {
        let (_dir, loaded) = project_fixture();
        let pid = loaded.summary.id.clone();
        let mut conn = loaded.conn;
        let domain = crate::knowledge::create_domain(
            &conn,
            &pid,
            &KnowledgeDomainInput {
                name: "能力".into(),
                description: "能力与代价".into(),
                fields: vec!["代价".into()],
            },
        )
        .unwrap();
        let candidate = crate::candidates::insert(
            &conn,
            &pid,
            "knowledge_entry",
            json!({
                "domainId":domain.id,
                "name":"回声定位",
                "summary":"通过声呐辨认旧车站结构。",
                "aliases":[],
                "fields":{"field1":"每次使用会暂时失去方向感"}
            }),
            Some("import:source-1｜林澈在旧车站找到黄铜钥匙。"),
            None,
            0.8,
        )
        .unwrap();
        let before: i64 = conn
            .query_row("SELECT COUNT(*) FROM knowledge_entry", [], |row| row.get(0))
            .unwrap();
        crate::candidates::accept(&mut conn, &pid, &candidate.id).unwrap();
        let after: i64 = conn
            .query_row("SELECT COUNT(*) FROM knowledge_entry", [], |row| row.get(0))
            .unwrap();
        assert_eq!(after, before + 1);
    }

    #[test]
    fn outline_patch_is_authoritative_only_after_accept() {
        let (_dir, loaded) = project_fixture();
        let pid = loaded.summary.id.clone();
        let mut conn = loaded.conn;
        let candidate = crate::candidates::insert(
            &conn,
            &pid,
            "outline_patch",
            json!({
                "level":"chapter",
                "orderNo":1,
                "title":"钥匙的来历",
                "goal":"揭示钥匙与旧站的关系",
                "conflict":"守夜人拒绝解释",
                "turn":"钥匙能够延迟列车发车",
                "payoff":"为车站时间规则埋伏笔"
            }),
            Some("import:source-1｜守夜人要求他交出钥匙。"),
            None,
            0.8,
        )
        .unwrap();
        let before: i64 = conn
            .query_row("SELECT COUNT(*) FROM outline_node", [], |row| row.get(0))
            .unwrap();
        crate::candidates::accept(&mut conn, &pid, &candidate.id).unwrap();
        let (after, source): (i64, String) = conn
            .query_row(
                "SELECT COUNT(*), MAX(source) FROM outline_node",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(after, before + 1);
        assert_eq!(source, "agent");
    }
}
