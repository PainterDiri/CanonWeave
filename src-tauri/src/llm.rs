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
    r#"你是长篇叙事策划，服务于中文网络小说与游戏剧情作者。
作者会给出一个 logline（可能只有一个画面或冲突）。请提出三种差异明显、都能支撑长篇的走向。

要求：
1. 三种走向的主角欲望、阻力来源和叙事承诺必须不同，不要只是换措辞。
2. 每种走向必须具体到“谁想要什么、什么在阻止、代价是什么”，避免抽象主题词。
3. 只输出 JSON 数组，不要解释，不要 Markdown 代码块。
4. 每项只输出以下精简字段，不要附加解释或重复内容：
   {
     "logline": "一句话故事",
     "genre": "类型",
     "theme": "核心命题",
     "tone": "情绪基调",
     "core_conflict": "谁想要什么、什么在阻止、代价是什么",
     "protagonist_desire": "主角想要什么",
     "flaw": "主角缺陷",
     "world_hook": ["1-2 条世界规则钩子"],
     "pov": "叙事视角",
     "taboo": ["应避免的写法"]
   }
5. 所有文本使用中文。"#
        .into()
}

pub fn seed_user_prompt(logline: &str) -> String {
    format!("作者的一句话或灵感：\n{logline}\n\n请给出三种走向。")
}

pub fn seed_compact_system_prompt() -> String {
    r#"你是中文长篇小说策划。根据作者灵感给出 3 个差异明显的长篇走向。
只输出一个合法 JSON 数组，不要 Markdown、解释、前后缀或省略号。
必须完整闭合数组和全部对象。每项只包含：
{"logline":"一句话故事","genre":"类型","theme":"核心命题","tone":"情绪基调","core_conflict":"谁想要什么、什么在阻止、代价是什么","protagonist_desire":"主角欲望","flaw":"主角缺陷","world_hook":["世界规则"],"pov":"叙事视角","taboo":["应避免的写法"]}
三个走向的主角欲望、阻力和叙事承诺必须不同。每项文本精简，确保数组完整。使用中文。"#
        .into()
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
    r#"你是长篇小说的章节结构编辑。按照作者定义的总篇幅和当前批次范围生成章节骨架。
每一章必须回答：目标、冲突、转折、兑现（回收或新增哪条伏笔）。相邻章节要推进因果，不平均分配情节。
本批次的 number 必须连续，数量和范围以用户要求为准。只输出 JSON 数组，不要 Markdown。每项字段：
{"number":1,"title":"章名","goal":"本章目标","conflict":"本章冲突","turn":"本章转折","payoff":"伏笔操作"}
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
