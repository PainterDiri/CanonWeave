use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Premise {
    pub id: String,
    pub logline: String,
    pub genre: String,
    pub subgenre: String,
    pub theme: String,
    pub emotional_promise: String,
    pub tone: String,
    pub core_conflict: String,
    pub dramatic_question: String,
    pub antagonistic_force: String,
    pub world_rule: String,
    pub ending_direction: String,
    pub protagonist_desire: String,
    pub flaw: String,
    pub secret: String,
    pub world_hook: Vec<String>,
    pub pov: String,
    pub person: String,
    pub tense: String,
    pub target_length: String,
    pub update_rhythm: String,
    pub ending_intent: String,
    pub taboo: Vec<String>,
    pub status: String,
    pub source_message_id: Option<String>,
    #[specta(type = specta_typescript::Any)]
    pub field_sources: Value,
    pub version: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PremiseInput {
    pub logline: String,
    pub genre: String,
    pub subgenre: String,
    pub theme: String,
    pub emotional_promise: String,
    pub tone: String,
    pub core_conflict: String,
    pub dramatic_question: String,
    pub antagonistic_force: String,
    pub world_rule: String,
    pub ending_direction: String,
    pub protagonist_desire: String,
    pub flaw: String,
    pub secret: String,
    pub world_hook: Vec<String>,
    pub pov: String,
    pub person: String,
    pub tense: String,
    pub target_length: String,
    pub update_rhythm: String,
    pub ending_intent: String,
    pub taboo: Vec<String>,
    pub source_message_id: Option<String>,
    pub author_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SeedGenerateInput {
    pub logline: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SeedGenerateResult {
    pub directions: Vec<Premise>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatSession {
    pub id: String,
    pub scope: String,
    pub scene_id: Option<String>,
    pub branch_id: Option<String>,
    pub parent_session_id: Option<String>,
    pub mode: String,
    pub title: String,
    pub model: String,
    pub status: String,
    pub summary: String,
    pub last_message_at: Option<String>,
    pub created_at: String,
    pub pinned: bool,
    pub archived_at: Option<String>,
    pub pending_candidates: i32,
    pub message_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub session_id: String,
    pub seq: i32,
    pub role: String,
    pub content: String,
    pub model: String,
    pub tokens: i32,
    pub created_at: String,
    pub linked_candidate_ids: Vec<String>,
    pub pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatSessionCreateInput {
    pub scope: String,
    pub mode: String,
    pub title: Option<String>,
    pub scene_id: Option<String>,
    pub branch_id: Option<String>,
    pub parent_session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatSendInput {
    pub session_id: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatSendResult {
    pub user_message: ChatMessage,
    pub assistant_message: ChatMessage,
    pub session: ChatSession,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatDeltaEvent {
    pub session_id: String,
    pub request_id: String,
    pub delta: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatFinishedEvent {
    pub session_id: String,
    pub request_id: String,
    pub message: ChatMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatErrorEvent {
    pub session_id: String,
    pub request_id: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub id: String,
    pub kind: String,
    #[specta(type = specta_typescript::Any)]
    pub payload: Value,
    pub source_span: Option<String>,
    pub source_message_id: Option<String>,
    pub confidence: f64,
    pub risk_level: String,
    pub status: String,
    pub batch_id: Option<String>,
    pub created_at: String,
    pub confirmed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CandidateCaptureInput {
    pub session_id: String,
    pub message_id: String,
    pub kind: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CandidateAcceptResult {
    pub candidate: Candidate,
    pub created_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CandidateCounts {
    pub l0: i32,
    pub l1: i32,
    pub l2: i32,
    pub l3: i32,
    pub total: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReviewQueue {
    pub candidates: Vec<Candidate>,
    pub counts: CandidateCounts,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OutlineNode {
    pub id: String,
    pub parent_id: Option<String>,
    pub level: String,
    pub order_no: i32,
    pub title: String,
    pub goal: String,
    pub conflict: String,
    pub turn: String,
    pub payoff: String,
    pub logic_node_ids: Vec<String>,
    pub presentation_mode: String,
    pub narrative_function: String,
    pub status: String,
    pub branch_id: Option<String>,
    pub source: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OutlineUpdateInput {
    pub id: String,
    pub title: String,
    pub goal: String,
    pub conflict: String,
    pub turn: String,
    pub payoff: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SourceDocument {
    pub id: String,
    pub path: String,
    pub hash: String,
    pub doc_type: String,
    pub title: String,
    pub word_count: i32,
    pub imported_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub document: SourceDocument,
    pub chapters_created: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionResult {
    pub candidates_created: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Entity {
    pub id: String,
    pub entity_type: String,
    pub canonical_name: String,
    pub aliases: Vec<String>,
    pub status: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CharacterDetail {
    pub entity: Entity,
    pub desire: String,
    pub need: String,
    pub fear: String,
    pub flaw: String,
    pub values: Vec<String>,
    pub secret: String,
    pub taboo: Vec<String>,
    pub voice_anchor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CharacterInput {
    pub entity_id: Option<String>,
    pub canonical_name: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub desire: String,
    pub need: String,
    pub fear: String,
    pub flaw: String,
    pub values: Vec<String>,
    pub secret: String,
    pub taboo: Vec<String>,
    pub voice_anchor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PromiseRecord {
    pub id: String,
    pub title: String,
    pub state: String,
    pub plant_scene_id: Option<String>,
    pub target_window: String,
    pub payoff_contract: String,
    pub evidence_ids: Vec<String>,
    pub plant_evidence_ids: Vec<String>,
    pub payoff_evidence_ids: Vec<String>,
    pub payoff_state_change: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PromiseInput {
    pub id: Option<String>,
    pub title: String,
    pub target_window: String,
    pub payoff_contract: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PromiseStateInput {
    pub id: String,
    pub state: String,
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PromisePayoffInput {
    pub id: String,
    pub source_id: String,
    pub quote: String,
    pub state_change: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PayoffCheck {
    pub ready: bool,
    pub blockers: Vec<String>,
    pub plant_evidence_ids: Vec<String>,
    pub payoff_evidence_ids: Vec<String>,
    pub state_change: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PremiseSources {
    #[specta(type = specta_typescript::Any)]
    pub field_sources: Value,
    pub source_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OutlineStatus {
    pub has_skeleton: bool,
    pub confirmed: i32,
}
