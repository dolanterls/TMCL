//! Crash report triage.
//!
//! A launcher can only see an exit code, and a crash report is a wall of stack
//! frames. The rules here cover the handful of causes that account for most
//! "it closed immediately" reports, so the log page can lead with a cause and
//! an action instead of leaving the player to read `hs_err_pid` output.
//!
//! The patterns are deliberately plain string matches: the report is already
//! structured enough, and a regex table would be harder to review and to keep
//! independent of other launchers.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// How much of the tail of a log is inspected when there is no crash report.
const LOG_TAIL: u64 = 512 * 1024;
const MAX_DETAIL: usize = 5;
const MAX_DETAIL_LEN: usize = 240;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrashKind {
    JavaTooOld,
    OutOfMemory,
    DuplicateMods,
    MissingDependency,
    GraphicsDriver,
    MixinFailure,
}

impl CrashKind {
    pub fn message_key(self) -> &'static str {
        match self {
            Self::JavaTooOld => "crash.java_too_old",
            Self::OutOfMemory => "crash.out_of_memory",
            Self::DuplicateMods => "crash.duplicate_mods",
            Self::MissingDependency => "crash.missing_dependency",
            Self::GraphicsDriver => "crash.graphics_driver",
            Self::MixinFailure => "crash.mixin_failure",
        }
    }

    pub fn advice_key(self) -> &'static str {
        match self {
            Self::JavaTooOld => "crash.advice.change_java",
            Self::OutOfMemory => "crash.advice.raise_memory",
            Self::DuplicateMods => "crash.advice.remove_duplicates",
            Self::MissingDependency => "crash.advice.install_dependency",
            Self::GraphicsDriver => "crash.advice.update_driver",
            Self::MixinFailure => "crash.advice.check_mod_versions",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrashFinding {
    pub kind: CrashKind,
    /// The lines that triggered the rule, capped so the log page stays usable.
    pub detail: Vec<String>,
}

/// Reads a crash report when the game wrote one, otherwise the tail of the
/// launcher's captured log.
pub fn analyze_files(log_path: &Path, crash_report: Option<&Path>) -> Option<CrashFinding> {
    if let Some(path) = crash_report
        && let Some(text) = read_text(path, u64::MAX)
    {
        return analyze(&text);
    }
    let text = read_text(log_path, LOG_TAIL)?;
    analyze(&text)
}

pub fn analyze(text: &str) -> Option<CrashFinding> {
    let lower = text.to_lowercase();

    if lower.contains("unsupportedclassversionerror")
        || lower.contains("compiled by a more recent version of the java runtime")
    {
        return Some(finding(
            CrashKind::JavaTooOld,
            text,
            &["UnsupportedClassVersionError"],
        ));
    }
    if lower.contains("java.lang.outofmemoryerror")
        || lower.contains("insufficient memory for the java runtime environment")
        || lower.contains("could not reserve enough space for")
        || lower.contains("too small maximum heap")
    {
        return Some(finding(
            CrashKind::OutOfMemory,
            text,
            &[
                "outofmemoryerror",
                "insufficient memory",
                "could not reserve enough space",
                "too small maximum heap",
            ],
        ));
    }
    if lower.contains("found a duplicate mod")
        || lower.contains("duplicatemodsfoundexception")
        || lower.contains("found duplicate mods")
    {
        return Some(finding(CrashKind::DuplicateMods, text, &["duplicate mod"]));
    }
    if lower.contains("missing or unsupported mandatory dependencies")
        || lower.contains("modresolutionexception: could not find required mod")
    {
        return Some(finding(
            CrashKind::MissingDependency,
            text,
            &["required mod", "mandatory dependencies", "requested by"],
        ));
    }
    if lower.contains("pixel format not accelerated")
        || lower.contains("the driver does not appear to support opengl")
        || (lower.contains("exception_access_violation")
            && (lower.contains("nvoglv") || lower.contains("atio") || lower.contains("igx")))
    {
        return Some(finding(
            CrashKind::GraphicsDriver,
            text,
            &[
                "opengl",
                "access_violation",
                "nvoglv",
                "atio",
                "pixel format",
            ],
        ));
    }
    if lower.contains("mixin apply failed")
        || lower.contains("mixintransformererror")
        || lower.contains("mixin.injection.throwables")
    {
        return Some(finding(CrashKind::MixinFailure, text, &["Mixin"]));
    }
    None
}

fn finding(kind: CrashKind, text: &str, needles: &[&str]) -> CrashFinding {
    let mut detail = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_lowercase();
        if needles
            .iter()
            .any(|needle| lower.contains(&needle.to_lowercase()))
        {
            detail.push(truncate(trimmed));
            if detail.len() == MAX_DETAIL {
                break;
            }
        }
    }
    CrashFinding { kind, detail }
}

fn truncate(line: &str) -> String {
    if line.chars().count() <= MAX_DETAIL_LEN {
        return line.to_owned();
    }
    let mut value = line.chars().take(MAX_DETAIL_LEN).collect::<String>();
    value.push('…');
    value
}

/// Reads at most `limit` bytes from the end of a file. Crash reports are small;
/// captured logs can be tens of megabytes and only their tail matters.
fn read_text(path: &Path, limit: u64) -> Option<String> {
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
    use super::{CrashKind, analyze};

    #[test]
    fn recognises_a_java_version_mismatch() {
        let finding = analyze(
            "java.lang.UnsupportedClassVersionError: net/fabricmc/loader/impl/launch/knot/KnotClient has been compiled by a more recent version of the Java Runtime",
        )
        .expect("finding");
        assert_eq!(finding.kind, CrashKind::JavaTooOld);
        assert!(!finding.detail.is_empty());
    }

    #[test]
    fn recognises_memory_exhaustion() {
        let finding = analyze("java.lang.OutOfMemoryError: Java heap space").expect("finding");
        assert_eq!(finding.kind, CrashKind::OutOfMemory);
    }

    #[test]
    fn recognises_duplicate_mods() {
        let finding = analyze(
            "Found a duplicate mod bettermineshafts at mods/bettermineshafts-1.0.jar\nFound a duplicate mod bettermineshafts at mods/bettermineshafts-1.1.jar",
        )
        .expect("finding");
        assert_eq!(finding.kind, CrashKind::DuplicateMods);
        assert_eq!(finding.detail.len(), 2);
    }

    #[test]
    fn recognises_a_missing_dependency() {
        let finding = analyze(
            "Missing or unsupported mandatory dependencies:\n\tMod ID: 'cloth-config', Requested by: 'sodium-extra'",
        )
        .expect("finding");
        assert_eq!(finding.kind, CrashKind::MissingDependency);
        assert!(
            finding
                .detail
                .iter()
                .any(|line| line.contains("cloth-config")),
            "{:?}",
            finding.detail
        );
    }

    #[test]
    fn recognises_a_graphics_driver_crash() {
        let finding = analyze(
            "A fatal error has been detected by the Java Runtime Environment:\nEXCEPTION_ACCESS_VIOLATION (0xc0000005)\nC  [nvoglv64.dll+0x9f3a12]",
        )
        .expect("finding");
        assert_eq!(finding.kind, CrashKind::GraphicsDriver);
    }

    #[test]
    fn recognises_a_mixin_failure() {
        let finding = analyze("Mixin apply failed mixins.json:create.mixins.json -> mixin.create")
            .expect("finding");
        assert_eq!(finding.kind, CrashKind::MixinFailure);
    }

    #[test]
    fn a_clean_log_has_no_finding() {
        assert!(analyze("[stdout] Setting user\n[stdout] LWJGL Version").is_none());
    }

    #[test]
    fn the_report_is_preferred_over_the_captured_log() {
        use std::io::Write;

        let root = std::env::temp_dir().join(format!("tmcl-crash-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("dir");
        let log = root.join("launch.log");
        std::fs::write(&log, "Started\n").expect("log");
        let report = root.join("crash.txt");
        let mut file = std::fs::File::create(&report).expect("report");
        writeln!(file, "java.lang.OutOfMemoryError: Java heap space").expect("report body");
        let finding = super::analyze_files(&log, Some(&report)).expect("finding");
        assert_eq!(finding.kind, CrashKind::OutOfMemory);
        std::fs::remove_dir_all(root).expect("clean");
    }
}
