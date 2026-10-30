//! 创作工艺层：作者守则、章节工艺卡诊断、叙事线程嵌套和创作流水线。
//!
//! 这些检查借鉴职业写作中可操作的结构工具：
//! - 每章必须发生价值转变（起点与终点不同），否则只是“信息搬运”；
//! - 叙事线程按“先开后关”嵌套收束，交错关闭会让结尾显得失衡；
//! - 后段大量开新线程、连续多章同极性收尾，都会让节奏变平。
//! 结果只是提示，不阻断写作，也不改写作者内容。

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::{
    domain::{
        CraftCheck, CraftReport, OutlineNode, PipelineStage, ProjectGuide, StoryThread,
        StoryThreadInput, WorkflowPipeline,
    },
    error::{AppError, AppResult},
    now,
};

pub const THREAD_KINDS: [&str; 4] = ["milieu", "inquiry", "character", "event"];

pub fn thread_kind_label(kind: &str) -> &'static str {
    match kind {
        "milieu" => "环境线",
        "inquiry" => "疑问线",
        "character" => "人物线",
        "event" => "事件线",
        _ => "线程",
    }
}

// ---------- 作者守则 ----------

pub fn guide(conn: &Connection, project_id: &str) -> AppResult<ProjectGuide> {
    Ok(conn
        .query_row(
            "SELECT content, updated_at FROM project_guide WHERE project_id=?1",
            [project_id],
            |row| {
                Ok(ProjectGuide {
                    content: row.get(0)?,
                    updated_at: row.get(1)?,
                })
            },
        )
        .optional()?
        .unwrap_or(ProjectGuide {
            content: String::new(),
            updated_at: None,
        }))
}

pub fn save_guide(conn: &Connection, project_id: &str, content: &str) -> AppResult<ProjectGuide> {
    let content = content.trim();
    if content.chars().count() > 8000 {
        return Err(AppError::Message("作者守则请控制在 8000 字以内".into()));
    }
    conn.execute(
        "INSERT INTO project_guide (project_id, content, updated_at) VALUES (?1,?2,?3)
         ON CONFLICT(project_id) DO UPDATE SET content=excluded.content, updated_at=excluded.updated_at",
        params![project_id, content, now()],
    )?;
    guide(conn, project_id)
}

// ---------- 叙事线程 ----------

fn thread_from_row(row: &Row<'_>) -> rusqlite::Result<StoryThread> {
    Ok(StoryThread {
        id: row.get(0)?,
        kind: row.get(1)?,
        title: row.get(2)?,
        question: row.get(3)?,
        open_chapter: row.get(4)?,
        close_chapter: row.get(5)?,
        status: row.get(6)?,
        notes: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

pub fn threads(conn: &Connection, project_id: &str) -> AppResult<Vec<StoryThread>> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, title, question, open_chapter, close_chapter, status, notes,
                created_at, updated_at
         FROM story_thread WHERE project_id=?1
         ORDER BY COALESCE(open_chapter, 1000000), created_at",
    )?;
    let rows = stmt
        .query_map([project_id], thread_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn save_thread(
    conn: &Connection,
    project_id: &str,
    input: &StoryThreadInput,
) -> AppResult<StoryThread> {
    let title = input.title.trim();
    if title.is_empty() {
        return Err(AppError::Message("线程标题不能为空".into()));
    }
    if !THREAD_KINDS.contains(&input.kind.as_str()) {
        return Err(AppError::Message(format!("未知线程类型：{}", input.kind)));
    }
    if !["open", "closed", "dropped"].contains(&input.status.as_str()) {
        return Err(AppError::Message(format!("未知线程状态：{}", input.status)));
    }
    if let (Some(open), Some(close)) = (input.open_chapter, input.close_chapter) {
        if close < open {
            return Err(AppError::Message("关闭章节不能早于开启章节".into()));
        }
    }
    let ts = now();
    let id = match input.id.as_deref().filter(|id| !id.is_empty()) {
        Some(id) => {
            let changed = conn.execute(
                "UPDATE story_thread SET kind=?3, title=?4, question=?5, open_chapter=?6,
                    close_chapter=?7, status=?8, notes=?9, updated_at=?10
                 WHERE project_id=?1 AND id=?2",
                params![
                    project_id,
                    id,
                    input.kind,
                    title,
                    input.question.trim(),
                    input.open_chapter,
                    input.close_chapter,
                    input.status,
                    input.notes.trim(),
                    ts
                ],
            )?;
            if changed == 0 {
                return Err(AppError::Message("找不到叙事线程".into()));
            }
            id.to_string()
        }
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO story_thread
                 (id, project_id, kind, title, question, open_chapter, close_chapter, status,
                  notes, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10)",
                params![
                    id,
                    project_id,
                    input.kind,
                    title,
                    input.question.trim(),
                    input.open_chapter,
                    input.close_chapter,
                    input.status,
                    input.notes.trim(),
                    ts
                ],
            )?;
            id
        }
    };
    Ok(conn.query_row(
        "SELECT id, kind, title, question, open_chapter, close_chapter, status, notes,
                created_at, updated_at FROM story_thread WHERE id=?1",
        [id],
        thread_from_row,
    )?)
}

pub fn delete_thread(conn: &Connection, project_id: &str, id: &str) -> AppResult<()> {
    conn.execute(
        "DELETE FROM story_thread WHERE project_id=?1 AND id=?2",
        params![project_id, id],
    )?;
    Ok(())
}

// ---------- 工艺诊断 ----------

/// 解析“+安全 → -暴露”这类价值转变；返回（起点极性, 终点极性, 起点文本, 终点文本）。
pub fn parse_value_shift(text: &str) -> Option<(Option<char>, Option<char>, String, String)> {
    let normalized = text
        .replace("->", "→")
        .replace("＝>", "→")
        .replace("=>", "→");
    let (from, to) = normalized.split_once('→')?;
    let polarity = |part: &str| -> (Option<char>, String) {
        let part = part.trim();
        let sign = part
            .chars()
            .find(|ch| matches!(ch, '+' | '-' | '＋' | '－' | '−'));
        let sign = sign.map(|ch| if matches!(ch, '+' | '＋') { '+' } else { '-' });
        let word: String = part
            .chars()
            .filter(|ch| !matches!(ch, '+' | '-' | '＋' | '－' | '−' | '(' | ')' | '（' | '）'))
            .collect::<String>()
            .trim()
            .to_string();
        (sign, word)
    };
    let (from_sign, from_word) = polarity(from);
    let (to_sign, to_word) = polarity(to);
    Some((from_sign, to_sign, from_word, to_word))
}

fn check(
    code: &str,
    severity: &str,
    chapter: Option<i32>,
    thread_id: Option<&str>,
    message: String,
    suggestion: &str,
) -> CraftCheck {
    CraftCheck {
        code: code.into(),
        severity: severity.into(),
        chapter,
        thread_id: thread_id.map(str::to_string),
        message,
        suggestion: suggestion.into(),
    }
}

pub fn chapter_checks(chapters: &[OutlineNode]) -> Vec<CraftCheck> {
    let mut checks = Vec::new();
    let mut run_sign: Option<char> = None;
    let mut run_len = 0;
    let last = chapters.len().saturating_sub(1);
    for (index, chapter) in chapters.iter().enumerate() {
        let number = Some(chapter.order_no);
        let shift = chapter.value_shift.trim();
        match parse_value_shift(shift) {
            None if shift.is_empty() => {
                checks.push(check(
                    "craft.value_shift_missing",
                    "warning",
                    number,
                    None,
                    format!(
                        "第 {} 章《{}》没有写价值转变",
                        chapter.order_no, chapter.title
                    ),
                    "用“+起点 → -终点”写清这一章让什么从一种状态变成另一种状态。",
                ));
                run_sign = None;
                run_len = 0;
            }
            None => {
                checks.push(check(
                    "craft.value_shift_format",
                    "info",
                    number,
                    None,
                    format!(
                        "第 {} 章的价值转变缺少“→”，无法判断起点和终点",
                        chapter.order_no
                    ),
                    "示例：+信任 → -猜疑。",
                ));
                run_sign = None;
                run_len = 0;
            }
            Some((from_sign, to_sign, from_word, to_word)) => {
                if from_sign.is_some() && from_sign == to_sign && from_word == to_word {
                    checks.push(check(
                        "craft.value_static",
                        "warning",
                        number,
                        None,
                        format!(
                            "第 {} 章《{}》起点与终点相同，章节没有发生实质变化",
                            chapter.order_no, chapter.title
                        ),
                        "让主角的处境、认知或关系在章末与章首不同；否则考虑与相邻章节合并。",
                    ));
                }
                match to_sign {
                    Some(sign) if Some(sign) == run_sign => run_len += 1,
                    Some(sign) => {
                        run_sign = Some(sign);
                        run_len = 1;
                    }
                    None => {
                        run_sign = None;
                        run_len = 0;
                    }
                }
                if run_len == 3 {
                    checks.push(check(
                        "craft.polarity_run",
                        "info",
                        number,
                        None,
                        format!(
                            "第 {} 章起已连续 3 章以{}收尾，节奏可能单调",
                            chapter.order_no - 2,
                            if run_sign == Some('+') {
                                "正向"
                            } else {
                                "负向"
                            }
                        ),
                        "在其中一章安排反向结果：胜利附带代价，或失败中获得新信息。",
                    ));
                }
            }
        }
        if index < last && chapter.hook.trim().is_empty() {
            checks.push(check(
                "craft.hook_missing",
                "info",
                number,
                None,
                format!("第 {} 章没有章末钩子", chapter.order_no),
                "写下读者翻到下一章的具体理由：一个未答的问题、一件将到的事或一个反常细节。",
            ));
        }
        if !chapter.crisis.trim().is_empty()
            && ["对还是错", "正确还是错误", "做对的事"]
                .iter()
                .any(|needle| chapter.crisis.contains(needle))
        {
            checks.push(check(
                "craft.false_dilemma",
                "info",
                number,
                None,
                format!("第 {} 章的危机看起来是“对错题”而不是两难", chapter.order_no),
                "让两个选项都要付出代价：两害相权，或两个都想要的好东西只能取其一。",
            ));
        }
    }
    checks
}

/// 线程按 FILO 收束：A 先开、B 后开，则 B 应先于 A 关闭。
pub fn thread_checks(threads: &[StoryThread], total_chapters: i32) -> Vec<CraftCheck> {
    let mut checks = Vec::new();
    let active: Vec<&StoryThread> = threads.iter().filter(|t| t.status != "dropped").collect();
    for (i, outer) in active.iter().enumerate() {
        let (Some(outer_open), Some(outer_close)) = (outer.open_chapter, outer.close_chapter)
        else {
            continue;
        };
        for inner in active.iter().skip(i + 1) {
            let (Some(inner_open), Some(inner_close)) = (inner.open_chapter, inner.close_chapter)
            else {
                continue;
            };
            let (first, second, first_close, second_open, second_close) =
                if outer_open <= inner_open {
                    (outer, inner, outer_close, inner_open, inner_close)
                } else {
                    (inner, outer, inner_close, outer_open, outer_close)
                };
            let first_open = first.open_chapter.unwrap_or_default();
            if first_open < second_open && second_open < first_close && first_close < second_close {
                checks.push(check(
                    "thread.crossed_close",
                    "warning",
                    Some(first_close),
                    Some(&first.id),
                    format!(
                        "「{}」先开启却在「{}」之前关闭（第 {} 章 < 第 {} 章），线程交错收束",
                        first.title, second.title, first_close, second_close
                    ),
                    "按先开后关的顺序收束：让后开启的线程先解决，或把先开的线程延后到外层收尾。",
                ));
            }
        }
    }
    if total_chapters > 0 {
        let late_mark = (total_chapters as f64 * 0.75).ceil() as i32;
        for thread in &active {
            if let Some(open) = thread.open_chapter {
                if open > late_mark && thread.kind != "event" {
                    checks.push(check(
                        "thread.late_open",
                        "info",
                        Some(open),
                        Some(&thread.id),
                        format!("「{}」在全书后 25% 才开启（第 {open} 章）", thread.title),
                        "后段应以收束为主；若这条线很重要，考虑在前文埋下它的问题。",
                    ));
                }
            }
            if thread.status == "open" && thread.close_chapter.is_none() {
                checks.push(check(
                    "thread.unclosed",
                    "info",
                    thread.open_chapter,
                    Some(&thread.id),
                    format!(
                        "{}「{}」还没有计划关闭章节",
                        thread_kind_label(&thread.kind),
                        thread.title
                    ),
                    "为它指定收束章节，或明确标记为续作/开放结尾保留。",
                ));
            }
        }
    }
    checks
}

pub fn report(conn: &Connection, project_id: &str) -> AppResult<CraftReport> {
    let chapters: Vec<OutlineNode> = crate::outline::list(conn, project_id)?
        .into_iter()
        .filter(|node| node.level == "chapter")
        .collect();
    let threads = threads(conn, project_id)?;
    let total = chapters.iter().map(|c| c.order_no).max().unwrap_or(0);
    let mut checks = chapter_checks(&chapters);
    checks.extend(thread_checks(&threads, total));
    let rank = |severity: &str| match severity {
        "error" => 0,
        "warning" => 1,
        _ => 2,
    };
    checks.sort_by_key(|item| (rank(&item.severity), item.chapter.unwrap_or(i32::MAX)));
    Ok(CraftReport {
        chapter_count: chapters.len() as i32,
        chapters_with_shift: chapters
            .iter()
            .filter(|c| !c.value_shift.trim().is_empty())
            .count() as i32,
        chapters_with_hook: chapters
            .iter()
            .filter(|c| !c.hook.trim().is_empty())
            .count() as i32,
        open_threads: threads.iter().filter(|t| t.status == "open").count() as i32,
        checks,
    })
}

/// 正文生成任务契约（L7）的补充：本章工艺卡与作者守则。二者都约束“怎么写”，不是世界事实。
pub fn task_contract_extras(
    conn: &Connection,
    project_id: &str,
    chapter_number: i32,
) -> AppResult<String> {
    let mut parts = Vec::new();
    if let Some(node) = crate::outline::list(conn, project_id)?
        .into_iter()
        .find(|node| {
            node.level == "chapter" && node.order_no == chapter_number && node.status == "confirmed"
        })
    {
        let mut card = Vec::new();
        for (label, value) in [
            ("价值转变", &node.value_shift),
            ("危机抉择", &node.crisis),
            ("章末钩子", &node.hook),
            ("读者揭示", &node.reveal),
        ] {
            if !value.trim().is_empty() {
                card.push(format!("{label}：{}", value.trim()));
            }
        }
        if !card.is_empty() {
            parts.push(format!("章节工艺卡（本章必须兑现）：\n{}", card.join("\n")));
        }
    }
    let guide = guide(conn, project_id)?;
    if !guide.content.trim().is_empty() {
        parts.push(format!("作者守则：\n{}", guide.content.trim()));
    }
    Ok(parts.join("\n\n"))
}

// ---------- 创作流水线 ----------

#[allow(clippy::too_many_arguments)]
fn stage(
    key: &str,
    label: &str,
    description: &str,
    done: bool,
    optional: bool,
    progress: f64,
    detail: String,
    nav: &str,
    prompt: &str,
) -> PipelineStage {
    PipelineStage {
        key: key.into(),
        label: label.into(),
        description: description.into(),
        status: if done {
            "done".into()
        } else if optional {
            "optional".into()
        } else {
            "todo".into()
        },
        progress: progress.clamp(0.0, 1.0),
        detail,
        nav: nav.into(),
        prompt: prompt.into(),
    }
}

/// 从前提到定稿的阶段门槛。顺序参考职业流程：先立创作契约与作者守则，
/// 再建最低可运行的故事圣经（人物、势力、能力代价、时间锚点），然后才是
/// 因果主线、卷章、章节工艺卡、正文与修订。阶段由数据库状态推导，不另存。
pub fn pipeline(conn: &Connection, project_id: &str) -> AppResult<WorkflowPipeline> {
    let scalar =
        |sql: &str| -> AppResult<i64> { Ok(conn.query_row(sql, [project_id], |row| row.get(0))?) };
    let premise_confirmed =
        scalar("SELECT COUNT(*) FROM premise WHERE project_id=?1 AND status='confirmed'")? > 0;
    let premise_any =
        scalar("SELECT COUNT(*) FROM premise WHERE project_id=?1 AND status!='archived'")? > 0;
    let guide = guide(conn, project_id)?;
    let foundation = crate::foundation::evaluate(conn, project_id)?;
    let blueprint_confirmed = crate::blueprint::current(conn, project_id)?.is_some();
    let blueprint_candidate = crate::blueprint::candidate(conn, project_id)?.is_some();
    let outline_total = scalar(
        "SELECT COUNT(*) FROM outline_node WHERE project_id=?1 AND level='chapter' AND status!='dropped'",
    )?;
    let outline_confirmed = scalar(
        "SELECT COUNT(*) FROM outline_node WHERE project_id=?1 AND level='chapter' AND status='confirmed'",
    )?;
    let crafted = scalar(
        "SELECT COUNT(*) FROM outline_node WHERE project_id=?1 AND level='chapter'
           AND status='confirmed' AND TRIM(value_shift)!='' AND TRIM(hook)!=''",
    )?;
    let chapters = scalar("SELECT COUNT(*) FROM chapter WHERE project_id=?1")?;
    let drafted = scalar(
        "SELECT COUNT(*) FROM chapter WHERE project_id=?1 AND current_revision_id IS NOT NULL",
    )?;
    let open_issues = scalar("SELECT COUNT(*) FROM issue WHERE project_id=?1 AND status='open'")?;
    let pending =
        scalar("SELECT COUNT(*) FROM candidate WHERE project_id=?1 AND status='pending'")?;
    let threads = scalar("SELECT COUNT(*) FROM story_thread WHERE project_id=?1")?;
    let ratio = |a: i64, b: i64| if b <= 0 { 0.0 } else { a as f64 / b as f64 };

    let premise_prompt = if premise_confirmed {
        "帮我回顾已确认的种子卡，指出最薄弱的一项并给出强化建议。"
    } else if premise_any {
        "帮我比较现有的种子卡候选，说明各自的长篇潜力，然后让我选一张确认。"
    } else {
        "我想开始一个新故事。请先问我想写什么——一句话、一个画面或一个冲突都可以——拿到我的想法后再生成三个方向。"
    };
    let mut stages =
        vec![
        stage(
            "premise",
            "创作契约",
            "一句话故事、核心冲突、情绪承诺与结局意图。",
            premise_confirmed,
            false,
            if premise_confirmed { 1.0 } else if premise_any { 0.5 } else { 0.0 },
            if premise_confirmed {
                "种子卡已确认".into()
            } else if premise_any {
                "已有种子卡候选，等待确认".into()
            } else {
                "还没有种子卡".into()
            },
            "seed",
            premise_prompt,
        ),
        stage(
            "guide",
            "作者守则",
            "长期有效的写作底线：禁写内容、命名规则、视角纪律、文风红线。",
            !guide.content.trim().is_empty(),
            true,
            if guide.content.trim().is_empty() { 0.0 } else { 1.0 },
            if guide.content.trim().is_empty() {
                "可选：写下 Agent 每次都必须遵守的规则".into()
            } else {
                format!("{} 字守则", guide.content.chars().count())
            },
            "canon",
            "根据种子卡起草一份作者守则：视角纪律、禁写内容、命名约定和文风红线，各 2 至 4 条，作为候选给我确认。",
        ),
        stage(
            "foundation",
            "故事圣经地基",
            "主角与对手、关系张力、势力、能力代价与边界、时间锚点、关键地点物品。",
            foundation.ready,
            false,
            ratio(foundation.passed_count as i64, foundation.total_count as i64),
            format!(
                "{}/{} 项通过{}",
                foundation.passed_count,
                foundation.total_count,
                if foundation.error_count > 0 {
                    format!("，{} 项阻断", foundation.error_count)
                } else {
                    String::new()
                }
            ),
            "characters",
            "检查叙事地基缺什么，优先补齐阻断项，并把可核实内容整理成候选。",
        ),
        stage(
            "causality",
            "因果主线",
            "唯一因果链：每个逻辑节点都写清前因、选择、代价和后果。",
            blueprint_confirmed,
            false,
            if blueprint_confirmed { 1.0 } else if blueprint_candidate { 0.5 } else { 0.0 },
            if blueprint_confirmed {
                "叙事蓝本已确认".into()
            } else if blueprint_candidate {
                "蓝本候选待审阅".into()
            } else {
                "尚未生成叙事蓝本".into()
            },
            "seed",
            "基于已确认的种子卡生成叙事蓝本，生成后告诉我有哪些阻断项需要先处理。",
        ),
        stage(
            "structure",
            "卷章骨架",
            "把因果节点编排为卷与章；篇幅由作者或逻辑链决定。",
            outline_confirmed > 0,
            false,
            ratio(outline_confirmed, outline_total.max(outline_confirmed)),
            if outline_total == 0 {
                "尚未规划卷章".into()
            } else {
                format!("{outline_confirmed}/{outline_total} 章已确认")
            },
            "seed",
            "按已确认的叙事蓝本规划卷章骨架；篇幅先问我，我没想法就按逻辑链自适应。",
        ),
        stage(
            "craft",
            "章节工艺卡",
            "每章写清价值转变、两难抉择、章末钩子；叙事线程先开后关。",
            outline_confirmed > 0 && crafted >= outline_confirmed,
            false,
            ratio(crafted, outline_confirmed),
            format!(
                "{crafted}/{outline_confirmed} 章具备价值转变与钩子 · {threads} 条叙事线程"
            ),
            "seed",
            "检查章节工艺卡：哪些章节没有价值转变或钩子，线程是否交错收束，并提出修订候选。",
        ),
        stage(
            "draft",
            "正文起草",
            "按章写作；草稿先进入候选，确认后才成为正文版本。",
            chapters > 0 && drafted >= chapters,
            false,
            ratio(drafted, chapters),
            format!("{drafted}/{chapters} 章已有正文"),
            "studio",
            "读取下一章的工艺卡与相关设定，为它起草正文候选。",
        ),
        stage(
            "revision",
            "审计与修订",
            "一致性审计、伏笔兑现、文笔漂移；候选逐项确认。",
            drafted > 0 && open_issues == 0 && pending == 0,
            false,
            if drafted == 0 {
                0.0
            } else {
                1.0 / (1.0 + (open_issues + pending) as f64 / 5.0)
            },
            format!("{open_issues} 个未处理冲突 · {pending} 个待确认候选"),
            "audit",
            "运行一致性巡检，按影响范围排序解释需要先处理的问题。",
        ),
    ];
    let active_key = stages
        .iter()
        .find(|stage| stage.status == "todo")
        .map(|stage| stage.key.clone())
        .unwrap_or_else(|| "revision".into());
    for stage in &mut stages {
        if stage.key == active_key && stage.status != "done" {
            stage.status = "active".into();
        }
    }
    Ok(WorkflowPipeline { stages, active_key })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapter(order: i32, shift: &str, hook: &str) -> OutlineNode {
        OutlineNode {
            id: format!("n{order}"),
            parent_id: None,
            level: "chapter".into(),
            order_no: order,
            title: format!("章{order}"),
            goal: String::new(),
            conflict: String::new(),
            turn: String::new(),
            payoff: String::new(),
            logic_node_ids: vec![],
            presentation_mode: String::new(),
            narrative_function: String::new(),
            value_shift: shift.into(),
            crisis: String::new(),
            hook: hook.into(),
            reveal: String::new(),
            status: "confirmed".into(),
            branch_id: None,
            source: "author".into(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn thread(id: &str, open: i32, close: Option<i32>) -> StoryThread {
        StoryThread {
            id: id.into(),
            kind: "inquiry".into(),
            title: id.into(),
            question: String::new(),
            open_chapter: Some(open),
            close_chapter: close,
            status: if close.is_some() { "closed" } else { "open" }.into(),
            notes: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn parses_value_shift_polarity() {
        let (from, to, a, b) = parse_value_shift("+安全 → -暴露").unwrap();
        assert_eq!((from, to), (Some('+'), Some('-')));
        assert_eq!((a.as_str(), b.as_str()), ("安全", "暴露"));
        assert!(parse_value_shift("没有箭头").is_none());
    }

    #[test]
    fn flags_static_missing_and_monotone_chapters() {
        let chapters = vec![
            chapter(1, "+希望 → +希望", "钩子"),
            chapter(2, "", "钩子"),
            chapter(3, "+a → -b", "钩子"),
            chapter(4, "+c → -d", "钩子"),
            chapter(5, "+e → -f", ""),
        ];
        let codes: Vec<String> = chapter_checks(&chapters)
            .into_iter()
            .map(|c| c.code)
            .collect();
        assert!(codes.contains(&"craft.value_static".to_string()));
        assert!(codes.contains(&"craft.value_shift_missing".to_string()));
        assert!(codes.contains(&"craft.polarity_run".to_string()));
        // 最后一章不要求钩子。
        assert!(!codes.contains(&"craft.hook_missing".to_string()));
    }

    #[test]
    fn detects_crossed_thread_closure_but_accepts_nesting() {
        let nested = vec![thread("outer", 1, Some(10)), thread("inner", 3, Some(8))];
        assert!(thread_checks(&nested, 10)
            .iter()
            .all(|c| c.code != "thread.crossed_close"));
        let crossed = vec![thread("first", 1, Some(6)), thread("second", 3, Some(9))];
        let checks = thread_checks(&crossed, 10);
        let crossed_check = checks
            .iter()
            .find(|c| c.code == "thread.crossed_close")
            .expect("crossed closure");
        assert_eq!(crossed_check.thread_id.as_deref(), Some("first"));
        let open = vec![thread("late", 9, None)];
        let codes: Vec<String> = thread_checks(&open, 10)
            .into_iter()
            .map(|c| c.code)
            .collect();
        assert!(codes.contains(&"thread.late_open".to_string()));
        assert!(codes.contains(&"thread.unclosed".to_string()));
    }
}
