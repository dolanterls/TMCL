//! What a finished run left behind.
//!
//! Four sources: the log TMCL captured, the crash report the game wrote, the
//! JVM's own `hs_err` file, and `logs/latest.log`. The first two are usually
//! enough; the other two exist because "the log is empty" is a real failure mode,
//! and a JVM that dies before its first line still leaves an `hs_err_pid` file.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use super::engine::Text;
use super::{ExitKind, LaunchStage, LogScope, RunRecord, Source};

/// The tail of the captured log. A crash is at the end; a mod list from an hour
/// ago is not worth a megabyte of scanning.
pub const LOG_TAIL: u64 = 512 * 1024;
pub const REPORT_LIMIT: u64 = 2 * 1024 * 1024;
pub const HS_ERR_LIMIT: u64 = 1024 * 1024;
pub const LATEST_LOG_LIMIT: u64 = 512 * 1024;

/// How much older than the run a neighbouring file may be and still count as
/// part of it.
///
/// Wide enough for coarse file timestamps (FAT rounds to two seconds) and for
/// the crash handler writing its report a moment before the log's last line;
/// far narrower than the days that separate two playing sessions.
const RUN_FILE_SLACK: u64 = 120;

#[derive(Debug, Clone)]
pub struct Evidence {
    pub text: Text,
    pub sources: Vec<Source>,
    pub stage: LaunchStage,
    pub exit: ExitKind,
}

pub fn collect(run: &RunRecord) -> Evidence {
    if run.log_scope == LogScope::File {
        return collect_file(run);
    }
    let mut sources = Vec::new();
    let mut body = String::new();

    let log = read_tail(&run.log_path, LOG_TAIL);
    sources.push(source(&run.log_path, log.as_ref().map(String::len)));
    body.push_str(log.as_deref().unwrap_or(""));

    let report_path = crash_report_for(run);
    let report = report_path
        .as_ref()
        .and_then(|path| read_tail(path, REPORT_LIMIT));
    match report_path.as_deref() {
        Some(path) => sources.push(source(path, report.as_ref().map(String::len))),
        // A folder full of older sessions' reports is not "not found": it is
        // "nothing this run wrote". Telling a player `未找到` about a directory
        // they can open in Explorer is how a diagnosis loses their trust.
        None if newest_crash_report(&run.instance_dir).is_none() => {
            sources.push(source(&run.instance_dir.join("crash-reports"), None));
        }
        None => {}
    }
    if let Some(report) = &report {
        body.push('\n');
        body.push_str(report);
    }

    let hs_err_path = hs_err_for(run);
    let hs_err = hs_err_path
        .as_ref()
        .and_then(|path| read_tail(path, HS_ERR_LIMIT));
    if hs_err_path.is_some() || !run.instance_dir.as_os_str().is_empty() {
        sources.push(source(
            hs_err_path
                .as_deref()
                .unwrap_or(&run.instance_dir.join("hs_err_pid*.log")),
            hs_err.as_ref().map(String::len),
        ));
    }
    if let Some(hs_err) = &hs_err {
        body.push('\n');
        body.push_str(hs_err);
    }

    let latest_path = run.instance_dir.join("logs").join("latest.log");
    // A `latest.log` left over from an earlier session carries the same trap as
    // a stale crash report: `Backend library: LWJGL version` in it moves the
    // stage to `InGame`, and every startup-only rule stops being scanned.
    if written_by_run(&latest_path, run) {
        let latest = read_tail(&latest_path, LATEST_LOG_LIMIT);
        sources.push(source(&latest_path, latest.as_ref().map(String::len)));
        if let Some(latest) = &latest {
            body.push('\n');
            body.push_str(latest);
        }
    }

    let text = Text::parse(&body);
    let stage = stage_of(&text, run.duration_secs);
    let exit = exit_of(run.exit_code, &text);
    Evidence {
        text,
        sources,
        stage,
        exit,
    }
}

/// The pointed-at file, and nothing else.
///
/// `--log` asks "explain this file". The instance's `latest.log`, its newest
/// crash report and its `hs_err` may all belong to a later session; appending
/// them would hand that session the tail bonus and let it outrank the run the
/// caller asked about.
fn collect_file(run: &RunRecord) -> Evidence {
    let log = read_tail(&run.log_path, LOG_TAIL);
    let sources = vec![source(&run.log_path, log.as_ref().map(String::len))];
    let text = Text::parse(log.as_deref().unwrap_or(""));
    let stage = stage_of(&text, run.duration_secs);
    let exit = exit_of(run.exit_code, &text);
    Evidence {
        text,
        sources,
        stage,
        exit,
    }
}

fn source(path: &Path, bytes: Option<usize>) -> Source {
    Source {
        path: path.display().to_string(),
        present: bytes.is_some(),
        bytes: bytes.unwrap_or(0) as u64,
    }
}

fn resolve(path: &Path, instance_dir: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        instance_dir.join(path)
    }
}

/// The newest report in `crash-reports/`, whatever wrote it.
fn newest_crash_report(instance_dir: &Path) -> Option<PathBuf> {
    newest(&instance_dir.join("crash-reports"), |path| {
        path.extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("txt"))
    })
}

/// Whether `path` was written during this run.
///
/// The fallbacks below exist for a run that did not name its own files, and
/// without this bound they hand the diagnosis the newest file in the directory
/// even when it is from a week-old session. Such a report contains `Backend
/// library: LWJGL version`, which moves the stage to `InGame` and drops every
/// startup-only rule, so a mod-resolution failure is reported as "a class is
/// missing, repair the instance".
///
/// A record with no clock (`started_at_epoch == 0`, used by tests and by
/// hand-built records) always returns true: there is nothing to compare against.
fn written_by_run(path: &Path, run: &RunRecord) -> bool {
    if run.started_at_epoch == 0 {
        return true;
    }
    let Some(modified) = path
        .metadata()
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
    else {
        // Missing file or unreadable timestamp: let the reader decide, as it
        // did before. `read_tail` reports the absence.
        return true;
    };
    modified.as_secs().saturating_add(RUN_FILE_SLACK) >= run.started_at_epoch
}

/// The crash report this run left behind: the path the game printed, else the
/// newest file in `crash-reports/`, provided that file belongs to this run.
///
/// Public because the export bundle has to ship the same file the diagnosis read.
/// A caller that only sets `crash_report` from the launch output and leaves the
/// fallback to `collect` hands the bundle `None` and exports a report-less archive
/// for a run that did write one.
pub fn crash_report_for(run: &RunRecord) -> Option<PathBuf> {
    // A pointed-at file has no run of its own; the newest report in the
    // instance may describe a later session, and shipping it would make the
    // bundle say two different things.
    if run.log_scope == LogScope::File {
        return None;
    }
    run.crash_report
        .as_ref()
        .map(|path| resolve(path, &run.instance_dir))
        .filter(|path| path.is_file())
        .or_else(|| newest_crash_report(&run.instance_dir).filter(|path| written_by_run(path, run)))
}

/// The newest JVM fatal-error file this run left behind, if any.
///
/// Shared with `bundle`: the diagnosis reads it for evidence, the export ships
/// it, and the two must agree on which file they mean. The JVM writes `hs_err`
/// at the moment of the abort, so a file older than the run belongs to some
/// earlier crash and must not be quoted as this one's.
pub fn hs_err_for(run: &RunRecord) -> Option<PathBuf> {
    newest(&run.instance_dir, |path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("hs_err_pid") && name.ends_with(".log"))
    })
    .filter(|path| written_by_run(path, run))
}

fn newest(directory: &Path, accept: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let entries = std::fs::read_dir(directory).ok()?;
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && accept(path))
        .max_by_key(|path| {
            path.metadata()
                .and_then(|meta| meta.modified())
                .unwrap_or(std::time::UNIX_EPOCH)
        })
}

/// Which stage a run reached, from the log alone.
///
/// `pub(crate)` so the fixture guard can call it. A guard with its own copy of
/// this rule drifts: the last one missed `LWJGL OpenAL` and the duration floor.
pub(crate) fn stage_of(text: &Text, duration_secs: u64) -> LaunchStage {
    // `LWJGL Version` is the 1.12 spelling; 1.13+ prints `Backend library:
    // LWJGL version 3.3.3`. Match case-insensitively so neither is missed.
    let window = (0..text.len()).any(|index| {
        let line = text.line(index).to_ascii_lowercase();
        line.contains("lwjgl version")
            || line.contains("backend library")
            || line.contains("lwjgl openal")
    });
    if window {
        LaunchStage::InGame
    } else if text.len() >= 3 && duration_secs >= 2 {
        LaunchStage::NoWindow
    } else {
        LaunchStage::NeverStarted
    }
}

/// How many trailing lines count as "where the log ended". A stack trace is
/// tens of lines; a hundred is generous without scanning a whole session.
const TAIL_FINDING_LINES: usize = 100;

fn exit_of(code: Option<i32>, text: &Text) -> ExitKind {
    match code {
        Some(0) => ExitKind::Clean,
        Some(code) if code < 0 => ExitKind::NativeAbort,
        Some(_) => ExitKind::Crashed,
        None => {
            let reported = (0..text.len()).any(|index| {
                let line = text.line(index);
                line.contains("---- Minecraft Crash Report ----")
                    || line.contains("A fatal error has been detected")
            });
            if reported {
                return ExitKind::Crashed;
            }
            // No exit code: the caller read a log file, not a process. A loader
            // that dies during bootstrap prints its own exception and exits before
            // Minecraft's crash handler can write a banner, so requiring the
            // banner would report every mod-resolution failure as "the process was
            // killed" and then diagnose nothing.
            //
            // A log that stops with no exception near its end is still treated as
            // killed, which matches a window closed from the task manager.
            let tail_starts = text.len().saturating_sub(TAIL_FINDING_LINES);
            let failed = (tail_starts..text.len()).any(|index| {
                let line = text.lower_line(index);
                ["exception", "error:", "error]:", "caused by:", "fatal"]
                    .iter()
                    .any(|needle| line.contains(needle))
            });
            if failed {
                return ExitKind::Crashed;
            }
            // The game announces its own shutdown. Checked after the exception
            // scan so a crash *during* shutdown still counts as a crash.
            let stopped = (tail_starts..text.len()).any(|index| {
                let line = text.lower_line(index);
                ["stopping!", "stopping server", "all dimensions are saved"]
                    .iter()
                    .any(|needle| line.contains(needle))
            });
            if stopped {
                ExitKind::Clean
            } else {
                ExitKind::UserKilled
            }
        }
    }
}

/// At most `limit` bytes from the end of a file.
fn read_tail(path: &Path, limit: u64) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    let start = if limit == u64::MAX || length <= limit {
        0
    } else {
        length - limit
    };
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.take(limit).read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::{Text, exit_of, stage_of};
    use crate::core::diagnose::{ExitKind, LaunchStage};

    /// A loader that dies during bootstrap never writes the crash banner; if
    /// the exit code is unknown, the exception at the tail has to count.
    #[test]
    fn a_loader_exception_without_a_code_counts_as_a_crash() {
        let text = Text::parse(
            "[stdout] [19:02:11] [main/INFO]: Loading Minecraft 1.21.1 with Fabric Loader 0.16.9\n\
             [stdout] [19:02:12] [main/INFO]: Loading 42 mods\n\
             [stderr] net.fabricmc.loader.impl.FormattedException: ModResolutionException: Could not find required mod: create requires flywheel",
        );
        assert_eq!(exit_of(None, &text), ExitKind::Crashed);
    }

    /// The game announces its own shutdown; a file that ends this way did not
    /// crash, even though there is no exit code to prove it.
    #[test]
    fn a_log_that_announces_its_shutdown_is_clean() {
        let text = Text::parse(
            "[stdout] [19:02:11] [main/INFO]: Backend library: LWJGL version 3.3.3\n\
             [stdout] [19:42:20] [main/INFO]: Stopping!",
        );
        assert_eq!(exit_of(None, &text), ExitKind::Clean);
    }

    /// A crash during shutdown still counts: the exception scan runs first.
    #[test]
    fn an_exception_after_the_shutdown_line_is_still_a_crash() {
        let text = Text::parse(
            "[stdout] [19:42:20] [main/INFO]: Stopping!\n\
             [stderr] java.lang.RuntimeException: Failed to shut down",
        );
        assert_eq!(exit_of(None, &text), ExitKind::Crashed);
    }

    /// A log that stops without an exception is still "killed", which is what a
    /// window closed from the task manager leaves behind.
    #[test]
    fn a_log_that_simply_stops_is_treated_as_killed() {
        let text = Text::parse(
            "[stdout] [19:02:11] [main/INFO]: Setting user: Player\n\
             [stdout] [19:02:12] [main/INFO]: Backend library: LWJGL version 3.3.3\n\
             [stdout] [19:03:40] [Render thread/INFO]: Loaded 120 advancements",
        );
        assert_eq!(exit_of(None, &text), ExitKind::UserKilled);
    }

    #[test]
    fn an_exception_far_from_the_end_does_not_count() {
        // The failure has to be *where the log ended*; an old warning at the top
        // of a long session is not why the process died.
        let mut body = String::from("[stderr] java.lang.RuntimeException: caught and handled\n");
        for index in 0..200 {
            body.push_str("[stdout] [19:02:14] [main/INFO]: tick ");
            body.push_str(&index.to_string());
            body.push('\n');
        }
        assert_eq!(exit_of(None, &Text::parse(&body)), ExitKind::UserKilled);
    }

    /// `--log` means "explain this file": the instance's own `latest.log` may
    /// belong to a later session, and appending it would let that session
    /// outrank the run the caller asked about.
    #[test]
    fn a_pointed_at_file_is_read_alone() {
        let root =
            std::env::temp_dir().join(format!("tmcl-evidence-scope-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("logs")).expect("dirs");
        std::fs::write(
            root.join("logs").join("latest.log"),
            "[stderr] java.lang.OutOfMemoryError: Java heap space\n",
        )
        .expect("latest");
        let named = root.join("named.log");
        std::fs::write(&named, "[stderr] Pixel format not accelerated\n").expect("named");

        let mut run = super::RunRecord {
            instance_id: uuid::Uuid::nil(),
            instance_name: "demo".to_owned(),
            instance_dir: root.clone(),
            log_path: named.clone(),
            crash_report: None,
            exit_code: None,
            duration_secs: 0,
            started_at_epoch: 0,
            launch_command: None,
            log_scope: super::LogScope::File,
        };
        let evidence = super::collect(&run);
        assert_eq!(evidence.sources.len(), 1, "only the named file is a source");
        assert_eq!(evidence.sources[0].path, named.display().to_string());
        assert!(
            !evidence.text.line(0).contains("OutOfMemoryError"),
            "the instance's latest.log leaked into the scan"
        );

        // The same run in `Run` scope does read the neighbours: this is the
        // behaviour the launch path needs.
        run.log_scope = super::LogScope::Run;
        let evidence = super::collect(&run);
        assert!(evidence.sources.len() > 1);
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn the_stage_follows_the_window_marker() {
        let playing = Text::parse("[stdout] Setting user\n[stdout] LWJGL Version 3.3.3\nx");
        assert_eq!(stage_of(&playing, 30), LaunchStage::InGame);

        let booting = Text::parse("a\nb\nc");
        assert_eq!(stage_of(&booting, 9), LaunchStage::NoWindow);

        let nothing = Text::parse("");
        assert_eq!(stage_of(&nothing, 0), LaunchStage::NeverStarted);
    }

    #[test]
    fn a_negative_code_is_a_native_abort() {
        let text = Text::parse("");
        assert_eq!(exit_of(Some(0), &text), ExitKind::Clean);
        assert_eq!(exit_of(Some(-1073741819), &text), ExitKind::NativeAbort);
        assert_eq!(exit_of(Some(1), &text), ExitKind::Crashed);
        assert_eq!(exit_of(None, &text), ExitKind::UserKilled);
    }

    #[test]
    fn a_killed_process_that_left_a_crash_report_still_counts_as_a_crash() {
        let text = Text::parse("---- Minecraft Crash Report ----");
        assert_eq!(exit_of(None, &text), ExitKind::Crashed);
    }

    /// A crash report left by an earlier session is not this run's evidence.
    ///
    /// It matters beyond tidiness: such a report carries `Backend library:
    /// LWJGL version`, which moves the stage to `InGame` and drops every
    /// startup-only rule. A Forge mod-resolution failure then loses to the old
    /// report's `NoClassDefFoundError` and the card says "repair the instance".
    #[test]
    fn a_crash_report_from_an_earlier_session_is_not_read() {
        let (root, run) = startup_run("stale-report");
        let old = run.instance_dir.join("crash-reports").join("crash-old.txt");
        std::fs::write(
            &old,
            "---- Minecraft Crash Report ----\nBackend library: LWJGL version 3.3.3\n",
        )
        .expect("old report");
        age_file(&old, 3 * 24 * 60 * 60);

        assert!(
            super::crash_report_for(&run).is_none(),
            "a three-day-old report is not this run's"
        );
        let evidence = super::collect(&run);
        assert_eq!(evidence.stage, LaunchStage::NoWindow);
        assert!(
            !text_contains(&evidence.text, "LWJGL"),
            "the stale report leaked into the scan"
        );

        // Written now, the fallback is exactly right: the bound rejects age,
        // not the fallback itself.
        let fresh = run.instance_dir.join("crash-reports").join("crash-new.txt");
        std::fs::write(
            &fresh,
            "---- Minecraft Crash Report ----\nBackend library: LWJGL version 3.3.3\n",
        )
        .expect("fresh report");
        assert_eq!(
            super::crash_report_for(&run).as_deref(),
            Some(fresh.as_path())
        );
        assert_eq!(
            super::collect(&run).stage,
            LaunchStage::InGame,
            "a report this run wrote is still read"
        );
        std::fs::remove_dir_all(root).expect("clean");
    }

    /// Same trap, same fix: `latest.log` is only evidence when this run wrote it.
    /// A leftover copy is not listed either, because the card's source list has to
    /// say what the diagnosis read.
    #[test]
    fn a_latest_log_from_an_earlier_session_is_not_read() {
        let (root, run) = startup_run("stale-latest");
        let latest = run.instance_dir.join("logs").join("latest.log");
        std::fs::write(&latest, "[stdout] Backend library: LWJGL version 3.3.3\n").expect("latest");
        age_file(&latest, 3 * 24 * 60 * 60);

        let evidence = super::collect(&run);
        assert_eq!(evidence.stage, LaunchStage::NoWindow);
        assert!(!text_contains(&evidence.text, "LWJGL"));
        assert!(
            evidence
                .sources
                .iter()
                .all(|source| source.path != latest.display().to_string()),
            "a stale latest.log was listed as a source"
        );
        std::fs::remove_dir_all(root).expect("clean");
    }

    /// The user-visible consequence of the rule above, end to end: with the
    /// stale report filtered out, the startup-only dependency rule survives and
    /// names the missing dependency; without it the card reported the old report's
    /// missing class and told the player to repair the instance.
    #[test]
    fn a_stale_report_does_not_hide_the_missing_dependency() {
        let (root, run) = startup_run("verdict");
        let old = run.instance_dir.join("crash-reports").join("crash-old.txt");
        std::fs::write(
            &old,
            "---- Minecraft Crash Report ----\n\
             Backend library: LWJGL version 3.3.3\n\
             Caused by: java.lang.NoClassDefFoundError: customskinloader/fake/Thing\n",
        )
        .expect("old report");
        age_file(&old, 3 * 24 * 60 * 60);

        let diagnosis =
            crate::core::diagnose::for_run(crate::core::diagnose::Facts::default(), &run);
        assert_eq!(diagnosis.stage, LaunchStage::NoWindow);
        assert_eq!(
            diagnosis.primary.as_ref().map(|finding| finding.rule),
            Some(crate::core::diagnose::RuleId::ForgeMissingDependencies)
        );
        std::fs::remove_dir_all(root).expect("clean");
    }

    /// A run that crashed during mod loading, in a real directory layout, with
    /// `started_at_epoch` set: the shape `collect` sees after a launch.
    fn startup_run(name: &str) -> (std::path::PathBuf, super::RunRecord) {
        let root =
            std::env::temp_dir().join(format!("tmcl-evidence-{name}-{}", uuid::Uuid::new_v4()));
        let instance = root.join("instance");
        std::fs::create_dir_all(instance.join("crash-reports")).expect("dirs");
        std::fs::create_dir_all(instance.join("logs")).expect("dirs");
        let log = root.join("run.log");
        std::fs::write(
            &log,
            "[stdout] Loading Minecraft 1.20.1 with Forge 47.4.10\n\
             [stdout] Missing or unsupported mandatory dependencies:\n\
             [stdout] \tMod ID: 'exposure', Requested by: 'exposure_camcorder'\n\
             [stderr] Caused by: java.lang.NoClassDefFoundError: customskinloader/fake/Thing\n",
        )
        .expect("log");
        let run = super::RunRecord {
            instance_id: uuid::Uuid::nil(),
            instance_name: name.to_owned(),
            instance_dir: instance,
            log_path: log,
            crash_report: None,
            exit_code: Some(1),
            duration_secs: 8,
            started_at_epoch: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_secs())
                .unwrap_or(0),
            launch_command: None,
            log_scope: super::LogScope::Run,
        };
        (root, run)
    }

    fn text_contains(text: &Text, needle: &str) -> bool {
        (0..text.len()).any(|index| text.line(index).contains(needle))
    }

    fn age_file(path: &std::path::Path, seconds: u64) {
        let when = std::time::SystemTime::now() - std::time::Duration::from_secs(seconds);
        std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .expect("open for mtime")
            .set_modified(when)
            .expect("set mtime");
    }
}
