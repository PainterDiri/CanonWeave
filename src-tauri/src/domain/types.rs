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
    pub knowledge_entries: i32,
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
    #[serde(default)]
    pub embedding_mode: String,
    pub embedding_model: Option<String>,
    #[serde(default)]
    pub embedding_base_url: Option<String>,
    #[serde(default)]
    pub embedding_dimensions: Option<i32>,
    #[serde(default = "default_embedding_batch_size")]
    pub embedding_batch_size: i32,
    #[serde(default)]
    pub embedding_auto_index: bool,
    pub has_api_key: bool,
}

fn default_embedding_batch_size() -> i32 {
    32
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o-mini".into(),
            temperature: 0.8,
            max_tokens: 4096,
            embedding_mode: "off".into(),
            embedding_model: None,
            embedding_base_url: None,
            embedding_dimensions: None,
            embedding_batch_size: default_embedding_batch_size(),
            embedding_auto_index: false,
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
    #[serde(default)]
    pub embedding_mode: String,
    pub embedding_model: Option<String>,
    #[serde(default)]
    pub embedding_base_url: Option<String>,
    #[serde(default)]
    pub embedding_dimensions: Option<i32>,
    #[serde(default = "default_embedding_batch_size")]
    pub embedding_batch_size: i32,
    #[serde(default)]
    pub embedding_auto_index: bool,
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
