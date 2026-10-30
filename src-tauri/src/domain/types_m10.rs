use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Candidate, ChatMessage, ChatSession};

/// 作者守则：项目级长期指令，注入 Agent 与生成任务契约。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGuide {
    pub content: String,
    pub updated_at: Option<String>,
}

/// 叙事线程：环境（milieu）、疑问（inquiry）、人物（character）、事件（event）。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StoryThread {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub question: String,
    pub open_chapter: Option<i32>,
    pub close_chapter: Option<i32>,
    pub status: String,
    pub notes: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StoryThreadInput {
    pub id: Option<String>,
    pub kind: String,
    pub title: String,
    pub question: String,
    pub open_chapter: Option<i32>,
    pub close_chapter: Option<i32>,
    pub status: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CraftCheck {
    pub code: String,
    /// error / warning / info
    pub severity: String,
    pub chapter: Option<i32>,
    pub thread_id: Option<String>,
    pub message: String,
    pub suggestion: String,
}

/// 章节工艺与叙事线程的确定性诊断。只提示，不阻断，也不改写作者内容。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CraftReport {
    pub chapter_count: i32,
    pub chapters_with_shift: i32,
    pub chapters_with_hook: i32,
    pub open_threads: i32,
    pub checks: Vec<CraftCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PipelineStage {
    pub key: String,
    pub label: String,
    pub description: String,
    /// done / active / todo / optional
    pub status: String,
    pub progress: f64,
    pub detail: String,
    /// 前端导航键
    pub nav: String,
    /// 交给 Agent 的一句话任务
    pub prompt: String,
}

/// 创作流水线：从前提到定稿的阶段门槛。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowPipeline {
    pub stages: Vec<PipelineStage>,
    pub active_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HarnessToolTrace {
    pub step_id: String,
    pub seq: i32,
    pub tool_name: String,
    pub title: String,
    #[specta(type = specta_typescript::Any)]
    pub input: Value,
    pub summary: String,
    pub status: String,
    pub candidate_ids: Vec<String>,
    /// ask_author 选项卡、工作流产物的确认卡等；无动作时为 null。
    #[specta(type = specta_typescript::Any)]
    pub action: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HarnessTurnResult {
    pub run_id: String,
    pub session: ChatSession,
    pub user_message: ChatMessage,
    pub assistant_message: ChatMessage,
    pub traces: Vec<HarnessToolTrace>,
    pub candidates: Vec<Candidate>,
    #[specta(type = specta_typescript::Any)]
    pub plan: Value,
    #[specta(type = specta_typescript::Any)]
    pub actions: Value,
}

/// `harness://event` 流式事件：delta / tool_start / tool_end / plan / done / error。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HarnessEvent {
    pub session_id: String,
    pub run_id: String,
    pub kind: String,
    pub text: String,
    #[specta(type = specta_typescript::Any)]
    pub data: Value,
}
