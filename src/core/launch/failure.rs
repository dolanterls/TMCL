//! Typed launch failures.
//!
//! A launch can fail at four different points, and each one needs a different
//! user action. Carrying that classification instead of a flat string lets the
//! status bar say what to do and the log keep the raw detail.

use crate::core::error::Error;

/// Prefix used by repair errors caused by connectivity rather than broken
/// metadata. The UI maps it to an offline-specific message.
pub const OFFLINE_MARKER: &str = "offline:";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Plan,
    Repair,
    Spawn,
    Runtime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    JavaMissing,
    JavaTooOld,
    ClientJarMissing,
    LibraryMissing,
    AssetMissing,
    MetadataInvalid,
    InstallerFailed,
    ProcessExited,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Verify,
    Repair,
    InstallJava,
    ChangeJava,
    /// Produced from M3 on, when loader installation moves to the official
    /// installer and can be retried from the UI.
    ReinstallLoader,
    None,
    /// Raise the instance's maximum heap. The number is computed when the
    /// action runs (`memory::recommend`), not baked into a rule table.
    RaiseMemory,
    LowerMemory,
    /// Open one of the instance's content folders.
    OpenContentDirectory(ContentDir),
    /// Open TMCL's own download centre, filtered by the cause when it names a
    /// mod. Not a browser link.
    OpenDownloadCenter,
    OpenInstanceDirectory,
    OpenCrashReport,
    /// An outside page: driver downloads, a compatibility table, a wiki.
    OpenUrl(UrlKey),
}

/// The content folders a diagnosis can point at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentDir {
    Mods,
    ResourcePacks,
    Shaders,
}

impl Action {
    /// The one catalogue key for an action.
    ///
    /// `LaunchFailure` and the diagnosis card both read this: two copies of a
    /// thirteen-branch match is a guarantee that one of them drifts.
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Verify => "failure.action_verify",
            Self::Repair => "failure.action_repair",
            Self::InstallJava => "failure.action_install_java",
            Self::ChangeJava => "failure.action_change_java",
            Self::ReinstallLoader => "failure.action_reinstall_loader",
            Self::None => "failure.action_none",
            Self::RaiseMemory => "diagnose.remedy.raise_memory",
            Self::LowerMemory => "diagnose.remedy.lower_memory",
            Self::OpenContentDirectory(kind) => kind.label_key(),
            Self::OpenDownloadCenter => "diagnose.remedy.open_downloads",
            Self::OpenInstanceDirectory => "diagnose.remedy.open_instance_dir",
            Self::OpenCrashReport => "diagnose.remedy.open_report",
            Self::OpenUrl(key) => key.label_key(),
        }
    }

    /// The key cap the diagnosis card prints next to a remedy, and therefore the
    /// key that runs it. `None` means the action has no cap: `Verify` and friends
    /// belong to launch failures, and `ReinstallLoader` arrives in M3.
    ///
    /// Two memory actions share `M` on purpose: a rule asks for one direction,
    /// never both, and a second cap for the same idea would be noise.
    pub const fn key_cap(self) -> Option<char> {
        match self {
            Self::ChangeJava => Some('F'),
            Self::InstallJava => Some('N'),
            Self::RaiseMemory | Self::LowerMemory => Some('M'),
            Self::Repair => Some('R'),
            Self::OpenContentDirectory(_) => Some('D'),
            Self::OpenDownloadCenter => Some('W'),
            Self::OpenInstanceDirectory => Some('O'),
            Self::OpenCrashReport => Some('C'),
            Self::OpenUrl(_) => Some('U'),
            Self::Verify | Self::ReinstallLoader | Self::None => None,
        }
    }
}

impl ContentDir {
    /// The folder inside the instance.
    pub const fn folder(self) -> &'static str {
        match self {
            Self::Mods => "mods",
            Self::ResourcePacks => "resourcepacks",
            Self::Shaders => "shaderpacks",
        }
    }

    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Mods => "diagnose.remedy.open_mods",
            Self::ResourcePacks => "diagnose.remedy.open_resource_packs",
            Self::Shaders => "diagnose.remedy.open_shaders",
        }
    }
}

/// External pages a rule may send the player to. Kept in one enum so the URLs
/// live in one table (`cli`/`tui` map a key to a URL) and can be audited or
/// replaced without touching the rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlKey {
    Gpu,
    Forge,
    OptiFine,
    Sodium,
    Jeid,
    MixinBootstrap,
    NightConfig,
    Map,
    Pagefile,
    Locale,
}

impl UrlKey {
    /// Where the key points.
    ///
    /// The six project keys go to the project page, not to a search: those
    /// pages are stable and are what the advice means. The four documentary
    /// keys (driver, page file, locale, world editor) have no single canonical
    /// page — a vendor download link would be wrong for half the machines — so
    /// they open a search with the exact phrase. M3 audits the table; a link
    /// that 404s is a bug, and a search never 404s.
    pub const fn url(self) -> &'static str {
        // Stability is noted per entry. A project page moves only when the
        // project does; a search page cannot 404 but can drift as the search
        // engine changes. Nothing here is checked by CI on purpose: the four
        // search links would make that check an internet-dependent flake.
        match self {
            // Search, not a vendor: NVIDIA, AMD and Intel each have their own
            // page, and half of them would be wrong on any given machine.
            Self::Gpu => "https://duckduckgo.com/?q=update+graphics+driver+windows",
            // Stable: the official Forge file index.
            Self::Forge => "https://files.minecraftforge.net/net/minecraftforge/forge/",
            // Stable: OptiFine has one download index and no deep links.
            Self::OptiFine => "https://optifine.net/downloads",
            // Stable: Modrinth project pages; the slug is the identity.
            Self::Sodium => "https://modrinth.com/mod/sodium",
            // Stable: Modrinth project page. The mod is old and unmaintained,
            // but the page is the only canonical description of it.
            Self::Jeid => "https://modrinth.com/mod/jeid",
            // Stable: Modrinth project page.
            Self::MixinBootstrap => "https://modrinth.com/mod/mixinbootstrap",
            // Search: the mod has no confirmed canonical slug, and a wrong slug
            // is a 404 where this at least lands on the right results.
            Self::NightConfig => "https://modrinth.com/mods?q=night+config+fixes",
            // Search: "map editor" is a category, not a product; the phrase
            // names the task the advice is asking for.
            Self::Map => "https://duckduckgo.com/?q=minecraft+world+editor+remove+entity",
            // Search: the Windows dialog differs between 10 and 11.
            Self::Pagefile => "https://duckduckgo.com/?q=windows+increase+virtual+memory+pagefile",
            // Search: the setting moved in Windows 11 and the beta toggle is
            // gone in newer builds; a fixed page would be wrong half the time.
            Self::Locale => "https://duckduckgo.com/?q=windows+change+system+locale+utf-8+beta",
        }
    }

    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Gpu => "diagnose.url.gpu",
            Self::Forge => "diagnose.url.forge",
            Self::OptiFine => "diagnose.url.optifine",
            Self::Sodium => "diagnose.url.sodium",
            Self::Jeid => "diagnose.url.jeid",
            Self::MixinBootstrap => "diagnose.url.mixinbootstrap",
            Self::NightConfig => "diagnose.url.nightconfig",
            Self::Map => "diagnose.url.map",
            Self::Pagefile => "diagnose.url.pagefile",
            Self::Locale => "diagnose.url.locale",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchFailure {
    pub stage: Stage,
    pub kind: FailureKind,
    pub detail: Vec<String>,
    pub action: Action,
}

const MAX_DETAIL: usize = 5;

impl LaunchFailure {
    pub fn new(stage: Stage, kind: FailureKind, action: Action) -> Self {
        Self {
            stage,
            kind,
            detail: Vec::new(),
            action,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        if self.detail.len() < MAX_DETAIL {
            self.detail.push(detail.into());
        } else if let Some(last) = self.detail.last_mut()
            && !last.ends_with('…')
        {
            // The list is capped so a broken install cannot flood the log. Mark
            // the cut once, otherwise a reader cannot tell details were lost.
            last.push('…');
        }
        self
    }

    pub fn i18n_key(&self) -> &'static str {
        match self.kind {
            FailureKind::JavaMissing => "failure.java_missing",
            FailureKind::JavaTooOld => "failure.java_too_old",
            FailureKind::ClientJarMissing => "failure.client_jar_missing",
            FailureKind::LibraryMissing => "failure.library_missing",
            FailureKind::AssetMissing => "failure.asset_missing",
            FailureKind::MetadataInvalid => "failure.metadata_invalid",
            FailureKind::InstallerFailed => "failure.installer_failed",
            FailureKind::ProcessExited => "failure.process_exited",
        }
    }

    pub fn action_i18n_key(&self) -> &'static str {
        self.action.label_key()
    }

    pub fn describe(&self) -> String {
        let mut message = format!("launch failed at {:?}: {:?}", self.stage, self.kind);
        if !self.detail.is_empty() {
            message.push_str(" (");
            message.push_str(&self.detail.join("; "));
            message.push(')');
        }
        message
    }
}

impl From<LaunchFailure> for Error {
    fn from(failure: LaunchFailure) -> Self {
        Error::Launch(failure.describe())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{Action, ContentDir, FailureKind, LaunchFailure, Stage, UrlKey};

    const KINDS: [FailureKind; 8] = [
        FailureKind::JavaMissing,
        FailureKind::JavaTooOld,
        FailureKind::ClientJarMissing,
        FailureKind::LibraryMissing,
        FailureKind::AssetMissing,
        FailureKind::MetadataInvalid,
        FailureKind::InstallerFailed,
        FailureKind::ProcessExited,
    ];

    #[test]
    fn every_failure_kind_maps_to_a_distinct_message_key() {
        let keys = KINDS
            .iter()
            .map(|kind| LaunchFailure::new(Stage::Spawn, *kind, Action::None).i18n_key())
            .collect::<BTreeSet<_>>();
        assert_eq!(keys.len(), KINDS.len());
        assert!(keys.iter().all(|key| key.starts_with("failure.")));
    }

    fn every_action() -> Vec<Action> {
        let mut actions = vec![
            Action::Verify,
            Action::Repair,
            Action::InstallJava,
            Action::ChangeJava,
            Action::ReinstallLoader,
            Action::None,
            Action::RaiseMemory,
            Action::LowerMemory,
            Action::OpenDownloadCenter,
            Action::OpenInstanceDirectory,
            Action::OpenCrashReport,
        ];
        for kind in [
            ContentDir::Mods,
            ContentDir::ResourcePacks,
            ContentDir::Shaders,
        ] {
            actions.push(Action::OpenContentDirectory(kind));
        }
        for key in [
            UrlKey::Gpu,
            UrlKey::Forge,
            UrlKey::OptiFine,
            UrlKey::Sodium,
            UrlKey::Jeid,
            UrlKey::MixinBootstrap,
            UrlKey::NightConfig,
            UrlKey::Map,
            UrlKey::Pagefile,
            UrlKey::Locale,
        ] {
            actions.push(Action::OpenUrl(key));
        }
        actions
    }

    /// A card that prints a key promises it works; a cap without a key handler
    /// is the same defect as a hint without a command.
    #[test]
    fn every_action_has_a_label_and_at_most_one_cap() {
        let labels = every_action()
            .into_iter()
            .map(Action::label_key)
            .collect::<BTreeSet<_>>();
        assert_eq!(labels.len(), 24, "an action lost its key");
        assert!(labels.iter().all(|key| key.contains('.')));

        let caps = every_action()
            .into_iter()
            .filter_map(Action::key_cap)
            .collect::<BTreeSet<_>>();
        assert_eq!(caps.len(), 9, "{caps:?}");
        assert!(!caps.contains(&'Q'), "q quits the launcher");
        assert!(!caps.contains(&'E'), "e exports the bundle");
        assert!(!caps.contains(&'L'), "l jumps to the log");
    }

    /// The folders are spelled again here, so they are checked against the
    /// content module's own spelling instead of drifting.
    #[test]
    fn the_content_folders_match_the_content_module() {
        assert_eq!(
            ContentDir::Mods.folder(),
            crate::core::content::ContentKind::Mod.directory()
        );
        assert_eq!(
            ContentDir::ResourcePacks.folder(),
            crate::core::content::ContentKind::ResourcePack.directory()
        );
        assert_eq!(
            ContentDir::Shaders.folder(),
            crate::core::content::ContentKind::Shader.directory()
        );
    }

    #[test]
    fn every_url_is_https_and_reachable_by_a_browser() {
        let urls = every_action()
            .into_iter()
            .filter_map(|action| match action {
                Action::OpenUrl(key) => Some(key.url()),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(urls.len(), 10);
        assert!(urls.iter().all(|url| url.starts_with("https://")));
    }

    #[test]
    fn details_are_capped_at_five_entries() {
        let failure = (0..8).fold(
            LaunchFailure::new(Stage::Repair, FailureKind::LibraryMissing, Action::Repair),
            |failure, index| failure.with_detail(format!("file-{index}.jar")),
        );
        assert_eq!(failure.detail.len(), 5);
        assert_eq!(
            failure.detail.last().map(String::as_str),
            Some("file-4.jar…"),
            "the cap is marked once so truncation is visible"
        );
    }

    #[test]
    fn failure_converts_to_launch_error_with_details() {
        let failure = LaunchFailure::new(Stage::Plan, FailureKind::MetadataInvalid, Action::None)
            .with_detail("versions/pack/pack.json");
        let error = crate::core::error::Error::from(failure);
        let message = error.to_string();
        assert!(message.contains("MetadataInvalid"), "{message}");
        assert!(message.contains("pack.json"), "{message}");
    }
}
