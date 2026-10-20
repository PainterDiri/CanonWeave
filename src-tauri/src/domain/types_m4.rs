use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BranchRecord {
    pub id: String,
    pub name: String,
    pub parent_branch_id: Option<String>,
    pub status: String,
    pub created_at: String,
    pub is_current: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TimelineItem {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub story_time: String,
    pub reveal_time: String,
    pub branch_id: Option<String>,
    pub source: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub community: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub id: String,
    pub from: String,
    pub to: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CommunityReport {
    pub id: String,
    pub level: i32,
    pub title: String,
    pub member_ids: Vec<String>,
    pub source_ids: Vec<String>,
    pub content: String,
    pub summary_hash: String,
    pub generated_at: String,
    pub stale_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct InsightResult {
    pub question: String,
    pub answer: String,
    pub source_ids: Vec<String>,
    pub community_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ImpactItem {
    pub kind: String,
    pub id: String,
    pub label: String,
    pub reason: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ImpactSet {
    pub target_id: String,
    pub target_label: String,
    pub items: Vec<ImpactItem>,
}
