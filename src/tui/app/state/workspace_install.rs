//! Loader installation from the instance workspace.
//!
//! Installing a loader is the one action here that rewrites the instance's
//! client json, so it always runs through the task engine, always copies the
//! previous json aside first, and asks before replacing a working setup that has
//! mods in it.

use super::*;
use crate::core::instance::config::{InstallableLoader, LoaderState};
use crate::core::task::LoaderSpec;
use crate::core::version::loader_meta::LoaderKind;

/// A loader the player picked but has not confirmed yet.
///
/// `ConfirmAction` is `Copy`, so it cannot carry this; the dialog stores only the
/// unit variant and the payload waits here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::tui::app) struct PendingLoaderInstall {
    pub kind: InstallableLoader,
    /// Loader build; `None` for Vanilla (Minecraft row or uninstall).
    pub loader_version: Option<String>,
    pub minecraft_version: String,
}

impl PendingLoaderInstall {
    fn spec(&self) -> LoaderSpec {
        match (self.kind, &self.loader_version) {
            (_, None) | (InstallableLoader::Minecraft, _) => LoaderSpec::Vanilla,
            (InstallableLoader::Fabric, Some(version)) => LoaderSpec::Fabric {
                version: version.clone(),
            },
            (InstallableLoader::Quilt, Some(version)) => LoaderSpec::Quilt {
                version: version.clone(),
            },
            (InstallableLoader::Forge, Some(version)) => LoaderSpec::Forge {
                version: version.clone(),
            },
            (InstallableLoader::NeoForge, Some(version)) => LoaderSpec::NeoForge {
                version: version.clone(),
            },
            // OptiFine never reaches here; it is not installable.
            _ => LoaderSpec::Vanilla,
        }
    }
}

impl App {
    /// The six install rows for the instance the workspace is showing.
    pub(in crate::tui::app) fn install_statuses(&self) -> Vec<(InstallableLoader, LoaderState)> {
        let Some(workspace) = self.workspace.as_ref() else {
            return Vec::new();
        };
        crate::core::instance::config::loader_status(
            &workspace.root,
            &workspace.draft.name,
            &workspace.draft.minecraft_version,
        )
    }

    fn current_install_row(&self) -> Option<(InstallableLoader, LoaderState)> {
        let workspace = self.workspace.as_ref()?;
        if workspace.category != crate::tui::pages::workspace::WorkspaceCategory::Install {
            return None;
        }
        self.install_statuses().into_iter().nth(workspace.selected)
    }

    /// `Enter` on the install category.
    pub(in crate::tui::app) fn activate_workspace_install(&mut self) {
        let Some((kind, state)) = self.current_install_row() else {
            return;
        };
        if !kind.installable() {
            self.status = Status::info(match kind {
                InstallableLoader::OptiFine => self.i18n.text("workspace.install.optifine_hint"),
                _ => self.i18n.text("workspace.install.unsupported"),
            });
            return;
        }
        if self.workspace_instance_running() {
            self.status = Status::warning(self.i18n.text("workspace.status.running_locked"));
            return;
        }
        if kind == InstallableLoader::Minecraft {
            self.open_minecraft_version_picker();
            return;
        }
        // The loader already occupies this instance: offer to reinstall it, which
        // is also how a different build of the same loader is chosen.
        let _ = state;
        self.spawn_workspace_loader_versions(kind);
    }

    /// Pick a game version from the list the download centre already loaded.
    ///
    /// Fetching the manifest here would duplicate that page's job, and an empty
    /// list simply means the player has not opened it yet.
    fn open_minecraft_version_picker(&mut self) {
        let versions = self
            .versions
            .iter()
            .filter(|version| version.kind == "release")
            .map(|version| version.id.clone())
            .collect::<Vec<_>>();
        if versions.is_empty() {
            self.status = Status::warning(self.i18n.text("workspace.install.load_versions_first"));
            return;
        }
        let options = versions
            .iter()
            .map(|version| format!("{version}|{version}"))
            .collect::<Vec<_>>();
        let current = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.draft.minecraft_version.clone())
            .unwrap_or_default();
        let selected = versions
            .iter()
            .position(|version| version == &current)
            .unwrap_or(0);
        self.modal = Some(
            Modal::form(
                self.i18n.text("workspace.install.pick_minecraft"),
                &[self.i18n.text("workspace.install.version_label")],
                &[options[selected].clone()],
                crate::tui::modal::FormAction::InstallLoader(InstallableLoader::Minecraft),
            )
            .with_selection_picker(0, options),
        );
    }

    pub(in crate::tui::app) fn spawn_workspace_loader_versions(&mut self, kind: InstallableLoader) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let minecraft_version = workspace.draft.minecraft_version.clone();
        let sender = self.sender.clone();
        self.status = Status::info(self.i18n.format(
            "status.loading_loader_versions",
            &[("loader", kind.key()), ("version", &minecraft_version)],
        ));
        tokio::spawn(async move {
            let result = async {
                crate::core::version::loader_meta::list_loader_versions(
                    &crate::core::version::http::HttpClient::new()?,
                    &[
                        crate::core::download::DownloadSource::Official,
                        crate::core::download::DownloadSource::Bmclapi,
                    ],
                    match kind {
                        InstallableLoader::Forge => LoaderKind::Forge,
                        InstallableLoader::NeoForge => LoaderKind::NeoForge,
                        InstallableLoader::Quilt => LoaderKind::Quilt,
                        _ => LoaderKind::Fabric,
                    },
                    &minecraft_version,
                )
                .await
            }
            .await
            .map_err(|error| error.to_string());
            let _ = sender.send(crate::tui::event::AppEvent::WorkspaceLoaderVersions {
                kind,
                minecraft_version,
                result,
            });
        });
    }

    /// Turn a fetched list into the version picker.
    pub(in crate::tui::app) fn apply_workspace_loader_versions(
        &mut self,
        kind: InstallableLoader,
        minecraft_version: String,
        result: std::result::Result<Vec<crate::core::version::loader_meta::LoaderVersion>, String>,
    ) {
        // The workspace may have moved on while the request was in flight.
        let still_here = self.workspace.as_ref().is_some_and(|workspace| {
            workspace.category == crate::tui::pages::workspace::WorkspaceCategory::Install
                && workspace.draft.minecraft_version == minecraft_version
        });
        if !still_here {
            return;
        }
        let versions = match result {
            Ok(versions) if !versions.is_empty() => versions,
            Ok(_) => {
                self.status = Status::warning(self.i18n.text("workspace.install.no_versions"));
                return;
            }
            Err(error) => {
                self.status = Status::error(self.i18n.format(
                    "status.loader_versions_failed",
                    &[("loader", kind.key()), ("error", &error)],
                ));
                return;
            }
        };
        // Stable builds first: a list sorted by the source is not a useful order
        // for someone picking "the current Forge".
        let mut ordered = versions;
        ordered.sort_by_key(|version| std::cmp::Reverse(version.stable));
        let options = ordered
            .iter()
            .map(|version| format!("{}|{}", version.version, version.version))
            .collect::<Vec<_>>();
        self.modal = Some(
            Modal::form(
                self.i18n.text("workspace.install.pick_version"),
                &[self.i18n.text("workspace.install.version_label")],
                &[options[0].clone()],
                crate::tui::modal::FormAction::InstallLoader(kind),
            )
            .with_selection_picker(0, options),
        );
        self.status = Status::info(self.i18n.format(
            "status.loader_versions_loaded",
            &[
                ("count", &ordered.len().to_string()),
                ("loader", kind.key()),
                ("version", &minecraft_version),
            ],
        ));
    }

    /// Handle the picker's answer: confirm if risky, otherwise start at once.
    pub(in crate::tui::app) fn request_loader_install(
        &mut self,
        kind: InstallableLoader,
        value: &str,
    ) {
        let picked = value
            .rsplit_once('|')
            .map(|(_, value)| value)
            .unwrap_or(value);
        let picked = picked.trim();
        if picked.is_empty() {
            return;
        }
        let Some(minecraft_version) = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.draft.minecraft_version.clone())
        else {
            return;
        };
        let pending = if kind == InstallableLoader::Minecraft {
            PendingLoaderInstall {
                kind,
                loader_version: None,
                minecraft_version: picked.to_owned(),
            }
        } else {
            PendingLoaderInstall {
                kind,
                loader_version: Some(picked.to_owned()),
                minecraft_version,
            }
        };
        self.confirm_or_install(pending);
    }

    /// Ask first when the instance has content that the change would orphan.
    fn confirm_or_install(&mut self, pending: PendingLoaderInstall) {
        let changes = self.install_changes(&pending);
        let mods = self.workspace_mod_count();
        if changes && mods > 0 {
            self.pending_loader_install = Some(pending.clone());
            self.modal = Some(Modal::confirm(
                self.i18n.format(
                    "workspace.install.confirm_switch",
                    &[
                        ("count", &mods.to_string()),
                        ("current", &self.installed_loader_label()),
                        ("target", &self.pending_label(&pending)),
                    ],
                ),
                crate::tui::modal::ConfirmAction::SwitchLoader,
            ));
            return;
        }
        self.start_loader_install(pending);
    }

    /// Whether the pick would actually change the instance's loader or version.
    fn install_changes(&self, pending: &PendingLoaderInstall) -> bool {
        let Some(workspace) = self.workspace.as_ref() else {
            return false;
        };
        match pending.kind {
            InstallableLoader::Minecraft => {
                workspace.draft.minecraft_version != pending.minecraft_version
            }
            kind => !matches!(
                crate::core::instance::config::loader_status(
                    &workspace.root,
                    &workspace.draft.name,
                    &workspace.draft.minecraft_version,
                )
                .into_iter()
                .find(|(row, _)| *row == kind)
                .map(|(_, state)| state),
                Some(LoaderState::Installed(_))
            ),
        }
    }

    fn workspace_mod_count(&self) -> usize {
        self.workspace
            .as_ref()
            .map(|workspace| {
                workspace
                    .content
                    .count(crate::core::content::ContentKind::Mod)
            })
            .unwrap_or(0)
    }

    fn installed_loader_label(&self) -> String {
        self.workspace
            .as_ref()
            .map(|workspace| workspace.draft.loader.key().to_owned())
            .unwrap_or_default()
    }

    fn pending_label(&self, pending: &PendingLoaderInstall) -> String {
        match (&pending.loader_version, pending.kind) {
            (_, InstallableLoader::Minecraft) => format!("Minecraft {}", pending.minecraft_version),
            (Some(version), kind) => format!("{} {version}", kind.key()),
            (None, kind) => kind.key().to_owned(),
        }
    }

    /// Copy the client json aside, then queue the install task.
    pub(in crate::tui::app) fn start_loader_install(&mut self, pending: PendingLoaderInstall) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let root = workspace.root.clone();
        let name = workspace.draft.name.clone();
        let instance_id = workspace.instance;
        let directory_id = workspace.draft.game_directory_id;
        let loader = pending.spec();
        let minecraft_version = pending.minecraft_version.clone();

        let json = root.join(format!("{name}.json"));
        if json.is_file() {
            let backup = json.with_file_name(format!(
                "{name}.json.bak-{}",
                crate::core::content::trash::now_stamp()
            ));
            if let Err(error) = std::fs::copy(&json, &backup) {
                self.status = Status::error(self.i18n.format(
                    "workspace.install.backup_failed",
                    &[("error", &error.to_string())],
                ));
                return;
            }
        }

        let Some(game_directory) = self
            .config
            .game_directories
            .iter()
            .find(|path| game_directory_id(path) == directory_id)
            .cloned()
        else {
            self.status = Status::error(self.i18n.text("workspace.install.no_game_directory"));
            return;
        };

        let java_executable = crate::core::java::select_java(
            &self.java,
            crate::core::java::minimum_java_major(&minecraft_version).unwrap_or(21),
        )
        .ok()
        .map(|java| java.executable.clone());

        let spec = crate::core::task::InstanceInstallSpec {
            instance_id,
            game_directory,
            instance_directory: root,
            instance_name: name,
            minecraft_version,
            loader: loader.clone(),
            java_executable,
            download_snapshot: self.download_snapshot(),
        };
        match self
            .task_engine
            .submit(crate::core::task::TaskOperation::InstanceInstall(spec))
        {
            Ok(task_id) => {
                self.apply_task_snapshots(self.task_engine.snapshots());
                self.status = Status::info(self.i18n.format(
                    "workspace.install.queued",
                    &[
                        ("loader", &self.pending_label(&pending)),
                        ("id", &task_id.to_string()),
                    ],
                ));
            }
            Err(error) => self.status = Status::error(error.to_string()),
        }
    }

    /// `d` on the install category: put the instance back to Vanilla.
    pub(in crate::tui::app) fn uninstall_workspace_loader(&mut self) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        if workspace.draft.loader == crate::core::instance::models::InstanceLoader::Vanilla {
            self.status = Status::info(self.i18n.text("workspace.install.already_vanilla"));
            return;
        }
        if self.workspace_instance_running() {
            self.status = Status::warning(self.i18n.text("workspace.status.running_locked"));
            return;
        }
        let minecraft_version = workspace.draft.minecraft_version.clone();
        self.pending_loader_install = Some(PendingLoaderInstall {
            kind: InstallableLoader::Minecraft,
            loader_version: None,
            minecraft_version,
        });
        self.modal = Some(Modal::confirm(
            self.i18n.text("workspace.install.confirm_uninstall"),
            crate::tui::modal::ConfirmAction::SwitchLoader,
        ));
    }

    /// Re-read the client json after an install rewrote it.
    pub(in crate::tui::app) fn refresh_workspace_draft(&mut self) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let root = workspace.root.clone();
        let mut draft = workspace.draft.clone();
        if crate::core::instance::config::sync_from_client_json(&mut draft, &root)
            && let Some(workspace) = self.workspace.as_mut()
        {
            workspace.draft = draft;
        }
    }
}
