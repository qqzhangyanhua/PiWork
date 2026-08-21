use crate::secret;

use super::CredentialVault;

pub struct PlatformCredentialVault;

impl PlatformCredentialVault {
    pub fn new() -> Self {
        Self
    }
}

impl CredentialVault for PlatformCredentialVault {
    fn store_api_key(&self, configuration_id: &str, api_key: &str) -> Result<(), String> {
        secret::store(&credential_target(configuration_id), "PiWork", api_key)
    }

    fn delete_api_key(&self, configuration_id: &str) -> Result<(), String> {
        secret::delete(&credential_target(configuration_id))
    }

    fn load_api_key(&self, configuration_id: &str) -> Result<String, String> {
        secret::load(&credential_target(configuration_id))
    }

    fn load_legacy_api_key(&self) -> Result<String, String> {
        secret::load("PiWork/default-model-api-key")
    }

    fn delete_legacy_api_key(&self) -> Result<(), String> {
        secret::delete("PiWork/default-model-api-key")
    }
}

fn credential_target(configuration_id: &str) -> String {
    format!("PiWork/model-api-key/{configuration_id}")
}
