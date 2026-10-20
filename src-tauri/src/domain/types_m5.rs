use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WikiPage {
    pub id: String,
    pub entity_id: Option<String>,
    pub kind: String,
    pub title: String,
    pub content: String,
    pub content_hash: String,
    pub source_ids: Vec<String>,
    pub source_spans: Vec<String>,
    pub generator_version: i32,
    pub edited_by_author: bool,
    pub generated_at: String,
    pub stale_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WikiRebuildResult {
    pub pages: Vec<WikiPage>,
    pub characters: i32,
    pub places: i32,
    pub rules: i32,
    pub timelines: i32,
    pub promises: i32,
    pub chapters: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WikiEditInput {
    pub page_id: String,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceDimensions {
    pub avg_sentence_len: i32,
    pub sentence_variance: i32,
    pub punctuation_per_100: i32,
    pub pronoun_ratio: i32,
    pub dialogue_action_ratio: i32,
    pub dialogue_ratio: i32,
    pub preferred_pronoun: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceThresholds {
    pub sentence_len: i32,
    pub sentence_variance: i32,
    pub punctuation: i32,
    pub pronoun: i32,
    pub dialogue_action: i32,
}

impl Default for VoiceThresholds {
    fn default() -> Self {
        Self {
            sentence_len: 10,
            sentence_variance: 12,
            punctuation: 5,
            pronoun: 6,
            dialogue_action: 25,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CharacterVoiceSample {
    pub id: String,
    pub character_id: String,
    pub text: String,
    pub source_span: String,
    pub source_type: String,
    pub metrics: VoiceDimensions,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CharacterVoiceProfile {
    pub character_id: String,
    pub centroid: VoiceDimensions,
    pub thresholds: VoiceThresholds,
    pub sample_count: i32,
    pub version: i32,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceSampleInput {
    pub character_id: String,
    pub text: String,
    pub source_span: String,
    pub source_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceThresholdInput {
    pub character_id: String,
    pub thresholds: VoiceThresholds,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceSegment {
    pub index: i32,
    pub kind: String,
    pub start: i32,
    pub end: i32,
    pub excerpt: String,
    pub metrics: VoiceDimensions,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceDrift {
    pub dimension: String,
    pub current: i32,
    pub target: i32,
    pub delta: i32,
    pub severity: String,
    pub segment_index: i32,
    pub start: i32,
    pub end: i32,
    pub excerpt: String,
    pub suggestion: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceCheck {
    pub character_id: String,
    pub centroid: VoiceDimensions,
    pub segments: Vec<VoiceSegment>,
    pub drifts: Vec<VoiceDrift>,
    pub voice_drift: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PreferenceTarget {
    pub avg_sentence_len: i32,
    pub dialogue_ratio: i32,
    pub sensory_per_1000: i32,
    pub metaphor_per_1000: i32,
    pub adverb_per_1000: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PreferenceSummary {
    pub choices: i32,
    pub target: Option<PreferenceTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MotifRecord {
    pub id: String,
    pub name: String,
    pub meaning: String,
    pub channels: Vec<String>,
    pub first_scene_id: Option<String>,
    pub last_scene_id: Option<String>,
    pub use_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MotifHint {
    pub motif: String,
    pub count: i32,
    pub per_1000: i32,
    pub severity: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MotifCheck {
    pub chars: i32,
    pub motifs: Vec<MotifRecord>,
    pub hints: Vec<MotifHint>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportInput {
    pub format: String,
    pub include_wiki: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub path: String,
    pub format: String,
    pub chapters: i32,
    pub words: i32,
    pub includes_wiki: bool,
}
