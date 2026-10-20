use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub schema_version: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub mode: String,
    pub active_branch_id: Option<String>,
    pub writing_language: String,
    pub schema_version: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCounts {
    pub premises: i32,
    pub sessions: i32,
    pub pending_candidates: i32,
    pub chapters: i32,
    pub entities: i32,
    pub open_issues: i32,
    pub promises: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSnapshot {
    pub project: ProjectSummary,
    pub counts: ProjectCounts,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectInput {
    pub name: String,
    pub root_path: String,
    pub mode: String,
    pub writing_language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfig {
    pub base_url: String,
    pub model: String,
    pub temperature: f64,
    pub max_tokens: i32,
    pub embedding_model: Option<String>,
    pub has_api_key: bool,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o-mini".into(),
            temperature: 0.8,
            max_tokens: 4096,
            embedding_model: None,
            has_api_key: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfigInput {
    pub base_url: String,
    pub model: String,
    pub temperature: f64,
    pub max_tokens: i32,
    pub embedding_model: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModelTestResult {
    pub ok: bool,
    pub message: String,
    pub latency_ms: u32,
    pub model: String,
}
