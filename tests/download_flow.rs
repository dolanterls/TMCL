use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::thread::{self, JoinHandle};

use sha1::Digest;
use tmcl::core::download::DownloadSourceSnapshot;
use tmcl::core::download::{
    CachePolicy, DownloadRequest, IntegrityKind, download_request, sha1_file, verify_sha1,
};
use tmcl::core::java::runtime::{
    RuntimeArtifact, RuntimeDownloads, RuntimeFile, RuntimeManifest, install_runtime_from_manifest,
};
use tmcl::core::version::http::HttpClient;

struct FixtureResponse {
    status: u16,
    headers: Vec<(&'static str, &'static str)>,
    body: Vec<u8>,
}

fn fixture_response(status: u16, body: impl Into<Vec<u8>>) -> FixtureResponse {
    FixtureResponse {
        status,
        headers: Vec::new(),
        body: body.into(),
    }
}

fn spawn_fixture(responses: Vec<FixtureResponse>) -> (String, Arc<AtomicUsize>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind HTTP fixture");
    let address = listener.local_addr().expect("fixture address");
    let count = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&count);
    let thread = thread::spawn(move || {
        for response in responses {
            let (mut stream, _) = listener.accept().expect("accept fixture request");
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request).expect("read fixture request");
            observed.fetch_add(1, Ordering::SeqCst);
            let status_text = match response.status {
                200 => "OK",
                404 => "Not Found",
                500 => "Internal Server Error",
                503 => "Service Unavailable",
                _ => "Fixture",
            };
            write!(
                stream,
                "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n",
                response.status,
                status_text,
                response.body.len()
            )
            .expect("write fixture headers");
            for (name, value) in response.headers {
                write!(stream, "{name}: {value}\r\n").expect("write fixture header");
            }
            write!(stream, "\r\n").expect("finish fixture headers");
            stream
                .write_all(&response.body)
                .expect("write fixture body");
        }
    });
    (format!("http://{address}/artifact"), count, thread)
}

fn temp_root() -> PathBuf {
    std::env::temp_dir().join(format!("tmcl-download-flow-{}", uuid::Uuid::new_v4()))
}

#[test]
fn hash_boundaries_work_without_network_access() {
    let root = temp_root();
    std::fs::create_dir_all(&root).expect("download fixture");
    let path = root.join("artifact.bin");
    std::fs::write(&path, b"tmcl").expect("artifact");
    let hash = sha1_file(&path).expect("hash artifact");
    assert!(verify_sha1(&path, Some(&hash)).is_ok());
    assert!(verify_sha1(&path, Some("bad")).is_err());

    std::fs::remove_dir_all(root).expect("clean download flow");
}

#[tokio::test]
async fn candidate_failure_switches_to_the_next_url_and_commits_atomically() {
    let (failed_url, failed_count, failed_server) =
        spawn_fixture(vec![fixture_response(404, b"missing".to_vec())]);
    let (success_url, success_count, success_server) =
        spawn_fixture(vec![fixture_response(200, b"artifact".to_vec())]);
    let root = temp_root();
    let destination = root.join("artifact.bin");
    let cache = root.join("cache");
    let body = b"artifact";
    let sha1 = format!("{:x}", sha1::Sha1::digest(body));
    let request = DownloadRequest::new(
        vec![failed_url, success_url],
        &destination,
        Some(sha1),
        Some(body.len() as u64),
        "fixture artifact",
    );

    let outcome = download_request(
        &HttpClient::new().expect("HTTP client"),
        &request,
        &cache,
        &std::sync::atomic::AtomicBool::new(false),
        None,
        |_, _| Ok(()),
    )
    .await
    .expect("fallback download");

    assert!(outcome.downloaded);
    assert_eq!(std::fs::read(&destination).expect("destination"), body);
    assert_eq!(failed_count.load(Ordering::SeqCst), 1);
    assert_eq!(success_count.load(Ordering::SeqCst), 1);
    failed_server.join().expect("failed fixture server");
    success_server.join().expect("success fixture server");
    std::fs::remove_dir_all(root).expect("clean fallback fixture");
}

#[tokio::test]
async fn retryable_status_is_retried_five_times_before_success() {
    let mut responses = (0..4)
        .map(|_| fixture_response(503, b"retry".to_vec()))
        .collect::<Vec<_>>();
    responses.push(fixture_response(200, b"eventual".to_vec()));
    let (url, count, server) = spawn_fixture(responses);
    let root = temp_root();
    let request = DownloadRequest::new(
        vec![url],
        root.join("artifact.bin"),
        None,
        Some(8),
        "retry fixture",
    );

    download_request(
        &HttpClient::new().expect("HTTP client"),
        &request,
        &root.join("cache"),
        &std::sync::atomic::AtomicBool::new(false),
        None,
        |_, _| Ok(()),
    )
    .await
    .expect("retryable fixture download");

    assert_eq!(count.load(Ordering::SeqCst), 5);
    server.join().expect("retry fixture server");
    std::fs::remove_dir_all(root).expect("clean retry fixture");
}

#[tokio::test]
async fn etag_cache_hit_avoids_a_second_network_request() {
    let (url, count, server) = spawn_fixture(vec![FixtureResponse {
        status: 200,
        headers: vec![("ETag", "\"fixture-v1\""), ("Accept-Ranges", "bytes")],
        body: b"cached".to_vec(),
    }]);
    let root = temp_root();
    let client = HttpClient::new().expect("HTTP client");
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    let first = DownloadRequest::new(
        vec![url.clone()],
        root.join("first.bin"),
        None,
        None,
        "etag fixture",
    )
    .with_integrity(IntegrityKind::File)
    .with_cache_policy(CachePolicy::Etag);
    let second = DownloadRequest::new(
        vec![url],
        root.join("second.bin"),
        None,
        None,
        "etag fixture",
    )
    .with_cache_policy(CachePolicy::Etag);

    download_request(
        &client,
        &first,
        &root.join("cache"),
        &cancelled,
        None,
        |_, _| Ok(()),
    )
    .await
    .expect("first ETag download");
    let second_outcome = download_request(
        &client,
        &second,
        &root.join("cache"),
        &cancelled,
        None,
        |_, _| Ok(()),
    )
    .await
    .expect("ETag cache hit");

    assert!(!second_outcome.downloaded);
    assert_eq!(
        std::fs::read(root.join("second.bin")).expect("cached destination"),
        b"cached"
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
    server.join().expect("ETag fixture server");
    std::fs::remove_dir_all(root).expect("clean ETag fixture");
}

#[tokio::test]
async fn bmclapi_hash_rejects_a_body_with_the_wrong_content() {
    let (url, count, server) = spawn_fixture(vec![FixtureResponse {
        status: 200,
        headers: vec![("x-bmclapi-hash", "c42b2247c7467caed77ee71cc67a234c327aa9d3")],
        body: b"wrong artifact".to_vec(),
    }]);
    let root = temp_root();
    let destination = root.join("artifact.bin");
    let request = DownloadRequest::new(vec![url], &destination, None, None, "BMCLAPI hash fixture")
        .with_cache_policy(CachePolicy::Etag);

    let result = download_request(
        &HttpClient::new().expect("HTTP client"),
        &request,
        &root.join("cache"),
        &std::sync::atomic::AtomicBool::new(false),
        None,
        |_, _| Ok(()),
    )
    .await;

    assert!(result.is_err());
    assert!(!destination.exists());
    assert_eq!(count.load(Ordering::SeqCst), 1);
    server.join().expect("BMCLAPI hash fixture server");
    std::fs::remove_dir_all(root).expect("clean BMCLAPI hash fixture");
}

#[tokio::test]
async fn profile_hash_replaces_an_existing_corrupt_jar() {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file(
            "net/neoforged/installertools/ConsoleTool.class",
            zip::write::SimpleFileOptions::default(),
        )
        .expect("start JAR entry");
    writer
        .write_all(b"valid class bytes")
        .expect("write JAR entry");
    let valid_jar = writer.finish().expect("finish JAR").into_inner();
    let mut corrupt_jar = valid_jar.clone();
    let local_header = corrupt_jar
        .windows(4)
        .position(|window| window == b"PK\x03\x04")
        .expect("local JAR header");
    let name_length = u16::from_le_bytes([
        corrupt_jar[local_header + 26],
        corrupt_jar[local_header + 27],
    ]) as usize;
    let data_offset = local_header + 30 + name_length;
    corrupt_jar[data_offset] ^= 0xff;

    let root = temp_root();
    let corrupt_path = root.join("corrupt.jar");
    std::fs::create_dir_all(&root).expect("create fixture root");
    std::fs::write(&corrupt_path, &corrupt_jar).expect("write corrupt JAR");
    assert!(
        tmcl::core::download::verify_download(&corrupt_path, None, None, IntegrityKind::ZipJar,)
            .is_ok()
    );

    let sha1 = format!("{:x}", sha1::Sha1::digest(&valid_jar));
    let (url, count, server) = spawn_fixture(vec![FixtureResponse {
        status: 200,
        headers: Vec::new(),
        body: valid_jar.clone(),
    }]);
    let destination = root.join("installertools-4.0.17-fatjar.jar");
    std::fs::write(&destination, &corrupt_jar).expect("write existing corrupt JAR");
    let request = DownloadRequest::new(
        vec![url],
        &destination,
        Some(sha1),
        Some(valid_jar.len() as u64),
        "NeoForge installertools",
    )
    .with_integrity(IntegrityKind::ZipJar)
    .with_cache_policy(CachePolicy::ContentAddressed);

    download_request(
        &HttpClient::new().expect("HTTP client"),
        &request,
        &root.join("cache"),
        &std::sync::atomic::AtomicBool::new(false),
        None,
        |_, _| Ok(()),
    )
    .await
    .expect("replace corrupt JAR");

    assert_eq!(std::fs::read(&destination).expect("read JAR"), valid_jar);
    assert_eq!(count.load(Ordering::SeqCst), 1);
    server.join().expect("profile hash fixture server");
    std::fs::remove_dir_all(root).expect("clean profile hash fixture");
}

#[tokio::test]
async fn java_lzma_fixture_is_verified_decompressed_and_committed() {
    let raw = b"runtime-java-fixture".to_vec();
    let mut compressed = Vec::new();
    lzma_rs::lzma_compress(&mut std::io::Cursor::new(&raw), &mut compressed)
        .expect("compress runtime fixture");
    let compressed_sha1 = format!("{:x}", sha1::Sha1::digest(&compressed));
    let (url, _, server) = spawn_fixture(vec![fixture_response(200, compressed.clone())]);
    let root = temp_root();
    let relative = if cfg!(windows) {
        "bin/java.exe"
    } else {
        "bin/java"
    };
    let manifest = RuntimeManifest {
        files: BTreeMap::from([(
            relative.into(),
            RuntimeFile {
                kind: "file".into(),
                downloads: RuntimeDownloads {
                    raw: None,
                    lzma: Some(RuntimeArtifact {
                        url,
                        sha1: compressed_sha1,
                        size: Some(compressed.len() as u64),
                    }),
                },
                executable: true,
                target: None,
            },
        )]),
    };
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    let result = install_runtime_from_manifest(
        &HttpClient::new().expect("HTTP client"),
        &root,
        21,
        &DownloadSourceSnapshot::new(
            tmcl::core::download::DownloadSourceMode::Official,
            tmcl::core::download::DownloadSourceMode::Official,
        ),
        &manifest,
        &root.join("cache"),
        &cancelled,
        None,
        |_, _| Ok(()),
    )
    .await
    .expect("install Java lzma fixture");

    server.join().expect("Java fixture server");
    assert_eq!(
        result.executable,
        root.join("runtime/java-21").join(relative)
    );
    assert_eq!(
        std::fs::read(&result.executable).expect("read Java fixture"),
        raw
    );
    std::fs::remove_dir_all(root).expect("clean Java fixture");
}
