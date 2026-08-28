use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use ts_rs::TS;
use uuid::Uuid;
use zeroize::Zeroize;

use crate::error::AppError;

pub mod commands;
mod connection;
mod credentials;

const LEGACY_SETTINGS_KEY: &str = "default_model_configuration";
const SETTINGS_KEY: &str = "model_configurations_v2";
const LEGACY_CONFIGURATION_ID: &str = "legacy-default";

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
    pub id: String,
    pub provider: ModelProvider,
    pub base_url: String,
    pub model_id: String,
    pub active: bool,
    pub credential_configured: bool,
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
    #[serde(default)]
    pub id: Option<String>,
    pub provider: ModelProvider,
    pub api_key: String,
    pub base_url: String,
    pub model_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct SelectModelForConfigurationInput {
    pub configuration_id: String,
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
    id: String,
    provider: ModelProvider,
    base_url: String,
    model_id: String,
    active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyStoredModelConfiguration {
    provider: ModelProvider,
    base_url: String,
    model_id: String,
}

enum LoadedConfigurations {
    Current(Vec<StoredModelConfiguration>),
    Legacy(LegacyStoredModelConfiguration),
    Empty,
}

#[derive(Clone)]
pub struct ModelConfigurationRepository {
    pool: SqlitePool,
}

impl ModelConfigurationRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    async fn load(&self) -> Result<LoadedConfigurations, AppError> {
        if let Some(value) = self.setting(SETTINGS_KEY).await? {
            let configurations =
                serde_json::from_str(&value).map_err(|_| AppError::ModelConfiguration {
                    message: "stored model configurations are invalid".into(),
                })?;
            return Ok(LoadedConfigurations::Current(configurations));
        }
        if let Some(value) = self.setting(LEGACY_SETTINGS_KEY).await? {
            let configuration =
                serde_json::from_str(&value).map_err(|_| AppError::ModelConfiguration {
                    message: "stored model configuration is invalid".into(),
                })?;
            return Ok(LoadedConfigurations::Legacy(configuration));
        }
        Ok(LoadedConfigurations::Empty)
    }

    async fn setting(&self, key: &str) -> Result<Option<String>, AppError> {
        Ok(
            sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
                .bind(key)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    async fn save_all(&self, configurations: &[StoredModelConfiguration]) -> Result<(), AppError> {
        let value =
            serde_json::to_string(configurations).map_err(|_| AppError::ModelConfiguration {
                message: "model configurations could not be serialized".into(),
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

pub use credentials::PlatformCredentialVault;

pub trait CredentialVault: Send + Sync {
    fn store_api_key(&self, configuration_id: &str, api_key: &str) -> Result<(), String>;
    fn delete_api_key(&self, configuration_id: &str) -> Result<(), String>;
    fn load_api_key(&self, configuration_id: &str) -> Result<String, String>;
    fn load_legacy_api_key(&self) -> Result<String, String>;
    fn delete_legacy_api_key(&self) -> Result<(), String>;
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

    async fn configurations(&self) -> Result<Vec<StoredModelConfiguration>, AppError> {
        match self.repository.load().await? {
            LoadedConfigurations::Current(configurations) => Ok(configurations),
            LoadedConfigurations::Empty => Ok(Vec::new()),
            LoadedConfigurations::Legacy(legacy) => {
                let configurations = vec![StoredModelConfiguration {
                    id: LEGACY_CONFIGURATION_ID.into(),
                    provider: legacy.provider,
                    base_url: legacy.base_url,
                    model_id: legacy.model_id,
                    active: true,
                }];
                if let Ok(mut api_key) = self.vault.load_legacy_api_key() {
                    self.vault
                        .store_api_key(LEGACY_CONFIGURATION_ID, &api_key)
                        .map_err(|_| AppError::Credential {
                            message: "legacy model API key could not be migrated".into(),
                        })?;
                    api_key.zeroize();
                }
                self.repository.save_all(&configurations).await?;
                let _ = self.vault.delete_legacy_api_key();
                Ok(configurations)
            }
        }
    }

    fn summary(&self, configuration: StoredModelConfiguration) -> ModelConfigurationSummary {
        let credential_configured = self
            .vault
            .load_api_key(&configuration.id)
            .map(|mut key| {
                key.zeroize();
                true
            })
            .unwrap_or(false);
        ModelConfigurationSummary {
            id: configuration.id,
            provider: configuration.provider,
            base_url: configuration.base_url,
            model_id: configuration.model_id,
            active: configuration.active,
            credential_configured,
        }
    }

    pub async fn list_configurations(&self) -> Result<Vec<ModelConfigurationSummary>, AppError> {
        Ok(self
            .configurations()
            .await?
            .into_iter()
            .map(|item| self.summary(item))
            .collect())
    }

    pub async fn status(&self) -> Result<ModelConfigurationStatus, AppError> {
        let configuration = self
            .list_configurations()
            .await?
            .into_iter()
            .find(|item| item.active && item.credential_configured);
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
            .configurations()
            .await?
            .into_iter()
            .find(|item| item.active)
            .ok_or(AppError::ModelConfigurationRequired)?;
        let api_key = self
            .vault
            .load_api_key(&stored.id)
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

    pub async fn test_saved_configuration(
        &self,
        configuration_id: &str,
    ) -> Result<ModelConnectionResult, AppError> {
        let stored = self
            .configurations()
            .await?
            .into_iter()
            .find(|item| item.id == configuration_id)
            .ok_or_else(|| {
                AppError::invalid_input("configurationId", "model configuration was not found")
            })?;
        let api_key = self
            .vault
            .load_api_key(&stored.id)
            .map_err(|_| AppError::Credential {
                message: "model API key is unavailable".into(),
            })?;
        self.test_connection(ModelConnectionInput {
            provider: stored.provider,
            api_key,
            base_url: stored.base_url,
        })
        .await
    }

    pub async fn activate_configuration(
        &self,
        configuration_id: &str,
    ) -> Result<ModelConfigurationSummary, AppError> {
        let mut configurations = self.configurations().await?;
        let target = configurations
            .iter()
            .position(|item| item.id == configuration_id)
            .ok_or_else(|| {
                AppError::invalid_input("configurationId", "model configuration was not found")
            })?;
        let mut api_key =
            self.vault
                .load_api_key(configuration_id)
                .map_err(|_| AppError::Credential {
                    message: "model API key is unavailable".into(),
                })?;
        api_key.zeroize();
        for (index, item) in configurations.iter_mut().enumerate() {
            item.active = index == target;
        }
        self.repository.save_all(&configurations).await?;
        Ok(self.summary(configurations.remove(target)))
    }

    pub async fn select_model(
        &self,
        input: SelectModelForConfigurationInput,
    ) -> Result<ModelConfigurationSummary, AppError> {
        let model_id = input.model_id.trim();
        if model_id.is_empty() {
            return Err(AppError::invalid_input("modelId", "model must be selected"));
        }

        let mut configurations = self.configurations().await?;
        let target = configurations
            .iter()
            .position(|item| item.id == input.configuration_id)
            .ok_or_else(|| {
                AppError::invalid_input("configurationId", "model configuration was not found")
            })?;
        let connection = self
            .test_saved_configuration(&input.configuration_id)
            .await?;
        if !connection.models.iter().any(|model| model.id == model_id) {
            return Err(AppError::invalid_input(
                "modelId",
                "selected model is not available from the provider",
            ));
        }

        configurations[target].model_id = model_id.into();
        self.repository.save_all(&configurations).await?;
        Ok(self.summary(configurations.remove(target)))
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

        let mut configurations = self.configurations().await?;
        let id = input
            .id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let existing = configurations.iter().position(|item| item.id == id);
        let active = existing
            .map(|index| configurations[index].active)
            .unwrap_or(configurations.is_empty());
        let previous_key = self.vault.load_api_key(&id).ok();
        self.vault
            .store_api_key(&id, input.api_key.trim())
            .map_err(|_| AppError::Credential {
                message: "API key could not be stored".into(),
            })?;

        let stored = StoredModelConfiguration {
            id: id.clone(),
            provider: input.provider,
            base_url: input.base_url.trim().trim_end_matches('/').into(),
            model_id: model_id.into(),
            active,
        };
        if let Some(index) = existing {
            configurations[index] = stored.clone();
        } else {
            configurations.push(stored.clone());
        }
        if let Err(error) = self.repository.save_all(&configurations).await {
            if let Some(mut previous_key) = previous_key {
                let _ = self.vault.store_api_key(&id, &previous_key);
                previous_key.zeroize();
            } else {
                let _ = self.vault.delete_api_key(&id);
            }
            return Err(error);
        }
        Ok(self.summary(stored))
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
