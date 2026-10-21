use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use crate::{
    domain::ModelConfig,
    error::{AppError, AppResult},
    model::{ApiKeyCache, ModelStore},
    project::LoadedProject,
};

pub struct AppState {
    pub project: Mutex<Option<LoadedProject>>,
    pub model_store: ModelStore,
    pub api_keys: ApiKeyCache,
    pub model: Mutex<ModelConfig>,
    pub cancellations: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl AppState {
    pub fn new(config_dir: PathBuf) -> Self {
        let model_store = ModelStore::new(&config_dir);
        let model = model_store.load();
        Self {
            project: Mutex::new(None),
            model_store,
            api_keys: ApiKeyCache::default(),
            model: Mutex::new(model),
            cancellations: Mutex::new(HashMap::new()),
        }
    }

    pub fn api_key(&self) -> AppResult<Option<String>> {
        let key = self.api_keys.get()?;
        let present = key.is_some();
        if let Ok(mut model) = self.model.lock() {
            model.has_api_key = present;
        }
        // Keep the non-secret presence flag current for the next app launch.
        let _ = self.model_store.mark_api_key_present(present);
        Ok(key)
    }

    pub fn with_project<T>(&self, f: impl FnOnce(&LoadedProject) -> AppResult<T>) -> AppResult<T> {
        let guard = self
            .project
            .lock()
            .map_err(|_| AppError::Message("项目状态锁已损坏".into()))?;
        match guard.as_ref() {
            Some(project) => f(project),
            None => Err(AppError::Message("尚未打开项目".into())),
        }
    }
}
