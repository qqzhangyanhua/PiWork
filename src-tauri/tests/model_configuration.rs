use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use piwork_lib::{
    model::{
        AvailableModel, CredentialVault, ModelConfigurationRepository, ModelConnectionInput,
        ModelConnectionResult, ModelConnectionTester, ModelProvider, ModelService,
        SaveModelConfigurationInput, SelectModelForConfigurationInput,
    },
    storage::sqlite::Database,
};

#[cfg(target_os = "macos")]
use piwork_lib::model::PlatformCredentialVault;

#[derive(Default)]
struct FakeVault {
    secrets: Mutex<HashMap<String, String>>,
    legacy_secret: Mutex<Option<String>>,
}

impl CredentialVault for FakeVault {
    fn store_api_key(&self, configuration_id: &str, api_key: &str) -> Result<(), String> {
        self.secrets
            .lock()
            .unwrap()
            .insert(configuration_id.into(), api_key.into());
        Ok(())
    }

    fn delete_api_key(&self, configuration_id: &str) -> Result<(), String> {
        self.secrets.lock().unwrap().remove(configuration_id);
        Ok(())
    }

    fn load_api_key(&self, configuration_id: &str) -> Result<String, String> {
        self.secrets
            .lock()
            .unwrap()
            .get(configuration_id)
            .cloned()
            .ok_or_else(|| "missing secret".into())
    }

    fn load_legacy_api_key(&self) -> Result<String, String> {
        self.legacy_secret
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "missing legacy secret".into())
    }

    fn delete_legacy_api_key(&self) -> Result<(), String> {
        *self.legacy_secret.lock().unwrap() = None;
        Ok(())
    }
}

struct FakeConnectionTester;

#[async_trait]
impl ModelConnectionTester for FakeConnectionTester {
    async fn test(&self, _input: &ModelConnectionInput) -> Result<ModelConnectionResult, String> {
        Ok(ModelConnectionResult {
            models: vec![
                AvailableModel {
                    id: "gpt-5.2".into(),
                    label: "GPT-5.2".into(),
                },
                AvailableModel {
                    id: "gpt-5.3".into(),
                    label: "GPT-5.3".into(),
                },
            ],
        })
    }
}

#[tokio::test]
async fn configuration_is_locked_until_a_verified_model_and_secret_are_saved() {
    let database = Database::open_in_memory().await.unwrap();
    let repository = ModelConfigurationRepository::new(database.pool().clone());
    let vault = Arc::new(FakeVault::default());
    let service = ModelService::new(repository, vault.clone(), Arc::new(FakeConnectionTester));

    let initial = service.status().await.unwrap();
    assert!(!initial.configured);
    assert!(initial.configuration.is_none());

    let saved = service
        .save(SaveModelConfigurationInput {
            id: None,
            provider: ModelProvider::Openai,
            api_key: "sk-secret-that-must-not-enter-sqlite".into(),
            base_url: "https://api.openai.com/v1".into(),
            model_id: "gpt-5.2".into(),
        })
        .await
        .unwrap();

    assert_eq!(saved.provider, ModelProvider::Openai);
    assert_eq!(saved.model_id, "gpt-5.2");
    assert!(saved.active);
    assert_eq!(
        vault
            .secrets
            .lock()
            .unwrap()
            .get(&saved.id)
            .map(String::as_str),
        Some("sk-secret-that-must-not-enter-sqlite")
    );
    let persisted = service.status().await.unwrap();
    assert!(persisted.configured);
    assert_eq!(persisted.configuration, Some(saved));

    let rows: Vec<String> = sqlx::query_scalar("SELECT value FROM settings")
        .fetch_all(database.pool())
        .await
        .unwrap();
    assert!(rows.iter().all(|value| !value.contains("sk-secret")));
}

#[tokio::test]
async fn an_unavailable_model_is_rejected_before_credentials_are_persisted() {
    let database = Database::open_in_memory().await.unwrap();
    let repository = ModelConfigurationRepository::new(database.pool().clone());
    let vault = Arc::new(FakeVault::default());
    let service = ModelService::new(repository, vault.clone(), Arc::new(FakeConnectionTester));

    let error = service
        .save(SaveModelConfigurationInput {
            id: None,
            provider: ModelProvider::Openai,
            api_key: "sk-secret".into(),
            base_url: "https://api.openai.com/v1".into(),
            model_id: "missing-model".into(),
        })
        .await
        .unwrap_err();

    assert_eq!(
        serde_json::to_value(error).unwrap()["code"],
        "invalid_input"
    );
    assert!(vault.secrets.lock().unwrap().is_empty());
    assert!(!service.status().await.unwrap().configured);
}

#[tokio::test]
async fn missing_secure_credential_relocks_the_application() {
    let database = Database::open_in_memory().await.unwrap();
    let vault = Arc::new(FakeVault::default());
    let service = ModelService::new(
        ModelConfigurationRepository::new(database.pool().clone()),
        vault.clone(),
        Arc::new(FakeConnectionTester),
    );
    service
        .save(SaveModelConfigurationInput {
            id: None,
            provider: ModelProvider::Openai,
            api_key: "sk-live".into(),
            base_url: "https://api.openai.com/v1".into(),
            model_id: "gpt-5.2".into(),
        })
        .await
        .unwrap();
    let active = service.status().await.unwrap().configuration.unwrap();
    vault.delete_api_key(&active.id).unwrap();

    let status = service.status().await.unwrap();

    assert!(!status.configured);
    assert!(status.configuration.is_none());
    assert_eq!(
        serde_json::to_value(service.require_configured().await.unwrap_err()).unwrap()["code"],
        "model_configuration_required"
    );
}

#[tokio::test]
async fn multiple_configurations_can_be_listed_and_only_one_is_active() {
    let database = Database::open_in_memory().await.unwrap();
    let service = ModelService::new(
        ModelConfigurationRepository::new(database.pool().clone()),
        Arc::new(FakeVault::default()),
        Arc::new(FakeConnectionTester),
    );

    let first = service
        .save(SaveModelConfigurationInput {
            id: None,
            provider: ModelProvider::Openai,
            api_key: "sk-first".into(),
            base_url: "https://api.openai.com/v1".into(),
            model_id: "gpt-5.2".into(),
        })
        .await
        .unwrap();
    let second = service
        .save(SaveModelConfigurationInput {
            id: None,
            provider: ModelProvider::Custom,
            api_key: "sk-second".into(),
            base_url: "https://models.example.com/v1".into(),
            model_id: "gpt-5.2".into(),
        })
        .await
        .unwrap();

    let configurations = service.list_configurations().await.unwrap();
    assert_eq!(configurations.len(), 2);
    assert!(configurations.iter().all(|item| item.credential_configured));
    assert!(
        configurations
            .iter()
            .find(|item| item.id == first.id)
            .unwrap()
            .active
    );
    assert!(
        !configurations
            .iter()
            .find(|item| item.id == second.id)
            .unwrap()
            .active
    );
    assert_eq!(configurations[0].base_url, "https://api.openai.com/v1");
}

#[tokio::test]
async fn activating_a_saved_configuration_changes_the_runtime_model() {
    let database = Database::open_in_memory().await.unwrap();
    let service = ModelService::new(
        ModelConfigurationRepository::new(database.pool().clone()),
        Arc::new(FakeVault::default()),
        Arc::new(FakeConnectionTester),
    );
    service
        .save(SaveModelConfigurationInput {
            id: None,
            provider: ModelProvider::Openai,
            api_key: "sk-first".into(),
            base_url: "https://api.openai.com/v1".into(),
            model_id: "gpt-5.2".into(),
        })
        .await
        .unwrap();
    let second = service
        .save(SaveModelConfigurationInput {
            id: None,
            provider: ModelProvider::Custom,
            api_key: "sk-second".into(),
            base_url: "https://models.example.com/v1".into(),
            model_id: "gpt-5.2".into(),
        })
        .await
        .unwrap();

    let activated = service.activate_configuration(&second.id).await.unwrap();
    let runtime = service.runtime_configuration().await.unwrap();

    assert!(activated.active);
    assert_eq!(runtime.provider, ModelProvider::Custom);
    assert_eq!(runtime.api_key, "sk-second");
}

#[tokio::test]
async fn a_saved_configuration_can_be_tested_without_returning_its_secret() {
    let database = Database::open_in_memory().await.unwrap();
    let service = ModelService::new(
        ModelConfigurationRepository::new(database.pool().clone()),
        Arc::new(FakeVault::default()),
        Arc::new(FakeConnectionTester),
    );
    let saved = service
        .save(SaveModelConfigurationInput {
            id: None,
            provider: ModelProvider::Openai,
            api_key: "sk-secret".into(),
            base_url: "https://api.openai.com/v1".into(),
            model_id: "gpt-5.2".into(),
        })
        .await
        .unwrap();

    let result = service.test_saved_configuration(&saved.id).await.unwrap();

    assert_eq!(result.models[0].id, "gpt-5.2");
    assert!(!serde_json::to_string(&saved).unwrap().contains("sk-secret"));
}

#[tokio::test]
async fn a_saved_configuration_can_select_an_available_runtime_model() {
    let database = Database::open_in_memory().await.unwrap();
    let service = ModelService::new(
        ModelConfigurationRepository::new(database.pool().clone()),
        Arc::new(FakeVault::default()),
        Arc::new(FakeConnectionTester),
    );
    let saved = service
        .save(SaveModelConfigurationInput {
            id: None,
            provider: ModelProvider::Openai,
            api_key: "sk-secret".into(),
            base_url: "https://api.openai.com/v1".into(),
            model_id: "gpt-5.2".into(),
        })
        .await
        .unwrap();

    let selected = service
        .select_model(SelectModelForConfigurationInput {
            configuration_id: saved.id.clone(),
            model_id: "gpt-5.3".into(),
        })
        .await
        .unwrap();

    assert_eq!(selected.model_id, "gpt-5.3");
    assert!(selected.active);
    assert_eq!(
        service.runtime_configuration().await.unwrap().model_id,
        "gpt-5.3"
    );
}

#[tokio::test]
async fn selecting_an_unavailable_or_unknown_saved_model_preserves_the_runtime_model() {
    let database = Database::open_in_memory().await.unwrap();
    let service = ModelService::new(
        ModelConfigurationRepository::new(database.pool().clone()),
        Arc::new(FakeVault::default()),
        Arc::new(FakeConnectionTester),
    );
    let saved = service
        .save(SaveModelConfigurationInput {
            id: None,
            provider: ModelProvider::Openai,
            api_key: "sk-secret".into(),
            base_url: "https://api.openai.com/v1".into(),
            model_id: "gpt-5.2".into(),
        })
        .await
        .unwrap();

    let unavailable = service
        .select_model(SelectModelForConfigurationInput {
            configuration_id: saved.id,
            model_id: "missing-model".into(),
        })
        .await
        .unwrap_err();
    let unknown = service
        .select_model(SelectModelForConfigurationInput {
            configuration_id: "missing".into(),
            model_id: "gpt-5.3".into(),
        })
        .await
        .unwrap_err();

    assert_eq!(
        serde_json::to_value(unavailable).unwrap()["code"],
        "invalid_input"
    );
    assert_eq!(
        serde_json::to_value(unknown).unwrap()["code"],
        "invalid_input"
    );
    assert_eq!(
        service.runtime_configuration().await.unwrap().model_id,
        "gpt-5.2"
    );
}

#[tokio::test]
async fn legacy_single_configuration_and_credential_are_migrated() {
    let database = Database::open_in_memory().await.unwrap();
    sqlx::query("INSERT INTO settings (key, value, updated_at) VALUES (?, ?, ?)")
        .bind("default_model_configuration")
        .bind(r#"{"provider":"openai","baseUrl":"https://api.openai.com/v1","modelId":"gpt-5.2"}"#)
        .bind(chrono::Utc::now())
        .execute(database.pool())
        .await
        .unwrap();
    let vault = Arc::new(FakeVault::default());
    *vault.legacy_secret.lock().unwrap() = Some("sk-legacy".into());
    let service = ModelService::new(
        ModelConfigurationRepository::new(database.pool().clone()),
        vault.clone(),
        Arc::new(FakeConnectionTester),
    );

    let configurations = service.list_configurations().await.unwrap();
    let runtime = service.runtime_configuration().await.unwrap();

    assert_eq!(configurations.len(), 1);
    assert!(configurations[0].active);
    assert_eq!(runtime.api_key, "sk-legacy");
    assert!(vault.legacy_secret.lock().unwrap().is_none());
    assert_eq!(
        vault
            .secrets
            .lock()
            .unwrap()
            .get(&configurations[0].id)
            .unwrap(),
        "sk-legacy"
    );
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn platform_vault_keeps_the_api_key_out_of_sqlite() {
    let database = Database::open_in_memory().await.unwrap();
    let vault = Arc::new(PlatformCredentialVault::new());
    let service = ModelService::new(
        ModelConfigurationRepository::new(database.pool().clone()),
        vault.clone(),
        Arc::new(FakeConnectionTester),
    );

    let saved = service
        .save(SaveModelConfigurationInput {
            id: None,
            provider: ModelProvider::Openai,
            api_key: "sk-secret-that-must-not-enter-sqlite".into(),
            base_url: "https://api.openai.com/v1".into(),
            model_id: "gpt-5.2".into(),
        })
        .await
        .unwrap();
    struct Cleanup<'a> {
        vault: &'a PlatformCredentialVault,
        id: String,
    }
    impl Drop for Cleanup<'_> {
        fn drop(&mut self) {
            let _ = self.vault.delete_api_key(&self.id);
        }
    }
    let _cleanup = Cleanup {
        vault: vault.as_ref(),
        id: saved.id.clone(),
    };

    assert_eq!(
        service.runtime_configuration().await.unwrap().api_key,
        "sk-secret-that-must-not-enter-sqlite"
    );

    service
        .save(SaveModelConfigurationInput {
            id: Some(saved.id.clone()),
            provider: ModelProvider::Openai,
            api_key: "sk-updated-keychain-secret".into(),
            base_url: "https://api.openai.com/v1".into(),
            model_id: "gpt-5.2".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        service.runtime_configuration().await.unwrap().api_key,
        "sk-updated-keychain-secret"
    );

    let rows: Vec<String> = sqlx::query_scalar("SELECT value FROM settings")
        .fetch_all(database.pool())
        .await
        .unwrap();
    assert!(
        rows.iter()
            .all(|value| !value.contains("sk-secret") && !value.contains("sk-updated"))
    );

    vault.delete_api_key(&saved.id).unwrap();
    assert!(vault.load_api_key(&saved.id).is_err());
}
