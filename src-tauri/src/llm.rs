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
            .timeout(Duration::from_secs(300))
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
            return Err(AppError::Message(format_http_error(status.as_u16(), &body)));
        }
        parse_completion_body(&body)
    }

    /// 长结构化请求优先使用流式响应。部分 OpenAI 兼容网关会给非流式长请求设置
    /// 较短的代理超时，即使客户端自身允许等待更久也会返回 502/504。
    pub async fn complete_streaming(
        &self,
        messages: &[ChatTurn],
        max_tokens: i32,
    ) -> AppResult<String> {
        let cancel = Arc::new(AtomicBool::new(false));
        self.stream_with_limit(messages, max_tokens, cancel, |_| {})
            .await
    }

    pub async fn stream<F>(
        &self,
        messages: &[ChatTurn],
        cancel: Arc<AtomicBool>,
        on_delta: F,
    ) -> AppResult<String>
    where
        F: FnMut(&str) + Send,
    {
        self.stream_with_limit(messages, self.max_tokens, cancel, on_delta)
            .await
    }

    async fn stream_with_limit<F>(
        &self,
        messages: &[ChatTurn],
        max_tokens: i32,
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
                "max_tokens": max_tokens,
                "stream": true
            }))
            .send()
            .await
            .map_err(|e| AppError::Message(format!("模型请求失败：{e}")))?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::Message(format_http_error(status.as_u16(), &body)));
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

fn clean_error_body(body: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for ch in body.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                text.push(' ');
            }
            _ if !in_tag => text.push(ch),
            _ => {}
        }
    }
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(400)
        .collect()
}

fn format_http_error(status: u16, body: &str) -> String {
    let clean = clean_error_body(body);
    if status == 504 {
        return "模型服务网关超时（504）。上游代理在长请求完成前中断了连接，应用会尝试自动重试。"
            .into();
    }
    if status == 502 || status == 503 {
        return format!("模型服务暂时不可用（{status}），应用会尝试自动重试。");
    }
    if clean.is_empty() {
        format!("模型服务返回 {status}")
    } else {
        format!("模型服务返回 {status}：{clean}")
    }
}

pub fn is_transient_model_error(message: &str) -> bool {
    let lower = message.to_lowercase();
    [
        "502",
        "503",
        "504",
        "gateway timeout",
        "timeout",
        "timed out",
        "模型请求失败",
        "读取流式响应失败",
        "连接失败",
        "暂时不可用",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
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

/// 分层构建故事圣经时每一层共用的标准。
const WORLD_DEPTH_RULES: &str = r#"通用标准（每一层都必须满足）：
- 设定为冲突服务：每个条目都要写清它制造什么冲突、限制谁、给谁机会、会被谁利用。
- 冰山原则：写出读者暂时看不到的水下部分（历史成因、真实动机、隐藏代价、错误认知），供后续伏笔与反转使用。
- 自洽压力测试：对每个能力或规则追问“为什么最强者不直接用它解决一切”；对每个势力追问“为什么它至今没有赢、也没有被消灭”。答案必须写进设定。
- 具体可检验：使用专有名词、数量、地点、时间、代价；禁止“神秘力量”“强大组织”“古老秘密”“复杂关系”之类空话。
- 彼此咬合：新条目必须与前几层已有条目建立关系（谁控制谁、谁需要谁、谁欠谁、谁在骗谁），不能各自孤立。
- 信息密度：单个字段 30 至 150 字，写事实与因果，不写抒情；宁可多条精炼，也不要少条空泛。
- 已确认设定优先：作者已确认的人物、知识条目与守则必须保留原名并在此基础上扩展，不得矛盾或改名。
- 只输出 JSON 对象，不要 Markdown 代码块，文本使用中文。"#;

/// 分层故事圣经：world → factions → cast → synthesis → critique。
/// 一次请求只建一层，后层读取前层结果，避免单次输出把世界观压扁成几条摘要。
pub fn world_layer_system_prompt(layer: &str, scale: &str) -> String {
    let epic = scale == "epic";
    let body = match layer {
        "world" => format!(
            r#"你是长篇幻想/悬疑/科幻作品的世界观总设计。本层只建立“世界运行规律”，不写人物和章节。
需要产出：
1. 能力/力量体系（{systems}）：每个体系作为 kind="concept" 的元素，metadata 写成
   {{"system":"体系名","tiers":[{{"rank":"阶位/序列","name":"称谓","abilities":"该阶能做什么","cost":"代价","risk":"失控/反噬风险"}}],"acquisition":"如何获得与晋升","cost":"通用代价","limits":"硬边界","counters":"克制与失效条件","socialImpact":"对社会结构、经济、法律的影响"}}
   阶位不少于 {tiers} 级；每一阶都要有明确代价，越强代价越重；必须写清普通人与掌握者的力量差，以及社会如何管控它。
2. 世界规则（kind="world_rule"）不少于 {rules} 条：物理/超自然法则、禁忌、社会制度、资源稀缺性。每条写 constraints（边界）与 secret（大多数人不知道的真相）。
3. 关键地点（kind="location"）不少于 {places} 个：地理关系、控制者、资源、危险、在故事中承担的功能。
4. 关键物品/遗物（kind="item"）不少于 {items} 个：来源、能力、代价、持有者、争夺者。
5. 历史时间线 timeline 不少于 {history} 条，跨越至少 3 个时代：每条写前因、结果、至今仍在影响什么（consequence）、读者何时得知（revealAt）。
输出：{{"elements":[{{"kind":"","name":"","role":"在冲突中的功能","summary":"","constraints":[],"secret":"","leverage":"","relations":[{{"target":"","type":"","tension":""}}],"metadata":{{}}}}],"timeline":[{{"title":"","storyTime":"时代/年份","cause":"","participants":[],"outcome":"","consequence":"","revealAt":""}}]}}"#,
            systems = if epic { "1 至 2 个，主体系必须完整" } else { "1 个" },
            tiers = if epic { 7 } else { 4 },
            rules = if epic { 8 } else { 4 },
            places = if epic { 8 } else { 4 },
            items = if epic { 4 } else { 2 },
            history = if epic { 10 } else { 5 },
        ),
        "factions" => format!(
            r#"你是长篇作品的势力格局设计师。本层基于已建立的世界规律，设计势力/组织（kind="faction"），不写章节。
需要产出不少于 {factions} 个势力，必须包含：至少两个结构性对立的阵营、至少一个中立或第三方（商会、学派、地下网络等）、至少一个隐藏势力（大多数角色不知道其存在或真实目的）。
每个势力：
- summary：成立缘由、现在的规模与公开形象；externalGoal：当前目标；internalNeed：组织真正缺什么；fear：最怕失去什么；flaw：组织病（腐败、教条、派系斗争等）；leverage：资源与筹码；secret：不可告人的秘密及暴露后果；constraints：底线与禁令。
- metadata：{{"structure":"层级与编制（至少 3 级，写出各级称谓与人数量级）","leader":"首领姓名与外号","doctrine":"信条","resources":"人力/财力/超凡资源","territory":"控制范围","methods":"惯用手段","schism":"内部裂痕与派系","powerAccess":"与能力体系的关系（垄断/限制/研究/走私哪一阶）","recruitment":"如何吸收成员"}}
- relations：与其他每个主要势力的关系（同盟/敌对/利用/渗透/交易），写清当前矛盾点 tension。
另补充 timeline：势力间的关键历史事件（战争、背叛、条约、清洗）不少于 {events} 条。
输出：{{"elements":[...],"timeline":[...]}}，元素字段与前层相同。"#,
            factions = if epic { 7 } else { 3 },
            events = if epic { 6 } else { 3 },
        ),
        "cast" => format!(
            r#"你是长篇作品的人物总设计。本层基于世界规律与势力格局，设计人物群像（kind="character"），不写章节。
需要不少于 {cast} 个人物，必须包含：主角、明面对手、隐藏对手（真正的幕后推动者）、导师或引路人、宿敌/竞争者、至少一个会背叛或立场摇摆的人物，以及各主要势力的首领或关键执行者。
每个人物：
- summary：身份、年龄段、外在印象与一句话核心矛盾；externalGoal：想得到什么；internalNeed：真正需要什么；fear；flaw：会如何自毁；leverage：筹码；secret：秘密及暴露后果；constraints：不会做的事（底线）。
- metadata：{{"faction":"所属势力或无","ability":"所用能力体系与阶位，以及标志性用法和代价","background":"塑造此人的关键往事（与历史时间线挂钩）","arc":"起点状态 → 终点状态","voice":"说话方式与口头习惯","wound":"旧伤","misbelief":"对世界或自己的错误信念"}}
- relations：与至少 3 个其他人物或势力的关系，写清 tension（利益冲突、恩怨、亏欠、误会）。主角与每个对手之间必须存在无法两全的目标冲突。
- 对手必须有自己合理甚至令人同情的理由，以及推进中的计划，而不是等主角上门。
输出：{{"elements":[...]}}，元素字段与前层相同。"#,
            cast = if epic { 12 } else { 5 },
        ),
        "synthesis" => format!(
            r#"你是总编剧。前三层已经建立世界、势力与人物。本层把它们收束成作品的总体戏剧结构，不写章节。
需要产出：
- title、coreDramaticQuestion（某人能否在某种代价下达成某目标）、narrativePromise、openingState、endingState、themeQuestion。
- threads 不少于 {threads} 条：main 主线、relationship 关系线、mystery 谜团线（targetState 中写出谜底）、faction 势力线、theme 主题线。每条写 dramaticQuestion、openingState、targetState、stakes，并说明它与哪条线互相施压。
- timeline：故事开局前后的“暗线时间表”不少于 {plans} 条——隐藏对手与各势力在主角不知情时正在推进的计划步骤，每条写 storyTime、cause、outcome、consequence，以及主角/读者何时发现（revealAt）。
输出：{{"title":"","coreDramaticQuestion":"","narrativePromise":"","openingState":"","endingState":"","themeQuestion":"","threads":[{{"kind":"","title":"","dramaticQuestion":"","openingState":"","targetState":"","stakes":""}}],"timeline":[...]}}"#,
            threads = if epic { 6 } else { 3 },
            plans = if epic { 8 } else { 4 },
        ),
        _ => r#"你是严苛的设定审校编辑，专门找长篇设定的逻辑漏洞。你会收到完整故事圣经。
逐类审查并找出至少 8 个具体问题：
1. 能力漏洞：某能力能轻易绕过主线困难、阶位代价不成比例、克制关系缺失。
2. 势力逻辑：目标与手段矛盾、早该发生的冲突没有发生、隐藏势力为何不被发现。
3. 人物动机：行动缺少动机、对手太被动或太蠢、关系只有标签没有张力。
4. 时间线：先后矛盾、历史事件没有留下后果、暗线计划与开局状态冲突。
5. 空泛条目：只有氛围没有功能的设定。
6. 结构缺口：缺少能支撑长篇的冲突来源、中段没有升级空间。
修复方式：直接给出修订后的完整条目（同名同类替换），必要时新增条目；不得删除作者已确认的设定。
输出：{"issues":[{"target":"条目名","problem":"问题","fix":"修复思路"}],"patch":{"elements":[完整元素],"timeline":[完整事件],"threads":[完整叙事线]}}"#
            .to_string(),
    };
    format!("{body}\n\n{WORLD_DEPTH_RULES}")
}

pub fn world_layer_user_prompt(
    layer: &str,
    premise: &Value,
    medium: &str,
    canon: &str,
    so_far: &Value,
) -> String {
    let instruction = match layer {
        "world" => "请建立世界运行规律层。",
        "factions" => "请在已有世界规律上建立势力格局层。",
        "cast" => "请在已有世界与势力上建立人物群像层。",
        "synthesis" => "请把以上内容收束成总体戏剧结构。",
        _ => "请审查以下完整故事圣经并输出修订补丁。",
    };
    format!(
        "已确认种子卡：\n{premise}\n\n媒介：{medium}\n\n作者已确认的故事资料与守则（必须保留）：\n{canon}\n\n\
         已完成的故事圣经：\n{so_far}\n\n{instruction}"
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

避免“水”和死板的硬性要求：
- 节点之间用“但是/因此”连接，而不是“然后”：每个节点要么因上一节点而发生，要么让上一节点的成果受挫。
- 对手是主动的：至少三分之一节点由对手或势力的暗线计划推动，主角是在应对和反制，而不是逐个打怪。
- 逐级升级：赌注、代价、对手层级和世界揭示范围随节点上升；同类冲突不得重复出现两次以上。
- 每个节点至少牵动两条叙事线；中段必须有一次让主角目标本身被动摇的反转。
- 能力不能白给：主角每次突破都要付出地基中写明的代价，并带来新的麻烦。
- 充分使用地基：主要势力、隐藏势力、关键物品和至少一半主要人物都要在链上承担功能。

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
至少 12 个节点（超长篇至少 18 个），长线作品应为阶段保留后续扩展空间，但本阶段因果链必须自洽。使用中文。"#
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
{"number":1,"title":"章名","goal":"本章目标","conflict":"本章冲突","turn":"本章转折","payoff":"伏笔操作","logicNodeIds":["逻辑节点标题或编号"],"presentationMode":"prose|dialogue|environmental|prop_text|cutscene|mixed|silent","narrativeFunction":"本章在整体逻辑链中的功能","valueShift":"+安全 → -暴露","crisis":"主角被迫在两个代价之间做出的选择","hook":"章末留给读者的具体悬念","reveal":"读者在本章新知道、而某些角色仍不知道的信息"}
工艺要求：
- valueShift 写成“起点 → 终点”，并用 + 或 - 标出价值正负；起点与终点必须不同，否则这一章没有发生变化。
- 相邻章节的终点极性不要连续三章相同，避免节奏单调。
- crisis 必须是两难：两个选项都有代价，不能是“做对的事还是做错的事”。
- hook 必须具体到人物、物件或信息，不写“命运的齿轮开始转动”之类空话。
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

pub fn agent_synthesis_system_prompt() -> String {
    r#"你是 CanonWeave 的项目整理 Agent。你只分析给定的只读项目快照，并把可以由原文片段复核的信息整理成候选变更。

最高优先级规则：
1. 不得把摘要、常识、猜测或模型补全当作原文事实。
2. 每个候选必须同时给出 sourceSpan 与 quote；quote 必须逐字复制快照 evidenceSources 中已有的原文片段，不得改写。
3. 同名人物先合并；把新称谓写入 aliases，不要创建重复实体。
4. 角色信念、传闻、角色陈述与世界客观事实必须区分。无法判断时，不提出事实候选。
5. 与已有权威记录或候选重复的内容不要重复提出。
6. 所有写入都只是候选，不能声称已经确认、提交正文或覆盖权威事实。
7. 如果证据不足，宁可不提出候选，并在 summary 中说明缺失。

只输出 JSON，不要 Markdown，不要解释：
{
  "summary": "本次整理的简短结论",
  "candidates": [
    {
      "kind": "entity|relation|claim|event|promise|knowledge_entry|outline_patch",
      "payload": {},
      "sourceSpan": "evidenceSources 中的 sourceSpan",
      "quote": "evidenceSources 中逐字存在的原文",
      "confidence": 0.0
    }
  ]
}

各候选 payload：
- entity: {"type":"character|faction|place|item|concept","canonicalName":"","aliases":[],"description":""}
- relation: {"from":"已有或本次实体名","to":"已有或本次实体名","type":"关系类型","tension":"关系张力"}
- claim: {"subject":"","predicate":"","object":"","truthLayer":"world|belief|utterance|narrative|planned"}
- event: {"title":"","storyTime":"","participants":[],"place":"","cause":"","outcome":""}
- promise: {"title":"","window":"","contract":"必须写清怎样才算兑现"}
- knowledge_entry: {"domainId":"已有知识域 ID，可省略","domainKey":"已有知识域 key，可省略","name":"","aliases":[],"summary":"","fields":{}}
- outline_patch: {"level":"volume|chapter","orderNo":1,"title":"","goal":"","conflict":"","turn":"","payoff":""}

不要输出没有原文证据的候选。不要输出 outline_patch，除非作者目标明确要求调整卷章结构。"#
        .into()
}

pub fn agent_synthesis_user_prompt(context: &Value, scope: &str, output: &str) -> String {
    format!(
        "任务范围：{scope}\n期望输出：{output}\n\n只读项目快照：\n{}",
        serde_json::to_string_pretty(context).unwrap_or_else(|_| context.to_string())
    )
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
- 项目快照会提供系统建议的下一步。回答末尾用一行“下一步：”给出一个最具体、立即可执行的动作，并说明完成标准。
- 回答使用中文，简洁具体。"#
    );
    if let Some(premise) = premise {
        prompt.push_str(&format!(
            "\n\n当前种子卡（若 status 不是 confirmed，则仍是候选，不是正式创作契约）：\n{premise}"
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

/// 模型请求调用的一个工具。`arguments` 是模型给出的原始 JSON 字符串。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Default)]
pub struct ToolTurnOutput {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: Option<String>,
}

/// 把流式 `delta.tool_calls` 分片按 index 拼回完整调用。
#[derive(Debug, Default)]
pub struct ToolCallAccumulator {
    calls: Vec<ToolCall>,
}

impl ToolCallAccumulator {
    pub fn push(&mut self, delta: &Value) {
        let Some(items) = delta.as_array() else {
            return;
        };
        for item in items {
            let index = item
                .get("index")
                .and_then(Value::as_u64)
                .map(|index| index as usize)
                .unwrap_or(self.calls.len().saturating_sub(1));
            while self.calls.len() <= index {
                self.calls.push(ToolCall::default());
            }
            let call = &mut self.calls[index];
            if let Some(id) = item.get("id").and_then(Value::as_str) {
                if !id.is_empty() {
                    call.id = id.to_string();
                }
            }
            if let Some(name) = item.pointer("/function/name").and_then(Value::as_str) {
                call.name.push_str(name);
            }
            if let Some(arguments) = item.pointer("/function/arguments").and_then(Value::as_str) {
                call.arguments.push_str(arguments);
            }
        }
    }

    pub fn finish(self) -> Vec<ToolCall> {
        self.calls
            .into_iter()
            .enumerate()
            .filter(|(_, call)| !call.name.trim().is_empty())
            .map(|(index, mut call)| {
                if call.id.is_empty() {
                    call.id = format!("call_{index}");
                }
                call
            })
            .collect()
    }
}

impl LlmClient {
    /// Agent harness 的一轮模型调用：流式输出正文，同时累积原生工具调用。
    /// `tools` 为 None 时只请求文本（用于文本协议回退）。
    pub async fn stream_tool_turn<F>(
        &self,
        messages: &[Value],
        tools: Option<&Value>,
        cancel: Arc<AtomicBool>,
        mut on_delta: F,
    ) -> AppResult<ToolTurnOutput>
    where
        F: FnMut(&str) + Send,
    {
        let mut body = json!({
            "model": self.model,
            "messages": messages,
            "temperature": self.temperature,
            "max_tokens": self.max_tokens,
            "stream": true
        });
        if let Some(tools) = tools {
            body["tools"] = tools.clone();
            body["tool_choice"] = json!("auto");
        }
        let response = self
            .http
            .post(&self.base_url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Message(format!("模型请求失败：{e}")))?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::Message(format_http_error(status.as_u16(), &body)));
        }
        let mut stream = response.bytes_stream();
        let mut pending = String::new();
        let mut output = ToolTurnOutput::default();
        let mut calls = ToolCallAccumulator::default();
        let mut handle_line = |line: &str, output: &mut ToolTurnOutput| {
            let Some(data) = line.strip_prefix("data:").map(str::trim) else {
                return;
            };
            if data.is_empty() || data == "[DONE]" {
                return;
            }
            let Ok(value) = serde_json::from_str::<Value>(data) else {
                return;
            };
            if let Some(delta) = value
                .pointer("/choices/0/delta/content")
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
            {
                output.content.push_str(delta);
                on_delta(delta);
            }
            if let Some(tool_calls) = value.pointer("/choices/0/delta/tool_calls") {
                calls.push(tool_calls);
            }
            if let Some(reason) = value
                .pointer("/choices/0/finish_reason")
                .and_then(Value::as_str)
            {
                output.finish_reason = Some(reason.to_string());
            }
        };
        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::Relaxed) {
                return Err(AppError::Message("生成已取消".into()));
            }
            let chunk = chunk.map_err(|e| AppError::Message(format!("读取流式响应失败：{e}")))?;
            pending.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(index) = pending.find('\n') {
                let line = pending[..index].trim_end_matches('\r').to_string();
                pending.drain(..=index);
                handle_line(&line, &mut output);
            }
        }
        handle_line(pending.trim(), &mut output);
        drop(handle_line);
        output.tool_calls = calls.finish();
        if output.content.is_empty() && output.tool_calls.is_empty() {
            return Err(AppError::Message(
                "模型没有返回文本或工具调用。请检查模型是否支持流式输出。".into(),
            ));
        }
        Ok(output)
    }
}

/// 网关拒绝 `tools` 参数时（不支持函数调用的模型），harness 改用文本协议。
pub fn is_tool_protocol_unsupported(message: &str) -> bool {
    let lower = message.to_lowercase();
    (lower.contains("400") || lower.contains("422") || lower.contains("not support"))
        && ["tool", "function"].iter().any(|needle| lower.contains(needle))
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    use super::*;

    #[test]
    fn accumulates_streamed_tool_call_fragments() {
        let mut calls = ToolCallAccumulator::default();
        calls.push(&json!([{"index":0,"id":"call_a","function":{"name":"read_","arguments":"{\"pa"}}]));
        calls.push(&json!([{"index":0,"function":{"name":"path","arguments":"th\":\"/人物\"}"}}]));
        calls.push(&json!([{"index":1,"function":{"name":"search","arguments":"{}"}}]));
        let calls = calls.finish();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "read_path");
        assert_eq!(calls[0].arguments, "{\"path\":\"/人物\"}");
        assert_eq!(calls[1].id, "call_1");
    }

    #[test]
    fn recognizes_tool_protocol_rejection() {
        assert!(is_tool_protocol_unsupported(
            "模型服务返回 400：This model does not support tools"
        ));
        assert!(!is_tool_protocol_unsupported("模型服务返回 401：invalid key"));
    }

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

    #[test]
    fn turns_gateway_html_into_readable_timeout_error() {
        let message = format_http_error(
            504,
            "<html><head><title>504 Gateway Time-out</title></head><body>nginx</body></html>",
        );
        assert!(message.contains("网关超时"));
        assert!(!message.contains("<html>"));
        assert!(is_transient_model_error(&message));
    }

    #[test]
    fn does_not_retry_authentication_errors() {
        let message = format_http_error(401, r#"{"error":"invalid api key"}"#);
        assert!(!is_transient_model_error(&message));
    }

    #[tokio::test]
    async fn streams_long_structured_response() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 4096];
            let _ = stream.read(&mut request);
            let events = concat!(
                "data: {\"choices\":[{\"delta\":{\"content\":\"{\\\"ok\\\":\"}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"true}\"}}]}\n\n",
                "data: [DONE]\n\n"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                events.len(),
                events
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        let client = LlmClient::new(
            &ModelConfig {
                base_url: format!("http://{address}/v1"),
                model: "stream-test".into(),
                temperature: 0.2,
                max_tokens: 1000,
                embedding_mode: "off".into(),
                embedding_model: None,
                embedding_base_url: None,
                embedding_dimensions: None,
                embedding_batch_size: 32,
                embedding_auto_index: false,
                has_api_key: true,
            },
            "test-key".into(),
        )
        .unwrap();
        let text = client
            .complete_streaming(&[ChatTurn::user("return json")], 100)
            .await
            .unwrap();
        assert_eq!(text, "{\"ok\":true}");
        server.join().unwrap();
    }
}
