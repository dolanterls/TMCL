use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::sync::watch;
use uuid::Uuid;

use crate::core::error::{Error, Result};

pub trait DownloadTaskControl: Send + Sync {
    fn task_id(&self) -> Option<Uuid> {
        None
    }

    fn pause_receiver(&self) -> watch::Receiver<bool>;

    fn record_downloaded_file(&self, path: &Path) -> Result<()>;

    fn record_file_reference(&self, path: &Path) -> Result<()>;
}

pub struct ScopedDownloadTaskControl<'a> {
    inner: &'a dyn DownloadTaskControl,
    cleanup_root: PathBuf,
}

impl<'a> ScopedDownloadTaskControl<'a> {
    pub fn new(inner: &'a dyn DownloadTaskControl, cleanup_root: impl Into<PathBuf>) -> Self {
        Self {
            inner,
            cleanup_root: cleanup_root.into(),
        }
    }
}

impl DownloadTaskControl for ScopedDownloadTaskControl<'_> {
    fn task_id(&self) -> Option<Uuid> {
        self.inner.task_id()
    }

    fn pause_receiver(&self) -> watch::Receiver<bool> {
        self.inner.pause_receiver()
    }

    fn record_downloaded_file(&self, path: &Path) -> Result<()> {
        let contained = path
            .canonicalize()
            .ok()
            .zip(self.cleanup_root.canonicalize().ok())
            .is_some_and(|(path, root)| path.starts_with(root));
        if contained {
            self.inner.record_downloaded_file(path)
        } else {
            self.inner.record_file_reference(path)
        }
    }

    fn record_file_reference(&self, path: &Path) -> Result<()> {
        self.inner.record_file_reference(path)
    }
}

pub async fn wait_if_paused(
    control: Option<&dyn DownloadTaskControl>,
    cancelled: &AtomicBool,
) -> Result<()> {
    let Some(control) = control else {
        return if cancelled.load(Ordering::Relaxed) {
            Err(Error::Download("download cancelled".into()))
        } else {
            Ok(())
        };
    };

    let mut paused = control.pause_receiver();
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Err(Error::Download("download cancelled".into()));
        }
        if !*paused.borrow() {
            return Ok(());
        }
        tokio::select! {
            changed = paused.changed() => {
                if changed.is_err() {
                    return Err(Error::Download("task pause control closed".into()));
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {}
        }
    }
}
