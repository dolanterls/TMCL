//! `tmcl diagnose`: one command, one verdict.
//!
//! Everything here is wiring: resolve the instance, find the log, hand the two
//! to the same `core::diagnose` entry point the card uses, print. If this file
//! grows rules or text of its own, the card and the CLI have started to
//! disagree and the logic belongs back in `core`.

use std::path::{Path, PathBuf};

use crate::cli::args::DiagnoseArgs;
use crate::cli::output::OutputOptions;
use crate::core::config::models::Config;
use crate::core::config::paths::DataPaths;
use crate::core::diagnose::bundle::{self, BundleInput, BundleRequest, JavaFacts};
use crate::core::diagnose::report;
use crate::core::diagnose::{Diagnosis, Facts, RunRecord};
use crate::core::error::{Error, Result};
use crate::core::instance::resolve::{ResolvedInstance, resolve};
use crate::i18n::I18n;

use super::common::load_config;

pub(super) async fn run(args: DiagnoseArgs, output: OutputOptions) -> Result<()> {
    let paths = DataPaths::discover()?;
    let config = load_config(&paths)?;
    let instance = resolve_target(args.reference.as_deref().unwrap_or("@last"), &config)?;
    // A pointed-at file is not evidence of a run: the instance's own
    // `latest.log` and newest crash report may belong to a later session.
    let explicit_log = args.log.is_some();
    let log_path = match args.log.clone() {
        Some(path) => {
            if !path.is_file() {
                return Err(Error::Config(format!(
                    "log file not found: {}",
                    path.display()
                )));
            }
            path
        }
        None => newest_log(&paths.game_logs_dir, &instance.config.name).ok_or_else(|| {
            Error::Config(format!(
                "no game log for {} under {}; pass --log PATH",
                instance.config.name,
                paths.game_logs_dir.display()
            ))
        })?,
    };

    let mut run = RunRecord {
        instance_id: instance.config.id,
        instance_name: instance.config.name.clone(),
        instance_dir: instance.config.path.clone(),
        log_path,
        // The evidence layer scans `crash-reports/` when this is `None`, which
        // is exactly right for a log that was produced by an earlier session.
        crash_report: None,
        exit_code: None,
        duration_secs: 0,
        started_at_epoch: modified_epoch(&instance),
        launch_command: None,
        log_scope: if explicit_log {
            crate::core::diagnose::LogScope::File
        } else {
            crate::core::diagnose::LogScope::Run
        },
    };
    // The evidence layer would find this anyway; recording it here means the
    // bundle ships the same file the verdict quotes.
    run.crash_report = crate::core::diagnose::evidence::crash_report_for(&run);
    let facts = facts_with_java(&config, &instance);
    let diagnosis = crate::core::diagnose::for_run(facts, &run);
    let i18n = I18n::new(config.state.ui.language);

    if let Some(path) = args.export.clone() {
        export(&i18n, &paths, &config, &path, &diagnosis, &run).map_err(|error| {
            // The io error alone ("access is denied") does not say which path.
            Error::Config(format!("cannot write {}: {error}", path.display()))
        })?;
        // stderr, so `--json | jq` keeps a single JSON document on stdout.
        eprintln!(
            "{}",
            i18n.format(
                "status.logs_exported",
                &[("path", &path.display().to_string())]
            )
        );
    }

    output.print(&diagnosis, || {
        report::render(&diagnosis, &|key, arguments| i18n.format(key, arguments))
    })
}

fn export(
    i18n: &I18n,
    paths: &DataPaths,
    config: &Config,
    path: &Path,
    diagnosis: &Diagnosis,
    run: &RunRecord,
) -> Result<()> {
    let launcher_log = paths.launcher_logs_dir.join("launcher.log");
    let input = BundleInput {
        mods: mod_files(&run.instance_dir),
        java: java_facts(paths, config),
        launcher_log: launcher_log.is_file().then_some(launcher_log),
    };
    let report = report::render(diagnosis, &|key, arguments| i18n.format(key, arguments));
    bundle::write(
        path,
        &BundleRequest {
            report,
            diagnosis,
            run,
            input: &input,
        },
    )
}

/// Probes the Java runtimes so `system.json` can list them.
///
/// Only called for an export: probing spawns one process per candidate, which
/// is fine for an explicit export and would be wrong on the plain path.
fn java_facts(paths: &DataPaths, config: &Config) -> Vec<JavaFacts> {
    crate::core::java::scan::discover_default(
        &paths.runtime_dir,
        config.game_directories.clone(),
        config
            .java
            .selected_path
            .clone()
            .into_iter()
            .collect::<Vec<PathBuf>>(),
    )
    .into_iter()
    .map(|java| JavaFacts {
        path: java.executable.display().to_string(),
        version: java.version,
        vendor: java.vendor.unwrap_or_default(),
    })
    .collect()
}

/// `facts_for`, plus the Java version when the configured runtime can say it.
///
/// Probing spawns one process. That is too much for a frame, and exactly right
/// for a command the player runs once after a crash: "Java" with no version
/// next to it is the first thing a helper asks about.
/// Resolve the reference, saying which one failed.
///
/// The first `diagnose` after installing often runs with no instance at all;
/// the resolver's own message is accurate but terse, and "cannot diagnose
/// @last" is the sentence that tells the player what to fix.
fn resolve_target(reference: &str, config: &Config) -> Result<ResolvedInstance> {
    resolve(reference, config)
        .map_err(|error| Error::Config(format!("cannot diagnose {reference}: {error}")))
}

fn facts_with_java(config: &Config, instance: &ResolvedInstance) -> Facts {
    let mut facts = facts_for(config, instance);
    let Some(path) = config
        .java
        .selected_path
        .as_deref()
        .filter(|path| path.is_file())
    else {
        return facts;
    };
    if let Ok(java) = super::common::probe_java(path) {
        facts.java_version = java.version;
        facts.java_major = java.major;
    }
    facts
}

fn facts_for(config: &Config, instance: &ResolvedInstance) -> Facts {
    let mods = mod_files(&instance.config.path).len();
    let loader = match instance.config.loader.version() {
        Some(version) => format!("{} {version}", instance.config.loader.key()),
        None => instance.config.loader.key().to_owned(),
    };
    let java_path = config
        .java
        .selected_path
        .as_deref()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    Facts {
        instance_name: instance.config.name.clone(),
        minecraft_version: instance.config.minecraft_version.clone(),
        loader,
        java_path,
        max_memory_mb: config.launch.max_memory_mb,
        system_memory_mb: crate::core::launch::memory::detect().total_mb,
        mod_count: mods,
        ..Facts::default()
    }
}

/// The newest `*.log` under `logs/game`.
fn newest_log(directory: &Path, instance_name: &str) -> Option<PathBuf> {
    // `logs/game` is shared by every instance. The file name starts with the
    // instance (`<stem>-<stamp>.log`, see `launch::session`), so the prefix is
    // what keeps `diagnose survival` from reading a log another instance wrote
    // later. The stem comes from the same function the writer used: a rename
    // must not silently break the lookup.
    let prefix = format!(
        "{}-",
        crate::core::launch::session::safe_file_stem(instance_name)
    );
    let entries = std::fs::read_dir(directory).ok()?;
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("log"))
        })
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&prefix))
        })
        .filter_map(|path| {
            let modified = std::fs::metadata(&path).ok()?.modified().ok()?;
            Some((modified, path))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

/// A log from a previous session has no start time of its own; the file's is
/// the closest honest answer.
fn modified_epoch(instance: &ResolvedInstance) -> u64 {
    let path = instance.config.path.join("logs").join("latest.log");
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

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

#[cfg(test)]
mod tests {
    use super::{facts_for, newest_log};
    use crate::core::config::models::Config;
    use crate::core::instance::models::InstanceLoader;
    use crate::core::instance::resolve::{RefSource, ResolvedInstance};

    fn instance(loader: InstanceLoader) -> ResolvedInstance {
        let mut config = crate::core::instance::models::InstanceConfig::new(
            "C:/games/versions/demo",
            uuid::Uuid::nil(),
            "demo",
            "1.21.1",
        );
        config.loader = loader;
        ResolvedInstance {
            game_directory: "C:/games".into(),
            source: RefSource::Name,
            config,
        }
    }

    #[test]
    fn the_facts_name_the_loader_with_its_version() {
        let facts = facts_for(
            &Config::default(),
            &instance(InstanceLoader::Fabric {
                version: "0.16.9".into(),
            }),
        );
        assert_eq!(facts.loader, "Fabric 0.16.9");
        assert_eq!(facts.minecraft_version, "1.21.1");
        assert_eq!(facts.instance_name, "demo");
        // No memory advice is invented here: the config's value is reported.
        assert_eq!(facts.max_memory_mb, Config::default().launch.max_memory_mb);
    }

    #[test]
    fn an_unknown_reference_names_itself_in_the_error() {
        let error = super::resolve_target("@last", &Config::default()).expect_err("no instances");
        let message = error.to_string();
        assert!(message.contains("@last"), "{message}");

        let error = super::resolve_target("demo", &Config::default()).expect_err("no instances");
        assert!(error.to_string().contains("demo"), "{error}");
    }

    #[test]
    fn a_vanilla_instance_has_no_version_suffix() {
        let facts = facts_for(&Config::default(), &instance(InstanceLoader::Vanilla));
        assert_eq!(facts.loader, "Vanilla");
    }

    #[test]
    fn the_newest_log_of_that_instance_wins() {
        let root = std::env::temp_dir().join(format!("tmcl-diagnose-cmd-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("dir");
        std::fs::write(root.join("survival-20261108-110211.log"), b"old").expect("old");
        std::fs::write(root.join("notes.txt"), b"txt").expect("txt");
        std::thread::sleep(std::time::Duration::from_millis(20));
        // Newer, but a different instance: this is the case the prefix filter
        // exists for.
        std::fs::write(root.join("creative-20261108-120000.log"), b"new").expect("new");
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(root.join("survival-20261108-130000.log"), b"mid").expect("mid");

        let found = newest_log(&root, "survival").expect("a log");
        assert_eq!(
            found.file_name().and_then(|name| name.to_str()),
            Some("survival-20261108-130000.log")
        );
        assert!(
            newest_log(&root, "creative").is_some_and(|path| path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("creative-"))),
            "each instance keeps its own newest log"
        );
        assert!(
            newest_log(&root, "deleted-instance").is_none(),
            "no fallback to whichever log happens to be newest"
        );
        std::fs::remove_dir_all(root).expect("clean");
    }

    /// The name on disk is sanitized, so the search has to sanitize the same
    /// way: a space in the instance name must not make its own log invisible.
    #[test]
    fn a_name_with_a_space_still_finds_its_log() {
        let root =
            std::env::temp_dir().join(format!("tmcl-diagnose-stem-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("dir");
        std::fs::write(root.join("my_world-20261108-110211.log"), b"x").expect("write");
        let found = newest_log(&root, "my world").expect("a log");
        assert!(
            found
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("my_world-"))
        );
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn an_empty_directory_has_no_log() {
        let root =
            std::env::temp_dir().join(format!("tmcl-diagnose-empty-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("dir");
        assert!(newest_log(&root, "demo").is_none());
        std::fs::remove_dir_all(root).expect("clean");
    }
}
