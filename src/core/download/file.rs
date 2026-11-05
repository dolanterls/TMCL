use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use reqwest::{Response, StatusCode, header::CONTENT_RANGE};
use tokio::io::AsyncWriteExt;
use tokio::sync::watch;
use uuid::Uuid;

use crate::core::config::storage::replace_path;
use crate::core::download::hash::verify_sha1;
use crate::core::download::{DownloadAuthentication, DownloadSourceSnapshot};
use crate::core::error::{Error, Result};
use crate::core::task::DownloadTaskControl;
use crate::core::version::http::HttpClient;

const DOWNLOAD_CANCEL_POLL: Duration = Duration::from_millis(100);

enum ChunkRead<T> {
    PauseChanged(bool),
    CancelPoll,
    IdleTimeout,
    Body(std::result::Result<Option<T>, reqwest::Error>),
}

#[derive(Clone, Copy)]
struct ContentRange {
    start: u64,
    end: u64,
    total: Option<u64>,
}

impl ContentRange {
    fn is_valid(self) -> bool {
        self.end >= self.start
            && self
                .end
                .checked_sub(self.start)
                .and_then(|length| length.checked_add(1))
                .is_some()
            && self.total.is_none_or(|total| self.end < total)
    }

    fn length(self) -> Option<u64> {
        self.end.checked_sub(self.start)?.checked_add(1)
    }
}

pub async fn download_to_file(
    client: &HttpClient,
    url: &str,
    target: &Path,
    expected_sha1: Option<&str>,
    cancelled: &AtomicBool,
) -> Result<()> {
    download_to_file_with_policy(
        client,
        url,
        target,
        expected_sha1,
        cancelled,
        &DownloadSourceSnapshot::default(),
    )
    .await
}

pub async fn download_to_file_with_progress<F>(
    client: &HttpClient,
    url: &str,
    target: &Path,
    expected_sha1: Option<&str>,
    cancelled: &AtomicBool,
    progress: F,
) -> Result<()>
where
    F: FnMut(u64, Option<u64>) -> Result<()>,
{
    download_to_file_with_progress_and_policy(
        client,
        url,
        target,
        expected_sha1,
        cancelled,
        &DownloadSourceSnapshot::default(),
        progress,
    )
    .await
}

pub async fn download_to_file_with_policy(
    client: &HttpClient,
    url: &str,
    target: &Path,
    expected_sha1: Option<&str>,
    cancelled: &AtomicBool,
    policy: &DownloadSourceSnapshot,
) -> Result<()> {
    download_to_file_with_progress_and_policy(
        client,
        url,
        target,
        expected_sha1,
        cancelled,
        policy,
        |_, _| Ok(()),
    )
    .await
}

pub async fn download_to_file_with_progress_and_policy<F>(
    client: &HttpClient,
    url: &str,
    target: &Path,
    expected_sha1: Option<&str>,
    cancelled: &AtomicBool,
    policy: &DownloadSourceSnapshot,
    progress: F,
) -> Result<()>
where
    F: FnMut(u64, Option<u64>) -> Result<()>,
{
    download_to_file_inner(
        client,
        url,
        target,
        expected_sha1,
        cancelled,
        None,
        Duration::from_secs(policy.timeout_secs.max(1)),
        progress,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn download_to_file_inner<F>(
    client: &HttpClient,
    url: &str,
    target: &Path,
    expected_sha1: Option<&str>,
    cancelled: &AtomicBool,
    control: Option<&dyn DownloadTaskControl>,
    read_timeout: Duration,
    mut progress: F,
) -> Result<()>
where
    F: FnMut(u64, Option<u64>) -> Result<()>,
{
    let mut paused = control.map(DownloadTaskControl::pause_receiver);
    wait_until_resumed(&mut paused, cancelled).await?;
    if cancelled.load(Ordering::Relaxed) {
        return Err(Error::Download("download cancelled".into()));
    }
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let temporary = temporary_path(target, control.and_then(DownloadTaskControl::task_id));
    let result = download_to_temporary(
        client,
        url,
        &temporary,
        expected_sha1,
        cancelled,
        &mut paused,
        read_timeout,
        &mut progress,
    )
    .await;
    if let Err(error) = result {
        if cancelled.load(Ordering::Relaxed) || !error.is_retryable() {
            let _ = tokio::fs::remove_file(&temporary).await;
        } else if temporary.is_file()
            && let Some(control) = control
        {
            control.record_downloaded_file(&temporary)?;
        }
        return Err(error);
    }
    if let Err(error) = verify_sha1(&temporary, expected_sha1) {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error);
    }
    let target_existed = target.exists();
    replace_path(&temporary, target).map_err(Error::from)?;
    if !target_existed && let Some(control) = control {
        control.record_downloaded_file(target)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn download_to_temporary(
    client: &HttpClient,
    url: &str,
    temporary: &Path,
    expected_sha1: Option<&str>,
    cancelled: &AtomicBool,
    paused: &mut Option<watch::Receiver<bool>>,
    read_timeout: Duration,
    progress: &mut impl FnMut(u64, Option<u64>) -> Result<()>,
) -> Result<()> {
    wait_until_resumed(paused, cancelled).await?;
    if expected_sha1.is_some() && verify_sha1(temporary, expected_sha1).is_ok() {
        let completed = tokio::fs::metadata(temporary).await?.len();
        progress(completed, Some(completed))?;
        return Ok(());
    }

    let mut offset = match tokio::fs::metadata(temporary).await {
        Ok(metadata) => metadata.len(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => return Err(error.into()),
    };
    let mut response = client
        .get_download_response(url, offset, None, DownloadAuthentication::None)
        .await?;
    if offset > 0 {
        match response.status() {
            StatusCode::PARTIAL_CONTENT
                if response_range(&response)
                    .is_some_and(|range| range.is_valid() && range.start == offset) => {}
            StatusCode::OK => offset = 0,
            StatusCode::RANGE_NOT_SATISFIABLE => {
                if expected_sha1.is_some() && verify_sha1(temporary, expected_sha1).is_ok() {
                    let completed = tokio::fs::metadata(temporary).await?.len();
                    progress(completed, Some(completed))?;
                    return Ok(());
                }
                tokio::fs::remove_file(temporary).await?;
                offset = 0;
                response = client
                    .get_download_response(url, 0, None, DownloadAuthentication::None)
                    .await?;
            }
            StatusCode::PARTIAL_CONTENT => {
                tokio::fs::remove_file(temporary).await?;
                offset = 0;
                response = client
                    .get_download_response(url, 0, None, DownloadAuthentication::None)
                    .await?;
            }
            status if !status.is_success() => return Err(http_status_error(url, status)),
            status => {
                return Err(Error::Network(format!(
                    "GET {url}: unexpected status {status} while resuming download"
                )));
            }
        }
    } else if response.status() == StatusCode::PARTIAL_CONTENT
        && !response_range(&response).is_some_and(|range| range.is_valid() && range.start == 0)
    {
        return Err(Error::Network(format!(
            "GET {url}: server returned an invalid Content-Range"
        )));
    }
    if !response.status().is_success() {
        return Err(http_status_error(url, response.status()));
    }

    let response_range = (response.status() == StatusCode::PARTIAL_CONTENT)
        .then(|| response_range(&response))
        .flatten();
    let content_length = response.content_length();
    let range_length = response_range.and_then(ContentRange::length);
    if content_length
        .zip(range_length)
        .is_some_and(|(body, range)| body != range)
    {
        return Err(Error::Network(format!(
            "GET {url}: Content-Length does not match Content-Range"
        )));
    }
    let expected_body_length = content_length.or(range_length);
    let total = response_range.and_then(|range| range.total).or_else(|| {
        response
            .content_length()
            .map(|remaining| offset.saturating_add(remaining))
    });
    let mut file = if offset == 0 {
        tokio::fs::File::create(temporary).await?
    } else {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(temporary)
            .await?
    };
    let mut completed = offset;
    if completed > 0 {
        progress(completed, total)?;
    }
    let mut idle_deadline = tokio::time::Instant::now() + read_timeout;

    loop {
        let event = if let Some(pause_receiver) = paused.as_mut() {
            tokio::select! {
                biased;
                changed = pause_receiver.changed() => ChunkRead::PauseChanged(changed.is_ok()),
                _ = tokio::time::sleep_until(idle_deadline) => ChunkRead::IdleTimeout,
                _ = tokio::time::sleep(DOWNLOAD_CANCEL_POLL) => ChunkRead::CancelPoll,
                chunk = response.chunk() => ChunkRead::Body(chunk),
            }
        } else {
            tokio::select! {
                _ = tokio::time::sleep_until(idle_deadline) => ChunkRead::IdleTimeout,
                _ = tokio::time::sleep(DOWNLOAD_CANCEL_POLL) => ChunkRead::CancelPoll,
                chunk = response.chunk() => ChunkRead::Body(chunk),
            }
        };
        let next = match event {
            ChunkRead::PauseChanged(false) => {
                return Err(Error::Download("task pause control closed".into()));
            }
            ChunkRead::PauseChanged(true) => {
                wait_until_resumed(paused, cancelled).await?;
                idle_deadline = tokio::time::Instant::now() + read_timeout;
                continue;
            }
            ChunkRead::IdleTimeout => {
                file.flush().await?;
                file.sync_all().await?;
                return Err(Error::Network(format!(
                    "read {url}: no response data received for {} seconds",
                    read_timeout.as_secs()
                )));
            }
            ChunkRead::CancelPoll => {
                if cancelled.load(Ordering::Relaxed) {
                    return Err(Error::Download("download cancelled".into()));
                }
                continue;
            }
            ChunkRead::Body(Err(error)) => {
                file.flush().await?;
                file.sync_all().await?;
                return Err(Error::Network(format!("read {url}: {error}")));
            }
            ChunkRead::Body(Ok(chunk)) => {
                idle_deadline = tokio::time::Instant::now() + read_timeout;
                chunk
            }
        };
        wait_until_resumed(paused, cancelled).await?;
        if cancelled.load(Ordering::Relaxed) {
            return Err(Error::Download("download cancelled".into()));
        }
        let Some(chunk) = next else { break };
        file.write_all(&chunk).await?;
        completed = completed.saturating_add(chunk.len() as u64);
        progress(completed, total)?;
    }
    file.flush().await?;
    file.sync_all().await?;
    drop(file);
    if expected_body_length.is_some_and(|length| length != completed.saturating_sub(offset))
        || total.is_some_and(|total| total != completed)
    {
        let expected = total
            .filter(|expected| *expected != completed)
            .or_else(|| expected_body_length.map(|length| length.saturating_add(offset)))
            .expect("an incomplete response has an expected length");
        return Err(Error::Network(format!(
            "read {url}: incomplete response body (expected {expected} bytes, received {completed})"
        )));
    }
    Ok(())
}

async fn wait_until_resumed(
    paused: &mut Option<watch::Receiver<bool>>,
    cancelled: &AtomicBool,
) -> Result<()> {
    let Some(paused) = paused else {
        return if cancelled.load(Ordering::Relaxed) {
            Err(Error::Download("download cancelled".into()))
        } else {
            Ok(())
        };
    };
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

fn temporary_path(target: &Path, task_id: Option<Uuid>) -> PathBuf {
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("download");
    match task_id {
        Some(id) => target.with_file_name(format!(".{name}.part-{id}")),
        None => target.with_file_name(format!(".{name}.part")),
    }
}

fn response_range(response: &Response) -> Option<ContentRange> {
    response
        .headers()
        .get(CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            let (range, total) = value.strip_prefix("bytes ")?.split_once('/')?;
            let (start, end) = range.split_once('-')?;
            Some(ContentRange {
                start: start.parse().ok()?,
                end: end.parse().ok()?,
                total: if total == "*" {
                    None
                } else {
                    Some(total.parse().ok()?)
                },
            })
        })
}

fn http_status_error(url: &str, status: StatusCode) -> Error {
    Error::HttpStatus {
        status: status.as_u16(),
        request: format!("GET {url}"),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::{download_to_file, download_to_file_with_progress};
    use crate::core::version::http::HttpClient;

    #[tokio::test]
    async fn cancellation_before_request_does_not_create_partial_file() {
        let root =
            std::env::temp_dir().join(format!("tmcl-download-test-{}", uuid::Uuid::new_v4()));
        let target = root.join("file.bin");
        let cancelled = AtomicBool::new(true);
        let client = HttpClient::new().expect("HTTP client");

        let error = download_to_file(
            &client,
            "http://127.0.0.1:1/never",
            &target,
            None,
            &cancelled,
        )
        .await
        .expect_err("cancelled download");

        assert!(error.to_string().contains("cancelled"));
        assert!(!target.exists());
        assert!(cancelled.load(Ordering::Relaxed));
        assert!(!root.exists());
    }

    #[tokio::test]
    async fn resumes_a_partial_download_after_the_response_body_is_truncated() {
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpListener;

        let root =
            std::env::temp_dir().join(format!("tmcl-download-resume-{}", uuid::Uuid::new_v4()));
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind fixture");
        let address = listener.local_addr().expect("fixture address");
        let server = tokio::spawn(async move {
            let (mut first, _) = listener.accept().await.expect("accept first request");
            let _ = read_http_request(&mut first).await;
            first
                .write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-length: 10\r\nconnection: close\r\n\r\nabcd",
                )
                .await
                .expect("write truncated body");
            first.shutdown().await.expect("close truncated response");

            let (mut resumed, _) = listener.accept().await.expect("accept resumed request");
            let request = read_http_request(&mut resumed).await;
            assert!(
                request.to_ascii_lowercase().contains("range: bytes=4-"),
                "retry should request only the missing suffix; got {request:?}"
            );
            resumed
                .write_all(
                    b"HTTP/1.1 206 Partial Content\r\ncontent-length: 6\r\ncontent-range: bytes 4-9/10\r\nconnection: close\r\n\r\nefghij",
                )
                .await
                .expect("write resumed body");
        });

        let client = HttpClient::new().expect("HTTP client");
        let target = root.join("download.bin");
        let cancelled = AtomicBool::new(false);
        let url = format!("http://{address}/download");
        let first_error =
            download_to_file_with_progress(&client, &url, &target, None, &cancelled, |_, _| Ok(()))
                .await
                .expect_err("truncated response should fail");
        assert!(first_error.to_string().contains("read"));
        let partial = target.with_file_name(".download.bin.part");
        assert_eq!(
            std::fs::read(&partial).expect("partial file should be kept"),
            b"abcd"
        );

        let mut resumed_progress = Vec::new();
        download_to_file_with_progress(
            &client,
            &url,
            &target,
            None,
            &cancelled,
            |completed, total| {
                resumed_progress.push((completed, total));
                Ok(())
            },
        )
        .await
        .expect("resume partial download");
        server.await.expect("serve both requests");

        assert_eq!(
            std::fs::read(&target).expect("read completed download"),
            b"abcdefghij"
        );
        assert_eq!(resumed_progress.first().map(|entry| entry.0), Some(4));
        assert_eq!(resumed_progress.last().map(|entry| entry.0), Some(10));
        assert!(!partial.exists());
        std::fs::remove_dir_all(root).expect("clean resume fixture");
    }

    #[tokio::test]
    async fn restarts_from_zero_when_a_server_ignores_the_range_request() {
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpListener;

        let root = std::env::temp_dir().join(format!(
            "tmcl-download-ignore-range-{}",
            uuid::Uuid::new_v4()
        ));
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind fixture");
        let address = listener.local_addr().expect("fixture address");
        let server = tokio::spawn(async move {
            let (mut first, _) = listener.accept().await.expect("accept first request");
            let _ = read_http_request(&mut first).await;
            first
                .write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-length: 10\r\nconnection: close\r\n\r\nabcd",
                )
                .await
                .expect("write truncated body");
            first.shutdown().await.expect("close truncated response");

            let (mut restarted, _) = listener.accept().await.expect("accept second request");
            let request = read_http_request(&mut restarted).await;
            assert!(request.to_ascii_lowercase().contains("range: bytes=4-"));
            restarted
                .write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-length: 10\r\nconnection: close\r\n\r\nabcdefghij",
                )
                .await
                .expect("write full body after ignoring Range");
        });

        let client = HttpClient::new().expect("HTTP client");
        let target = root.join("download.bin");
        let cancelled = AtomicBool::new(false);
        download_to_file(
            &client,
            &format!("http://{address}/download"),
            &target,
            None,
            &cancelled,
        )
        .await
        .expect_err("first response should be truncated");
        download_to_file(
            &client,
            &format!("http://{address}/download"),
            &target,
            None,
            &cancelled,
        )
        .await
        .expect("restart cleanly when Range is ignored");
        server.await.expect("serve both requests");

        assert_eq!(
            std::fs::read(&target).expect("read completed download"),
            b"abcdefghij"
        );
        std::fs::remove_dir_all(root).expect("clean ignored-range fixture");
    }

    #[tokio::test]
    async fn large_download_is_not_limited_by_metadata_request_timeout() {
        use std::time::Duration;
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpListener;

        let root = std::env::temp_dir().join(format!(
            "tmcl-download-long-response-{}",
            uuid::Uuid::new_v4()
        ));
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind fixture");
        let address = listener.local_addr().expect("fixture address");
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept request");
            let _ = read_http_request(&mut stream).await;
            stream
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 3\r\nconnection: close\r\n\r\na")
                .await
                .expect("write first byte");
            stream.flush().await.expect("flush first byte");
            tokio::time::sleep(Duration::from_millis(120)).await;
            stream.write_all(b"b").await.expect("write second byte");
            tokio::time::sleep(Duration::from_millis(120)).await;
            stream.write_all(b"c").await.expect("write final byte");
        });

        let client =
            HttpClient::new_with_timeouts(Duration::from_millis(50), Duration::from_secs(2))
                .expect("HTTP client");
        let target = root.join("download.bin");
        let cancelled = AtomicBool::new(false);
        download_to_file(
            &client,
            &format!("http://{address}/download"),
            &target,
            None,
            &cancelled,
        )
        .await
        .expect("streaming download should outlive metadata timeout");
        server.await.expect("fixture server");

        assert_eq!(
            std::fs::read(&target).expect("read completed download"),
            b"abc"
        );
        std::fs::remove_dir_all(root).expect("clean long-response fixture");
    }

    async fn read_http_request(stream: &mut tokio::net::TcpStream) -> String {
        use tokio::io::AsyncReadExt;

        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = stream.read(&mut buffer).await.expect("read request");
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
        }
        String::from_utf8_lossy(&request).into_owned()
    }
}
