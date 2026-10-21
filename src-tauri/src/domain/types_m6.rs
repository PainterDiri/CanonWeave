use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VolumePlanInput {
    pub title: Option<String>,
    pub chapter_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OutlineGenerateInput {
    pub mode: String,
    pub total_chapters: Option<i32>,
    pub volumes: Vec<VolumePlanInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeField {
    pub key: String,
    pub label: String,
    pub placeholder: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDomain {
    pub id: String,
    pub key: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub accent: String,
    pub is_system: bool,
    pub sort_order: i32,
    pub field_schema: Vec<KnowledgeField>,
    pub entry_count: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDomainInput {
    pub name: String,
    pub description: String,
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeEntry {
    pub id: String,
    pub entity_id: Option<String>,
    pub domain_id: String,
    pub domain_key: String,
    pub domain_name: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub summary: String,
    #[specta(type = specta_typescript::Any)]
    pub fields: Value,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeEntryInput {
    pub id: Option<String>,
    pub domain_id: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub summary: String,
    #[specta(type = specta_typescript::Any)]
    pub fields: Value,
}
