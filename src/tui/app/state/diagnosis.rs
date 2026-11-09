//! Running the diagnosis and keeping the result.
//!
//! The analysis reads up to half a megabyte of log and runs 59 rules over it, so
//! it goes to `spawn_blocking`; doing it between two frames would drop a frame
//! while the player watches the launcher. The result returns as
//! [`AppEvent::DiagnosisReady`] with the launch number it belongs to, because a
//! second launch makes the first verdict stale.

use super::*;
use crate::core::diagnose::{Diagnosis, Facts, RunRecord};
// `Action` is also the name of the pointer-gesture enum in `tui::action`; an
// explicit import wins over the glob, and this file only means the remedy.
use crate::core::launch::failure::{Action, ContentDir};

/// How much of the primary finding's evidence line to reuse as a log filter.
/// Long enough to be unique in a log, short enough to be typable.
const FILTER_FRAGMENT: usize = 32;

impl App {
    /// Collect what the verdict needs from `App` and hand the rest to a worker.
    ///
    /// `Facts` is built here because only `App` knows the instance list, the
    /// selected Java and the memory setting; the worker only sees plain data.
    pub(in crate::tui::app) fn start_diagnosis(&mut self, seq: u64, run: RunRecord) {
        let facts = self.diagnosis_facts(&run);
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                let sender = self.sender.clone();
                handle.spawn_blocking(move || {
                    let diagnosis = crate::core::diagnose::for_run(facts, &run);
                    let _ = sender.send(crate::tui::event::AppEvent::DiagnosisReady {
                        seq,
                        diagnosis: Box::new(diagnosis),
                    });
                });
            }
            // Unit tests drive `handle(LaunchFinished)` without a runtime. The
            // analysis is the same code; running it inline keeps the assertion
            // synchronous instead of dropping the verdict into a dead channel.
            Err(_) => {
                let diagnosis = crate::core::diagnose::for_run(facts, &run);
                self.handle_diagnosis_ready(seq, Box::new(diagnosis));
            }
        }
    }

    /// `F3`: show the card, or hide it if it is already up.
    ///
    /// With no verdict there is nothing to show, and an empty card would be
    /// worse than ignoring the press.
    pub(in crate::tui::app) fn toggle_diagnosis(&mut self) {
        if self.diagnosis.is_none() {
            return;
        }
        if self.diagnosis_open {
            self.diagnosis_open = false;
            return;
        }
        if self.modal.is_some() {
            return;
        }
        self.detail_open = false;
        self.diagnosis_scroll = 0;
        self.diagnosis_open = true;
    }

    pub(in crate::tui::app) fn handle_diagnosis_ready(
        &mut self,
        seq: u64,
        diagnosis: Box<Diagnosis>,
    ) {
        if seq != self.launch_seq {
            return;
        }
        let crash = diagnosis.exit.is_crash();
        let reason = diagnosis
            .primary
            .as_ref()
            .map(|finding| self.finding_reason(finding))
            .or_else(|| {
                diagnosis
                    .unknown
                    .then(|| self.i18n.text("diagnose.unknown.reason"))
            });
        self.diagnosis = Some(*diagnosis);
        self.diagnosis_scroll = 0;
        if let Some(reason) = reason {
            self.status = Status::warning(
                self.i18n
                    .format("status.crash_detected", &[("reason", &reason)]),
            );
        }
        // A dialog owns the keyboard while it is open; opening the card on top
        // of a half-filled form would eat the next keystroke.
        if crash && self.config.diagnosis.auto_open && self.modal.is_none() {
            self.detail_open = false;
            self.diagnosis_open = true;
        }
    }

    /// `诊断：内存不足`: the one-line badge the log page and the status bar both
    /// show, so the two never disagree about the verdict.
    pub(in crate::tui::app) fn diagnosis_badge(&self) -> Option<String> {
        let diagnosis = self.diagnosis.as_ref()?;
        let reason = if let Some(primary) = &diagnosis.primary {
            self.finding_reason(primary)
        } else if diagnosis.unknown {
            self.i18n.text("diagnose.unknown.short")
        } else {
            return None;
        };
        Some(self.i18n.format("diagnose.badge", &[("reason", &reason)]))
    }

    /// One finding's sentence, with its captures substituted.
    pub(in crate::tui::app) fn finding_reason(
        &self,
        finding: &crate::core::diagnose::Finding,
    ) -> String {
        let rule = crate::core::diagnose::rules::rule_for(finding.rule);
        self.i18n.format(rule.reason_key, &finding.arguments())
    }

    /// Run one remedy from the card.
    ///
    /// Every branch either does something visible or says why it cannot: a
    /// remedy that silently does nothing is worse than no key cap at all.
    pub(in crate::tui::app) fn run_action(&mut self, action: Action) {
        match action {
            Action::ChangeJava => self.focus_setting(SettingId::JavaSelection),
            // `N` says "install Java"; landing on the same row as `F` would
            // make two caps do one thing and neither of them install anything.
            // The form below is the one the Java page's `i` opens.
            Action::InstallJava => self.open_java_runtime_form(),
            Action::RaiseMemory | Action::LowerMemory => {
                self.focus_setting(SettingId::MemoryMaximum)
            }
            Action::Repair => match self.diagnosis_instance_id() {
                Some(id) => {
                    // Same path as the log page's `R`: one confirmation, one
                    // `ConfirmAction`, so a repair cannot behave differently
                    // depending on how it was asked for.
                    let name = self
                        .last_run
                        .as_ref()
                        .map(|run| run.instance_name.clone())
                        .or_else(|| {
                            self.current_instance()
                                .map(|instance| instance.name.clone())
                        })
                        .unwrap_or_default();
                    self.modal = Some(Modal::confirm(
                        self.i18n
                            .format("diagnosis.confirm_repair", &[("name", &name)]),
                        ConfirmAction::RepairInstance(id),
                    ));
                }
                None => {
                    self.status = Status::warning(self.i18n.text("status.diagnosis_no_instance"));
                }
            },
            Action::OpenContentDirectory(kind) => self.open_content_directory(kind),
            Action::OpenDownloadCenter => self.open_download_center_for_cause(),
            Action::OpenInstanceDirectory => self.open_instance_directory(),
            Action::OpenCrashReport => self.open_crash_report_file(),
            Action::OpenUrl(key) => match crate::platform::windows::open_url(key.url()) {
                Ok(()) => {}
                Err(error) => self.status = Status::error(error.to_string()),
            },
            // Failure-only actions. The card prints no cap for them, so reaching
            // this arm means a rule asked for something the card cannot do; say
            // so instead of doing nothing.
            Action::Verify | Action::ReinstallLoader | Action::None => {
                self.status = Status::warning(self.i18n.text("status.diagnosis_no_action"));
            }
        }
    }

    /// The instance a remedy acts on: the run the verdict describes, else
    /// whatever the list marks as current.
    fn diagnosis_instance_id(&self) -> Option<uuid::Uuid> {
        self.last_run
            .as_ref()
            .map(|run| run.instance_id)
            .or_else(|| self.current_instance().map(|instance| instance.id))
    }

    fn instance_directory(&self) -> Option<PathBuf> {
        self.last_run
            .as_ref()
            .map(|run| run.instance_dir.clone())
            .or_else(|| {
                self.current_instance()
                    .map(|instance| instance.path.clone())
            })
    }

    fn open_content_directory(&mut self, kind: ContentDir) {
        let Some(root) = self.instance_directory() else {
            self.status = Status::warning(self.i18n.text("status.diagnosis_no_instance"));
            return;
        };
        let folder = root.join(kind.folder());
        // A missing folder is not an error worth a dialog: the instance may have
        // no shader packs yet, and the instance root is where the player would
        // create one.
        let target = if folder.is_dir() { folder } else { root };
        match crate::platform::windows::open_path(&target) {
            Ok(()) => {}
            Err(error) => self.status = Status::error(error.to_string()),
        }
    }

    fn open_instance_directory(&mut self) {
        let Some(root) = self.instance_directory() else {
            self.status = Status::warning(self.i18n.text("status.diagnosis_no_instance"));
            return;
        };
        match crate::platform::windows::open_path(&root) {
            Ok(()) => {}
            Err(error) => self.status = Status::error(error.to_string()),
        }
    }

    fn open_crash_report_file(&mut self) {
        let Some(path) = self
            .last_run
            .as_ref()
            .and_then(|run| run.crash_report.clone())
        else {
            self.status = Status::warning(self.i18n.text("status.no_crash_report"));
            return;
        };
        match crate::platform::windows::open_path(&path) {
            Ok(()) => self.status = Status::success(self.i18n.text("status.crash_report_opened")),
            Err(error) => self.status = Status::error(error.to_string()),
        }
    }

    /// Park the player in the download centre with the mod named by the verdict
    /// already typed in. The search still has to be submitted: firing a network
    /// request from a remedy would make one keypress do two things at once.
    fn open_download_center_for_cause(&mut self) {
        let query = self
            .diagnosis
            .as_ref()
            .and_then(|diagnosis| diagnosis.primary.as_ref())
            .and_then(|finding| finding.argument("mod"))
            .map(|(_, value)| value.to_owned());
        self.download_center
            .set_category(DownloadCategory::Resource(ResourceKind::Mod));
        self.download_center.set_source(ResourceSource::Modrinth);
        // Without this the page keeps yesterday's results (and an open project
        // detail) under the new query; the category switch elsewhere clears
        // the same state through the same helper.
        self.reset_resource_view();
        if let Some(query) = query {
            self.download_center.search = query;
        }
        self.set_page(Page::DownloadCenter);
        self.status = Status::info(self.i18n.text("status.download_search_hint"));
    }

    /// The key cap was pressed: find the remedy it belongs to and run it.
    ///
    /// `'\0'` is `Enter`, meaning "the first one": the card's key row says so,
    /// and a player who does not want to learn seven letters gets one.
    pub(in crate::tui::app) fn run_remedy_cap(&mut self, cap: char) {
        let Some(action) = self.remedy_for_cap(cap) else {
            return;
        };
        self.run_action(action);
    }

    fn remedy_for_cap(&self, cap: char) -> Option<Action> {
        let primary = self.diagnosis.as_ref()?.primary.as_ref()?;
        let remedies = crate::core::diagnose::rules::rule_for(primary.rule).remedies;
        if cap == '\0' {
            return remedies.first().copied();
        }
        remedies
            .iter()
            .copied()
            .find(|action| action.key_cap() == Some(cap))
    }

    /// `L`: reuse the primary evidence line as a log filter and jump there.
    pub(in crate::tui::app) fn locate_in_log(&mut self) {
        let Some(finding) = self
            .diagnosis
            .as_ref()
            .and_then(|diagnosis| diagnosis.primary.as_ref())
        else {
            return;
        };
        let Some(line) = finding.evidence.first() else {
            return;
        };
        // Drop the launcher's stream prefix (`[stdout] `): the buffer holds the
        // same line, so the remainder still matches. The game's own timestamp
        // stays in the fragment on purpose; both copies come from this run, and
        // hunting the second `]:` would mean parsing log lines to save eleven
        // characters.
        let body = line
            .text
            .split_once("] ")
            .map_or(line.text.as_str(), |(_, rest)| rest);
        let fragment = body.chars().take(FILTER_FRAGMENT).collect::<String>();
        if fragment.trim().is_empty() {
            return;
        }
        self.log_filter = Some(fragment);
        self.detail_open = false;
        self.diagnosis_open = false;
        self.set_page(Page::Logs);
        self.follow_log_bottom();
    }

    /// `E`: ask where to put the bundle. Every export asks, because an overwrite
    /// in a folder the player forgot about is not recoverable.
    pub(in crate::tui::app) fn begin_diagnosis_export(&mut self) {
        let Some(run) = self.last_run.clone() else {
            self.status = Status::warning(self.i18n.text("status.diagnosis_no_run"));
            return;
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or(0);
        self.pending_log_export = Some(LogExportKind::Bundle);
        self.save_file_browse_request = Some((
            self.paths.game_logs_dir.clone(),
            crate::core::diagnose::bundle::suggested_file_name(&run.instance_name, now),
        ));
        self.status = Status::info(self.i18n.text("status.save_log_target"));
    }

    /// Write the bundle to `path`. Called from the save-file branch.
    pub(in crate::tui::app) fn export_diagnosis_bundle(&mut self, path: &Path) {
        let Some(run) = self.last_run.clone() else {
            self.status = Status::warning(self.i18n.text("status.diagnosis_no_run"));
            return;
        };
        let Some(diagnosis) = self.diagnosis.as_ref() else {
            self.status = Status::warning(self.i18n.text("status.diagnosis_no_run"));
            return;
        };
        let report = crate::core::diagnose::report::render(diagnosis, &|key, arguments| {
            self.i18n.format(key, arguments)
        });
        let input = self.bundle_input(&run);
        let request = crate::core::diagnose::bundle::BundleRequest {
            report,
            diagnosis,
            run: &run,
            input: &input,
        };
        match crate::core::diagnose::bundle::write(path, &request) {
            Ok(()) => {
                self.status = Status::info(
                    self.i18n
                        .format("status.logs_exported", &[("path", &display_path(path))]),
                );
            }
            Err(error) => {
                self.status = Status::error(self.i18n.format(
                    "status.logs_export_failed",
                    &[("error", &error.to_string())],
                ));
            }
        }
    }

    fn bundle_input(&self, run: &RunRecord) -> crate::core::diagnose::bundle::BundleInput {
        let launcher_log = self.paths.launcher_logs_dir.join("launcher.log");
        crate::core::diagnose::bundle::BundleInput {
            mods: mod_files(&run.instance_dir),
            java: self
                .java
                .iter()
                .map(|java| crate::core::diagnose::bundle::JavaFacts {
                    path: java.executable.display().to_string(),
                    version: java.version.clone(),
                    vendor: java.vendor.clone().unwrap_or_default(),
                })
                .collect(),
            launcher_log: launcher_log.is_file().then_some(launcher_log),
        }
    }

    fn diagnosis_facts(&self, run: &RunRecord) -> Facts {
        let mut facts = Facts {
            instance_name: run.instance_name.clone(),
            exit_code: run.exit_code,
            duration_secs: run.duration_secs,
            ..Facts::default()
        };
        if let Some(item) = self
            .instances
            .items
            .iter()
            .find(|item| item.id == run.instance_id)
        {
            facts.minecraft_version = item.version.clone();
            facts.loader = match &item.loader_version {
                Some(version) => format!("{} {version}", item.loader),
                None => item.loader.clone(),
            };
        }
        let selected = self.config.java.selected_path.as_deref();
        let java = selected
            .and_then(|path| self.java.iter().find(|java| java.executable == path))
            .or_else(|| self.java.first());
        if let Some(java) = java {
            facts.java_version = java.version.clone();
            facts.java_major = java.major;
            facts.java_path = java.executable.display().to_string();
        }
        facts.max_memory_mb = self.config.launch.max_memory_mb;
        facts.system_memory_mb = crate::core::launch::memory::detect().total_mb;
        facts.mod_count = mod_count(&run.instance_dir);
        facts
    }
}

/// The jars in `mods`, name and size. The jar itself is never opened: the
/// bundle lists what is installed, and reading a few hundred archives is how an
/// export takes a minute.
fn mod_files(instance_dir: &Path) -> Vec<(String, u64)> {
    let Ok(entries) = std::fs::read_dir(instance_dir.join("mods")) else {
        return Vec::new();
    };
    let mut files = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("jar"))
            {
                return None;
            }
            let name = path.file_name()?.to_str()?.to_owned();
            Some((name, entry.metadata().map(|meta| meta.len()).unwrap_or(0)))
        })
        .collect::<Vec<_>>();
    files.sort();
    files
}

/// How many jars sit in `mods`. Cheap enough for the UI thread; the launcher
/// needs the number only to say "42 mods loaded".
fn mod_count(instance_dir: &Path) -> usize {
    std::fs::read_dir(instance_dir.join("mods"))
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| {
                    entry
                        .path()
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("jar"))
                })
                .count()
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use crate::core::diagnose::finding::{EvidenceLine, Finding};
    use crate::core::diagnose::{Diagnosis, ExitKind, Facts, LaunchStage, RuleId};
    use crate::core::launch::failure::Action;
    use crate::tui::app::App;
    use crate::tui::app::Page;
    use crate::tui::modal::{ConfirmAction, Modal};
    use crate::tui::pages::settings::{SettingId, SettingsCategory};

    fn verdict(exit: ExitKind) -> Box<Diagnosis> {
        Box::new(Diagnosis {
            stage: LaunchStage::InGame,
            exit,
            primary: None,
            others: Vec::new(),
            keywords: Vec::new(),
            facts: Facts::default(),
            sources: Vec::new(),
            unknown: exit.is_crash(),
        })
    }

    fn verdict_with(rule: RuleId) -> Box<Diagnosis> {
        Box::new(Diagnosis {
            stage: LaunchStage::InGame,
            exit: ExitKind::Crashed,
            primary: Some(Finding {
                rule,
                captures: Vec::new(),
                evidence: vec![EvidenceLine {
                    line: 7,
                    text: "[stderr] java.lang.OutOfMemoryError: Java heap space".to_owned(),
                }],
                score: 50,
            }),
            others: Vec::new(),
            keywords: Vec::new(),
            facts: Facts::default(),
            sources: Vec::new(),
            unknown: false,
        })
    }

    /// The same verdict with the primary finding naming a mod: the download
    /// remedy reads that capture to prefill the query.
    fn verdict_naming(rule: RuleId, name: &str) -> Box<Diagnosis> {
        let mut diagnosis = verdict_with(rule);
        if let Some(primary) = diagnosis.primary.as_mut() {
            primary.captures = vec![(crate::core::diagnose::Capture::Mod, name.to_owned())];
        }
        diagnosis
    }

    fn sample_run(name: &str) -> crate::core::diagnose::RunRecord {
        crate::core::diagnose::RunRecord {
            instance_id: uuid::Uuid::new_v4(),
            instance_name: name.to_owned(),
            instance_dir: std::env::temp_dir(),
            log_path: std::env::temp_dir().join("tmcl-nonexistent.log"),
            crash_report: None,
            exit_code: Some(1),
            log_scope: crate::core::diagnose::LogScope::Run,
            duration_secs: 10,
            started_at_epoch: 0,
            launch_command: None,
        }
    }

    #[test]
    fn a_verdict_for_an_older_launch_is_dropped() {
        let mut app = App::new();
        app.page = Page::Logs;
        app.launch_seq = 2;
        app.handle_diagnosis_ready(1, verdict(ExitKind::Crashed));
        assert!(app.diagnosis.is_none());
        assert!(!app.diagnosis_open);
    }

    #[test]
    fn the_current_verdict_opens_the_card_and_warns() {
        let mut app = App::new();
        app.page = Page::Logs;
        app.launch_seq = 3;
        app.handle_diagnosis_ready(3, verdict(ExitKind::Crashed));
        assert!(app.diagnosis.is_some());
        assert!(app.diagnosis_open);
    }

    #[test]
    fn a_clean_exit_never_opens_the_card() {
        let mut app = App::new();
        app.launch_seq = 1;
        app.handle_diagnosis_ready(1, verdict(ExitKind::Clean));
        assert!(app.diagnosis.is_some());
        assert!(!app.diagnosis_open);
    }

    #[test]
    fn a_modal_keeps_the_keyboard_even_after_a_crash() {
        let mut app = App::new();
        app.launch_seq = 1;
        app.modal = Some(Modal::confirm(
            "Delete instance?",
            ConfirmAction::RemoveInstance(uuid::Uuid::nil()),
        ));
        app.handle_diagnosis_ready(1, verdict(ExitKind::Crashed));
        assert!(app.diagnosis.is_some());
        assert!(!app.diagnosis_open);
    }

    /// With `auto_open` off the verdict is still stored and the status bar still
    /// warns; only the card stays closed.
    #[test]
    fn turning_auto_open_off_keeps_the_verdict_but_not_the_card() {
        let mut app = App::new();
        app.config.diagnosis.auto_open = false;
        app.launch_seq = 1;
        app.handle_diagnosis_ready(1, verdict(ExitKind::Crashed));
        assert!(app.diagnosis.is_some());
        assert!(!app.diagnosis_open);
        assert!(app.diagnosis_badge().is_some());
    }

    /// A remedy has to land on the row it names, in the category that holds it.
    #[test]
    fn raising_memory_focuses_the_memory_row() {
        let mut app = App::new();
        app.page = Page::Overview;
        app.run_action(Action::RaiseMemory);
        assert_eq!(app.page, Page::Settings);
        assert_eq!(app.settings.category, SettingsCategory::Game);
        let row = app.current_setting_row().expect("a selected row");
        assert_eq!(row.id, SettingId::MemoryMaximum);
    }

    /// The number on that row is the configured one, not something a rule
    /// baked in: `RaiseMemory` carries no value, and the launcher resolves the
    /// recommendation through `launch::memory` when it needs one.
    #[test]
    fn the_memory_row_shows_the_configured_value() {
        let mut app = App::new();
        app.config.launch.memory_mode = crate::core::config::models::MemoryMode::Manual;
        app.config.launch.max_memory_mb = 6144;
        app.run_action(Action::RaiseMemory);
        let row = app.current_setting_row().expect("a row");
        assert!(row.value.contains("6144"), "{}", row.value);
    }

    #[test]
    fn changing_java_focuses_the_java_row() {
        let mut app = App::new();
        app.run_action(Action::ChangeJava);
        assert_eq!(app.page, Page::Settings);
        assert_eq!(
            app.current_setting_row().expect("a selected row").id,
            SettingId::JavaSelection
        );
    }

    /// `N` and `F` are separate caps on the card, so they must do separate
    /// things: `N` opens the download form, not the row `F` already reaches.
    #[test]
    fn installing_java_opens_the_download_form() {
        let mut app = App::new();
        app.run_action(Action::InstallJava);
        match app.modal.as_ref() {
            Some(Modal::Form { action, .. }) => {
                assert_eq!(*action, crate::tui::modal::FormAction::DownloadJavaRuntime);
            }
            other => panic!("expected the install form, got {other:?}"),
        }
        assert_eq!(
            app.page,
            Page::Overview,
            "the form is a dialog; changing the page underneath would lose the card"
        );
    }

    /// The key caps are data on the action; pressing one runs the remedy that
    /// printed it, and a cap nobody printed does nothing.
    #[test]
    fn only_the_printed_caps_run_a_remedy() {
        let mut app = App::new();
        app.diagnosis = Some(*verdict_with(RuleId::OutOfMemory));

        app.run_remedy_cap('W');
        assert_eq!(app.page, Page::Overview, "W is not on this rule's card");

        app.run_remedy_cap('M');
        assert_eq!(app.page, Page::Settings, "M raises the memory");
        assert_eq!(
            app.current_setting_row().expect("a row").id,
            SettingId::MemoryMaximum
        );
    }

    /// Enter means "the first remedy", which is what the key row promises.
    #[test]
    fn enter_runs_the_first_remedy() {
        let mut app = App::new();
        app.diagnosis = Some(*verdict_with(RuleId::OutOfMemory));
        app.run_remedy_cap('\0');
        assert_eq!(app.page, Page::Settings);
    }

    /// `W` must not paint yesterday's results under today's query: the page
    /// renders `resource_projects` and `resource_detail` directly, so a stale
    /// list (or an open detail) is what the player would see.
    #[test]
    fn the_download_remedy_drops_the_previous_result_set() {
        use crate::core::resource::ResourceSource;
        use crate::core::resource::model::ResourceProject;

        let mut app = App::new();
        app.diagnosis = Some(*verdict_naming(RuleId::FabricMissingDependency, "create"));
        app.download_center.search = "sodium".to_owned();
        app.download_center.total = 99;
        app.download_center.loading = true;
        app.download_center.error = Some("old error".to_owned());
        app.resource_projects = vec![ResourceProject {
            source: ResourceSource::Modrinth,
            project_id: "sodium".into(),
            title: "Sodium".into(),
            author: "jellysquid3".into(),
            description: String::new(),
            icon_url: None,
            categories: Vec::new(),
            game_versions: Vec::new(),
            loaders: Vec::new(),
            downloads: None,
            updated_at: None,
        }];
        app.resource_detail = Some("sodium".to_owned());

        app.run_action(Action::OpenDownloadCenter);

        assert_eq!(app.page, Page::DownloadCenter);
        assert_eq!(app.download_center.source, ResourceSource::Modrinth);
        assert_eq!(app.download_center.search, "create");
        assert!(app.resource_projects.is_empty(), "the old list survived");
        assert!(app.resource_detail.is_none(), "the old detail stayed open");
        assert_eq!(app.download_center.total, 0);
        assert!(!app.download_center.loading);
        assert!(app.download_center.error.is_none());
    }

    #[test]
    fn repair_asks_for_confirmation_on_the_run_instance() {
        let mut app = App::new();
        let run = sample_run("survival");
        let id = run.instance_id;
        app.last_run = Some(run);
        app.run_action(Action::Repair);
        match app.modal.as_ref() {
            Some(Modal::Confirm { action, .. }) => {
                assert_eq!(*action, ConfirmAction::RepairInstance(id));
            }
            other => panic!("expected a confirmation, got {other:?}"),
        }
    }

    #[test]
    fn open_crash_report_without_a_report_warns_instead_of_opening_nothing() {
        let mut app = App::new();
        app.last_run = Some(sample_run("survival"));
        app.status = crate::tui::app::Status::info("before");
        app.run_action(Action::OpenCrashReport);
        assert_eq!(app.status.as_str(), app.i18n.text("status.no_crash_report"));
    }

    /// `E` asks every time, and the suggested name carries the instance and a
    /// stamp so two exports of one run do not collide by default.
    #[test]
    fn every_export_asks_for_a_path() {
        let mut app = App::new();
        assert!(app.last_run.is_none());
        app.begin_diagnosis_export();
        assert!(app.save_file_browse_request.is_none(), "nothing to export");

        app.last_run = Some(sample_run("生存 存档"));
        app.begin_diagnosis_export();
        let (_, name) = app.save_file_browse_request.clone().expect("a request");
        assert!(name.starts_with("tmcl-diagnosis-生存_存档-"), "{name}");
        assert!(name.ends_with(".zip"), "{name}");

        // Consuming the request and asking again must produce another one.
        assert!(app.take_save_file_browse_request().is_some());
        app.begin_diagnosis_export();
        assert!(app.save_file_browse_request.is_some());
    }

    /// The card's export goes through the same `bundle::write` as the CLI; this
    /// checks the file it produces, not just that a dialog opened.
    #[test]
    fn the_card_writes_a_bundle_with_the_rendered_report() {
        let root = std::env::temp_dir().join(format!("tmcl-card-bundle-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("dir");
        let log = root.join("game.log");
        std::fs::write(
            &log,
            "[stderr] java.lang.OutOfMemoryError: Java heap space\n",
        )
        .expect("log");

        let mut app = App::new();
        app.diagnosis = Some(*verdict_with(RuleId::OutOfMemory));
        app.last_run = Some(crate::core::diagnose::RunRecord {
            instance_dir: root.clone(),
            log_path: log,
            ..sample_run("生存 存档")
        });

        let target = root.join("bundle.zip");
        app.export_diagnosis_bundle(&target);
        assert!(target.is_file(), "status: {}", app.status.as_str());

        let mut archive =
            zip::ZipArchive::new(std::fs::File::open(&target).expect("open")).expect("zip");
        let mut report = String::new();
        {
            use std::io::Read;
            archive
                .by_name("report.txt")
                .expect("report")
                .read_to_string(&mut report)
                .expect("read");
        }
        assert!(
            report.contains(&app.i18n.text("diagnose.title")),
            "{report}"
        );
        assert!(
            report.contains(&app.i18n.text("diagnose.reason.out_of_memory")),
            "{report}"
        );
        // The instance name reaches instance.json, not the report.
        let mut instance = String::new();
        {
            use std::io::Read;
            archive
                .by_name("instance.json")
                .expect("instance")
                .read_to_string(&mut instance)
                .expect("read");
        }
        assert!(instance.contains("生存 存档"), "{instance}");

        std::fs::remove_dir_all(root).expect("clean");
    }
}
