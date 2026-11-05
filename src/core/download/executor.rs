use std::sync::{Arc, OnceLock, RwLock};

use tokio::sync::Semaphore;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadExecutorConfig {
    pub auto: bool,
    pub threads: usize,
}

impl Default for DownloadExecutorConfig {
    fn default() -> Self {
        Self {
            auto: true,
            threads: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DownloadExecutor {
    semaphore: Arc<Semaphore>,
    concurrency: usize,
}

impl DownloadExecutor {
    pub fn new(config: DownloadExecutorConfig) -> Self {
        let concurrency = if config.auto {
            default_concurrency()
        } else {
            config.threads.max(1)
        };
        Self {
            semaphore: Arc::new(Semaphore::new(concurrency)),
            concurrency,
        }
    }

    pub fn global() -> Arc<Self> {
        global_lock()
            .read()
            .expect("download executor lock poisoned")
            .clone()
    }

    pub fn configure_global(config: DownloadExecutorConfig) {
        *global_lock()
            .write()
            .expect("download executor lock poisoned") = Arc::new(Self::new(config));
    }

    pub fn concurrency(&self) -> usize {
        self.concurrency
    }

    pub async fn acquire(
        &self,
    ) -> Result<tokio::sync::OwnedSemaphorePermit, tokio::sync::AcquireError> {
        self.semaphore.clone().acquire_owned().await
    }
}

fn global_lock() -> &'static RwLock<Arc<DownloadExecutor>> {
    static GLOBAL: OnceLock<RwLock<Arc<DownloadExecutor>>> = OnceLock::new();
    GLOBAL.get_or_init(|| {
        RwLock::new(Arc::new(DownloadExecutor::new(
            DownloadExecutorConfig::default(),
        )))
    })
}

pub fn default_concurrency() -> usize {
    std::thread::available_parallelism()
        .map(|value| value.get().saturating_mul(4))
        .unwrap_or(4)
        .clamp(1, 64)
}

#[cfg(test)]
mod tests {
    use super::{DownloadExecutor, DownloadExecutorConfig, default_concurrency};

    #[test]
    fn automatic_concurrency_matches_hmcl_bound() {
        assert_eq!(
            DownloadExecutor::new(DownloadExecutorConfig::default()).concurrency(),
            default_concurrency()
        );
        assert!(default_concurrency() >= 1);
    }

    #[test]
    fn manual_concurrency_is_clamped_to_one() {
        assert_eq!(
            DownloadExecutor::new(DownloadExecutorConfig {
                auto: false,
                threads: 0,
            })
            .concurrency(),
            1
        );
    }
}
