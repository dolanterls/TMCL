use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use tokio::sync::watch;
use uuid::Uuid;

use crate::core::error::Result;

use super::control::{DownloadTaskControl, wait_if_paused};

/// Minimum time between two progress reports that reach the engine.
///
/// A download reports once per network chunk - tens of thousands of calls
/// for one install - and every one of them would take the task state mutex,
/// which the UI also runs on (and which, once a second, holds it across a
/// disk write). The throttle therefore lives here, on the worker side: the
/// common call is one atomic read and never touches a lock.
const PROGRESS_PUBLISH_INTERVAL_MS: u64 = 100;

/// Per-run progress coalescer shared by every clone of one [`TaskContext`].
///
/// Concurrent downloads of a single task all report into the same task
/// record, so they share one clock: whoever wins the interval publishes, the
/// rest are dropped. Without the shared clock the record would also thrash
/// between per-file byte counts.
#[derive(Debug, Clone, Default)]
pub(crate) struct ProgressGate {
    last_publish_ms: Arc<AtomicU64>,
}

impl ProgressGate {
    fn should_publish(&self) -> bool {
        let now = unix_millis();
        let last = self.last_publish_ms.load(Ordering::Relaxed);
        if now.saturating_sub(last) < PROGRESS_PUBLISH_INTERVAL_MS {
            return false;
        }
        self.last_publish_ms
            .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    }
}

fn unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64)
}

pub(crate) trait TaskContextSink: Send + Sync {
    fn report_progress(
        &self,
        id: Uuid,
        generation: u64,
        completed: u64,
        total: Option<u64>,
    ) -> Result<()>;
    fn record_owned(&self, id: Uuid, generation: u64, path: PathBuf) -> Result<()>;
    fn record_committed(&self, id: Uuid, generation: u64, path: PathBuf) -> Result<()>;
    fn record_shared(&self, id: Uuid, generation: u64, key: String, path: PathBuf) -> Result<()>;
}

pub struct TaskContext {
    pub(crate) id: Uuid,
    pub(crate) generation: u64,
    pub(crate) cancelled: Arc<AtomicBool>,
    pub(crate) paused: watch::Receiver<bool>,
    pub(crate) sink: Arc<dyn TaskContextSink>,
    pub(crate) progress_gate: ProgressGate,
}

impl Clone for TaskContext {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            generation: self.generation,
            cancelled: self.cancelled.clone(),
            paused: self.paused.clone(),
            sink: self.sink.clone(),
            progress_gate: self.progress_gate.clone(),
        }
    }
}

impl TaskContext {
    pub fn task_id(&self) -> Uuid {
        self.id
    }

    pub fn report_progress(&self, completed: u64, total: Option<u64>) -> Result<()> {
        // A finished artifact always reports: the throttle must never hide the
        // last byte of a file, or a completed task would keep the byte count
        // it had up to 100 ms earlier. Same rule as HMCL's `progress == 1.0`.
        let finished = total.is_some_and(|total| completed >= total);
        if !finished && !self.progress_gate.should_publish() {
            return Ok(());
        }
        self.sink
            .report_progress(self.id, self.generation, completed, total)
    }

    pub async fn wait_if_paused(&self) -> Result<()> {
        wait_if_paused(Some(self), &self.cancelled).await
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    pub(crate) fn cancellation_flag(&self) -> &AtomicBool {
        &self.cancelled
    }

    pub fn own_temporary_file(&self, path: impl Into<PathBuf>) -> Result<()> {
        self.sink
            .record_owned(self.id, self.generation, path.into())
    }

    pub fn commit_file(&self, path: impl Into<PathBuf>) -> Result<()> {
        self.sink
            .record_committed(self.id, self.generation, path.into())
    }

    pub fn reference_shared_file(
        &self,
        key: impl Into<String>,
        path: impl Into<PathBuf>,
    ) -> Result<()> {
        self.sink
            .record_shared(self.id, self.generation, key.into(), path.into())
    }
}

impl DownloadTaskControl for TaskContext {
    fn task_id(&self) -> Option<Uuid> {
        Some(self.id)
    }

    fn pause_receiver(&self) -> watch::Receiver<bool> {
        self.paused.clone()
    }

    fn record_downloaded_file(&self, path: &Path) -> Result<()> {
        self.own_temporary_file(path.to_path_buf())
    }

    fn record_file_reference(&self, path: &Path) -> Result<()> {
        self.reference_shared_file(path.display().to_string(), path.to_path_buf())
    }
}

#[cfg(test)]
impl TaskContext {
    /// A context with no engine behind it, for unit tests of task bodies.
    pub(crate) fn for_tests() -> Self {
        struct NoopSink;

        impl TaskContextSink for NoopSink {
            fn report_progress(
                &self,
                _id: Uuid,
                _generation: u64,
                _completed: u64,
                _total: Option<u64>,
            ) -> Result<()> {
                Ok(())
            }

            fn record_owned(&self, _id: Uuid, _generation: u64, _path: PathBuf) -> Result<()> {
                Ok(())
            }

            fn record_committed(&self, _id: Uuid, _generation: u64, _path: PathBuf) -> Result<()> {
                Ok(())
            }

            fn record_shared(
                &self,
                _id: Uuid,
                _generation: u64,
                _key: String,
                _path: PathBuf,
            ) -> Result<()> {
                Ok(())
            }
        }

        let (_sender, receiver) = watch::channel(false);
        Self {
            id: Uuid::new_v4(),
            generation: 0,
            cancelled: Arc::new(AtomicBool::new(false)),
            paused: receiver,
            sink: Arc::new(NoopSink),
            progress_gate: ProgressGate::default(),
        }
    }
}
