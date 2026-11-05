//! Data-driven settings rows.
//!
//! Each category is a static [`SettingSpec`] table; the value, description,
//! disabled reason and row list all derive from the same `SettingId`, so
//! titles, ordering, activation and the rendered list cannot drift apart.

use super::*;
use crate::tui::widgets::settings::{SettingKind, SettingRow};

#[derive(Debug, Clone, Copy)]
struct SettingSpec {
    id: SettingId,
    title: &'static str,
    description: &'static str,
    kind: SettingKind,
    /// Presets whose skin actually reads this value. Empty means every preset
    /// does; a row scoped to another preset is left out of the list entirely.
    presets: &'static [UiPreset],
}

const fn spec(
    id: SettingId,
    title: &'static str,
    description: &'static str,
    kind: SettingKind,
) -> SettingSpec {
    SettingSpec {
        id,
        title,
        description,
        kind,
        presets: &[],
    }
}

// Scope follows the paint code, not the preset's name:
//
// * `app/view/classic/*` and `modal/classic.rs` paint with `self.theme`, which
//   is `Theme::from_ui(&ui)` (`tui/theme.rs`). That function reads `theme_mode`
//   (dark / light / terminal), then overwrites `accent` from `accent_color` and
//   brightens `dim` when `high_contrast` is on. So all three reach classic.
// * `app/view/paper/tone.rs` builds its palette from `paper_theme` and
//   `theme_mode` only — paper owns its accents, so neither the accent row nor
//   the contrast row would do anything there.
// * `app/view/nightlight/tone.rs` reads `nightlight_theme`, `theme_mode`,
//   `high_contrast`, and seeds the accent from `accent_color` / `live_accent`.
// * `app/view/creeper/*` and `modal/creeper.rs` paint from the `palette`
//   constants; `self.theme` never enters a creeper frame, so nothing here
//   changes a creeper screen.
// * `app/view/shelf/tone.rs` is a fixed palette too, but it carries a scheme
//   axis of its own (`shelf_theme`). Shelf reads neither `accent_color` nor
//   `high_contrast`; it reads `theme_mode` for day / night and the scheme row
//   for the binding.
//
// Reading only `nightlight/` when this table was first written put the three
// shared rows behind nightlight and took them away from classic, which paints
// with them every frame. Check the paint path before scoping a row.

/// Nightlight is the only skin with a theme of its own and the live accent.
const NIGHTLIGHT: &[UiPreset] = &[UiPreset::Nightlight];
/// The stock row belongs to paper.
const PAPER: &[UiPreset] = &[UiPreset::Paper];
/// The binding row belongs to shelf.
const SHELF: &[UiPreset] = &[UiPreset::Shelf];
/// `Theme::from_ui` (classic) and `tone::palette` (nightlight) both read the
/// accent and the contrast flag.
const THEMED: &[UiPreset] = &[UiPreset::Classic, UiPreset::Nightlight];
/// Day / night reaches every preset: classic's `Theme`, nightlight's ladder,
/// paper's six sheets, shelf's five bindings × two faces and creeper's lime
/// pair. Only the accent and the contrast flag stay on the presets whose
/// `Theme` carries them.
const THEME_AWARE: &[UiPreset] = &[
    UiPreset::Classic,
    UiPreset::Nightlight,
    UiPreset::Paper,
    UiPreset::Shelf,
    UiPreset::Creeper,
];

const fn spec_for(
    presets: &'static [UiPreset],
    id: SettingId,
    title: &'static str,
    description: &'static str,
    kind: SettingKind,
) -> SettingSpec {
    SettingSpec {
        id,
        title,
        description,
        kind,
        presets,
    }
}

impl SettingSpec {
    fn applies_to(self, preset: UiPreset) -> bool {
        self.presets.is_empty() || self.presets.contains(&preset)
    }
}

const GENERAL_SPECS: [SettingSpec; 6] = [
    spec(
        SettingId::LauncherVisibility,
        "settings.visibility",
        "settings.desc.visibility",
        SettingKind::Choice,
    ),
    spec(
        SettingId::AutoCheckUpdates,
        "settings.auto_updates",
        "settings.desc.auto_updates",
        SettingKind::Toggle,
    ),
    spec(
        SettingId::CheckForUpdates,
        "settings.check_updates",
        "settings.desc.check_updates",
        SettingKind::Action,
    ),
    spec(
        SettingId::AddToPath,
        "settings.add_to_path",
        "settings.desc.add_to_path",
        SettingKind::Action,
    ),
    spec(
        SettingId::ServerAddress,
        "settings.server_address",
        "settings.desc.server_address",
        SettingKind::Text,
    ),
    spec(
        SettingId::ProcessPriority,
        "settings.process_priority",
        "settings.desc.process_priority",
        SettingKind::Choice,
    ),
];

const GAME_SPECS: [SettingSpec; 10] = [
    spec(
        SettingId::JavaSelection,
        "settings.java_selection",
        "settings.desc.java_selection",
        SettingKind::Choice,
    ),
    spec(
        SettingId::MemoryMode,
        "settings.memory_mode",
        "settings.desc.memory_mode",
        SettingKind::Choice,
    ),
    spec(
        SettingId::MemoryMinimum,
        "settings.memory_min",
        "settings.desc.memory_min",
        SettingKind::Number { step: 256 },
    ),
    spec(
        SettingId::MemoryMaximum,
        "settings.memory_max",
        "settings.desc.memory_max",
        SettingKind::Number { step: 256 },
    ),
    spec(
        SettingId::WindowMode,
        "settings.window_mode",
        "settings.desc.window_mode",
        SettingKind::Choice,
    ),
    spec(
        SettingId::WindowWidth,
        "settings.window_width",
        "settings.desc.window_width",
        SettingKind::Number { step: 16 },
    ),
    spec(
        SettingId::WindowHeight,
        "settings.window_height",
        "settings.desc.window_height",
        SettingKind::Number { step: 16 },
    ),
    spec(
        SettingId::AutoRepair,
        "settings.auto_repair",
        "settings.desc.auto_repair",
        SettingKind::Toggle,
    ),
    spec(
        SettingId::JvmArguments,
        "settings.jvm_args",
        "settings.desc.jvm_args",
        SettingKind::Text,
    ),
    spec(
        SettingId::GameArguments,
        "settings.game_args",
        "settings.desc.game_args",
        SettingKind::Text,
    ),
];

const DOWNLOAD_SPECS: [SettingSpec; 9] = [
    spec(
        SettingId::VersionSource,
        "settings.version_source",
        "settings.desc.version_source",
        SettingKind::Choice,
    ),
    spec(
        SettingId::FileSource,
        "settings.file_source",
        "settings.desc.file_source",
        SettingKind::Choice,
    ),
    spec(
        SettingId::AutoDownloadThreads,
        "settings.auto_threads",
        "settings.desc.auto_threads",
        SettingKind::Toggle,
    ),
    spec(
        SettingId::DownloadThreads,
        "settings.download_threads",
        "settings.desc.download_threads",
        SettingKind::Number { step: 1 },
    ),
    spec(
        SettingId::CacheDirectory,
        "settings.cache_directory",
        "settings.desc.cache_directory",
        SettingKind::Text,
    ),
    spec(
        SettingId::CacheLimit,
        "settings.cache_limit",
        "settings.desc.cache_limit",
        SettingKind::Number { step: 256 },
    ),
    spec(
        SettingId::CacheSize,
        "settings.cache_size",
        "settings.desc.cache_size",
        SettingKind::Action,
    ),
    spec(
        SettingId::RetryCount,
        "settings.retry_count",
        "settings.desc.retry_count",
        SettingKind::Number { step: 1 },
    ),
    spec(
        SettingId::Timeout,
        "settings.timeout",
        "settings.desc.timeout",
        SettingKind::Number { step: 5 },
    ),
];

const APPEARANCE_SPECS: [SettingSpec; 10] = [
    spec(
        SettingId::UiPreset,
        "settings.preset",
        "settings.desc.preset",
        SettingKind::Choice,
    ),
    spec_for(
        THEME_AWARE,
        SettingId::ThemeMode,
        "settings.theme",
        "settings.desc.theme",
        SettingKind::Choice,
    ),
    spec_for(
        THEMED,
        SettingId::HighContrast,
        "settings.high_contrast",
        "settings.desc.high_contrast",
        SettingKind::Toggle,
    ),
    spec_for(
        THEMED,
        SettingId::AccentColor,
        "settings.accent",
        "settings.desc.accent",
        SettingKind::Choice,
    ),
    spec_for(
        NIGHTLIGHT,
        SettingId::NightlightTheme,
        "settings.nightlight_theme",
        "settings.desc.nightlight_theme",
        SettingKind::Choice,
    ),
    spec_for(
        PAPER,
        SettingId::PaperTheme,
        "settings.paper_theme",
        "settings.desc.paper_theme",
        SettingKind::Choice,
    ),
    spec_for(
        SHELF,
        SettingId::ShelfTheme,
        "settings.shelf_theme",
        "settings.desc.shelf_theme",
        SettingKind::Choice,
    ),
    spec_for(
        NIGHTLIGHT,
        SettingId::LiveAccent,
        "settings.live_accent",
        "settings.desc.live_accent",
        SettingKind::Toggle,
    ),
    spec(
        SettingId::Language,
        "settings.language",
        "settings.desc.language",
        SettingKind::Choice,
    ),
    spec(
        SettingId::Mouse,
        "settings.mouse",
        "settings.desc.mouse",
        SettingKind::Toggle,
    ),
];

const LOG_SPECS: [SettingSpec; 8] = [
    spec(
        SettingId::LauncherLogs,
        "settings.launcher_logs",
        "settings.desc.launcher_logs",
        SettingKind::Toggle,
    ),
    spec(
        SettingId::GameDebug,
        "settings.debug_logs",
        "settings.desc.debug_logs",
        SettingKind::Toggle,
    ),
    spec(
        SettingId::LogRetention,
        "settings.log_retention",
        "settings.desc.log_retention",
        SettingKind::Number { step: 1 },
    ),
    spec(
        SettingId::GameLogs,
        "settings.game_logs",
        "settings.desc.game_logs",
        SettingKind::Action,
    ),
    spec(
        SettingId::LauncherLogDirectory,
        "settings.launcher_log_directory",
        "settings.desc.launcher_log_directory",
        SettingKind::Action,
    ),
    spec(
        SettingId::ExportLauncherLogs,
        "settings.export_launcher_logs",
        "settings.desc.export_launcher_logs",
        SettingKind::Action,
    ),
    spec(
        SettingId::ExportGameLogs,
        "settings.export_game_logs",
        "settings.desc.export_game_logs",
        SettingKind::Action,
    ),
    spec(
        SettingId::ExportCrashReport,
        "settings.export_crash_report",
        "settings.desc.export_crash_report",
        SettingKind::Action,
    ),
];

/// One match in the cross-category settings search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::tui::app) struct SettingSearchHit {
    pub category: SettingsCategory,
    pub index: usize,
    pub row: SettingRow,
}

impl App {
    pub(in crate::tui::app) fn settings_rows(&self) -> Vec<SettingRow> {
        self.settings_rows_for(self.settings.category)
    }

    pub(in crate::tui::app) fn settings_rows_for(
        &self,
        category: SettingsCategory,
    ) -> Vec<SettingRow> {
        self.visible_specs_for(category)
            .iter()
            .map(|spec| self.setting_row(spec))
            .collect()
    }

    /// The rows of `category` that this preset can actually use.
    ///
    /// The row list, the search index and the sidebar counter all come from
    /// here: filtering in one of them only makes the cursor address a row the
    /// screen does not show.
    fn visible_specs_for(&self, category: SettingsCategory) -> Vec<SettingSpec> {
        let preset = self.preset();
        Self::setting_specs_for(category)
            .into_iter()
            .filter(|spec| spec.applies_to(preset))
            .collect()
    }

    /// Flat, cross-category matches for the settings search. An empty query
    /// lists every key/value row, which doubles as a searchable index.
    pub(in crate::tui::app) fn settings_search_results(&self) -> Vec<SettingSearchHit> {
        let query = self
            .settings_search
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_lowercase();
        let mut hits = Vec::new();
        for category in SettingsCategory::ALL {
            if category.is_management() {
                continue;
            }
            for (index, spec) in self.visible_specs_for(category).iter().enumerate() {
                let row = self.setting_row(spec);
                if query.is_empty()
                    || row.title.to_lowercase().contains(&query)
                    || row.description.to_lowercase().contains(&query)
                {
                    hits.push(SettingSearchHit {
                        category,
                        index,
                        row,
                    });
                }
            }
        }
        hits
    }

    pub(in crate::tui::app) fn settings_search_active(&self) -> bool {
        self.settings_search.is_some()
    }

    #[cfg(test)]
    pub(in crate::tui::app) fn setting_ids(&self) -> Vec<SettingId> {
        self.setting_specs()
            .into_iter()
            .map(|spec| spec.id)
            .collect()
    }

    /// Number of selectable rows on the current settings surface. Management
    /// categories feed their own lists instead of key/value rows.
    pub(in crate::tui::app) fn settings_row_count(&self) -> usize {
        if self.settings_search_active() {
            return self.settings_search_results().len();
        }
        match self.settings.category {
            SettingsCategory::Java => self.java.len(),
            SettingsCategory::GameDirectories => 2 + self.config.game_directories.len(),
            _ => self.setting_specs().len(),
        }
    }

    pub(in crate::tui::app) fn current_setting_row(&self) -> Option<SettingRow> {
        let specs = self.setting_specs();
        let spec = specs.get(self.settings.selected)?;
        Some(self.setting_row(spec))
    }

    /// Row for a known id, regardless of the active category. Editors use this
    /// so `open_setting_editor(id)` never borrows a neighbouring row's title.
    pub(in crate::tui::app) fn setting_row_for(&self, id: SettingId) -> Option<SettingRow> {
        for category in SettingsCategory::ALL {
            if let Some(spec) = Self::setting_specs_for(category)
                .into_iter()
                .find(|spec| spec.id == id)
            {
                return Some(self.setting_row(&spec));
            }
        }
        None
    }

    fn setting_specs(&self) -> Vec<SettingSpec> {
        self.visible_specs_for(self.settings.category)
    }

    fn setting_specs_for(category: SettingsCategory) -> Vec<SettingSpec> {
        match category {
            SettingsCategory::General => GENERAL_SPECS.to_vec(),
            SettingsCategory::Game => GAME_SPECS.to_vec(),
            SettingsCategory::Java => Vec::new(),
            SettingsCategory::Downloads => DOWNLOAD_SPECS.to_vec(),
            SettingsCategory::Appearance => APPEARANCE_SPECS.to_vec(),
            SettingsCategory::Logs => LOG_SPECS.to_vec(),
            SettingsCategory::GameDirectories => Vec::new(),
        }
    }

    fn setting_row(&self, spec: &SettingSpec) -> SettingRow {
        let mut row = SettingRow::new(
            spec.id,
            self.i18n.text(spec.title),
            self.i18n.text(spec.description),
            self.setting_value(spec.id),
            spec.kind,
            self.i18n.text(spec.kind.hint_key()),
        );
        if spec.kind == SettingKind::Toggle {
            row = row.toggle_state(self.toggle_value(spec.id));
        }
        if let Some(reason) = self.setting_disabled_reason(spec.id) {
            row = row.disabled(self.i18n.text(reason));
        }
        row
    }

    fn toggle_value(&self, id: SettingId) -> bool {
        match id {
            SettingId::AutoCheckUpdates => self.config.launcher.auto_check_updates,
            SettingId::AutoDownloadThreads => self.config.downloads.auto_download_threads,
            SettingId::HighContrast => self.config.state.ui.high_contrast,
            SettingId::LiveAccent => self.config.state.ui.live_accent,
            SettingId::Mouse => self.config.state.ui.show_mouse,
            SettingId::LauncherLogs => self.config.logs.launcher_enabled,
            SettingId::GameDebug => self.config.logs.game_debug,
            SettingId::AutoRepair => self.config.launch.auto_repair,
            _ => false,
        }
    }

    pub(in crate::tui::app) fn setting_disabled_reason(
        &self,
        id: SettingId,
    ) -> Option<&'static str> {
        match id {
            SettingId::ProcessPriority
                if !crate::platform::windows::process_priority_supported() =>
            {
                Some("settings.reason.unsupported")
            }
            SettingId::ExportCrashReport
                if !self
                    .last_crash_report
                    .as_ref()
                    .is_some_and(|path| path.is_file()) =>
            {
                Some("status.no_crash_report")
            }
            SettingId::MemoryMinimum | SettingId::MemoryMaximum
                if self.config.launch.memory_mode == MemoryMode::Automatic =>
            {
                Some("settings.reason.memory_auto")
            }
            SettingId::WindowWidth | SettingId::WindowHeight if self.config.launch.fullscreen => {
                Some("settings.reason.fullscreen")
            }
            SettingId::DownloadThreads if self.config.downloads.auto_download_threads => {
                Some("settings.reason.threads_auto")
            }
            // Warming is the defining behaviour of the `live` theme, not a
            // filter over the fixed ones: a theme with a pinned accent ignores
            // the seed entirely. Leaving the switch live and enabled under
            // `ember` promised a colour change that could never happen.
            SettingId::LiveAccent
                if self.preset() != UiPreset::Nightlight
                    || self.config.state.ui.nightlight_theme != NightlightTheme::Live =>
            {
                Some("settings.reason.live_theme_only")
            }
            SettingId::AccentColor
                if self.preset() == UiPreset::Nightlight
                    && self.config.state.ui.nightlight_theme != NightlightTheme::Live =>
            {
                Some("settings.reason.accent_theme_fixed")
            }
            _ => None,
        }
    }

    fn setting_value(&self, id: SettingId) -> String {
        match id {
            SettingId::LauncherVisibility => self.settings_value_visibility(),
            SettingId::AutoCheckUpdates => self.on_off(self.config.launcher.auto_check_updates),
            SettingId::CheckForUpdates => self.i18n.text("settings.action.check_now"),
            SettingId::AddToPath => self.i18n.text("settings.action.add_to_path"),
            SettingId::ServerAddress => self
                .config
                .launcher
                .server_address
                .as_deref()
                .unwrap_or("-")
                .to_owned(),
            SettingId::ProcessPriority => self.settings_value_priority(),
            SettingId::JavaSelection => self.settings_value_java_selection(),
            SettingId::MemoryMode => self.settings_value_memory_mode(),
            SettingId::MemoryMinimum => self.memory_mb_value(self.config.launch.min_memory_mb),
            SettingId::MemoryMaximum => self.memory_mb_value(self.config.launch.max_memory_mb),
            SettingId::CacheLimit => self.config.downloads.cache_limit_mb.to_string(),
            SettingId::WindowMode => self.settings_value_window_mode(),
            SettingId::WindowWidth => self.config.launch.width.to_string(),
            SettingId::WindowHeight => self.config.launch.height.to_string(),
            SettingId::JvmArguments => self.config.launch.jvm_args.len().to_string(),
            SettingId::GameArguments => self.config.launch.game_args.len().to_string(),
            SettingId::VersionSource => {
                self.settings_value_source(self.config.downloads.version_list_source)
            }
            SettingId::FileSource => {
                self.settings_value_source(self.config.downloads.file_download_source)
            }
            SettingId::AutoDownloadThreads => {
                self.on_off(self.config.downloads.auto_download_threads)
            }
            SettingId::DownloadThreads => {
                if self.config.downloads.auto_download_threads {
                    self.i18n.text("common.auto")
                } else {
                    self.config.downloads.download_threads.to_string()
                }
            }
            SettingId::CacheDirectory => self
                .config
                .downloads
                .cache_dir
                .as_deref()
                .map(display_path)
                .unwrap_or_else(|| self.i18n.text("common.default")),
            SettingId::CacheSize => self.cache_stats_summary(),
            SettingId::RetryCount => self.config.downloads.retry_count.to_string(),
            SettingId::Timeout => format!("{} s", self.config.downloads.timeout_secs),
            SettingId::UiPreset => self.i18n.text(self.preset().label_key()),
            SettingId::ThemeMode => self.settings_value_theme(),
            SettingId::HighContrast => self.on_off(self.config.state.ui.high_contrast),
            SettingId::AccentColor => self.settings_value_accent(),
            SettingId::NightlightTheme => self
                .i18n
                .text(self.config.state.ui.nightlight_theme.label_key()),
            SettingId::PaperTheme => self.i18n.text(self.config.state.ui.paper_theme.label_key()),
            SettingId::ShelfTheme => self.i18n.text(self.config.state.ui.shelf_theme.label_key()),
            SettingId::LiveAccent => self.on_off(self.config.state.ui.live_accent),
            SettingId::Language => self.i18n.language_name(self.config.state.ui.language),
            SettingId::Mouse => self.on_off(self.config.state.ui.show_mouse),
            SettingId::LauncherLogs => self.on_off(self.config.logs.launcher_enabled),
            SettingId::GameDebug => self.on_off(self.config.logs.game_debug),
            SettingId::AutoRepair => self.on_off(self.config.launch.auto_repair),
            SettingId::LogRetention => self.i18n.format(
                "settings.retention_value",
                &[("days", &self.config.logs.retention_days.to_string())],
            ),
            SettingId::GameLogs => self.i18n.text("action.open"),
            SettingId::LauncherLogDirectory => display_path(&self.paths.launcher_logs_dir),
            SettingId::ExportLauncherLogs | SettingId::ExportGameLogs => {
                self.i18n.text("action.save")
            }
            SettingId::ExportCrashReport => {
                if self
                    .last_crash_report
                    .as_ref()
                    .is_some_and(|path| path.is_file())
                {
                    self.i18n.text("action.save")
                } else {
                    self.i18n.text("common.none")
                }
            }
        }
    }

    fn memory_mb_value(&self, value: u32) -> String {
        if self.config.launch.memory_mode == MemoryMode::Automatic {
            self.i18n.text("common.auto")
        } else {
            format!("{value} MB")
        }
    }

    fn on_off(&self, enabled: bool) -> String {
        if enabled {
            self.i18n.text("common.on")
        } else {
            self.i18n.text("common.off")
        }
    }

    pub(in crate::tui::app) fn settings_value_visibility(&self) -> String {
        self.i18n.text(match self.config.launcher.visibility {
            LauncherVisibility::KeepOpen => "settings.visibility.keep",
            LauncherVisibility::HideAfterLaunch => "settings.visibility.hide",
            LauncherVisibility::ExitAfterLaunch => "settings.visibility.exit",
        })
    }

    pub(in crate::tui::app) fn settings_value_priority(&self) -> String {
        self.i18n.text(match self.config.launcher.process_priority {
            ProcessPriority::Default => "settings.priority.default",
            ProcessPriority::AboveNormal => "settings.priority.above",
            ProcessPriority::High => "settings.priority.high",
        })
    }

    pub(in crate::tui::app) fn settings_value_java_selection(&self) -> String {
        self.i18n.text(match self.config.java.selection_mode {
            JavaSelectionMode::Automatic => "settings.java.auto",
            JavaSelectionMode::Fixed => "settings.java.fixed",
        })
    }

    pub(in crate::tui::app) fn settings_value_memory_mode(&self) -> String {
        self.i18n.text(match self.config.launch.memory_mode {
            MemoryMode::Automatic => "settings.memory.auto",
            MemoryMode::Manual => "settings.memory.manual",
        })
    }

    pub(in crate::tui::app) fn settings_value_window_mode(&self) -> String {
        if self.config.launch.fullscreen {
            self.i18n.text("settings.window.fullscreen")
        } else {
            self.i18n.text("settings.window.windowed")
        }
    }

    pub(in crate::tui::app) fn settings_value_theme(&self) -> String {
        self.i18n.text(match self.config.state.ui.theme_mode {
            ThemeMode::Dark => "settings.theme.dark",
            ThemeMode::Light => "settings.theme.light",
            ThemeMode::Terminal => "settings.theme.terminal",
        })
    }

    pub(in crate::tui::app) fn settings_value_accent(&self) -> String {
        self.i18n.text(match self.config.state.ui.accent_color {
            AccentColor::Orange => "settings.accent.orange",
            AccentColor::Cyan => "settings.accent.cyan",
            AccentColor::Blue => "settings.accent.blue",
            AccentColor::Green => "settings.accent.green",
            AccentColor::Yellow => "settings.accent.yellow",
            AccentColor::Magenta => "settings.accent.magenta",
        })
    }

    pub(in crate::tui::app) fn settings_value_source(&self, mode: DownloadSourceMode) -> String {
        self.i18n.text(match mode {
            DownloadSourceMode::Default => "settings.source.default",
            DownloadSourceMode::Official => "settings.source.official",
            DownloadSourceMode::Mirror => "settings.source.mirror",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{APPEARANCE_SPECS, DOWNLOAD_SPECS, GAME_SPECS, GENERAL_SPECS, LOG_SPECS};

    #[test]
    fn specs_keep_the_rendered_row_order() {
        assert_eq!(GENERAL_SPECS.len(), 6);
        assert_eq!(GAME_SPECS.len(), 10);
        assert_eq!(DOWNLOAD_SPECS.len(), 9);
        assert_eq!(APPEARANCE_SPECS.len(), 10);
        assert_eq!(LOG_SPECS.len(), 8);
    }
}
