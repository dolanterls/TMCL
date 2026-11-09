//! Loading, formatting and parsing helpers.

use std::collections::{HashMap, HashSet};

use crate::core::instance::config::load_or_create;

use super::*;

/// Two-glyph playtime for the overview hero, where the value is drawn as
/// block letters and four characters would not fit in half a card.
///
/// `format_play_time` is right for a table cell (`12h03m`); the hero needs the
/// same information in `12h` / `5m`, and a brand-new instance must not read as
/// `0h` when it is under the hour mark.
pub(in crate::tui::app) fn compact_play_time(seconds: u64) -> String {
    let hours = seconds / 3_600;
    if hours > 0 {
        format!("{hours}h")
    } else {
        format!("{}m", (seconds % 3_600) / 60)
    }
}

pub(in crate::tui::app) fn format_play_time(seconds: u64) -> String {
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if hours > 0 {
        format!("{hours}h{minutes:02}m")
    } else {
        format!("{minutes}m")
    }
}

impl App {
    /// Overlay "installing" on every instance whose files are still arriving.
    ///
    /// The summary is built from the disk, and a modpack writes the version jar
    /// and json before it fetches a single mod, so a pack at 50% reads like a
    /// finished instance. Only the task engine knows the difference, so the list
    /// asks it on every refresh and on every task update.
    pub(in crate::tui::app) fn sync_install_statuses(&mut self) {
        let busy: Vec<Uuid> = self
            .task_snapshots
            .iter()
            .filter(|snapshot| {
                matches!(
                    snapshot.state,
                    crate::core::task::TaskState::Queued
                        | crate::core::task::TaskState::Running
                        | crate::core::task::TaskState::Paused
                )
            })
            .filter_map(|snapshot| {
                self.task_engine
                    .operation(snapshot.id)
                    .and_then(|operation| operation.instance_id())
            })
            .collect();
        for item in &mut self.instances.items {
            if busy.contains(&item.id) {
                if item.status != INSTALLING {
                    item.status = INSTALLING.into();
                }
            } else if item.status == INSTALLING {
                // The task is gone; the disk decides again.
                item.status = if instance_files_ready(&item.path, &item.name) {
                    READY.into()
                } else {
                    "not installed".into()
                };
            }
        }
    }

    /// Paint the live launch onto the instance list.
    ///
    /// The summary's `status` is what the disk says, and a running game writes
    /// nothing to disk, so without this an instance reads `就绪` while its window
    /// is on screen. Called on every launch transition and after every reload,
    /// which makes it the single writer of the launch statuses.
    pub(in crate::tui::app) fn sync_launch_status(&mut self) {
        let live = self.active_launch.as_ref().map(|launch| {
            let status = match launch.phase {
                LaunchPhase::Starting => LAUNCHING,
                LaunchPhase::Running => RUNNING,
            };
            (launch.instance_id, status)
        });
        for item in &mut self.instances.items {
            if let Some((id, status)) = live {
                if item.id == id {
                    if item.status != status {
                        item.status = status.into();
                    }
                    continue;
                }
            }
            // Either no process is running, or it belongs to another row: a row
            // still marked live is stale and the disk answers again. Leaving it
            // alone would keep `运行中` on an instance whose launch was replaced.
            if item.status == LAUNCHING || item.status == RUNNING {
                item.status = if instance_files_ready(&item.path, &item.name) {
                    "ready".into()
                } else {
                    "not installed".into()
                };
            }
        }
    }

    pub(in crate::tui::app) fn current_account(&self) -> Option<Account> {
        self.config
            .state
            .current_account_id
            .as_deref()
            .and_then(|id| Uuid::parse_str(id).ok())
            .and_then(|id| {
                self.accounts
                    .accounts
                    .iter()
                    .find(|account| account.id == id)
            })
            .or_else(|| self.accounts.accounts.get(self.accounts.selected))
            .cloned()
    }

    pub(in crate::tui::app) fn upsert_instance(&mut self, instance: InstanceConfig) {
        let summary = instance_summary(instance);
        if let Some(index) = self
            .instances
            .items
            .iter()
            .position(|item| item.id == summary.id || item.path == summary.path)
        {
            self.instances.items[index] = summary;
            self.instances.focused = index;
        } else {
            self.instances.items.push(summary);
            self.instances.focused = self.instances.items.len() - 1;
        }
    }

    pub(in crate::tui::app) fn remove_exclusion(&mut self, path: &Path) {
        let normalized = normalize_instance_path(path);
        self.config
            .excluded_instances
            .retain(|excluded| normalize_instance_path(excluded) != normalized);
    }

    pub(in crate::tui::app) fn save_config(&self) -> Result<()> {
        write_json_atomic(&self.paths.config_file, &self.config)?;
        configure_download_executor(&self.config);
        Ok(())
    }

    pub(in crate::tui::app) fn commit_config(&mut self, next: Config) -> Result<()> {
        write_json_atomic(&self.paths.config_file, &next)?;
        configure_download_executor(&next);
        self.theme = Theme::from_ui(&next.state.ui);
        self.config = next;
        Ok(())
    }

    pub(in crate::tui::app) fn save_config_change<F>(&mut self, change: F) -> Result<()>
    where
        F: FnOnce(&mut Config),
    {
        let mut next = self.config.clone();
        change(&mut next);
        write_json_atomic(&self.paths.config_file, &next)?;
        configure_download_executor(&next);
        self.theme = Theme::from_ui(&next.state.ui);
        self.config = next;
        Ok(())
    }
}

pub(in crate::tui::app) fn configure_download_executor(config: &Config) {
    crate::core::download::DownloadExecutor::configure_global(
        crate::core::download::DownloadExecutorConfig {
            auto: config.downloads.auto_download_threads,
            threads: config.downloads.download_threads,
        },
    );
}

/// One instance directory as it was last scanned.
#[derive(Debug, Clone)]
struct CachedInstance {
    stamp: InstanceStamp,
    summary: InstanceSummary,
}

/// Everything that decides what an instance looks like in the list.
///
/// `tmcl.json` carries the name, version and loader; the jar/json pair decides
/// whether the row reads `ready`. A reinstall can add only the jar, so the
/// manifest alone is not enough to tell whether a cached row is still right.
#[derive(Debug, Clone, PartialEq, Eq)]
struct InstanceStamp {
    /// Nanoseconds since the epoch: a one-second stamp would miss two saves
    /// that land in the same second, which is exactly what a settings toggle
    /// does.
    modified: u128,
    len: u64,
    jar: bool,
    json: bool,
}

/// Results of the previous scan, keyed by instance directory.
///
/// A launcher with a few hundred instances re-reads every `tmcl.json` on each
/// refresh; the file changes only when an instance is renamed, reinstalled or
/// switched to another loader, so comparing a small stamp keeps the common
/// case to a handful of `stat` calls. Entries for directories that disappeared
/// are dropped at the end of every scan.
#[derive(Debug, Default)]
pub(in crate::tui::app) struct InstanceScanCache {
    entries: HashMap<PathBuf, CachedInstance>,
    /// Directories read from disk, as opposed to served from `entries`.
    /// Tests use it to prove the second refresh did not touch the filesystem.
    scans: u64,
}

impl InstanceScanCache {
    #[cfg(test)]
    pub(in crate::tui::app) fn scans(&self) -> u64 {
        self.scans
    }
}

/// Walks every game directory once and returns the instance list.
///
/// The previous version ran the generic discovery pass and then walked
/// `versions/` again to pick up instances whose jar is missing; that second
/// walk also de-duplicated with a linear scan, which is quadratic on a full
/// library. Both jobs happen here in one pass, and unchanged directories come
/// from `cache`.
pub(in crate::tui::app) fn load_instances(
    config: &Config,
    cache: &mut InstanceScanCache,
) -> Result<Vec<InstanceSummary>> {
    let mut instances = Vec::new();
    // Two roots can expose the same instance; so can one directory after a
    // rename that kept the id. Either match means "already listed".
    let mut ids: HashSet<Uuid> = HashSet::new();
    let mut paths: HashSet<PathBuf> = HashSet::new();
    let mut live: HashSet<PathBuf> = HashSet::new();

    for game_directory in &config.game_directories {
        if !game_directory.is_dir() {
            continue;
        }
        let root = normalize_game_directory(game_directory)?;
        let game_id = game_directory_id(&root);
        let versions = root.join("versions");
        if !versions.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(versions)?.flatten() {
            let path = entry.path();
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if !path.is_dir() || is_excluded(config, &path) {
                continue;
            }
            let manifest = path.join("tmcl.json");
            let jar = path.join(format!("{name}.jar"));
            let json = path.join(format!("{name}.json"));
            let has_manifest = manifest.is_file();
            let installed = jar.is_file() && json.is_file();
            // Neither an instance TMCL knows nor a complete install: a leftover
            // folder from a half-finished copy.
            if !has_manifest && !installed {
                continue;
            }

            live.insert(path.clone());
            let cached = has_manifest
                .then(|| instance_stamp(&manifest, installed))
                .flatten()
                .and_then(|stamp| {
                    cache
                        .entries
                        .get(&path)
                        .filter(|cached| cached.stamp == stamp)
                        .map(|cached| cached.summary.clone())
                });
            let summary = match cached {
                Some(summary) => summary,
                None => {
                    let instance = if installed {
                        load_or_create(&path, game_id, &name)?
                    } else {
                        // A jar-less directory still shows up so the player can
                        // see (and repair) it; its stored config is authoritative
                        // because there is no client json to repair from.
                        let Ok(mut instance) = read_json::<InstanceConfig>(&manifest) else {
                            continue;
                        };
                        instance.path = path.clone();
                        instance
                    };
                    let summary = instance_summary(instance);
                    cache.scans += 1;
                    // Stamp after reading: `load_or_create` writes `tmcl.json`
                    // for an instance imported from another launcher, and the
                    // stamp taken before that write would never match again.
                    if let Some(stamp) = instance_stamp(&manifest, installed) {
                        cache.entries.insert(
                            path.clone(),
                            CachedInstance {
                                stamp,
                                summary: summary.clone(),
                            },
                        );
                    }
                    summary
                }
            };

            let normalized = normalize_instance_path(&summary.path);
            if ids.contains(&summary.id) || paths.contains(&normalized) {
                continue;
            }
            ids.insert(summary.id);
            paths.insert(normalized);
            instances.push(summary);
        }
    }

    cache.entries.retain(|path, _| live.contains(path));
    instances.sort_by_key(|instance| instance.name.to_lowercase());
    Ok(instances)
}

fn instance_stamp(manifest: &Path, installed: bool) -> Option<InstanceStamp> {
    let metadata = std::fs::metadata(manifest).ok()?;
    let modified = metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some(InstanceStamp {
        modified,
        len: metadata.len(),
        jar: installed,
        json: installed,
    })
}

pub(in crate::tui::app) fn normalize_instance_path(path: &Path) -> PathBuf {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let text = path.to_string_lossy();
    // Windows canonicalize returns a verbatim `\\?\` path, while a path
    // that does not exist yet is plain. Normalize both to the same key so an
    // excluded instance is still matched after it is recreated.
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
    #[cfg(windows)]
    {
        PathBuf::from(text.to_ascii_lowercase())
    }
    #[cfg(not(windows))]
    {
        PathBuf::from(text.as_ref())
    }
}

pub(in crate::tui::app) fn is_excluded(config: &Config, path: &Path) -> bool {
    let path = normalize_instance_path(path);
    config
        .excluded_instances
        .iter()
        .any(|excluded| normalize_instance_path(excluded) == path)
}

/// Status string for an instance whose files are still being fetched.
pub(in crate::tui::app) const INSTALLING: &str = "installing";

/// Status strings for an instance the launcher is currently driving. They are
/// not on disk: `sync_launch_status` writes them from the live process and
/// restores the disk value when it ends.
pub(in crate::tui::app) const READY: &str = "ready";
pub(in crate::tui::app) const LAUNCHING: &str = "launching";
pub(in crate::tui::app) const RUNNING: &str = "running";

/// A version is complete when its jar and its json are both on disk.
pub(in crate::tui::app) fn instance_files_ready(path: &Path, name: &str) -> bool {
    path.join(format!("{name}.jar")).is_file() && path.join(format!("{name}.json")).is_file()
}

pub(in crate::tui::app) fn instance_summary(instance: InstanceConfig) -> InstanceSummary {
    let ready = instance_files_ready(&instance.path, &instance.name);
    let (loader, loader_version) = match &instance.loader {
        InstanceLoader::Vanilla => ("Vanilla".to_owned(), None),
        InstanceLoader::Fabric { version } => ("Fabric".to_owned(), Some(version.clone())),
        InstanceLoader::Quilt { version } => ("Quilt".to_owned(), Some(version.clone())),
        InstanceLoader::Forge { version } => ("Forge".to_owned(), Some(version.clone())),
        InstanceLoader::NeoForge { version } => ("NeoForge".to_owned(), Some(version.clone())),
    };
    InstanceSummary {
        id: instance.id,
        name: instance.name,
        version: instance.minecraft_version,
        loader,
        loader_version,
        status: if ready { "ready" } else { "not installed" }.into(),
        path: instance.path,
        launch_count: instance.stats.launch_count,
        play_time_seconds: instance.stats.play_time_seconds,
    }
}

pub(in crate::tui::app) fn instance_status_kind(status: &str) -> StatusKind {
    let normalized = status.to_ascii_lowercase();
    if normalized.contains("fail") || normalized.contains("error") {
        StatusKind::Failed
    } else if normalized == INSTALLING || normalized == LAUNCHING || normalized == RUNNING {
        // A live process is progress, not a missing file: the `install` arm
        // below would call it `Missing`.
        StatusKind::Running
    } else if normalized.contains("ready") {
        StatusKind::Ready
    } else if normalized.contains("install") || normalized.contains("missing") {
        StatusKind::Missing
    } else {
        StatusKind::Warning
    }
}

pub(in crate::tui::app) fn localize_instance_status(status: &str, i18n: &I18n) -> String {
    match status.to_ascii_lowercase().as_str() {
        "ready" => i18n.text("status.ready"),
        INSTALLING => i18n.text("status.installing"),
        LAUNCHING => i18n.text("status.launching"),
        RUNNING => i18n.text("status.running"),
        "missing" => i18n.text("common.missing"),
        "failed" => i18n.text("task.failed"),
        value => value.to_owned(),
    }
}

pub(in crate::tui::app) fn localize_task_state(state: TaskState, i18n: &I18n) -> String {
    let key = match state {
        TaskState::Queued => "task.queued",
        TaskState::Running => "task.running",
        TaskState::Paused => "task.paused",
        TaskState::Succeeded => "task.completed",
        TaskState::Failed => "task.failed",
        TaskState::Cancelled => "task.cancelled",
    };
    i18n.text(key)
}

pub(in crate::tui::app) fn localize_resource_source(source: ResourceSource, i18n: &I18n) -> String {
    i18n.text(match source {
        ResourceSource::Modrinth => "download.source_modrinth",
        ResourceSource::CurseForge => "download.source_curseforge",
    })
}

pub(in crate::tui::app) fn localize_resource_sort(sort: ResourceSort, i18n: &I18n) -> String {
    i18n.text(match sort {
        ResourceSort::Relevance => "download.sort_relevance",
        ResourceSort::Downloads => "download.sort_downloads",
        ResourceSort::Follows => "download.sort_follows",
        ResourceSort::Newest => "download.sort_newest",
        ResourceSort::Updated => "download.sort_updated",
    })
}

pub(in crate::tui::app) fn localize_compatibility(
    compatibility: crate::core::resource::model::Compatibility,
    i18n: &I18n,
) -> String {
    i18n.text(match compatibility {
        crate::core::resource::model::Compatibility::Compatible => "download.compatible",
        crate::core::resource::model::Compatibility::Unknown => "download.unknown_compatibility",
        crate::core::resource::model::Compatibility::Incompatible => "download.incompatible",
    })
}

pub(in crate::tui::app) fn task_status_kind(state: TaskState) -> StatusKind {
    match state {
        TaskState::Queued => StatusKind::Queued,
        TaskState::Running => StatusKind::Running,
        TaskState::Paused => StatusKind::Warning,
        TaskState::Succeeded => StatusKind::Completed,
        TaskState::Failed | TaskState::Cancelled => StatusKind::Failed,
    }
}

pub(in crate::tui::app) fn truncate_text(value: &str, max_width: usize) -> String {
    if Line::from(value).width() <= max_width {
        return value.to_owned();
    }
    let budget = max_width.saturating_sub(1);
    let mut truncated = String::new();
    let mut used = 0;
    for character in value.chars() {
        let width = Line::from(character.to_string()).width();
        if used + width > budget {
            break;
        }
        truncated.push(character);
        used += width;
    }
    truncated.push('…');
    truncated
}

/// Stable id of a game directory; the rule lives in `core::instance::path` so
/// the CLI and the TUI cannot derive two different ids for the same folder.
pub(in crate::tui::app) fn game_directory_id(path: &Path) -> Uuid {
    crate::core::instance::path::game_directory_id(path)
}

pub(in crate::tui::app) fn add_game_directory(config: &mut Config, path: &Path) {
    config
        .game_directories
        .retain(|candidate| candidate != path);
    config.game_directories.insert(0, path.to_path_buf());
}

pub(in crate::tui::app) fn infer_minecraft_version(instance_directory: &Path) -> String {
    let instance_name = instance_directory
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let json_path = instance_directory.join(format!("{instance_name}.json"));
    let version = std::fs::read(json_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|json| {
            json.get("inheritsFrom")
                .and_then(serde_json::Value::as_str)
                .or_else(|| json.get("id").and_then(serde_json::Value::as_str))
                .map(str::to_owned)
        });
    version.unwrap_or_else(|| instance_name.to_owned())
}

pub(in crate::tui::app) fn required_value<'a>(
    values: &'a [String],
    index: usize,
    label: &str,
) -> Result<&'a str> {
    values
        .get(index)
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| Error::Config(format!("{label} is required")))
}

pub(in crate::tui::app) fn move_bounded(current: usize, length: usize, delta: isize) -> usize {
    if length == 0 {
        return 0;
    }
    if delta < 0 {
        current.saturating_sub(delta.unsigned_abs())
    } else {
        current.saturating_add(delta as usize).min(length - 1)
    }
}

pub(in crate::tui::app) fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}
