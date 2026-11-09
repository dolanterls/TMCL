use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use uuid::Uuid;

use crate::core::config::storage::replace_path;
use crate::core::download::hash::verify_sha1;
use crate::core::error::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EtagEntry {
    etag: Option<String>,
    last_modified: Option<String>,
    path: PathBuf,
    size: u64,
    #[serde(default)]
    sha1: Option<String>,
}

#[derive(Debug)]
pub struct DownloadCache {
    root: PathBuf,
    etag_index: Mutex<BTreeMap<String, EtagEntry>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheStats {
    pub files: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheClearFailure {
    pub path: PathBuf,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheClearReport {
    pub before: CacheStats,
    pub removed_files: u64,
    pub removed_bytes: u64,
    pub failures: Vec<CacheClearFailure>,
}

pub fn stats(root: &Path) -> Result<CacheStats> {
    let mut result = CacheStats { files: 0, bytes: 0 };
    if !is_safe_root(root)? {
        return Ok(result);
    }
    collect_stats(root, &mut result)?;
    Ok(result)
}

/// Result of trimming the cache down to a size limit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheEviction {
    pub before: CacheStats,
    pub after: CacheStats,
    pub removed_files: u64,
    pub removed_bytes: u64,
    pub failures: Vec<CacheClearFailure>,
}

/// Deletes the least recently used files until the cache fits `limit_bytes`.
///
/// Files newer than `protect` are kept regardless: a file written moments ago is
/// about to be copied into an instance, and evicting it would make the install read
/// the same bytes twice. `limit_bytes == 0` means "no limit" and evicts nothing.
pub fn evict_to(
    root: &Path,
    limit_bytes: u64,
    protect: std::time::Duration,
) -> Result<CacheEviction> {
    let mut before = CacheStats { files: 0, bytes: 0 };
    let mut failures = Vec::new();
    if limit_bytes == 0 || !is_safe_root(root)? {
        return Ok(CacheEviction {
            before,
            after: before,
            removed_files: 0,
            removed_bytes: 0,
            failures,
        });
    }
    let mut entries = Vec::new();
    collect_entries(root, &mut before, &mut failures, &mut entries);
    let mut eviction = CacheEviction {
        before,
        after: before,
        removed_files: 0,
        removed_bytes: 0,
        failures,
    };
    if before.bytes <= limit_bytes {
        return Ok(eviction);
    }
    let now = std::time::SystemTime::now();
    entries.sort_by_key(|entry| entry.modified);
    for entry in entries {
        if eviction.after.bytes <= limit_bytes {
            break;
        }
        if now
            .duration_since(entry.modified)
            .is_ok_and(|age| age < protect)
        {
            continue;
        }
        match fs::remove_file(&entry.path) {
            Ok(()) => {
                eviction.removed_files = eviction.removed_files.saturating_add(1);
                eviction.removed_bytes = eviction.removed_bytes.saturating_add(entry.bytes);
                eviction.after.files = eviction.after.files.saturating_sub(1);
                eviction.after.bytes = eviction.after.bytes.saturating_sub(entry.bytes);
            }
            Err(error) => eviction.failures.push(CacheClearFailure {
                path: entry.path,
                error: error.to_string(),
            }),
        }
    }
    Ok(eviction)
}

struct CacheEntry {
    path: PathBuf,
    bytes: u64,
    modified: std::time::SystemTime,
}

fn collect_entries(
    path: &Path,
    stats: &mut CacheStats,
    failures: &mut Vec<CacheClearFailure>,
    entries: &mut Vec<CacheEntry>,
) {
    let read_dir = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) => {
            failures.push(CacheClearFailure {
                path: path.to_path_buf(),
                error: error.to_string(),
            });
            return;
        }
    };
    for entry in read_dir {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                failures.push(CacheClearFailure {
                    path: path.to_path_buf(),
                    error: error.to_string(),
                });
                continue;
            }
        };
        let entry_path = entry.path();
        let metadata = match fs::symlink_metadata(&entry_path) {
            Ok(metadata) => metadata,
            Err(error) => {
                failures.push(CacheClearFailure {
                    path: entry_path,
                    error: error.to_string(),
                });
                continue;
            }
        };
        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_entries(&entry_path, stats, failures, entries);
        } else if file_type.is_file() {
            stats.files = stats.files.saturating_add(1);
            stats.bytes = stats.bytes.saturating_add(metadata.len());
            entries.push(CacheEntry {
                path: entry_path,
                bytes: metadata.len(),
                // An unreadable timestamp sorts first: the file is evicted
                // before anything the launcher can date.
                modified: metadata.modified().unwrap_or(std::time::UNIX_EPOCH),
            });
        }
    }
}

pub fn clear(root: &Path) -> Result<CacheStats> {
    Ok(clear_with_report(root)?.before)
}

pub fn clear_with_report(root: &Path) -> Result<CacheClearReport> {
    let mut before = CacheStats { files: 0, bytes: 0 };
    let mut failures = Vec::new();
    if !is_safe_root(root)? {
        return Ok(CacheClearReport {
            before,
            removed_files: 0,
            removed_bytes: 0,
            failures,
        });
    }
    collect_stats_best_effort(root, &mut before, &mut failures);
    let mut report = CacheClearReport {
        before,
        removed_files: 0,
        removed_bytes: 0,
        failures,
    };
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => {
            report.failures.push(CacheClearFailure {
                path: root.to_path_buf(),
                error: error.to_string(),
            });
            return Ok(report);
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                report.failures.push(CacheClearFailure {
                    path: root.to_path_buf(),
                    error: error.to_string(),
                });
                continue;
            }
        };
        let path = entry.path();
        clear_entry(&path, &mut report);
    }
    Ok(report)
}

fn clear_entry(path: &Path, report: &mut CacheClearReport) {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            report.failures.push(CacheClearFailure {
                path: path.to_path_buf(),
                error: error.to_string(),
            });
            return;
        }
    };
    let file_type = metadata.file_type();
    if file_type.is_symlink() || file_type.is_file() {
        match fs::remove_file(path) {
            Ok(()) => {
                if file_type.is_file() {
                    report.removed_files = report.removed_files.saturating_add(1);
                    report.removed_bytes = report.removed_bytes.saturating_add(metadata.len());
                }
            }
            Err(error) => report.failures.push(CacheClearFailure {
                path: path.to_path_buf(),
                error: error.to_string(),
            }),
        }
    } else if file_type.is_dir() {
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(error) => {
                report.failures.push(CacheClearFailure {
                    path: path.to_path_buf(),
                    error: error.to_string(),
                });
                return;
            }
        };
        for entry in entries {
            match entry {
                Ok(entry) => clear_entry(&entry.path(), report),
                Err(error) => report.failures.push(CacheClearFailure {
                    path: path.to_path_buf(),
                    error: error.to_string(),
                }),
            }
        }
        match fs::remove_dir(path) {
            Ok(()) => {}
            Err(error) => report.failures.push(CacheClearFailure {
                path: path.to_path_buf(),
                error: error.to_string(),
            }),
        }
    }
}

fn is_safe_root(root: &Path) -> Result<bool> {
    if !root.is_dir() {
        return Ok(false);
    }
    if fs::symlink_metadata(root)?.file_type().is_symlink() {
        return Ok(false);
    }
    let canonical = root.canonicalize()?;
    let Some(name) = canonical.file_name().and_then(|name| name.to_str()) else {
        return Ok(false);
    };
    Ok(name.eq_ignore_ascii_case("cache") || name.eq_ignore_ascii_case(".tmcl-cache"))
}

fn collect_stats(path: &Path, result: &mut CacheStats) -> Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_stats(&path, result)?;
        } else if file_type.is_file() {
            result.files = result.files.saturating_add(1);
            result.bytes = result.bytes.saturating_add(metadata.len());
        }
    }
    Ok(())
}

fn collect_stats_best_effort(
    path: &Path,
    result: &mut CacheStats,
    failures: &mut Vec<CacheClearFailure>,
) {
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) => {
            failures.push(CacheClearFailure {
                path: path.to_path_buf(),
                error: error.to_string(),
            });
            return;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                failures.push(CacheClearFailure {
                    path: path.to_path_buf(),
                    error: error.to_string(),
                });
                continue;
            }
        };
        let entry_path = entry.path();
        let metadata = match fs::symlink_metadata(&entry_path) {
            Ok(metadata) => metadata,
            Err(error) => {
                failures.push(CacheClearFailure {
                    path: entry_path,
                    error: error.to_string(),
                });
                continue;
            }
        };
        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_stats_best_effort(&entry_path, result, failures);
        } else if file_type.is_file() {
            result.files = result.files.saturating_add(1);
            result.bytes = result.bytes.saturating_add(metadata.len());
        }
    }
}

impl DownloadCache {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        let _index_guard = etag_index_lock()
            .lock()
            .map_err(|_| Error::Download("ETag cache lock poisoned".into()))?;
        let etag_index = read_etag_index(&root)?;
        Ok(Self {
            root,
            etag_index: Mutex::new(etag_index),
        })
    }

    pub fn sha1_path(&self, sha1: &str) -> PathBuf {
        let normalized = sha1.trim().to_ascii_lowercase();
        let prefix = normalized.get(..2).unwrap_or("00");
        self.root.join("sha1").join(prefix).join(normalized)
    }

    pub fn lookup_sha1(&self, expected_sha1: &str) -> Result<Option<PathBuf>> {
        let path = self.sha1_path(expected_sha1);
        if !path.is_file() {
            return Ok(None);
        }
        if verify_sha1(&path, Some(expected_sha1)).is_ok() {
            Ok(Some(path))
        } else {
            remove_if_exists(&path)?;
            Ok(None)
        }
    }

    /// Stores an already verified file under its content hash.
    ///
    /// The caller must have checked `source` against `expected_sha1` - the
    /// download path hashes what it writes, so re-hashing here meant reading
    /// every file a second time for nothing. A cache entry that somehow went
    /// bad is still caught by [`Self::lookup_sha1`], which verifies on read.
    pub fn store_sha1(&self, source: &Path, expected_sha1: &str) -> Result<PathBuf> {
        let target = self.sha1_path(expected_sha1);
        atomic_copy(source, &target)?;
        Ok(target)
    }

    pub fn lookup_etag(&self, url: &str) -> Result<Option<EtagHit>> {
        let key = normalize_url(url);
        let guard = self
            .etag_index
            .lock()
            .map_err(|_| Error::Download("ETag cache lock poisoned".into()))?;
        let Some(entry) = guard.get(&key) else {
            return Ok(None);
        };
        if !entry.path.is_file() {
            return Ok(None);
        }
        Ok(Some(EtagHit {
            path: entry.path.clone(),
            etag: entry.etag.clone(),
            last_modified: entry.last_modified.clone(),
            size: entry.size,
            sha1: entry.sha1.clone(),
        }))
    }

    pub fn store_etag(
        &self,
        url: &str,
        source: &Path,
        etag: Option<&str>,
        last_modified: Option<&str>,
        sha1: Option<&str>,
    ) -> Result<EtagHit> {
        let _index_guard = etag_index_lock()
            .lock()
            .map_err(|_| Error::Download("ETag cache lock poisoned".into()))?;
        let key = normalize_url(url);
        let filename = format!("{:x}", Sha1::digest(key.as_bytes()));
        let path = self.root.join("etag").join(filename);
        atomic_copy(source, &path)?;
        let entry = EtagEntry {
            etag: etag.map(str::to_owned),
            last_modified: last_modified.map(str::to_owned),
            path: path.clone(),
            size: fs::metadata(&path)?.len(),
            sha1: sha1.map(str::to_owned),
        };
        let mut guard = self
            .etag_index
            .lock()
            .map_err(|_| Error::Download("ETag cache lock poisoned".into()))?;
        *guard = read_etag_index(&self.root)?;
        guard.insert(key, entry.clone());
        self.persist_index(&guard)?;
        Ok(EtagHit {
            path,
            etag: entry.etag,
            last_modified: entry.last_modified,
            size: entry.size,
            sha1: entry.sha1,
        })
    }

    pub fn remove_etag(&self, url: &str) -> Result<()> {
        let _index_guard = etag_index_lock()
            .lock()
            .map_err(|_| Error::Download("ETag cache lock poisoned".into()))?;
        let key = normalize_url(url);
        let mut guard = self
            .etag_index
            .lock()
            .map_err(|_| Error::Download("ETag cache lock poisoned".into()))?;
        *guard = read_etag_index(&self.root)?;
        guard.remove(&key);
        self.persist_index(&guard)
    }

    fn persist_index(&self, index: &BTreeMap<String, EtagEntry>) -> Result<()> {
        let path = self.root.join("etag.json");
        let temporary = path.with_file_name(format!(".etag.json.tmp-{}", Uuid::new_v4()));
        let result = (|| {
            fs::write(&temporary, serde_json::to_vec_pretty(index)?)?;
            replace_path(&temporary, &path)?;
            Ok::<(), Error>(())
        })();
        if result.is_err() {
            remove_if_exists(&temporary)?;
        }
        result
    }
}

fn etag_index_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn read_etag_index(root: &Path) -> Result<BTreeMap<String, EtagEntry>> {
    let index_path = root.join("etag.json");
    if index_path.is_file() {
        Ok(serde_json::from_slice(&fs::read(index_path)?)?)
    } else {
        Ok(BTreeMap::new())
    }
}

#[derive(Debug, Clone)]
pub struct EtagHit {
    pub path: PathBuf,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub size: u64,
    pub sha1: Option<String>,
}

fn normalize_url(url: &str) -> String {
    reqwest::Url::parse(url)
        .map(|mut parsed| {
            parsed.set_query(None);
            parsed.set_fragment(None);
            parsed.to_string()
        })
        .unwrap_or_else(|_| url.to_owned())
}

fn atomic_copy(source: &Path, target: &Path) -> Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = target.with_file_name(format!(
        ".{}.tmp-{}",
        target
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("cache"),
        Uuid::new_v4()
    ));
    let result = (|| {
        fs::copy(source, &temporary)?;
        replace_path(&temporary, target)?;
        Ok::<(), Error>(())
    })();
    if result.is_err() {
        remove_if_exists(&temporary)?;
    }
    result?;
    Ok(())
}

fn remove_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CacheClearReport, CacheStats, DownloadCache, clear, clear_entry, clear_with_report,
        evict_to, stats,
    };
    use crate::core::download::hash::sha1_file;

    #[test]
    fn stores_and_validates_sha1_content() {
        let root = std::env::temp_dir().join(format!("tmcl-cache-{}", uuid::Uuid::new_v4()));
        let source = root.join("source.bin");
        std::fs::create_dir_all(&root).expect("create root");
        std::fs::write(&source, b"cache").expect("write source");
        let hash = sha1_file(&source).expect("hash");
        let cache = DownloadCache::new(root.join("cache")).expect("cache");
        let cached = cache.store_sha1(&source, &hash).expect("store");
        assert_eq!(
            cache.lookup_sha1(&hash).expect("lookup"),
            Some(cached.clone())
        );
        std::fs::remove_file(cached).expect("remove corrupt cache");
        assert!(cache.lookup_sha1(&hash).expect("miss").is_none());
        std::fs::remove_dir_all(root).expect("clean root");
    }

    #[test]
    fn stats_and_clear_stay_inside_the_cache_root() {
        let parent =
            std::env::temp_dir().join(format!("tmcl-cache-stats-{}", uuid::Uuid::new_v4()));
        let root = parent.join("cache");
        let outside = parent.join("outside");
        std::fs::create_dir_all(root.join("sha1/aa")).expect("create cache");
        std::fs::create_dir_all(&outside).expect("create outside");
        std::fs::write(root.join("sha1/aa/file"), b"cache").expect("write cache");
        std::fs::write(outside.join("keep"), b"keep").expect("write outside");

        assert_eq!(stats(&root).expect("stats").bytes, 5);
        assert_eq!(clear(&root).expect("clear").files, 1);
        assert!(stats(&root).expect("empty stats").files == 0);
        assert!(outside.join("keep").is_file());

        std::fs::remove_dir_all(parent).expect("clean cache tree");
    }

    #[test]
    fn rejects_a_broad_directory_as_a_cache_root() {
        let root =
            std::env::temp_dir().join(format!("tmcl-cache-boundary-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create broad directory");
        assert_eq!(
            stats(&root).expect("stats"),
            super::CacheStats { files: 0, bytes: 0 }
        );
        let report = clear_with_report(&root).expect("clear report");
        assert_eq!(report.removed_files, 0);
        assert!(report.failures.is_empty());
        std::fs::remove_dir_all(root).expect("clean broad directory");
    }

    #[test]
    fn eviction_removes_the_oldest_files_until_the_limit_fits() {
        let parent =
            std::env::temp_dir().join(format!("tmcl-cache-evict-{}", uuid::Uuid::new_v4()));
        let root = parent.join("cache");
        std::fs::create_dir_all(root.join("objects")).expect("create cache");

        // Distinct write times, so "least recently used" has an order to follow.
        let mut paths = Vec::new();
        for name in ["old", "middle", "new"] {
            let path = root.join("objects").join(format!("{name}.bin"));
            std::fs::write(&path, vec![0u8; 100]).expect("write cache file");
            std::thread::sleep(std::time::Duration::from_millis(30));
            paths.push(path);
        }

        let report = evict_to(&root, 150, std::time::Duration::ZERO).expect("evict");
        assert_eq!(report.before.files, 3);
        assert_eq!(report.before.bytes, 300);
        assert_eq!(report.removed_files, 2);
        assert_eq!(report.removed_bytes, 200);
        assert!(!paths[0].exists(), "oldest file should go first");
        assert!(!paths[1].exists());
        assert!(paths[2].exists(), "the newest file is kept");
        assert!(report.failures.is_empty());
        std::fs::remove_dir_all(parent).expect("clean cache");
    }

    #[test]
    fn eviction_keeps_files_written_inside_the_protection_window() {
        let parent =
            std::env::temp_dir().join(format!("tmcl-cache-protect-{}", uuid::Uuid::new_v4()));
        let root = parent.join("cache");
        std::fs::create_dir_all(&root).expect("create cache");
        let path = root.join("hot.bin");
        std::fs::write(&path, vec![0u8; 100]).expect("write cache file");

        // Over the limit, but the file was written moments ago: an install about to
        // copy it must not lose it to its own cleanup.
        let report = evict_to(&root, 50, std::time::Duration::from_secs(600)).expect("evict");
        assert_eq!(report.removed_files, 0);
        assert!(path.exists());
        std::fs::remove_dir_all(parent).expect("clean cache");
    }

    #[test]
    fn a_zero_limit_disables_eviction() {
        let parent =
            std::env::temp_dir().join(format!("tmcl-cache-unlimited-{}", uuid::Uuid::new_v4()));
        let root = parent.join("cache");
        std::fs::create_dir_all(&root).expect("create cache");
        let path = root.join("kept.bin");
        std::fs::write(&path, vec![0u8; 100]).expect("write cache file");
        let report = evict_to(&root, 0, std::time::Duration::ZERO).expect("evict");
        assert_eq!(report.removed_files, 0);
        assert!(path.exists());
        std::fs::remove_dir_all(parent).expect("clean cache");
    }

    #[test]
    fn empty_cache_clear_is_successful_and_reports_no_failures() {
        let parent =
            std::env::temp_dir().join(format!("tmcl-cache-empty-{}", uuid::Uuid::new_v4()));
        let root = parent.join("cache");
        std::fs::create_dir_all(&root).expect("create cache");
        let report = clear_with_report(&root).expect("clear report");
        assert_eq!(report.before, super::CacheStats { files: 0, bytes: 0 });
        assert_eq!(report.removed_files, 0);
        assert!(report.failures.is_empty());
        std::fs::remove_dir_all(parent).expect("clean cache");
    }

    #[test]
    fn failed_entry_removal_is_recorded_in_the_report() {
        let root =
            std::env::temp_dir().join(format!("tmcl-cache-failure-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("cache")).expect("create cache");
        let mut report = CacheClearReport {
            before: CacheStats { files: 1, bytes: 1 },
            removed_files: 0,
            removed_bytes: 0,
            failures: Vec::new(),
        };
        clear_entry(&root.join("cache/missing"), &mut report);
        assert_eq!(report.removed_files, 0);
        assert_eq!(report.failures.len(), 1);
        std::fs::remove_dir_all(root).expect("clean cache");
    }
}
