use tauri::State;

use crate::{
    domain::{AuditIssue, AuditRunResult, IssueActionInput, RuleRecord},
    error::IpcResult,
    rules,
    state::AppState,
};

#[tauri::command]
#[specta::specta]
pub fn rule_list(state: State<'_, AppState>) -> IpcResult<Vec<RuleRecord>> {
    state
        .with_project(|loaded| rules::list(&loaded.conn, &loaded.summary.id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn audit_run(state: State<'_, AppState>) -> IpcResult<AuditRunResult> {
    state
        .with_project(|loaded| rules::run_audit(&loaded.conn, &loaded.summary.id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn issue_list(
    state: State<'_, AppState>,
    status: Option<String>,
) -> IpcResult<Vec<AuditIssue>> {
    state
        .with_project(|loaded| rules::issues(&loaded.conn, &loaded.summary.id, status.as_deref()))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn issue_action(
    state: State<'_, AppState>,
    input: IssueActionInput,
) -> IpcResult<AuditIssue> {
    state
        .with_project(|loaded| {
            rules::issue_action(
                &loaded.conn,
                &loaded.summary.id,
                &input.id,
                &input.action,
                &input.reason,
            )
        })
        .map_err(|e| e.to_string())
}
