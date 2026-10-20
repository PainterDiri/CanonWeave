mod commands;
mod candidates;
mod chat;
mod context;
mod db;
mod domain;
mod entities;
mod error;
mod fact_delta;
mod branches;
mod graphrag;
mod import;
mod llm;
mod model;
mod outline;
mod project;
mod promises;
mod retrieval;
mod review;
mod revisions;
mod rules;
mod solver;
mod scenes;
mod seed;
mod state;
mod style;
mod wiki;
mod voice;
mod preferences;
mod motifs;
mod export;

use tauri_specta::{collect_commands, Builder};
use tauri::Manager;

use crate::{commands::*, state::AppState};

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn hash_text(text: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(text.as_bytes()))
}

#[tauri::command]
#[specta::specta]
fn app_info() -> domain::AppInfo {
    domain::AppInfo {
        name: "CanonWeave".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        schema_version: db::SCHEMA_VERSION as i32,
    }
}

fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new().commands(collect_commands![
        app_info, project_create, project_open, project_close, project_state, model_get,
        model_save, model_test, seed_generate, premise_list, premise_active, premise_update,
        premise_confirm, premise_archive, premise_debug_sources, chat_session_create,
        chat_session_list, chat_session_get, chat_session_update, chat_session_archive,
        chat_session_unarchive, chat_session_delete, chat_session_fork, chat_messages_list,
        chat_capture, chat_summary, chat_send, chat_cancel, chat_candidates, outline_list,
        outline_generate, outline_update, outline_confirm, outline_drop, outline_status,
        document_import, document_list, extraction_run, review_queue, candidate_list,
        candidate_accept, candidate_reject, candidate_update_payload, entity_list,
        character_list, character_save, promise_list, promise_save, promise_transition,
        promise_payoff_check, promise_payoff,
        style_preset_list, style_profile_list, style_profile_get, style_profile_save,
        style_profile_apply_preset, style_sample_add, style_sample_list, style_analyze,
        style_contract_text, style_check, style_compare, style_calibrate
        , chapter_list, scene_list, scene_save, scene_contract_text, context_preview,
        chapter_text_get, draft_accept, generation_start, generation_cancel, generation_runs,
        plot_beats, scene_from_beat, chat_continue, chat_to_scene, review_batch_preview,
        review_batch_apply, chapter_text_save, review_policy_get, review_policy_save,
        review_policy_apply, review_batch_undo, review_sample_list, rule_list, audit_run,
        issue_list, issue_action, draft_accept_variant
        , revision_list, revision_compare, revision_rollback, preference_save, branch_list,
        branch_create, branch_switch, timeline_data, graph_data, community_build,
        community_list, impact_analysis, insight_query, wiki_list, wiki_rebuild,
        wiki_propose_edit, voice_sample_list, voice_sample_add, voice_profile_get,
        voice_thresholds_save, voice_check, preference_summary, motif_list,
        motif_density_check, export_project
    ])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = specta_builder();

    #[cfg(debug_assertions)]
    builder
        .export(
            specta_typescript::Typescript::default(),
            concat!(env!("CARGO_MANIFEST_DIR"), "/../src/lib/bindings.ts"),
        )
        .expect("failed to export typescript bindings");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            let config_dir = app.path().app_config_dir()?;
            std::fs::create_dir_all(&config_dir)?;
            app.manage(AppState::new(config_dir));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod bindings_tests {
    #[test]
    fn export_typescript_bindings() {
        super::specta_builder()
            .export(
                specta_typescript::Typescript::default(),
                concat!(env!("CARGO_MANIFEST_DIR"), "/../src/lib/bindings.ts"),
            )
            .expect("failed to export typescript bindings");
    }
}
