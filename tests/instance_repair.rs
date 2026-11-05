//! Repairs download exactly the files a launch plan needs, once.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;

use tmcl::core::download::{DownloadSourceSnapshot, sha1_file};
use tmcl::core::instance::repair::repair;
use tmcl::core::launch::plan::FileRole;
use tmcl::core::launch::preflight::MissingFile;
use tmcl::core::task::InstanceRepairSpec;

fn sha1_of(bytes: &[u8]) -> String {
    let path = std::env::temp_dir().join(format!("tmcl-sha1-{}", uuid::Uuid::new_v4()));
    std::fs::write(&path, bytes).expect("temp file");
    let hash = sha1_file(&path).expect("sha1");
    let _ = std::fs::remove_file(&path);
    hash
}

fn serve_once(body: Vec<u8>) -> (String, Arc<AtomicUsize>, thread::JoinHandle<()>) {
    serve_bodies(vec![body])
}

/// Serves the queued bodies in order; extra requests repeat the last one.
fn serve_bodies(bodies: Vec<Vec<u8>>) -> (String, Arc<AtomicUsize>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture");
    let address = listener.local_addr().expect("fixture address");
    let count = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&count);
    let thread = thread::spawn(move || {
        for (served, stream) in listener.incoming().enumerate() {
            let Ok(mut stream) = stream else { break };
            let mut buffer = [0_u8; 2048];
            let _ = stream.read(&mut buffer);
            observed.fetch_add(1, Ordering::SeqCst);
            let body = &bodies[served.min(bodies.len() - 1)];
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(body);
        }
    });
    (format!("http://{address}/library.jar"), count, thread)
}

fn instance_spec(root: &std::path::Path, url: &str, sha1: &str) -> InstanceRepairSpec {
    let instance_directory = root.join("versions/pack");
    let game_directory = root.to_path_buf();
    std::fs::create_dir_all(&instance_directory).expect("instance dir");
    std::fs::write(instance_directory.join("pack.jar"), b"jar").expect("client jar");
    let json = format!(
        r#"{{"id":"pack","mainClass":"net.minecraft.client.main.Main","arguments":{{"game":[],"jvm":[]}},"libraries":[{{"name":"org.example:library:1.0","downloads":{{"artifact":{{"path":"org/example/library/1.0/library-1.0.jar","url":"{url}","sha1":"{sha1}"}}}}}}]}}"#
    );
    std::fs::write(instance_directory.join("pack.json"), json).expect("client json");

    InstanceRepairSpec {
        instance_id: uuid::Uuid::new_v4(),
        instance_name: "pack".into(),
        instance_directory,
        game_directory,
        cache_root: root.join("cache"),
        missing: vec![MissingFile {
            path: root.join("libraries/org/example/library/1.0/library-1.0.jar"),
            relative: Some(std::path::PathBuf::from(
                "libraries/org/example/library/1.0/library-1.0.jar",
            )),
            role: FileRole::Library,
        }],
        download_snapshot: DownloadSourceSnapshot::default(),
    }
}

/// Two missing libraries, so a cancellation can land between them.
fn two_library_spec(
    root: &std::path::Path,
    first: (&str, &str),
    second: (&str, &str),
) -> InstanceRepairSpec {
    let instance_directory = root.join("versions/pack");
    let game_directory = root.to_path_buf();
    std::fs::create_dir_all(&instance_directory).expect("instance dir");
    std::fs::write(instance_directory.join("pack.jar"), b"jar").expect("client jar");
    let json = r#"{"id":"pack","mainClass":"net.minecraft.client.main.Main","arguments":{"game":[],"jvm":[]},"libraries":[{"name":"org.example:a:1.0","downloads":{"artifact":{"path":"org/example/a/1.0/a-1.0.jar","url":"__A__","sha1":"__ASHA__"}}},{"name":"org.example:b:1.0","downloads":{"artifact":{"path":"org/example/b/1.0/b-1.0.jar","url":"__B__","sha1":"__BSHA__"}}}]}"#
        .replace("__A__", first.0)
        .replace("__ASHA__", first.1)
        .replace("__B__", second.0)
        .replace("__BSHA__", second.1);
    std::fs::write(instance_directory.join("pack.json"), json).expect("client json");

    InstanceRepairSpec {
        instance_id: uuid::Uuid::new_v4(),
        instance_name: "pack".into(),
        instance_directory,
        game_directory,
        cache_root: root.join("cache"),
        missing: vec![
            MissingFile {
                path: root.join("libraries/org/example/a/1.0/a-1.0.jar"),
                relative: Some(std::path::PathBuf::from(
                    "libraries/org/example/a/1.0/a-1.0.jar",
                )),
                role: FileRole::Library,
            },
            MissingFile {
                path: root.join("libraries/org/example/b/1.0/b-1.0.jar"),
                relative: Some(std::path::PathBuf::from(
                    "libraries/org/example/b/1.0/b-1.0.jar",
                )),
                role: FileRole::Library,
            },
        ],
        download_snapshot: DownloadSourceSnapshot::default(),
    }
}

#[tokio::test]
async fn repair_downloads_the_missing_library_once() {
    let body = b"library-bytes".to_vec();
    let sha1 = sha1_of(&body);
    let (url, requests, server) = serve_once(body.clone());
    let root = std::env::temp_dir().join(format!("tmcl-repair-{}", uuid::Uuid::new_v4()));
    let spec = instance_spec(&root, &url, &sha1);
    let cancelled = AtomicBool::new(false);

    let first = repair(&spec, None, &cancelled, |_, _| Ok(()))
        .await
        .expect("first repair");
    assert_eq!(first.downloaded, 1);

    let repaired = root.join("libraries/org/example/library/1.0/library-1.0.jar");
    assert_eq!(std::fs::read(&repaired).expect("downloaded file"), body);

    let second = repair(&spec, None, &cancelled, |_, _| Ok(()))
        .await
        .expect("second repair");
    assert_eq!(
        second.downloaded, 0,
        "an existing file is not downloaded again"
    );
    assert_eq!(requests.load(Ordering::SeqCst), 1);

    drop(server);
    std::fs::remove_dir_all(root).expect("clean");
}

#[tokio::test]
async fn repair_reports_files_it_cannot_download() {
    let root = std::env::temp_dir().join(format!("tmcl-repair-{}", uuid::Uuid::new_v4()));
    let mut spec = instance_spec(&root, "https://example.invalid/library.jar", "deadbeef");
    spec.missing[0].relative = Some(std::path::PathBuf::from("libraries/unknown.jar"));
    let cancelled = AtomicBool::new(false);

    let error = repair(&spec, None, &cancelled, |_, _| Ok(()))
        .await
        .expect_err("unknown metadata cannot be repaired");

    assert!(
        error.to_string().contains("cannot be downloaded"),
        "{error}"
    );
    assert!(
        error.to_string().contains("unknown.jar"),
        "the message names the file it cannot fetch: {error}"
    );
    std::fs::remove_dir_all(root).expect("clean");
}

#[tokio::test]
async fn repair_marks_connectivity_failures_as_offline() {
    let root = std::env::temp_dir().join(format!("tmcl-repair-{}", uuid::Uuid::new_v4()));
    let mut spec = instance_spec(&root, "http://127.0.0.1:1/library.jar", "deadbeef");
    spec.download_snapshot.retry_count = 0;
    let cancelled = AtomicBool::new(false);

    let error = repair(&spec, None, &cancelled, |_, _| Ok(()))
        .await
        .expect_err("an unreachable source must fail");

    assert!(
        error
            .to_string()
            .contains(tmcl::core::launch::failure::OFFLINE_MARKER),
        "{error}"
    );
    std::fs::remove_dir_all(root).expect("clean");
}

#[tokio::test]
async fn repair_refuses_installs_that_would_download_too_many_files() {
    let root = std::env::temp_dir().join(format!("tmcl-repair-{}", uuid::Uuid::new_v4()));
    let mut spec = instance_spec(&root, "https://example.invalid/library.jar", "deadbeef");
    let template = spec.missing[0].clone();
    spec.missing = (0..801)
        .map(|index| MissingFile {
            path: root.join(format!(
                "libraries/org/example/library/1.0/library-{index}.jar"
            )),
            ..template.clone()
        })
        .collect();
    let cancelled = AtomicBool::new(false);

    let error = repair(&spec, None, &cancelled, |_, _| Ok(()))
        .await
        .expect_err("the budget must stop the repair");

    assert!(error.to_string().contains("limit 800"), "{error}");
    std::fs::remove_dir_all(root).expect("clean");
}

#[tokio::test]
async fn repair_retries_a_file_whose_checksum_does_not_match() {
    let good = b"correct-library-bytes".to_vec();
    let (url, requests, server) = serve_bodies(vec![b"corrupted".to_vec(), good.clone()]);
    let root = std::env::temp_dir().join(format!("tmcl-repair-{}", uuid::Uuid::new_v4()));
    let mut spec = instance_spec(&root, &url, &sha1_of(&good));
    spec.download_snapshot.retry_count = 1;
    let cancelled = AtomicBool::new(false);

    let summary = repair(&spec, None, &cancelled, |_, _| Ok(()))
        .await
        .expect("the second attempt matches the checksum");

    assert_eq!(summary.downloaded, 1);
    assert!(
        requests.load(Ordering::SeqCst) >= 2,
        "a checksum mismatch must be retried, not accepted"
    );
    let repaired = root.join("libraries/org/example/library/1.0/library-1.0.jar");
    assert_eq!(std::fs::read(&repaired).expect("downloaded file"), good);

    drop(server);
    std::fs::remove_dir_all(root).expect("clean");
}

#[tokio::test]
async fn cancelled_repair_leaves_existing_files_untouched() {
    let first = b"first-library-bytes".to_vec();
    let second = b"second-library-bytes".to_vec();
    let (first_url, first_requests, first_server) = serve_once(first.clone());
    let (second_url, second_requests, second_server) = serve_once(second);
    let root = std::env::temp_dir().join(format!("tmcl-repair-{}", uuid::Uuid::new_v4()));
    let spec = two_library_spec(
        &root,
        (&first_url, &sha1_of(&first)),
        (&second_url, &"0".repeat(40)),
    );
    // Pretend an earlier session downloaded the first library, then the user
    // cancelled before the second one started.
    let already_downloaded = root.join("libraries/org/example/a/1.0/a-1.0.jar");
    std::fs::create_dir_all(already_downloaded.parent().expect("library dir")).expect("dirs");
    std::fs::write(&already_downloaded, &first).expect("existing library");
    let cancelled = AtomicBool::new(true);

    let error = repair(&spec, None, &cancelled, |_, _| Ok(()))
        .await
        .expect_err("a cancelled task stops before downloading");

    assert!(error.to_string().contains("cancelled"), "{error}");
    assert_eq!(
        std::fs::read(&already_downloaded).expect("existing file"),
        first,
        "cancelling must not delete or rewrite files already on disk"
    );
    assert!(!root.join("libraries/org/example/b/1.0/b-1.0.jar").exists());
    assert_eq!(first_requests.load(Ordering::SeqCst), 0);
    assert_eq!(second_requests.load(Ordering::SeqCst), 0);

    drop(first_server);
    drop(second_server);
    std::fs::remove_dir_all(root).expect("clean");
}
