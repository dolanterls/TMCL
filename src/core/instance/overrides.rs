//! Per-instance overrides for launch settings.
//!
//! Isolation in TMCL is structural: `--gameDir` is always the instance directory,
//! so mods and saves never mix. How to launch still comes from the global config
//! (memory, window, Java, arguments, auto-connect), and this module adds the
//! missing per-instance layer.
//!
//! Every field is an `Option`. `None` means "follow the global value", which is
//! why an instance created by an older build (no `overrides` key at all) keeps
//! behaving exactly as before. A total "enable instance settings" switch was
//! deliberately not used: it freezes a snapshot of the globals, so later global
//! edits silently stop applying and unchecking it loses every custom value.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::core::config::models::{Config, JavaSelectionMode, MemoryMode};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaunchOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_mode: Option<MemoryMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_memory_mb: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_memory_mb: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fullscreen: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub java_path: Option<PathBuf>,
    /// `Some(vec![])` clears the global arguments on purpose.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jvm_args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game_args: Option<Vec<String>>,
    /// `Some("")` means "do not auto-connect", which is different from
    /// `None` ("use the global address"). A nested `Option` cannot express
    /// this: serde folds JSON `null` into the outer `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_address: Option<String>,
}

impl LaunchOverrides {
    pub fn is_empty(&self) -> bool {
        self.overridden_count() == 0
    }

    pub fn overridden_count(&self) -> usize {
        [
            self.memory_mode.is_some(),
            self.min_memory_mb.is_some(),
            self.max_memory_mb.is_some(),
            self.width.is_some(),
            self.height.is_some(),
            self.fullscreen.is_some(),
            self.java_path.is_some(),
            self.jvm_args.is_some(),
            self.game_args.is_some(),
            self.server_address.is_some(),
        ]
        .into_iter()
        .filter(|overridden| *overridden)
        .count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JavaChoice {
    /// Pick a runtime that satisfies the version's minimum requirement.
    Auto,
    Fixed(PathBuf),
}

/// The values a launch uses, after merging globals with overrides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveLaunch {
    pub memory_mode: MemoryMode,
    pub min_memory_mb: u32,
    pub max_memory_mb: u32,
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
    pub java: JavaChoice,
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
    pub server_address: Option<String>,
}

impl EffectiveLaunch {
    /// The single place that knows the precedence. Both the TUI and the CLI
    /// build their command from this, so they cannot drift apart.
    pub fn resolve(config: &Config, overrides: &LaunchOverrides) -> Self {
        // A manual memory bound without an explicit mode still means manual:
        // otherwise `memory::resolve` would run the automatic recommendation and
        // silently ignore the number the user typed.
        let manual_memory = overrides.memory_mode == Some(MemoryMode::Manual)
            || overrides.min_memory_mb.is_some()
            || overrides.max_memory_mb.is_some();
        let memory_mode = if manual_memory {
            MemoryMode::Manual
        } else {
            config.launch.memory_mode
        };

        let java = match &overrides.java_path {
            Some(path) => JavaChoice::Fixed(path.clone()),
            None => match config.java.selection_mode {
                JavaSelectionMode::Fixed => config
                    .java
                    .selected_path
                    .clone()
                    .map(JavaChoice::Fixed)
                    .unwrap_or(JavaChoice::Auto),
                JavaSelectionMode::Automatic => JavaChoice::Auto,
            },
        };

        let server_address = match &overrides.server_address {
            Some(value) if value.trim().is_empty() => None,
            Some(value) => Some(value.clone()),
            None => config.launcher.server_address.clone(),
        };

        Self {
            memory_mode,
            min_memory_mb: overrides
                .min_memory_mb
                .unwrap_or(config.launch.min_memory_mb),
            max_memory_mb: overrides
                .max_memory_mb
                .unwrap_or(config.launch.max_memory_mb),
            width: overrides.width.unwrap_or(config.launch.width),
            height: overrides.height.unwrap_or(config.launch.height),
            fullscreen: overrides.fullscreen.unwrap_or(config.launch.fullscreen),
            java,
            jvm_args: overrides
                .jvm_args
                .clone()
                .unwrap_or_else(|| config.launch.jvm_args.clone()),
            game_args: overrides
                .game_args
                .clone()
                .unwrap_or_else(|| config.launch.game_args.clone()),
            server_address,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EffectiveLaunch, JavaChoice, LaunchOverrides};
    use crate::core::config::models::{Config, JavaSelectionMode, MemoryMode};
    use std::path::PathBuf;

    fn config() -> Config {
        let mut config = Config::default();
        config.launch.memory_mode = MemoryMode::Automatic;
        config.launch.min_memory_mb = 1024;
        config.launch.max_memory_mb = 4096;
        config.launch.width = 1280;
        config.launch.height = 720;
        config.launch.jvm_args = vec!["-XX:+UseG1GC".into()];
        config.launcher.server_address = Some("mc.example.com:25565".into());
        config
    }

    #[test]
    fn empty_overrides_match_global() {
        let config = config();
        let effective = EffectiveLaunch::resolve(&config, &LaunchOverrides::default());
        assert_eq!(effective.memory_mode, config.launch.memory_mode);
        assert_eq!(effective.min_memory_mb, config.launch.min_memory_mb);
        assert_eq!(effective.max_memory_mb, config.launch.max_memory_mb);
        assert_eq!(effective.width, config.launch.width);
        assert_eq!(effective.height, config.launch.height);
        assert_eq!(effective.fullscreen, config.launch.fullscreen);
        assert_eq!(effective.jvm_args, config.launch.jvm_args);
        assert_eq!(effective.game_args, config.launch.game_args);
        assert_eq!(effective.server_address, config.launcher.server_address);
        assert_eq!(effective.java, JavaChoice::Auto);
    }

    #[test]
    fn partial_override_keeps_rest_following() {
        let config = config();
        let overrides = LaunchOverrides {
            width: Some(1920),
            ..Default::default()
        };
        let effective = EffectiveLaunch::resolve(&config, &overrides);
        assert_eq!(effective.width, 1920);
        assert_eq!(effective.height, 720);
        assert_eq!(effective.max_memory_mb, 4096);
        assert_eq!(overrides.overridden_count(), 1);
        assert!(!overrides.is_empty());
    }

    #[test]
    fn custom_memory_forces_manual() {
        let config = config();
        let overrides = LaunchOverrides {
            min_memory_mb: Some(2048),
            max_memory_mb: Some(6144),
            ..Default::default()
        };
        let effective = EffectiveLaunch::resolve(&config, &overrides);
        assert_eq!(effective.memory_mode, MemoryMode::Manual);
        assert_eq!(effective.min_memory_mb, 2048);
        assert_eq!(effective.max_memory_mb, 6144);
    }

    #[test]
    fn java_override_beats_selection_mode() {
        let mut config = config();
        config.java.selection_mode = JavaSelectionMode::Automatic;
        let overrides = LaunchOverrides {
            java_path: Some(PathBuf::from("C:/Java/jdk-17/bin/java.exe")),
            ..Default::default()
        };
        assert_eq!(
            EffectiveLaunch::resolve(&config, &overrides).java,
            JavaChoice::Fixed(PathBuf::from("C:/Java/jdk-17/bin/java.exe"))
        );
    }

    #[test]
    fn global_fixed_java_applies_when_instance_follows() {
        let mut config = config();
        config.java.selection_mode = JavaSelectionMode::Fixed;
        config.java.selected_path = Some(PathBuf::from("C:/Java/jdk-21/bin/java.exe"));
        assert_eq!(
            EffectiveLaunch::resolve(&config, &LaunchOverrides::default()).java,
            JavaChoice::Fixed(PathBuf::from("C:/Java/jdk-21/bin/java.exe"))
        );
    }

    #[test]
    fn empty_server_override_disables_auto_connect() {
        let config = config();
        let overrides = LaunchOverrides {
            server_address: Some(String::new()),
            ..Default::default()
        };
        assert_eq!(
            EffectiveLaunch::resolve(&config, &overrides).server_address,
            None
        );
        // A non-empty override replaces the global address.
        let custom = LaunchOverrides {
            server_address: Some("other.example:25565".into()),
            ..Default::default()
        };
        assert_eq!(
            EffectiveLaunch::resolve(&config, &custom)
                .server_address
                .as_deref(),
            Some("other.example:25565")
        );
    }

    #[test]
    fn empty_argument_overrides_clear_global_arguments() {
        let config = config();
        let overrides = LaunchOverrides {
            jvm_args: Some(Vec::new()),
            ..Default::default()
        };
        let effective = EffectiveLaunch::resolve(&config, &overrides);
        assert!(effective.jvm_args.is_empty());
        assert_eq!(effective.game_args, config.launch.game_args);
    }
}
