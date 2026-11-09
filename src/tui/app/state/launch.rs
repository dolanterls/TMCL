//! Launch command assembly and game startup.

use super::*;

impl App {
    pub(in crate::tui::app) fn launch_selected_instance(&mut self) {
        let Some(instance) = self.instances.selected_item().cloned() else {
            self.status = Status::warning(self.i18n.text("status.create_instance_first"));
            return;
        };
        self.launch_instance(instance);
    }

    /// Launches the instance marked 当前, not the row the instance list happens
    /// to be pointing at.
    ///
    /// Browsing the instance list only moves a cursor (`focused`); marking an
    /// instance 当前 is a separate, deliberate action. The overview says 当前实例
    /// on its card, so Enter there has to launch that one even after the player
    /// has clicked through the list.
    pub(in crate::tui::app) fn launch_current_instance(&mut self) {
        let target = self.overview_launch_target().cloned();
        match target {
            Some(instance) => self.launch_instance(instance),
            None => self.status = Status::warning(self.i18n.text("status.create_instance_first")),
        }
    }

    /// The instance a launch from the overview acts on.
    ///
    /// The 当前 instance when one is chosen; otherwise the highlighted row,
    /// which is the best guess the player has made so far.
    pub(in crate::tui::app) fn overview_launch_target(&self) -> Option<&InstanceSummary> {
        self.current_instance()
            .or_else(|| self.instances.selected_item())
    }

    /// The Java list a launch should trust.
    ///
    /// The launcher starts with an empty list and fills it from a background
    /// scan. An empty cache therefore means "not known yet", not "this machine
    /// has no Java": handing it to `prepare` as authoritative is what produced
    /// a `no Java runtime was detected` failure for a launch that happened in
    /// the first moments after startup. `None` lets `prepare` discover instead.
    pub(in crate::tui::app) fn cached_java(
        &self,
    ) -> Option<Vec<crate::core::java::JavaInstallation>> {
        (!self.java.is_empty()).then(|| self.java.clone())
    }

    fn launch_instance(&mut self, instance: InstanceSummary) {
        // One repair round per launch: the flag is consumed here, so a second
        // missing-file check right after a repair stops with an error instead of
        // starting another repair task.
        let after_repair = std::mem::take(&mut self.launch_after_repair);
        // Metadata is a promise, the disk is the truth: repair first, then launch.
        match self.launch_blockers(&instance) {
            Ok(missing) if !missing.is_empty() => {
                if after_repair {
                    self.status = Status::error(self.i18n.text("status.repair_incomplete"));
                    return;
                }
                if !self.config.launch.auto_repair {
                    // Automatic repair is off: ask once, then act.
                    self.modal = Some(Modal::confirm(
                        self.i18n.format(
                            "confirm.repair_instance",
                            &[
                                ("name", &instance.name),
                                ("count", &missing.len().to_string()),
                            ],
                        ),
                        crate::tui::modal::ConfirmAction::RepairInstance(instance.id),
                    ));
                    return;
                }
                self.start_instance_repair(&instance, missing);
                return;
            }
            Ok(_) => {}
            Err(error) => {
                // The metadata itself is unreadable, so no repair can help.
                let failure = crate::core::launch::failure::LaunchFailure::new(
                    crate::core::launch::failure::Stage::Plan,
                    crate::core::launch::failure::FailureKind::MetadataInvalid,
                    crate::core::launch::failure::Action::Verify,
                )
                .with_detail(error.to_string());
                // The status bar gets the short version; the launcher log keeps
                // the full stage/kind/detail so the cause is not lost.
                self.record_launcher_log(&failure.describe());
                self.status = Status::error(self.i18n.format(
                    "status.launch_blocked",
                    &[
                        ("reason", &self.i18n.text(failure.i18n_key())),
                        ("action", &self.i18n.text(failure.action_i18n_key())),
                    ],
                ));
                return;
            }
        }
        let Some(account) = self.current_account() else {
            self.status = Status::warning(self.i18n.text("status.account_for_launch"));
            return;
        };
        let game_directory = match crate::core::instance::path::game_directory_of(&instance.path) {
            Ok(path) => path,
            Err(error) => {
                self.status = Status::error(error.to_string());
                return;
            }
        };
        let request = crate::core::launch::session::LaunchRequest {
            instance: resolved_instance(&instance, &game_directory),
            account_id: Some(account.id),
            java_installations: self.cached_java(),
            java_override: None,
            // The TUI has never downloaded a runtime at launch time; that stays
            // a deliberate decision so the launch never starts a long download
            // the player did not ask for.
            allow_java_download: false,
            client_json: None,
            min_memory_mb: None,
            max_memory_mb: None,
            width: None,
            height: None,
            fullscreen: None,
        };

        let sender = self.sender.clone();
        // Separate handle: the game-log forwarder below owns its own clone, and
        // the stage callback runs on the same task that later reports the end.
        let stage_sender = sender.clone();
        let language = self.i18n.language();
        let paths = self.paths.clone();
        let config = self.config.clone();
        let process_priority = self.config.launcher.process_priority;
        let game_debug = self.config.logs.game_debug;
        let instance_id = instance.id;
        let instance_directory = instance.path.clone();
        let (game_events, mut game_event_receiver) = tokio::sync::mpsc::unbounded_channel();
        let event_sender = sender.clone();
        tokio::spawn(async move {
            while let Some(event) = game_event_receiver.recv().await {
                if event_sender.send(AppEvent::Game(event)).is_err() {
                    break;
                }
            }
        });
        // The Microsoft branch reports its own failure text: the refresh chain
        // is the part that can fail before anything else is attempted.
        let microsoft = account.kind == AccountKind::Microsoft;
        self.status = Status::info(if microsoft {
            self.i18n
                .format("status.refresh_session", &[("name", &instance.name)])
        } else {
            self.i18n
                .format("status.starting_instance", &[("name", &instance.name)])
        });
        self.active_launch = Some(ActiveLaunch::starting(instance.id, instance.name.clone()));
        // The list has to say the instance is starting before the process can
        // report anything: `GameStarted` arrives one spawn later.
        self.sync_launch_status();
        tokio::spawn(async move {
            let i18n = I18n::new(language);
            let mut local_authlib = None;
            let result = async {
                let prepared =
                    crate::core::launch::session::prepare(request, &paths, &config, |stage| {
                        // Best-effort: a closed receiver means the app is
                        // shutting down, not that the launch failed.
                        let _ = stage_sender.send(AppEvent::Status(stage_status(&i18n, stage)));
                    })
                    .await?;
                let mut spec = prepared.spec;
                local_authlib =
                    authlib::LocalAuthlibService::start(&paths.root, &prepared.account).await?;
                if let Some(service) = &local_authlib {
                    let main_class = spec
                        .args
                        .iter()
                        .position(|argument| argument == &prepared.client.main_class)
                        .ok_or_else(|| {
                            Error::Launch(
                                "cannot locate Minecraft main class in launch command".into(),
                            )
                        })?;
                    spec.args
                        .splice(main_class..main_class, service.jvm_args.clone());
                }
                // Captured before the run so the diagnosis bundle can ship
                // `launch.txt`: the log says what the game printed, not what
                // the JVM was asked to run.
                let command = process::describe_command(&spec);
                let result = process::run_with_events_and_priority_and_logging(
                    &spec,
                    &prepared.log_path,
                    Some(game_events),
                    game_debug,
                    process_priority,
                )
                .await?;
                Ok::<_, Error>((result, command))
            }
            .await;
            if let Some(service) = local_authlib {
                service.stop().await;
            }
            match result {
                Ok((result, command)) => {
                    let _ = sender.send(AppEvent::LaunchFinished {
                        instance_id,
                        instance_directory,
                        play_time_seconds: result.duration.as_secs(),
                        exit_code: result.exit_code,
                        log_path: result.log_path,
                        crash_report: result.crash_report,
                        launch_command: command,
                    });
                }
                Err(error) => {
                    let key = if microsoft {
                        "status.microsoft_launch_failed"
                    } else {
                        "status.launch_failed"
                    };
                    let _ = sender.send(AppEvent::LaunchAborted {
                        error: i18n.format(key, &[("error", &error.to_string())]),
                    });
                }
            }
        });
    }
}

impl App {
    /// Writes the launch command to a script next to the instance.
    ///
    /// The token is replaced by an environment variable reference, so the file
    /// can be attached to a bug report as-is. Offline sessions use the literal
    /// `0`, which is what the game expects and is not a secret.
    pub(in crate::tui::app) fn export_launch_script(&mut self) {
        let instance = if self.workspace_is_open() {
            self.workspace
                .as_ref()
                .and_then(|workspace| {
                    self.instances
                        .items
                        .iter()
                        .find(|item| item.id == workspace.instance)
                })
                .cloned()
        } else {
            self.instances.selected_item().cloned()
        };
        let Some(instance) = instance else {
            self.status = Status::warning(self.i18n.text("status.create_instance_first"));
            return;
        };
        let Some(account) = self.current_account() else {
            self.status = Status::warning(self.i18n.text("status.account_for_launch"));
            return;
        };
        let hidden = account.kind == AccountKind::Microsoft;
        let built_token = if hidden {
            crate::core::launch::script::TOKEN_SENTINEL
        } else {
            "0"
        };
        let built = crate::core::instance::path::game_directory_of(&instance.path).and_then(
            |game_directory| {
                let resolved = resolved_instance(&instance, &game_directory);
                let client = crate::core::launch::session::load_client(&resolved, None)?;
                crate::core::launch::session::build_command(
                    crate::core::launch::session::CommandRequest {
                        instance_directory: &resolved.config.path,
                        instance_name: &resolved.config.name,
                        minecraft_version: &resolved.config.minecraft_version,
                        game_directory: &resolved.game_directory,
                        config: &self.config,
                        overrides: &resolved.config.overrides,
                        java_installations: &self.java,
                        java_override: None,
                        account: &account,
                        token: built_token,
                        client: &client,
                        logs_dir: &self.paths.game_logs_dir,
                        min_memory_mb: None,
                        max_memory_mb: None,
                        width: None,
                        height: None,
                        fullscreen: None,
                    },
                )
            },
        );
        let (spec, _) = match built {
            Ok(value) => value,
            Err(error) => {
                self.status = Status::error(error.to_string());
                return;
            }
        };
        let shell = crate::core::launch::script::ScriptShell::for_host();
        let rendered = crate::core::launch::script::render(
            &spec,
            shell,
            if hidden { built_token } else { "" },
        );
        let path = instance
            .path
            .join(format!("tmcl-launch.{}", shell.extension()));
        if let Err(error) = std::fs::write(&path, &rendered.contents) {
            self.status = Status::error(error.to_string());
            return;
        }
        self.status = match rendered.warnings.first() {
            Some(warning) => Status::warning(self.i18n.format(
                "status.script_exported_warning",
                &[("path", &path.display().to_string()), ("warning", warning)],
            )),
            None => Status::info(self.i18n.format(
                "status.script_exported",
                &[("path", &path.display().to_string())],
            )),
        };
    }
}

/// Instance metadata plus the game directory it belongs to.
///
/// The workspace reads the metadata itself; a missing or unreadable file falls
/// back to values derived from the folder, which is what a launch of an
/// instance created by another launcher needs.
fn resolved_instance(
    summary: &InstanceSummary,
    game_directory: &Path,
) -> crate::core::instance::resolve::ResolvedInstance {
    let mut config = crate::core::config::storage::read_json::<
        crate::core::instance::models::InstanceConfig,
    >(&summary.path.join("tmcl.json"))
    .unwrap_or_else(|_| {
        crate::core::instance::models::InstanceConfig::new(
            &summary.path,
            crate::core::instance::path::game_directory_id(game_directory),
            &summary.name,
            &summary.version,
        )
    });
    config.path = summary.path.clone();
    crate::core::instance::resolve::ResolvedInstance {
        config,
        game_directory: game_directory.to_path_buf(),
        source: crate::core::instance::resolve::RefSource::Path,
    }
}

/// Builds the launch plan for an installed instance.
fn plan_for_instance(
    instance: &InstanceSummary,
) -> Result<(crate::core::launch::plan::LaunchPlan, std::path::PathBuf)> {
    let game_directory = crate::core::instance::path::game_directory_of(&instance.path)?;
    let client_path = instance.path.join(format!("{}.json", instance.name));
    let client =
        client_json::parse(&std::fs::read(client_path)?)?.resolve_inheritance(&game_directory)?;
    let plan = crate::core::launch::plan::plan(&instance.path, &game_directory, &client);
    Ok((plan, game_directory))
}

impl App {
    /// Files the instance is missing. Empty means the launch can proceed.
    ///
    /// Instances without readable metadata are left to the launch path: repair
    /// cannot invent a version JSON, and guessing would block the account and
    /// Java checks that give the user a better message.
    pub(in crate::tui::app) fn launch_blockers(
        &self,
        instance: &InstanceSummary,
    ) -> Result<Vec<crate::core::launch::preflight::MissingFile>> {
        let client_json = instance.path.join(format!("{}.json", instance.name));
        if !client_json.is_file() {
            return Ok(Vec::new());
        }
        let (plan, _) = plan_for_instance(instance)?;
        Ok(crate::core::launch::preflight::inspect_plan(&plan))
    }

    pub(in crate::tui::app) fn start_instance_repair(
        &mut self,
        instance: &InstanceSummary,
        missing: Vec<crate::core::launch::preflight::MissingFile>,
    ) {
        let (_, game_directory) = match plan_for_instance(instance) {
            Ok(value) => value,
            Err(error) => {
                self.status = Status::error(error.to_string());
                return;
            }
        };
        let spec = crate::core::task::InstanceRepairSpec {
            instance_id: instance.id,
            instance_name: instance.name.clone(),
            instance_directory: instance.path.clone(),
            game_directory,
            cache_root: self
                .config
                .downloads
                .cache_dir
                .clone()
                .unwrap_or_else(|| self.paths.cache_dir.clone()),
            missing: missing.clone(),
            download_snapshot: self.download_snapshot(),
        };
        match self
            .task_engine
            .submit(crate::core::task::TaskOperation::InstanceRepair(spec))
        {
            Ok(_) => {
                self.pending_launch = Some(instance.id);
                self.status = Status::info(self.i18n.format(
                    "status.repair_started",
                    &[
                        ("name", &instance.name),
                        ("count", &missing.len().to_string()),
                    ],
                ));
            }
            Err(error) => self.status = Status::error(error.to_string()),
        }
    }
}

/// Status-bar text for one launch stage.
///
/// Kept next to the launch path rather than in `core`: the CLI prints its own
/// English labels, and the TUI has to translate.
fn stage_status(i18n: &I18n, stage: crate::core::launch::session::LaunchStage) -> String {
    use crate::core::launch::session::LaunchStage;
    match stage {
        LaunchStage::ResolvingAccount => i18n.text("status.launch_stage.resolving_account"),
        LaunchStage::SelectingJava => i18n.text("status.launch_stage.selecting_java"),
        LaunchStage::DownloadingJava { major } => i18n.format(
            "status.launch_stage.downloading_java",
            &[("major", &major.to_string())],
        ),
        LaunchStage::PreparingNatives => i18n.text("status.launch_stage.preparing_natives"),
        LaunchStage::BuildingCommand => i18n.text("status.launch_stage.building_command"),
        LaunchStage::VerifyingFiles => i18n.text("status.launch_stage.verifying_files"),
    }
}
