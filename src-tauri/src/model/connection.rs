use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde_json::Value;

use crate::error::AppError;

use super::{
    AvailableModel, ModelConnectionInput, ModelConnectionResult, ModelConnectionTester,
    ModelProvider,
};

pub struct HttpModelConnectionTester {
    client: reqwest::Client,
}

impl HttpModelConnectionTester {
    pub fn new() -> Result<Self, AppError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|_| AppError::ModelConfiguration {
                message: "model HTTP client could not be initialized".into(),
            })?;
        Ok(Self { client })
    }
}

#[async_trait]
impl ModelConnectionTester for HttpModelConnectionTester {
    async fn test(&self, input: &ModelConnectionInput) -> Result<ModelConnectionResult, String> {
        let base_url = input.base_url.trim().trim_end_matches('/');
        let request = match input.provider {
            ModelProvider::Anthropic => {
                let mut headers = HeaderMap::new();
                headers.insert(
                    "x-api-key",
                    HeaderValue::from_str(input.api_key.trim()).map_err(|_| "invalid key")?,
                );
                headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
                self.client
                    .get(format!("{base_url}/models?limit=1000"))
                    .headers(headers)
            }
            ModelProvider::Google => self
                .client
                .get(format!("{base_url}/models?pageSize=1000"))
                .query(&[("key", input.api_key.trim())]),
            ModelProvider::Openai
            | ModelProvider::Openrouter
            | ModelProvider::Deepseek
            | ModelProvider::Custom => self
                .client
                .get(format!("{base_url}/models"))
                .header(AUTHORIZATION, format!("Bearer {}", input.api_key.trim())),
        };

        let response = request.send().await.map_err(|_| "request failed")?;
        if !response.status().is_success() {
            return Err("provider rejected connection".into());
        }
        let body: Value = response.json().await.map_err(|_| "invalid response")?;
        let models = match input.provider {
            ModelProvider::Google => body
                .get("models")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|model| {
                    let name = model.get("name")?.as_str()?.trim_start_matches("models/");
                    let supports_generation = model
                        .get("supportedGenerationMethods")
                        .and_then(Value::as_array)
                        .is_none_or(|methods| {
                            methods.iter().any(|method| method == "generateContent")
                        });
                    supports_generation.then(|| AvailableModel {
                        id: name.into(),
                        label: model
                            .get("displayName")
                            .and_then(Value::as_str)
                            .unwrap_or(name)
                            .into(),
                    })
                })
                .collect(),
            _ => body
                .get("data")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|model| {
                    let id = model.get("id")?.as_str()?;
                    Some(AvailableModel {
                        id: id.into(),
                        label: model
                            .get("display_name")
                            .or_else(|| model.get("name"))
                            .and_then(Value::as_str)
                            .unwrap_or(id)
                            .into(),
                    })
                })
                .collect(),
        };
        Ok(ModelConnectionResult { models })
    }
}
