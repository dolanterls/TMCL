use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crossterm::event::{KeyEvent, KeyEventKind, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
};
use uuid::Uuid;

use crate::core::account::authlib;
use crate::core::account::{
    microsoft::oauth::MicrosoftOAuth,
    models::{Account, AccountKind},
    offline::create_account,
    secrets::SecretStore,
    store::AccountStore,
};
use crate::core::config::{
    Config, DataPaths, load_or_default,
    models::{
        AccentColor, JavaSelectionMode, Language, LauncherVisibility, MemoryMode, NightlightTheme,
        PaperTheme, ProcessPriority, ShelfTheme, ThemeMode, UiPreset,
    },
    storage::{read_json, write_json_atomic},
};
use crate::core::download::cache;
use crate::core::download::{DownloadSource, DownloadSourceMode};
use crate::core::error::{Error, Result};
use crate::core::instance::{
    delete::delete_metadata,
    discovery::discover,
    models::{InstanceConfig, InstanceLoader},
    path::{display_path, executable_directory, normalize_game_directory},
    rename::rename_instance,
    service::{create_instance, import_instance},
};
use crate::core::java::{
    JavaInstallation,
    scan::discover_default,
    select::{minimum_java_major, select_java},
};
use crate::core::launch::process;
use crate::core::launch::process::GameEvent;
use crate::core::loader::{client_json, service::InstallLoader};
use crate::core::resource::model::{
    LoaderFilter, ResourceFile, ResourceKind, ResourceProject, ResourceSearchPage, ResourceSort,
    ResourceSource,
};
use crate::core::task::{
    InstanceInstallSpec, JavaRuntimeSpec, MinecraftTaskRunner, ModpackInstallSpec,
    ResourceInstallSpec, ResourceSaveAsSpec, TaskEngine, TaskOperation, TaskSnapshot, TaskState,
};
use crate::core::version::{
    cache::{VersionCache, list_versions},
    http::HttpClient,
    loader_meta::{LoaderKind as MetadataLoaderKind, list_loader_versions},
    models::{VersionCategory, VersionSummary},
};
use crate::i18n::I18n;
use crate::tui::action::Action;
use crate::tui::app::state::status::Status;
use crate::tui::command::Mode;
use crate::tui::hit::Hitmap;
use crate::tui::input::SearchState;
use crate::tui::input::SearchTarget;
use crate::tui::modal::{ConfirmAction, FormAction, Modal, ModalResult};
use crate::tui::pages::accounts::{AccountPage, DeviceCodeView};
use crate::tui::pages::download_center::{
    DownloadCategory, DownloadCenterState, DownloadDestination, RESOURCE_PAGE_SIZE,
};
use crate::tui::pages::instances::{InstancePage, InstanceSummary};
use crate::tui::pages::log::LogBuffer;
use crate::tui::pages::settings::{SettingId, SettingsCategory, SettingsState};
use crate::tui::pages::tasks::TaskFilter;
use crate::tui::shell::{ChromeRegions, LayoutMode, layout_chrome};
use crate::tui::theme::Theme;
use crate::tui::widgets::rows::{RowLayout, RowWindow, RowWindows};
use crate::tui::widgets::status::StatusKind;
use crate::tui::widgets::tabs::TabBar;

use super::event::AppEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Overview,
    Instances,
    Accounts,
    DownloadCenter,
    Tasks,
    Logs,
    Settings,
}

#[derive(Debug, Clone)]
enum LogExportKind {
    Launcher,
    Game,
    CrashReport(PathBuf),
}

const CREATE_VERSION_FIELD: usize = 2;
const CREATE_LOADER_FIELD: usize = 3;
const CREATE_LOADER_VERSION_FIELD: usize = 4;
const MAX_APP_EVENTS_PER_DRAW: usize = 128;

fn resource_page_count(total: u64, page_size: u32) -> u64 {
    total.div_ceil(u64::from(page_size.max(1))).max(1)
}

fn resource_search_context(
    kind: ResourceKind,
    target: Option<(String, LoaderFilter)>,
) -> (Option<String>, Option<LoaderFilter>) {
    if kind == ResourceKind::Modpack {
        return (None, None);
    }
    target.map_or((None, None), |(version, loader)| {
        let version = normalize_game_version(&version);
        let loader =
            (loader != LoaderFilter::Any && loader != LoaderFilter::Vanilla).then_some(loader);
        (version, loader)
    })
}

/// Instances imported from other launchers can store a folder label instead of
/// a Minecraft version (for example `1.12.2cocricot&miniaturia`). Keep the
/// leading dotted version so resource searches still filter sanely.
fn normalize_game_version(version: &str) -> Option<String> {
    let trimmed = version.trim();
    let end = trimmed
        .find(|character: char| !(character.is_ascii_digit() || character == '.'))
        .unwrap_or(trimmed.len());
    let candidate = trimmed[..end].trim_end_matches('.');
    let parts = candidate.split('.').filter(|part| !part.is_empty()).count();
    (parts >= 2).then(|| candidate.to_owned())
}

pub(super) fn drain_pending_events(
    app: &mut App,
    events: &mut tokio::sync::mpsc::UnboundedReceiver<AppEvent>,
) {
    for _ in 0..MAX_APP_EVENTS_PER_DRAW {
        let Ok(event) = events.try_recv() else {
            break;
        };
        app.handle(event);
        if app.should_quit()
            || app.directory_browse_request.is_some()
            || app.save_file_browse_request.is_some()
            || app.modpack_browse_request.is_some()
        {
            break;
        }
    }
}

impl Page {
    pub(crate) const ALL: [Self; 7] = [
        Self::Overview,
        Self::Instances,
        Self::DownloadCenter,
        Self::Tasks,
        Self::Accounts,
        Self::Logs,
        Self::Settings,
    ];

    fn title_key(self) -> &'static str {
        match self {
            Self::Overview => "page.overview",
            Self::Instances => "page.instances",
            Self::Accounts => "page.accounts",
            Self::DownloadCenter => "page.download",
            Self::Tasks => "page.tasks",
            Self::Logs => "page.logs",
            Self::Settings => "page.settings",
        }
    }
}

struct PendingModpack {
    root: PathBuf,
    name: String,
    /// Artwork of the project the pack came from, when it came from one.
    ///
    /// Travels with the setup because the setup outlives the download centre:
    /// the archive is fetched first and the install is prepared afterwards, so
    /// the project page is long gone by then.
    icon_url: Option<String>,
}

/// The file a modpack install starts from, with the artwork of the project it
/// was picked from.
///
/// Kept as one value rather than a second `App` field so an icon can never
/// outlive its file: a cancelled form or a local import drops both together,
/// and the next pack cannot inherit the previous project's cover.
struct PendingModpackFile {
    file: ResourceFile,
    icon_url: Option<String>,
}

pub struct App {
    page: Page,
    theme: Theme,
    i18n: I18n,
    status: Status,
    quit: bool,
    instances: InstancePage,
    /// Decoded instance covers. Draw-time only; the list asks for one per
    /// frame and the cache answers from memory.
    covers: std::cell::RefCell<state::covers::CoverCache>,
    /// Worlds of each instance, for the world panel under the facts. Same
    /// draw-time cache as `covers`.
    worlds: std::cell::RefCell<state::worlds::WorldCache>,
    accounts: AccountPage,
    task_snapshots: Vec<TaskSnapshot>,
    speed_tracker: state::tasks::SpeedTracker,
    /// Whether the running Java scan was asked for by the player.
    ///
    /// Every game-directory switch and every runtime install rescans, and
    /// announcing each result buried the message the action itself produced
    /// ("switched to …", "removed …") under "found N runtimes".
    announce_java_scan: bool,
    /// Whether a Java scan is still running; a second one would only race it.
    java_scan_in_flight: bool,
    /// The inputs of the last scan, so an unchanged reload can skip it.
    java_scan_key: Option<(Vec<std::path::PathBuf>, Option<std::path::PathBuf>)>,
    /// When the last scan started, for [`JAVA_SCAN_IDLE`].
    java_scanned_at: Option<std::time::Instant>,
    /// Previous scan results, so a refresh only re-reads changed instances.
    instance_cache: state::data::InstanceScanCache,
    /// Last cache eviction sweep; the sweep is throttled to once per few
    /// minutes because `reload` runs after every task.
    last_cache_sweep: Option<std::time::Instant>,
    /// Updates found by the last check, waiting for the confirmation.
    pending_mod_updates: Vec<crate::core::content::update::ModUpdate>,
    logs: LogBuffer,
    launcher_logs: LogBuffer,
    task_engine: TaskEngine,
    log_filter: Option<String>,
    /// Live query for the cross-category settings search. `None` means the
    /// normal category list is shown.
    settings_search: Option<String>,
    java: Vec<JavaInstallation>,
    java_selected: usize,
    versions: Vec<VersionSummary>,
    version_selected: usize,
    version_scroll: usize,
    detail_scroll: usize,
    pane_scroll: usize,
    /// Last scroll range the detail pane reported while drawing. Wheel events
    /// clamp against it so overscrolling cannot accumulate past the end.
    pane_max_scroll: Cell<usize>,
    terminal_height: u16,
    task_filter: TaskFilter,
    task_selected: usize,
    task_selected_id: Option<Uuid>,
    config: Config,
    settings: SettingsState,
    paths: DataPaths,
    sender: tokio::sync::mpsc::UnboundedSender<AppEvent>,
    modal: Option<Modal>,
    help_open: bool,
    help_scroll: usize,
    regions: Cell<ChromeRegions>,
    regions_size: Cell<(u16, u16)>,
    /// `nightlight` palette cache: minute, theme, preset, mode, contrast.
    nightlight_cache: view::nightlight::PaletteCache,
    /// Test-only: how many times the palette was actually derived.
    #[cfg(test)]
    nightlight_computes: Cell<usize>,
    /// Test-only clock override: snapshots must not move with the wall clock.
    #[cfg(test)]
    nightlight_minute: Cell<Option<u32>>,
    /// Regions the last frame registered; the mouse reads them back.
    hitmap: Hitmap,
    /// Where each list's window sat on the last frame it was drawn. Drawing
    /// writes it back so a selection that is already on screen never scrolls the
    /// list under the pointer.
    row_windows: RowWindows,
    /// Target currently under the pointer, if any.
    hover: Option<Action>,
    /// Previous click, for double-click detection.
    last_click: Option<(
        Action,
        crate::tui::app::state::apply::ClickContext,
        std::time::Instant,
    )>,
    /// Bumped on every resize so stale hit regions are ignored.
    resize_gen: u64,
    detail_open: bool,
    /// Second-level instance screen; `None` when the list is showing.
    workspace: Option<crate::tui::pages::workspace::Workspace>,
    search: Option<SearchState>,
    /// Query that produced the visible resource results; `Esc` in the inline
    /// search bar restores it instead of leaving unsubmitted text behind.
    search_backup: Option<String>,
    instance_filter: usize,
    directory_browse_request: Option<PathBuf>,
    /// File picker request for the local-modpack import form.
    modpack_browse_request: Option<PathBuf>,
    save_file_browse_request: Option<(PathBuf, String)>,
    pending_log_export: Option<LogExportKind>,
    last_crash_report: Option<PathBuf>,
    update_check_in_flight: bool,
    terminal_width: u16,
    download_target_id: Option<Uuid>,
    download_center: DownloadCenterState,
    /// Cached category lists, keyed by `(source, kind)`.
    resource_categories: state::downloads_categories::CategoryCache,
    /// Sequence for category fetches; independent of `download_center.request_id`.
    category_request_seq: u64,
    /// True while the category filter dialog is the open modal.
    filter_open: bool,
    /// True while the version-kind filter dialog is the open modal.
    version_filter_open: bool,
    resource_projects: Vec<ResourceProject>,
    resource_files: Vec<ResourceFile>,
    resource_detail: Option<String>,
    resource_detail_project: Option<ResourceProject>,
    pending_world_file: Option<ResourceFile>,
    pending_resource_file: Option<ResourceFile>,
    pending_modpack: Option<PendingModpack>,
    /// Job id shared by the steps of one modpack install. The download task is
    /// submitted before the instance exists, so the id is parked here until the
    /// loader and file tasks are queued.
    modpack_job: Option<Uuid>,
    /// Loader install waiting for the confirmation dialog.
    pending_loader_install: Option<state::workspace_install::PendingLoaderInstall>,
    pending_modpack_file: Option<PendingModpackFile>,
    /// Wall clock used by the animated preset; `classic` ignores it.
    started: std::time::Instant,
    /// False when `TMCL_NO_ANIM` is set, so the preset can freeze its motion.
    motion: bool,
    /// Instance currently starting or running, cleared once the game exits.
    active_launch: Option<ActiveLaunch>,
    /// Instance waiting for its repair task to finish before launching again.
    pending_launch: Option<Uuid>,
    /// True for the single launch attempt that follows a repair, so a second
    /// missing-file round stops instead of repairing forever.
    launch_after_repair: bool,
    /// Recent status messages shown as toasts: text, kind, and when it appeared.
    status_toasts:
        std::cell::RefCell<std::collections::VecDeque<(String, StatusKind, std::time::Instant)>>,
}

/// A game process the launcher is waiting on. `creeper` lists it in the
/// activity feed so a launch is visible while no install task is running.
#[derive(Debug, Clone)]
pub(in crate::tui::app) struct ActiveLaunch {
    /// Which instance the process belongs to. The name alone is not enough:
    /// two game directories can hold same-named versions, and the list marks
    /// rows by id.
    pub instance_id: Uuid,
    pub instance_name: String,
    pub phase: LaunchPhase,
    pub started: std::time::Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::tui::app) enum LaunchPhase {
    Starting,
    Running,
}

impl ActiveLaunch {
    pub(in crate::tui::app) fn starting(instance_id: Uuid, instance_name: String) -> Self {
        Self {
            instance_id,
            instance_name,
            phase: LaunchPhase::Starting,
            started: std::time::Instant::now(),
        }
    }
}

impl App {
    /// An app with throwaway data paths.
    ///
    /// Only tests call this: the task store is a single JSON file and its writes
    /// are atomic, so two apps sharing one directory would fight over the same
    /// rename. A unique directory per app keeps the harness parallel-safe.
    pub fn new() -> Self {
        let (sender, _) = tokio::sync::mpsc::unbounded_channel();
        Self::with_state(
            DataPaths::from_appdata(
                std::env::temp_dir().join(format!("TMCL-app-{}", uuid::Uuid::new_v4())),
            ),
            Config::default(),
            sender,
        )
    }
    fn with_state(
        paths: DataPaths,
        config: Config,
        sender: tokio::sync::mpsc::UnboundedSender<AppEvent>,
    ) -> Self {
        Self::try_with_state(paths, config, sender).expect("create task engine")
    }

    fn try_with_state(
        paths: DataPaths,
        config: Config,
        sender: tokio::sync::mpsc::UnboundedSender<AppEvent>,
    ) -> Result<Self> {
        let (terminal_width, terminal_height) = crossterm::terminal::size().unwrap_or((80, 24));
        let i18n = I18n::new(config.state.ui.language);
        let status = Status::info(i18n.text("status.ready"));
        let task_engine = TaskEngine::open(
            crate::core::task::store::JsonTaskStore::new(paths.tasks_dir.clone()),
            Arc::new(MinecraftTaskRunner),
            3,
        )?;
        Ok(Self {
            page: Page::Overview,
            theme: Theme::from_ui(&config.state.ui),
            i18n,
            status,
            quit: false,
            instances: InstancePage::default(),
            covers: std::cell::RefCell::new(state::covers::CoverCache::default()),
            worlds: std::cell::RefCell::new(state::worlds::WorldCache::default()),
            accounts: AccountPage::default(),
            task_snapshots: task_engine.snapshots(),
            speed_tracker: state::tasks::SpeedTracker::default(),
            announce_java_scan: false,
            java_scan_in_flight: false,
            java_scan_key: None,
            java_scanned_at: None,
            instance_cache: state::data::InstanceScanCache::default(),
            last_cache_sweep: None,
            pending_mod_updates: Vec::new(),
            logs: LogBuffer::new(4000),
            launcher_logs: LogBuffer::new(2000),
            task_engine,
            log_filter: None,
            settings_search: None,
            java: Vec::new(),
            java_selected: 0,
            versions: Vec::new(),
            version_selected: 0,
            version_scroll: 0,
            detail_scroll: 0,
            pane_scroll: 0,
            pane_max_scroll: Cell::new(0),
            terminal_height,
            task_filter: TaskFilter::All,
            task_selected: 0,
            task_selected_id: None,
            config,
            settings: SettingsState::default(),
            paths,
            sender,
            modal: None,
            help_open: false,
            help_scroll: 0,
            regions: Cell::new(layout_chrome(Rect::new(0, 0, 80, 24))),
            regions_size: Cell::new((0, 0)),
            nightlight_cache: view::nightlight::PaletteCache::default(),
            #[cfg(test)]
            nightlight_computes: Cell::new(0),
            #[cfg(test)]
            nightlight_minute: Cell::new(None),
            hitmap: Hitmap::default(),
            row_windows: RowWindows::default(),
            hover: None,
            last_click: None,
            resize_gen: 0,
            detail_open: false,
            workspace: None,
            search: None,
            search_backup: None,
            instance_filter: 0,
            directory_browse_request: None,
            modpack_browse_request: None,
            save_file_browse_request: None,
            pending_log_export: None,
            last_crash_report: None,
            update_check_in_flight: false,
            terminal_width,
            download_target_id: None,
            download_center: DownloadCenterState::default(),
            resource_categories: state::downloads_categories::CategoryCache::default(),
            category_request_seq: 0,
            filter_open: false,
            version_filter_open: false,
            resource_projects: Vec::new(),
            resource_files: Vec::new(),
            resource_detail: None,
            resource_detail_project: None,
            pending_world_file: None,
            pending_resource_file: None,
            pending_modpack: None,
            modpack_job: None,
            pending_loader_install: None,
            pending_modpack_file: None,
            started: std::time::Instant::now(),
            motion: std::env::var_os("TMCL_NO_ANIM").is_none(),
            active_launch: None,
            pending_launch: None,
            launch_after_repair: false,
            status_toasts: std::cell::RefCell::new(std::collections::VecDeque::new()),
        })
    }

    pub fn load(sender: tokio::sync::mpsc::UnboundedSender<AppEvent>) -> Result<Self> {
        let paths = DataPaths::discover()?;
        paths.ensure_dirs()?;
        let mut config = load_or_default(&paths)?;
        state::configure_download_executor(&config);
        let retention = std::time::Duration::from_secs(
            u64::from(config.logs.retention_days).saturating_mul(24 * 60 * 60),
        );
        let cutoff = std::time::SystemTime::now()
            .checked_sub(retention)
            .unwrap_or(std::time::UNIX_EPOCH);
        let _ = crate::core::config::log_retention::cleanup_logs(&paths.logs_dir, cutoff);
        if config.game_directories.is_empty() {
            config.game_directories.push(executable_directory()?);
            write_json_atomic(&paths.config_file, &config)?;
        }
        let mut app = Self::try_with_state(paths, config, sender)?;
        app.reload()?;
        app.page = Page::Overview;
        Ok(app)
    }

    pub fn start_update_check(&mut self) {
        self.spawn_update_check(true);
    }

    fn retry_update_check(&mut self) {
        self.spawn_update_check(false);
    }

    fn spawn_update_check(&mut self, automatic: bool) {
        if automatic && !self.config.launcher.auto_check_updates {
            return;
        }
        if self.update_check_in_flight {
            return;
        }
        let Some(endpoint) = std::env::var_os("TMCL_UPDATE_URL") else {
            self.status = Status::info(self.i18n.text("status.update_check_not_configured"));
            return;
        };
        let endpoint = endpoint.to_string_lossy().into_owned();
        let timeout = self.config.downloads.timeout_secs;
        let sender = self.sender.clone();
        self.update_check_in_flight = true;
        tokio::spawn(async move {
            let result = async {
                let client = HttpClient::with_timeout(timeout)?;
                crate::core::version::update::check_latest(&client, &endpoint).await
            }
            .await
            .map_err(|error: Error| error.to_string());
            let _ = sender.send(AppEvent::UpdateCheck(result));
        });
    }

    fn reload(&mut self) -> Result<()> {
        self.config = load_or_default(&self.paths)?;
        let exclusions_before = self.config.excluded_instances.len();
        self.config
            .excluded_instances
            .retain(|path| !path.join("tmcl.json").is_file());
        if self.config.excluded_instances.len() != exclusions_before {
            write_json_atomic(&self.paths.config_file, &self.config)?;
        }
        self.i18n = I18n::new(self.config.state.ui.language);
        state::configure_download_executor(&self.config);
        let instances = load_instances(&self.config, &mut self.instance_cache)?;
        self.instances.apply(instances);
        // The disk cannot tell a finished instance from one whose mods are
        // still downloading; the running tasks can.
        self.sync_install_statuses();
        // Nor can it tell one whose game is on screen. Both overlays run here
        // so a reload mid-session cannot wipe them.
        self.sync_launch_status();
        let requested_current = self
            .config
            .state
            .current_instance_id
            .as_deref()
            .and_then(|id| Uuid::parse_str(id).ok());
        let current = requested_current
            .filter(|id| self.instances.items.iter().any(|item| item.id == *id))
            .or_else(|| self.instances.items.first().map(|item| item.id));
        self.instances.set_current(current);
        if let Some(index) =
            current.and_then(|id| self.instances.items.iter().position(|item| item.id == id))
        {
            self.instances.focused = index;
        }
        let normalized_current = current.map(|id| id.to_string());
        if self.config.state.current_instance_id != normalized_current {
            self.config.state.current_instance_id = normalized_current;
            self.save_config()?;
        }
        self.download_target_id = self
            .download_target_id
            .filter(|id| self.instances.items.iter().any(|item| item.id == *id))
            .or(current);
        self.accounts.accounts = AccountStore::new(&self.paths.accounts_file).load()?;
        self.accounts.device_code = None;
        self.accounts.current = self
            .config
            .state
            .current_account_id
            .as_deref()
            .and_then(|id| Uuid::parse_str(id).ok());
        if let Some(current) = self.accounts.current
            && let Some(index) = self
                .accounts
                .accounts
                .iter()
                .position(|account| account.id == current)
        {
            self.accounts.selected = index;
        }
        self.set_task_snapshots(self.task_engine.snapshots());
        // Discovery spawns `java -version` per candidate, so the refresh runs
        // in the background and lands as `AppEvent::JavaScanned`. The list
        // already on screen stays valid until then, and an unchanged answer
        // from a minute ago is reused instead of recomputed.
        self.refresh_java_scan();
        self.enforce_cache_limit();
        Ok(())
    }
}

#[cfg(test)]
mod tests;

pub(crate) mod state;
pub(crate) mod view;

use state::*;

/// Providers used by every download-center background task.
pub(in crate::tui::app) fn resource_registry(
    client: std::sync::Arc<crate::core::version::http::HttpClient>,
) -> crate::core::resource::ProviderRegistry {
    crate::core::resource::registry(client)
}

#[cfg(windows)]
fn open_verification_url(url: &str) -> std::io::Result<()> {
    use std::{ffi::OsStr, os::windows::ffi::OsStrExt};

    use windows::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
    use windows::core::PCWSTR;

    let mut url: Vec<u16> = OsStr::new(url).encode_wide().collect();
    url.push(0);
    let result =
        unsafe { ShellExecuteW(None, None, PCWSTR(url.as_ptr()), None, None, SW_SHOWNORMAL) };
    if result.0 as usize <= 32 {
        return Err(std::io::Error::other(format!(
            "Windows could not open the URL (ShellExecuteW result {})",
            result.0 as usize
        )));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn open_verification_url(url: &str) -> std::io::Result<()> {
    std::process::Command::new("open")
        .arg(url)
        .spawn()
        .map(|_| ())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_verification_url(url: &str) -> std::io::Result<()> {
    std::process::Command::new("xdg-open")
        .arg(url)
        .spawn()
        .map(|_| ())
}

#[cfg(not(any(windows, unix)))]
fn open_verification_url(_url: &str) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "automatic browser opening is not supported on this platform",
    ))
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
