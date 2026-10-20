use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChapterRecord {
    pub id: String,
    pub number: i32,
    pub title: String,
    pub status: String,
    pub current_revision_id: Option<String>,
    pub style_profile_id: Option<String>,
    pub style_epoch: i32,
    pub scene_count: i32,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EmotionBeat {
    pub target: String,
    pub intensity: i32,
    pub entry: String,
    pub exit: String,
    pub turn: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SensoryPlan {
    pub channels: Vec<String>,
    pub motifs: Vec<String>,
    pub distance: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SceneRecord {
    pub id: String,
    pub chapter_id: String,
    pub number: i32,
    pub goal: String,
    pub conflict: String,
    pub pov: String,
    pub story_time: String,
    pub branch_id: Option<String>,
    pub place_id: Option<String>,
    pub style_profile_id: Option<String>,
    pub emotion_beat: EmotionBeat,
    pub sensory_plan: SensoryPlan,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SceneInput {
    pub id: Option<String>,
    pub chapter_id: String,
    pub goal: String,
    pub conflict: String,
    pub pov: String,
    pub story_time: String,
    pub place_id: Option<String>,
    pub style_profile_id: Option<String>,
    pub emotion_beat: EmotionBeat,
    pub sensory_plan: SensoryPlan,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ContextSection {
    pub layer: String,
    pub title: String,
    pub content: String,
    pub chars: i32,
    pub budget: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ContextPreview {
    pub sections: Vec<ContextSection>,
    pub total_chars: i32,
    pub budget: i32,
    pub context_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GenerationInput {
    pub scene_id: String,
    pub mode: String,
    pub style_profile_id: Option<String>,
    pub extra_instruction: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GenerationStartResult {
    pub run_id: String,
    pub context_hash: String,
    pub budget: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GenerationDelta {
    pub run_id: String,
    pub delta: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GenerationFinished {
    pub run_id: String,
    pub candidate_id: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GenerationError {
    pub run_id: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GenerationRunRow {
    pub id: String,
    pub scene_id: Option<String>,
    pub mode: String,
    pub status: String,
    pub model: String,
    pub context_hash: String,
    pub error: String,
    pub created_at: String,
    pub candidate_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BeatOption {
    pub title: String,
    pub beat: String,
    pub turn: String,
    pub cost: String,
    pub scene_goal: String,
    pub scene_conflict: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlotBeatsInput {
    pub session_id: String,
    pub instruction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DraftAcceptResult {
    pub chapter_id: String,
    pub revision_id: String,
    pub snapshot_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BatchPreview {
    pub ids: Vec<String>,
    pub count: i32,
    pub l1: i32,
    pub l2: i32,
    pub l3: i32,
    pub blocked: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BatchResult {
    pub batch_id: String,
    pub revision_id: String,
    pub accepted: i32,
    pub rejected: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub source_id: String,
    pub chapter_id: Option<String>,
    pub excerpt: String,
    pub score: i32,
}
