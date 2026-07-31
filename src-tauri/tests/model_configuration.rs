use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use piwork_lib::{
    model::{
        AvailableModel, CredentialVault, ModelConfigurationRepository, ModelConnectionInput,
        ModelConnectionResult, ModelConnectionTester, ModelProvider, ModelService,
        SaveModelConfigurationInput,
    },
    storage::sqlite::Database,
};

#[derive(Default)]
struct FakeVault {
    secret: Mutex<Option<String>>,
}

impl CredentialVault for FakeVault {
    fn store_api_key(&self, api_key: &str) -> Result<(), String> {
        *self.secret.lock().unwrap() = Some(api_key.to_owned());
        Ok(())
    }

    fn delete_api_key(&self) -> Result<(), String> {
        *self.secret.lock().unwrap() = None;
        Ok(())
    }

    fn load_api_key(&self) -> Result<String, String> {
        self.secret
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "missing secret".into())
    }
}

struct FakeConnectionTester;

#[async_trait]
impl ModelConnectionTester for FakeConnectionTester {
    async fn test(&self, _input: &ModelConnectionInput) -> Result<ModelConnectionResult, String> {
        Ok(ModelConnectionResult {
            models: vec![AvailableModel {
                id: "gpt-5.2".into(),
                label: "GPT-5.2".into(),
            }],
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
            provider: ModelProvider::Openai,
            api_key: "sk-secret-that-must-not-enter-sqlite".into(),
            base_url: "https://api.openai.com/v1".into(),
            model_id: "gpt-5.2".into(),
        })
        .await
        .unwrap();

    assert_eq!(saved.provider, ModelProvider::Openai);
    assert_eq!(saved.model_id, "gpt-5.2");
    assert_eq!(
        vault.secret.lock().unwrap().as_deref(),
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
    assert!(vault.secret.lock().unwrap().is_none());
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
            provider: ModelProvider::Openai,
            api_key: "sk-live".into(),
            base_url: "https://api.openai.com/v1".into(),
            model_id: "gpt-5.2".into(),
        })
        .await
        .unwrap();
    vault.delete_api_key().unwrap();

    let status = service.status().await.unwrap();

    assert!(!status.configured);
    assert!(status.configuration.is_none());
    assert_eq!(
        serde_json::to_value(service.require_configured().await.unwrap_err()).unwrap()["code"],
        "model_configuration_required"
    );
}
