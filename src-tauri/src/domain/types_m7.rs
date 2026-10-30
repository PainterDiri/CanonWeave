use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NarrativeBlueprint {
    pub id: String,
    pub premise_id: Option<String>,
    pub title: String,
    pub medium: String,
    pub structure_strategy: String,
    pub target_units: Option<i32>,
    pub core_dramatic_question: String,
    pub narrative_promise: String,
    pub opening_state: String,
    pub ending_state: String,
    pub theme_question: String,
    pub status: String,
    pub completeness: i32,
    pub version: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NarrativeElement {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub role: String,
    pub summary: String,
    pub external_goal: String,
    pub internal_need: String,
    pub fear: String,
    pub flaw: String,
    pub leverage: String,
    pub constraints: Vec<String>,
    pub secret: String,
    #[specta(type = specta_typescript::Any)]
    pub relations: Value,
    #[specta(type = specta_typescript::Any)]
    pub metadata: Value,
    pub status: String,
    pub sort_order: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NarrativeTimelineEvent {
    pub id: String,
    pub order_no: i32,
    pub title: String,
    pub story_time: String,
    pub cause: String,
    pub participants: Vec<String>,
    pub outcome: String,
    pub consequence: String,
    pub reveal_at: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NarrativeThread {
    pub id: String,
    pub order_no: i32,
    pub kind: String,
    pub title: String,
    pub dramatic_question: String,
    pub opening_state: String,
    pub target_state: String,
    pub stakes: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NarrativeLogicNode {
    pub id: String,
    pub thread_id: Option<String>,
    pub thread_title: String,
    pub order_no: i32,
    pub kind: String,
    pub title: String,
    pub cause: String,
    pub choice: String,
    pub cost: String,
    pub outcome: String,
    pub state_change: String,
    pub knowledge_change: String,
    pub participants: Vec<String>,
    pub location: String,
    pub story_time: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NarrativeLogicEdge {
    pub id: String,
    pub from_node_id: String,
    pub from_title: String,
    pub to_node_id: String,
    pub to_title: String,
    pub relation: String,
    pub condition: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NarrativePresentation {
    pub id: String,
    pub logic_node_id: String,
    pub logic_node_title: String,
    pub medium: String,
    pub mode: String,
    pub pov: String,
    pub dialogue_ratio: i32,
    pub narration_ratio: i32,
    pub player_agency: String,
    pub visual_requirements: String,
    pub audio_requirements: String,
    pub interaction: String,
    pub deliverable: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NarrativeDiagnostic {
    pub code: String,
    pub severity: String,
    pub message: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NarrativeBlueprintView {
    pub blueprint: NarrativeBlueprint,
    pub elements: Vec<NarrativeElement>,
    pub timeline: Vec<NarrativeTimelineEvent>,
    pub threads: Vec<NarrativeThread>,
    pub nodes: Vec<NarrativeLogicNode>,
    pub edges: Vec<NarrativeLogicEdge>,
    pub presentations: Vec<NarrativePresentation>,
    pub diagnostics: Vec<NarrativeDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NarrativeGenerateInput {
    pub medium: String,
    pub structure_strategy: String,
    pub target_units: Option<i32>,
    /// quick 单次生成；standard / epic 分层构建故事圣经。缺省时按种子卡篇幅推断。
    #[serde(default)]
    pub depth: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NarrativeBlueprintEditInput {
    pub id: String,
    pub title: String,
    pub medium: String,
    pub structure_strategy: String,
    pub target_units: Option<i32>,
    pub core_dramatic_question: String,
    pub narrative_promise: String,
    pub opening_state: String,
    pub ending_state: String,
    pub theme_question: String,
    pub elements: Vec<NarrativeElement>,
    pub timeline: Vec<NarrativeTimelineEvent>,
    pub threads: Vec<NarrativeThread>,
    pub nodes: Vec<NarrativeLogicNode>,
    pub edges: Vec<NarrativeLogicEdge>,
    pub presentations: Vec<NarrativePresentation>,
}
