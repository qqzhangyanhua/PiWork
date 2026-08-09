use std::future::Future;

use tokio::sync::OnceCell;

use crate::domain::environment::RuntimeStatus;

pub struct RuntimeStatusCache {
    value: OnceCell<RuntimeStatus>,
}

impl Default for RuntimeStatusCache {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeStatusCache {
    pub const fn new() -> Self {
        Self {
            value: OnceCell::const_new(),
        }
    }

    pub async fn get_or_init_with<F, Fut>(&self, load: F) -> RuntimeStatus
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = RuntimeStatus>,
    {
        self.value.get_or_init(load).await.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use crate::domain::environment::{RuntimeCheck, RuntimeStatus};

    use super::RuntimeStatusCache;

    fn status() -> RuntimeStatus {
        let ready = RuntimeCheck {
            available: true,
            version: Some("1.0".into()),
        };
        RuntimeStatus {
            python: ready.clone(),
            node: ready.clone(),
            git: ready,
        }
    }

    #[tokio::test]
    async fn detects_only_once_per_cache() {
        let cache = RuntimeStatusCache::new();
        let calls = Arc::new(AtomicUsize::new(0));

        for _ in 0..2 {
            let calls = Arc::clone(&calls);
            let _ = cache
                .get_or_init_with(|| async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    status()
                })
                .await;
        }

        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn default_cache_uses_the_same_lazy_initialization_semantics() {
        let cache = RuntimeStatusCache::default();
        let calls = Arc::new(AtomicUsize::new(0));

        for _ in 0..2 {
            let calls = Arc::clone(&calls);
            let _ = cache
                .get_or_init_with(|| async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    status()
                })
                .await;
        }

        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
