use std::path::PathBuf;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::core::download::DownloadSourceMode;

pub const CURRENT_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub schema_version: u32,
    pub game_directories: Vec<PathBuf>,
    pub excluded_instances: Vec<PathBuf>,
    pub launcher: LauncherConfig,
    pub java: JavaConfig,
    pub downloads: DownloadConfig,
    pub launch: LaunchConfig,
    pub state: StateConfig,
    pub logs: LogConfig,
    pub diagnosis: DiagnosisConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            game_directories: Vec::new(),
            excluded_instances: Vec::new(),
            launcher: LauncherConfig::default(),
            java: JavaConfig::default(),
            downloads: DownloadConfig::default(),
            launch: LaunchConfig::default(),
            state: StateConfig::default(),
            logs: LogConfig::default(),
            diagnosis: DiagnosisConfig::default(),
        }
    }
}

impl Config {
    /// Download settings a task starts from.
    ///
    /// The snapshot decides mirrors, cache directory, retry count and timeout, so a
    /// copy per front end would make the same download behave differently depending
    /// on which one started it.
    pub fn download_snapshot(
        &self,
        paths: &crate::core::config::paths::DataPaths,
    ) -> crate::core::download::DownloadSourceSnapshot {
        let mut snapshot = crate::core::download::DownloadSourceSnapshot::new(
            self.downloads.version_list_source,
            self.downloads.file_download_source,
        );
        snapshot.cache_dir = Some(
            self.downloads
                .cache_dir
                .clone()
                .unwrap_or_else(|| paths.cache_dir.clone()),
        );
        snapshot.retry_count = self.downloads.retry_count;
        snapshot.timeout_secs = self.downloads.timeout_secs;
        snapshot
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LauncherVisibility {
    #[default]
    KeepOpen,
    HideAfterLaunch,
    ExitAfterLaunch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProcessPriority {
    #[default]
    Default,
    AboveNormal,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MemoryMode {
    #[default]
    Automatic,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum JavaSelectionMode {
    #[default]
    Automatic,
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AccentColor {
    #[default]
    Orange,
    Cyan,
    Blue,
    Green,
    Yellow,
    Magenta,
}

/// Which TUI preset draws the interface. All presets share the same state,
/// commands and key map; only the rendering layer differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UiPreset {
    /// Binsider-style framed shell (header box, top tabs, `|Title|` panels).
    #[default]
    Classic,
    /// Card-based shell with a sidebar, gradient rule and neon palette.
    Creeper,
    /// Night-light shell: no top banner, numbered sidebar nav, borderless
    /// cards and a single accent that derives the whole ladder.
    Nightlight,
    /// Paper shell: a light page with hairline rules, no cards, and a selection
    /// marked by a margin bar and an underline instead of a filled row.
    Paper,
    /// Shelf shell: a cover-led library. One instance list on the left, a
    /// framed cover and its facts on the right, and a single lavender accent
    /// on deep indigo. No sidebar, no cards, no animation.
    Shelf,
}

impl UiPreset {
    pub const ALL: [Self; 5] = [
        Self::Classic,
        Self::Creeper,
        Self::Nightlight,
        Self::Paper,
        Self::Shelf,
    ];

    pub const fn code(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::Creeper => "creeper",
            Self::Nightlight => "nightlight",
            Self::Paper => "paper",
            Self::Shelf => "shelf",
        }
    }

    pub fn from_code(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "creeper" => Self::Creeper,
            "nightlight" => Self::Nightlight,
            "paper" => Self::Paper,
            "shelf" => Self::Shelf,
            _ => Self::Classic,
        }
    }

    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Classic => "settings.preset.classic",
            Self::Creeper => "settings.preset.creeper",
            Self::Nightlight => "settings.preset.nightlight",
            Self::Paper => "settings.preset.paper",
            Self::Shelf => "settings.preset.shelf",
        }
    }

    pub const fn next(self, direction: isize) -> Self {
        let index = match self {
            Self::Classic => 0,
            Self::Creeper => 1,
            Self::Nightlight => 2,
            Self::Paper => 3,
            Self::Shelf => 4,
        };
        let next = if direction < 0 {
            (index + Self::ALL.len() - 1) % Self::ALL.len()
        } else {
            (index + 1) % Self::ALL.len()
        };
        Self::ALL[next]
    }
}

/// Which of the eight palettes the `nightlight` preset wears.
///
/// `Live` is the identity: its accent follows the local clock and warms as the
/// evening comes on. The rest are fixed palettes. Names stay in their original form
/// in every language, because translating `gruvbox` would make it unrecognisable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NightlightTheme {
    #[default]
    Live,
    Tokyo,
    Mocha,
    Nord,
    Gruvbox,
    Synth,
    Ember,
    Phosphor,
}

impl NightlightTheme {
    pub const ALL: [Self; 8] = [
        Self::Live,
        Self::Tokyo,
        Self::Mocha,
        Self::Nord,
        Self::Gruvbox,
        Self::Synth,
        Self::Ember,
        Self::Phosphor,
    ];

    pub const fn code(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Tokyo => "tokyo",
            Self::Mocha => "mocha",
            Self::Nord => "nord",
            Self::Gruvbox => "gruvbox",
            Self::Synth => "synth",
            Self::Ember => "ember",
            Self::Phosphor => "phosphor",
        }
    }

    pub fn from_code(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "tokyo" => Self::Tokyo,
            "mocha" => Self::Mocha,
            "nord" => Self::Nord,
            "gruvbox" => Self::Gruvbox,
            "synth" => Self::Synth,
            "ember" => Self::Ember,
            "phosphor" => Self::Phosphor,
            _ => Self::Live,
        }
    }

    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Live => "theme.live",
            Self::Tokyo => "theme.tokyo",
            Self::Mocha => "theme.mocha",
            Self::Nord => "theme.nord",
            Self::Gruvbox => "theme.gruvbox",
            Self::Synth => "theme.synth",
            Self::Ember => "theme.ember",
            Self::Phosphor => "theme.phosphor",
        }
    }

    pub const fn next(self, direction: isize) -> Self {
        let index = match self {
            Self::Live => 0,
            Self::Tokyo => 1,
            Self::Mocha => 2,
            Self::Nord => 3,
            Self::Gruvbox => 4,
            Self::Synth => 5,
            Self::Ember => 6,
            Self::Phosphor => 7,
        };
        let next = if direction < 0 {
            (index + Self::ALL.len() - 1) % Self::ALL.len()
        } else {
            (index + 1) % Self::ALL.len()
        };
        Self::ALL[next]
    }
}

impl Serialize for UiPreset {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.code())
    }
}

impl<'de> Deserialize<'de> for UiPreset {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Option::<String>::deserialize(deserializer)?;
        Ok(value.as_deref().map_or(Self::Classic, Self::from_code))
    }
}

impl Serialize for NightlightTheme {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.code())
    }
}

impl<'de> Deserialize<'de> for NightlightTheme {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Option::<String>::deserialize(deserializer)?;
        Ok(value.as_deref().map_or(Self::Live, Self::from_code))
    }
}

/// Which stock the `paper` preset is printed on.
///
/// Paper is one axis and `ThemeMode` is the other: `Ivory` under `Light` is a
/// different palette from `Ivory` under `Dark`, and both are legitimate. Keeping
/// the mode out of this enum is what lets the preset follow the global
/// appearance switch without a second setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaperTheme {
    #[default]
    Ivory,
    Ash,
    Kraft,
}

impl PaperTheme {
    pub const ALL: [Self; 3] = [Self::Ivory, Self::Ash, Self::Kraft];

    pub const fn code(self) -> &'static str {
        match self {
            Self::Ivory => "ivory",
            Self::Ash => "ash",
            Self::Kraft => "kraft",
        }
    }

    pub fn from_code(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "ash" => Self::Ash,
            "kraft" => Self::Kraft,
            _ => Self::Ivory,
        }
    }

    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Ivory => "paper.theme.ivory",
            Self::Ash => "paper.theme.ash",
            Self::Kraft => "paper.theme.kraft",
        }
    }

    pub const fn next(self, direction: isize) -> Self {
        let index = match self {
            Self::Ivory => 0,
            Self::Ash => 1,
            Self::Kraft => 2,
        };
        let next = if direction < 0 {
            (index + Self::ALL.len() - 1) % Self::ALL.len()
        } else {
            (index + 1) % Self::ALL.len()
        };
        Self::ALL[next]
    }
}

impl Serialize for PaperTheme {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.code())
    }
}

impl<'de> Deserialize<'de> for PaperTheme {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Option::<String>::deserialize(deserializer)?;
        Ok(value.as_deref().map_or(Self::Ivory, Self::from_code))
    }
}

/// Which sheet the `shelf` preset is bound in.
///
/// One axis, like [`PaperTheme`]: the scheme picks the material and
/// `ThemeMode` picks day or night within it. The two are separate because the
/// alternative — a dozen entries spelled day and night — would make the
/// appearance row twice as long and let an old config land on a sheet the
/// user never chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShelfTheme {
    #[default]
    Lavender,
    Moss,
    Slate,
    Leather,
    Rose,
}

impl ShelfTheme {
    pub const ALL: [Self; 5] = [
        Self::Lavender,
        Self::Moss,
        Self::Slate,
        Self::Leather,
        Self::Rose,
    ];

    pub const fn code(self) -> &'static str {
        match self {
            Self::Lavender => "lavender",
            Self::Moss => "moss",
            Self::Slate => "slate",
            Self::Leather => "leather",
            Self::Rose => "rose",
        }
    }

    pub fn from_code(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "moss" => Self::Moss,
            "slate" => Self::Slate,
            "leather" => Self::Leather,
            "rose" => Self::Rose,
            _ => Self::Lavender,
        }
    }

    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Lavender => "shelf.theme.lavender",
            Self::Moss => "shelf.theme.moss",
            Self::Slate => "shelf.theme.slate",
            Self::Leather => "shelf.theme.leather",
            Self::Rose => "shelf.theme.rose",
        }
    }

    pub const fn next(self, direction: isize) -> Self {
        let index = match self {
            Self::Lavender => 0,
            Self::Moss => 1,
            Self::Slate => 2,
            Self::Leather => 3,
            Self::Rose => 4,
        };
        let next = if direction < 0 {
            (index + Self::ALL.len() - 1) % Self::ALL.len()
        } else {
            (index + 1) % Self::ALL.len()
        };
        Self::ALL[next]
    }
}

impl Serialize for ShelfTheme {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.code())
    }
}

impl<'de> Deserialize<'de> for ShelfTheme {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Option::<String>::deserialize(deserializer)?;
        // `Lavender` is the sheet shelf shipped with, so it is both the default
        // and the fallback: an unknown code must not silently repaint a frame.
        Ok(value.as_deref().map_or(Self::Lavender, Self::from_code))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LauncherConfig {
    pub visibility: LauncherVisibility,
    pub auto_check_updates: bool,
    pub server_address: Option<String>,
    pub process_priority: ProcessPriority,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            visibility: LauncherVisibility::KeepOpen,
            auto_check_updates: true,
            server_address: None,
            process_priority: ProcessPriority::Default,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaunchConfig {
    pub memory_mode: MemoryMode,
    pub min_memory_mb: u32,
    pub max_memory_mb: u32,
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
    /// Download files the launch is missing before starting, instead of only
    /// reporting them. Mirrors the "check/repair before launch" switch other
    /// launchers expose.
    pub auto_repair: bool,
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
}

impl Default for LaunchConfig {
    fn default() -> Self {
        Self {
            memory_mode: MemoryMode::Automatic,
            min_memory_mb: 1024,
            max_memory_mb: 4096,
            width: 1280,
            height: 720,
            fullscreen: false,
            auto_repair: true,
            jvm_args: Vec::new(),
            game_args: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct JavaConfig {
    pub selected_path: Option<PathBuf>,
    pub auto_download: bool,
    pub selection_mode: JavaSelectionMode,
}

impl Default for JavaConfig {
    fn default() -> Self {
        Self {
            selected_path: None,
            auto_download: false,
            selection_mode: JavaSelectionMode::Automatic,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DownloadConfig {
    pub version_list_source: DownloadSourceMode,
    pub file_download_source: DownloadSourceMode,
    pub auto_download_threads: bool,
    pub download_threads: usize,
    pub cache_dir: Option<PathBuf>,
    /// Upper bound for the download cache in MiB; `0` disables eviction.
    #[serde(default = "default_cache_limit_mb")]
    pub cache_limit_mb: u64,
    pub retry_count: u8,
    pub timeout_secs: u64,
}

const fn default_cache_limit_mb() -> u64 {
    8 * 1024
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            version_list_source: DownloadSourceMode::Default,
            file_download_source: DownloadSourceMode::Default,
            auto_download_threads: true,
            download_threads: 0,
            cache_dir: None,
            cache_limit_mb: default_cache_limit_mb(),
            retry_count: 5,
            timeout_secs: 60,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct StateConfig {
    pub current_account_id: Option<String>,
    pub current_instance_id: Option<String>,
    pub ui: UiConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub color_scheme: String,
    pub show_mouse: bool,
    pub theme_mode: ThemeMode,
    pub high_contrast: bool,
    pub accent_color: AccentColor,
    #[serde(default)]
    pub preset: UiPreset,
    #[serde(default)]
    pub language: Language,
    /// Palette worn by the `nightlight` preset; ignored by the other two.
    #[serde(default)]
    pub nightlight_theme: NightlightTheme,
    /// Paper stock worn by the `paper` preset; the light/dark variant comes from
    /// `theme_mode`, so this field only answers "which paper".
    #[serde(default)]
    pub paper_theme: PaperTheme,
    /// Sheet worn by the `shelf` preset, on the same one-axis rule as
    /// `paper_theme`.
    #[serde(default)]
    pub shelf_theme: ShelfTheme,
    /// Whether the `live` palette takes its accent from the local clock.
    #[serde(default = "default_live_accent")]
    pub live_accent: bool,
    /// Lifts auxiliary text to a legible floor when the sheet follows the
    /// terminal. Only cells painted directly on the terminal background move:
    /// a label sitting on a solid pill must keep the ink the pill was built
    /// around.
    #[serde(default = "default_true")]
    pub lift_dim_text: bool,
    /// Asks the terminal for its background colour (OSC 11) at start-up, so a
    /// light terminal gets the light sheet instead of pale text on white.
    #[serde(default = "default_true")]
    pub detect_terminal: bool,
}

fn default_live_accent() -> bool {
    true
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    #[default]
    English,
    SimplifiedChinese,
}

impl Language {
    pub const fn code(self) -> &'static str {
        match self {
            Self::English => "en-US",
            Self::SimplifiedChinese => "zh-CN",
        }
    }

    pub fn from_code(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "zh-cn" | "zh_cn" | "zh" => Self::SimplifiedChinese,
            _ => Self::English,
        }
    }
}

impl Serialize for Language {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.code())
    }
}

impl<'de> Deserialize<'de> for Language {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Option::<String>::deserialize(deserializer)?;
        Ok(value.as_deref().map_or(Self::English, Self::from_code))
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            color_scheme: "default".into(),
            show_mouse: true,
            theme_mode: ThemeMode::Dark,
            high_contrast: false,
            accent_color: AccentColor::Orange,
            preset: UiPreset::Classic,
            language: Language::English,
            nightlight_theme: NightlightTheme::Live,
            paper_theme: PaperTheme::Ivory,
            shelf_theme: ShelfTheme::Lavender,
            live_accent: true,
            lift_dim_text: true,
            detect_terminal: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LogConfig {
    pub retention_days: u32,
    pub launcher_enabled: bool,
    pub game_debug: bool,
}

/// What the diagnosis card does on its own.
///
/// A crash is the one moment a player cannot be expected to go looking for a tab,
/// so the card opens itself, but only over pages where it cannot eat a half-typed
/// form, which `handle_diagnosis_ready` enforces separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosisConfig {
    pub auto_open: bool,
}

impl Default for DiagnosisConfig {
    fn default() -> Self {
        Self { auto_open: true }
    }
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            retention_days: 30,
            launcher_enabled: true,
            game_debug: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Config, Language, LauncherVisibility, MemoryMode, NightlightTheme, PaperTheme,
        ProcessPriority, ShelfTheme, ThemeMode, UiPreset,
    };

    #[test]
    fn language_is_backward_compatible_and_persisted() {
        let legacy = serde_json::from_str::<Config>("{}").expect("legacy config");
        assert_eq!(legacy.state.ui.language, Language::English);

        let chinese = Config {
            state: super::StateConfig {
                ui: super::UiConfig {
                    language: Language::SimplifiedChinese,
                    ..super::UiConfig::default()
                },
                ..super::StateConfig::default()
            },
            ..Config::default()
        };
        let encoded = serde_json::to_string(&chinese).expect("serialize config");
        assert!(encoded.contains(r#""language":"zh-CN""#));
        let restored = serde_json::from_str::<Config>(&encoded).expect("restore config");
        assert_eq!(restored.state.ui.language, Language::SimplifiedChinese);
    }

    #[test]
    fn new_settings_have_safe_defaults() {
        let config = Config::default();
        assert_eq!(config.launcher.visibility, LauncherVisibility::KeepOpen);
        assert_eq!(config.launcher.process_priority, ProcessPriority::Default);
        assert_eq!(config.launch.memory_mode, MemoryMode::Automatic);
        assert_eq!(config.state.ui.theme_mode, ThemeMode::Dark);
        assert_eq!(config.state.ui.nightlight_theme, NightlightTheme::Live);
        assert_eq!(config.state.ui.paper_theme, PaperTheme::Ivory);
        assert_eq!(config.state.ui.shelf_theme, ShelfTheme::Lavender);
        assert!(config.state.ui.live_accent);
        assert_eq!(config.downloads.retry_count, 5);
        assert_eq!(config.downloads.timeout_secs, 60);
        assert!(config.logs.launcher_enabled);
        assert!(!config.logs.game_debug);
    }

    #[test]
    fn missing_new_settings_use_defaults_when_reading_existing_shape() {
        let config: Config = serde_json::from_str(
            r#"{
                "schema_version": 2,
                "game_directories": [],
                "excluded_instances": [],
                "java": {},
                "downloads": {},
                "launch": {},
                "state": {},
                "logs": {}
            }"#,
        )
        .expect("existing config shape");
        assert_eq!(config.launcher.visibility, LauncherVisibility::KeepOpen);
        assert_eq!(config.launch.memory_mode, MemoryMode::Automatic);
        assert_eq!(config.state.ui.theme_mode, ThemeMode::Dark);
    }

    #[test]
    fn ui_preset_defaults_to_classic_and_round_trips() {
        let legacy: Config = serde_json::from_str(r#"{"state":{"ui":{"theme_mode":"dark"}}}"#)
            .expect("legacy ui config");
        assert_eq!(legacy.state.ui.preset, UiPreset::Classic);

        let encoded = serde_json::to_string(&UiPreset::Creeper).expect("serialize preset");
        assert_eq!(encoded, r#""creeper""#);

        let mut config = Config::default();
        config.state.ui.preset = UiPreset::Creeper;
        let encoded = serde_json::to_string(&config).expect("serialize config");
        assert!(encoded.contains(r#""preset":"creeper""#));
        let restored: Config = serde_json::from_str(&encoded).expect("restore config");
        assert_eq!(restored.state.ui.preset, UiPreset::Creeper);

        let unknown: Config = serde_json::from_str(r#"{"state":{"ui":{"preset":"unknown"}}}"#)
            .expect("unknown preset falls back");
        assert_eq!(unknown.state.ui.preset, UiPreset::Classic);
    }

    #[test]
    fn ui_preset_cycles_through_all_presets() {
        assert_eq!(UiPreset::Classic.next(1), UiPreset::Creeper);
        assert_eq!(UiPreset::Creeper.next(1), UiPreset::Nightlight);
        assert_eq!(UiPreset::Nightlight.next(1), UiPreset::Paper);
        assert_eq!(UiPreset::Paper.next(1), UiPreset::Shelf);
        assert_eq!(UiPreset::Shelf.next(1), UiPreset::Classic);
        assert_eq!(UiPreset::Classic.next(-1), UiPreset::Shelf);
        assert_eq!(UiPreset::Shelf.next(-1), UiPreset::Paper);
        assert_eq!(UiPreset::ALL.len(), 5);
        assert_eq!(UiPreset::from_code("nightlight"), UiPreset::Nightlight);
        assert_eq!(UiPreset::Nightlight.code(), "nightlight");
        assert_eq!(
            UiPreset::Nightlight.label_key(),
            "settings.preset.nightlight"
        );
        assert_eq!(UiPreset::from_code(" Paper "), UiPreset::Paper);
        assert_eq!(UiPreset::Paper.code(), "paper");
        assert_eq!(UiPreset::Paper.label_key(), "settings.preset.paper");
        assert_eq!(UiPreset::from_code("shelf"), UiPreset::Shelf);
        assert_eq!(UiPreset::Shelf.code(), "shelf");
        assert_eq!(UiPreset::Shelf.label_key(), "settings.preset.shelf");
    }

    #[test]
    fn nightlight_theme_round_trips_and_falls_back_to_live() {
        assert_eq!(
            serde_json::to_string(&NightlightTheme::Tokyo).expect("serialize theme"),
            r#""tokyo""#
        );
        let legacy: Config = serde_json::from_str(r#"{"state":{"ui":{"preset":"nightlight"}}}"#)
            .expect("legacy ui config");
        assert_eq!(legacy.state.ui.nightlight_theme, NightlightTheme::Live);
        assert!(legacy.state.ui.live_accent);

        let unknown: Config = serde_json::from_str(
            r#"{"state":{"ui":{"nightlight_theme":"solarized","live_accent":false}}}"#,
        )
        .expect("unknown theme falls back");
        assert_eq!(unknown.state.ui.nightlight_theme, NightlightTheme::Live);
        assert!(!unknown.state.ui.live_accent);
    }

    #[test]
    fn nightlight_theme_cycles_through_all_eight() {
        let mut theme = NightlightTheme::Live;
        for _ in 0..NightlightTheme::ALL.len() {
            theme = theme.next(1);
        }
        assert_eq!(theme, NightlightTheme::Live);
        assert_eq!(NightlightTheme::Live.next(-1), NightlightTheme::Phosphor);
        assert_eq!(NightlightTheme::ALL.len(), 8);
        for (index, theme) in NightlightTheme::ALL.iter().enumerate() {
            assert_eq!(NightlightTheme::from_code(theme.code()), *theme);
            assert_eq!(
                NightlightTheme::ALL
                    .iter()
                    .position(|candidate| candidate == theme),
                Some(index)
            );
            assert_eq!(theme.label_key(), format!("theme.{}", theme.code()));
        }
    }

    #[test]
    fn paper_theme_round_trips_and_falls_back_to_ivory() {
        assert_eq!(
            serde_json::to_string(&PaperTheme::Kraft).expect("serialize paper"),
            r#""kraft""#
        );

        // A config written before the paper preset existed has no field at all.
        let legacy: Config = serde_json::from_str(r#"{"state":{"ui":{"preset":"paper"}}}"#)
            .expect("legacy paper config");
        assert_eq!(legacy.state.ui.paper_theme, PaperTheme::Ivory);

        let unknown: Config =
            serde_json::from_str(r#"{"state":{"ui":{"paper_theme":"newsprint"}}}"#)
                .expect("unknown paper falls back");
        assert_eq!(unknown.state.ui.paper_theme, PaperTheme::Ivory);

        let kraft: Config = serde_json::from_str(r#"{"state":{"ui":{"paper_theme":"kraft"}}}"#)
            .expect("kraft paper");
        assert_eq!(kraft.state.ui.paper_theme, PaperTheme::Kraft);
    }

    #[test]
    fn paper_theme_cycles_through_three_stocks() {
        assert_eq!(PaperTheme::Ivory.next(1), PaperTheme::Ash);
        assert_eq!(PaperTheme::Ash.next(1), PaperTheme::Kraft);
        assert_eq!(PaperTheme::Kraft.next(1), PaperTheme::Ivory);
        assert_eq!(PaperTheme::Ivory.next(-1), PaperTheme::Kraft);
        assert_eq!(PaperTheme::ALL.len(), 3);
        assert_eq!(PaperTheme::from_code(" Kraft "), PaperTheme::Kraft);
        for theme in PaperTheme::ALL {
            assert_eq!(PaperTheme::from_code(theme.code()), theme);
            assert_eq!(theme.label_key(), format!("paper.theme.{}", theme.code()));
        }
    }

    #[test]
    fn shelf_theme_round_trips_and_falls_back_to_lavender() {
        assert_eq!(
            serde_json::to_string(&ShelfTheme::Moss).expect("serialize shelf"),
            r#""moss""#
        );

        // Every config written before this setting existed has no field at all,
        // and must keep painting the sheet shelf shipped with.
        let legacy: Config = serde_json::from_str(r#"{"state":{"ui":{"preset":"shelf"}}}"#)
            .expect("legacy shelf config");
        assert_eq!(legacy.state.ui.shelf_theme, ShelfTheme::Lavender);

        let unknown: Config = serde_json::from_str(r#"{"state":{"ui":{"shelf_theme":"sepia"}}}"#)
            .expect("unknown shelf falls back");
        assert_eq!(unknown.state.ui.shelf_theme, ShelfTheme::Lavender);

        let leather: Config = serde_json::from_str(r#"{"state":{"ui":{"shelf_theme":"leather"}}}"#)
            .expect("leather shelf");
        assert_eq!(leather.state.ui.shelf_theme, ShelfTheme::Leather);
    }

    #[test]
    fn shelf_theme_cycles_through_five_sheets() {
        assert_eq!(ShelfTheme::Lavender.next(1), ShelfTheme::Moss);
        assert_eq!(ShelfTheme::Moss.next(1), ShelfTheme::Slate);
        assert_eq!(ShelfTheme::Slate.next(1), ShelfTheme::Leather);
        assert_eq!(ShelfTheme::Leather.next(1), ShelfTheme::Rose);
        assert_eq!(ShelfTheme::Rose.next(1), ShelfTheme::Lavender);
        assert_eq!(ShelfTheme::Lavender.next(-1), ShelfTheme::Rose);
        assert_eq!(ShelfTheme::ALL.len(), 5);
        assert_eq!(ShelfTheme::from_code(" Slate "), ShelfTheme::Slate);
        for theme in ShelfTheme::ALL {
            assert_eq!(ShelfTheme::from_code(theme.code()), theme);
            assert_eq!(theme.label_key(), format!("shelf.theme.{}", theme.code()));
        }
    }
}
