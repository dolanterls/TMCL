//! Settings page: hints, activation and value editing.
//!
//! One key grammar for every row: `←/→` adjusts, `Space` toggles, `Enter`
//! edits or executes. The category rail is navigation only, so `Enter` can
//! never mean "enter category" on one row and "edit value" on the next.

use super::*;
use crate::tui::widgets::settings::SettingKind;

/// How long a background Java refresh trusts its last answer.
///
/// A scan costs one `java -version` subprocess per install, and `reload()` runs
/// after every save and every finished task, so the same answer was being
/// recomputed several times a minute.
const JAVA_SCAN_IDLE: std::time::Duration = std::time::Duration::from_secs(120);

impl App {
    pub(in crate::tui::app) fn settings_hints(&self) -> Vec<crate::tui::chrome::Hint> {
        let mut hints = Vec::new();
        if self.settings.category.is_management() {
            match self.settings.category {
                SettingsCategory::Java => {
                    hints.extend([
                        self.hint("Enter", "settings.set_current"),
                        self.hint("a", "action.add"),
                        self.hint("i", "action.install"),
                        self.hint("x", "action.remove"),
                        self.hint("r", "action.scan"),
                    ]);
                }
                SettingsCategory::GameDirectories => {
                    hints.extend([
                        self.hint("Enter", "settings.open_or_activate"),
                        self.hint("s", "action.scan"),
                        self.hint("x", "action.remove_directory"),
                    ]);
                }
                _ => {}
            }
        } else if let Some(row) = self.current_setting_row() {
            match row.kind {
                SettingKind::Toggle => hints.push(self.hint("Space", "action.toggle")),
                SettingKind::Choice => {
                    hints.push(self.hint("←/→", "action.adjust"));
                    hints.push(self.hint("Enter", "action.toggle"));
                }
                SettingKind::Number { .. } => {
                    hints.push(self.hint("←/→", "action.adjust"));
                    hints.push(self.hint("Enter", "action.edit"));
                }
                SettingKind::Text => hints.push(self.hint("Enter", "action.edit")),
                SettingKind::Action => hints.push(self.hint("Enter", "action.run")),
            }
        }
        hints.push(self.hint("[ ]", "action.category"));
        if !self.settings.category.is_management() {
            hints.push(self.hint("/", "action.search"));
        }
        hints
    }

    pub(in crate::tui::app) fn change_settings_category(&mut self, delta: isize) {
        self.settings.cycle_category(delta);
        self.settings.clamp_selection(self.settings_row_count());
        self.pane_scroll = 0;
    }

    /// `Enter` in the settings search: jump to the highlighted hit and leave
    /// search mode with that row selected.
    pub(in crate::tui::app) fn jump_to_settings_search_hit(&mut self) {
        let results = self.settings_search_results();
        let Some(hit) = results.get(self.settings.selected).cloned() else {
            self.settings_search = None;
            self.status = Status::warning(self.i18n.text("status.no_setting_match"));
            return;
        };
        self.settings.select_category(hit.category);
        self.settings.selected = hit.index;
        self.settings_search = None;
        self.status = Status::info(
            self.i18n
                .format("status.setting_jumped", &[("name", &hit.row.title)]),
        );
    }

    /// `Enter` / `Space`: run the primary action of the current row.
    pub(in crate::tui::app) fn activate_settings_selection(&mut self) {
        if self.settings_search_active() {
            self.jump_to_settings_search_hit();
            return;
        }
        match self.settings.category {
            SettingsCategory::Java => {
                self.select_current_java();
                return;
            }
            SettingsCategory::GameDirectories => {
                match self.settings.selected {
                    0 => self.open_game_directory_form(),
                    1 => self.open_import_form(),
                    index => self.set_active_game_directory(index - 2),
                }
                return;
            }
            _ => {}
        }
        let Some(row) = self.current_setting_row() else {
            return;
        };
        if !row.enabled {
            self.status = Status::warning(row.disabled_reason);
            return;
        }
        match row.kind {
            SettingKind::Toggle => self.toggle_setting(row.id),
            SettingKind::Choice => self.cycle_setting(row.id, 1),
            SettingKind::Number { .. } | SettingKind::Text => self.open_setting_editor(row.id),
            SettingKind::Action => self.activate_setting(row.id),
        }
    }

    /// Kept for the few call sites that activate a known setting by id.
    pub(in crate::tui::app) fn activate_setting(&mut self, setting: SettingId) {
        if self.setting_disabled_reason(setting).is_some() {
            self.status = Status::warning(
                self.i18n.text(
                    self.setting_disabled_reason(setting)
                        .unwrap_or("common.not_available"),
                ),
            );
            return;
        }
        match setting {
            SettingId::CheckForUpdates => self.retry_update_check(),
            SettingId::AddToPath => self.open_add_to_path_confirmation(),
            SettingId::CacheSize => self.open_cache_clear_confirmation(),
            SettingId::GameLogs => self.set_page(Page::Logs),
            SettingId::LauncherLogDirectory => self.open_log_directory(),
            SettingId::ExportLauncherLogs => self.begin_log_export(LogExportKind::Launcher),
            SettingId::ExportGameLogs => self.begin_log_export(LogExportKind::Game),
            SettingId::ExportCrashReport => self.begin_crash_report_export(),
            SettingId::LauncherVisibility
            | SettingId::ProcessPriority
            | SettingId::JavaSelection
            | SettingId::MemoryMode
            | SettingId::WindowMode
            | SettingId::VersionSource
            | SettingId::FileSource
            | SettingId::ThemeMode
            | SettingId::UiPreset
            | SettingId::AccentColor
            | SettingId::NightlightTheme
            | SettingId::PaperTheme
            | SettingId::ShelfTheme
            | SettingId::Language => self.cycle_setting(setting, 1),
            SettingId::AutoCheckUpdates
            | SettingId::AutoDownloadThreads
            | SettingId::HighContrast
            | SettingId::LiveAccent
            | SettingId::Mouse
            | SettingId::AutoRepair
            | SettingId::LauncherLogs
            | SettingId::GameDebug => self.toggle_setting(setting),
            SettingId::ServerAddress
            | SettingId::JvmArguments
            | SettingId::GameArguments
            | SettingId::CacheDirectory
            | SettingId::MemoryMinimum
            | SettingId::MemoryMaximum
            | SettingId::WindowWidth
            | SettingId::WindowHeight
            | SettingId::DownloadThreads
            | SettingId::CacheLimit
            | SettingId::RetryCount
            | SettingId::Timeout
            | SettingId::LogRetention => self.open_setting_editor(setting),
        }
    }

    /// `←/→`: change an adjustable value in place. Text and action rows are
    /// deliberately inert so the key never triggers a dialog by accident.
    pub(in crate::tui::app) fn adjust_current_setting(&mut self, direction: isize) {
        if self.settings.category.is_management() {
            return;
        }
        let Some(row) = self.current_setting_row() else {
            return;
        };
        if !row.enabled {
            self.status = Status::warning(row.disabled_reason);
            return;
        }
        match row.kind {
            SettingKind::Toggle => self.toggle_setting(row.id),
            SettingKind::Choice => self.cycle_setting(row.id, direction),
            SettingKind::Number { step } => self.step_number_setting(row.id, step, direction),
            SettingKind::Text | SettingKind::Action => {}
        }
    }

    /// Number rows step by their own increment and clamp to a sane range; the
    /// same ranges are enforced again when the value is typed into the prompt.
    fn step_number_setting(&mut self, id: SettingId, step: i64, direction: isize) {
        let result = self.save_config_change(|config| match id {
            SettingId::MemoryMinimum => {
                let max = config.launch.max_memory_mb.max(256);
                config.launch.min_memory_mb = step_u64(
                    u64::from(config.launch.min_memory_mb),
                    step,
                    direction,
                    256,
                    u64::from(max),
                ) as u32;
            }
            SettingId::MemoryMaximum => {
                let total = crate::core::launch::memory::detect().total_mb.max(2048);
                let min = config.launch.min_memory_mb.max(256);
                config.launch.max_memory_mb = step_u64(
                    u64::from(config.launch.max_memory_mb),
                    step,
                    direction,
                    u64::from(min),
                    total,
                ) as u32;
            }
            SettingId::WindowWidth => {
                config.launch.width =
                    step_u64(u64::from(config.launch.width), step, direction, 320, 7680) as u32;
            }
            SettingId::WindowHeight => {
                config.launch.height =
                    step_u64(u64::from(config.launch.height), step, direction, 240, 4320) as u32;
            }
            SettingId::DownloadThreads => {
                config.downloads.download_threads = step_u64(
                    config.downloads.download_threads as u64,
                    step,
                    direction,
                    1,
                    64,
                ) as usize;
            }
            SettingId::CacheLimit => {
                config.downloads.cache_limit_mb = step_u64(
                    config.downloads.cache_limit_mb,
                    step,
                    direction,
                    0,
                    1_048_576,
                );
            }
            SettingId::RetryCount => {
                config.downloads.retry_count = step_u64(
                    u64::from(config.downloads.retry_count),
                    step,
                    direction,
                    0,
                    10,
                ) as u8;
            }
            SettingId::Timeout => {
                config.downloads.timeout_secs =
                    step_u64(config.downloads.timeout_secs, step, direction, 5, 300);
            }
            SettingId::LogRetention => {
                config.logs.retention_days = step_u64(
                    u64::from(config.logs.retention_days),
                    step,
                    direction,
                    1,
                    365,
                ) as u32;
            }
            _ => {}
        });
        self.finish_setting_save(result);
    }

    pub(in crate::tui::app) fn toggle_setting(&mut self, setting: SettingId) {
        let result = self.save_config_change(|config| match setting {
            SettingId::AutoCheckUpdates => {
                config.launcher.auto_check_updates = !config.launcher.auto_check_updates
            }
            SettingId::AutoDownloadThreads => {
                config.downloads.auto_download_threads = !config.downloads.auto_download_threads
            }
            SettingId::HighContrast => {
                config.state.ui.high_contrast = !config.state.ui.high_contrast
            }
            SettingId::Mouse => config.state.ui.show_mouse = !config.state.ui.show_mouse,
            SettingId::LiveAccent => config.state.ui.live_accent = !config.state.ui.live_accent,
            SettingId::AutoRepair => config.launch.auto_repair = !config.launch.auto_repair,
            SettingId::LauncherLogs => config.logs.launcher_enabled = !config.logs.launcher_enabled,
            SettingId::GameDebug => config.logs.game_debug = !config.logs.game_debug,
            _ => {}
        });
        self.finish_setting_save(result);
    }

    pub(in crate::tui::app) fn cycle_setting(&mut self, setting: SettingId, direction: isize) {
        let result = self.save_config_change(|config| match setting {
            SettingId::LauncherVisibility => {
                config.launcher.visibility = match (config.launcher.visibility, direction < 0) {
                    (LauncherVisibility::KeepOpen, false) => LauncherVisibility::HideAfterLaunch,
                    (LauncherVisibility::HideAfterLaunch, false) => {
                        LauncherVisibility::ExitAfterLaunch
                    }
                    (LauncherVisibility::ExitAfterLaunch, false) => LauncherVisibility::KeepOpen,
                    (LauncherVisibility::KeepOpen, true) => LauncherVisibility::ExitAfterLaunch,
                    (LauncherVisibility::HideAfterLaunch, true) => LauncherVisibility::KeepOpen,
                    (LauncherVisibility::ExitAfterLaunch, true) => {
                        LauncherVisibility::HideAfterLaunch
                    }
                };
            }
            SettingId::ProcessPriority => {
                config.launcher.process_priority = cycle_enum(
                    config.launcher.process_priority,
                    direction,
                    [
                        ProcessPriority::Default,
                        ProcessPriority::AboveNormal,
                        ProcessPriority::High,
                    ],
                );
            }
            SettingId::JavaSelection => {
                config.java.selection_mode = cycle_enum(
                    config.java.selection_mode,
                    direction,
                    [JavaSelectionMode::Automatic, JavaSelectionMode::Fixed],
                );
            }
            SettingId::MemoryMode => {
                config.launch.memory_mode = cycle_enum(
                    config.launch.memory_mode,
                    direction,
                    [MemoryMode::Automatic, MemoryMode::Manual],
                );
            }
            SettingId::WindowMode => {
                config.launch.fullscreen = !config.launch.fullscreen;
            }
            SettingId::VersionSource => {
                config.downloads.version_list_source =
                    cycle_download_source(config.downloads.version_list_source, direction);
            }
            SettingId::FileSource => {
                config.downloads.file_download_source =
                    cycle_download_source(config.downloads.file_download_source, direction);
            }
            SettingId::ThemeMode => {
                config.state.ui.theme_mode = cycle_enum(
                    config.state.ui.theme_mode,
                    direction,
                    [ThemeMode::Dark, ThemeMode::Light, ThemeMode::Terminal],
                );
            }
            SettingId::UiPreset => {
                config.state.ui.preset = config.state.ui.preset.next(direction);
            }
            SettingId::AccentColor => {
                config.state.ui.accent_color = cycle_enum(
                    config.state.ui.accent_color,
                    direction,
                    [
                        AccentColor::Orange,
                        AccentColor::Cyan,
                        AccentColor::Blue,
                        AccentColor::Green,
                        AccentColor::Yellow,
                        AccentColor::Magenta,
                    ],
                );
            }
            SettingId::Language => {
                config.state.ui.language = cycle_enum(
                    config.state.ui.language,
                    direction,
                    [Language::SimplifiedChinese, Language::English],
                );
            }
            SettingId::NightlightTheme => {
                config.state.ui.nightlight_theme = cycle_enum(
                    config.state.ui.nightlight_theme,
                    direction,
                    NightlightTheme::ALL,
                );
            }
            SettingId::PaperTheme => {
                config.state.ui.paper_theme =
                    cycle_enum(config.state.ui.paper_theme, direction, PaperTheme::ALL);
            }
            SettingId::ShelfTheme => {
                config.state.ui.shelf_theme =
                    cycle_enum(config.state.ui.shelf_theme, direction, ShelfTheme::ALL);
            }
            _ => {}
        });
        if result.is_ok() && setting == SettingId::Language {
            // Rebuild the catalogue before the success message so it is shown
            // in the newly selected language.
            self.i18n = I18n::new(self.config.state.ui.language);
        }
        if result.is_ok() && setting == SettingId::UiPreset {
            // The appearance list is scoped to the preset: switching can take
            // rows away, and a cursor left on one of them would point past the
            // end of the shorter list.
            self.settings.clamp_selection(self.settings_row_count());
        }
        self.finish_setting_save(result);
    }

    fn finish_setting_save(&mut self, result: Result<()>) {
        if let Err(error) = result {
            self.status = Status::error(error.to_string());
        } else {
            self.status = Status::success(self.i18n.text("status.settings_saved"));
        }
    }

    /// Single-value editor for text and number rows. There is no multi-field
    /// settings form any more: a row edits exactly one value.
    pub(in crate::tui::app) fn open_setting_editor(&mut self, id: SettingId) {
        let Some(row) = self.setting_row_for(id) else {
            return;
        };
        let (value, help) = match id {
            SettingId::ServerAddress => (
                self.config
                    .launcher
                    .server_address
                    .clone()
                    .unwrap_or_default(),
                self.i18n.text("settings.help.server_address"),
            ),
            SettingId::JvmArguments => (
                join_args(&self.config.launch.jvm_args),
                self.i18n.text("settings.help.jvm_args"),
            ),
            SettingId::GameArguments => (
                join_args(&self.config.launch.game_args),
                self.i18n.text("settings.help.game_args"),
            ),
            SettingId::CacheDirectory => (
                self.config
                    .downloads
                    .cache_dir
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                self.i18n.text("settings.help.cache_directory"),
            ),
            SettingId::MemoryMinimum => (
                self.config.launch.min_memory_mb.to_string(),
                self.i18n.text("settings.help.memory_min"),
            ),
            SettingId::MemoryMaximum => (
                self.config.launch.max_memory_mb.to_string(),
                self.i18n.text("settings.help.memory_max"),
            ),
            SettingId::WindowWidth => (
                self.config.launch.width.to_string(),
                self.i18n.text("settings.help.window_width"),
            ),
            SettingId::WindowHeight => (
                self.config.launch.height.to_string(),
                self.i18n.text("settings.help.window_height"),
            ),
            SettingId::DownloadThreads => (
                self.config.downloads.download_threads.to_string(),
                self.i18n.text("settings.help.download_threads"),
            ),
            SettingId::CacheLimit => (
                self.config.downloads.cache_limit_mb.to_string(),
                self.i18n.text("settings.help.cache_limit"),
            ),
            SettingId::RetryCount => (
                self.config.downloads.retry_count.to_string(),
                self.i18n.text("settings.help.retry_count"),
            ),
            SettingId::Timeout => (
                self.config.downloads.timeout_secs.to_string(),
                self.i18n.text("settings.help.timeout"),
            ),
            SettingId::LogRetention => (
                self.config.logs.retention_days.to_string(),
                self.i18n.text("settings.help.log_retention"),
            ),
            _ => return,
        };
        let action = FormAction::SettingValue(id);
        let value_label = self.i18n.text("settings.value");
        if id == SettingId::CacheDirectory {
            self.modal = Some(
                Modal::form(&row.title, &[value_label], &[value], action).with_directory_picker(0),
            );
        } else {
            self.modal = Some(Modal::prompt(
                row.title.clone(),
                value_label,
                help,
                value,
                action,
            ));
        }
    }

    /// Applies the single value produced by [`open_setting_editor`]. Every
    /// range is validated here as well, so typed values cannot bypass the
    /// clamps used by `←/→`.
    pub(in crate::tui::app) fn apply_setting_value(
        &mut self,
        id: SettingId,
        raw: &str,
    ) -> Result<()> {
        let value = raw.trim();
        let mut next = self.config.clone();
        match id {
            SettingId::ServerAddress => {
                next.launcher.server_address = if value.is_empty() {
                    None
                } else {
                    validate_server_address(value)?;
                    Some(value.to_owned())
                };
            }
            SettingId::CacheDirectory => {
                next.downloads.cache_dir = if value.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(value))
                };
            }
            SettingId::JvmArguments => next.launch.jvm_args = split_args(value),
            SettingId::GameArguments => next.launch.game_args = split_args(value),
            SettingId::MemoryMinimum => {
                let parsed = parse_number::<u32>(value, "minimum memory")?;
                let max = next.launch.max_memory_mb.max(256);
                if !(256..=max).contains(&parsed) {
                    return Err(Error::Config(format!(
                        "minimum memory must be between 256 and {max} MB"
                    )));
                }
                next.launch.min_memory_mb = parsed;
            }
            SettingId::MemoryMaximum => {
                let parsed = parse_number::<u32>(value, "maximum memory")?;
                let min = u64::from(next.launch.min_memory_mb.max(256));
                let max = crate::core::launch::memory::detect().total_mb.max(min);
                if !(min..=max).contains(&u64::from(parsed)) {
                    return Err(Error::Config(format!(
                        "maximum memory must be between {min} and {max} MB"
                    )));
                }
                next.launch.max_memory_mb = parsed;
            }
            SettingId::WindowWidth => {
                next.launch.width = parse_ranged::<u32>(value, "window width", 320, 7680)?;
            }
            SettingId::WindowHeight => {
                next.launch.height = parse_ranged::<u32>(value, "window height", 240, 4320)?;
            }
            SettingId::DownloadThreads => {
                next.downloads.download_threads =
                    parse_ranged::<usize>(value, "download threads", 1, 64)?;
            }
            SettingId::CacheLimit => {
                next.downloads.cache_limit_mb =
                    parse_ranged::<u64>(value, "cache limit", 0, 1_048_576)?;
            }
            SettingId::RetryCount => {
                next.downloads.retry_count = parse_ranged::<u8>(value, "retry count", 0, 10)?;
            }
            SettingId::Timeout => {
                next.downloads.timeout_secs =
                    parse_ranged::<u64>(value, "timeout seconds", 5, 300)?;
            }
            SettingId::LogRetention => {
                next.logs.retention_days =
                    parse_ranged::<u32>(value, "log retention days", 1, 365)?;
            }
            _ => return Err(Error::Config("this setting is not editable".into())),
        }
        self.commit_config(next)?;
        self.status = Status::success(self.i18n.text("status.settings_saved"));
        Ok(())
    }

    pub(in crate::tui::app) fn cache_root(&self) -> PathBuf {
        self.config
            .downloads
            .cache_dir
            .clone()
            .unwrap_or_else(|| self.paths.cache_dir.clone())
    }

    pub(in crate::tui::app) fn game_directory_instance_count(&self, directory: &Path) -> usize {
        let directory = normalize_instance_path(directory);
        self.instances
            .items
            .iter()
            .filter(|instance| {
                crate::core::instance::path::game_directory_of(&instance.path)
                    .is_ok_and(|root| normalize_instance_path(&root) == directory)
            })
            .count()
    }

    pub(in crate::tui::app) fn scan_game_directory(&mut self, directory_index: usize) {
        let Some(path) = self.config.game_directories.get(directory_index).cloned() else {
            self.status = Status::warning(self.i18n.text("status.select_directory"));
            return;
        };
        match self.reload() {
            Ok(()) => {
                let count = self.game_directory_instance_count(&path);
                self.status = Status::info(self.i18n.format(
                    "status.directory_scanned",
                    &[
                        ("path", &display_path(&path)),
                        ("count", &count.to_string()),
                    ],
                ));
            }
            Err(error) => self.status = Status::error(error.to_string()),
        }
    }

    pub(in crate::tui::app) fn cache_stats_summary(&self) -> String {
        match cache::stats(&self.cache_root()) {
            Ok(stats) => self.i18n.format(
                "settings.cache_size_value",
                &[
                    ("files", &stats.files.to_string()),
                    ("bytes", &format_size(stats.bytes)),
                ],
            ),
            Err(_) => self.i18n.text("common.not_available"),
        }
    }

    /// Trims the download cache to its configured limit, off the UI thread.
    ///
    /// Called from reload, which runs after every task. The sweep is throttled
    /// and only reports back when something was actually freed, so a normal
    /// refresh never replaces the status line with a cache message.
    pub(in crate::tui::app) fn enforce_cache_limit(&mut self) {
        let limit_mb = self.config.downloads.cache_limit_mb;
        if limit_mb == 0 {
            return;
        }
        let now = std::time::Instant::now();
        if self
            .last_cache_sweep
            .is_some_and(|last| now.duration_since(last) < std::time::Duration::from_secs(300))
        {
            return;
        }
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        self.last_cache_sweep = Some(now);
        let root = self.cache_root();
        let sender = self.sender.clone();
        handle.spawn_blocking(move || {
            let limit = limit_mb.saturating_mul(1024 * 1024);
            // Ten minutes of protection: a file written by the download that
            // just finished is about to be copied into an instance.
            match cache::evict_to(&root, limit, std::time::Duration::from_secs(600)) {
                Ok(report) if report.removed_bytes > 0 => {
                    let _ = sender.send(crate::tui::event::AppEvent::CacheEvicted {
                        files: report.removed_files,
                        bytes: report.removed_bytes,
                    });
                }
                _ => {}
            }
        });
    }

    /// Confirms before touching a system-wide-ish value: the dialog names the
    /// exact directory and says a new terminal is required, because a PATH edit
    /// that silently appears to do nothing is the whole reason this exists.
    pub(in crate::tui::app) fn open_add_to_path_confirmation(&mut self) {
        let directory = crate::core::shell::path::launcher_directory()
            .map(|path| display_path(&path))
            .unwrap_or_else(|_| "-".to_owned());
        self.modal = Some(Modal::confirm(
            self.i18n
                .format("confirm.add_to_path", &[("directory", &directory)]),
            ConfirmAction::AddToPath,
        ));
    }

    /// One line at startup when this executable cannot be found by name.
    ///
    /// A status message on purpose: the next thing the user does replaces it,
    /// and once the directory is on PATH it is not shown at all.
    pub(in crate::tui) fn announce_missing_path_entry(&mut self) {
        let Ok(status) = crate::core::shell::path::status() else {
            return;
        };
        if status.presence == crate::core::shell::path::Presence::Present {
            return;
        }
        // The settings row names the directory; this line only has to make the
        // user look there, so it stays short enough for the status bar.
        self.status = Status::info(self.i18n.text("status.path_missing"));
    }

    pub(in crate::tui::app) fn open_cache_clear_confirmation(&mut self) {
        let path = display_path(&self.cache_root());
        self.modal = Some(Modal::confirm(
            self.i18n.format("confirm.clear_cache", &[("path", &path)]),
            ConfirmAction::ClearDownloadCache,
        ));
    }

    /// Kicks off Java discovery on a blocking thread pool.
    ///
    /// `discover_default` runs `java -version` for every candidate it cannot
    /// read a `release` file for, so it is a subprocess per install - never
    /// something to do while the player waits for a frame. HMCL makes the same
    /// call through `Task.supplyAsync` and applies the answer on the UI thread
    /// afterwards; the result comes back here as [`AppEvent::JavaScanned`].
    pub(in crate::tui::app) fn start_java_scan(&mut self) {
        // Tests build an `App` outside a runtime; there is nothing to schedule
        // on there, and the empty list is the same answer a scan would give.
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let runtime_dir = self.paths.runtime_dir.clone();
        let game_directories = self.config.game_directories.clone();
        let selected = self.config.java.selected_path.clone();
        let sender = self.sender.clone();
        self.java_scan_in_flight = true;
        self.java_scan_key = Some((game_directories.clone(), selected.clone()));
        self.java_scanned_at = Some(std::time::Instant::now());
        runtime.spawn(async move {
            let discovered = tokio::task::spawn_blocking(move || {
                discover_default(&runtime_dir, game_directories, selected)
            })
            .await
            .unwrap_or_default();
            let _ = sender.send(crate::tui::event::AppEvent::JavaScanned(discovered));
        });
    }

    /// Whether a background refresh would find anything new.
    ///
    /// `discover_default` spawns one `java -version` per candidate, and
    /// `reload()` runs after every config save and every finished task. Neither
    /// the game directories nor the chosen runtime change that often, so a
    /// fresh result for the same inputs is simply reused.
    pub(in crate::tui::app) fn java_scan_due(&self) -> bool {
        if self.java_scan_in_flight {
            return false;
        }
        let key = (
            self.config.game_directories.clone(),
            self.config.java.selected_path.clone(),
        );
        if self.java_scan_key.as_ref() != Some(&key) {
            return true;
        }
        self.java_scanned_at
            .is_none_or(|at| at.elapsed() >= JAVA_SCAN_IDLE)
    }

    /// Refresh the Java list only when the answer can have changed.
    pub(in crate::tui::app) fn refresh_java_scan(&mut self) {
        if self.java_scan_due() {
            self.start_java_scan();
        }
    }

    pub(in crate::tui::app) fn apply_java_scan(&mut self, java: Vec<JavaInstallation>) {
        self.java = java;
        self.java_selected = 0;
        self.java_scan_in_flight = false;
        // A background rescan keeps its results to itself: the list on screen
        // already shows them, and the player is reading the message the action
        // produced (switched directory, installed runtime…).
        if std::mem::take(&mut self.announce_java_scan) {
            self.status = Status::info(self.i18n.format(
                "status.java_found",
                &[("count", &self.java.len().to_string())],
            ));
        }
    }

    /// Rescan because the player asked for it: this one reports back.
    pub(in crate::tui::app) fn scan_java(&mut self) {
        self.announce_java_scan = true;
        self.status = Status::info(self.i18n.text("status.java_scanning"));
        // A scan already running answers this request; starting a second one
        // would only make two subprocess rounds race for the same list.
        if !self.java_scan_in_flight {
            self.start_java_scan();
        }
    }

    pub(in crate::tui::app) fn is_managed_java(&self, java: &JavaInstallation) -> bool {
        let Ok(runtime_root) = self.paths.runtime_dir.canonicalize() else {
            return false;
        };
        let Ok(executable) = java.executable.canonicalize() else {
            return false;
        };
        executable.starts_with(runtime_root)
    }

    pub(in crate::tui::app) fn select_current_java(&mut self) {
        let Some(java) = self.java.get(self.java_selected).cloned() else {
            self.status = Status::warning(self.i18n.text("status.scan_java_first"));
            return;
        };
        let mut next = self.config.clone();
        next.java.selected_path = Some(java.executable.clone());
        if let Err(error) = self.commit_config(next) {
            self.status = Status::error(error.to_string());
        } else {
            self.status = Status::info(self.i18n.format(
                "status.java_selected",
                &[
                    ("major", &java.major.to_string()),
                    ("path", &display_path(&java.executable)),
                ],
            ));
        }
    }

    pub(in crate::tui::app) fn set_active_game_directory(&mut self, directory_index: usize) {
        let Some(path) = self.config.game_directories.get(directory_index).cloned() else {
            self.status = Status::info(self.i18n.text("status.add_directory"));
            return;
        };
        let mut next = self.config.clone();
        add_game_directory(&mut next, &path);
        if let Err(error) = self.commit_config(next).and_then(|()| self.reload()) {
            self.status = Status::error(error.to_string());
        } else {
            self.settings
                .select_category(SettingsCategory::GameDirectories);
            self.settings.selected = directory_index + 2;
            self.status = Status::info(
                self.i18n
                    .format("status.directory_active", &[("path", &display_path(&path))]),
            );
        }
    }

    /// Per-category letter shortcuts for the management pages. Every one of
    /// these is printed in the footer; settings rows have no hidden keys.
    pub(in crate::tui::app) fn run_setting_shortcut(&mut self, character: char) {
        match (self.settings.category, character) {
            (SettingsCategory::Java, 'a') => self.open_java_add_form(),
            (SettingsCategory::Java, 'i') => self.open_java_runtime_form(),
            (SettingsCategory::Java, 'x') => self.open_remove_confirmation(),
            (SettingsCategory::Java, 'r') => self.scan_java(),
            (SettingsCategory::GameDirectories, 's') if self.settings.selected >= 2 => {
                self.scan_game_directory(self.settings.selected - 2);
            }
            (SettingsCategory::GameDirectories, 'x') if self.settings.selected >= 2 => {
                self.open_remove_confirmation();
            }
            _ => {}
        }
    }
}

fn step_u64(value: u64, step: i64, direction: isize, min: u64, max: u64) -> u64 {
    let delta = step.saturating_mul(direction as i64);
    let next = (value as i64).saturating_add(delta);
    next.clamp(min as i64, max as i64) as u64
}

fn parse_number<T>(value: &str, field: &str) -> Result<T>
where
    T: std::str::FromStr,
{
    value
        .parse::<T>()
        .map_err(|_| Error::Config(format!("{field} must be a number")))
}

fn parse_ranged<T>(value: &str, field: &str, min: T, max: T) -> Result<T>
where
    T: std::str::FromStr + PartialOrd + Copy + std::fmt::Display,
{
    let parsed = parse_number::<T>(value, field)?;
    if parsed < min || parsed > max {
        return Err(Error::Config(format!(
            "{field} must be between {min} and {max}"
        )));
    }
    Ok(parsed)
}

pub(crate) fn command(
    _mode: crate::tui::command::Mode,
    key: crossterm::event::KeyEvent,
) -> crate::tui::command::Command {
    use crate::tui::command::Command;
    use crossterm::event::KeyCode;

    let plain = key.modifiers.is_empty();
    match key.code {
        KeyCode::Left => Command::AdjustSetting(-1),
        KeyCode::Right => Command::AdjustSetting(1),
        KeyCode::Char('[') | KeyCode::Char('h') => Command::PrevBlock,
        KeyCode::Char(']') | KeyCode::Char('l') => Command::NextBlock,
        KeyCode::Char('/') => Command::Search,
        KeyCode::Char(' ') => Command::ActivateSetting,
        KeyCode::Char(character @ ('a' | 'i' | 'r' | 's' | 'x')) if plain => {
            Command::SettingAction(character)
        }
        _ => Command::Nothing,
    }
}
