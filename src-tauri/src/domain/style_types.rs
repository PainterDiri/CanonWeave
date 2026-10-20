use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StyleSyntax {
    pub avg_sentence_min: i32,
    pub avg_sentence_max: i32,
    pub short_ratio: i32,
    pub paragraph_sentences_min: i32,
    pub paragraph_sentences_max: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StyleLexicon {
    pub concrete_ratio: i32,
    pub register: String,
    pub adverb_density_max: i32,
    pub banned_words: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StyleImagery {
    pub sensory_channels: Vec<String>,
    pub motifs: Vec<String>,
    pub sensory_per_1000: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StyleRhetoric {
    pub metaphor_per_1000: i32,
    pub irony: i32,
    pub minimalism: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StyleEmotion {
    pub interiority: i32,
    pub externality: i32,
    pub intensity_min: i32,
    pub intensity_max: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StyleRatios {
    pub action: i32,
    pub dialogue: i32,
    pub description: i32,
    pub interiority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StyleProfile {
    pub id: String,
    pub scope: String,
    pub scope_id: Option<String>,
    pub name: String,
    pub pov: String,
    pub person: String,
    pub tense: String,
    pub syntax: StyleSyntax,
    pub lexicon: StyleLexicon,
    pub imagery: StyleImagery,
    pub rhetoric: StyleRhetoric,
    pub emotion: StyleEmotion,
    pub ratios: StyleRatios,
    pub anti_style: Vec<String>,
    pub version: i32,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StyleProfileInput {
    pub id: Option<String>,
    pub scope: String,
    pub scope_id: Option<String>,
    pub name: String,
    pub pov: String,
    pub person: String,
    pub tense: String,
    pub syntax: StyleSyntax,
    pub lexicon: StyleLexicon,
    pub imagery: StyleImagery,
    pub rhetoric: StyleRhetoric,
    pub emotion: StyleEmotion,
    pub ratios: StyleRatios,
    pub anti_style: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StylePreset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub syntax: StyleSyntax,
    pub lexicon: StyleLexicon,
    pub imagery: StyleImagery,
    pub rhetoric: StyleRhetoric,
    pub emotion: StyleEmotion,
    pub ratios: StyleRatios,
    pub anti_style: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StyleSample {
    pub id: String,
    pub profile_id: Option<String>,
    pub text: String,
    pub source_span: String,
    pub source_type: String,
    pub approved: bool,
    pub tags: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StyleSampleInput {
    pub profile_id: Option<String>,
    pub text: String,
    pub source_span: String,
    pub source_type: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TellIssue {
    pub excerpt: String,
    pub emotion: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StyleMetrics {
    pub chars: i32,
    pub sentences: i32,
    pub paragraphs: i32,
    pub avg_sentence_len: i32,
    pub p90_sentence_len: i32,
    pub sentence_variance: i32,
    pub avg_paragraph_len: i32,
    pub dialogue_ratio: i32,
    pub description_ratio: i32,
    pub sensory_per_1000: i32,
    pub adverb_per_1000: i32,
    pub metaphor_per_1000: i32,
    pub cliche_hits: Vec<String>,
    pub repeated_openings: Vec<String>,
    pub tell_without_show: Vec<TellIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StyleDrift {
    pub dimension: String,
    pub current: String,
    pub target: String,
    pub delta: String,
    pub severity: String,
    pub excerpt: String,
    pub suggestion: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StyleCheck {
    pub metrics: StyleMetrics,
    pub drifts: Vec<StyleDrift>,
    pub style_drift: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StyleCompare {
    pub left: StyleMetrics,
    pub right: StyleMetrics,
    pub deltas: Vec<StyleDrift>,
}
