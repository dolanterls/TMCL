use std::fs::File;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use futures_util::StreamExt;
use std::path::{Component, Path, PathBuf};

use crate::core::download::{
    DownloadAuthentication, DownloadRequest, DownloadSourceSnapshot, IntegrityKind,
    download_request,
};
use crate::core::error::{Error, Result};
use crate::core::resource::conflict::{check_available, normalize_target, validate_file_name};
use crate::core::task::{
    ModpackFilesSpec, ModpackInstallSpec, ResourceInstallSpec, ResourceSaveAsSpec, TaskContext,
};
use crate::core::version::http::HttpClient;

pub async fn install_resource(spec: ResourceInstallSpec, context: TaskContext) -> Result<()> {
    validate_file_name(&spec.file_name).map_err(Error::Download)?;
    ensure_within(&spec.instance_root, &spec.destination)?;
    download_to(
        &HttpClient::with_timeout(spec.download_snapshot.timeout_secs)?,
        &spec.candidate_urls,
        spec.destination,
        spec.cache_root,
        spec.expected_sha1,
        spec.expected_size,
        spec.source,
        format!("{} {}", spec.kind.label(), spec.file_name),
        spec.download_snapshot,
        &context,
        |completed, total| context.report_progress(completed, total),
    )
    .await
}

pub async fn save_world(spec: ResourceSaveAsSpec, context: TaskContext) -> Result<()> {
    validate_file_name(&spec.file_name).map_err(Error::Download)?;
    let destination = normalize_target(&spec.destination).map_err(Error::Download)?;
    download_to(
        &HttpClient::with_timeout(spec.download_snapshot.timeout_secs)?,
        &spec.candidate_urls,
        destination,
        spec.cache_root,
        spec.expected_sha1,
        spec.expected_size,
        spec.source,
        format!("world {}", spec.file_name),
        spec.download_snapshot,
        &context,
        |completed, total| context.report_progress(completed, total),
    )
    .await
}

pub async fn download_modpack(spec: ModpackInstallSpec, context: TaskContext) -> Result<()> {
    validate_file_name(&spec.file_name).map_err(Error::Download)?;
    let destination = normalize_target(&spec.staging_path).map_err(Error::Download)?;
    check_available(&destination).map_err(Error::Download)?;
    download_to(
        &HttpClient::with_timeout(spec.download_snapshot.timeout_secs)?,
        &spec.candidate_urls,
        destination,
        spec.cache_root,
        spec.expected_sha1,
        spec.expected_size,
        spec.source,
        format!("modpack {}", spec.file_name),
        spec.download_snapshot,
        &context,
        |completed, total| context.report_progress(completed, total),
    )
    .await
}

pub async fn install_modpack_files(spec: ModpackFilesSpec, context: TaskContext) -> Result<()> {
    let client = HttpClient::with_timeout(spec.download_snapshot.timeout_secs)?;
    let instance_root = spec.instance_root;
    let cache_root = spec.cache_root;
    let policy = spec.download_snapshot;
    // One bar for the whole task: the sum of every file's size, and a byte
    // counter that only ever grows. Reporting each file's own byte count made
    // the task record jump between file sizes, so the list showed a number
    // that was neither the task's size nor its progress.
    let total: Option<u64> = spec.files.iter().map(|file| file.size).sum();
    let completed = Arc::new(AtomicU64::new(0));
    let total_files = spec.files.len();
    let downloads = spec.files.into_iter().map(|file| {
        let client = client.clone();
        let root = instance_root.clone();
        let cache_root = cache_root.clone();
        let policy = policy.clone();
        let completed = Arc::clone(&completed);
        let context = &context;
        async move {
            let label = file.path.clone();
            let fail = |error: crate::core::error::Error| (label.clone(), error.to_string());
            let relative = safe_modpack_path(&file.path).map_err(fail)?;
            let destination = root.join(relative);
            let valid = if destination.is_file() {
                // Re-hashing 107 files at once is the same blocking work as
                // downloading them, so it runs off the async workers too.
                let path = destination.clone();
                let sha1 = file.sha1.clone();
                let size = file.size;
                crate::core::download::blocking(move || {
                    Ok(match (sha1.as_deref(), size) {
                        (Some(hash), _) => {
                            crate::core::download::verify_sha1(&path, Some(hash)).is_ok()
                        }
                        (None, Some(size)) => {
                            std::fs::metadata(&path).is_ok_and(|metadata| metadata.len() == size)
                        }
                        _ => false,
                    })
                })
                .await
                .map_err(fail)?
            } else {
                false
            };
            if valid {
                // Already on disk: count it so the bar starts where the work
                // actually stands instead of at zero.
                let bytes = std::fs::metadata(&destination)
                    .map_err(|error| (label.clone(), error.to_string()))?
                    .len();
                let done = completed.fetch_add(bytes, Ordering::Relaxed) + bytes;
                context
                    .report_progress(done, total)
                    .map_err(|error| (label.clone(), error.to_string()))?;
                return Ok(());
            }
            if destination.is_file() {
                std::fs::remove_file(&destination)
                    .map_err(|error| (label.clone(), error.to_string()))?;
            }
            // A pack is one bad mirror away from losing every mod that came
            // after it; one attempt with a short backoff keeps a transient
            // timeout from failing the whole install.
            let mut counted = 0_u64;
            let mut last_error = None;
            for attempt in 0..2 {
                if attempt > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                }
                let result = download_to(
                    &client,
                    &file.urls,
                    destination.clone(),
                    cache_root.clone(),
                    file.sha1.clone(),
                    file.size,
                    crate::core::resource::model::ResourceSource::Modrinth,
                    format!("modpack file {}", file.path),
                    policy.clone(),
                    context,
                    |bytes, _| {
                        let delta = bytes.saturating_sub(counted);
                        counted = bytes;
                        let done = completed.fetch_add(delta, Ordering::Relaxed) + delta;
                        context.report_progress(done, total)
                    },
                )
                .await;
                match result {
                    Ok(()) => return Ok(()),
                    Err(error) => last_error = Some(error),
                }
            }
            Err((
                file.path,
                last_error.map_or_else(|| "download failed".to_owned(), |error| error.to_string()),
            ))
        }
    });
    let mut stream = futures_util::stream::iter(downloads).buffer_unordered(8);
    let mut failed: Vec<(String, String)> = Vec::new();
    while let Some(result) = stream.next().await {
        if let Err(failure) = result {
            failed.push(failure);
        }
    }
    if !failed.is_empty() {
        failed.sort_by(|left, right| left.0.cmp(&right.0));
        let listed = failed
            .iter()
            .take(3)
            .map(|(path, error)| format!("{path} ({error})"))
            .collect::<Vec<_>>()
            .join(", ");
        let remaining = failed.len().saturating_sub(3);
        let suffix = if remaining > 0 {
            format!(" (+{remaining} more)")
        } else {
            String::new()
        };
        return Err(Error::Download(format!(
            "{} of {total_files} modpack files could not be downloaded: {listed}{suffix}; the other files are installed, retry the task to resume the missing ones",
            failed.len()
        )));
    }
    // Land the final byte count. The throttle would otherwise keep whatever
    // report happened to pass the gate last, and a finished task would show
    // slightly less than its total.
    let done = completed.load(Ordering::Relaxed);
    context.report_progress(done, total)?;
    Ok(())
}

fn safe_modpack_path(value: &str) -> Result<PathBuf> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(Error::Download(format!(
            "unsafe modpack file path: {value:?}"
        )));
    }
    Ok(path.to_path_buf())
}

pub fn extract_modpack_archive(archive_path: &Path, target_root: &Path) -> Result<Vec<String>> {
    let mut archive = crate::core::resource::archive::open(archive_path)?;
    // The relative paths of every written file: the caller records them as the
    // pack's ownership so a later update knows what it may replace.
    let mut written = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| Error::Download(format!("read modpack entry: {error}")))?;
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| Error::Download("modpack archive contains an unsafe path".into()))?
            .to_path_buf();
        let relative = enclosed
            .strip_prefix("overrides")
            .unwrap_or(&enclosed)
            .to_path_buf();
        if relative.as_os_str().is_empty()
            || relative == Path::new("manifest.json")
            || relative == Path::new("modrinth.index.json")
            // An exported instance carries its metadata as a snapshot. Copying
            // it would replace the new instance's id, name and game directory.
            || relative == Path::new("tmcl.json")
            // The pack ownership record is per instance; a pack exported from
            // one instance must not hand its old record to another.
            || relative == Path::new(".tmcl-pack.json")
        {
            continue;
        }
        let destination = target_root.join(&relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&destination)?;
            continue;
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut output = File::create(&destination)?;
        // Some packers compress an empty file, then label the entry `stored`
        // with a zero uncompressed size: two bytes of deflate stream, compressed
        // size 2, CRC 0. `zip` copies the two bytes and then fails the checksum,
        // while readers that trust the declared size (Python, and our own
        // extraction before this) produce an empty file — which is what the
        // entry says it is, and what inflating those two bytes returns.
        if entry.size() > 0 {
            std::io::copy(&mut entry, &mut output)?;
        }
        written.push(relative.to_string_lossy().replace('\\', "/"));
    }
    Ok(written)
}

#[allow(clippy::too_many_arguments)]
async fn download_to<P>(
    client: &HttpClient,
    candidate_urls: &[String],
    destination: PathBuf,
    cache_root: PathBuf,
    expected_sha1: Option<String>,
    expected_size: Option<u64>,
    source: crate::core::resource::model::ResourceSource,
    artifact_label: String,
    policy: DownloadSourceSnapshot,
    context: &TaskContext,
    progress: P,
) -> Result<()>
where
    P: FnMut(u64, Option<u64>) -> Result<()> + Send,
{
    let authentication = match source {
        crate::core::resource::model::ResourceSource::Modrinth => DownloadAuthentication::None,
        crate::core::resource::model::ResourceSource::CurseForge => {
            DownloadAuthentication::CurseForge
        }
    };
    let request = DownloadRequest::new(
        candidate_urls.to_vec(),
        destination.clone(),
        expected_sha1,
        expected_size,
        artifact_label,
    )
    .with_integrity(IntegrityKind::File)
    .with_authentication(authentication)
    .without_replacing()
    .with_policy(policy);
    download_request(
        client,
        &request,
        &cache_root,
        context.cancellation_flag(),
        Some(context),
        progress,
    )
    .await?;
    context.commit_file(destination)?;
    Ok(())
}

fn ensure_within(root: &Path, destination: &Path) -> Result<()> {
    let parent = destination
        .parent()
        .ok_or_else(|| Error::Download("resource destination has no parent".into()))?;
    if !comparable(parent).starts_with(comparable(root)) {
        return Err(Error::Download(
            "resource destination escapes the instance content root".into(),
        ));
    }
    check_available(destination).map_err(Error::Download)
}

/// Absolute form of `path` that is safe to compare with another absolute form.
///
/// The target directory often does not exist yet (`mods/` on a fresh instance),
/// and `canonicalize` then fails while the root succeeds. On Windows the two
/// results would differ by the `\\?\` prefix and look like an escape. Stripping
/// the prefix on both sides makes the comparison depend on the path, not on
/// which of the two happened to exist.
fn comparable(path: &Path) -> PathBuf {
    let absolute = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    crate::core::instance::path::normalize_canonical(absolute)
}

#[cfg(test)]
mod tests {
    use super::{ensure_within, extract_modpack_archive, safe_modpack_path};
    use std::path::PathBuf;

    #[test]
    fn destination_must_remain_under_root() {
        let root =
            std::env::temp_dir().join(format!("tmcl-resource-root-{}", uuid::Uuid::new_v4()));
        let inside = root.join("mods").join("demo.jar");
        std::fs::create_dir_all(inside.parent().expect("parent")).expect("create root");
        assert!(ensure_within(&root, &inside).is_ok());
        assert!(ensure_within(&root, &PathBuf::from("outside.jar")).is_err());
        std::fs::remove_dir_all(root).expect("clean root");
    }

    #[test]
    fn a_destination_directory_that_does_not_exist_yet_is_still_inside() {
        let root =
            std::env::temp_dir().join(format!("tmcl-resource-root-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create root");
        // `mods/` is created by the install itself, after this check: the
        // missing directory must not be mistaken for an escape.
        let inside = root.join("mods").join("demo.jar");
        assert!(
            ensure_within(&root, &inside).is_ok(),
            "a fresh instance has no mods directory yet"
        );
        std::fs::remove_dir_all(root).expect("clean root");
    }

    #[test]
    fn modpack_extraction_rejects_traversal() {
        let root = std::env::temp_dir().join(format!("tmcl-modpack-{}", uuid::Uuid::new_v4()));
        let archive = root.join("pack.zip");
        let target = root.join("target");
        std::fs::create_dir_all(&target).expect("create target");
        let file = std::fs::File::create(&archive).expect("create archive");
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file(
                "overrides/config/demo.txt",
                zip::write::FileOptions::<()>::default(),
            )
            .expect("start file");
        use std::io::Write;
        writer.write_all(b"ok").expect("write file");
        writer.finish().expect("finish archive");
        extract_modpack_archive(&archive, &target).expect("extract archive");
        assert_eq!(
            std::fs::read(target.join("config/demo.txt")).expect("read file"),
            b"ok"
        );
        std::fs::remove_dir_all(root).expect("clean archive");
    }

    /// Empty files are stored by some packers as `deflate(03 00)` with a zero
    /// uncompressed size. `zip` reads the two bytes and then reports a checksum
    /// failure, which used to abort the whole import.
    #[test]
    fn modpack_extraction_accepts_an_empty_deflate_entry() {
        let root = std::env::temp_dir().join(format!("tmcl-empty-{}", uuid::Uuid::new_v4()));
        let archive = root.join("pack.zip");
        let target = root.join("target");
        std::fs::create_dir_all(&target).expect("create target");
        let mut bytes = Vec::new();
        // Local header + the two-byte empty deflate stream.
        bytes.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        bytes.extend_from_slice(&20u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&8u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&26u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(b"overrides/config/empty.txt");
        bytes.extend_from_slice(&[0x03, 0x00]);
        let central_offset = bytes.len() as u32;
        bytes.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        bytes.extend_from_slice(&0x0314u16.to_le_bytes());
        bytes.extend_from_slice(&20u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&8u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&26u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(b"overrides/config/empty.txt");
        let central_size = bytes.len() as u32 - central_offset;
        bytes.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&central_size.to_le_bytes());
        bytes.extend_from_slice(&central_offset.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        std::fs::write(&archive, &bytes).expect("write archive");

        let written = extract_modpack_archive(&archive, &target).expect("extract archive");
        assert_eq!(written, vec!["config/empty.txt".to_owned()]);
        assert_eq!(
            std::fs::metadata(target.join("config/empty.txt"))
                .expect("stat")
                .len(),
            0
        );
        std::fs::remove_dir_all(root).expect("clean archive");
    }

    /// Serves three paths over HTTP until the test flips `running`.
    fn fixture_server(
        running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> (String, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::atomic::Ordering;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture");
        listener.set_nonblocking(true).expect("nonblocking");
        let address = listener.local_addr().expect("address");
        let handle = std::thread::spawn(move || {
            while running.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut buffer = [0u8; 2048];
                        let read = stream.read(&mut buffer).unwrap_or(0);
                        let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                        let path = request
                            .split_whitespace()
                            .nth(1)
                            .unwrap_or_default()
                            .to_owned();
                        let (status, body) = match path.as_str() {
                            "/a.jar" => ("200 OK", b"aaaa".as_slice()),
                            "/b.jar" => ("200 OK", b"bbbb".as_slice()),
                            _ => ("404 Not Found", b"missing".as_slice()),
                        };
                        let response = format!(
                            "HTTP/1.1 {status}\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(response.as_bytes());
                        let _ = stream.write_all(body);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        (format!("http://{address}"), handle)
    }

    #[tokio::test]
    async fn one_bad_modpack_file_does_not_abort_the_rest() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let root = std::env::temp_dir().join(format!("tmcl-partial-{}", uuid::Uuid::new_v4()));
        let instance = root.join("instance");
        std::fs::create_dir_all(&instance).expect("instance dir");
        let running = std::sync::Arc::new(AtomicBool::new(true));
        let (base, server) = fixture_server(running.clone());

        let file = |path: &str, name: &str| crate::core::task::ModpackFileSpec {
            path: path.to_owned(),
            urls: vec![format!("{base}/{name}")],
            sha1: None,
            size: Some(4),
        };
        let spec = crate::core::task::ModpackFilesSpec {
            instance_id: uuid::Uuid::new_v4(),
            instance_root: instance.clone(),
            cache_root: root.join("cache"),
            files: vec![
                file("mods/a.jar", "a.jar"),
                file("mods/missing.jar", "missing.jar"),
                file("mods/b.jar", "b.jar"),
            ],
            download_snapshot: crate::core::download::DownloadSourceSnapshot::default(),
        };
        let error = super::install_modpack_files(spec, crate::core::task::TaskContext::for_tests())
            .await
            .expect_err("the missing file must be reported");
        running.store(false, Ordering::Relaxed);
        server.join().expect("join server");

        let message = error.to_string();
        assert!(message.contains("1 of 3"), "{message}");
        assert!(message.contains("mods/missing.jar"), "{message}");
        assert_eq!(
            std::fs::read(instance.join("mods/a.jar")).expect("a"),
            b"aaaa"
        );
        assert_eq!(
            std::fs::read(instance.join("mods/b.jar")).expect("b"),
            b"bbbb",
            "a failure in the middle must not stop the files after it"
        );
        assert!(!instance.join("mods/missing.jar").exists());
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[tokio::test]
    async fn a_second_run_skips_the_files_already_downloaded() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let root = std::env::temp_dir().join(format!("tmcl-resume-{}", uuid::Uuid::new_v4()));
        let instance = root.join("instance");
        std::fs::create_dir_all(instance.join("mods")).expect("mods dir");
        // The first run would have left this file behind; its size matches, so
        // the second run must not touch the network for it.
        std::fs::write(instance.join("mods/a.jar"), b"aaaa").expect("a");
        let running = std::sync::Arc::new(AtomicBool::new(true));
        let (base, server) = fixture_server(running.clone());

        let spec = crate::core::task::ModpackFilesSpec {
            instance_id: uuid::Uuid::new_v4(),
            instance_root: instance.clone(),
            cache_root: root.join("cache"),
            files: vec![
                crate::core::task::ModpackFileSpec {
                    path: "mods/a.jar".into(),
                    urls: vec![format!("{base}/a.jar")],
                    sha1: None,
                    size: Some(4),
                },
                crate::core::task::ModpackFileSpec {
                    path: "mods/b.jar".into(),
                    urls: vec![format!("{base}/b.jar")],
                    sha1: None,
                    size: Some(4),
                },
            ],
            download_snapshot: crate::core::download::DownloadSourceSnapshot::default(),
        };
        super::install_modpack_files(spec, crate::core::task::TaskContext::for_tests())
            .await
            .expect("second run");
        running.store(false, Ordering::Relaxed);
        server.join().expect("join server");
        assert_eq!(
            std::fs::read(instance.join("mods/b.jar")).expect("b"),
            b"bbbb"
        );
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn modpack_extraction_keeps_the_destination_instance_metadata() {
        let root = std::env::temp_dir().join(format!("tmcl-modpack-id-{}", uuid::Uuid::new_v4()));
        let archive = root.join("pack.zip");
        let target = root.join("target");
        std::fs::create_dir_all(&target).expect("create target");
        std::fs::write(target.join("tmcl.json"), b"keep").expect("write instance metadata");
        let file = std::fs::File::create(&archive).expect("create archive");
        let mut writer = zip::ZipWriter::new(file);
        for (name, contents) in [
            ("tmcl.json", b"packed".as_slice()),
            ("mods/alpha.jar", b"jar".as_slice()),
        ] {
            writer
                .start_file(name, zip::write::FileOptions::<()>::default())
                .expect("start file");
            use std::io::Write;
            writer.write_all(contents).expect("write file");
        }
        writer.finish().expect("finish archive");

        extract_modpack_archive(&archive, &target).expect("extract archive");
        assert_eq!(
            std::fs::read(target.join("tmcl.json")).expect("read metadata"),
            b"keep",
            "a pack must not overwrite the identity of its destination"
        );
        assert_eq!(
            std::fs::read(target.join("mods/alpha.jar")).expect("read mod"),
            b"jar"
        );
        std::fs::remove_dir_all(root).expect("clean archive");
    }
    #[test]
    fn modpack_file_paths_must_stay_inside_the_instance() {
        assert!(safe_modpack_path("mods/example.jar").is_ok());
        assert!(safe_modpack_path("../escape.jar").is_err());
        assert!(safe_modpack_path("/absolute.jar").is_err());
        assert!(safe_modpack_path("mods\\..\\escape.jar").is_err());
    }
}
