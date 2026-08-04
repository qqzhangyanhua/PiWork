use std::sync::OnceLock;

use crate::domain::environment::RuntimeStatus;

use super::{RuntimeStatusCache, detect_runtime_status};

fn runtime_status_cache() -> &'static RuntimeStatusCache {
    static CACHE: OnceLock<RuntimeStatusCache> = OnceLock::new();
    CACHE.get_or_init(RuntimeStatusCache::new)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_runtime_status() -> RuntimeStatus {
    runtime_status_cache()
        .get_or_init_with(detect_runtime_status)
        .await
}
