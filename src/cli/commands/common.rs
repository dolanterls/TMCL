use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::config::models::Config;
use crate::core::config::paths::DataPaths;
use crate::core::error::{Error, Result};
use crate::core::instance::resolve::{ResolvedInstance, resolve};
use crate::core::java::models::JavaInstallation;
use crate::core::java::parse::parse_command_output;

pub(super) fn load_config(paths: &DataPaths) -> Result<Config> {
    let config = crate::core::config::load_or_default(paths)?;
    crate::core::download::DownloadExecutor::configure_global(
        crate::core::download::DownloadExecutorConfig {
            auto: config.downloads.auto_download_threads,
            threads: config.downloads.download_threads,
        },
    );
    Ok(config)
}

/// The instance a command works on; `None` means "the current one".
///
/// Every family shares this so `@last` is spelled once, not once per command.
pub(super) fn instance(reference: Option<&str>, config: &Config) -> Result<ResolvedInstance> {
    resolve(reference.unwrap_or("@last"), config)
}

/// Splits `[REF] <NAME>` when both positionals are optional.
///
/// One value names the target, two are reference then target. The rule is the
/// same for every command that takes both, so a user who learned
/// `instance rename x` does not have to learn a second one here.
pub(super) fn ref_and_name(
    reference: Option<String>,
    name: Option<String>,
    missing: &str,
) -> Result<(Option<String>, String)> {
    match (reference, name) {
        (Some(reference), Some(name)) => Ok((Some(reference), name)),
        (Some(name), None) => Ok((None, name)),
        (None, _) => Err(Error::Config(missing.to_owned())),
    }
}

/// Splits `[REF] <WORLD> [<NAME>]`, the shape `world datapack` needs.
pub(super) fn ref_world_name(
    reference: Option<String>,
    world: Option<String>,
    name: Option<String>,
) -> Result<(Option<String>, String, Option<String>)> {
    match (reference, world, name) {
        (Some(reference), Some(world), Some(name)) => Ok((Some(reference), world, Some(name))),
        (Some(world), Some(name), None) => Ok((None, world, Some(name))),
        (Some(world), None, None) => Ok((None, world, None)),
        _ => Err(Error::Config(
            "name the world, and the entry when the command needs one: \
             `tmcl world datapack <list|enable|disable|remove> [REF] <WORLD> [<NAME>]`"
                .into(),
        )),
    }
}

/// Finds one content entry by file name, falling back to its display name.
///
/// A query that matches several entries is an error, not a first-hit: acting on
/// the wrong mod is worse than asking for a more precise name.
pub(super) fn find_content<'a>(
    entries: &'a [crate::core::content::ContentEntry],
    query: &str,
    kind: crate::core::content::ContentKind,
) -> Result<&'a crate::core::content::ContentEntry> {
    let by_file = entries
        .iter()
        .filter(|entry| entry.display_file_name().eq_ignore_ascii_case(query))
        .collect::<Vec<_>>();
    let candidates = if by_file.is_empty() {
        entries
            .iter()
            .filter(|entry| entry.name.eq_ignore_ascii_case(query))
            .collect::<Vec<_>>()
    } else {
        by_file
    };
    match candidates.len() {
        1 => Ok(candidates[0]),
        0 => Err(Error::Config(format!(
            "no {} named {query}{}",
            kind.label(),
            near_matches(entries, query)
        ))),
        _ => Err(Error::Config(format!(
            "{query} matches {} {} entries: {}",
            candidates.len(),
            kind.label(),
            candidates
                .iter()
                .take(5)
                .map(|entry| entry.display_file_name())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// " Did you mean …" suffix, empty when nothing is close enough to be useful.
fn near_matches(entries: &[crate::core::content::ContentEntry], query: &str) -> String {
    let needle = query.to_lowercase();
    let close = entries
        .iter()
        .filter(|entry| entry.display_file_name().to_lowercase().contains(&needle))
        .take(5)
        .map(|entry| entry.display_file_name())
        .collect::<Vec<_>>();
    if close.is_empty() {
        String::new()
    } else {
        format!("; did you mean: {}", close.join(", "))
    }
}

/// Bytes as a short human-readable string.
pub(super) fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Runs one task to its end, rendering progress on the way.
pub(super) async fn run_task(
    paths: &DataPaths,
    operation: crate::core::task::TaskOperation,
    output: &crate::cli::output::OutputOptions,
) -> Result<crate::core::task::TaskOutcome> {
    let mut outcomes = run_tasks(paths, vec![operation], output).await?;
    outcomes
        .pop()
        .ok_or_else(|| Error::Download("the task engine returned nothing".into()))
}

/// Runs several tasks as one job and follows each to its end.
///
/// The tasks are submitted together so the engine can run them concurrently
/// (an install of a mod plus three dependencies spends its time waiting on the
/// network), and grouped under one job id so `task list` shows them as one
/// operation rather than four unrelated rows.
pub(super) async fn run_tasks(
    paths: &DataPaths,
    operations: Vec<crate::core::task::TaskOperation>,
    output: &crate::cli::output::OutputOptions,
) -> Result<Vec<crate::core::task::TaskOutcome>> {
    use crate::cli::progress::{ProgressRenderer, Watch};
    use crate::core::task::{MinecraftTaskRunner, TaskEngine};

    let engine = TaskEngine::open(
        crate::core::task::store::JsonTaskStore::new(paths.tasks_dir.clone()),
        Arc::new(MinecraftTaskRunner),
        3,
    )?;
    let job = uuid::Uuid::new_v4();
    let ids = operations
        .into_iter()
        .map(|operation| engine.submit_with_job(operation, Some(job)))
        .collect::<Result<Vec<_>>>()?;
    let watcher = watch_for_interrupt(engine.clone(), ids.clone());
    let mut outcomes = Vec::with_capacity(ids.len());
    for id in ids {
        let mut renderer = ProgressRenderer::new(output.progress_mode(), id, "task");
        outcomes.push(
            Watch::new(id)
                .run(&engine, &mut renderer, |line| {
                    eprint!("{line}");
                    let _ = std::io::Write::flush(&mut std::io::stderr());
                })
                .await?,
        );
    }
    watcher.abort();
    Ok(outcomes)
}

/// Cancels the running tasks when the user presses Ctrl-C, then exits.
///
/// The task records are deleted, so there is no state to unwind and no reason to
/// build an "interrupted" error that every layer would have to carry: the
/// process is going away either way, and 130 is the exit code a shell expects
/// from an interrupted command.
fn watch_for_interrupt(
    engine: crate::core::task::TaskEngine,
    ids: Vec<uuid::Uuid>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_err() {
            return;
        }
        // Step off the progress line before writing anything.
        eprintln!();
        for id in ids {
            let _ = engine.delete(id).await;
        }
        // Give the workers a moment to notice the cancellation and clean up
        // their temporary files before the process disappears.
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        eprintln!("Interrupted: the task was cancelled.");
        std::process::exit(130);
    })
}

/// A throttled one-line download indicator for single-file fetches.
///
/// Steps that go through the task engine get `ProgressRenderer` instead; this is
/// for the few places that fetch one file and would not justify a task.
pub(super) struct DownloadProgress {
    quiet: bool,
    terminal: bool,
    last: std::time::Instant,
}

impl DownloadProgress {
    pub(super) fn new(output: &crate::cli::output::OutputOptions) -> Self {
        Self {
            quiet: output.quiet,
            terminal: crate::cli::output::OutputOptions::progress_is_terminal(),
            last: std::time::Instant::now()
                .checked_sub(std::time::Duration::from_secs(1))
                .unwrap_or_else(std::time::Instant::now),
        }
    }

    pub(super) fn line(&mut self, done: u64, total: Option<u64>) {
        if self.quiet || !self.terminal {
            return;
        }
        if self.last.elapsed() < std::time::Duration::from_millis(200) {
            return;
        }
        self.last = std::time::Instant::now();
        match total {
            Some(total) if total > 0 => eprint!(
                "\r  {}%  {} of {}",
                done.min(total) * 100 / total,
                human_size(done),
                human_size(total)
            ),
            _ => eprint!("\r  {}", human_size(done)),
        }
        let _ = std::io::Write::flush(&mut std::io::stderr());
    }

    /// Closes the line the carriage returns were written on.
    pub(super) fn finish(&self) {
        if !self.quiet && self.terminal {
            eprintln!();
        }
    }
}

pub(super) fn current_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

pub(super) fn probe_java(path: &Path) -> Result<JavaInstallation> {
    let output = std::process::Command::new(path)
        .args(["-XshowSettings:properties", "-version"])
        .output()
        .map_err(|error| Error::Java(format!("probe Java {}: {error}", path.display())))?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    parse_command_output(path.to_path_buf(), &text)
}
