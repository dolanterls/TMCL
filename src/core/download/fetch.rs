use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use reqwest::{Response, StatusCode, header};
use sha1::{Digest, Sha1};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::core::config::storage::replace_path;
use crate::core::error::{Error, Result};
use crate::core::task::{DownloadTaskControl, wait_if_paused};
use crate::core::version::http::HttpClient;

use super::cache::{DownloadCache, EtagHit};
use super::executor::DownloadExecutor;
use super::request::{CachePolicy, DestinationPolicy, DownloadRequest};
use super::verify::{IntegrityKind, verify_download};

/// Runs blocking file work off the async workers.
///
/// Hashing, copying and unpacking are ordinary blocking syscalls, and the
/// runtime that drives them also drives the TUI. Handing them to the blocking
/// pool keeps a 200 MB hash from freezing the frame the player is looking at -
/// the same split HMCL makes by giving every task an I/O executor.
pub(crate) async fn blocking<T, F>(work: F) -> Result<T>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| Error::Download(format!("blocking task failed: {error}")))?
}

const RETRY_DELAY: Duration = Duration::from_millis(250);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(5);

fn retry_delay(attempt: usize) -> Duration {
    let factor = 1u64 << attempt.min(5);
    RETRY_DELAY
        .saturating_mul(factor as u32)
        .min(MAX_RETRY_DELAY)
}
const CANCEL_POLL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadOutcome {
    pub downloaded: bool,
    pub bytes: u64,
    pub cache_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct ResumeContext {
    etag: Option<String>,
    last_modified: Option<String>,
    total: u64,
}

pub async fn download_request<F>(
    client: &HttpClient,
    request: &DownloadRequest,
    cache_root: &Path,
    cancelled: &AtomicBool,
    control: Option<&dyn DownloadTaskControl>,
    mut progress: F,
) -> Result<DownloadOutcome>
where
    F: FnMut(u64, Option<u64>) -> Result<()> + Send,
{
    let executor = DownloadExecutor::global();
    download_request_with_executor(
        client,
        request,
        cache_root,
        &executor,
        cancelled,
        control,
        &mut progress,
    )
    .await
}

pub async fn download_request_with_executor<F>(
    client: &HttpClient,
    request: &DownloadRequest,
    cache_root: &Path,
    executor: &DownloadExecutor,
    cancelled: &AtomicBool,
    control: Option<&dyn DownloadTaskControl>,
    progress: &mut F,
) -> Result<DownloadOutcome>
where
    F: FnMut(u64, Option<u64>) -> Result<()> + Send,
{
    if request.candidate_urls.is_empty() {
        return Err(Error::Download(format!(
            "{} has no candidate URL",
            request.artifact_label
        )));
    }
    for url in &request.candidate_urls {
        validate_url(url)?;
    }
    if let Some(parent) = request.destination.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let effective_cache_root = request.policy.cache_dir.as_deref().unwrap_or(cache_root);
    let cache = Arc::new(DownloadCache::new(effective_cache_root.to_path_buf())?);
    if let Some(outcome) = use_existing(request, &cache, cancelled, control, progress).await? {
        return Ok(outcome);
    }

    // Keep the partial file on the destination volume. The previous code
    // used the system temp directory, so every committed file became a
    // cross-volume MoveFileExW copy; with dozens of concurrent assets that is
    // both slow and prone to transient sharing violations on Windows.
    let temporary = request
        .destination
        .with_file_name(format!(".tmcl-{}.part", Uuid::new_v4()));
    let mut errors = Vec::new();
    let mut last_status = None;
    let mut has_retryable_error = false;
    let mut final_result = None;
    for url in &request.candidate_urls {
        let mut resume = None;
        let mut candidate_error = None;
        let attempts = usize::from(request.policy.retry_count).saturating_add(1);
        for attempt in 0..attempts {
            if cancelled.load(Ordering::Relaxed) {
                break;
            }
            if let Some(hit) = etag_cache_hit(request, &cache, url).await? {
                copy_to_destination(&hit.path, &request.destination, request.destination_policy)
                    .await?;
                let bytes = hit.size;
                progress(bytes, Some(bytes))?;
                final_result = Some(DownloadOutcome {
                    downloaded: false,
                    bytes,
                    cache_path: Some(hit.path),
                });
                break;
            }

            let _permit = executor
                .acquire()
                .await
                .map_err(|_| Error::Download("download executor closed".into()))?;
            match download_attempt(
                client,
                request,
                url,
                &temporary,
                &cache,
                &mut resume,
                cancelled,
                control,
                progress,
            )
            .await
            {
                Ok(outcome) => {
                    super::resolver::record_success(url);
                    final_result = Some(outcome);
                    break;
                }
                Err(error) => {
                    if let Error::HttpStatus { status, request } = &error {
                        last_status = Some((*status, request.clone()));
                    }
                    candidate_error = Some(error);
                    has_retryable_error |=
                        candidate_error.as_ref().is_some_and(Error::is_retryable);
                    let retryable = candidate_error.as_ref().is_some_and(Error::is_retryable);
                    if !retryable || is_client_error(candidate_error.as_ref().expect("error")) {
                        break;
                    }
                    if attempt + 1 < attempts {
                        tokio::time::sleep(retry_delay(attempt)).await;
                    }
                }
            }
        }
        if final_result.is_some() {
            break;
        }
        if candidate_error
            .as_ref()
            .is_some_and(|error| error.is_retryable())
        {
            // Only network trouble counts against a host; a 404 for one file
            // says nothing about the host serving the next one.
            super::resolver::record_failure(url);
        }
        if let Some(error) = candidate_error {
            errors.push(format!("{url}: {error}"));
        }
        remove_if_exists(&temporary).await?;
    }

    remove_if_exists(&temporary).await?;
    if let Some(result) = final_result {
        Ok(result)
    } else if cancelled.load(Ordering::Relaxed) {
        Err(Error::Download(format!(
            "{} cancelled",
            request.artifact_label
        )))
    } else if !has_retryable_error {
        if let Some((status, request_name)) = last_status {
            Err(Error::HttpStatus {
                status,
                request: request_name,
            })
        } else {
            Err(Error::Network(format!(
                "{} failed for all candidates: {}",
                request.artifact_label,
                errors.join("; ")
            )))
        }
    } else {
        Err(Error::Network(format!(
            "{} failed for all candidates: {}",
            request.artifact_label,
            errors.join("; ")
        )))
    }
}

async fn use_existing<F>(
    request: &DownloadRequest,
    cache: &DownloadCache,
    _cancelled: &AtomicBool,
    control: Option<&dyn DownloadTaskControl>,
    progress: &mut F,
) -> Result<Option<DownloadOutcome>>
where
    F: FnMut(u64, Option<u64>) -> Result<()> + Send,
{
    if request.destination_policy == DestinationPolicy::NoReplace && request.destination.exists() {
        return Err(Error::Download(format!(
            "destination already exists: {}",
            request.destination.display()
        )));
    }
    if request.destination.is_file() {
        let path = request.destination.clone();
        let sha1 = request.expected_sha1.clone();
        let size = request.expected_size;
        let integrity = request.integrity;
        let verified = blocking(move || verify_download(&path, sha1.as_deref(), size, integrity))
            .await
            .is_ok();
        if verified {
            let bytes = std::fs::metadata(&request.destination)?.len();
            if let Some(control) = control {
                control.record_file_reference(&request.destination)?;
            }
            progress(bytes, request.expected_size.or(Some(bytes)))?;
            return Ok(Some(DownloadOutcome {
                downloaded: false,
                bytes,
                cache_path: None,
            }));
        }
    }
    if let Some(sha1) = request.expected_sha1.as_deref()
        && request.cache_policy == CachePolicy::ContentAddressed
        && let Some(path) = cache.lookup_sha1(sha1)?
    {
        copy_to_destination(&path, &request.destination, request.destination_policy).await?;
        let bytes = std::fs::metadata(&request.destination)?.len();
        if let Some(control) = control {
            control.record_file_reference(&path)?;
        }
        progress(bytes, request.expected_size.or(Some(bytes)))?;
        return Ok(Some(DownloadOutcome {
            downloaded: false,
            bytes,
            cache_path: Some(path),
        }));
    }
    Ok(None)
}

async fn etag_cache_hit(
    request: &DownloadRequest,
    cache: &DownloadCache,
    url: &str,
) -> Result<Option<EtagHit>> {
    if request.cache_policy != CachePolicy::Etag || request.expected_sha1.is_some() {
        return Ok(None);
    }
    let Some(hit) = cache.lookup_etag(url)? else {
        return Ok(None);
    };
    if request.expected_size.is_some_and(|size| size != hit.size) {
        cache.remove_etag(url)?;
        return Ok(None);
    }
    let path = hit.path.clone();
    let sha1 = hit.sha1.clone();
    let size = request.expected_size;
    let integrity = request.integrity;
    let intact = blocking(move || verify_download(&path, sha1.as_deref(), size, integrity))
        .await
        .is_ok();
    if !intact {
        cache.remove_etag(url)?;
        return Ok(None);
    }
    Ok(Some(hit))
}

#[allow(clippy::too_many_arguments)]
async fn download_attempt<F>(
    client: &HttpClient,
    request: &DownloadRequest,
    url: &str,
    temporary: &Path,
    cache: &Arc<DownloadCache>,
    resume: &mut Option<ResumeContext>,
    cancelled: &AtomicBool,
    control: Option<&dyn DownloadTaskControl>,
    progress: &mut F,
) -> Result<DownloadOutcome>
where
    F: FnMut(u64, Option<u64>) -> Result<()> + Send,
{
    wait_if_resumed(control, cancelled).await?;
    let mut offset = match tokio::fs::metadata(temporary).await {
        Ok(metadata) => metadata.len(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => return Err(error.into()),
    };
    let mut response = client
        .get_download_response(
            url,
            offset,
            resume
                .as_ref()
                .and_then(|value| value.etag.as_deref().or(value.last_modified.as_deref())),
            request.authentication,
        )
        .await?;
    let mut response_sha1 = bmclapi_sha1(&response);

    if response.status() == StatusCode::NOT_MODIFIED {
        return Err(Error::Network(format!(
            "GET {url}: received 304 without a usable local cache"
        )));
    }
    if response.status() == StatusCode::RANGE_NOT_SATISFIABLE && offset > 0 {
        if verify_download(
            temporary,
            request
                .expected_sha1
                .as_deref()
                .or(response_sha1.as_deref()),
            request.expected_size,
            request.integrity,
        )
        .is_ok()
        {
            let bytes = tokio::fs::metadata(temporary).await?.len();
            commit_temporary(temporary, &request.destination, request.destination_policy)?;
            return Ok(DownloadOutcome {
                downloaded: true,
                bytes,
                cache_path: None,
            });
        }
        remove_if_exists(temporary).await?;
        *resume = None;
        offset = 0;
        response = client
            .get_download_response(url, 0, None, request.authentication)
            .await?;
        response_sha1 = bmclapi_sha1(&response);
    }
    if !response.status().is_success() {
        return Err(http_status_error(url, response.status()));
    }

    if let Some(sha1) = response_sha1.as_deref()
        && request
            .expected_sha1
            .as_deref()
            .is_none_or(|expected| expected.eq_ignore_ascii_case(sha1))
        && let Some(path) = cache.lookup_sha1(sha1)?
        && verify_download(&path, Some(sha1), request.expected_size, request.integrity).is_ok()
    {
        copy_to_destination(&path, &request.destination, request.destination_policy).await?;
        let bytes = std::fs::metadata(&request.destination)?.len();
        if let Some(control) = control {
            control.record_file_reference(&path)?;
        }
        progress(bytes, request.expected_size.or(Some(bytes)))?;
        return Ok(DownloadOutcome {
            downloaded: false,
            bytes,
            cache_path: Some(path),
        });
    }

    let mut append = false;
    if offset > 0 {
        if response.status() == StatusCode::PARTIAL_CONTENT
            && resume
                .as_ref()
                .is_some_and(|context| valid_resume_response(&response, offset, context))
        {
            append = true;
        } else {
            remove_if_exists(temporary).await?;
            *resume = None;
            offset = 0;
            response = client
                .get_download_response(url, 0, None, request.authentication)
                .await?;
            response_sha1 = bmclapi_sha1(&response);
            if !response.status().is_success() {
                return Err(http_status_error(url, response.status()));
            }
        }
    }

    let response_context = (!append).then(|| resume_context(&response)).flatten();
    if !append {
        *resume = response_context.clone();
    }
    let total = if append {
        resume.as_ref().map(|context| context.total)
    } else {
        response.content_length()
    };
    let mut file = if offset == 0 {
        if let Some(parent) = temporary.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::File::create(temporary).await?
    } else {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(temporary)
            .await?
    };
    // Hash what we write instead of re-reading the finished file. A 200 MB
    // jar used to be read once to download it and again to verify it, and a
    // third time to place it in the content-addressed cache.
    let content_sha1 = request
        .expected_sha1
        .as_deref()
        .or(response_sha1.as_deref());
    let mut streamed = (offset == 0 && content_sha1.is_some()).then(Sha1::new);
    let mut completed = offset;
    progress(completed, total.or(request.expected_size))?;
    let read_timeout = Duration::from_secs(request.policy.timeout_secs.max(1));
    let mut idle_deadline = tokio::time::Instant::now() + read_timeout;
    loop {
        wait_if_resumed(control, cancelled).await?;
        let chunk = tokio::select! {
            biased;
            _ = tokio::time::sleep_until(idle_deadline) => {
                return Err(Error::Network(format!("read {url}: response timed out")));
            }
            _ = tokio::time::sleep(CANCEL_POLL) => {
                if cancelled.load(Ordering::Relaxed) {
                    return Err(Error::Download("download cancelled".into()));
                }
                continue;
            }
            chunk = response.chunk() => {
                chunk.map_err(|error| Error::Network(format!("read {url}: {error}")))?
            }
        };
        idle_deadline = tokio::time::Instant::now() + read_timeout;
        let Some(chunk) = chunk else {
            break;
        };
        file.write_all(&chunk).await?;
        if let Some(digest) = streamed.as_mut() {
            digest.update(&chunk);
        }
        completed = completed.saturating_add(chunk.len() as u64);
        progress(completed, total.or(request.expected_size))?;
    }
    file.flush().await?;
    file.sync_all().await?;
    drop(file);

    if let Some(expected) = total
        && completed != expected
    {
        return Err(Error::Network(format!(
            "unexpected file size for {url}: expected {expected}, got {completed}"
        )));
    }

    match streamed.map(|digest| format!("{:x}", digest.finalize())) {
        Some(actual) => {
            if let Some(expected) = content_sha1
                && !actual.eq_ignore_ascii_case(expected.trim())
            {
                return Err(Error::Download(format!(
                    "SHA-1 mismatch for {}: expected {}, got {actual}",
                    temporary.display(),
                    expected
                )));
            }
            if let Some(expected) = request.expected_size
                && completed != expected
            {
                return Err(Error::Download(format!(
                    "size mismatch for {}: expected {expected}, got {completed}",
                    temporary.display()
                )));
            }
            // The bytes were hashed as they arrived; only the container check
            // still has to touch the file.
            if matches!(request.integrity, IntegrityKind::ZipJar) {
                let path = temporary.to_path_buf();
                blocking(move || verify_download(&path, None, None, IntegrityKind::ZipJar)).await?;
            }
        }
        None => {
            let path = temporary.to_path_buf();
            let sha1 = content_sha1.map(str::to_owned);
            let size = request.expected_size;
            let integrity = request.integrity;
            blocking(move || verify_download(&path, sha1.as_deref(), size, integrity)).await?;
        }
    }
    let mut cache_path = None;
    let headers = response.headers().clone();
    if request.cache_policy != CachePolicy::None
        && let Some(sha1) = content_sha1
    {
        let cache = Arc::clone(cache);
        let path = temporary.to_path_buf();
        let sha1 = sha1.to_owned();
        cache_path = Some(blocking(move || cache.store_sha1(&path, &sha1)).await?);
    }
    if request.cache_policy == CachePolicy::Etag {
        let cache = Arc::clone(cache);
        let path = temporary.to_path_buf();
        let url = url.to_owned();
        let etag = header_value(&headers, header::ETAG);
        let last_modified = header_value(&headers, header::LAST_MODIFIED);
        let sha1 = content_sha1.map(str::to_owned);
        cache_path = Some(
            blocking(move || {
                cache
                    .store_etag(
                        &url,
                        &path,
                        etag.as_deref(),
                        last_modified.as_deref(),
                        sha1.as_deref(),
                    )
                    .map(|hit| hit.path)
            })
            .await?,
        );
    }
    commit_temporary(temporary, &request.destination, request.destination_policy)?;
    Ok(DownloadOutcome {
        downloaded: true,
        bytes: completed,
        cache_path,
    })
}

fn resume_context(response: &Response) -> Option<ResumeContext> {
    if response.status() != StatusCode::OK
        || response
            .headers()
            .get(header::ACCEPT_RANGES)
            .and_then(|value| value.to_str().ok())
            .is_none_or(|value| !value.eq_ignore_ascii_case("bytes"))
        || response
            .headers()
            .get(header::CONTENT_ENCODING)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| !value.eq_ignore_ascii_case("identity"))
    {
        return None;
    }
    let total = response.content_length()?;
    let etag =
        header_value(response.headers(), header::ETAG).filter(|value| !value.starts_with("W/"));
    let last_modified = header_value(response.headers(), header::LAST_MODIFIED);
    if etag.is_none() && last_modified.is_none() {
        return None;
    }
    Some(ResumeContext {
        etag,
        last_modified,
        total,
    })
}

fn valid_resume_response(response: &Response, offset: u64, context: &ResumeContext) -> bool {
    if response.status() != StatusCode::PARTIAL_CONTENT
        || response
            .headers()
            .get(header::CONTENT_ENCODING)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| !value.eq_ignore_ascii_case("identity"))
    {
        return false;
    }
    if context.etag.as_deref().is_some_and(|expected| {
        header_value(response.headers(), header::ETAG).as_deref() != Some(expected)
    }) {
        return false;
    }
    if context.etag.is_none()
        && context.last_modified.as_deref()
            != header_value(response.headers(), header::LAST_MODIFIED).as_deref()
    {
        return false;
    }
    let Some((start, end, total)) = content_range(response) else {
        return false;
    };
    start == offset && end >= start && total == context.total
}

fn content_range(response: &Response) -> Option<(u64, u64, u64)> {
    let value = response
        .headers()
        .get(header::CONTENT_RANGE)?
        .to_str()
        .ok()?;
    let (range, total) = value.strip_prefix("bytes ")?.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    Some((start.parse().ok()?, end.parse().ok()?, total.parse().ok()?))
}

fn header_value(headers: &header::HeaderMap, name: header::HeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

fn bmclapi_sha1(response: &Response) -> Option<String> {
    let value = response
        .headers()
        .get("x-bmclapi-hash")
        .and_then(|value| value.to_str().ok())?
        .trim();
    (value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| value.to_ascii_lowercase())
}

fn is_client_error(error: &Error) -> bool {
    matches!(error, Error::HttpStatus { status, .. } if (400..500).contains(status))
}

fn http_status_error(url: &str, status: StatusCode) -> Error {
    Error::HttpStatus {
        status: status.as_u16(),
        request: format!("GET {url}"),
    }
}

fn validate_url(url: &str) -> Result<()> {
    let parsed = reqwest::Url::parse(url)
        .map_err(|_| Error::Network(format!("invalid download URL: {url}")))?;
    let loopback = parsed.scheme() == "http"
        && parsed
            .host_str()
            .is_some_and(|host| matches!(host, "127.0.0.1" | "localhost" | "::1"));
    if parsed.scheme() != "https" && !loopback {
        return Err(Error::Network(format!(
            "download URL must use HTTPS (or loopback HTTP): {url}"
        )));
    }
    Ok(())
}

async fn wait_if_resumed(
    control: Option<&dyn DownloadTaskControl>,
    cancelled: &AtomicBool,
) -> Result<()> {
    if let Some(control) = control {
        wait_if_paused(Some(control), cancelled).await
    } else if cancelled.load(Ordering::Relaxed) {
        Err(Error::Download("download cancelled".into()))
    } else {
        Ok(())
    }
}

fn commit_temporary(temporary: &Path, destination: &Path, policy: DestinationPolicy) -> Result<()> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if policy == DestinationPolicy::NoReplace && destination.exists() {
        return Err(Error::Download(format!(
            "destination already exists: {}",
            destination.display()
        )));
    }
    replace_path(temporary, destination)?;
    Ok(())
}

/// Copies a verified cache entry into place off the async workers.
async fn copy_to_destination(
    source: &Path,
    destination: &Path,
    policy: DestinationPolicy,
) -> Result<()> {
    let source = source.to_path_buf();
    let destination = destination.to_path_buf();
    blocking(move || copy_to_destination_blocking(&source, &destination, policy)).await
}

fn copy_to_destination_blocking(
    source: &Path,
    destination: &Path,
    policy: DestinationPolicy,
) -> Result<()> {
    if policy == DestinationPolicy::NoReplace && destination.exists() {
        return Err(Error::Download(format!(
            "destination already exists: {}",
            destination.display()
        )));
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = destination.with_file_name(format!(
        ".{}.cache-{}",
        destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("download"),
        Uuid::new_v4()
    ));
    let result = (|| {
        std::fs::copy(source, &temporary)?;
        if policy == DestinationPolicy::NoReplace && destination.exists() {
            return Err(Error::Download(format!(
                "destination already exists: {}",
                destination.display()
            )));
        }
        replace_path(&temporary, destination)?;
        Ok::<(), Error>(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

async fn remove_if_exists(path: &Path) -> Result<()> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
