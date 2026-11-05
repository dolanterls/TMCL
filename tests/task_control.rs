use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use tmcl::core::task::{DownloadTaskControl, ScopedDownloadTaskControl, wait_if_paused};
use tokio::sync::watch;
use uuid::Uuid;

#[derive(Clone)]
struct TestControl {
    id: Uuid,
    paused: watch::Receiver<bool>,
    downloaded: std::sync::Arc<std::sync::Mutex<Vec<PathBuf>>>,
    referenced: std::sync::Arc<std::sync::Mutex<Vec<PathBuf>>>,
}

impl DownloadTaskControl for TestControl {
    fn task_id(&self) -> Option<Uuid> {
        Some(self.id)
    }

    fn pause_receiver(&self) -> watch::Receiver<bool> {
        self.paused.clone()
    }

    fn record_downloaded_file(&self, path: &Path) -> tmcl::core::error::Result<()> {
        self.downloaded
            .lock()
            .expect("downloaded lock")
            .push(path.to_path_buf());
        Ok(())
    }

    fn record_file_reference(&self, path: &Path) -> tmcl::core::error::Result<()> {
        self.referenced
            .lock()
            .expect("referenced lock")
            .push(path.to_path_buf());
        Ok(())
    }
}

fn control() -> (TestControl, watch::Sender<bool>) {
    let (sender, receiver) = watch::channel(false);
    (
        TestControl {
            id: Uuid::new_v4(),
            paused: receiver,
            downloaded: Default::default(),
            referenced: Default::default(),
        },
        sender,
    )
}

#[test]
fn scoped_control_separates_owned_files_from_shared_references() {
    let root = std::env::temp_dir().join(format!("tmcl-control-{}", Uuid::new_v4()));
    let owned = root.join("owned.part");
    let shared_root = root.join("shared");
    let shared = shared_root.join("library.jar");
    std::fs::create_dir_all(&shared_root).expect("create fixture");
    std::fs::write(&owned, b"partial").expect("write owned");
    std::fs::write(&shared, b"shared").expect("write shared");
    let outside = std::env::temp_dir().join(format!("tmcl-shared-{}.jar", Uuid::new_v4()));
    std::fs::write(&outside, b"outside").expect("write outside");

    let (inner, _) = control();
    let scoped = ScopedDownloadTaskControl::new(&inner, &root);
    scoped.record_downloaded_file(&owned).expect("record owned");
    scoped
        .record_downloaded_file(&outside)
        .expect("record outside");

    assert_eq!(
        inner.downloaded.lock().expect("downloaded lock").as_slice(),
        &[owned]
    );
    assert_eq!(
        inner.referenced.lock().expect("referenced lock").as_slice(),
        std::slice::from_ref(&outside)
    );

    std::fs::remove_dir_all(root).expect("cleanup root");
    std::fs::remove_file(outside).expect("cleanup outside");
}

#[tokio::test]
async fn wait_if_paused_releases_only_after_resume() {
    let (inner, sender) = control();
    sender.send(true).expect("pause");
    let cancelled = AtomicBool::new(false);
    let waiter = tokio::spawn(async move {
        wait_if_paused(Some(&inner), &cancelled)
            .await
            .expect("resume");
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(!waiter.is_finished());
    sender.send(false).expect("resume");
    tokio::time::timeout(Duration::from_secs(1), waiter)
        .await
        .expect("waiter completes")
        .expect("join waiter");
}

#[tokio::test]
async fn wait_if_paused_returns_cancellation_without_a_control() {
    let cancelled = AtomicBool::new(true);
    let error = wait_if_paused(None, &cancelled)
        .await
        .expect_err("cancelled task");
    assert!(error.to_string().contains("cancelled"));
}
