//! 写作 Agent harness：工具注册表、权限分级、虚拟项目文件系统与运行记录。
//!
//! 设计取自编码 Agent（Codex / opencode）的成熟做法，但边界按叙事项目重写：
//! - 模型通过工具调用自主决定读什么、查什么，不再走固定五步计划；
//! - 项目被映射成一棵只读的虚拟路径树（/人物/林澈、/章节/12 …），
//!   让作者和 Agent 可以用同一套“路径”指代资料；
//! - 工具分三级：read 自动执行；propose 只生成候选；control 只影响本轮
//!   对话（计划、提问、工作流建议）。确认候选、提交正文和覆盖权威事实的
//!   能力根本不在注册表里，模型无法调用；
//! - “规划”模式只读，“执笔”模式才允许 propose。

use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use crate::{
    domain::{HarnessToolTrace, StoryThreadInput},
    error::{AppError, AppResult},
    llm::ToolCall,
    now,
};

pub const MAX_MODEL_TURNS: usize = 10;
pub const MAX_CALLS_PER_TURN: usize = 8;
const MAX_TOOL_CHARS: usize = 6000;
const PAGE_CHARS: usize = 3000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Read,
    Propose,
    Control,
}

impl Tier {
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Read => "read",
            Tier::Propose => "propose",
            Tier::Control => "control",
        }
    }
}

pub struct HarnessTool {
    pub name: &'static str,
    pub description: &'static str,
    pub tier: Tier,
    pub schema: Value,
}

/// 作者选择的 Agent 模式。plan 只读；write 可以提出候选。
pub fn normalize_mode(mode: &str) -> &'static str {
    if mode == "plan" {
        "plan"
    } else {
        "write"
    }
}

pub fn allowed(mode: &str, tier: Tier) -> bool {
    !(normalize_mode(mode) == "plan" && tier == Tier::Propose)
}

pub fn registry() -> Vec<HarnessTool> {
    let path_schema = |extra: Value| {
        let mut schema = json!({
            "type":"object",
            "properties":{"path":{"type":"string","description":"虚拟路径，例如 /、/人物、/人物/林澈、/章节/3"}},
            "required":["path"]
        });
        if let (Some(target), Some(extra)) =
            (schema["properties"].as_object_mut(), extra.as_object())
        {
            for (key, value) in extra {
                target.insert(key.clone(), value.clone());
            }
        }
        schema
    };
    vec![
        HarnessTool {
            name: "list_paths",
            description: "列出项目虚拟目录。从 / 开始可以看到守则、种子卡、蓝本、大纲、章节、人物、设定、知识、伏笔、线程、时间线、审计、候选和流水线。",
            tier: Tier::Read,
            schema: path_schema(json!({})),
        },
        HarnessTool {
            name: "read_path",
            description: "读取一个虚拟路径的内容。章节正文较长时用 offset/limit（按字符）分页。返回内容是权威资料或其派生视图，引用原文时必须逐字复制。",
            tier: Tier::Read,
            schema: path_schema(json!({
                "offset":{"type":"integer","description":"正文起始字符，默认 0"},
                "limit":{"type":"integer","description":"读取字符数，默认 3000，最大 6000"}
            })),
        },
        HarnessTool {
            name: "search",
            description: "在正文、导入稿、人物、知识条目中做全文 + 实体图谱混合检索，返回带来源的片段。用于定位设定出处或核对事实。",
            tier: Tier::Read,
            schema: json!({
                "type":"object",
                "properties":{
                    "query":{"type":"string"},
                    "limit":{"type":"integer","description":"默认 8，最大 20"}
                },
                "required":["query"]
            }),
        },
        HarnessTool {
            name: "project_status",
            description: "读取创作流水线阶段、项目计数、叙事地基检查和章节工艺诊断摘要。不确定下一步做什么时先调用它。",
            tier: Tier::Read,
            schema: json!({"type":"object","properties":{}}),
        },
        HarnessTool {
            name: "craft_check",
            description: "运行章节工艺与叙事线程诊断：价值转变缺失或静止、连续同极性、缺少章末钩子、伪两难、线程交错收束、后段新开线程。",
            tier: Tier::Read,
            schema: json!({"type":"object","properties":{}}),
        },
        HarnessTool {
            name: "run_audit",
            description: "运行一致性巡检（死亡后行动、同时两地、物品生命周期、关系方向、视角知情泄露、伏笔逾期），结果进入审计队列。",
            tier: Tier::Read,
            schema: json!({"type":"object","properties":{}}),
        },
        HarnessTool {
            name: "propose_change",
            description: "提出一条资料候选，作者确认后才写入权威库。kind 与 payload：\n\
- entity: {\"type\":\"character|faction|place|item|concept\",\"canonicalName\":\"\",\"aliases\":[],\"description\":\"\"}；人物可再加 desire/need/fear/flaw/secret 与 values/taboo（数组）\n\
- relation: {\"from\":\"实体名\",\"to\":\"实体名\",\"type\":\"关系类型\",\"tension\":\"关系张力\"}\n\
- claim: {\"subject\":\"\",\"predicate\":\"\",\"object\":\"\",\"truthLayer\":\"world|belief|utterance|narrative|planned\"}\n\
- event: {\"title\":\"\",\"storyTime\":\"\",\"participants\":[],\"cause\":\"\",\"outcome\":\"\"}\n\
- promise: {\"title\":\"\",\"window\":\"计划兑现章节范围\",\"contract\":\"怎样才算兑现\"}\n\
- knowledge_entry: {\"domainKey\":\"environment|item|faction|power|worldrule|自定义域 key\",\"name\":\"\",\"aliases\":[],\"summary\":\"\",\"fields\":{}}\n\
- outline_patch: {\"level\":\"volume|chapter\",\"orderNo\":1,\"title\":\"\",\"goal\":\"\",\"conflict\":\"\",\"turn\":\"\",\"payoff\":\"\"}\n\
- entity_patch：修订已存在的人物/实体，{\"canonicalName\":\"已有名称\",\"description\":\"\",\"desire\":\"\",\"need\":\"\",\"fear\":\"\",\"flaw\":\"\",\"secret\":\"\"}，只填要改的字段\n\
evidence_quote：若内容来自项目原文，逐字复制 read_path/search 返回的片段（至少 4 个字）；若是你的创意建议则留空，并在 reason 中说明。",
            tier: Tier::Propose,
            schema: json!({
                "type":"object",
                "properties":{
                    "kind":{"type":"string","enum":["entity","entity_patch","relation","claim","event","promise","knowledge_entry","outline_patch"]},
                    "payload":{"type":"object"},
                    "evidence_quote":{"type":"string"},
                    "reason":{"type":"string"}
                },
                "required":["kind","payload","reason"]
            }),
        },
        HarnessTool {
            name: "propose_craft",
            description: "为已有章节提出工艺卡修订候选：价值转变（+起点 → -终点）、危机两难、章末钩子、读者揭示，也可同时修订目标/冲突/转折/兑现。只填需要修改的字段。",
            tier: Tier::Propose,
            schema: json!({
                "type":"object",
                "properties":{
                    "chapter":{"type":"integer","description":"章节序号"},
                    "valueShift":{"type":"string"},
                    "crisis":{"type":"string"},
                    "hook":{"type":"string"},
                    "reveal":{"type":"string"},
                    "goal":{"type":"string"},
                    "conflict":{"type":"string"},
                    "turn":{"type":"string"},
                    "payoff":{"type":"string"},
                    "reason":{"type":"string"}
                },
                "required":["chapter","reason"]
            }),
        },
        HarnessTool {
            name: "propose_blueprint_patch",
            description: "对尚未确认的叙事蓝本提出逐字段修订候选：设定/人物条目（按名称）或因果节点（按序号）。用于修复审稿中发现的矛盾、年代、代价不合规等问题。每处只改一个字段，写出完整新文本；作者确认后才写入并重新诊断。",
            tier: Tier::Propose,
            schema: json!({
                "type":"object",
                "properties":{
                    "changes":{
                        "type":"array",
                        "items":{
                            "type":"object",
                            "properties":{
                                "target":{"type":"string","enum":["element","node"]},
                                "name":{"type":"string","description":"target=element 时的条目名称"},
                                "order":{"type":"integer","description":"target=node 时的节点序号"},
                                "field":{"type":"string","description":"element：summary/role/externalGoal/internalNeed/fear/flaw/leverage/secret；node：title/cause/choice/cost/outcome/stateChange/knowledgeChange/storyTime/location"},
                                "value":{"type":"string","description":"该字段修订后的完整文本"}
                            },
                            "required":["target","field","value"]
                        }
                    },
                    "reason":{"type":"string"}
                },
                "required":["changes","reason"]
            }),
        },
        HarnessTool {
            name: "propose_thread",
            description: "提出一条叙事线程候选。kind：milieu 环境线（进入/离开一个世界）、inquiry 疑问线（提出/解答问题）、character 人物线（内在不满/改变）、event 事件线（秩序被打破/恢复）。线程应按先开后关的顺序嵌套。",
            tier: Tier::Propose,
            schema: json!({
                "type":"object",
                "properties":{
                    "kind":{"type":"string","enum":["milieu","inquiry","character","event"]},
                    "title":{"type":"string"},
                    "question":{"type":"string","description":"这条线向读者提出的问题"},
                    "openChapter":{"type":"integer"},
                    "closeChapter":{"type":"integer"},
                    "reason":{"type":"string"}
                },
                "required":["kind","title","question","reason"]
            }),
        },
        HarnessTool {
            name: "propose_draft",
            description: "为某一章起草正文候选。写作前必须先 read_path 该章工艺卡、相关人物和前一章结尾。mode=append 续写到现有正文之后；mode=replace 给出整章新稿。作者在正文页确认后才成为正式版本。",
            tier: Tier::Propose,
            schema: json!({
                "type":"object",
                "properties":{
                    "chapter":{"type":"integer"},
                    "mode":{"type":"string","enum":["append","replace"]},
                    "text":{"type":"string"},
                    "note":{"type":"string","description":"给作者的写作说明：遵循了哪些工艺卡要素"}
                },
                "required":["chapter","mode","text"]
            }),
        },
        HarnessTool {
            name: "propose_guide",
            description: "提出作者守则修订候选（视角纪律、禁写内容、命名约定、文风红线）。content 是修订后的完整守则。",
            tier: Tier::Propose,
            schema: json!({
                "type":"object",
                "properties":{"content":{"type":"string"},"reason":{"type":"string"}},
                "required":["content","reason"]
            }),
        },
        HarnessTool {
            name: "update_plan",
            description: "维护本次任务的计划清单。多步骤任务开始时调用，执行过程中更新状态。",
            tier: Tier::Control,
            schema: json!({
                "type":"object",
                "properties":{
                    "items":{
                        "type":"array",
                        "items":{
                            "type":"object",
                            "properties":{
                                "title":{"type":"string"},
                                "status":{"type":"string","enum":["pending","in_progress","done"]}
                            },
                            "required":["title","status"]
                        }
                    }
                },
                "required":["items"]
            }),
        },
        HarnessTool {
            name: "ask_author",
            description: "把关键创作取舍交还作者：给出问题和 2 至 4 个选项。调用后本轮结束，等待作者选择。",
            tier: Tier::Control,
            schema: json!({
                "type":"object",
                "properties":{
                    "question":{"type":"string"},
                    "options":{"type":"array","items":{"type":"string"}}
                },
                "required":["question","options"]
            }),
        },
        HarnessTool {
            name: "propose_knowledge_domain",
            description: "提出新建知识域候选（如禁墟、神道、灾厄、门派），fields 是该域条目要填写的字段名。作者确认后才能往该域提出知识条目。",
            tier: Tier::Propose,
            schema: json!({
                "type":"object",
                "properties":{
                    "name":{"type":"string"},
                    "description":{"type":"string"},
                    "fields":{"type":"array","items":{"type":"string"}},
                    "reason":{"type":"string"}
                },
                "required":["name","description","fields"]
            }),
        },
        HarnessTool {
            name: "sync_blueprint",
            description: "把已确认叙事蓝本中的人物、势力、能力体系、世界规则、地点、物品、关系和时间线整理成资料库候选（人物页、知识库、时间线可见），已存在的同名条目自动跳过。蓝本确认后用它把故事圣经落到资料库。",
            tier: Tier::Propose,
            schema: json!({"type":"object","properties":{}}),
        },
        HarnessTool {
            name: "generate_seed",
            description: "根据作者的一句话（画面、冲突或故事梗概）生成三个因果发动机不同的种子卡候选，作者可在“灵感与卷章 → 种子卡”查看。logline 必须来自作者本人的话；作者还没说想写什么时，不要调用，先问作者。",
            tier: Tier::Propose,
            schema: json!({
                "type":"object",
                "properties":{"logline":{"type":"string","description":"作者给出的灵感原话，可附上作者补充的要求"}},
                "required":["logline"]
            }),
        },
        HarnessTool {
            name: "generate_blueprint",
            description: "基于已确认的种子卡分层构建故事圣经并生成叙事蓝本候选：世界规律与能力体系 → 势力格局 → 人物群像 → 总体结构与暗线 → 漏洞审查 → 因果链。会保留作者已确认的人物与知识条目。需要先有已确认种子卡，耗时数分钟。",
            tier: Tier::Propose,
            schema: json!({
                "type":"object",
                "properties":{
                    "medium":{"type":"string","enum":["novel","game"]},
                    "strategy":{"type":"string","enum":["adaptive","fixed"],"description":"adaptive 由逻辑链决定体量；fixed 需要 targetUnits"},
                    "targetUnits":{"type":"integer","description":"fixed 时的目标章节/任务数"},
                    "depth":{"type":"string","enum":["epic","standard","quick"],"description":"epic 分层构建庞大严谨的世界观（长篇默认）；standard 中等规模；quick 单次快速草案"}
                },
                "required":["medium","strategy"]
            }),
        },
        HarnessTool {
            name: "generate_outline",
            description: "基于已确认叙事蓝本生成卷章骨架候选。mode：adaptive 由逻辑链估算体量；chapters 单卷直排需 totalChapters；volumes 分卷需 volumes。作者没说篇幅时用 adaptive 或先问作者。",
            tier: Tier::Propose,
            schema: json!({
                "type":"object",
                "properties":{
                    "mode":{"type":"string","enum":["adaptive","chapters","volumes"]},
                    "totalChapters":{"type":"integer"},
                    "volumes":{
                        "type":"array",
                        "items":{
                            "type":"object",
                            "properties":{"title":{"type":"string"},"chapterCount":{"type":"integer"}},
                            "required":["chapterCount"]
                        }
                    }
                },
                "required":["mode"]
            }),
        },
        HarnessTool {
            name: "recommend_style",
            description: "按作者要求和故事气质，从文笔预设中选一个并生成文笔档案候选。",
            tier: Tier::Propose,
            schema: json!({
                "type":"object",
                "properties":{"instruction":{"type":"string"}},
                "required":["instruction"]
            }),
        },
        HarnessTool {
            name: "rebuild_wiki",
            description: "重建叙事百科（派生视图，不改变权威事实）。资料大量变化后使用。",
            tier: Tier::Read,
            schema: json!({"type":"object","properties":{}}),
        },
        HarnessTool {
            name: "rebuild_graph",
            description: "重建叙事图社区报告（派生视图，不改变权威事实）。",
            tier: Tier::Read,
            schema: json!({"type":"object","properties":{}}),
        },
    ]
}

/// 需要调用模型或长事务的工具，由命令层异步执行，结果仍然只产生候选或派生视图。
pub const WORKFLOW_TOOLS: [&str; 6] = [
    "generate_seed",
    "generate_blueprint",
    "generate_outline",
    "recommend_style",
    "rebuild_wiki",
    "rebuild_graph",
];

pub fn is_workflow_tool(name: &str) -> bool {
    WORKFLOW_TOOLS.contains(&name)
}

pub fn find_tool(name: &str) -> Option<HarnessTool> {
    registry().into_iter().find(|tool| tool.name == name)
}

/// OpenAI 兼容 `tools` 参数；规划模式下不暴露 propose 工具。
pub fn tools_schema(mode: &str) -> Value {
    Value::Array(
        registry()
            .into_iter()
            .filter(|tool| allowed(mode, tool.tier))
            .map(|tool| {
                json!({
                    "type":"function",
                    "function":{
                        "name":tool.name,
                        "description":tool.description,
                        "parameters":tool.schema
                    }
                })
            })
            .collect(),
    )
}

/// 文本协议回退：不支持函数调用的模型在正文里输出 `<tool_call>{json}</tool_call>`。
pub fn text_protocol_instructions(mode: &str) -> String {
    let mut text = String::from(
        "\n\n## 工具调用协议\n当前模型通过文本调用工具。需要调用时单独输出一行：\n\
         <tool_call>{\"name\":\"工具名\",\"arguments\":{参数}}</tool_call>\n\
         可以连续输出多个调用；输出调用后立即停止，等待 <tool_result>。不需要工具时直接回答。\n可用工具：\n",
    );
    for tool in registry()
        .into_iter()
        .filter(|tool| allowed(mode, tool.tier))
    {
        text.push_str(&format!(
            "- {}：{}\n  参数：{}\n",
            tool.name, tool.description, tool.schema
        ));
    }
    text
}

/// 模型把原生函数调用标记（如 DeepSeek 的 DSML、未闭合的 tool_call）当正文输出时为真。
pub fn leaked_tool_markup(text: &str) -> bool {
    ["DSML", "<tool_call>", "<|tool_call", "<｜tool"]
        .iter()
        .any(|marker| text.contains(marker))
        || (text.contains("invoke name=") && text.contains("parameter name="))
}

pub fn parse_text_tool_calls(text: &str) -> (String, Vec<ToolCall>) {
    let pattern = Regex::new(r"(?s)<tool_call>\s*(.*?)\s*</tool_call>").expect("valid regex");
    let mut calls = Vec::new();
    for (index, capture) in pattern.captures_iter(text).enumerate() {
        let raw = capture.get(1).map(|m| m.as_str()).unwrap_or_default();
        let Ok(value) = crate::llm::parse_json_loose::<Value>(raw) else {
            continue;
        };
        let Some(name) = value.get("name").and_then(Value::as_str) else {
            continue;
        };
        calls.push(ToolCall {
            id: format!("text_call_{index}"),
            name: name.to_string(),
            arguments: value
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}))
                .to_string(),
        });
    }
    let visible = pattern.replace_all(text, "").trim().to_string();
    (visible, calls)
}

pub fn system_prompt(conn: &Connection, project_id: &str, mode: &str) -> AppResult<String> {
    let mode = normalize_mode(mode);
    let project_name: String = conn.query_row(
        "SELECT name FROM project WHERE id=?1",
        [project_id],
        |row| row.get(0),
    )?;
    let guide = crate::craft::guide(conn, project_id)?;
    let pipeline = crate::craft::pipeline(conn, project_id)?;
    let counts = crate::project::counts(conn, project_id)?;
    let stage_lines = pipeline
        .stages
        .iter()
        .map(|stage| {
            format!(
                "- [{}] {}：{}",
                match stage.status.as_str() {
                    "done" => "完成",
                    "active" => "当前",
                    "optional" => "可选",
                    _ => "待办",
                },
                stage.label,
                stage.detail
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut prompt = format!(
        r#"你是 CanonWeave 的写作 Agent，协助作者经营长篇小说或游戏剧情项目《{project_name}》。你像编码 Agent 管理代码仓库一样管理故事：先读资料，再提出可审阅的变更。

## 工作纪律
1. 先看后写：回答设定问题、评价章节或提出修改前，用 list_paths / read_path / search 读取相关资料。不要凭记忆编造项目内容；查不到就明说“项目中尚无记录”。
2. 多步任务先用 update_plan 列出 2 至 6 步，执行时更新状态。简单问题直接回答。
3. 你通过 propose_* 与 generate_* 工具产出候选。你不能确认候选、提交正文或覆盖权威事实；本轮产出的候选会以“确认卡”出现在你的回答下方，由作者点击确认。不要声称“已写入”“已确认”。
4. 证据纪律：来自项目原文的事实，evidence_quote 必须逐字复制工具返回的原文；你的创意建议不填 evidence_quote，并在 reason 里写明“建议”。
5. 区分真值层：world 客观事实、belief 角色相信、utterance 角色声称、narrative 叙述者给出、planned 作者计划揭示。
6. 关键创作取舍（结局走向、人物生死、核心设定、主线分叉）用 ask_author 给出 2 至 4 个选项，由作者拍板。
7. 作者用自然语言提出需求时直接调用对应工具：种子卡三方向用 generate_seed，叙事蓝本用 generate_blueprint，卷章骨架用 generate_outline，蓝本确认后落入资料库用 sync_blueprint，文笔方案用 recommend_style，人物/地点/势力/关系/事件/伏笔/知识条目用 propose_change，新知识域用 propose_knowledge_domain，章节工艺用 propose_craft，未确认蓝本里的条目或因果节点需要修订时用 propose_blueprint_patch（逐字段给出完整新文本），叙事线程用 propose_thread，正文草稿用 propose_draft，作者守则用 propose_guide。
   - 作者还没有说出自己的灵感或要求时，绝不替作者编造：先用一两句话问清楚（或 ask_author 给出选项），拿到作者原话后再生成。
   - 生成蓝本前确认种子卡已确认；生成卷章前确认蓝本已确认。前置条件不满足时说明缺什么，并引导作者在确认卡中完成确认。
8. 作者守则优先于你的偏好；发现请求与守则冲突时先指出冲突。
9. 收尾用中文简洁汇报：读了什么、提出了哪些候选（不是“已完成修改”）、还缺什么。最后一行写“下一步：”加一个具体可执行的动作。

## 构建故事圣经
- 长篇作品的地基要庞大而严谨：完整的能力体系（多阶位、每阶代价、边界、克制、社会影响）、6 个以上相互咬合的势力（含中立方与隐藏势力）、10 个以上有动机、秘密与错误信念的人物、跨时代且留下后果的历史、对手正在推进的暗线计划。
- 推荐路径：确认种子卡 → generate_blueprint（默认 epic，会分层构建并自我审查）→ 作者在确认卡中确认蓝本 → sync_blueprint 把故事圣经落入人物页、知识库与时间线。
- 作者想逐块打磨时，按“世界规律/能力体系 → 势力 → 人物 → 暗线”的顺序，一次深挖一块：先读已有资料，再成组提出候选。能力体系用 knowledge_entry 的 power 域，势力用 faction 域，世界规则用 worldrule 域，地点用 environment，物品用 item。
- 每个设定都做压力测试：为什么最强者不直接用它解决一切？为什么这个势力至今没赢也没被消灭？答案要写进设定。
- 拒绝空话：不写“神秘力量”“强大组织”，要写专有名词、数量、代价与因果。

## 回答格式
使用简洁的 Markdown：短段落、必要时用列表和加粗；不要用一级标题，不要输出大段表格。

## 叙事工艺准则
- 每章必须发生价值转变：章末与章首在处境、认知或关系上不同（如 +信任 → -猜疑）；没有转变的章节应合并或重写。
- 危机必须是真两难：两个选项都有代价。
- 章末钩子具体到人、物或信息；避免空泛的“命运开始转动”。
- 叙事线程（环境/疑问/人物/事件）先开后关、层层嵌套；全书后 25% 以收束为主。
- 能力与设定的限制、代价和失效条件比能力本身更能制造故事；解决冲突的能力必须事先被读者理解。
- 伏笔遵循“承诺 → 推进 → 兑现”；兑现必须改变读者认知或故事状态，只出现关键词不算兑现。
- 角色只能使用自己知道的信息；读者先知道而角色不知道的信息制造悬念。
- 用动作、感官和具体细节呈现情绪，不做总结式抒情。

## 当前模式
{}

## 创作流水线（由项目数据推导）
{stage_lines}

## 项目计数
人物/实体 {} · 知识条目 {} · 章节 {} · 未结伏笔 {} · 未处理冲突 {} · 待确认候选 {}"#,
        if mode == "plan" {
            "规划（只读）：只能阅读、检索、诊断和提问，不能提出候选。需要写入时请作者切换到“执笔”模式。"
        } else {
            "执笔：可以阅读并通过 propose_* 提出候选；所有候选都等待作者确认。"
        },
        counts.entities,
        counts.knowledge_entries,
        counts.chapters,
        counts.promises,
        counts.open_issues,
        counts.pending_candidates
    );
    if !guide.content.trim().is_empty() {
        prompt.push_str("\n\n## 作者守则（必须遵守）\n");
        prompt.push_str(guide.content.trim());
    }
    Ok(prompt)
}

// ---------- 工具执行 ----------

pub struct ToolContext<'a> {
    pub conn: &'a Connection,
    pub project_id: &'a str,
    pub session_id: &'a str,
    pub run_id: &'a str,
    pub step_id: &'a str,
    pub mode: &'a str,
}

#[derive(Debug, Default)]
pub struct ToolOutcome {
    /// 返回给模型的内容
    pub content: String,
    /// 给作者看的一行摘要
    pub summary: String,
    pub candidate_ids: Vec<String>,
    pub failed: bool,
    /// ask_author 等工具结束本轮
    pub terminal: bool,
    pub plan: Option<Value>,
    pub action: Option<Value>,
}

impl ToolOutcome {
    fn ok(content: String, summary: impl Into<String>) -> Self {
        Self {
            content,
            summary: summary.into(),
            ..Self::default()
        }
    }

    fn error(message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            content: format!("错误：{message}"),
            summary: message,
            failed: true,
            ..Self::default()
        }
    }
}

pub fn tool_title(name: &str, args: &Value) -> String {
    let text = |key: &str| args.get(key).and_then(Value::as_str).unwrap_or_default();
    match name {
        "list_paths" => format!("浏览 {}", text("path")),
        "read_path" => format!("读取 {}", text("path")),
        "search" => format!("检索「{}」", text("query")),
        "project_status" => "查看项目状态".into(),
        "craft_check" => "章节工艺诊断".into(),
        "run_audit" => "一致性巡检".into(),
        "propose_change" => format!("提出候选 · {}", kind_label(text("kind"))),
        "propose_craft" => format!(
            "提出工艺卡修订 · 第 {} 章",
            args.get("chapter")
                .and_then(Value::as_i64)
                .unwrap_or_default()
        ),
        "propose_thread" => format!("提出叙事线程 · {}", text("title")),
        "propose_blueprint_patch" => format!(
            "提出蓝本修订 · {} 处",
            args.get("changes")
                .and_then(Value::as_array)
                .map_or(0, Vec::len)
        ),
        "propose_draft" => format!(
            "起草正文 · 第 {} 章",
            args.get("chapter")
                .and_then(Value::as_i64)
                .unwrap_or_default()
        ),
        "propose_guide" => "提出作者守则修订".into(),
        "update_plan" => "更新计划".into(),
        "ask_author" => "询问作者".into(),
        "propose_knowledge_domain" => format!("提出知识域 · {}", text("name")),
        "generate_seed" => "生成种子卡三方向".into(),
        "sync_blueprint" => "蓝本落入资料库".into(),
        "generate_blueprint" => "生成叙事蓝本".into(),
        "generate_outline" => "规划卷章骨架".into(),
        "recommend_style" => "推荐文笔方案".into(),
        "rebuild_wiki" => "重建叙事百科".into(),
        "rebuild_graph" => "重建叙事图".into(),
        other => other.to_string(),
    }
}

fn kind_label(kind: &str) -> &'static str {
    match kind {
        "entity" => "实体",
        "relation" => "关系",
        "claim" => "事实",
        "event" => "事件",
        "promise" => "伏笔",
        "knowledge_entry" => "知识条目",
        "outline_patch" => "大纲节点",
        "blueprint_patch" => "蓝本修订",
        "entity_patch" => "人物修订",
        _ => "资料",
    }
}

fn clip(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut clipped: String = text.chars().take(limit).collect();
    clipped.push_str("\n…（已截断，请缩小范围或分页读取）");
    clipped
}

pub fn execute(ctx: &ToolContext<'_>, call: &ToolCall) -> ToolOutcome {
    let Some(tool) = find_tool(&call.name) else {
        return ToolOutcome::error(format!("未知工具：{}", call.name));
    };
    if !allowed(ctx.mode, tool.tier) {
        return ToolOutcome::error(
            "当前为规划（只读）模式，不能提出候选。请先汇报分析结果，由作者决定是否切换到执笔模式。",
        );
    }
    let args: Value = if call.arguments.trim().is_empty() {
        json!({})
    } else {
        match crate::llm::parse_json_loose(&call.arguments) {
            Ok(value) => value,
            Err(_) => return ToolOutcome::error("工具参数不是合法 JSON"),
        }
    };
    if is_workflow_tool(tool.name) {
        return ToolOutcome::error("该工具需要由命令层异步执行");
    }
    let result = match tool.name {
        "list_paths" => list_paths(ctx, &args),
        "read_path" => read_path(ctx, &args),
        "search" => search(ctx, &args),
        "project_status" => project_status(ctx),
        "craft_check" => craft_check(ctx),
        "run_audit" => run_audit(ctx),
        "propose_change" => propose_change(ctx, &args),
        "propose_craft" => propose_craft(ctx, &args),
        "propose_blueprint_patch" => propose_blueprint_patch(ctx, &args),
        "propose_thread" => propose_thread(ctx, &args),
        "propose_draft" => propose_draft(ctx, &args),
        "propose_guide" => propose_guide(ctx, &args),
        "update_plan" => update_plan(&args),
        "ask_author" => ask_author(&args),
        "propose_knowledge_domain" => propose_domain(ctx, &args),
        "sync_blueprint" => sync_blueprint(ctx),
        _ => Ok(ToolOutcome::error("工具尚未实现")),
    };
    match result {
        Ok(mut outcome) => {
            outcome.content = clip(&outcome.content, MAX_TOOL_CHARS);
            outcome
        }
        Err(error) => ToolOutcome::error(error.to_string()),
    }
}

fn segments(path: &str) -> Vec<String> {
    path.trim()
        .trim_matches('/')
        .split('/')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
        .collect()
}

fn chapter_number(segment: &str) -> Option<i32> {
    let digits: String = segment.chars().filter(|ch| ch.is_ascii_digit()).collect();
    digits.parse().ok()
}

const ROOT_ENTRIES: [(&str, &str); 14] = [
    ("守则", "作者守则：长期有效的写作规则"),
    ("种子卡", "创作契约：一句话故事、冲突、主题与结局意图"),
    ("蓝本", "叙事蓝本：核心戏剧问题、人物势力规则与因果链"),
    ("大纲", "卷章骨架与章节工艺卡"),
    ("章节", "按章正文"),
    ("人物", "人物档案、核心动机与关系"),
    ("设定", "势力、地点、物品、概念等实体"),
    ("知识", "知识域与知识条目"),
    ("伏笔", "伏笔台账与兑现契约"),
    ("线程", "叙事线程与嵌套诊断"),
    ("时间线", "已确认事件"),
    ("审计", "未处理的一致性问题"),
    ("候选", "等待作者确认的候选"),
    ("流水线", "创作流水线阶段"),
];

fn list_paths(ctx: &ToolContext<'_>, args: &Value) -> AppResult<ToolOutcome> {
    let path = args.get("path").and_then(Value::as_str).unwrap_or("/");
    let parts = segments(path);
    let (conn, pid) = (ctx.conn, ctx.project_id);
    let lines: Vec<String> = match parts.first().map(String::as_str) {
        None => ROOT_ENTRIES
            .iter()
            .map(|(name, note)| format!("/{name}  — {note}"))
            .collect(),
        Some("章节") => crate::scenes::chapters(conn, pid)?
            .into_iter()
            .map(|chapter| {
                format!(
                    "/章节/{}  — {}{}",
                    chapter.number,
                    chapter.title,
                    if chapter.current_revision_id.is_some() {
                        ""
                    } else {
                        "（尚无正文）"
                    }
                )
            })
            .collect(),
        Some("人物") => crate::entities::list(conn, pid, Some("character"))?
            .into_iter()
            .map(|entity| {
                format!(
                    "/人物/{}  — {}",
                    entity.canonical_name,
                    clip(&entity.description, 40)
                )
            })
            .collect(),
        Some("设定") => crate::entities::list(conn, pid, None)?
            .into_iter()
            .filter(|entity| entity.entity_type != "character")
            .map(|entity| {
                format!(
                    "/设定/{}  — [{}] {}",
                    entity.canonical_name,
                    entity.entity_type,
                    clip(&entity.description, 40)
                )
            })
            .collect(),
        Some("知识") => match parts.get(1) {
            None => crate::knowledge::domains(conn, pid)?
                .into_iter()
                .map(|domain| {
                    format!(
                        "/知识/{}  — key={} · {} 条",
                        domain.name, domain.key, domain.entry_count
                    )
                })
                .collect(),
            Some(domain_name) => {
                let domain = crate::knowledge::domains(conn, pid)?
                    .into_iter()
                    .find(|domain| &domain.name == domain_name || &domain.key == domain_name)
                    .ok_or_else(|| AppError::Message(format!("找不到知识域：{domain_name}")))?;
                crate::knowledge::entries(conn, pid, Some(&domain.id))?
                    .into_iter()
                    .map(|entry| {
                        format!(
                            "/知识/{}/{}  — {}",
                            domain.name,
                            entry.name,
                            clip(&entry.summary, 40)
                        )
                    })
                    .collect()
            }
        },
        Some("伏笔") => crate::promises::list(conn, pid)?
            .into_iter()
            .map(|promise| {
                format!(
                    "/伏笔/{}  — {} · {}",
                    promise.title, promise.state, promise.target_window
                )
            })
            .collect(),
        Some("线程") => crate::craft::threads(conn, pid)?
            .into_iter()
            .map(|thread| {
                format!(
                    "/线程/{}  — {} · 第{}章开 → {}",
                    thread.title,
                    crate::craft::thread_kind_label(&thread.kind),
                    thread
                        .open_chapter
                        .map(|n| n.to_string())
                        .unwrap_or("?".into()),
                    thread
                        .close_chapter
                        .map(|n| format!("第{n}章关"))
                        .unwrap_or_else(|| "未关闭".into())
                )
            })
            .collect(),
        Some(other) if ROOT_ENTRIES.iter().any(|(name, _)| *name == other) => {
            vec![format!("/{other} 是文件，请用 read_path 读取")]
        }
        Some(other) => return Err(AppError::Message(format!("未知目录：/{other}"))),
    };
    let summary = format!("{} 项", lines.len());
    let content = if lines.is_empty() {
        "（空）".to_string()
    } else {
        lines.join("\n")
    };
    Ok(ToolOutcome::ok(content, summary))
}

fn read_path(ctx: &ToolContext<'_>, args: &Value) -> AppResult<ToolOutcome> {
    let path = args.get("path").and_then(Value::as_str).unwrap_or("/");
    let parts = segments(path);
    let (conn, pid) = (ctx.conn, ctx.project_id);
    let Some(head) = parts.first().map(String::as_str) else {
        return list_paths(ctx, args);
    };
    let content = match (head, parts.get(1).map(String::as_str)) {
        ("守则", _) => {
            let guide = crate::craft::guide(conn, pid)?;
            if guide.content.trim().is_empty() {
                "作者尚未填写守则。".into()
            } else {
                guide.content
            }
        }
        ("种子卡", _) => match crate::seed::active(conn, pid)? {
            Some(premise) => format!(
                "状态：{}\n一句话故事：{}\n类型：{} / {}\n主题：{}\n情绪承诺：{}\n核心冲突：{}\n核心戏剧问题：{}\n对抗力量：{}\n主角欲望：{}\n缺陷：{}\n秘密：{}\n世界钩子：{}\n世界规则：{}\n视角：{} {} {}\n目标篇幅：{}\n结局意图：{}\n禁写：{}",
                premise.status,
                premise.logline,
                premise.genre,
                premise.subgenre,
                premise.theme,
                premise.emotional_promise,
                premise.core_conflict,
                premise.dramatic_question,
                premise.antagonistic_force,
                premise.protagonist_desire,
                premise.flaw,
                premise.secret,
                premise.world_hook.join("；"),
                premise.world_rule,
                premise.pov,
                premise.person,
                premise.tense,
                premise.target_length,
                premise.ending_intent,
                premise.taboo.join("；")
            ),
            None => "尚无种子卡。".into(),
        },
        ("蓝本", _) => {
            if crate::blueprint::current(conn, pid)?.is_some() {
                format!("状态：已确认\n{}", crate::blueprint::logic_context(conn, pid, 40)?)
            } else if let Some(view) = crate::blueprint::candidate(conn, pid)? {
                let mut text = format!(
                    "状态：候选（未确认）\n标题：{}\n核心戏剧问题：{}\n开局：{}\n终局：{}\n",
                    view.blueprint.title,
                    view.blueprint.core_dramatic_question,
                    view.blueprint.opening_state,
                    view.blueprint.ending_state
                );
                // 候选蓝本要能被 Agent 完整审阅：元素带秘密，另列叙事线与全部因果节点（用 offset 分页）。
                text.push_str("\n## 设定条目\n");
                for element in &view.elements {
                    text.push_str(&format!("- [{}] {}：{}", element.kind, element.name, element.summary));
                    if !element.secret.trim().is_empty() {
                        text.push_str(&format!("｜秘密：{}", element.secret));
                    }
                    text.push('\n');
                }
                text.push_str("\n## 叙事线\n");
                for thread in &view.threads {
                    text.push_str(&format!(
                        "- [{}] {}：{} → {}\n",
                        thread.kind, thread.title, thread.opening_state, thread.target_state
                    ));
                }
                text.push_str("\n## 因果链\n");
                for node in &view.nodes {
                    text.push_str(&format!(
                        "{}. [{}] {}（{}）\n   前因：{}\n   选择：{}\n   代价：{}\n   结果：{}\n",
                        node.order_no,
                        node.kind,
                        node.title,
                        node.story_time,
                        node.cause,
                        node.choice,
                        node.cost,
                        node.outcome
                    ));
                }
                text
            } else {
                "尚未生成叙事蓝本。".into()
            }
        }
        ("大纲", chapter) => read_outline(conn, pid, chapter.and_then(chapter_number))?,
        ("章节", None) => return list_paths(ctx, args),
        ("章节", Some(segment)) => {
            let number = chapter_number(segment)
                .ok_or_else(|| AppError::Message("章节路径应为 /章节/序号".into()))?;
            let chapter = crate::scenes::chapters(conn, pid)?
                .into_iter()
                .find(|chapter| chapter.number == number)
                .ok_or_else(|| AppError::Message(format!("第 {number} 章不存在")))?;
            let text = crate::scenes::chapter_text(conn, pid, &chapter.id)?;
            let total = text.chars().count();
            let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
            let limit = args
                .get("limit")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
                .unwrap_or(PAGE_CHARS)
                .clamp(200, MAX_TOOL_CHARS - 200);
            let page: String = text.chars().skip(offset).take(limit).collect();
            let end = (offset + page.chars().count()).min(total);
            format!(
                "第 {number} 章《{}》 · 共 {total} 字 · 显示 {offset}-{end}{}\n\n{}",
                chapter.title,
                if end < total {
                    format!("（继续读取用 offset={end}）")
                } else {
                    String::new()
                },
                if page.is_empty() { "（尚无正文）".into() } else { page }
            )
        }
        ("人物" | "设定", None) => return list_paths(ctx, args),
        ("人物" | "设定", Some(name)) => read_entity(conn, pid, name)?,
        ("知识", None) | ("知识", Some(_)) if parts.len() < 3 => return list_paths(ctx, args),
        ("知识", Some(domain_name)) => {
            let entry_name = &parts[2];
            let domain = crate::knowledge::domains(conn, pid)?
                .into_iter()
                .find(|domain| domain.name == domain_name || domain.key == domain_name)
                .ok_or_else(|| AppError::Message(format!("找不到知识域：{domain_name}")))?;
            let entry = crate::knowledge::entries(conn, pid, Some(&domain.id))?
                .into_iter()
                .find(|entry| &entry.name == entry_name || entry.aliases.iter().any(|a| a == entry_name))
                .ok_or_else(|| AppError::Message(format!("找不到知识条目：{entry_name}")))?;
            format!(
                "{} · {}\n别名：{}\n概述：{}\n字段：{}",
                domain.name,
                entry.name,
                entry.aliases.join("、"),
                entry.summary,
                serde_json::to_string_pretty(&entry.fields).unwrap_or_default()
            )
        }
        ("伏笔", None) => return list_paths(ctx, args),
        ("伏笔", Some(title)) => {
            let promise = crate::promises::list(conn, pid)?
                .into_iter()
                .find(|promise| promise.title == title)
                .ok_or_else(|| AppError::Message(format!("找不到伏笔：{title}")))?;
            serde_json::to_string_pretty(&promise)?
        }
        ("线程", _) => {
            let threads = crate::craft::threads(conn, pid)?;
            let report = crate::craft::report(conn, pid)?;
            let mut text = String::new();
            for thread in &threads {
                text.push_str(&format!(
                    "- {}「{}」 问题：{} · 第{}章开 · {} · 状态 {}\n",
                    crate::craft::thread_kind_label(&thread.kind),
                    thread.title,
                    thread.question,
                    thread.open_chapter.map(|n| n.to_string()).unwrap_or("?".into()),
                    thread
                        .close_chapter
                        .map(|n| format!("第{n}章关"))
                        .unwrap_or_else(|| "未关闭".into()),
                    thread.status
                ));
            }
            if text.is_empty() {
                text.push_str("尚无叙事线程。\n");
            }
            let thread_checks: Vec<String> = report
                .checks
                .iter()
                .filter(|check| check.code.starts_with("thread."))
                .map(|check| format!("- [{}] {}", check.severity, check.message))
                .collect();
            if !thread_checks.is_empty() {
                text.push_str("\n诊断：\n");
                text.push_str(&thread_checks.join("\n"));
            }
            text
        }
        ("时间线", _) => read_timeline(conn, pid)?,
        ("审计", _) => {
            let issues = crate::rules::issues(conn, pid, Some("open"))?;
            if issues.is_empty() {
                "没有未处理的一致性问题。".into()
            } else {
                issues
                    .iter()
                    .take(30)
                    .map(|issue| format!("- [{}] {}", issue.severity, issue.explanation))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
        ("候选", _) => {
            let pending = crate::candidates::list(conn, pid, Some("pending"), None)?;
            if pending.is_empty() {
                "没有待确认候选。".into()
            } else {
                pending
                    .iter()
                    .take(40)
                    .map(|candidate| {
                        format!(
                            "- [{}·{}] {}",
                            candidate.kind,
                            candidate.risk_level,
                            clip(&candidate.payload.to_string(), 120)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
        ("流水线", _) => {
            let pipeline = crate::craft::pipeline(conn, pid)?;
            pipeline
                .stages
                .iter()
                .map(|stage| format!("- {}（{}）：{} — {}", stage.label, stage.status, stage.detail, stage.description))
                .collect::<Vec<_>>()
                .join("\n")
        }
        (other, _) => return Err(AppError::Message(format!("未知路径：/{other}"))),
    };
    // 蓝本、审计等长视图与章节正文一样按 offset/limit 分页，否则后半部分永远读不到。
    let total = content.chars().count();
    let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(PAGE_CHARS)
        .clamp(200, MAX_TOOL_CHARS - 200);
    if total <= limit && offset == 0 {
        return Ok(ToolOutcome::ok(content, format!("{total} 字")));
    }
    let page: String = content.chars().skip(offset).take(limit).collect();
    let end = (offset + page.chars().count()).min(total);
    let more = if end < total {
        format!("\n\n（共 {total} 字，显示 {offset}-{end}；继续读取用 offset={end}）")
    } else {
        format!("\n\n（共 {total} 字，已读到结尾）")
    };
    Ok(ToolOutcome::ok(
        format!("{page}{more}"),
        format!("{offset}-{end} / {total} 字"),
    ))
}

fn read_outline(conn: &Connection, pid: &str, chapter: Option<i32>) -> AppResult<String> {
    let nodes = crate::outline::list(conn, pid)?;
    if nodes.is_empty() {
        return Ok("尚无卷章骨架。".into());
    }
    let mut lines = Vec::new();
    for node in &nodes {
        if let Some(number) = chapter {
            if node.level != "chapter" || node.order_no != number {
                continue;
            }
        }
        if node.level == "volume" {
            lines.push(format!(
                "## 卷 {}《{}》[{}] 目标：{}",
                node.order_no, node.title, node.status, node.goal
            ));
            continue;
        }
        let detailed = chapter.is_some();
        let mut line = format!(
            "第 {} 章《{}》[{}] 目标：{} ｜冲突：{} ｜转折：{} ｜价值转变：{} ｜钩子：{}",
            node.order_no,
            node.title,
            node.status,
            node.goal,
            node.conflict,
            node.turn,
            if node.value_shift.is_empty() {
                "（缺）"
            } else {
                &node.value_shift
            },
            if node.hook.is_empty() {
                "（缺）"
            } else {
                &node.hook
            }
        );
        if detailed {
            line.push_str(&format!(
                "\n危机：{}\n读者揭示：{}\n兑现/伏笔：{}\n呈现方式：{}\n叙事功能：{}",
                node.crisis,
                node.reveal,
                node.payoff,
                node.presentation_mode,
                node.narrative_function
            ));
            line.push_str(&format!(
                "\n\n本章因果上下文：\n{}",
                crate::blueprint::chapter_logic_context(conn, pid, node.order_no)
                    .unwrap_or_default()
            ));
        }
        lines.push(line);
    }
    if lines.is_empty() {
        return Ok("找不到该章节的骨架节点。".into());
    }
    Ok(lines.join("\n"))
}

fn read_entity(conn: &Connection, pid: &str, name: &str) -> AppResult<String> {
    let row: Option<(String, String, String, String, String)> = conn
        .query_row(
            "SELECT id, type, canonical_name, aliases, description FROM entity
             WHERE project_id=?1 AND (canonical_name=?2 OR EXISTS(
               SELECT 1 FROM json_each(entity.aliases) WHERE json_each.value=?2))
             LIMIT 1",
            params![pid, name],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()?;
    let Some((id, entity_type, canonical, aliases, description)) = row else {
        return Err(AppError::Message(format!(
            "找不到实体：{name}。可先 search 或 list_paths /人物"
        )));
    };
    let mut text = format!(
        "{canonical}（{entity_type}）\n别名：{}\n描述：{description}\n",
        serde_json::from_str::<Vec<String>>(&aliases)
            .unwrap_or_default()
            .join("、")
    );
    if let Some((desire, need, fear, flaw, values, secret, taboo)) = conn
        .query_row(
            "SELECT desire, need, fear, flaw, \"values\", secret, taboo FROM character_core WHERE entity_id=?1",
            [&id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                    row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                    row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                    row.get::<_, Option<String>>(6)?.unwrap_or_default(),
                ))
            },
        )
        .optional()?
    {
        text.push_str(&format!(
            "欲望：{desire}\n需求：{need}\n恐惧：{fear}\n缺陷：{flaw}\n价值观：{values}\n秘密：{secret}\n底线：{taboo}\n"
        ));
    }
    let mut stmt = conn.prepare(
        "SELECT a.canonical_name, r.type, b.canonical_name FROM relation r
         JOIN entity a ON a.id=r.from_id JOIN entity b ON b.id=r.to_id
         WHERE r.project_id=?1 AND (r.from_id=?2 OR r.to_id=?2) AND r.status='confirmed' LIMIT 30",
    )?;
    let relations = stmt
        .query_map(params![pid, id], |row| {
            Ok(format!(
                "- {} —{}→ {}",
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if !relations.is_empty() {
        text.push_str("关系：\n");
        text.push_str(&relations.join("\n"));
        text.push('\n');
    }
    let mut stmt = conn.prepare(
        "SELECT predicate, COALESCE(object,''), truth_layer, COALESCE(valid_from,'') FROM claim
         WHERE project_id=?1 AND (subject_id=?2 OR subject=?3) AND status='confirmed'
         ORDER BY updated_at DESC LIMIT 20",
    )?;
    let claims = stmt
        .query_map(params![pid, id, canonical], |row| {
            Ok(format!(
                "- [{}] {} {} {}",
                row.get::<_, String>(2)?,
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(3)?
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if !claims.is_empty() {
        text.push_str("已确认事实：\n");
        text.push_str(&claims.join("\n"));
    }
    Ok(text)
}

fn read_timeline(conn: &Connection, pid: &str) -> AppResult<String> {
    let mut stmt = conn.prepare(
        "SELECT title, COALESCE(story_time,''), COALESCE(cause,''), COALESCE(outcome,'')
         FROM event WHERE project_id=?1 ORDER BY story_time, created_at LIMIT 80",
    )?;
    let events = stmt
        .query_map([pid], |row| {
            Ok(format!(
                "- [{}] {} ｜因：{} ｜果：{}",
                row.get::<_, String>(1)?,
                row.get::<_, String>(0)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(if events.is_empty() {
        "尚无已确认事件。".into()
    } else {
        events.join("\n")
    })
}

fn search(ctx: &ToolContext<'_>, args: &Value) -> AppResult<ToolOutcome> {
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|query| !query.is_empty())
        .ok_or_else(|| AppError::Message("检索词不能为空".into()))?;
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(8)
        .clamp(1, 20) as usize;
    let result =
        crate::retrieval::hybrid_search(ctx.conn, ctx.project_id, query, limit, None, None)?;
    let lines: Vec<String> = result
        .hits
        .iter()
        .map(|hit| format!("- [{}:{}] {}", hit.source_kind, hit.source_id, hit.excerpt))
        .collect();
    let summary = format!("{} 条命中", lines.len());
    Ok(ToolOutcome::ok(
        if lines.is_empty() {
            format!("「{query}」没有命中。可换同义词、人物别名或更短的关键词。")
        } else {
            format!("检索模式：{}\n{}", result.retrieval_mode, lines.join("\n"))
        },
        summary,
    ))
}

fn project_status(ctx: &ToolContext<'_>) -> AppResult<ToolOutcome> {
    let pipeline = crate::craft::pipeline(ctx.conn, ctx.project_id)?;
    let foundation = crate::foundation::evaluate(ctx.conn, ctx.project_id)?;
    let craft = crate::craft::report(ctx.conn, ctx.project_id)?;
    let counts = crate::project::counts(ctx.conn, ctx.project_id)?;
    let mut text = String::from("创作流水线：\n");
    for stage in &pipeline.stages {
        text.push_str(&format!(
            "- {}（{}）：{}\n",
            stage.label, stage.status, stage.detail
        ));
    }
    text.push_str("\n叙事地基：\n");
    for check in foundation
        .checks
        .iter()
        .filter(|check| check.severity != "ok")
    {
        text.push_str(&format!(
            "- [{}] {}：{}\n",
            check.severity, check.label, check.message
        ));
    }
    text.push_str(&format!(
        "\n章节工艺：{} 章 · {} 章有价值转变 · {} 章有钩子 · {} 条线程未关闭 · {} 条诊断\n",
        craft.chapter_count,
        craft.chapters_with_shift,
        craft.chapters_with_hook,
        craft.open_threads,
        craft.checks.len()
    ));
    text.push_str(&format!("\n计数：{}", serde_json::to_string(&counts)?));
    let active = pipeline
        .stages
        .iter()
        .find(|stage| stage.key == pipeline.active_key)
        .map(|stage| stage.label.clone())
        .unwrap_or_default();
    Ok(ToolOutcome::ok(text, format!("当前阶段：{active}")))
}

fn craft_check(ctx: &ToolContext<'_>) -> AppResult<ToolOutcome> {
    let report = crate::craft::report(ctx.conn, ctx.project_id)?;
    let mut text = format!(
        "{} 章 · {} 章有价值转变 · {} 章有钩子 · {} 条线程未关闭\n",
        report.chapter_count,
        report.chapters_with_shift,
        report.chapters_with_hook,
        report.open_threads
    );
    for check in report.checks.iter().take(60) {
        text.push_str(&format!(
            "- [{}] {} → {}\n",
            check.severity, check.message, check.suggestion
        ));
    }
    Ok(ToolOutcome::ok(
        text,
        format!("{} 条诊断", report.checks.len()),
    ))
}

fn run_audit(ctx: &ToolContext<'_>) -> AppResult<ToolOutcome> {
    let result = crate::rules::run_audit(ctx.conn, ctx.project_id)?;
    let open = crate::rules::issues(ctx.conn, ctx.project_id, Some("open"))?;
    let mut text = format!(
        "新发现 {} 项；当前共 {} 项未处理。\n",
        result.issues_created,
        open.len()
    );
    for issue in open.iter().take(20) {
        text.push_str(&format!("- [{}] {}\n", issue.severity, issue.explanation));
    }
    Ok(ToolOutcome::ok(text, format!("{} 项未处理", open.len())))
}

/// 在项目原文与作者亲笔内容里定位引用；找不到返回 None。
pub fn locate_quote(
    conn: &Connection,
    project_id: &str,
    session_id: &str,
    quote: &str,
) -> AppResult<Option<String>> {
    let quote = quote.trim();
    if quote.chars().count() < 4 {
        return Ok(None);
    }
    let lookups: [(&str, &dyn rusqlite::ToSql); 6] = [
        (
            "SELECT source_kind || ':' || source_id FROM text_fts
             WHERE project_id=?1 AND instr(content, ?2) > 0 LIMIT 1",
            &project_id,
        ),
        (
            "SELECT 'entity:' || id FROM entity
             WHERE project_id=?1 AND instr(description, ?2) > 0 LIMIT 1",
            &project_id,
        ),
        (
            "SELECT 'knowledge:' || id FROM knowledge_entry
             WHERE project_id=?1 AND (instr(summary, ?2) > 0 OR instr(fields, ?2) > 0) LIMIT 1",
            &project_id,
        ),
        (
            "SELECT 'premise:' || id FROM premise
             WHERE project_id=?1 AND status!='archived' AND (
               instr(logline, ?2) > 0 OR instr(core_conflict, ?2) > 0 OR instr(world_hook, ?2) > 0
               OR instr(theme, ?2) > 0 OR instr(COALESCE(dramatic_question,''), ?2) > 0
               OR instr(COALESCE(world_rule,''), ?2) > 0) LIMIT 1",
            &project_id,
        ),
        (
            "SELECT 'outline:' || id FROM outline_node
             WHERE project_id=?1 AND status='confirmed' AND (instr(goal, ?2) > 0
               OR instr(conflict, ?2) > 0 OR instr(turn, ?2) > 0 OR instr(payoff, ?2) > 0) LIMIT 1",
            &project_id,
        ),
        (
            "SELECT 'chat:' || id FROM chat_message
             WHERE session_id=?1 AND role='user' AND instr(content, ?2) > 0 LIMIT 1",
            &session_id,
        ),
    ];
    for (sql, scope) in lookups {
        if let Some(span) = conn
            .query_row(sql, params![scope, quote], |row| row.get::<_, String>(0))
            .optional()?
        {
            return Ok(Some(span));
        }
    }
    Ok(None)
}

fn link_candidate(ctx: &ToolContext<'_>, candidate_id: &str) -> AppResult<()> {
    ctx.conn.execute(
        "UPDATE candidate SET batch_id=?3, agent_run_id=?3, agent_step_id=?4
         WHERE project_id=?1 AND id=?2",
        params![ctx.project_id, candidate_id, ctx.run_id, ctx.step_id],
    )?;
    Ok(())
}

fn reason_of(args: &Value) -> String {
    args.get("reason")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

const ENTITY_PATCH_FIELDS: [&str; 6] = ["description", "desire", "need", "fear", "flaw", "secret"];

/// 修订已存在的实体：记下修订前的值与更新时间，作者确认时据此防止覆盖期间的改动。
fn propose_entity_patch(
    ctx: &ToolContext<'_>,
    args: &Value,
    payload: &Value,
) -> AppResult<ToolOutcome> {
    let name = payload
        .get("canonicalName")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    let found = ctx
        .conn
        .query_row(
            "SELECT e.id, e.updated_at, e.description, COALESCE(c.desire,''), COALESCE(c.need,''),
                COALESCE(c.fear,''), COALESCE(c.flaw,''), COALESCE(c.secret,'')
             FROM entity e LEFT JOIN character_core c ON c.entity_id=e.id
             WHERE e.project_id=?1 AND e.canonical_name=?2",
            rusqlite::params![ctx.project_id, name],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    (2..8)
                        .map(|index| row.get::<_, String>(index))
                        .collect::<rusqlite::Result<Vec<_>>>()?,
                ))
            },
        )
        .optional()?;
    let Some((entity_id, updated_at, before_values)) = found else {
        return Ok(ToolOutcome::error(format!(
            "找不到实体「{name}」；新实体请用 kind=entity"
        )));
    };
    let mut changes = serde_json::Map::new();
    let mut before = serde_json::Map::new();
    for (field, old) in ENTITY_PATCH_FIELDS.iter().zip(before_values) {
        if let Some(value) = payload.get(*field).and_then(Value::as_str).map(str::trim) {
            if !value.is_empty() && value != old {
                changes.insert((*field).into(), Value::String(value.into()));
                before.insert((*field).into(), Value::String(old));
            }
        }
    }
    if changes.is_empty() {
        return Ok(ToolOutcome::error("没有与现有内容不同的修订字段"));
    }
    let count = changes.len();
    let candidate = crate::candidates::insert(
        ctx.conn,
        ctx.project_id,
        "entity_patch",
        json!({
            "entityId": entity_id,
            "canonicalName": name,
            "changes": changes,
            "before": before,
            "baseUpdatedAt": updated_at,
            "agentReason": reason_of(args)
        }),
        None,
        None,
        0.6,
    )?;
    link_candidate(ctx, &candidate.id)?;
    Ok(ToolOutcome {
        content: format!(
            "已创建「{name}」的修订候选 {}（{count} 个字段），等待作者确认。",
            candidate.id
        ),
        summary: format!("{name} · {count} 个字段"),
        candidate_ids: vec![candidate.id],
        ..ToolOutcome::default()
    })
}

fn propose_change(ctx: &ToolContext<'_>, args: &Value) -> AppResult<ToolOutcome> {
    let kind = args.get("kind").and_then(Value::as_str).unwrap_or_default();
    let mut payload = args.get("payload").cloned().unwrap_or_else(|| json!({}));
    if !payload.is_object() {
        return Ok(ToolOutcome::error("payload 必须是对象"));
    }
    if kind == "entity_patch" {
        return propose_entity_patch(ctx, args, &payload);
    }
    if let Err(reason) = crate::agent::validate_proposal(kind, &payload) {
        return Ok(ToolOutcome::error(reason));
    }
    if crate::agent::duplicate_proposal(ctx.conn, ctx.project_id, kind, &payload)? {
        return Ok(ToolOutcome::error(format!(
            "「{}」与已有权威记录或待审候选重复，不再提出；要修订已有条目请用 kind=entity_patch。",
            crate::agent::proposal_title(kind, &payload)
        )));
    }
    let reason = reason_of(args);
    let quote = args
        .get("evidence_quote")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let (source_span, confidence) = if quote.is_empty() {
        (None, 0.5)
    } else {
        match locate_quote(ctx.conn, ctx.project_id, ctx.session_id, &quote)? {
            Some(span) => (Some(format!("{span}｜{quote}")), 0.75),
            None => {
                return Ok(ToolOutcome::error(
                    "evidence_quote 无法在项目原文或作者消息中逐字找到。请用 read_path/search 取得原文后逐字引用；若这是你的创意建议，请留空 evidence_quote。",
                ))
            }
        }
    };
    if let Some(object) = payload.as_object_mut() {
        object.insert("agentReason".into(), Value::String(reason));
        object.insert(
            "origin".into(),
            Value::String(
                if source_span.is_some() {
                    "evidence"
                } else {
                    "suggestion"
                }
                .into(),
            ),
        );
    }
    let title = crate::agent::proposal_title(kind, &payload);
    let candidate = crate::candidates::insert(
        ctx.conn,
        ctx.project_id,
        kind,
        payload,
        source_span.as_deref(),
        None,
        confidence,
    )?;
    link_candidate(ctx, &candidate.id)?;
    if let Some(span) = &source_span {
        let (source_ref, quote_text) = span.split_once('｜').unwrap_or((span.as_str(), ""));
        let (source_kind, source_id) = source_ref.split_once(':').unwrap_or(("source", source_ref));
        ctx.conn.execute(
            "INSERT INTO agent_evidence
             (id, run_id, step_id, source_kind, source_id, source_span, quote, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                uuid::Uuid::new_v4().to_string(),
                ctx.run_id,
                ctx.step_id,
                source_kind,
                source_id,
                source_ref,
                quote_text,
                now()
            ],
        )?;
    }
    Ok(ToolOutcome {
        content: format!(
            "已创建候选 {}（{}，风险 {}{}）。它在作者确认前不会进入权威资料。",
            candidate.id,
            title,
            candidate.risk_level,
            if source_span.is_some() {
                "，附原文证据"
            } else {
                "，创意建议"
            }
        ),
        summary: format!("{} · {}", kind_label(kind), title),
        candidate_ids: vec![candidate.id],
        ..ToolOutcome::default()
    })
}

fn propose_craft(ctx: &ToolContext<'_>, args: &Value) -> AppResult<ToolOutcome> {
    let number = args
        .get("chapter")
        .and_then(Value::as_i64)
        .ok_or_else(|| AppError::Message("缺少章节序号".into()))? as i32;
    let node = crate::outline::list(ctx.conn, ctx.project_id)?
        .into_iter()
        .find(|node| node.level == "chapter" && node.order_no == number)
        .ok_or_else(|| AppError::Message(format!("第 {number} 章没有骨架节点")))?;
    let mut changes = serde_json::Map::new();
    for field in [
        "valueShift",
        "crisis",
        "hook",
        "reveal",
        "goal",
        "conflict",
        "turn",
        "payoff",
    ] {
        if let Some(value) = args.get(field).and_then(Value::as_str).map(str::trim) {
            if !value.is_empty() {
                changes.insert(field.into(), Value::String(value.into()));
            }
        }
    }
    if changes.is_empty() {
        return Ok(ToolOutcome::error("没有给出任何要修订的字段"));
    }
    let payload = json!({
        "outlineNodeId": node.id,
        "chapter": number,
        "title": node.title,
        "changes": changes,
        "before": {
            "valueShift": node.value_shift,
            "crisis": node.crisis,
            "hook": node.hook,
            "reveal": node.reveal,
            "goal": node.goal,
            "conflict": node.conflict,
            "turn": node.turn,
            "payoff": node.payoff
        },
        "baseUpdatedAt": node.updated_at,
        "agentReason": reason_of(args)
    });
    let candidate = crate::candidates::insert(
        ctx.conn,
        ctx.project_id,
        "craft_patch",
        payload,
        None,
        None,
        0.6,
    )?;
    link_candidate(ctx, &candidate.id)?;
    Ok(ToolOutcome {
        content: format!(
            "已创建第 {number} 章工艺卡修订候选 {}，等待作者确认。",
            candidate.id
        ),
        summary: format!("第 {number} 章 · {} 个字段", changes.len()),
        candidate_ids: vec![candidate.id],
        ..ToolOutcome::default()
    })
}

/// 蓝本字段名（camelCase）→ 列名；只开放文本字段，结构（关系、边）仍由作者在蓝本面板编辑。
pub(crate) fn blueprint_patch_column(target: &str, field: &str) -> Option<&'static str> {
    match (target, field) {
        ("element", "summary") => Some("summary"),
        ("element", "role") => Some("role"),
        ("element", "externalGoal") => Some("external_goal"),
        ("element", "internalNeed") => Some("internal_need"),
        ("element", "fear") => Some("fear"),
        ("element", "flaw") => Some("flaw"),
        ("element", "leverage") => Some("leverage"),
        ("element", "secret") => Some("secret"),
        ("node", "title") => Some("title"),
        ("node", "cause") => Some("cause"),
        ("node", "choice") => Some("choice"),
        ("node", "cost") => Some("cost"),
        ("node", "outcome") => Some("outcome"),
        ("node", "stateChange") => Some("state_change"),
        ("node", "knowledgeChange") => Some("knowledge_change"),
        ("node", "storyTime") => Some("story_time"),
        ("node", "location") => Some("location"),
        _ => None,
    }
}

fn propose_blueprint_patch(ctx: &ToolContext<'_>, args: &Value) -> AppResult<ToolOutcome> {
    if crate::blueprint::current(ctx.conn, ctx.project_id)?.is_some() {
        return Ok(ToolOutcome::error(
            "蓝本已确认，不能再静默修改；请让作者在蓝本面板中处理。",
        ));
    }
    let Some(view) = crate::blueprint::candidate(ctx.conn, ctx.project_id)? else {
        return Ok(ToolOutcome::error("还没有候选蓝本。"));
    };
    let requested = args
        .get("changes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if requested.is_empty() {
        return Ok(ToolOutcome::error("没有给出任何修订"));
    }
    let mut changes = Vec::new();
    let mut problems = Vec::new();
    for change in &requested {
        let target = change
            .get("target")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let field = change
            .get("field")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let value = change
            .get("value")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        if value.is_empty() || blueprint_patch_column(target, field).is_none() {
            problems.push(format!("{target}.{field}：字段不可修订或内容为空"));
            continue;
        }
        let (id, label, before) = if target == "element" {
            let name = change
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let Some(element) = view.elements.iter().find(|item| item.name == name) else {
                problems.push(format!("找不到条目「{name}」"));
                continue;
            };
            let before = match field {
                "summary" => &element.summary,
                "role" => &element.role,
                "externalGoal" => &element.external_goal,
                "internalNeed" => &element.internal_need,
                "fear" => &element.fear,
                "flaw" => &element.flaw,
                "leverage" => &element.leverage,
                _ => &element.secret,
            };
            (element.id.clone(), element.name.clone(), before.clone())
        } else {
            let order = change.get("order").and_then(Value::as_i64).unwrap_or(-1) as i32;
            let Some(node) = view.nodes.iter().find(|item| item.order_no == order) else {
                problems.push(format!("找不到第 {order} 号节点"));
                continue;
            };
            let before = match field {
                "title" => &node.title,
                "cause" => &node.cause,
                "choice" => &node.choice,
                "cost" => &node.cost,
                "outcome" => &node.outcome,
                "stateChange" => &node.state_change,
                "knowledgeChange" => &node.knowledge_change,
                "storyTime" => &node.story_time,
                _ => &node.location,
            };
            (
                node.id.clone(),
                format!("N{} {}", node.order_no, node.title),
                before.clone(),
            )
        };
        changes.push(json!({
            "target": target,
            "id": id,
            "label": label,
            "field": field,
            "before": before,
            "value": value
        }));
    }
    if changes.is_empty() {
        return Ok(ToolOutcome::error(format!(
            "没有可用的修订：{}",
            problems.join("；")
        )));
    }
    let count = changes.len();
    let candidate = crate::candidates::insert(
        ctx.conn,
        ctx.project_id,
        "blueprint_patch",
        json!({
            "blueprintId": view.blueprint.id,
            "title": view.blueprint.title,
            "changes": changes,
            "agentReason": reason_of(args)
        }),
        None,
        None,
        0.6,
    )?;
    link_candidate(ctx, &candidate.id)?;
    let mut content = format!(
        "已创建蓝本修订候选 {}（{count} 处），等待作者确认。",
        candidate.id
    );
    if !problems.is_empty() {
        content.push_str(&format!("\n未采用：{}", problems.join("；")));
    }
    Ok(ToolOutcome {
        content,
        summary: format!("{count} 处修订"),
        candidate_ids: vec![candidate.id],
        ..ToolOutcome::default()
    })
}

fn propose_thread(ctx: &ToolContext<'_>, args: &Value) -> AppResult<ToolOutcome> {
    let input = StoryThreadInput {
        id: None,
        kind: args
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
        title: args
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .into(),
        question: args
            .get("question")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .into(),
        open_chapter: args
            .get("openChapter")
            .and_then(Value::as_i64)
            .map(|v| v as i32),
        close_chapter: args
            .get("closeChapter")
            .and_then(Value::as_i64)
            .map(|v| v as i32),
        status: if args.get("closeChapter").and_then(Value::as_i64).is_some() {
            "closed".into()
        } else {
            "open".into()
        },
        notes: reason_of(args),
    };
    if input.title.is_empty() || !crate::craft::THREAD_KINDS.contains(&input.kind.as_str()) {
        return Ok(ToolOutcome::error("线程需要合法的 kind 和 title"));
    }
    let exists: bool = ctx.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM story_thread WHERE project_id=?1 AND title=?2
           UNION ALL SELECT 1 FROM candidate WHERE project_id=?1 AND kind='story_thread'
             AND status='pending' AND json_extract(payload,'$.title')=?2)",
        params![ctx.project_id, input.title],
        |row| row.get(0),
    )?;
    if exists {
        return Ok(ToolOutcome::error(format!(
            "线程「{}」已存在或已在候选区",
            input.title
        )));
    }
    let payload = serde_json::to_value(&input)?;
    let candidate = crate::candidates::insert(
        ctx.conn,
        ctx.project_id,
        "story_thread",
        payload,
        None,
        None,
        0.6,
    )?;
    link_candidate(ctx, &candidate.id)?;
    Ok(ToolOutcome {
        content: format!("已创建叙事线程候选 {}。", candidate.id),
        summary: format!(
            "{} · {}",
            crate::craft::thread_kind_label(&input.kind),
            input.title
        ),
        candidate_ids: vec![candidate.id],
        ..ToolOutcome::default()
    })
}

fn propose_draft(ctx: &ToolContext<'_>, args: &Value) -> AppResult<ToolOutcome> {
    let number = args
        .get("chapter")
        .and_then(Value::as_i64)
        .ok_or_else(|| AppError::Message("缺少章节序号".into()))? as i32;
    let text = args
        .get("text")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| AppError::Message("草稿正文为空".into()))?;
    let mode = args.get("mode").and_then(Value::as_str).unwrap_or("append");
    let chapter = crate::scenes::chapters(ctx.conn, ctx.project_id)?
        .into_iter()
        .find(|chapter| chapter.number == number)
        .ok_or_else(|| {
            AppError::Message(format!("第 {number} 章尚未建立；请先确认卷章骨架中的该章"))
        })?;
    let existing = crate::scenes::chapter_text(ctx.conn, ctx.project_id, &chapter.id)?;
    let full = if mode == "replace" || existing.trim().is_empty() {
        text.to_string()
    } else {
        format!("{}\n\n{}", existing.trim_end(), text)
    };
    let payload = json!({
        "text": full,
        "excerpt": text,
        "mode": mode,
        "source": "agent",
        "chapterId": chapter.id,
        "chapterNumber": number,
        "baseRevisionId": chapter.current_revision_id,
        "wordCount": text.chars().count(),
        "note": args.get("note").and_then(Value::as_str).unwrap_or_default(),
        "sessionId": ctx.session_id,
        "styleProfileId": null
    });
    let candidate = crate::candidates::insert(
        ctx.conn,
        ctx.project_id,
        "draft",
        payload,
        Some(&format!("agent:{}", ctx.run_id)),
        None,
        0.6,
    )?;
    link_candidate(ctx, &candidate.id)?;
    Ok(ToolOutcome {
        content: format!(
            "已创建第 {number} 章正文草稿候选 {}（{} 字，{}）。作者在正文页确认后才会生成新版本。",
            candidate.id,
            text.chars().count(),
            if mode == "replace" {
                "整章替换"
            } else {
                "续写"
            }
        ),
        summary: format!("第 {number} 章 · {} 字", text.chars().count()),
        candidate_ids: vec![candidate.id],
        ..ToolOutcome::default()
    })
}

fn propose_guide(ctx: &ToolContext<'_>, args: &Value) -> AppResult<ToolOutcome> {
    let content = args
        .get("content")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| AppError::Message("守则内容为空".into()))?;
    let current = crate::craft::guide(ctx.conn, ctx.project_id)?;
    let payload = json!({
        "content": content,
        "before": current.content,
        "baseUpdatedAt": current.updated_at,
        "agentReason": reason_of(args)
    });
    let candidate = crate::candidates::insert(
        ctx.conn,
        ctx.project_id,
        "project_guide",
        payload,
        None,
        None,
        0.6,
    )?;
    link_candidate(ctx, &candidate.id)?;
    Ok(ToolOutcome {
        content: format!("已创建作者守则修订候选 {}。", candidate.id),
        summary: format!("{} 字", content.chars().count()),
        candidate_ids: vec![candidate.id],
        ..ToolOutcome::default()
    })
}

fn update_plan(args: &Value) -> AppResult<ToolOutcome> {
    let items: Vec<Value> = args
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .take(12)
        .filter_map(|item| {
            let title = item
                .get("title")
                .and_then(Value::as_str)?
                .trim()
                .to_string();
            if title.is_empty() {
                return None;
            }
            let status = match item.get("status").and_then(Value::as_str) {
                Some("done") => "done",
                Some("in_progress") => "in_progress",
                _ => "pending",
            };
            Some(json!({"title": title, "status": status}))
        })
        .collect();
    let done = items.iter().filter(|item| item["status"] == "done").count();
    Ok(ToolOutcome {
        content: "计划已更新。".into(),
        summary: format!("{done}/{} 步完成", items.len()),
        plan: Some(Value::Array(items)),
        ..ToolOutcome::default()
    })
}

fn ask_author(args: &Value) -> AppResult<ToolOutcome> {
    let question = args
        .get("question")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| AppError::Message("问题为空".into()))?;
    let options: Vec<String> = args
        .get("options")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .take(4)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    Ok(ToolOutcome {
        content: "问题已交给作者，本轮结束，等待作者选择。".into(),
        summary: question.to_string(),
        terminal: true,
        action: Some(json!({"type":"ask_author","question":question,"options":options})),
        ..ToolOutcome::default()
    })
}

fn propose_domain(ctx: &ToolContext<'_>, args: &Value) -> AppResult<ToolOutcome> {
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| AppError::Message("知识域名称为空".into()))?;
    let exists: bool = ctx.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM knowledge_domain WHERE project_id=?1 AND name=?2
           UNION ALL SELECT 1 FROM candidate WHERE project_id=?1 AND kind='knowledge_domain'
             AND status='pending' AND json_extract(payload,'$.name')=?2)",
        params![ctx.project_id, name],
        |row| row.get(0),
    )?;
    if exists {
        return Ok(ToolOutcome::error(format!(
            "知识域「{name}」已存在或已在候选区"
        )));
    }
    let fields: Vec<String> = args
        .get("fields")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .take(12)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let payload = json!({
        "name": name,
        "description": args.get("description").and_then(Value::as_str).unwrap_or_default(),
        "fields": fields,
        "agentReason": reason_of(args)
    });
    let candidate = crate::candidates::insert(
        ctx.conn,
        ctx.project_id,
        "knowledge_domain",
        payload,
        None,
        None,
        0.6,
    )?;
    link_candidate(ctx, &candidate.id)?;
    Ok(ToolOutcome {
        content: format!(
            "已创建知识域候选 {}；确认后才能向该域提出条目。",
            candidate.id
        ),
        summary: format!("{name} · {} 个字段", fields.len()),
        candidate_ids: vec![candidate.id],
        ..ToolOutcome::default()
    })
}

fn meta_text(metadata: &Value, key: &str) -> String {
    match metadata.get(key) {
        Some(Value::String(text)) => text.trim().to_string(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

fn join_nonempty(parts: &[(&str, String)]) -> String {
    parts
        .iter()
        .filter(|(_, value)| !value.trim().is_empty())
        .map(|(label, value)| format!("**{label}**：{}", value.trim()))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn power_tiers(metadata: &Value) -> String {
    let Some(tiers) = metadata.get("tiers").and_then(Value::as_array) else {
        return meta_text(metadata, "tiers");
    };
    tiers
        .iter()
        .map(|tier| {
            let text = |key: &str| tier.get(key).and_then(Value::as_str).unwrap_or_default();
            format!(
                "- **{} {}**：{}；代价：{}{}",
                text("rank"),
                text("name"),
                text("abilities"),
                text("cost"),
                if text("risk").is_empty() {
                    String::new()
                } else {
                    format!("；风险：{}", text("risk"))
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn relations_text(relations: &Value) -> String {
    relations
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let text =
                        |key: &str| item.get(key).and_then(Value::as_str).unwrap_or_default();
                    format!(
                        "- {}（{}）：{}",
                        text("target"),
                        text("type"),
                        text("tension")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// 已确认蓝本 → 资料库候选。蓝本本身是作者确认过的来源，候选附 blueprint 来源片段。
fn sync_blueprint(ctx: &ToolContext<'_>) -> AppResult<ToolOutcome> {
    let Some(view) = crate::blueprint::current(ctx.conn, ctx.project_id)? else {
        return Ok(ToolOutcome::error(
            "还没有已确认的叙事蓝本。请先让作者在确认卡中确认蓝本。",
        ));
    };
    let span = |name: &str, summary: &str| {
        format!(
            "blueprint:{}｜{}",
            view.blueprint.id,
            if summary.trim().is_empty() {
                name
            } else {
                summary.trim()
            }
        )
    };
    let mut proposals: Vec<(String, Value, String)> = Vec::new();
    let mut relation_proposals = Vec::new();
    for element in &view.elements {
        let meta = &element.metadata;
        let constraints = element.constraints.join("；");
        let (kind, payload) = match element.kind.as_str() {
            "character" => {
                let description = join_nonempty(&[
                    ("身份", element.summary.clone()),
                    ("结构功能", element.role.clone()),
                    ("所属势力", meta_text(meta, "faction")),
                    ("能力", meta_text(meta, "ability")),
                    ("背景", meta_text(meta, "background")),
                    ("弧光", meta_text(meta, "arc")),
                    ("说话方式", meta_text(meta, "voice")),
                    ("错误信念", meta_text(meta, "misbelief")),
                    ("筹码", element.leverage.clone()),
                ]);
                (
                    "entity",
                    json!({
                        "type":"character",
                        "canonicalName": element.name,
                        "aliases": [],
                        "description": description,
                        "desire": element.external_goal,
                        "need": element.internal_need,
                        "fear": element.fear,
                        "flaw": element.flaw,
                        "secret": element.secret,
                        "taboo": element.constraints,
                    }),
                )
            }
            "faction" => (
                "knowledge_entry",
                json!({
                    "domainKey":"faction",
                    "name": element.name,
                    "aliases": [],
                    "summary": element.summary,
                    "fields": {
                        "structure": meta_text(meta, "structure"),
                        "leader": meta_text(meta, "leader"),
                        "doctrine": join_nonempty(&[("信条", meta_text(meta, "doctrine")), ("当前目标", element.external_goal.clone()), ("真正所缺", element.internal_need.clone())]),
                        "resources": join_nonempty(&[("资源", meta_text(meta, "resources")), ("地盘", meta_text(meta, "territory")), ("能力渠道", meta_text(meta, "powerAccess")), ("筹码", element.leverage.clone())]),
                        "methods": join_nonempty(&[("手段", meta_text(meta, "methods")), ("招募", meta_text(meta, "recruitment")), ("底线", constraints.clone())]),
                        "schism": join_nonempty(&[("内部裂痕", meta_text(meta, "schism")), ("组织病", element.flaw.clone()), ("秘密", element.secret.clone())]),
                        "relations": relations_text(&element.relations),
                    }
                }),
            ),
            "concept" if meta.get("tiers").is_some() || meta.get("system").is_some() => (
                "knowledge_entry",
                json!({
                    "domainKey":"power",
                    "name": element.name,
                    "aliases": [],
                    "summary": element.summary,
                    "fields": {
                        "tiers": power_tiers(meta),
                        "acquisition": meta_text(meta, "acquisition"),
                        "cost": meta_text(meta, "cost"),
                        "limits": join_nonempty(&[("边界", meta_text(meta, "limits")), ("克制", meta_text(meta, "counters")), ("约束", constraints.clone())]),
                        "society": meta_text(meta, "socialImpact"),
                    }
                }),
            ),
            "world_rule" | "concept" => (
                "knowledge_entry",
                json!({
                    "domainKey":"worldrule",
                    "name": element.name,
                    "aliases": [],
                    "summary": element.summary,
                    "fields": {
                        "content": element.role,
                        "limits": constraints,
                        "cost": element.leverage,
                        "truth": element.secret,
                        "impact": relations_text(&element.relations),
                    }
                }),
            ),
            "location" => (
                "knowledge_entry",
                json!({
                    "domainKey":"environment",
                    "name": element.name,
                    "aliases": [],
                    "summary": element.summary,
                    "fields": {
                        "category": element.role,
                        "geography": relations_text(&element.relations),
                        "rules": join_nonempty(&[("限制", constraints.clone()), ("秘密", element.secret.clone())]),
                    }
                }),
            ),
            "item" => (
                "knowledge_entry",
                json!({
                    "domainKey":"item",
                    "name": element.name,
                    "aliases": [],
                    "summary": element.summary,
                    "fields": {
                        "function": element.leverage,
                        "ownership": relations_text(&element.relations),
                        "limits": join_nonempty(&[("限制", constraints.clone()), ("秘密", element.secret.clone())]),
                        "storyRole": element.role,
                    }
                }),
            ),
            _ => continue,
        };
        proposals.push((
            kind.to_string(),
            payload,
            span(&element.name, &element.summary),
        ));
        if element.kind == "character" || element.kind == "faction" {
            for relation in element.relations.as_array().into_iter().flatten() {
                let target = relation
                    .get("target")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if target.is_empty() {
                    continue;
                }
                relation_proposals.push((
                    "relation".to_string(),
                    json!({
                        "from": element.name,
                        "to": target,
                        "type": relation.get("type").and_then(Value::as_str).unwrap_or("关联"),
                        "tension": relation.get("tension").and_then(Value::as_str).unwrap_or_default(),
                    }),
                    span(&element.name, &element.summary),
                ));
            }
        }
    }
    for event in &view.timeline {
        proposals.push((
            "event".into(),
            json!({
                "title": event.title,
                "storyTime": event.story_time,
                "participants": event.participants,
                "cause": event.cause,
                "outcome": format!("{}{}", event.outcome, if event.consequence.is_empty() { String::new() } else { format!("；长期后果：{}", event.consequence) }),
                "revealAt": event.reveal_at,
            }),
            span(&event.title, &event.cause),
        ));
    }
    // 关系依赖两端实体，放在最后，作者“全部确认”时按顺序先建实体。
    proposals.extend(relation_proposals);
    let mut created = Vec::new();
    let mut skipped = 0;
    for (kind, payload, source_span) in proposals {
        if crate::agent::validate_proposal(&kind, &payload).is_err()
            || crate::agent::duplicate_proposal(ctx.conn, ctx.project_id, &kind, &payload)?
        {
            skipped += 1;
            continue;
        }
        let candidate = crate::candidates::insert(
            ctx.conn,
            ctx.project_id,
            &kind,
            payload,
            Some(&source_span),
            None,
            0.8,
        )?;
        link_candidate(ctx, &candidate.id)?;
        created.push(candidate.id);
    }
    Ok(ToolOutcome {
        content: format!(
            "已把已确认蓝本整理为 {} 项资料库候选（人物、势力、能力体系、世界规则、地点、物品、事件、关系），跳过 {skipped} 项已存在或不完整的条目。作者在确认卡中确认后，它们会出现在人物页、知识库和时间线。",
            created.len()
        ),
        summary: format!("{} 项候选 · 跳过 {skipped}", created.len()),
        candidate_ids: created,
        ..ToolOutcome::default()
    })
}

// ---------- 运行记录 ----------

pub fn create_run(
    conn: &Connection,
    project_id: &str,
    session_id: &str,
    goal: &str,
    mode: &str,
    model: &str,
) -> AppResult<String> {
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now();
    conn.execute(
        "INSERT INTO agent_run
         (id, project_id, session_id, mode, scope, output, goal, status, plan_json,
          current_step, approval_policy, model, started_at, created_at, updated_at)
         VALUES (?1,?2,?3,'harness','project',?4,?5,'running','[]',0,'manual',?6,?7,?7,?7)",
        params![
            id,
            project_id,
            session_id,
            normalize_mode(mode),
            goal,
            model,
            ts
        ],
    )?;
    Ok(id)
}

pub fn next_seq(conn: &Connection, run_id: &str) -> AppResult<i32> {
    Ok(conn.query_row(
        "SELECT COALESCE(MAX(seq), 0) + 1 FROM agent_step WHERE run_id=?1",
        [run_id],
        |row| row.get(0),
    )?)
}

pub fn begin_step(conn: &Connection, run_id: &str, call: &ToolCall) -> AppResult<(String, i32)> {
    let id = uuid::Uuid::new_v4().to_string();
    let seq = next_seq(conn, run_id)?;
    let tier = find_tool(&call.name)
        .map(|tool| tool.tier)
        .unwrap_or(Tier::Read);
    let input: Value = crate::llm::parse_json_loose(&call.arguments).unwrap_or_else(|_| json!({}));
    conn.execute(
        "INSERT INTO agent_step
         (id, run_id, seq, kind, tool_name, input_json, status, risk_level, requires_approval, started_at)
         VALUES (?1,?2,?3,'tool',?4,?5,'running',?6,?7,?8)",
        params![
            id,
            run_id,
            seq,
            call.name,
            input.to_string(),
            tier.as_str(),
            i32::from(tier == Tier::Propose),
            now()
        ],
    )?;
    conn.execute(
        "UPDATE agent_run SET current_step=?2, updated_at=?3 WHERE id=?1",
        params![run_id, seq, now()],
    )?;
    Ok((id, seq))
}

pub fn finish_step(
    conn: &Connection,
    step_id: &str,
    title: &str,
    outcome: &ToolOutcome,
) -> AppResult<()> {
    conn.execute(
        "UPDATE agent_step SET status=?2, output_json=?3, candidate_ids=?4, error=?5, finished_at=?6
         WHERE id=?1",
        params![
            step_id,
            if outcome.failed { "failed" } else { "completed" },
            json!({
                "title": title,
                "summary": outcome.summary,
                "content": clip(&outcome.content, 1200),
                "action": outcome.action
            })
            .to_string(),
            serde_json::to_string(&outcome.candidate_ids)?,
            if outcome.failed { outcome.summary.as_str() } else { "" },
            now()
        ],
    )?;
    Ok(())
}

pub fn save_plan(conn: &Connection, run_id: &str, plan: &Value) -> AppResult<()> {
    conn.execute(
        "UPDATE agent_run SET plan_json=?2, updated_at=?3 WHERE id=?1",
        params![run_id, plan.to_string(), now()],
    )?;
    Ok(())
}

pub fn finish_run(
    conn: &Connection,
    project_id: &str,
    run_id: &str,
    status: &str,
    error: &str,
) -> AppResult<()> {
    let pending: i64 = conn.query_row(
        "SELECT COUNT(*) FROM candidate WHERE agent_run_id=?1 AND status='pending'",
        [run_id],
        |row| row.get(0),
    )?;
    let status = if status == "completed" && pending > 0 {
        "waiting_approval"
    } else {
        status
    };
    conn.execute(
        "UPDATE agent_run SET status=?3, error=?4, finished_at=?5, updated_at=?5
         WHERE project_id=?1 AND id=?2",
        params![project_id, run_id, status, error, now()],
    )?;
    Ok(())
}

pub fn traces(conn: &Connection, run_id: &str) -> AppResult<Vec<HarnessToolTrace>> {
    let mut stmt = conn.prepare(
        "SELECT id, seq, tool_name, input_json, output_json, status, candidate_ids
         FROM agent_step WHERE run_id=?1 ORDER BY seq",
    )?;
    let rows = stmt
        .query_map([run_id], |row| {
            let input: Value =
                serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_else(|_| json!({}));
            let output: Value =
                serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_else(|_| json!({}));
            let tool_name: String = row.get(2)?;
            Ok(HarnessToolTrace {
                step_id: row.get(0)?,
                seq: row.get(1)?,
                title: output
                    .get("title")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| tool_title(&tool_name, &input)),
                tool_name,
                input,
                summary: output
                    .get("summary")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                status: row.get(5)?,
                candidate_ids: serde_json::from_str(&row.get::<_, String>(6)?).unwrap_or_default(),
                action: output.get("action").cloned().unwrap_or(Value::Null),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn run_actions(conn: &Connection, run_id: &str) -> AppResult<Value> {
    let mut stmt =
        conn.prepare("SELECT output_json FROM agent_step WHERE run_id=?1 ORDER BY seq")?;
    let actions = stmt
        .query_map([run_id], |row| row.get::<_, String>(0))?
        .filter_map(Result::ok)
        .filter_map(|raw| serde_json::from_str::<Value>(&raw).ok())
        .filter_map(|output| {
            output
                .get("action")
                .cloned()
                .filter(|value| !value.is_null())
        })
        .collect();
    Ok(Value::Array(actions))
}

/// 供 @ 补全使用：项目内可引用的虚拟路径。
pub fn workspace_paths(conn: &Connection, project_id: &str) -> AppResult<Vec<String>> {
    let mut paths: Vec<String> = ROOT_ENTRIES
        .iter()
        .map(|(name, _)| format!("/{name}"))
        .collect();
    for chapter in crate::scenes::chapters(conn, project_id)? {
        paths.push(format!("/章节/{}", chapter.number));
    }
    for entity in crate::entities::list(conn, project_id, None)? {
        let folder = if entity.entity_type == "character" {
            "人物"
        } else {
            "设定"
        };
        paths.push(format!("/{folder}/{}", entity.canonical_name));
    }
    for domain in crate::knowledge::domains(conn, project_id)? {
        for entry in crate::knowledge::entries(conn, project_id, Some(&domain.id))? {
            paths.push(format!("/知识/{}/{}", domain.name, entry.name));
        }
    }
    for promise in crate::promises::list(conn, project_id)? {
        paths.push(format!("/伏笔/{}", promise.title));
    }
    Ok(paths)
}

/// 作者在输入里写的 @路径 会被预先读取，作为本轮附件交给模型。
pub fn mentioned_paths(text: &str) -> Vec<String> {
    let pattern = Regex::new(r"@(/[^\s，。,;；:：！？!?]+)").expect("valid regex");
    let mut paths = Vec::new();
    for capture in pattern.captures_iter(text) {
        let path = capture[1].trim_end_matches(['、', '）', ')']).to_string();
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    paths.truncate(6);
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_mode_hides_and_blocks_propose_tools() {
        let schema = tools_schema("plan");
        let names: Vec<&str> = schema
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|tool| tool.pointer("/function/name").and_then(Value::as_str))
            .collect();
        assert!(names.contains(&"read_path"));
        assert!(!names.iter().any(|name| name.starts_with("propose_")));
        assert!(!names.iter().any(|name| name.starts_with("generate_")));
        assert!(names.contains(&"rebuild_wiki"));
        assert!(!allowed("plan", Tier::Propose));
        assert!(allowed("write", Tier::Propose));
    }

    #[test]
    fn registry_has_no_authoritative_write_tools() {
        for tool in registry() {
            for forbidden in ["accept", "confirm", "commit", "delete", "apply"] {
                assert!(
                    !tool.name.contains(forbidden),
                    "{} must not be exposed to the model",
                    tool.name
                );
            }
        }
    }

    #[test]
    fn parses_text_protocol_calls() {
        let raw = "我先看看人物。\n<tool_call>{\"name\":\"read_path\",\"arguments\":{\"path\":\"/人物/林澈\"}}</tool_call>";
        let (visible, calls) = parse_text_tool_calls(raw);
        assert_eq!(visible, "我先看看人物。");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read_path");
        assert!(calls[0].arguments.contains("林澈"));
    }

    #[test]
    fn extracts_mentioned_paths() {
        let paths = mentioned_paths("看看 @/人物/林澈 和 @/章节/3，再对比@/守则。");
        assert_eq!(paths, vec!["/人物/林澈", "/章节/3", "/守则"]);
    }

    fn fixture() -> (tempfile::TempDir, crate::project::LoadedProject, String) {
        let dir = tempfile::tempdir().unwrap();
        let loaded = crate::project::create(&crate::domain::CreateProjectInput {
            name: "harness".into(),
            root_path: dir.path().join("p").to_string_lossy().into_owned(),
            mode: "both".into(),
            writing_language: None,
        })
        .unwrap();
        let pid = loaded.summary.id.clone();
        let text = "林澈在旧车站找到黄铜钥匙。守夜人要求他交出钥匙。";
        loaded
            .conn
            .execute(
                "INSERT INTO text_fts
                 (content, tokenized, source_id, project_id, source_kind, chapter_id, branch_id)
                 VALUES (?1,?2,'source-1',?3,'import',NULL,NULL)",
                params![text, crate::retrieval::tokenize(text), pid],
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
        (dir, loaded, session.id)
    }

    fn call(name: &str, args: Value) -> ToolCall {
        ToolCall {
            id: "c".into(),
            name: name.into(),
            arguments: args.to_string(),
        }
    }

    fn run_tool(
        loaded: &crate::project::LoadedProject,
        session_id: &str,
        run_id: &str,
        mode: &str,
        call: &ToolCall,
    ) -> ToolOutcome {
        let (step_id, _) = begin_step(&loaded.conn, run_id, call).unwrap();
        let ctx = ToolContext {
            conn: &loaded.conn,
            project_id: &loaded.summary.id,
            session_id,
            run_id,
            step_id: &step_id,
            mode,
        };
        let outcome = execute(&ctx, call);
        finish_step(
            &loaded.conn,
            &step_id,
            &tool_title(&call.name, &json!({})),
            &outcome,
        )
        .unwrap();
        outcome
    }

    #[test]
    fn existing_characters_are_revised_through_entity_patch() {
        let (_dir, mut loaded, session_id) = fixture();
        let pid = loaded.summary.id.clone();
        let run_id = create_run(&loaded.conn, &pid, &session_id, "人物", "write", "m").unwrap();
        let created = run_tool(
            &loaded,
            &session_id,
            &run_id,
            "write",
            &call(
                "propose_change",
                json!({"kind":"entity","payload":{"type":"character","canonicalName":"林澈","description":"拾钥人","secret":"旧秘密"},"reason":"r"}),
            ),
        );
        crate::candidates::accept(&mut loaded.conn, &pid, &created.candidate_ids[0]).unwrap();
        let duplicate = run_tool(
            &loaded,
            &session_id,
            &run_id,
            "write",
            &call(
                "propose_change",
                json!({"kind":"entity","payload":{"type":"character","canonicalName":"林澈"},"reason":"r"}),
            ),
        );
        assert!(duplicate.content.contains("entity_patch"));
        let patch = run_tool(
            &loaded,
            &session_id,
            &run_id,
            "write",
            &call(
                "propose_change",
                json!({"kind":"entity_patch","payload":{"canonicalName":"林澈","secret":"他在封箱夜写下了第一页"},"reason":"对齐蓝本"}),
            ),
        );
        assert_eq!(patch.candidate_ids.len(), 1, "{}", patch.content);
        crate::candidates::accept(&mut loaded.conn, &pid, &patch.candidate_ids[0]).unwrap();
        let secret: String = loaded
            .conn
            .query_row(
                "SELECT c.secret FROM character_core c JOIN entity e ON e.id=c.entity_id WHERE e.canonical_name='林澈'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(secret, "他在封箱夜写下了第一页");
    }

    #[test]
    fn detects_leaked_tool_markup() {
        assert!(leaked_tool_markup(
            "<｜｜DSML｜｜ invoke name=\"read_path\">\n<｜｜DSML｜｜ parameter name=\"path\" string=\"true\">/人物</｜｜DSML｜｜ parameter>"
        ));
        assert!(!leaked_tool_markup("已登记四个事件，等待你确认。"));
    }

    #[test]
    fn long_paths_page_with_offset() {
        let (_dir, loaded, session_id) = fixture();
        let pid = loaded.summary.id.clone();
        crate::craft::save_guide(&loaded.conn, &pid, &"守".repeat(7000)).unwrap();
        let run_id = create_run(&loaded.conn, &pid, &session_id, "读守则", "plan", "m").unwrap();
        let first = run_tool(
            &loaded,
            &session_id,
            &run_id,
            "plan",
            &call("read_path", json!({"path":"/守则"})),
        );
        assert!(
            first.content.contains("继续读取用 offset=3000"),
            "{}",
            first.content
        );
        let last = run_tool(
            &loaded,
            &session_id,
            &run_id,
            "plan",
            &call("read_path", json!({"path":"/守则","offset":6000})),
        );
        assert!(last.content.contains("已读到结尾"));
        assert_eq!(last.content.chars().filter(|ch| *ch == '守').count(), 1000);
    }

    #[test]
    fn tools_read_project_and_only_create_candidates() {
        let (_dir, loaded, session_id) = fixture();
        let pid = loaded.summary.id.clone();
        let run_id = create_run(&loaded.conn, &pid, &session_id, "整理人物", "write", "m").unwrap();

        let root = run_tool(
            &loaded,
            &session_id,
            &run_id,
            "write",
            &call("list_paths", json!({"path":"/"})),
        );
        assert!(root.content.contains("/人物") && root.content.contains("/流水线"));
        let hits = run_tool(
            &loaded,
            &session_id,
            &run_id,
            "write",
            &call("search", json!({"query":"黄铜钥匙"})),
        );
        assert!(hits.content.contains("黄铜钥匙"), "{}", hits.content);

        // 伪造引用被拒绝。
        let forged = run_tool(
            &loaded,
            &session_id,
            &run_id,
            "write",
            &call(
                "propose_change",
                json!({"kind":"entity","payload":{"type":"character","canonicalName":"林澈","description":"拾钥人"},
                       "evidence_quote":"林澈是皇帝的私生子","reason":"r"}),
            ),
        );
        assert!(forged.failed);

        // 逐字引用通过，并且只进入候选区。
        let ok = run_tool(
            &loaded,
            &session_id,
            &run_id,
            "write",
            &call(
                "propose_change",
                json!({"kind":"entity","payload":{"type":"character","canonicalName":"林澈","description":"拾钥人"},
                       "evidence_quote":"林澈在旧车站找到黄铜钥匙","reason":"原文出场"}),
            ),
        );
        assert!(!ok.failed, "{}", ok.content);
        assert_eq!(ok.candidate_ids.len(), 1);
        let entity_count: i64 = loaded
            .conn
            .query_row(
                "SELECT COUNT(*) FROM entity WHERE project_id=?1",
                [&pid],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(entity_count, 0);
        let (span, linked_run): (String, String) = loaded
            .conn
            .query_row(
                "SELECT source_span, agent_run_id FROM candidate WHERE id=?1",
                [&ok.candidate_ids[0]],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert!(span.starts_with("import:source-1｜"));
        assert_eq!(linked_run, run_id);

        // 重复提出被拦截。
        let dup = run_tool(
            &loaded,
            &session_id,
            &run_id,
            "write",
            &call(
                "propose_change",
                json!({"kind":"entity","payload":{"type":"character","canonicalName":"林澈"},"reason":"r"}),
            ),
        );
        assert!(dup.failed);

        // 规划模式不能提出候选。
        let blocked = run_tool(
            &loaded,
            &session_id,
            &run_id,
            "plan",
            &call("propose_guide", json!({"content":"不写穿越","reason":"r"})),
        );
        assert!(blocked.failed);

        let traces = traces(&loaded.conn, &run_id).unwrap();
        assert_eq!(traces.len(), 6);
        finish_run(&loaded.conn, &pid, &run_id, "completed", "").unwrap();
        let status: String = loaded
            .conn
            .query_row(
                "SELECT status FROM agent_run WHERE id=?1",
                [&run_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "waiting_approval");
    }

    #[test]
    fn guide_and_thread_candidates_apply_only_after_author_accepts() {
        let (_dir, mut loaded, session_id) = fixture();
        let pid = loaded.summary.id.clone();
        let run_id = create_run(&loaded.conn, &pid, &session_id, "守则", "write", "m").unwrap();
        let guide = run_tool(
            &loaded,
            &session_id,
            &run_id,
            "write",
            &call(
                "propose_guide",
                json!({"content":"第三人称限知，不写上帝视角。","reason":"r"}),
            ),
        );
        let thread = run_tool(
            &loaded,
            &session_id,
            &run_id,
            "write",
            &call(
                "propose_thread",
                json!({"kind":"inquiry","title":"钥匙开哪扇门","question":"黄铜钥匙属于谁？","openChapter":1,"reason":"r"}),
            ),
        );
        assert!(crate::craft::guide(&loaded.conn, &pid)
            .unwrap()
            .content
            .is_empty());
        assert!(crate::craft::threads(&loaded.conn, &pid)
            .unwrap()
            .is_empty());

        crate::candidates::accept(&mut loaded.conn, &pid, &guide.candidate_ids[0]).unwrap();
        crate::candidates::accept(&mut loaded.conn, &pid, &thread.candidate_ids[0]).unwrap();
        assert!(crate::craft::guide(&loaded.conn, &pid)
            .unwrap()
            .content
            .contains("限知"));
        let threads = crate::craft::threads(&loaded.conn, &pid).unwrap();
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].status, "open");

        // 守则被作者改过之后，旧候选不能覆盖新内容。
        let stale = run_tool(
            &loaded,
            &session_id,
            &run_id,
            "write",
            &call("propose_guide", json!({"content":"旧版守则","reason":"r"})),
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        crate::craft::save_guide(&loaded.conn, &pid, "作者刚刚改过").unwrap();
        assert!(
            crate::candidates::accept(&mut loaded.conn, &pid, &stale.candidate_ids[0]).is_err()
        );
        assert_eq!(
            crate::craft::guide(&loaded.conn, &pid).unwrap().content,
            "作者刚刚改过"
        );
    }

    #[test]
    fn character_core_and_domain_candidates_apply_on_accept() {
        let (_dir, mut loaded, session_id) = fixture();
        let pid = loaded.summary.id.clone();
        let run_id = create_run(&loaded.conn, &pid, &session_id, "人物", "write", "m").unwrap();
        let person = run_tool(
            &loaded,
            &session_id,
            &run_id,
            "write",
            &call(
                "propose_change",
                json!({"kind":"entity","payload":{"type":"character","canonicalName":"守夜人",
                    "description":"车站守夜者","desire":"守住秘密","fear":"钥匙落入外人手中",
                    "values":"忠诚、沉默"},"reason":"建议"}),
            ),
        );
        let domain = run_tool(
            &loaded,
            &session_id,
            &run_id,
            "write",
            &call(
                "propose_knowledge_domain",
                json!({"name":"禁墟","description":"封印之地","fields":["代价","克制"],"reason":"r"}),
            ),
        );
        assert!(
            !person.failed && !domain.failed,
            "{} {}",
            person.content,
            domain.content
        );
        crate::candidates::accept(&mut loaded.conn, &pid, &person.candidate_ids[0]).unwrap();
        crate::candidates::accept(&mut loaded.conn, &pid, &domain.candidate_ids[0]).unwrap();
        let (desire, values): (String, String) = loaded
            .conn
            .query_row(
                "SELECT c.desire, c.\"values\" FROM character_core c JOIN entity e ON e.id=c.entity_id
                 WHERE e.canonical_name='守夜人'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(desire, "守住秘密");
        assert_eq!(values, "[\"忠诚\",\"沉默\"]");
        assert!(crate::knowledge::domains(&loaded.conn, &pid)
            .unwrap()
            .iter()
            .any(|domain| domain.name == "禁墟"));
        // 工作流工具只能在命令层异步执行。
        let direct = run_tool(
            &loaded,
            &session_id,
            &run_id,
            "write",
            &call("generate_seed", json!({"logline":"测试"})),
        );
        assert!(direct.failed);
    }

    #[test]
    fn event_candidates_accept_without_reveal_time() {
        let (_dir, mut loaded, session_id) = fixture();
        let pid = loaded.summary.id.clone();
        let run_id = create_run(&loaded.conn, &pid, &session_id, "时间线", "write", "m").unwrap();
        let event = run_tool(
            &loaded,
            &session_id,
            &run_id,
            "write",
            &call(
                "propose_change",
                json!({"kind":"event","payload":{"title":"山塘镇矿难","storyTime":"十年前",
                    "participants":"孟秋、矿工","place":"山塘镇","cause":"违规开采","outcome":"全镇遗属抚恤被冻结"},
                    "reason":"建议"}),
            ),
        );
        assert!(!event.failed, "{}", event.content);
        crate::candidates::accept(&mut loaded.conn, &pid, &event.candidate_ids[0]).unwrap();
        let (reveal, participants): (String, String) = loaded
            .conn
            .query_row(
                "SELECT reveal_time, participants FROM event WHERE title='山塘镇矿难'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(reveal, "");
        assert_eq!(participants, "[\"孟秋\",\"矿工\"]");
    }
}
