use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReviewPolicy {
    pub id: String,
    pub scope: String,
    pub scope_id: Option<String>,
    pub mode: String,
    pub auto_accept_level: String,
    pub sample_rate: i32,
    pub lock_hard: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReviewPolicyInput {
    pub scope: String,
    pub scope_id: Option<String>,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PolicyApplyResult {
    pub mode: String,
    pub accepted: i32,
    pub sampled: i32,
    pub batch_id: Option<String>,
    pub revision_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReviewSampleRow {
    pub id: String,
    pub batch_id: String,
    pub candidate_id: String,
    pub sampled: bool,
    pub review_status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BatchUndoResult {
    pub batch_id: String,
    pub revision_id: String,
    pub restored: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RuleRecord {
    pub id: String,
    pub code: String,
    pub constraint_layer: String,
    pub severity: String,
    #[specta(type = specta_typescript::Any)]
    pub predicate: Value,
    #[specta(type = specta_typescript::Any)]
    pub exception_scope: Value,
    pub description: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConflictCertificate {
    pub rule_id: String,
    pub severity: String,
    pub claim_ids: Vec<String>,
    pub evidence_spans: Vec<String>,
    pub solver_core: Vec<String>,
    pub explanation: String,
    pub suggested_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AuditIssue {
    pub id: String,
    pub rule_id: String,
    pub rule_code: String,
    pub constraint_layer: String,
    pub severity: String,
    pub claim_ids: Vec<String>,
    pub evidence_spans: Vec<String>,
    pub explanation: String,
    pub certificate: ConflictCertificate,
    pub solver_core: Vec<String>,
    pub risk_level: String,
    pub fingerprint: String,
    pub status: String,
    pub deferred_until: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AuditRunResult {
    pub issues_created: i32,
    pub issues: Vec<AuditIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct IssueActionInput {
    pub id: String,
    pub action: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FactChange {
    pub kind: String,
    pub before: String,
    pub after: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FactDelta {
    pub changes: Vec<FactChange>,
    pub blocked: bool,
    pub added_numbers: Vec<String>,
    pub removed_numbers: Vec<String>,
    pub added_entities: Vec<String>,
    pub removed_entities: Vec<String>,
    pub possible_inflation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RevisionRecord {
    pub id: String,
    pub parent_id: Option<String>,
    pub chapter_id: Option<String>,
    pub author_action: String,
    #[specta(type = specta_typescript::Any)]
    pub diff: Value,
    pub snapshot_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RevisionCompare {
    pub from_text: String,
    pub to_text: String,
    pub from_chars: i32,
    pub to_chars: i32,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PreferenceInput {
    pub scene_id: Option<String>,
    pub left_run_id: String,
    pub right_run_id: String,
    pub chosen_run_id: String,
    pub reason: String,
    pub left_text: Option<String>,
    pub right_text: Option<String>,
}
