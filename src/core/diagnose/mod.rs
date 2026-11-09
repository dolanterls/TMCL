//! Crash diagnosis: evidence in, one ranked conclusion out.
//!
//! Five lines printed into the log buffer scroll away, so the work is split into
//! four jobs:
//!
//! 1. [`evidence`] collects what the run left behind (log tail, crash report,
//!    `hs_err_pid*.log`, `latest.log`) and decides the stage and the exit kind.
//! 2. [`rule`] / [`rules`] hold the 59 phenomena worth naming, as data.
//! 3. [`finding`] scores every hit, keeps one per family, and picks a primary.
//! 4. [`report`] renders the same diagnosis for the overlay, the bundle and the
//!    CLI without owning any text: strings come in through [`report::TextFn`].
//!
//! Nothing here draws or reads `App`; the front ends fill [`Facts`] in and read
//! the [`Diagnosis`] back.

pub mod bundle;
pub mod engine;
pub mod evidence;
pub mod finding;
pub mod keywords;
pub mod report;
pub mod rule;
pub mod rule_id;
pub mod rules;

/// The action type is `launch::failure::Action`; the alias is what the rule
/// table and the card read, because "remedy" is what it means here.
pub use crate::core::launch::failure::Action as Remedy;
pub use finding::Finding;
pub use rule::{Matcher, Rule};
pub use rule_id::RuleId;

use std::path::PathBuf;

use serde::Serialize;

/// What a rule is about. Only used for grouping and for the `NativeAbort`
/// weighting; the verdict comes from the rules themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Category {
    Java,
    Memory,
    Graphics,
    Loader,
    Native,
    Integrity,
    World,
}

/// Rules that answer the same question share a family, and only the highest
/// scorer survives. Five OptiFine incompatibilities are one finding: the action
/// is the same, and listing five buries the cause.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Family {
    JavaVersion,
    JavaVendor,
    Memory,
    Graphics,
    NativeClass,
    ModResolution,
    Mixin,
    OptiFine,
    Integrity,
    WorldData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Any,
    Windows,
    MacOs,
}

impl Platform {
    pub fn host() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Any
        }
    }

    pub fn allows(self, host: Self) -> bool {
        matches!(self, Self::Any) || self == host
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LaunchStage {
    /// No log at all, or fewer than three lines: the JVM never got going.
    NeverStarted,
    /// A log exists but nothing ever created a window.
    NoWindow,
    /// LWJGL started, so the failure happened inside the game.
    InGame,
}

impl LaunchStage {
    pub const fn key(self) -> &'static str {
        match self {
            Self::NeverStarted => "diagnose.stage.never_started",
            Self::NoWindow => "diagnose.stage.no_window",
            Self::InGame => "diagnose.stage.in_game",
        }
    }
}

/// Which stages a rule can fire in. A rule that names a world entity cannot be
/// the reason a JVM failed to start, and letting it try is how advice ends up
/// telling the player to delete a block they never reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StageMask(u8);

impl StageMask {
    pub const NEVER_STARTED: Self = Self(0b001);
    pub const NO_WINDOW: Self = Self(0b010);
    pub const IN_GAME: Self = Self(0b100);
    pub const STARTUP: Self = Self(0b011);
    pub const ALL: Self = Self(0b111);

    pub const fn contains(self, stage: LaunchStage) -> bool {
        let bit = match stage {
            LaunchStage::NeverStarted => 0b001,
            LaunchStage::NoWindow => 0b010,
            LaunchStage::InGame => 0b100,
        };
        self.0 & bit != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum ExitKind {
    Clean,
    UserKilled,
    /// The JVM died natively: Windows reports the `0xC0000005` family as a
    /// negative `i32`, and no Minecraft crash handler ran.
    NativeAbort,
    Crashed,
}

impl ExitKind {
    pub const fn key(self) -> &'static str {
        match self {
            Self::Clean => "diagnose.exit.clean",
            Self::UserKilled => "diagnose.exit.user_killed",
            Self::NativeAbort => "diagnose.exit.native_abort",
            Self::Crashed => "diagnose.exit.crashed",
        }
    }

    pub const fn is_crash(self) -> bool {
        matches!(self, Self::NativeAbort | Self::Crashed)
    }
}

/// A value pulled out of a matching line. The enum is the set of slots a
/// message template may use; a rule cannot invent a placeholder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum Capture {
    Mod,
    ModId,
    Path,
    File,
    Class,
    /// The Java feature release a class file needs. A class file says `61.0`,
    /// which means Java 17; the engine subtracts 44 when it captures one, so a
    /// message template never has to explain the encoding.
    ExpectedJava,
    Version,
    EntityType,
    Location,
    Reason,
}

impl Capture {
    /// Slot name inside a `{…}` placeholder.
    pub const fn slot(self) -> &'static str {
        match self {
            Self::Mod => "mod",
            Self::ModId => "id",
            Self::Path => "path",
            Self::File => "file",
            Self::Class => "class",
            Self::ExpectedJava => "expected",
            Self::Version => "version",
            Self::EntityType => "type",
            Self::Location => "location",
            Self::Reason => "reason",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Facts {
    pub tmcl_version: String,
    pub instance_name: String,
    pub minecraft_version: String,
    pub loader: String,
    pub java_version: String,
    pub java_major: u32,
    pub java_path: String,
    pub max_memory_mb: u32,
    pub system_memory_mb: u64,
    pub mod_count: usize,
    pub exit_code: Option<i32>,
    pub duration_secs: u64,
    pub os: String,
    pub arch: String,
}

impl Default for Facts {
    fn default() -> Self {
        Self {
            tmcl_version: env!("CARGO_PKG_VERSION").to_owned(),
            instance_name: String::new(),
            minecraft_version: String::new(),
            loader: "vanilla".to_owned(),
            java_version: String::new(),
            java_major: 0,
            java_path: String::new(),
            max_memory_mb: 0,
            system_memory_mb: 0,
            mod_count: 0,
            exit_code: None,
            duration_secs: 0,
            os: std::env::consts::OS.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
        }
    }
}

/// Which files the evidence layer is allowed to read.
///
/// `Run` is a launch TMCL performed: the captured log is one of four sources, and
/// the crash report, `hs_err` and `latest.log` beside it belong to the same
/// session. `File` is a log the caller pointed at (`--log`): the other files in the
/// instance may come from a later run, and appending them lets that run outrank the
/// one being asked about, because the later file sits at the end of the scanned
/// text where the position bonus lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogScope {
    #[default]
    Run,
    File,
}

/// The identity of one finished launch. Everything evidence collection needs to
/// look at the right directory, and nothing that depends on the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRecord {
    pub instance_id: uuid::Uuid,
    pub instance_name: String,
    pub instance_dir: PathBuf,
    pub log_path: PathBuf,
    pub crash_report: Option<PathBuf>,
    pub exit_code: Option<i32>,
    pub duration_secs: u64,
    pub started_at_epoch: u64,
    /// The launch command with the access token masked, when the front end
    /// captured one. `None` for a verdict built from a log found on disk.
    pub launch_command: Option<String>,
    /// Whether the other files in the instance belong to this run.
    pub log_scope: LogScope,
}

/// One input file and whether it was there. The report lists these so a reader
/// knows what the diagnosis did *not* see.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Source {
    pub path: String,
    pub present: bool,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnosis {
    pub stage: LaunchStage,
    pub exit: ExitKind,
    pub primary: Option<Finding>,
    pub others: Vec<Finding>,
    pub keywords: Vec<String>,
    pub facts: Facts,
    pub sources: Vec<Source>,
    /// No rule matched. The overlay says so instead of inventing a cause.
    pub unknown: bool,
}

impl Diagnosis {
    fn idle(stage: LaunchStage, exit: ExitKind, facts: Facts, sources: Vec<Source>) -> Self {
        Self {
            stage,
            exit,
            primary: None,
            others: Vec::new(),
            keywords: Vec::new(),
            facts,
            sources,
            unknown: false,
        }
    }
}

/// The one entry point: collect the evidence, score the rules, rank the hits.
///
/// A clean exit and a killed process still return a `Diagnosis`, which the overlay
/// uses to say "the game quit normally" rather than showing the last crash.
pub fn for_run(facts: Facts, run: &RunRecord) -> Diagnosis {
    let evidence = evidence::collect(run);
    let (stage, mut exit) = (evidence.stage, evidence.exit);
    // No exit code means the caller read a file, where `UserKilled` only says
    // "no exception-shaped line near the end". A loader that prints one
    // sentence and exits (`OpenJ9 is incompatible`, `Could not reserve enough
    // space`) never matches those words, so a confident rule hit is allowed to
    // outrank the word list.
    if exit == ExitKind::UserKilled && has_confident_hit(&evidence.text, stage) {
        exit = ExitKind::Crashed;
    }
    if !exit.is_crash() {
        return Diagnosis::idle(stage, exit, facts, evidence.sources);
    }

    let (primary, others) = finding::rank(finding::collect(&evidence.text, stage, exit));
    let unknown = primary.is_none();
    let keywords = if unknown {
        keywords::extract(&evidence.text, evidence.text.crash_start())
    } else {
        Vec::new()
    };
    Diagnosis {
        stage,
        exit,
        primary,
        others,
        keywords,
        facts,
        sources: evidence.sources,
        unknown,
    }
}

/// A rule that cannot plausibly mean anything else fired.
///
/// Weight 3 and up are the rules whose signal is the failure itself; the
/// weight-2 pair (`FabricWarnings`, `LoaderReportedModFailure`) only say "the
/// loader printed something", and must not turn a log into a crash on their
/// own.
fn has_confident_hit(text: &engine::Text, stage: LaunchStage) -> bool {
    let host = Platform::host();
    rules::RULES.iter().any(|rule| {
        rule.weight >= 3
            && rule.stages.contains(stage)
            && rule.platform.allows(host)
            && engine::scan(text, rule).is_some()
    })
}

#[cfg(test)]
mod tests {
    use super::{ExitKind, LaunchStage, Platform, StageMask};

    #[test]
    fn a_stage_mask_only_admits_the_stages_it_names() {
        assert!(StageMask::STARTUP.contains(LaunchStage::NeverStarted));
        assert!(StageMask::STARTUP.contains(LaunchStage::NoWindow));
        assert!(!StageMask::STARTUP.contains(LaunchStage::InGame));
        assert!(StageMask::ALL.contains(LaunchStage::InGame));
    }

    #[test]
    fn the_platform_gate_lets_any_through_and_gates_the_rest() {
        assert!(Platform::Any.allows(Platform::Windows));
        assert!(Platform::Any.allows(Platform::MacOs));
        assert!(Platform::Windows.allows(Platform::Windows));
        assert!(!Platform::Windows.allows(Platform::MacOs));
    }

    /// The scan budget from the design: half a megabyte and fifty-nine rules
    /// must stay well inside a frame's worth of work. Debug builds are slow, so
    /// the bound is generous; release is an order of magnitude faster.
    #[test]
    fn half_a_megabyte_of_log_is_analysed_quickly() {
        use std::time::Instant;

        let mut body = String::new();
        for index in 0..9000 {
            body.push_str("[stdout] [19:02:14] [main/INFO]: - some_mod-1.0.jar loaded ");
            body.push_str(&index.to_string());
            body.push('\n');
        }
        body.push_str("[stderr] java.lang.OutOfMemoryError: Java heap space\n");
        assert!(body.len() > 500 * 1024, "{}", body.len());

        let started = Instant::now();
        let diagnosis = super::for_run(super::Facts::default(), &run_with_log(&body));
        let elapsed = started.elapsed();
        assert_eq!(
            diagnosis.primary.as_ref().map(|finding| finding.rule),
            Some(super::RuleId::OutOfMemory)
        );
        // Visible with `--nocapture`, which is how the number gets recorded in
        // the acceptance notes: debug and release are an order of magnitude
        // apart, and only the release number says anything about a frame.
        println!("diagnosis of {} bytes took {elapsed:?}", body.len());
        assert!(
            elapsed.as_millis() < 300,
            "diagnosis took {elapsed:?} for {} bytes",
            body.len()
        );
    }

    /// A four-megabyte log is read from the tail, not from the start: the only
    /// part that explains a crash is what the game printed last, and reading
    /// the whole file would put a multi-megabyte allocation on the exit path.
    #[test]
    fn a_huge_log_is_read_from_the_tail() {
        let root =
            std::env::temp_dir().join(format!("tmcl-evidence-tail-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("dir");
        let mut body = String::with_capacity(4 * 1024 * 1024);
        while body.len() < 4 * 1024 * 1024 {
            body.push_str("[stdout] [19:02:14] [main/INFO]: preparing spawn area ");
            body.push_str(&body.len().to_string());
            body.push('\n');
        }
        body.push_str("[stderr] java.lang.OutOfMemoryError: Java heap space\n");
        let log = root.join("game.log");
        std::fs::write(&log, &body).expect("log");

        let diagnosis = super::for_run(
            super::Facts::default(),
            &super::RunRecord {
                instance_id: uuid::Uuid::nil(),
                instance_name: "big".to_owned(),
                instance_dir: root.clone(),
                log_path: log,
                crash_report: None,
                exit_code: Some(1),
                log_scope: super::LogScope::File,
                duration_secs: 60,
                started_at_epoch: 0,
                launch_command: None,
            },
        );
        let read = diagnosis
            .sources
            .first()
            .expect("the log is a source")
            .bytes;
        assert!(
            read <= super::evidence::LOG_TAIL,
            "read {read} bytes of a {} byte log",
            body.len()
        );
        assert_eq!(
            diagnosis.primary.as_ref().map(|finding| finding.rule),
            Some(super::RuleId::OutOfMemory),
            "the tail, not the head, has to be scanned"
        );
        std::fs::remove_dir_all(root).expect("clean");
    }

    /// The whole chain in one call: evidence, stage, exit classification,
    /// ranking. The fixture guard stops at `collect` + `rank`, so without these
    /// three the layers are only tested apart from each other.
    #[test]
    fn memory_and_a_mixin_failure_are_reported_together() {
        let body = [
            "[stdout] [19:02:11] [main/INFO]: Loading Minecraft 1.21.1 with Fabric Loader 0.16.9",
            "[stdout] [19:02:12] [main/INFO]: Loading 42 mods",
            "[stdout] [19:02:20] [main/ERROR]: Mixin apply failed create.mixins.json:BlockMixin",
            "[stderr] java.lang.OutOfMemoryError: Java heap space",
        ]
        .join("\n");
        let run = run_with_log(&body);
        let diagnosis = super::for_run(super::Facts::default(), &run);

        assert_eq!(diagnosis.stage, LaunchStage::NoWindow);
        assert_eq!(diagnosis.exit, ExitKind::Crashed);
        assert_eq!(
            diagnosis.primary.as_ref().map(|finding| finding.rule),
            Some(super::RuleId::OutOfMemory)
        );
        assert_eq!(diagnosis.others.len(), 1);
        assert_eq!(diagnosis.others[0].rule, super::RuleId::MixinApplyError);
        assert!(!diagnosis.unknown, "a verdict, not keywords");
        std::fs::remove_dir_all(&run.instance_dir).expect("clean");
    }

    #[test]
    fn a_missing_dependency_is_named_with_the_mod_it_belongs_to() {
        let body = [
            "[stdout] [19:02:11] [main/INFO]: Loading Minecraft 1.20.1 with Fabric Loader 0.15.0",
            "[stdout] [19:02:12] [main/INFO]: Loading 42 mods",
            "[stderr] net.fabricmc.loader.impl.FormattedException: ModResolutionException:              Could not find required mod: create requires flywheel",
        ]
        .join("\n");
        let run = run_with_log(&body);
        let diagnosis = super::for_run(super::Facts::default(), &run);

        assert_eq!(diagnosis.stage, LaunchStage::NoWindow);
        let primary = diagnosis.primary.as_ref().expect("primary");
        assert_eq!(primary.rule, super::RuleId::FabricMissingDependency);
        assert_eq!(
            primary.argument("mod"),
            Some(("mod", "create")),
            "the card has to name the mod that owns the missing dependency"
        );
        std::fs::remove_dir_all(&run.instance_dir).expect("clean");
    }

    /// With no exit code, a confident rule hit outranks the English word list:
    /// `OpenJ9 is incompatible` is a fatal message that contains none of the
    /// words `exit_of` looks for.
    #[test]
    fn a_confident_hit_counts_as_a_crash_without_an_exit_code() {
        let mut run = run_with_log("[stdout] OpenJ9 is incompatible with Minecraft");
        run.exit_code = None;
        let diagnosis = super::for_run(super::Facts::default(), &run);
        assert_eq!(diagnosis.exit, ExitKind::Crashed);
        assert_eq!(
            diagnosis.primary.as_ref().map(|finding| finding.rule),
            Some(super::RuleId::OpenJ9)
        );
        std::fs::remove_dir_all(&run.instance_dir).expect("clean");
    }

    /// The weight-2 pair only says "the loader printed something"; that must
    /// not turn a log into a crash on its own.
    #[test]
    fn a_vague_hit_alone_does_not_turn_a_file_into_a_crash() {
        let mut run = run_with_log("[stdout] Warnings were found!");
        run.exit_code = None;
        let diagnosis = super::for_run(super::Facts::default(), &run);
        assert_eq!(diagnosis.exit, ExitKind::UserKilled);
        assert!(diagnosis.primary.is_none());
        std::fs::remove_dir_all(&run.instance_dir).expect("clean");
    }

    /// A log that announces its own shutdown is clean even without an exit
    /// code, and a clean run is never diagnosed.
    #[test]
    fn a_log_that_says_stopping_is_clean() {
        let mut run = run_with_log(
            "[stdout] [19:02:11] [main/INFO]: Backend library: LWJGL version 3.3.3\n\
             [stdout] [19:42:20] [main/INFO]: Stopping!",
        );
        run.exit_code = None;
        let diagnosis = super::for_run(super::Facts::default(), &run);
        assert_eq!(diagnosis.exit, ExitKind::Clean);
        assert!(diagnosis.primary.is_none());
        std::fs::remove_dir_all(&run.instance_dir).expect("clean");
    }

    /// A clean exit is never diagnosed, even when a scary word turns up in the
    /// log. This is what stops "I closed the game" from opening a crash card.
    #[test]
    fn a_clean_exit_has_no_verdict_even_with_a_scary_line() {
        let body = [
            "[stdout] [19:02:11] [main/INFO]: Setting user: Player",
            "[stdout] [19:02:12] [main/INFO]: Backend library: LWJGL version 3.3.3",
            "[stdout] [19:20:00] [main/WARN]: java.lang.OutOfMemoryError: Java heap space",
            "[stdout] [19:42:20] [main/INFO]: Stopping!",
        ]
        .join("\n");
        let mut run = run_with_log(&body);
        run.exit_code = Some(0);
        let diagnosis = super::for_run(super::Facts::default(), &run);

        assert_eq!(diagnosis.exit, ExitKind::Clean);
        assert!(diagnosis.primary.is_none());
        assert!(diagnosis.others.is_empty());
        assert!(!diagnosis.unknown);
        std::fs::remove_dir_all(&run.instance_dir).expect("clean");
    }

    /// A run record pointing at a temporary log with `body` in it.
    fn run_with_log(body: &str) -> super::RunRecord {
        let root =
            std::env::temp_dir().join(format!("tmcl-diagnose-perf-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("dir");
        let log = root.join("game.log");
        std::fs::write(&log, body).expect("log");
        super::RunRecord {
            instance_id: uuid::Uuid::nil(),
            instance_name: "perf".to_owned(),
            instance_dir: root,
            log_path: log,
            crash_report: None,
            exit_code: Some(1),
            duration_secs: 60,
            started_at_epoch: 0,
            launch_command: None,
            log_scope: super::LogScope::Run,
        }
    }

    /// The dependency direction is part of the design: `diagnose` reads
    /// `launch`, never the other way round. Two modules in one crate can call
    /// into each other without the compiler complaining, so a rule table that
    /// reaches back — or a `process.rs` that imports a diagnosis helper — would
    /// only be noticed by whoever next tried to move one of them.
    #[test]
    fn launch_never_reaches_into_diagnose() {
        use std::path::Path;

        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/core/launch");
        let mut stack = vec![root];
        let mut offenders = Vec::new();
        while let Some(directory) = stack.pop() {
            for entry in std::fs::read_dir(&directory)
                .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
                .flatten()
            {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().is_none_or(|extension| extension != "rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
                if text.contains("core::diagnose") {
                    offenders.push(path.display().to_string());
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "launch depends on diagnose here: {offenders:?}"
        );
    }

    #[test]
    fn only_a_crash_exit_asks_for_a_verdict() {
        assert!(!ExitKind::Clean.is_crash());
        assert!(!ExitKind::UserKilled.is_crash());
        assert!(ExitKind::Crashed.is_crash());
        assert!(ExitKind::NativeAbort.is_crash());
    }
}
