//! The diagnosis bundle: one zip a helper can open without asking anything.
//!
//! A crash report alone usually is not enough. The launcher's own log, the JVM
//! fatal-error file and the exact launch command each answer a question the others
//! cannot, and collecting them by hand is where a bug report stalls, so the card
//! offers one file instead.
//!
//! Entry names are fixed: people forward these archives, and a reader should not
//! have to care which launcher version produced one. A missing source is skipped,
//! and `report.txt` says which ones were looked at; an empty bundle is better than
//! a bundle of plausible-looking placeholders.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use zip::write::SimpleFileOptions;

use super::evidence;
use super::{Diagnosis, LogScope, RunRecord};
use crate::core::error::Result;

/// Per-entry ceilings. They add up to well under [`MAX_TOTAL`], so the bundle cap
/// is a structural guarantee rather than a trimming pass; see
/// `the_entry_limits_cannot_reach_the_bundle_cap`.
pub const MAX_GAME_LOG: u64 = 4 * 1024 * 1024;
pub const MAX_LAUNCHER_LOG: u64 = 2 * 1024 * 1024;
pub const MAX_CRASH_REPORT: u64 = 2 * 1024 * 1024;
pub const MAX_HS_ERR: u64 = 1024 * 1024;
/// Refuse to produce anything larger. Unreachable with the limits above; kept
/// as a tripwire so a future entry cannot quietly turn the bundle into a dump.
pub const MAX_TOTAL: u64 = 24 * 1024 * 1024;

/// One Java runtime, as the bundle lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JavaFacts {
    pub path: String,
    pub version: String,
    pub vendor: String,
}

/// What only the front end knows.
///
/// Paths that belong to the run itself live in [`RunRecord`], and the verdict
/// lives in [`Diagnosis`]; this struct is the rest.
#[derive(Debug, Clone, Default)]
pub struct BundleInput {
    /// `mods/` entries: file name and size. The jar is never opened; reading a few
    /// hundred jars to list their metadata is how an export takes a minute.
    pub mods: Vec<(String, u64)>,
    pub java: Vec<JavaFacts>,
    /// The launcher's own log, if the player keeps one.
    pub launcher_log: Option<PathBuf>,
}

/// Everything one bundle needs, including the already-rendered report.
///
/// The report text comes in finished because `core` holds no language: the same
/// [`super::report::render`] output feeds the card, the CLI and the archive.
pub struct BundleRequest<'a> {
    pub report: String,
    pub diagnosis: &'a Diagnosis,
    pub run: &'a RunRecord,
    pub input: &'a BundleInput,
}

pub fn write(path: &Path, request: &BundleRequest<'_>) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let mut zip = zip::ZipWriter::new(File::create(path)?);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    let mut written = 0u64;
    written += write_text(&mut zip, "report.txt", &request.report, options)?;

    written += write_tail(
        &mut zip,
        "game.log",
        &request.run.log_path,
        MAX_GAME_LOG,
        options,
    )?;
    if let Some(launcher) = request.input.launcher_log.as_deref() {
        written += write_tail(
            &mut zip,
            "launcher.log",
            launcher,
            MAX_LAUNCHER_LOG,
            options,
        )?;
    }
    // Resolved the same way the diagnosis resolved it: a run that only knows
    // the report's folder still has a report to ship.
    if let Some(report) = evidence::crash_report_for(request.run) {
        written += write_tail(
            &mut zip,
            "crash-report.txt",
            &report,
            MAX_CRASH_REPORT,
            options,
        )?;
    }
    // Same rule as the evidence: a pointed-at file has no JVM fatal-error
    // file of its own, and one from an earlier session is not this run's.
    if request.run.log_scope == LogScope::Run
        && let Some(hs_err) = evidence::hs_err_for(request.run)
    {
        written += write_tail(&mut zip, "hs-err.log", &hs_err, MAX_HS_ERR, options)?;
    }

    written += write_text(&mut zip, "instance.json", &instance_json(request), options)?;
    written += write_text(&mut zip, "system.json", &system_json(request), options)?;
    if let Some(command) = request.run.launch_command.as_deref() {
        written += write_text(&mut zip, "launch.txt", command, options)?;
    }

    debug_assert!(
        written <= MAX_TOTAL,
        "the bundle grew past its ceiling: {written} bytes"
    );
    zip.finish().map_err(zip_error)?;
    Ok(())
}

/// `tmcl-diagnosis-<instance>-<stamp>.zip`, the name the save dialog suggests.
///
/// The stamp is the same UTC `yyyymmdd-HHMMSS` the game log uses, so the two
/// files from one run sort together.
pub fn suggested_file_name(instance_name: &str, epoch: u64) -> String {
    format!(
        "tmcl-diagnosis-{}-{}.zip",
        safe_stem(instance_name),
        crate::core::launch::session::utc_stamp(epoch)
    )
}

/// Characters Windows refuses in a file name become `_`.
fn safe_stem(name: &str) -> String {
    let mut out = String::new();
    for character in name.chars().take(40) {
        if character.is_control() || character.is_whitespace() || "\\/:*?\"<>|".contains(character)
        {
            out.push('_');
        } else {
            out.push(character);
        }
    }
    if out.trim_matches('_').is_empty() {
        "instance".to_owned()
    } else {
        out
    }
}

fn instance_json(request: &BundleRequest<'_>) -> String {
    let facts = &request.diagnosis.facts;
    let value = serde_json::json!({
        "name": request.run.instance_name,
        "path": request.run.instance_dir.display().to_string(),
        "minecraft_version": facts.minecraft_version,
        "loader": facts.loader,
        "java": {
            "path": facts.java_path,
            "version": facts.java_version,
            "major": facts.java_major,
        },
        "memory_mb": facts.max_memory_mb,
        "mods": request
            .input
            .mods
            .iter()
            .map(|(name, bytes)| serde_json::json!({ "name": name, "bytes": bytes }))
            .collect::<Vec<_>>(),
        "exit_code": request.run.exit_code,
        "duration_secs": request.run.duration_secs,
        "started_at": request.run.started_at_epoch,
    });
    pretty(&value)
}

fn system_json(request: &BundleRequest<'_>) -> String {
    let facts = &request.diagnosis.facts;
    let value = serde_json::json!({
        "tmcl": facts.tmcl_version,
        "os": facts.os,
        "arch": facts.arch,
        "system_memory_mb": facts.system_memory_mb,
        "java": request.input.java,
    });
    pretty(&value)
}

fn pretty(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn write_text(
    zip: &mut zip::ZipWriter<File>,
    name: &str,
    text: &str,
    options: SimpleFileOptions,
) -> Result<u64> {
    zip.start_file(name, options).map_err(zip_error)?;
    zip.write_all(text.as_bytes())?;
    Ok(text.len() as u64)
}

/// Append the tail of `path` if it exists. A missing source is not an error: the
/// bundle carries fewer entries, and `report.txt` lists what the diagnosis looked
/// for.
fn write_tail(
    zip: &mut zip::ZipWriter<File>,
    name: &str,
    path: &Path,
    limit: u64,
    options: SimpleFileOptions,
) -> Result<u64> {
    let Ok(bytes) = read_tail(path, limit) else {
        return Ok(0);
    };
    zip.start_file(name, options).map_err(zip_error)?;
    zip.write_all(&bytes)?;
    Ok(bytes.len() as u64)
}

/// `zip`'s own error type is not part of this crate's error surface; the
/// bundle only fails for I/O reasons in practice, so it is folded into one.
fn zip_error(error: zip::result::ZipError) -> crate::core::error::Error {
    std::io::Error::other(error).into()
}

/// The last `limit` bytes of a file.
///
/// Seeks from the end instead of reading the whole file: a multi-megabyte log
/// would otherwise be pulled into memory only to be cut. A byte cut in the middle
/// of a UTF-8 sequence is tolerated, because zip entries are bytes and the readers
/// of these logs open them as UTF-8 with replacement.
fn read_tail(path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let length = file.metadata()?.len();
    let start = length.saturating_sub(limit);
    if start > 0 {
        file.seek(SeekFrom::Start(start))?;
    }
    let mut bytes = Vec::with_capacity((length - start) as usize);
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::io::Read;

    use super::{
        BundleInput, BundleRequest, JavaFacts, MAX_CRASH_REPORT, MAX_GAME_LOG, MAX_HS_ERR,
        MAX_LAUNCHER_LOG, MAX_TOTAL, read_tail, suggested_file_name, write,
    };
    use crate::core::diagnose::finding::{EvidenceLine, Finding};
    use crate::core::diagnose::{
        Diagnosis, ExitKind, Facts, LaunchStage, RuleId, RunRecord, Source,
    };

    fn sample_run(root: &std::path::Path) -> RunRecord {
        RunRecord {
            instance_id: uuid::Uuid::nil(),
            instance_name: "生存 存档".to_owned(),
            instance_dir: root.to_path_buf(),
            log_path: root.join("game.log"),
            crash_report: Some(root.join("crash.txt")),
            exit_code: Some(-1073741819),
            duration_secs: 91,
            started_at_epoch: 1_794_135_731,
            launch_command: Some(
                "program: java.exe\nargs:\n  --accessToken\n  <redacted>\n".to_owned(),
            ),
            log_scope: super::LogScope::Run,
        }
    }

    fn sample_diagnosis() -> Diagnosis {
        Diagnosis {
            stage: LaunchStage::InGame,
            exit: ExitKind::Crashed,
            primary: Some(Finding {
                rule: RuleId::OutOfMemory,
                captures: Vec::new(),
                evidence: vec![EvidenceLine {
                    line: 2,
                    text: "java.lang.OutOfMemoryError: Java heap space".to_owned(),
                }],
                score: 50,
            }),
            others: Vec::new(),
            keywords: Vec::new(),
            facts: Facts {
                instance_name: "生存 存档".to_owned(),
                minecraft_version: "1.21.1".to_owned(),
                loader: "fabric 0.16.9".to_owned(),
                java_version: "21.0.1".to_owned(),
                java_major: 21,
                java_path: "C:/java/bin/java.exe".to_owned(),
                max_memory_mb: 4096,
                system_memory_mb: 32768,
                mod_count: 1,
                ..Facts::default()
            },
            sources: vec![Source {
                path: "game.log".to_owned(),
                present: true,
                bytes: 12,
            }],
            unknown: false,
        }
    }

    fn bundle_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!("tmcl-bundle-{tag}-{}", uuid::Uuid::new_v4()))
            .join("bundle.zip")
    }

    #[test]
    fn a_full_bundle_round_trips_through_the_archive() {
        let root = std::env::temp_dir().join(format!("tmcl-bundle-src-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("dir");
        std::fs::write(root.join("game.log"), "line one\nline two\n").expect("log");
        std::fs::write(root.join("crash.txt"), "---- Minecraft Crash Report ----\n")
            .expect("report");
        std::fs::write(root.join("launcher.log"), "launcher line\n").expect("launcher");
        std::fs::write(root.join("hs_err_pid42.log"), "SIGSEGV\n").expect("hs_err");

        let run = sample_run(&root);
        let diagnosis = sample_diagnosis();
        let input = BundleInput {
            mods: vec![("sodium-0.5.8.jar".to_owned(), 1024)],
            java: vec![JavaFacts {
                path: "C:/java/bin/java.exe".to_owned(),
                version: "21.0.1".to_owned(),
                vendor: "Eclipse Adoptium".to_owned(),
            }],
            launcher_log: Some(root.join("launcher.log")),
        };
        let path = bundle_path("full");
        write(
            &path,
            &BundleRequest {
                report: "结论\n内存不足\n".to_owned(),
                diagnosis: &diagnosis,
                run: &run,
                input: &input,
            },
        )
        .expect("write bundle");

        let mut archive =
            zip::ZipArchive::new(std::fs::File::open(&path).expect("open")).expect("read archive");
        let names = (0..archive.len())
            .map(|index| archive.by_index(index).expect("entry").name().to_owned())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            names,
            [
                "crash-report.txt",
                "game.log",
                "hs-err.log",
                "instance.json",
                "launch.txt",
                "launcher.log",
                "report.txt",
                "system.json",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>()
        );

        let mut report = String::new();
        archive
            .by_name("report.txt")
            .expect("report")
            .read_to_string(&mut report)
            .expect("read report");
        assert!(report.contains("结论"), "{report}");

        let mut command = String::new();
        archive
            .by_name("launch.txt")
            .expect("command")
            .read_to_string(&mut command)
            .expect("read command");
        assert!(!command.contains("secret"), "{command}");
        assert!(command.contains("<redacted>"), "{command}");

        let mut instance = String::new();
        archive
            .by_name("instance.json")
            .expect("instance")
            .read_to_string(&mut instance)
            .expect("read instance");
        assert!(instance.contains("\"sodium-0.5.8.jar\""), "{instance}");
        assert!(instance.contains("\"memory_mb\": 4096"), "{instance}");

        std::fs::remove_dir_all(root).expect("clean source");
        std::fs::remove_dir_all(path.parent().expect("bundle dir")).expect("clean bundle");
    }

    #[test]
    fn a_missing_crash_report_is_skipped_not_faked() {
        let root = std::env::temp_dir().join(format!("tmcl-bundle-miss-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("dir");
        std::fs::write(root.join("game.log"), "only the log\n").expect("log");
        let run = RunRecord {
            crash_report: Some(root.join("does-not-exist.txt")),
            launch_command: None,
            ..sample_run(&root)
        };
        let diagnosis = sample_diagnosis();
        let path = bundle_path("missing");
        write(
            &path,
            &BundleRequest {
                report: "report\n".to_owned(),
                diagnosis: &diagnosis,
                run: &run,
                input: &BundleInput::default(),
            },
        )
        .expect("write bundle");

        let mut archive =
            zip::ZipArchive::new(std::fs::File::open(&path).expect("open")).expect("read archive");
        assert!(archive.by_name("crash-report.txt").is_err());
        // No launch command captured, so no launch.txt either.
        assert!(archive.by_name("launch.txt").is_err());
        assert!(archive.by_name("game.log").is_ok());

        std::fs::remove_dir_all(root).expect("clean source");
        std::fs::remove_dir_all(path.parent().expect("bundle dir")).expect("clean bundle");
    }

    /// Writing over an existing export must replace it: the save dialog asks
    /// before it gets here, and a second attempt after a fix is the normal case.
    #[test]
    fn a_second_export_overwrites_the_first() {
        let root = std::env::temp_dir().join(format!("tmcl-bundle-twice-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("dir");
        std::fs::write(root.join("game.log"), "first\n").expect("log");
        let run = sample_run(&root);
        let diagnosis = sample_diagnosis();
        let path = bundle_path("twice");
        for report in ["first report", "second report"] {
            write(
                &path,
                &BundleRequest {
                    report: report.to_owned(),
                    diagnosis: &diagnosis,
                    run: &run,
                    input: &BundleInput::default(),
                },
            )
            .expect("write bundle");
        }
        let mut archive =
            zip::ZipArchive::new(std::fs::File::open(&path).expect("open")).expect("read archive");
        let mut text = String::new();
        archive
            .by_name("report.txt")
            .expect("report")
            .read_to_string(&mut text)
            .expect("read");
        assert_eq!(text, "second report");

        std::fs::remove_dir_all(root).expect("clean source");
        std::fs::remove_dir_all(path.parent().expect("bundle dir")).expect("clean bundle");
    }

    #[test]
    fn the_entry_limits_cannot_reach_the_bundle_cap() {
        let logs = MAX_GAME_LOG + MAX_LAUNCHER_LOG + MAX_CRASH_REPORT + MAX_HS_ERR;
        assert!(
            logs < MAX_TOTAL / 2,
            "logs alone would need {logs} bytes of a {MAX_TOTAL} byte budget"
        );
    }

    #[test]
    fn only_the_tail_of_a_large_file_is_read() {
        let root = std::env::temp_dir().join(format!("tmcl-bundle-tail-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("dir");
        let path = root.join("big.log");
        std::fs::write(&path, vec![b'x'; 4096]).expect("log");
        std::fs::write(&path, format!("{}END", "x".repeat(4096))).expect("log");
        let tail = read_tail(&path, 3).expect("tail");
        assert_eq!(tail, b"END");
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn the_suggested_name_is_safe_and_sorts_with_the_log() {
        let name = suggested_file_name("生存 存档/测试", 1_794_135_731);
        assert_eq!(name, "tmcl-diagnosis-生存_存档_测试-20261108-110211.zip");
        assert!(!name.contains('/'));
        assert_eq!(
            suggested_file_name("  ", 0),
            "tmcl-diagnosis-instance-19700101-000000.zip"
        );
    }
}
