#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsCategory {
    General,
    Game,
    Java,
    Downloads,
    Appearance,
    Logs,
    GameDirectories,
}

impl SettingsCategory {
    pub const ALL: [Self; 7] = [
        Self::General,
        Self::Game,
        Self::Java,
        Self::Downloads,
        Self::Appearance,
        Self::Logs,
        Self::GameDirectories,
    ];

    pub const fn title_key(self) -> &'static str {
        match self {
            Self::General => "settings.category.general",
            Self::Game => "settings.category.game",
            Self::Java => "settings.category.java",
            Self::Downloads => "settings.category.downloads",
            Self::Appearance => "settings.category.appearance",
            Self::Logs => "settings.category.logs",
            Self::GameDirectories => "settings.category.game_directories",
        }
    }

    pub const fn index(self) -> usize {
        match self {
            Self::General => 0,
            Self::Game => 1,
            Self::Java => 2,
            Self::Downloads => 3,
            Self::Appearance => 4,
            Self::Logs => 5,
            Self::GameDirectories => 6,
        }
    }

    pub fn from_index(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or(Self::General)
    }

    /// Java runtimes and game directories are management pages, not lists of
    /// key/value settings: they render their own surface.
    pub const fn is_management(self) -> bool {
        matches!(self, Self::Java | Self::GameDirectories)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingId {
    // General
    LauncherVisibility,
    AutoCheckUpdates,
    CheckForUpdates,
    AddToPath,
    ServerAddress,
    ProcessPriority,
    // Game
    JavaSelection,
    MemoryMode,
    MemoryMinimum,
    MemoryMaximum,
    WindowMode,
    WindowWidth,
    WindowHeight,
    AutoRepair,
    JvmArguments,
    GameArguments,
    // Downloads
    VersionSource,
    FileSource,
    AutoDownloadThreads,
    DownloadThreads,
    CacheDirectory,
    CacheLimit,
    CacheSize,
    RetryCount,
    Timeout,
    // Appearance
    UiPreset,
    ThemeMode,
    HighContrast,
    AccentColor,
    NightlightTheme,
    PaperTheme,
    ShelfTheme,
    LiveAccent,
    Language,
    Mouse,
    /// `跟随终端` only: lift auxiliary text to a legible floor.
    LiftDimText,
    /// `跟随终端` only: ask the terminal for its background colour.
    DetectTerminal,
    // Logs
    LauncherLogs,
    GameDebug,
    LogRetention,
    GameLogs,
    LauncherLogDirectory,
    ExportLauncherLogs,
    ExportGameLogs,
    ExportDiagnosisBundle,
    DiagnoseAutoOpen,
}

/// One focus only: the category is navigation, the selected row is the cursor.
/// The old `in_category` / `category_focus` pair let `Enter` mean two different
/// things depending on invisible state; both are gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsState {
    pub category: SettingsCategory,
    pub selected: usize,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            category: SettingsCategory::General,
            selected: 0,
        }
    }
}

impl SettingsState {
    pub fn select_category(&mut self, category: SettingsCategory) {
        if self.category != category {
            self.category = category;
            self.selected = 0;
        }
    }

    pub fn cycle_category(&mut self, delta: isize) {
        let index = move_index(self.category.index(), SettingsCategory::ALL.len(), delta);
        self.select_category(SettingsCategory::from_index(index));
    }

    pub fn move_selection(&mut self, count: usize, delta: isize) {
        self.selected = move_index(self.selected, count, delta);
    }

    pub fn clamp_selection(&mut self, count: usize) {
        self.selected = self.selected.min(count.saturating_sub(1));
    }
}

fn move_index(current: usize, count: usize, delta: isize) -> usize {
    if count == 0 {
        return 0;
    }
    let last = count - 1;
    if delta.is_negative() {
        current.saturating_sub(delta.unsigned_abs())
    } else {
        current.saturating_add(delta as usize).min(last)
    }
}

#[cfg(test)]
mod tests {
    use super::{SettingId, SettingsCategory, SettingsState};

    #[test]
    fn categories_keep_the_approved_order() {
        assert_eq!(SettingsCategory::ALL.len(), 7);
        assert_eq!(SettingsCategory::from_index(0), SettingsCategory::General);
        assert_eq!(SettingsCategory::from_index(2), SettingsCategory::Java);
        assert_eq!(
            SettingsCategory::from_index(6),
            SettingsCategory::GameDirectories
        );
        assert!(SettingsCategory::Java.is_management());
        assert!(!SettingsCategory::General.is_management());
    }

    #[test]
    fn switching_category_resets_the_row_cursor() {
        let mut state = SettingsState::default();
        state.move_selection(5, 3);
        assert_eq!(state.selected, 3);
        state.cycle_category(1);
        assert_eq!(state.category, SettingsCategory::Game);
        assert_eq!(state.selected, 0);
        // Re-selecting the active category keeps the cursor.
        state.move_selection(5, 2);
        state.select_category(SettingsCategory::Game);
        assert_eq!(state.selected, 2);
    }

    #[test]
    fn selection_clamps_inside_the_row_count() {
        let mut state = SettingsState::default();
        state.move_selection(3, 100);
        assert_eq!(state.selected, 2);
        state.move_selection(3, -100);
        assert_eq!(state.selected, 0);
        state.selected = 9;
        state.clamp_selection(4);
        assert_eq!(state.selected, 3);
    }

    #[test]
    fn setting_ids_stay_distinct() {
        assert_ne!(SettingId::MemoryMode, SettingId::MemoryMaximum);
        assert_ne!(SettingId::Language, SettingId::WindowMode);
    }
}
