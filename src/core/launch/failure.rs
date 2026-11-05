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
        match self.action {
            Action::Verify => "failure.action_verify",
            Action::Repair => "failure.action_repair",
            Action::InstallJava => "failure.action_install_java",
            Action::ChangeJava => "failure.action_change_java",
            Action::ReinstallLoader => "failure.action_reinstall_loader",
            Action::None => "failure.action_none",
        }
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

    use super::{Action, FailureKind, LaunchFailure, Stage};

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
