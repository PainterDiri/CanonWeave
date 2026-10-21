use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    domain::ModelConfig,
    error::{AppError, AppResult},
    model::chat_completions_url,
};

pub const PROMPT_VERSION: &str = "cw-v1.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatTurn {
    pub role: String,
    pub content: String,
}

impl ChatTurn {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
        }
    }

    #[allow(dead_code)]
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
        }
    }
}

pub struct LlmClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    pub model: String,
    pub temperature: f64,
    pub max_tokens: i32,
}

#[derive(Debug, Clone)]
pub struct CompletionOutput {
    pub text: String,
    pub finish_reason: Option<String>,
    pub truncated: bool,
}

impl LlmClient {
    pub fn new(config: &ModelConfig, api_key: String) -> AppResult<Self> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(180))
            .build()
            .map_err(|e| AppError::Message(format!("初始化模型客户端失败：{e}")))?;
        Ok(Self {
            http,
            base_url: chat_completions_url(&config.base_url),
            api_key,
            model: config.model.clone(),
            temperature: config.temperature,
            max_tokens: config.max_tokens,
        })
    }

    pub async fn complete(
        &self,
        messages: &[ChatTurn],
        max_tokens: Option<i32>,
    ) -> AppResult<String> {
        let output = self.complete_with_meta(messages, max_tokens).await?;
        if output.truncated {
            return Err(AppError::Message(format!(
                "模型输出达到 max_tokens 上限（{}），结果不完整。请重试或提高 Max tokens。",
                output.finish_reason.as_deref().unwrap_or("length")
            )));
        }
        Ok(output.text)
    }

    pub async fn complete_with_meta(
        &self,
        messages: &[ChatTurn],
        max_tokens: Option<i32>,
    ) -> AppResult<CompletionOutput> {
        let response = self
            .http
            .post(&self.base_url)
            .bearer_auth(&self.api_key)
            .json(&json!({
                "model": self.model,
                "messages": messages,
                "temperature": self.temperature,
                "max_tokens": max_tokens.unwrap_or(self.max_tokens),
                "stream": false
            }))
            .send()
            .await
            .map_err(|e| AppError::Message(format!("模型请求失败：{e}")))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| AppError::Message(format!("读取模型响应失败：{e}")))?;
        if !status.is_success() {
            return Err(AppError::Message(format!(
                "模型服务返回 {status}：{}",
                body.chars().take(400).collect::<String>()
            )));
        }
        parse_completion_body(&body)
    }

    pub async fn stream<F>(
        &self,
        messages: &[ChatTurn],
        cancel: Arc<AtomicBool>,
        mut on_delta: F,
    ) -> AppResult<String>
    where
        F: FnMut(&str) + Send,
    {
        let response = self
            .http
            .post(&self.base_url)
            .bearer_auth(&self.api_key)
            .json(&json!({
                "model": self.model,
                "messages": messages,
                "temperature": self.temperature,
                "max_tokens": self.max_tokens,
                "stream": true
            }))
            .send()
            .await
            .map_err(|e| AppError::Message(format!("模型请求失败：{e}")))?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::Message(format!(
                "模型服务返回 {status}：{}",
                body.chars().take(400).collect::<String>()
            )));
        }

        let mut stream = response.bytes_stream();
        let mut pending = String::new();
        let mut full = String::new();
        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::Relaxed) {
                return Err(AppError::Message("生成已取消".into()));
            }
            let chunk = chunk.map_err(|e| AppError::Message(format!("读取流式响应失败：{e}")))?;
            pending.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(index) = pending.find('\n') {
                let line = pending[..index].trim_end_matches('\r').to_string();
                pending.drain(..=index);
                if let Some(delta) = parse_sse_data(&line) {
                    full.push_str(&delta);
                    on_delta(&delta);
                }
            }
        }
        if let Some(delta) = parse_sse_data(pending.trim()) {
            full.push_str(&delta);
            on_delta(&delta);
        }
        if full.is_empty() {
            return Err(AppError::Message(
                "模型没有返回文本。该服务可能不支持流式输出，请在设置中关闭流式或更换模型。".into(),
            ));
        }
        Ok(full)
    }
}

pub fn parse_completion_body(body: &str) -> AppResult<CompletionOutput> {
    let value: Value = serde_json::from_str(body)
        .map_err(|e| AppError::Message(format!("模型响应不是合法 JSON：{e}")))?;
    let text = value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| AppError::Message("模型响应缺少 choices[0].message.content".into()))?;
    let finish_reason = value
        .pointer("/choices/0/finish_reason")
        .and_then(Value::as_str)
        .map(str::to_string);
    let truncated = matches!(
        finish_reason.as_deref(),
        Some("length") | Some("max_tokens")
    );
    Ok(CompletionOutput {
        text,
        finish_reason,
        truncated,
    })
}

/// 解析一行 SSE；返回正文增量。会忽略 `[DONE]`、注释与无法解析的事件。
pub fn parse_sse_data(line: &str) -> Option<String> {
    let data = line.strip_prefix("data:")?.trim();
    if data.is_empty() || data == "[DONE]" {
        return None;
    }
    let value: Value = serde_json::from_str(data).ok()?;
    value
        .pointer("/choices/0/delta/content")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            value
                .pointer("/choices/0/text")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

/// 模型常把 JSON 包在代码块或前后缀里；抽取第一个完整 JSON 值。
pub fn parse_json_loose<T: serde::de::DeserializeOwned>(raw: &str) -> AppResult<T> {
    let trimmed = raw.trim();
    if let Ok(value) = serde_json::from_str::<T>(trimmed) {
        return Ok(value);
    }
    let start = trimmed.find(['{', '[']);
    let end = trimmed.rfind(['}', ']']);
    if let (Some(start), Some(end)) = (start, end) {
        if end > start {
            return serde_json::from_str::<T>(&trimmed[start..=end])
                .map_err(|e| AppError::Message(format!("模型返回的 JSON 无法解析：{e}")));
        }
    }
    Err(AppError::Message(
        "模型没有返回可解析的 JSON。请重试，或换用更稳定的模型。".into(),
    ))
}

pub fn seed_system_prompt() -> String {
    r#"你是中文长篇小说与游戏叙事的项目策划。作者会给出一个 logline，可能只有一个画面或冲突。你要给出三种都能支撑长篇、且因果发动机完全不同的走向。
不要让三个方向只是换类型、换称号或换反派；每个方向都必须有不同的“主角选择会怎样改变世界/关系”的结构。

每个方向必须形成最小因果闭环：
具体处境 → 主角想要的结果 → 结构性阻力如何阻止 → 主角必须付出的代价 → 这个代价如何制造下一层问题 → 终局可能改变什么。
任何一句抽象主题都必须落到人物、规则、行动或代价上。

只输出 JSON 数组，不要解释，不要 Markdown。每项字段：
{
  "logline": "一句话故事：谁想要什么，什么阻止，代价是什么",
  "genre": "类型",
  "theme": "核心命题，必须与因果链有关",
  "tone": "情绪基调",
  "core_conflict": "主角目标、阻力来源、不可逆代价三层都要写清",
  "dramatic_question": "全书用一个可以用是/否或代价选择回答的问题",
  "antagonistic_force": "谁或什么在结构性阻止主角；不是单纯坏人，必须有资源、底线和行动方式",
  "world_rule": "一条会持续制造代价、限制选择且允许被利用的世界规则",
  "ending_direction": "终局必须改变的世界状态、关系状态或自我认知，至少两项",
  "protagonist_desire": "主角具体想要得到/保住什么",
  "flaw": "会导致主角主动犯错并支付代价的缺陷",
  "world_hook": ["2-3 条具体世界规则/异常现象，每条必须说明代价或后果"],
  "pov": "叙事视角",
  "taboo": ["应避免的写法"]
}

三种方向必须满足：
- A 偏外部行动与生存压力，B 偏关系与身份撕裂，C 偏真相、规则或被压抑历史的反转；三者的核心阻力不能相同。
- 至少有一个方向的阻力来自主角自己的选择或世界规则，而不是外部敌人。
- 至少有一个方向允许长线扩展：解决眼前问题会产生更大的世界问题。
- 不使用“神秘力量”“命运”“人性”“黑暗”等空泛词代替具体机制。
所有文本使用中文。"#
        .into()
}

pub fn seed_user_prompt(logline: &str) -> String {
    format!(
        "作者的一句话或灵感：\n{logline}\n\n请给出三种因果发动机不同、可直接进入叙事蓝本的走向。"
    )
}

pub fn seed_compact_system_prompt() -> String {
    r#"你是中文长篇叙事策划。根据作者灵感给出 3 个因果发动机不同的长篇走向。
只输出一个合法 JSON 数组，不要 Markdown、解释、前后缀或省略号。必须完整闭合数组和全部对象。
每项只包含：
{"logline":"谁想要什么、什么阻止、代价是什么","genre":"类型","theme":"与因果链有关的命题","tone":"情绪基调","core_conflict":"目标/阻力/代价","dramatic_question":"可用选择回答的核心问题","antagonistic_force":"有资源、底线和行动方式的结构性阻力","world_rule":"持续制造代价的世界规则","ending_direction":"终局改变什么","protagonist_desire":"具体欲望","flaw":"会主动制造代价的缺陷","world_hook":["2 条具体规则或现象"],"pov":"叙事视角","taboo":["应避免的写法"]}
三个方向的核心阻力必须分别偏外部行动、关系身份、真相规则；不能只是换措辞。每项文字精简但具体。使用中文。"#
        .into()
}

pub fn narrative_foundation_system_prompt() -> String {
    r#"你是资深长篇叙事架构师与游戏叙事总监。你的任务不是写章节，而是先建立能支撑超长篇或长线游戏运营的“叙事地基”。
必须像制作人做设定评审一样严谨：每个设定都要能影响行动、制造阻力或生成后果；禁止只有氛围没有功能的词条。

工作原则：
1. 先确定全书/全作品的唯一核心戏剧问题。它必须能用“某人能否在某种代价下达成某目标”回答。
2. 明确开局长板与终局状态。终局不是一句主题，而是世界、关系、认知和代价都发生可验证变化后的状态。
3. 人物必须有外部目标、内在需要、恐惧、缺陷、筹码、秘密和相互冲突关系。重要人物关系要能形成压力，而不是标签。
4. 势力必须有资源、诉求、底线、行动方式和与主角的结构性冲突。世界规则必须有代价、边界、漏洞和被利用方式。
5. 时间线必须写前因、发生、结果、长期后果和读者/玩家何时得知。不能只列日期。
6. 叙事线必须有戏剧问题、起始状态、目标状态和赌注。主线、关系线、谜团线、势力线之间必须相互施压。
7. 为长篇保留可扩展空间：设定要生成新的冲突，不要一次把所有底牌写死。
8. 所有未确认内容都是候选。不要引用现实作品名称，不要模仿在世作者的可识别文风。

只输出 JSON 对象，不要 Markdown 代码块。结构：
{
  "title": "作品/阶段标题",
  "coreDramaticQuestion": "唯一核心戏剧问题",
  "narrativePromise": "持续提供给读者/玩家的体验承诺",
  "openingState": "开局可验证状态",
  "endingState": "终局可验证状态",
  "themeQuestion": "作品反复追问但不必直接回答的问题",
  "elements": [
    {
      "kind": "character|faction|world_rule|location|item|concept",
      "name": "名称",
      "role": "在冲突中的结构功能",
      "summary": "具体、可验证的设定摘要",
      "externalGoal": "要做什么",
      "internalNeed": "真正需要面对什么",
      "fear": "最怕失去/承认什么",
      "flaw": "会如何自毁",
      "leverage": "能影响他人的资源或能力",
      "constraints": ["不能/必须遵守的边界"],
      "secret": "隐藏信息及其影响",
      "relations": [{"target":"名称","type":"关系","tension":"当前矛盾"}],
      "metadata": {"resources":"","bottomLine":"","method":""}
    }
  ],
  "timeline": [
    {"orderNo":1,"title":"事件","storyTime":"故事时间","cause":"前因","participants":["名称"],"outcome":"直接结果","consequence":"长期后果","revealAt":"读者/玩家何时得知"}
  ],
  "threads": [
    {"orderNo":1,"kind":"main|relationship|mystery|faction|theme","title":"叙事线","dramaticQuestion":"本线要回答的问题","openingState":"起始","targetState":"目标","stakes":"失败会失去什么"}
  ]
}

要求：
- 至少 4 个 elements，其中至少 2 个人物、1 个势力或组织、1 条世界规则。
- 至少 3 个 timeline，且必须形成因果，不是事件清单。
- 至少 2 条 threads，主线与其他线必须相互影响。
- 文本使用中文，名称可中文。每个字段具体，禁止“神秘力量”“复杂关系”这类空话。"#
        .into()
}

pub fn narrative_foundation_user_prompt(
    premise: &Value,
    medium: &str,
    strategy: &str,
    target_units: Option<i32>,
) -> String {
    let target = target_units
        .map(|value| value.to_string())
        .unwrap_or_else(|| "由逻辑链决定".into());
    format!(
        "已确认种子卡：\n{premise}\n\n目标媒介：{medium}\n编排策略：{strategy}\n目标章节/任务数：{target}\n\n\
         请先建立叙事地基，不要生成章节或逻辑链节点。"
    )
}

pub fn narrative_chain_system_prompt() -> String {
    r#"你是长篇叙事因果链编辑与游戏任务编排架构师。你会收到已完成的叙事地基。你必须把人物欲望、世界规则、时间线、势力压力转化为唯一的可验证因果链。
章节不是逻辑链的来源；逻辑链才是统筹章节、任务和演出的唯一依据。

每个逻辑节点必须形成“前因 → 人物选择 → 代价 → 结果 → 状态变化 → 认知变化”。如果任意一环缺失，节点无效。
每条边必须说明上一节点如何迫使、开启、阻止、反转或收束下一节点。禁止为了节奏把事件随意拼接。
认知变化必须写清“谁知道、之前误以为什么、现在确认什么”，用于控制信息差与反转。
反转必须来自已埋设的信息、人物选择或世界规则，不能靠临时增加设定。
结尾必须回应当前阶段的核心戏剧问题，同时改变世界、关系或人物自我认知中的至少两项。

如果是游戏作品，还要为每个逻辑节点提供呈现契约：
- dialogue：以 NPC 对话为主，小量旁白。
- environmental：主要通过环境、场景调度、可观察行为呈现。
- prop_text：主要通过纸张、道具、碑文、终端等文本呈现。
- cutscene：主要通过过场动画呈现。
- mixed：多种媒介混合。
- silent：无对白，通过画面、声音和交互呈现。
必须写清玩家能动性、可见画面要求、声音要求和可交付脚本形式。

只输出 JSON 对象，不要 Markdown：
{
  "nodes": [
    {
      "orderNo": 1,
      "threadIndex": 0,
      "kind": "inciting|complication|reversal|reveal|crisis|climax|resolution",
      "title": "节点名",
      "cause": "直接前因",
      "choice": "谁做了什么选择",
      "cost": "付出了什么不可逆代价",
      "outcome": "直接结果",
      "stateChange": "世界/关系/资源/身份变化",
      "knowledgeChange": "谁知道/误信/确认了什么",
      "participants": ["名称"],
      "location": "地点",
      "storyTime": "故事时间"
    }
  ],
  "edges": [
    {"fromOrder":1,"toOrder":2,"relation":"导致|迫使|开启|阻止|反转|收束","condition":"成立条件"}
  ],
  "presentations": [
    {
      "nodeOrder": 1,
      "medium": "game|novel",
      "mode": "dialogue|environmental|prop_text|cutscene|mixed|silent",
      "pov": "视角角色",
      "dialogueRatio": 70,
      "narrationRatio": 10,
      "playerAgency": "玩家参与什么选择",
      "visualRequirements": "画面必须传达的信息",
      "audioRequirements": "声音必须传达的信息",
      "interaction": "交互形式",
      "deliverable": "对白稿/任务脚本/环境叙事清单/过场分镜"
    }
  ]
}
节点必须从 orderNo 1 连续编号；边必须形成从唯一起点到终局的完整可达网络，不能有孤立节点。
至少 6 个节点，长线作品应为阶段保留后续扩展空间，但本阶段因果链必须自洽。使用中文。"#
        .into()
}

pub fn narrative_chain_user_prompt(foundation: &Value, medium: &str, strategy: &str) -> String {
    format!(
        "已建立叙事地基：\n{foundation}\n\n媒介：{medium}\n编排策略：{strategy}\n\n\
         请生成完整因果链。{game_rule}",
        game_rule = if medium == "game" {
            "这是游戏作品，每个节点都必须有 presentations 呈现契约。"
        } else {
            "这是小说作品，presentations 可以返回空数组。"
        }
    )
}

pub fn narrative_repair_system_prompt() -> String {
    r#"你是叙事蓝本的逻辑修复编辑。你会收到当前叙事地基、因果链和确定性诊断。
你的任务不是重写作品，而是以最小改动修复具体问题，使人物动机、世界规则、时间线、因果边、状态变化和认知变化闭环。

修复规则：
1. 保留已经具体、有效且不冲突的设定和节点，不要为了显得丰富而新增大段无关内容。
2. 每个逻辑节点仍必须包含前因、人物选择、代价、结果、状态变化、认知变化。
3. 边必须解释上一节点怎样迫使、开启、阻止、反转或收束下一节点；修复孤立节点和断点。
4. 游戏作品必须让每个逻辑节点都有呈现契约，且呈现模式必须能执行。
5. 不得用临时新设定解决逻辑错误；优先修改缺失条件、动机、代价或因果连接。
6. 修复完成后，整条链必须从唯一核心起点可达终局。

只输出 JSON 对象，不要 Markdown：
{
  "foundation": { "title":"", "coreDramaticQuestion":"", "narrativePromise":"", "openingState":"", "endingState":"", "themeQuestion":"", "elements":[], "timeline":[], "threads":[] },
  "chain": { "nodes":[], "edges":[], "presentations":[] }
}
字段结构和原输入完全相同。输出完整对象，不要只输出补丁。使用中文。"#
        .into()
}

pub fn narrative_repair_user_prompt(
    foundation: &Value,
    chain: &Value,
    diagnostics: &Value,
) -> String {
    format!(
        "当前叙事地基：\n{foundation}\n\n当前因果链：\n{chain}\n\n需要修复的诊断：\n{diagnostics}\n\n\
         请修复全部 error，并尽量处理 warning。"
    )
}

pub fn outline_adaptive_system_prompt() -> String {
    r#"你是长篇叙事编排编辑。你会收到已确认的叙事蓝本与完整逻辑链。你的唯一任务是估算叙事体量并划分卷/阶段。
章节数不是作者预先固定的目标，而是逻辑链复杂度、人物弧线、信息揭示、世界扩张和叙事留白的结果。
不得为了凑整平均切分。每个阶段必须完成一条或多条逻辑线的状态跃迁，并留下由下一阶段必然承接的问题。
只输出 JSON 对象，不要 Markdown：
{"totalChapters":120,"reasoning":"为什么这个体量足够且不冗余","volumes":[{"title":"卷名","chapterCount":40,"logicNodeRange":"1-3","goal":"阶段目标","turn":"阶段转折"}]}
章节总数必须在 24 至 1200 之间，各卷 chapterCount 之和必须等于 totalChapters。逻辑链较薄时不得虚增重复冲突。
使用中文。"#
        .into()
}

pub fn outline_adaptive_user_prompt(premise: &Value, blueprint_context: &str) -> String {
    format!(
        "种子卡：\n{premise}\n\n已确认叙事蓝本：\n{blueprint_context}\n\n\
         请依据逻辑链确定全书体量与卷/阶段划分。"
    )
}

pub fn outline_volume_system_prompt() -> String {
    r#"你是长篇小说的结构编辑。作者已经确定卷数和每卷章节数，你只负责规划每一卷的叙事功能。
卷与卷之间必须形成递进：目标升级、代价升级、世界揭示扩大，不能把同一冲突重复换皮。
只输出 JSON 数组，不要 Markdown。数组长度必须与作者给出的卷数完全一致。每项字段：
{"index":1,"title":"卷名","goal":"本卷目标","conflict":"本卷核心阻力","turn":"本卷最关键转折","payoff":"本卷收束与下一卷钩子"}
作者已经填写卷名时必须沿用，不得改名。所有文本使用中文。"#
        .into()
}

pub fn outline_volume_user_prompt(premise: &Value, volume_requests: &str) -> String {
    format!(
        "已确认的种子卡：\n{premise}\n\n作者定义的分卷结构：\n{volume_requests}\n\n请为每一卷生成卷级骨架。"
    )
}

pub fn outline_chapter_system_prompt() -> String {
    r#"你是长篇叙事的编排编辑。叙事蓝本和因果链已经确定，你只能把逻辑节点编排成章节/任务，不能发明新的世界事实或逻辑节点。
每一章必须回答：它承接哪些逻辑节点、如何把一个可验证状态推进到下一状态、目标、冲突、转折、兑现。
章节不是逻辑链的来源；一章既不能多讲同一逻辑节点，也不能跳过关键代价、选择或认知变化。
相邻章节必须形成因果，不允许平均分配情节或为了凑数重复换皮。
本批次的 number 必须连续，数量和范围以用户要求为准。只输出 JSON 数组，不要 Markdown。每项字段：
{"number":1,"title":"章名","goal":"本章目标","conflict":"本章冲突","turn":"本章转折","payoff":"伏笔操作","logicNodeIds":["逻辑节点标题或编号"],"presentationMode":"prose|dialogue|environmental|prop_text|cutscene|mixed|silent","narrativeFunction":"本章在整体逻辑链中的功能"}
不得提前写出后续批次没有要求的章节。所有文本使用中文。"#
        .into()
}

#[allow(clippy::too_many_arguments)]
pub fn outline_chapter_user_prompt(
    premise: &Value,
    structure_context: &str,
    volume_context: &str,
    start: i32,
    end: i32,
    previous_context: &str,
) -> String {
    format!(
        "已确认的种子卡：\n{premise}\n\n全书结构：\n{structure_context}\n\n当前卷：\n{volume_context}\n\n\
         本批次要求生成第 {start} 章至第 {end} 章，共 {} 章。\
         必须参考上面的叙事蓝本与因果链，保证本批次每章都标注承接的逻辑节点。\
         前一章或前一批次摘要：\n{previous_context}\n\n只返回本批次章节。",
        end - start + 1
    )
}

pub fn extraction_system_prompt() -> String {
    r#"你是小说设定抽取器。请从给定正文片段中抽取可核实的事实候选，不要润色，不要补写原作没有的内容。
区分真值层（truth_layer）：
- world：故事世界中客观成立
- belief：某个角色相信
- utterance：角色说过，可能撒谎或误传
- narrative：叙述者提供，可能不可靠
- planned：作者计划稍后揭示

只输出 JSON 对象，不要 Markdown 代码块：
{
  "entities": [{"type": "character|place|item|faction|rule|concept", "canonicalName": "...", "aliases": [], "description": "..."}],
  "claims": [{"subject": "...", "predicate": "...", "object": "...", "truthLayer": "world|belief|utterance|narrative|planned", "perspective": "角色名或空", "validFrom": "故事时间或空", "validTo": "空", "quote": "原文短句"}],
  "events": [{"title": "...", "participants": [], "place": "...", "storyTime": "...", "cause": "...", "outcome": "...", "quote": "原文短句"}],
  "relations": [{"from": "...", "to": "...", "type": "关系类型", "quote": "原文短句"}],
  "promises": [{"title": "...", "contract": "怎样才算真正兑现", "window": "预期章节或范围", "quote": "原文短句"}]
}
没有把握的内容不要输出。每条候选都必须带可定位的原文短句 quote。"#
        .into()
}

pub fn extraction_user_prompt(title: &str, text: &str) -> String {
    let clipped: String = text.chars().take(12_000).collect();
    format!("作品片段标题：{title}\n\n正文：\n{clipped}")
}

pub fn chat_system_prompt(mode: &str, premise: Option<&Value>, constraints: &[String]) -> String {
    let mode_rule = match mode {
        "structure" => "你在帮助作者推演卷、章、场景结构。先给结构性选择，不写整章正文。",
        "plot" => "你在帮助作者推进当前剧情。除非作者明确要求“直接续写”，否则先给 3 个节拍选项，不写整章。",
        "lookup" => "你在回答设定问题。只依据提供的已确认事实；没有依据时明确说“设定中尚无记录”。",
        "audit" => "你在解释冲突。必须引用提供的规则与证据，不得发明事实。",
        _ => "你在帮助作者发散故事方向。可以提出大胆假设，但必须标明哪些是建议、尚未确认。",
    };
    let mut prompt = format!(
        r#"你是 CanonWeave 的叙事任务助手。{mode_rule}

边界：
- 你输出的任何设定都只是建议，只有作者“采纳为候选”并确认后才成为正式事实。
- 不要把建议写成既成事实；区分「建议」与「已确认」。
- 不要替作者决定审美判断，不要自动发布内容。
- 面对多步骤请求时，先给出简短任务计划，再执行当前回复能完成的部分。
- 回答使用中文，简洁具体。"#
    );
    if let Some(premise) = premise {
        prompt.push_str(&format!(
            "\n\n已确认种子卡（创作契约，不是世界事实）：\n{premise}"
        ));
    }
    if !constraints.is_empty() {
        prompt.push_str("\n\n作者固定的会话决定（进入任务契约，不是世界事实）：\n");
        for item in constraints {
            prompt.push_str("- ");
            prompt.push_str(item);
            prompt.push('\n');
        }
    }
    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_openai_sse_delta() {
        let line = r#"data: {"choices":[{"delta":{"content":"雨"}}]}"#;
        assert_eq!(parse_sse_data(line), Some("雨".into()));
        assert_eq!(parse_sse_data("data: [DONE]"), None);
        assert_eq!(parse_sse_data(": keep-alive"), None);
    }

    #[test]
    fn parses_json_wrapped_in_markdown() {
        let raw = "好的，如下：\n```json\n{\"a\": 1}\n```\n";
        let value: Value = parse_json_loose(raw).unwrap();
        assert_eq!(value["a"], 1);
    }

    #[test]
    fn detects_truncated_completion_from_finish_reason() {
        let body = r#"{"choices":[{"message":{"content":"[{\"logline\":\"A\"}"},"finish_reason":"length"}]}"#;
        let output = parse_completion_body(body).unwrap();
        assert!(output.truncated);
        assert_eq!(output.finish_reason.as_deref(), Some("length"));
    }
}
