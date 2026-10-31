use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use tauri::{Emitter, State};

use crate::{
    domain::{EmbeddingIndexResult, EmbeddingProgressEvent, EmbeddingStatus, ModelTestResult},
    embedding,
    error::IpcResult,
    retrieval::SemanticQuery,
    state::AppState,
};

/// 本地 Embedding 服务不需要密钥：只有 API 模式才读取钥匙串，避免为本地检索弹出授权框。
fn runtime_key(
    state: &AppState,
    runtime: &embedding::EmbeddingRuntime,
) -> Result<Option<String>, String> {
    if runtime.requires_api_key() {
        state.api_key().map_err(|error| error.to_string())
    } else {
        Ok(None)
    }
}

pub async fn semantic_query(
    state: &AppState,
    query: &str,
) -> (Option<SemanticQuery>, Option<String>) {
    let config = state.model_store.load();
    if !embedding::configured(&config) {
        return (None, None);
    }
    let runtime = match embedding::EmbeddingRuntime::from_config(&config) {
        Ok(runtime) => runtime,
        Err(error) => return (None, Some(error.to_string())),
    };
    let key = match runtime_key(state, &runtime) {
        Ok(key) => key,
        Err(error) => return (None, Some(error)),
    };
    match embedding::embed_query(&runtime, key.as_deref(), query).await {
        Ok(vector) => (
            Some(SemanticQuery {
                dimensions: vector.len() as i32,
                vector,
                model: runtime.model,
            }),
            None,
        ),
        Err(error) => (None, Some(error.to_string())),
    }
}

#[tauri::command]
#[specta::specta]
pub fn embedding_status(state: State<'_, AppState>) -> IpcResult<EmbeddingStatus> {
    let config = state.model_store.load();
    state
        .with_project(|loaded| embedding::status(&loaded.conn, &loaded.summary.id, &config))
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn embedding_test(state: State<'_, AppState>) -> IpcResult<ModelTestResult> {
    let started = Instant::now();
    let config = state.model_store.load();
    let runtime = match embedding::EmbeddingRuntime::from_config(&config) {
        Ok(runtime) => runtime,
        Err(error) => {
            return Ok(ModelTestResult {
                ok: false,
                message: error.to_string(),
                latency_ms: started.elapsed().as_millis() as u32,
                model: config.embedding_model.unwrap_or_default(),
            })
        }
    };
    let key = runtime_key(&state, &runtime)?;
    let result = embedding::embed_texts(
        &runtime,
        key.as_deref(),
        &["CanonWeave 语义检索连接测试".to_string()],
    )
    .await;
    let latency_ms = started.elapsed().as_millis() as u32;
    match result {
        Ok(vectors) => Ok(ModelTestResult {
            ok: true,
            message: format!(
                "Embedding 连接成功 · {} 维 · 模式 {}",
                vectors.first().map(Vec::len).unwrap_or(0),
                runtime.mode
            ),
            latency_ms,
            model: runtime.model,
        }),
        Err(error) => Ok(ModelTestResult {
            ok: false,
            message: error.to_string(),
            latency_ms,
            model: runtime.model,
        }),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn embedding_index(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    rebuild: bool,
) -> IpcResult<EmbeddingIndexResult> {
    let config = state.model_store.load();
    let runtime =
        embedding::EmbeddingRuntime::from_config(&config).map_err(|error| error.to_string())?;
    let key = runtime_key(&state, &runtime)?;
    let plan = state
        .with_project(|loaded| {
            embedding::prepare_index(&loaded.conn, &loaded.summary.id, &runtime, rebuild)
        })
        .map_err(|error| error.to_string())?;
    let total = plan.documents.len() as i32;
    if plan.documents.is_empty() {
        return Ok(EmbeddingIndexResult {
            model: runtime.model,
            dimensions: runtime.dimensions.unwrap_or(0),
            indexed: 0,
            skipped: plan.skipped,
            failed: 0,
            remaining: 0,
            canceled: false,
        });
    }

    let cancel = Arc::new(AtomicBool::new(false));
    state
        .cancellations
        .lock()
        .map_err(|_| "取消状态锁已损坏".to_string())?
        .insert("embedding:index".into(), cancel.clone());

    let mut indexed = 0;
    let mut canceled = false;
    let mut dimensions = runtime.dimensions.unwrap_or(0);
    for batch in plan.documents.chunks(runtime.batch_size) {
        if cancel.load(Ordering::Relaxed) {
            canceled = true;
            break;
        }
        let texts = batch
            .iter()
            .map(|document| document.content.clone())
            .collect::<Vec<_>>();
        let vectors = match embedding::embed_texts(&runtime, key.as_deref(), &texts).await {
            Ok(vectors) => vectors,
            Err(error) => {
                if let Ok(guard) = state.project.lock() {
                    if let Some(loaded) = guard.as_ref() {
                        let _ = embedding::mark_batch_error(
                            &loaded.conn,
                            &loaded.summary.id,
                            &runtime.model,
                            batch,
                        );
                    }
                }
                if let Ok(mut map) = state.cancellations.lock() {
                    map.remove("embedding:index");
                }
                return Err(error.to_string());
            }
        };
        dimensions = vectors.first().map(Vec::len).unwrap_or(dimensions as usize) as i32;
        {
            let mut guard = state
                .project
                .lock()
                .map_err(|_| "项目状态锁已损坏".to_string())?;
            let loaded = guard.as_mut().ok_or("尚未打开项目")?;
            embedding::apply_vectors(
                &mut loaded.conn,
                &loaded.summary.id,
                &runtime,
                batch,
                &vectors,
            )
            .map_err(|error| error.to_string())?;
        }
        indexed += batch.len() as i32;
        let _ = app.emit(
            "embedding://progress",
            EmbeddingProgressEvent {
                indexed,
                total,
                model: runtime.model.clone(),
                message: format!("已索引 {indexed}/{total} 条内容"),
            },
        );
    }

    let remaining = if canceled {
        total - indexed
    } else {
        state
            .with_project(|loaded| {
                Ok(
                    embedding::prepare_index(&loaded.conn, &loaded.summary.id, &runtime, false)?
                        .documents
                        .len() as i32,
                )
            })
            .map_err(|error| error.to_string())?
    };
    if !canceled {
        state
            .with_project(|loaded| {
                embedding::finalize_index(&loaded.conn, &loaded.summary.id, &runtime)
            })
            .map_err(|error| error.to_string())?;
    }
    if let Ok(mut map) = state.cancellations.lock() {
        map.remove("embedding:index");
    }
    Ok(EmbeddingIndexResult {
        model: runtime.model,
        dimensions,
        indexed,
        skipped: plan.skipped,
        failed: 0,
        remaining,
        canceled,
    })
}

#[tauri::command]
#[specta::specta]
pub fn embedding_cancel_index(state: State<'_, AppState>) -> IpcResult<()> {
    let map = state
        .cancellations
        .lock()
        .map_err(|_| "取消状态锁已损坏".to_string())?;
    if let Some(flag) = map.get("embedding:index") {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}
