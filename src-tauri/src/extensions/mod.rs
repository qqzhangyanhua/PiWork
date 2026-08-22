use std::{collections::BTreeSet, path::PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sqlx::{FromRow, SqlitePool};

use crate::{error::AppError, secret};

pub mod commands;

pub const WEB_ACCESS_PACKAGE_ID: &str = "pi-web-access";
pub const WEB_ACCESS_VERSION: &str = "0.24.0";

const SEARCH_PROVIDER_IDS: &[&str] = &[
    "exa",
    "brave",
    "tavily",
    "bocha",
    "jina",
    "firecrawl",
    "openai",
    "searxng",
];
const WEB_ACCESS_TOOL_IDS: &[&str] = &[
    "web_search",
    "fetch_content",
    "get_search_content",
    "source_check",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionSummary {
    pub package_id: String,
    pub display_name: String,
    pub description: String,
    pub publisher: String,
    pub trust_tier: String,
    pub source_kind: String,
    pub installed_version: Option<String>,
    pub latest_version: String,
    pub lifecycle_status: String,
    pub builtin: bool,
    pub manifest: Value,
    pub permissions: Value,
    pub enabled_agent_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunityExtensionSummary {
    pub package_id: String,
    pub version: String,
    pub description: String,
    pub publisher: String,
    pub npm_url: Option<String>,
    pub score: f64,
    pub executable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSearchProviderSummary {
    pub provider_id: String,
    pub enabled: bool,
    pub endpoint: Option<String>,
    pub credential_configured: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebAccessSettingsSummary {
    pub enabled: bool,
    pub url_fetch_enabled: bool,
    pub default_provider: Option<String>,
    pub fallback_provider: Option<String>,
    pub providers: Vec<WebSearchProviderSummary>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveWebSearchProviderInput {
    pub provider_id: String,
    pub enabled: bool,
    pub endpoint: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub clear_credential: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveWebAccessSettingsInput {
    pub enabled: bool,
    pub url_fetch_enabled: bool,
    pub default_provider: Option<String>,
    pub fallback_provider: Option<String>,
    pub providers: Vec<SaveWebSearchProviderInput>,
}

#[derive(Debug, Clone, Default)]
pub struct ExtensionRuntimeSnapshot {
    pub extension_paths: Vec<PathBuf>,
    pub tool_ids: Vec<String>,
    pub runtime_files: Vec<(PathBuf, String)>,
    pub sensitive_values: Vec<String>,
}

#[derive(Clone)]
pub struct ExtensionService {
    pool: SqlitePool,
    bundled_web_access_entry: Option<PathBuf>,
    http: reqwest::Client,
}

#[derive(FromRow)]
struct ExtensionRow {
    package_id: String,
    display_name: String,
    description: String,
    publisher: String,
    trust_tier: String,
    source_kind: String,
    installed_version: Option<String>,
    latest_version: String,
    lifecycle_status: String,
    builtin: bool,
    manifest_json: String,
    permissions_json: String,
}

#[derive(FromRow)]
struct ProviderRow {
    provider_id: String,
    enabled: bool,
    endpoint: Option<String>,
}

impl ExtensionService {
    pub fn new(
        pool: SqlitePool,
        bundled_web_access_entry: Option<PathBuf>,
    ) -> Result<Self, AppError> {
        let http = reqwest::Client::builder()
            .user_agent(format!("PiWork/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(12))
            .build()
            .map_err(|_| AppError::engine("extension HTTP client could not be created"))?;
        Ok(Self {
            pool,
            bundled_web_access_entry,
            http,
        })
    }

    pub async fn list_extensions(&self) -> Result<Vec<ExtensionSummary>, AppError> {
        let rows = sqlx::query_as::<_, ExtensionRow>(
            "SELECT package_id, display_name, description, publisher, trust_tier, source_kind, \
             installed_version, latest_version, lifecycle_status, builtin, manifest_json, permissions_json \
             FROM extension_packages ORDER BY builtin DESC, display_name",
        )
        .fetch_all(&self.pool)
        .await?;
        let active_agent_ids = sqlx::query_scalar::<_, String>(
            "SELECT id FROM agent_instances WHERE status = 'active' ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| ExtensionSummary {
                enabled_agent_ids: if matches!(row.lifecycle_status.as_str(), "installed") {
                    active_agent_ids.clone()
                } else {
                    Vec::new()
                },
                package_id: row.package_id,
                display_name: row.display_name,
                description: row.description,
                publisher: row.publisher,
                trust_tier: row.trust_tier,
                source_kind: row.source_kind,
                installed_version: row.installed_version,
                latest_version: row.latest_version,
                lifecycle_status: row.lifecycle_status,
                builtin: row.builtin,
                manifest: parse_json(&row.manifest_json),
                permissions: parse_json(&row.permissions_json),
            })
            .collect())
    }

    pub async fn set_agent_enabled(
        &self,
        package_id: &str,
        agent_instance_id: &str,
        enabled: bool,
        tool_allowlist: Vec<String>,
    ) -> Result<ExtensionSummary, AppError> {
        let package = sqlx::query_as::<_, (String, String)>(
            "SELECT trust_tier, lifecycle_status FROM extension_packages WHERE package_id = ?",
        )
        .bind(package_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::invalid_input("packageId", "extension was not found"))?;
        if package.0 == "community" || !matches!(package.1.as_str(), "installed" | "disabled") {
            return Err(AppError::invalid_input(
                "packageId",
                "extension is not eligible for execution",
            ));
        }
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agent_instances WHERE id = ?)")
                .bind(agent_instance_id)
                .fetch_one(&self.pool)
                .await?;
        if !exists {
            return Err(AppError::invalid_input(
                "agentInstanceId",
                "agent instance was not found",
            ));
        }
        let allowlist = normalize_tool_allowlist(tool_allowlist)?;
        sqlx::query(
            "INSERT INTO extension_agent_grants \
             (package_id, agent_instance_id, enabled, tool_allowlist_json, updated_at) \
             VALUES (?, ?, ?, ?, ?) \
             ON CONFLICT(package_id, agent_instance_id) DO UPDATE SET \
             enabled = excluded.enabled, tool_allowlist_json = excluded.tool_allowlist_json, \
             updated_at = excluded.updated_at",
        )
        .bind(package_id)
        .bind(agent_instance_id)
        .bind(enabled)
        .bind(serde_json::to_string(&allowlist).unwrap_or_else(|_| "[]".into()))
        .bind(Utc::now())
        .execute(&self.pool)
        .await?;

        self.list_extensions()
            .await?
            .into_iter()
            .find(|item| item.package_id == package_id)
            .ok_or_else(|| AppError::invalid_input("packageId", "extension was not found"))
    }

    pub async fn search_community(
        &self,
        query: &str,
    ) -> Result<Vec<CommunityExtensionSummary>, AppError> {
        let mut url = reqwest::Url::parse("https://registry.npmjs.org/-/v1/search")
            .map_err(|_| AppError::engine("npm registry URL is invalid"))?;
        let search = if query.trim().is_empty() {
            "keywords:pi-package".to_owned()
        } else {
            format!("keywords:pi-package {}", query.trim())
        };
        url.query_pairs_mut()
            .append_pair("text", &search)
            .append_pair("size", "30")
            .append_pair("from", "0");
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|_| AppError::engine("npm registry search failed"))?;
        if !response.status().is_success() {
            return Err(AppError::engine("npm registry search failed"));
        }
        let payload = response
            .json::<NpmSearchResponse>()
            .await
            .map_err(|_| AppError::engine("npm registry returned invalid search data"))?;
        Ok(payload
            .objects
            .into_iter()
            .filter(|item| {
                item.package
                    .keywords
                    .iter()
                    .any(|keyword| keyword.eq_ignore_ascii_case("pi-package"))
            })
            .map(|item| CommunityExtensionSummary {
                package_id: item.package.name,
                version: item.package.version,
                description: item.package.description.unwrap_or_default(),
                publisher: item
                    .package
                    .publisher
                    .map(|publisher| publisher.username)
                    .unwrap_or_default(),
                npm_url: item.package.links.and_then(|links| links.npm),
                score: item.score.final_score,
                executable: false,
            })
            .collect())
    }

    pub async fn web_access_settings(&self) -> Result<WebAccessSettingsSummary, AppError> {
        let (enabled, url_fetch_enabled, default_provider, fallback_provider) =
            sqlx::query_as::<_, (bool, bool, Option<String>, Option<String>)>(
                "SELECT enabled, url_fetch_enabled, default_provider, fallback_provider \
                 FROM web_access_settings WHERE singleton_id = 1",
            )
            .fetch_one(&self.pool)
            .await?;
        let providers = sqlx::query_as::<_, ProviderRow>(
            "SELECT provider_id, enabled, endpoint FROM web_search_providers ORDER BY rowid",
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|provider| WebSearchProviderSummary {
            credential_configured: provider.provider_id == "searxng"
                || secret::load(&web_credential_target(&provider.provider_id)).is_ok(),
            provider_id: provider.provider_id,
            enabled: provider.enabled,
            endpoint: provider.endpoint,
        })
        .collect();
        Ok(WebAccessSettingsSummary {
            enabled,
            url_fetch_enabled,
            default_provider,
            fallback_provider,
            providers,
        })
    }

    pub async fn save_web_access_settings(
        &self,
        input: SaveWebAccessSettingsInput,
    ) -> Result<WebAccessSettingsSummary, AppError> {
        validate_web_access_input(&input)?;
        let now = Utc::now();
        let mut transaction = self.pool.begin().await?;
        for provider in &input.providers {
            if provider.clear_credential {
                if secret::load(&web_credential_target(&provider.provider_id)).is_ok() {
                    secret::delete(&web_credential_target(&provider.provider_id)).map_err(
                        |_| AppError::Credential {
                            message: "web search credential could not be removed".into(),
                        },
                    )?;
                }
            }
            if let Some(api_key) = provider
                .api_key
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                secret::store(
                    &web_credential_target(&provider.provider_id),
                    "PiWork",
                    api_key,
                )
                .map_err(|_| AppError::Credential {
                    message: "web search credential could not be stored".into(),
                })?;
            }
            if provider.enabled
                && provider.provider_id != "searxng"
                && secret::load(&web_credential_target(&provider.provider_id)).is_err()
            {
                return Err(AppError::invalid_input(
                    "apiKey",
                    format!("{} requires an API key", provider.provider_id),
                ));
            }
            sqlx::query(
                "UPDATE web_search_providers SET enabled = ?, endpoint = ?, updated_at = ? \
                 WHERE provider_id = ?",
            )
            .bind(provider.enabled)
            .bind(normalize_endpoint(provider.endpoint.as_deref())?)
            .bind(now)
            .bind(&provider.provider_id)
            .execute(&mut *transaction)
            .await?;
        }
        sqlx::query(
            "UPDATE web_access_settings SET enabled = ?, url_fetch_enabled = ?, \
             default_provider = ?, fallback_provider = ?, updated_at = ? WHERE singleton_id = 1",
        )
        .bind(input.enabled)
        .bind(input.url_fetch_enabled)
        .bind(input.default_provider.as_deref())
        .bind(input.fallback_provider.as_deref())
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        self.web_access_settings().await
    }

    pub async fn runtime_snapshot(
        &self,
        _agent_instance_id: &str,
        _work_id: &str,
    ) -> Result<ExtensionRuntimeSnapshot, AppError> {
        let lifecycle_status = sqlx::query_scalar::<_, String>(
            "SELECT lifecycle_status FROM extension_packages WHERE package_id = ?",
        )
        .bind(WEB_ACCESS_PACKAGE_ID)
        .fetch_optional(&self.pool)
        .await?;
        if lifecycle_status.as_deref() != Some("installed") {
            return Ok(ExtensionRuntimeSnapshot::default());
        }
        let settings = self.web_access_settings().await?;
        if !settings.enabled {
            return Ok(ExtensionRuntimeSnapshot::default());
        }
        let entry = self
            .bundled_web_access_entry
            .as_ref()
            .filter(|path| path.is_file())
            .cloned()
            .ok_or_else(|| AppError::engine("bundled Pi Web Access extension is unavailable"))?;

        let search_enabled = settings.providers.iter().any(|provider| provider.enabled);
        let fetch_enabled = settings.url_fetch_enabled;
        if !search_enabled && !fetch_enabled {
            return Ok(ExtensionRuntimeSnapshot::default());
        }

        let mut config = Map::new();
        config.insert("workflow".into(), json!("none"));
        config.insert("autoOpenBrowser".into(), json!(false));
        config.insert("allowBrowserCookies".into(), json!(false));
        config.insert("webSearch".into(), json!({ "enabled": search_enabled }));
        config.insert(
            "tools".into(),
            json!({
                "webSearch": { "enabled": search_enabled },
                "sourceCheck": { "enabled": search_enabled },
                "fetchContent": { "enabled": fetch_enabled },
                "getSearchContent": { "enabled": fetch_enabled }
            }),
        );
        let mut sensitive_values = Vec::new();
        let enabled_providers = settings
            .providers
            .iter()
            .filter(|provider| provider.enabled)
            .collect::<Vec<_>>();
        for provider in &enabled_providers {
            if provider.provider_id == "searxng" {
                if let Some(endpoint) = &provider.endpoint {
                    config.insert("searxngBaseUrl".into(), json!(endpoint));
                }
                continue;
            }
            let key =
                secret::load(&web_credential_target(&provider.provider_id)).map_err(|_| {
                    AppError::Credential {
                        message: "enabled web search credential is unavailable".into(),
                    }
                })?;
            config.insert(
                provider_config_key(&provider.provider_id).into(),
                json!(key),
            );
            sensitive_values.push(key);
            if provider.provider_id == "firecrawl"
                && let Some(endpoint) = &provider.endpoint
            {
                config.insert("firecrawlBaseUrl".into(), json!(endpoint));
            }
            if provider.provider_id == "openai"
                && let Some(endpoint) = &provider.endpoint
            {
                config.insert("openaiResponsesUrl".into(), json!(endpoint));
            }
        }
        if search_enabled {
            let route = configured_route(&settings, &enabled_providers);
            config.insert(
                "searchRouting".into(),
                json!({
                    "providers": route,
                    "fallbackOn": ["transient", "quota", "network", "invalid-response"]
                }),
            );
        }
        let config = serde_json::to_string_pretty(&config)
            .map_err(|_| AppError::engine("web access runtime configuration is invalid"))?;
        let mut tool_ids = Vec::new();
        if search_enabled {
            tool_ids.extend(["web_search".to_owned(), "source_check".to_owned()]);
        }
        if fetch_enabled {
            tool_ids.extend(["fetch_content".to_owned(), "get_search_content".to_owned()]);
        }
        Ok(ExtensionRuntimeSnapshot {
            extension_paths: vec![entry],
            tool_ids,
            runtime_files: vec![(PathBuf::from("web-search.json"), config)],
            sensitive_values,
        })
    }
}

fn validate_web_access_input(input: &SaveWebAccessSettingsInput) -> Result<(), AppError> {
    let ids = input
        .providers
        .iter()
        .map(|provider| provider.provider_id.as_str())
        .collect::<BTreeSet<_>>();
    if ids.len() != SEARCH_PROVIDER_IDS.len()
        || SEARCH_PROVIDER_IDS
            .iter()
            .any(|provider| !ids.contains(provider))
    {
        return Err(AppError::invalid_input(
            "providers",
            "web search provider set is invalid",
        ));
    }
    let enabled = input
        .providers
        .iter()
        .filter(|provider| provider.enabled)
        .map(|provider| provider.provider_id.as_str())
        .collect::<BTreeSet<_>>();
    if enabled.is_empty() {
        if input.default_provider.is_some() || input.fallback_provider.is_some() {
            return Err(AppError::invalid_input(
                "defaultProvider",
                "routing providers require an enabled search provider",
            ));
        }
    } else {
        let default = input.default_provider.as_deref().ok_or_else(|| {
            AppError::invalid_input("defaultProvider", "select a default search provider")
        })?;
        if !enabled.contains(default) {
            return Err(AppError::invalid_input(
                "defaultProvider",
                "default provider must be enabled",
            ));
        }
        if let Some(fallback) = input.fallback_provider.as_deref() {
            if !enabled.contains(fallback) || fallback == default {
                return Err(AppError::invalid_input(
                    "fallbackProvider",
                    "fallback provider must be a different enabled provider",
                ));
            }
        }
    }
    for provider in &input.providers {
        if provider.provider_id == "searxng" && provider.enabled {
            let endpoint = normalize_endpoint(provider.endpoint.as_deref())?;
            if endpoint.is_none() {
                return Err(AppError::invalid_input(
                    "endpoint",
                    "SearXNG requires an endpoint",
                ));
            }
        }
    }
    Ok(())
}

fn normalize_endpoint(value: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let url = reqwest::Url::parse(value)
        .map_err(|_| AppError::invalid_input("endpoint", "provider endpoint is invalid"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(AppError::invalid_input(
            "endpoint",
            "provider endpoint must use HTTP or HTTPS",
        ));
    }
    Ok(Some(value.trim_end_matches('/').to_owned()))
}

fn normalize_tool_allowlist(values: Vec<String>) -> Result<Vec<String>, AppError> {
    let mut normalized = values
        .into_iter()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>();
    if normalized
        .iter()
        .any(|tool| !WEB_ACCESS_TOOL_IDS.contains(&tool.as_str()))
    {
        return Err(AppError::invalid_input(
            "toolAllowlist",
            "extension tool allowlist contains an unknown tool",
        ));
    }
    Ok(std::mem::take(&mut normalized).into_iter().collect())
}

fn configured_route(
    settings: &WebAccessSettingsSummary,
    providers: &[&WebSearchProviderSummary],
) -> Vec<String> {
    let mut route = Vec::new();
    if let Some(default) = &settings.default_provider {
        route.push(default.clone());
    }
    if let Some(fallback) = &settings.fallback_provider {
        route.push(fallback.clone());
    }
    if route.is_empty()
        && let Some(first) = providers.first()
    {
        route.push(first.provider_id.clone());
    }
    route
}

fn provider_config_key(provider: &str) -> &'static str {
    match provider {
        "exa" => "exaApiKey",
        "brave" => "braveApiKey",
        "tavily" => "tavilyApiKey",
        "bocha" => "bochaApiKey",
        "jina" => "jinaApiKey",
        "firecrawl" => "firecrawlApiKey",
        "openai" => "openaiApiKey",
        _ => "unusedApiKey",
    }
}

fn web_credential_target(provider: &str) -> String {
    format!("PiWork/web-search/{provider}")
}

fn parse_json(value: &str) -> Value {
    serde_json::from_str(value).unwrap_or_else(|_| json!({}))
}

#[derive(Deserialize)]
struct NpmSearchResponse {
    objects: Vec<NpmSearchObject>,
}

#[derive(Deserialize)]
struct NpmSearchObject {
    package: NpmSearchPackage,
    score: NpmSearchScore,
}

#[derive(Deserialize)]
struct NpmSearchPackage {
    name: String,
    version: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    publisher: Option<NpmPublisher>,
    #[serde(default)]
    links: Option<NpmLinks>,
}

#[derive(Deserialize)]
struct NpmPublisher {
    username: String,
}

#[derive(Deserialize)]
struct NpmLinks {
    #[serde(default)]
    npm: Option<String>,
}

#[derive(Deserialize)]
struct NpmSearchScore {
    #[serde(rename = "final")]
    final_score: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::sqlite::Database;

    #[test]
    fn tool_allowlists_are_normalized_and_reject_unknown_tools() {
        assert_eq!(
            normalize_tool_allowlist(vec![" web_search ".into(), "web_search".into()]).unwrap(),
            vec!["web_search"]
        );
        assert!(normalize_tool_allowlist(vec!["bash".into()]).is_err());
    }

    #[tokio::test]
    async fn enabled_builtin_extension_is_available_without_agent_or_work_grants() {
        let database = Database::open_in_memory().await.unwrap();
        sqlx::query(
            "UPDATE web_access_settings SET enabled = 1, url_fetch_enabled = 1, \
             default_provider = 'searxng' WHERE singleton_id = 1",
        )
        .execute(database.pool())
        .await
        .unwrap();
        sqlx::query("UPDATE web_search_providers SET enabled = 1 WHERE provider_id = 'searxng'")
            .execute(database.pool())
            .await
            .unwrap();
        let temporary = tempfile::tempdir().unwrap();
        let entry = temporary.path().join("index.ts");
        std::fs::write(&entry, "export default () => {};").unwrap();
        let service = ExtensionService::new(database.pool().clone(), Some(entry)).unwrap();

        let snapshot = service
            .runtime_snapshot("agent-instance:future", "work-future")
            .await
            .unwrap();

        assert_eq!(snapshot.extension_paths.len(), 1);
        assert_eq!(
            snapshot.tool_ids,
            [
                "web_search",
                "source_check",
                "fetch_content",
                "get_search_content"
            ]
        );
    }
}
