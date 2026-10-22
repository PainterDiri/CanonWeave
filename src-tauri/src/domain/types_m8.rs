use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolDefinition {
    pub name: String,
    pub description: String,
    #[specta(type = specta_typescript::Any)]
    pub parameter_schema: Value,
    pub access: String,
    pub risk_level: String,
    pub requires_approval: bool,
    pub retryable: bool,
    pub output_target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentRun {
    pub id: String,
    pub session_id: Option<String>,
    pub mode: String,
    pub scope: String,
    pub output: String,
    pub goal: String,
    pub status: String,
    #[specta(type = specta_typescript::Any)]
    pub plan: Value,
    pub current_step: i32,
    pub approval_policy: String,
    pub model: String,
    pub context_hash: String,
    pub error: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub pending_candidates: i32,
    pub total_candidates: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentStep {
    pub id: String,
    pub run_id: String,
    pub seq: i32,
    pub kind: String,
    pub tool_name: String,
    #[specta(type = specta_typescript::Any)]
    pub input: Value,
    #[specta(type = specta_typescript::Any)]
    pub output: Value,
    pub status: String,
    pub risk_level: String,
    pub requires_approval: bool,
    pub candidate_ids: Vec<String>,
    #[specta(type = specta_typescript::Any)]
    pub evidence: Value,
    pub error: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentEvidence {
    pub id: String,
    pub run_id: String,
    pub step_id: String,
    pub source_kind: String,
    pub source_id: String,
    pub source_span: String,
    pub quote: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunDetail {
    pub run: AgentRun,
    pub steps: Vec<AgentStep>,
    pub candidates: Vec<crate::domain::Candidate>,
    pub evidence: Vec<AgentEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunCreateInput {
    pub goal: String,
    pub mode: String,
    pub scope: String,
    pub output: String,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentProgressEvent {
    pub run_id: String,
    pub status: String,
    pub current_step: i32,
    pub total_steps: i32,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentNextAction {
    pub key: String,
    pub label: String,
    pub description: String,
    pub prompt: String,
    pub action: String,
    pub priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StyleRecommendation {
    pub preset_id: String,
    pub preset_name: String,
    pub reason: String,
    pub message: crate::domain::ChatMessage,
    pub candidate: crate::domain::Candidate,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FoundationCheck {
    pub code: String,
    pub label: String,
    pub severity: String,
    pub message: String,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FoundationReport {
    pub ready: bool,
    pub passed_count: i32,
    pub total_count: i32,
    pub error_count: i32,
    pub warning_count: i32,
    pub checks: Vec<FoundationCheck>,
}
