use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingStatus {
    pub configured: bool,
    pub mode: String,
    pub model: Option<String>,
    pub dimensions: Option<i32>,
    pub batch_size: i32,
    pub auto_index: bool,
    pub ready: i32,
    pub stale: i32,
    pub pending: i32,
    pub error: i32,
    pub other_model: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingIndexResult {
    pub model: String,
    pub dimensions: i32,
    pub indexed: i32,
    pub skipped: i32,
    pub failed: i32,
    pub remaining: i32,
    pub canceled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingProgressEvent {
    pub indexed: i32,
    pub total: i32,
    pub model: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HybridSearchHit {
    pub source_kind: String,
    pub source_id: String,
    pub chapter_id: Option<String>,
    pub excerpt: String,
    pub score: i32,
    pub fts_rank: Option<i32>,
    pub vector_rank: Option<i32>,
    pub graph_rank: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HybridSearchResult {
    pub hits: Vec<HybridSearchHit>,
    pub retrieval_mode: String,
    pub model: Option<String>,
    pub dimensions: Option<i32>,
    pub embedding_error: Option<String>,
}
