use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use crossterm::event::{self, Event, KeyEvent, MouseEvent, MouseEventKind};
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;

use std::path::PathBuf;

use crate::core::instance::models::InstanceConfig;
use crate::core::launch::process::GameEvent;
use crate::core::resource::model::{ResourceFile, ResourceSearchPage, ResourceSource};
use crate::core::task::TaskSnapshot;
use crate::core::version::{
    loader_meta::{LoaderKind, LoaderVersion},
    models::VersionSummary,
};

#[derive(Debug)]
pub struct PreparedModpack {
    pub instance: InstanceConfig,
    pub root: PathBuf,
    pub name: String,
    pub loader: String,
    pub loader_version: String,
    pub files: Vec<crate::core::task::ModpackFileSpec>,
    /// Manifest entries that could not be resolved; the install still runs.
    pub unresolved: Vec<String>,
}

/// Result of a world job, carrying only what the status line needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldJobOutcome {
    BackedUp(std::path::PathBuf),
    Restored { safety: std::path::PathBuf },
    Cloned(std::path::PathBuf),
}

impl WorldJobOutcome {
    /// File name of the archive or directory the job produced.
    pub fn name(&self) -> String {
        let path = match self {
            Self::BackedUp(path) | Self::Cloned(path) => path,
            Self::Restored { safety } => safety,
        };
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned()
    }
}

/// One variant carries the prepared modpack payload, which is much larger
/// than a key event. Boxing it would touch every construction and match site
/// for no measurable gain, so the size difference is accepted.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum AppEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize(u16, u16),
    Tick,
    Status(String),
    UpdateCheck(std::result::Result<crate::core::version::update::UpdateInfo, String>),
    /// Result of a background Java discovery scan.
    JavaScanned(Vec<crate::core::java::JavaInstallation>),
    /// Cache trimmed to its configured limit; only sent when bytes were freed.
    CacheEvicted {
        files: u64,
        bytes: u64,
    },
    /// Result of applying a newer modpack archive to an instance.
    ModpackUpdated {
        instance: uuid::Uuid,
        result: std::result::Result<crate::core::resource::modpack_update::UpdateOutcome, String>,
    },
    /// Result of a mod update check for one instance.
    ModUpdatesChecked {
        instance: uuid::Uuid,
        result: std::result::Result<crate::core::content::update::UpdateReport, String>,
    },
    TerminalError(String),
    Reload,
    DeviceCode {
        user_code: String,
        verification_uri: String,
        expires_in: u64,
        browser_status: String,
    },
    VersionsLoaded {
        versions: Vec<VersionSummary>,
        from_cache: bool,
    },
    VersionsLoadFailed {
        for_picker: bool,
        error: String,
    },
    ResourceSearchLoaded {
        request_id: u64,
        source: ResourceSource,
        result: ResourceSearchPage,
    },
    /// Result of a background content scan of one workspace category.
    WorkspaceScanned {
        /// Which instance was scanned: the workspace can be closed or
        /// pointed at another instance before the answer arrives.
        instance: uuid::Uuid,
        kind: crate::core::content::ContentKind,
        entries: Vec<crate::core::content::ContentEntry>,
        stamp: Option<(std::time::SystemTime, usize)>,
        /// Which shader loader the scan found; only meaningful for shaders.
        shader_loader: Option<crate::core::content::shader::ShaderLoader>,
    },
    /// Loader versions fetched for the instance workspace install screen.
    WorkspaceLoaderVersions {
        kind: crate::core::instance::config::InstallableLoader,
        minecraft_version: String,
        result: std::result::Result<Vec<crate::core::version::loader_meta::LoaderVersion>, String>,
    },
    /// A world backup / restore / clone finished off the UI thread.
    WorldJob(Result<WorldJobOutcome, String>),
    /// An instance export finished off the UI thread.
    ModpackExported(std::result::Result<std::path::PathBuf, String>),
    /// Recursive size of a workspace instance, computed off the UI thread.
    WorkspaceSize {
        instance: uuid::Uuid,
        bytes: u64,
    },
    ResourceSearchFailed {
        request_id: u64,
        source: ResourceSource,
        error: String,
    },
    /// Category list for one `(source, kind)`, fetched for the filter dialog.
    ResourceCategoriesLoaded {
        request_id: u64,
        source: ResourceSource,
        kind: crate::core::resource::model::ResourceKind,
        result: std::result::Result<Vec<crate::core::resource::model::ResourceCategory>, String>,
    },
    ResourceFilesLoaded {
        request_id: u64,
        source: ResourceSource,
        project_id: String,
        files: Vec<ResourceFile>,
    },
    ResourceFilesFailed {
        request_id: u64,
        source: ResourceSource,
        error: String,
    },
    LoaderVersionsLoaded {
        minecraft_version: String,
        kind: LoaderKind,
        result: std::result::Result<Vec<LoaderVersion>, String>,
    },
    TasksUpdated(Vec<TaskSnapshot>),
    ModpackPrepared(std::result::Result<PreparedModpack, String>),
    /// Required dependencies resolved for a mod that was just queued.
    DependenciesResolved {
        parent: String,
        target_id: uuid::Uuid,
        files: Vec<crate::core::resource::model::ResourceFile>,
        unresolved: Vec<String>,
    },
    Game(GameEvent),
    LaunchFinished {
        instance_id: uuid::Uuid,
        instance_directory: std::path::PathBuf,
        play_time_seconds: u64,
        exit_code: Option<i32>,
        log_path: std::path::PathBuf,
        crash_report: Option<std::path::PathBuf>,
    },
    /// The launch never reached the game process (bad Java, auth failure, …).
    LaunchAborted {
        error: String,
    },
}

pub struct ReaderStop(Arc<AtomicBool>);

impl Drop for ReaderStop {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

pub fn spawn_reader(
    sender: UnboundedSender<AppEvent>,
) -> (ReaderStop, JoinHandle<std::result::Result<(), String>>) {
    let running = Arc::new(AtomicBool::new(true));
    let reader_running = Arc::clone(&running);
    let task = tokio::task::spawn_blocking(move || {
        // Any-event mouse mode reports every cell crossing; the pointer staying
        // inside one cell is not news for hover.
        let mut last_moved: Option<(u16, u16)> = None;
        while reader_running.load(Ordering::Relaxed) {
            let ready = match event::poll(Duration::from_millis(100)) {
                Ok(ready) => ready,
                Err(error) => {
                    let _ = sender.send(AppEvent::TerminalError(format!("poll terminal: {error}")));
                    break;
                }
            };
            if !ready {
                continue;
            }
            let event = match event::read() {
                Ok(event) => event,
                Err(error) => {
                    let _ = sender.send(AppEvent::TerminalError(format!("read terminal: {error}")));
                    break;
                }
            };
            let event = match event {
                Event::Key(key) => Some(AppEvent::Key(key)),
                Event::Mouse(mouse) => {
                    if mouse.kind == MouseEventKind::Moved {
                        let cell = (mouse.column, mouse.row);
                        if last_moved == Some(cell) {
                            None
                        } else {
                            last_moved = Some(cell);
                            Some(AppEvent::Mouse(mouse))
                        }
                    } else {
                        Some(AppEvent::Mouse(mouse))
                    }
                }
                Event::Resize(width, height) => Some(AppEvent::Resize(width, height)),
                _ => None,
            };
            if let Some(event) = event
                && sender.send(event).is_err()
            {
                break;
            }
        }
        Ok(())
    });
    (ReaderStop(running), task)
}
