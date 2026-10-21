use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::json;

use crate::{
    domain::{ModelConfig, ModelConfigInput, ModelTestResult},
    error::{AppError, AppResult},
};

const KEYRING_SERVICE: &str = "com.canonweave.app";
const KEYRING_ACCOUNT: &str = "openai-compatible-api-key";
const CONFIG_FILE: &str = "model.json";

#[derive(Default)]
struct ApiKeyCacheState {
    loaded: bool,
    value: Option<String>,
}

pub struct ApiKeyCache {
    state: Mutex<ApiKeyCacheState>,
}

impl Default for ApiKeyCache {
    fn default() -> Self {
        Self {
            state: Mutex::new(ApiKeyCacheState::default()),
        }
    }
}

impl ApiKeyCache {
    pub fn get(&self) -> AppResult<Option<String>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| AppError::Message("API Key 缓存锁已损坏".into()))?;
        if !state.loaded {
            state.value = load_api_key()?;
            state.loaded = true;
        }
        Ok(state.value.clone())
    }

    pub fn set(&self, key: String) -> AppResult<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| AppError::Message("API Key 缓存锁已损坏".into()))?;
        state.value = Some(key);
        state.loaded = true;
        Ok(())
    }
}

pub struct ModelStore {
    path: PathBuf,
}

impl ModelStore {
    pub fn new(config_dir: &Path) -> Self {
        Self {
            path: config_dir.join(CONFIG_FILE),
        }
    }

    pub fn load(&self) -> ModelConfig {
        self.read_file().unwrap_or_default()
    }

    pub fn mark_api_key_present(&self, present: bool) -> AppResult<()> {
        let mut config = self.load();
        if config.has_api_key == present {
            return Ok(());
        }
        config.has_api_key = present;
        self.write_file(&config)
    }

    fn read_file(&self) -> Option<ModelConfig> {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|raw| serde_json::from_str::<ModelConfig>(&raw).ok())
    }

    fn write_file(&self, config: &ModelConfig) -> AppResult<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&self.path, serde_json::to_string_pretty(config)?)?;
        Ok(())
    }

    pub fn save(&self, input: &ModelConfigInput) -> AppResult<ModelConfig> {
        if input.base_url.trim().is_empty() {
            return Err(AppError::Message("Base URL 不能为空".into()));
        }
        if input.model.trim().is_empty() {
            return Err(AppError::Message("模型名不能为空".into()));
        }
        let mut has_api_key = self.load().has_api_key;
        if let Some(key) = input.api_key.as_deref() {
            let key = key.trim();
            if !key.is_empty() {
                save_api_key(key)?;
                has_api_key = true;
            }
        }
        let config = ModelConfig {
            base_url: input.base_url.trim().trim_end_matches('/').to_string(),
            model: input.model.trim().to_string(),
            temperature: input.temperature.clamp(0.0, 2.0),
            max_tokens: input.max_tokens.clamp(64, 200_000),
            embedding_model: input
                .embedding_model
                .clone()
                .filter(|v| !v.trim().is_empty()),
            has_api_key,
        };
        self.write_file(&config)?;
        Ok(config)
    }
}

pub fn save_api_key(key: &str) -> AppResult<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|e| AppError::Message(format!("无法访问系统钥匙串：{e}")))?;
    entry
        .set_password(key)
        .map_err(|e| AppError::Message(format!("写入系统钥匙串失败：{e}")))
}

fn load_api_key() -> AppResult<Option<String>> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|e| AppError::Message(format!("无法访问系统钥匙串：{e}")))?;
    match entry.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AppError::Message(format!("读取系统钥匙串失败：{e}"))),
    }
}

pub fn chat_completions_url(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if base.ends_with("/chat/completions") {
        base.to_string()
    } else {
        format!("{base}/chat/completions")
    }
}

pub async fn test_connection(config: &ModelConfig, key: Option<&str>) -> ModelTestResult {
    let started = Instant::now();
    let key = match key {
        Some(key) => key,
        None => {
            return ModelTestResult {
                ok: false,
                message: "尚未保存 API Key".into(),
                latency_ms: started.elapsed().as_millis() as u32,
                model: config.model.clone(),
            }
        }
    };
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
    {
        Ok(client) => client,
        Err(e) => {
            return ModelTestResult {
                ok: false,
                message: format!("初始化网络客户端失败：{e}"),
                latency_ms: started.elapsed().as_millis() as u32,
                model: config.model.clone(),
            }
        }
    };
    let response = client
        .post(chat_completions_url(&config.base_url))
        .bearer_auth(key)
        .json(&json!({
            "model": config.model,
            "messages": [
                {"role": "system", "content": "Reply with the single word: ok"},
                {"role": "user", "content": "ping"}
            ],
            "max_tokens": 8,
            "stream": false
        }))
        .send()
        .await;
    let latency_ms = started.elapsed().as_millis() as u32;
    match response {
        Ok(resp) if resp.status().is_success() => ModelTestResult {
            ok: true,
            message: "连接成功".into(),
            latency_ms,
            model: config.model.clone(),
        },
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            let detail: String = body.chars().take(300).collect();
            ModelTestResult {
                ok: false,
                message: format!("服务返回 {status}：{detail}"),
                latency_ms,
                model: config.model.clone(),
            }
        }
        Err(e) => ModelTestResult {
            ok: false,
            message: format!("连接失败：{e}"),
            latency_ms,
            model: config.model.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_normalization() {
        assert_eq!(
            chat_completions_url("https://api.openai.com/v1/"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            chat_completions_url("http://localhost:8080/v1/chat/completions"),
            "http://localhost:8080/v1/chat/completions"
        );
    }

    #[test]
    fn config_file_never_contains_api_key() {
        let dir = tempfile::tempdir().unwrap();
        let store = ModelStore::new(dir.path());
        let config = ModelConfig {
            base_url: "http://localhost:1234/v1".into(),
            model: "local".into(),
            temperature: 0.7,
            max_tokens: 512,
            embedding_model: None,
            has_api_key: true,
        };
        store.write_file(&config).unwrap();
        let loaded = store.load();
        assert_eq!(loaded.model, "local");
        assert!(loaded.has_api_key);
        let raw = std::fs::read_to_string(dir.path().join(CONFIG_FILE)).unwrap();
        assert!(!raw.contains("sk-"));
        assert!(!raw.contains("api_key"));
    }

    #[test]
    fn saving_model_settings_without_new_key_preserves_presence() {
        let dir = tempfile::tempdir().unwrap();
        let store = ModelStore::new(dir.path());
        let existing = ModelConfig {
            base_url: "http://localhost:1234/v1".into(),
            model: "local".into(),
            temperature: 0.7,
            max_tokens: 512,
            embedding_model: None,
            has_api_key: true,
        };
        store.write_file(&existing).unwrap();

        let saved = store
            .save(&ModelConfigInput {
                base_url: "http://localhost:5678/v1".into(),
                model: "updated".into(),
                temperature: 0.5,
                max_tokens: 1024,
                embedding_model: None,
                api_key: None,
            })
            .unwrap();

        assert!(saved.has_api_key);
        assert!(store.load().has_api_key);
        let raw = std::fs::read_to_string(dir.path().join(CONFIG_FILE)).unwrap();
        assert!(!raw.contains("api_key"));
    }
}
