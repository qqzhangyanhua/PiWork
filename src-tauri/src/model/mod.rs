use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use ts_rs::TS;
use zeroize::Zeroize;

use crate::error::AppError;

pub mod commands;
mod connection;
mod credentials;

const SETTINGS_KEY: &str = "default_model_configuration";

macro_rules! binding_path {
    () => {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings/")
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case", export_to = binding_path!())]
pub enum ModelProvider {
    Openai,
    Anthropic,
    Google,
    Openrouter,
    Deepseek,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct ModelConfigurationSummary {
    pub provider: ModelProvider,
    pub model_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct ModelConfigurationStatus {
    pub configured: bool,
    pub configuration: Option<ModelConfigurationSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct ModelConnectionInput {
    pub provider: ModelProvider,
    pub api_key: String,
    pub base_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct SaveModelConfigurationInput {
    pub provider: ModelProvider,
    pub api_key: String,
    pub base_url: String,
    pub model_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct AvailableModel {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct ModelConnectionResult {
    pub models: Vec<AvailableModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredModelConfiguration {
    provider: ModelProvider,
    base_url: String,
    model_id: String,
}

#[derive(Clone)]
pub struct ModelConfigurationRepository {
    pool: SqlitePool,
}

impl ModelConfigurationRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    async fn get(&self) -> Result<Option<StoredModelConfiguration>, AppError> {
        let value: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
            .bind(SETTINGS_KEY)
            .fetch_optional(&self.pool)
            .await?;
        value
            .map(|value| {
                serde_json::from_str(&value).map_err(|_| AppError::ModelConfiguration {
                    message: "stored model configuration is invalid".into(),
                })
            })
            .transpose()
    }

    async fn save(&self, configuration: &StoredModelConfiguration) -> Result<(), AppError> {
        let value =
            serde_json::to_string(configuration).map_err(|_| AppError::ModelConfiguration {
                message: "model configuration could not be serialized".into(),
            })?;
        sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, ?) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        )
        .bind(SETTINGS_KEY)
        .bind(value)
        .bind(Utc::now())
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

pub trait CredentialVault: Send + Sync {
    fn store_api_key(&self, api_key: &str) -> Result<(), String>;
    fn delete_api_key(&self) -> Result<(), String>;
    fn load_api_key(&self) -> Result<String, String>;
}

pub struct RuntimeModelConfiguration {
    pub provider: ModelProvider,
    pub api_key: String,
    pub base_url: String,
    pub model_id: String,
}

impl Drop for RuntimeModelConfiguration {
    fn drop(&mut self) {
        self.api_key.zeroize();
    }
}

#[async_trait]
pub trait ModelConnectionTester: Send + Sync {
    async fn test(&self, input: &ModelConnectionInput) -> Result<ModelConnectionResult, String>;
}

#[derive(Clone)]
pub struct ModelService {
    repository: ModelConfigurationRepository,
    vault: Arc<dyn CredentialVault>,
    connection_tester: Arc<dyn ModelConnectionTester>,
}

impl ModelService {
    pub fn new(
        repository: ModelConfigurationRepository,
        vault: Arc<dyn CredentialVault>,
        connection_tester: Arc<dyn ModelConnectionTester>,
    ) -> Self {
        Self {
            repository,
            vault,
            connection_tester,
        }
    }

    pub fn production(repository: ModelConfigurationRepository) -> Result<Self, AppError> {
        Ok(Self::new(
            repository,
            Arc::new(credentials::PlatformCredentialVault::new()),
            Arc::new(connection::HttpModelConnectionTester::new()?),
        ))
    }

    pub async fn status(&self) -> Result<ModelConfigurationStatus, AppError> {
        let credential_available = self
            .vault
            .load_api_key()
            .map(|mut api_key| {
                api_key.zeroize();
                true
            })
            .unwrap_or(false);
        let configuration = self
            .repository
            .get()
            .await?
            .filter(|_| credential_available)
            .map(|configuration| ModelConfigurationSummary {
                provider: configuration.provider,
                model_id: configuration.model_id,
            });
        Ok(ModelConfigurationStatus {
            configured: configuration.is_some(),
            configuration,
        })
    }

    pub async fn require_configured(&self) -> Result<(), AppError> {
        if self.status().await?.configured {
            Ok(())
        } else {
            Err(AppError::ModelConfigurationRequired)
        }
    }

    pub async fn runtime_configuration(&self) -> Result<RuntimeModelConfiguration, AppError> {
        let stored = self
            .repository
            .get()
            .await?
            .ok_or(AppError::ModelConfigurationRequired)?;
        let api_key = self
            .vault
            .load_api_key()
            .map_err(|_| AppError::Credential {
                message: "model API key is unavailable".into(),
            })?;
        Ok(RuntimeModelConfiguration {
            provider: stored.provider,
            api_key,
            base_url: stored.base_url,
            model_id: stored.model_id,
        })
    }

    pub async fn test_connection(
        &self,
        input: ModelConnectionInput,
    ) -> Result<ModelConnectionResult, AppError> {
        validate_connection_input(&input)?;
        let mut result =
            self.connection_tester
                .test(&input)
                .await
                .map_err(|_| AppError::ModelConnection {
                    message: "provider connection failed".into(),
                })?;
        result.models.sort_by(|left, right| left.id.cmp(&right.id));
        result.models.dedup_by(|left, right| left.id == right.id);
        if result.models.is_empty() {
            return Err(AppError::ModelConnection {
                message: "provider returned no usable models".into(),
            });
        }
        Ok(result)
    }

    pub async fn save(
        &self,
        input: SaveModelConfigurationInput,
    ) -> Result<ModelConfigurationSummary, AppError> {
        let model_id = input.model_id.trim();
        if model_id.is_empty() {
            return Err(AppError::invalid_input("modelId", "model must be selected"));
        }
        let connection = self
            .test_connection(ModelConnectionInput {
                provider: input.provider,
                api_key: input.api_key.clone(),
                base_url: input.base_url.clone(),
            })
            .await?;
        if !connection.models.iter().any(|model| model.id == model_id) {
            return Err(AppError::invalid_input(
                "modelId",
                "selected model is not available from the provider",
            ));
        }

        self.vault
            .store_api_key(input.api_key.trim())
            .map_err(|_| AppError::Credential {
                message: "API key could not be stored".into(),
            })?;
        let stored = StoredModelConfiguration {
            provider: input.provider,
            base_url: input.base_url.trim().trim_end_matches('/').into(),
            model_id: model_id.into(),
        };
        if let Err(error) = self.repository.save(&stored).await {
            let _ = self.vault.delete_api_key();
            return Err(error);
        }
        Ok(ModelConfigurationSummary {
            provider: stored.provider,
            model_id: stored.model_id,
        })
    }
}

fn validate_connection_input(input: &ModelConnectionInput) -> Result<(), AppError> {
    if input.api_key.trim().is_empty() {
        return Err(AppError::invalid_input("apiKey", "API key is required"));
    }
    let url = reqwest::Url::parse(input.base_url.trim())
        .map_err(|_| AppError::invalid_input("baseUrl", "Base URL is invalid"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(AppError::invalid_input(
            "baseUrl",
            "Base URL must use HTTP or HTTPS",
        ));
    }
    Ok(())
}
