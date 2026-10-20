use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use crate::{
    domain::ModelConfig,
    error::{AppError, AppResult},
    model::ModelStore,
    project::LoadedProject,
};

pub struct AppState {
    pub project: Mutex<Option<LoadedProject>>,
    pub model_store: ModelStore,
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
            model: Mutex::new(model),
            cancellations: Mutex::new(HashMap::new()),
        }
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
