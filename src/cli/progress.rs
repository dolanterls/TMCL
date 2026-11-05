//! Progress output for long-running tasks.
//!
//! Four shapes, chosen once from `--json` / `--quiet` / whether stderr is a
//! terminal:
//!
//! | mode | when | looks like |
//! |---|---|---|
//! | bar | stderr is a terminal | one line rewritten with `\r` |
//! | line | stderr is redirected | a plain line every ≥5% or ≥2s |
//! | ndjson | `--json` | one JSON object per line, for machine consumers |
//! | silent | `--quiet` | nothing |
//!
//! Progress always goes to stderr: `tmcl ... --json | jq` must keep working
//! while a task prints progress.
//!
//! The renderer returns the text instead of writing it. That keeps the byte
//! formatting testable and leaves the choice of stream to the caller.

use std::time::{Duration, Instant};

use uuid::Uuid;

use crate::core::task::{TaskSnapshot, TaskState};

/// How often a line-mode update is forced even when the percentage barely moved.
const LINE_INTERVAL: Duration = Duration::from_secs(2);
/// Percentage step that forces a line-mode update.
const LINE_STEP: u32 = 5;
/// Sampling window for the transfer rate.
const RATE_WINDOW: Duration = Duration::from_millis(200);
/// Weight of the newest sample; the rest is the previous average.
const RATE_ALPHA: f64 = 0.3;
const BAR_WIDTH: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressMode {
    Bar,
    Line,
    Ndjson,
    Silent,
}

impl ProgressMode {
    /// `quiet` wins over `json`: a caller that asked for silence gets silence.
    pub fn choose(json: bool, quiet: bool, stderr_is_terminal: bool) -> Self {
        if quiet {
            Self::Silent
        } else if json {
            Self::Ndjson
        } else if stderr_is_terminal {
            Self::Bar
        } else {
            Self::Line
        }
    }
}

pub struct ProgressRenderer {
    mode: ProgressMode,
    task: Uuid,
    label: String,
    started: bool,
    last_percent: Option<u32>,
    last_emit: Option<Instant>,
    last_sample: Option<(Instant, u64)>,
    rate: Option<f64>,
    /// Width of the last bar line, so a shorter update can overwrite the tail.
    last_width: usize,
    /// Set once the final event was rendered; a second one would duplicate it.
    done: bool,
}

impl ProgressRenderer {
    pub fn new(mode: ProgressMode, task: Uuid, label: impl Into<String>) -> Self {
        Self {
            mode,
            task,
            label: label.into(),
            started: false,
            last_percent: None,
            last_emit: None,
            last_sample: None,
            rate: None,
            last_width: 0,
            done: false,
        }
    }

    pub fn mode(&self) -> ProgressMode {
        self.mode
    }

    pub fn on_snapshot(&mut self, snapshot: &TaskSnapshot) -> Option<String> {
        self.on_snapshot_at(snapshot, Instant::now())
    }

    /// Same as [`Self::on_snapshot`] with an injected clock, so the throttling
    /// and the rate average can be tested without sleeping.
    pub fn on_snapshot_at(&mut self, snapshot: &TaskSnapshot, now: Instant) -> Option<String> {
        if snapshot.id != self.task || self.mode == ProgressMode::Silent || self.done {
            return None;
        }
        let percent = percent_of(snapshot.completed, snapshot.total);
        let rate = self.sample_rate(snapshot.completed, now);
        let finished = is_finished(snapshot.state);

        let event = if !self.started {
            self.started = true;
            Some(Event::Start)
        } else if finished {
            Some(Event::Done)
        } else if self.mode == ProgressMode::Bar || self.should_emit(percent, now) {
            Some(Event::Progress)
        } else {
            None
        };
        let event = event?;
        if event == Event::Done {
            self.done = true;
        }
        self.last_percent = percent;
        self.last_emit = Some(now);
        let mut line = self.render(snapshot, percent, rate, event);
        if self.mode == ProgressMode::Bar {
            // A shorter line would leave the tail of the previous one visible.
            if line.len() < self.last_width {
                line.push_str(&" ".repeat(self.last_width - line.len()));
            }
            self.last_width = line.len();
        }
        Some(line)
    }

    fn should_emit(&mut self, percent: Option<u32>, now: Instant) -> bool {
        let step = match (percent, self.last_percent) {
            (Some(current), Some(previous)) => current.saturating_sub(previous),
            // A task without a total only moves in bytes; fall back to time.
            _ => 0,
        };
        if step >= LINE_STEP {
            return true;
        }
        self.last_emit
            .is_none_or(|last| now.duration_since(last) >= LINE_INTERVAL)
    }

    fn sample_rate(&mut self, completed: u64, now: Instant) -> Option<f64> {
        let instant = match self.last_sample {
            Some((at, bytes)) => {
                let elapsed = now.duration_since(at).as_secs_f64();
                if elapsed < RATE_WINDOW.as_secs_f64() {
                    return self.rate;
                }
                let delta = completed.saturating_sub(bytes) as f64;
                Some(delta / elapsed)
            }
            None => None,
        };
        self.last_sample = Some((now, completed));
        match (instant, self.rate) {
            (Some(current), Some(previous)) => {
                self.rate = Some(RATE_ALPHA * current + (1.0 - RATE_ALPHA) * previous);
            }
            (Some(current), None) => self.rate = Some(current),
            (None, existing) => self.rate = existing,
        }
        self.rate
    }

    fn render(
        &self,
        snapshot: &TaskSnapshot,
        percent: Option<u32>,
        rate: Option<f64>,
        event: Event,
    ) -> String {
        let line = match self.mode {
            ProgressMode::Ndjson => self.render_ndjson(snapshot, percent, rate, event),
            ProgressMode::Bar => format!("\r{}  {}", bar(percent), detail(snapshot, percent, rate)),
            ProgressMode::Line | ProgressMode::Silent => {
                let head = match event {
                    Event::Start => format!("{}: {}", self.label, snapshot.label),
                    Event::Done => format!("{}: {}", self.label, state_word(snapshot.state)),
                    Event::Progress => String::new(),
                };
                let body = detail(snapshot, percent, rate);
                if head.is_empty() {
                    body
                } else if event == Event::Start {
                    head
                } else {
                    format!("{head}  {body}")
                }
            }
        };
        // The writer puts this string on stderr verbatim. A bar rewrites one
        // line with `\r`, but a line or an NDJSON event has to end here —
        // without it, thirty events become one unparseable line.
        if self.mode == ProgressMode::Bar {
            line
        } else {
            format!("{line}\n")
        }
    }

    fn render_ndjson(
        &self,
        snapshot: &TaskSnapshot,
        percent: Option<u32>,
        rate: Option<f64>,
        event: Event,
    ) -> String {
        let mut object = serde_json::Map::new();
        object.insert("event".into(), event.as_str().into());
        object.insert("id".into(), snapshot.id.to_string().into());
        object.insert("label".into(), snapshot.label.clone().into());
        object.insert("state".into(), state_slug(snapshot.state).into());
        object.insert("done".into(), snapshot.completed.into());
        object.insert(
            "total".into(),
            snapshot
                .total
                .map_or(serde_json::Value::Null, serde_json::Value::from),
        );
        if let Some(percent) = percent {
            object.insert("percent".into(), percent.into());
        }
        if let Some(rate) = rate.filter(|rate| *rate > 0.0) {
            object.insert("rate".into(), (rate.round() as u64).into());
        }
        serde_json::Value::Object(object).to_string()
    }
}

/// One launch-stage message, in the shape the caller asked for.
///
/// `launch` prepares a command rather than downloading a task, so it does not
/// have a `TaskSnapshot` to feed the renderer. The stage line still has to obey
/// the same contract: nothing under `--quiet`, NDJSON under `--json`, human text
/// otherwise. Returns `None` when nothing should be printed.
pub fn stage_line(
    mode: ProgressMode,
    stage: crate::core::launch::session::LaunchStage,
) -> Option<String> {
    match mode {
        ProgressMode::Silent => None,
        ProgressMode::Ndjson => Some(
            serde_json::json!({
                "event": "stage",
                "stage": stage_slug(stage),
                "label": stage_label(stage),
            })
            .to_string(),
        ),
        ProgressMode::Bar | ProgressMode::Line => Some(stage_label(stage).to_owned()),
    }
}

/// Stable machine name for a stage, independent of the display language.
pub fn stage_slug(stage: crate::core::launch::session::LaunchStage) -> &'static str {
    use crate::core::launch::session::LaunchStage;
    match stage {
        LaunchStage::ResolvingAccount => "resolving_account",
        LaunchStage::SelectingJava => "selecting_java",
        LaunchStage::DownloadingJava { .. } => "downloading_java",
        LaunchStage::PreparingNatives => "preparing_natives",
        LaunchStage::BuildingCommand => "building_command",
        LaunchStage::VerifyingFiles => "verifying_files",
    }
}

/// Human label for a stage. The CLI is English-only (design §13.2).
pub fn stage_label(stage: crate::core::launch::session::LaunchStage) -> &'static str {
    use crate::core::launch::session::LaunchStage;
    match stage {
        LaunchStage::ResolvingAccount => "resolving account",
        LaunchStage::SelectingJava => "selecting Java",
        LaunchStage::DownloadingJava { .. } => "downloading Java runtime",
        LaunchStage::PreparingNatives => "preparing natives",
        LaunchStage::BuildingCommand => "building command line",
        LaunchStage::VerifyingFiles => "verifying files",
    }
}

/// Follows one task until it finishes, rendering every snapshot it reports.
///
/// Only tasks this process submitted are observable: the snapshots travel over
/// an in-process `watch` channel. A task started by another process can only be
/// read back from disk, which is what `task watch` falls back to.
pub struct Watch {
    pub task: Uuid,
}

impl Watch {
    pub fn new(task: Uuid) -> Self {
        Self { task }
    }

    pub async fn run<F>(
        self,
        engine: &crate::core::task::TaskEngine,
        renderer: &mut ProgressRenderer,
        mut write: F,
    ) -> crate::core::error::Result<crate::core::task::TaskOutcome>
    where
        F: FnMut(&str),
    {
        let mut snapshots = engine.subscribe();
        let outcome = {
            let wait = engine.wait(self.task);
            tokio::pin!(wait);
            loop {
                tokio::select! {
                    outcome = &mut wait => break outcome,
                    changed = snapshots.changed() => {
                        if changed.is_err() {
                            break engine.wait(self.task).await;
                        }
                        // Clone out of the watch guard before touching anything
                        // else: holding it across an engine call deadlocks.
                        let list = snapshots.borrow_and_update().clone();
                        if let Some(snapshot) = list.iter().find(|item| item.id == self.task)
                            && let Some(line) = renderer.on_snapshot(snapshot)
                        {
                            write(&line);
                        }
                    }
                }
            }
        };
        if let Some(snapshot) = engine
            .snapshots()
            .into_iter()
            .find(|item| item.id == self.task)
            && let Some(line) = renderer.on_snapshot(&snapshot)
        {
            write(&line);
        }
        if renderer.mode() == ProgressMode::Bar {
            write("\n");
        }
        outcome
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Event {
    Start,
    Progress,
    Done,
}

impl Event {
    fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Progress => "progress",
            Self::Done => "done",
        }
    }
}

fn is_finished(state: TaskState) -> bool {
    matches!(
        state,
        TaskState::Succeeded | TaskState::Failed | TaskState::Cancelled
    )
}

fn state_slug(state: TaskState) -> &'static str {
    match state {
        TaskState::Queued => "queued",
        TaskState::Running => "running",
        TaskState::Paused => "paused",
        TaskState::Succeeded => "succeeded",
        TaskState::Failed => "failed",
        TaskState::Cancelled => "cancelled",
    }
}

fn state_word(state: TaskState) -> &'static str {
    match state {
        TaskState::Succeeded => "done",
        TaskState::Failed => "failed",
        TaskState::Cancelled => "cancelled",
        TaskState::Queued => "queued",
        TaskState::Running => "running",
        TaskState::Paused => "paused",
    }
}

/// Percentage complete, clamped to 100. `None` when the total is unknown, which
/// is the only honest answer for a task that reports bytes without a size.
fn percent_of(completed: u64, total: Option<u64>) -> Option<u32> {
    match total {
        Some(total) if total > 0 => Some(((completed.min(total) * 100) / total) as u32),
        _ => None,
    }
}

fn bar(percent: Option<u32>) -> String {
    let filled = percent.map_or(0, |percent| {
        (percent as usize * BAR_WIDTH / 100).min(BAR_WIDTH)
    });
    format!("[{}{}]", "#".repeat(filled), "-".repeat(BAR_WIDTH - filled))
}

fn detail(snapshot: &TaskSnapshot, percent: Option<u32>, rate: Option<f64>) -> String {
    let mut parts = Vec::new();
    if let Some(percent) = percent {
        parts.push(format!("{percent:>3}%"));
    }
    match snapshot.total {
        Some(total) => parts.push(format!(
            "{}/{}",
            bytes(snapshot.completed as f64),
            bytes(total as f64)
        )),
        None => parts.push(bytes(snapshot.completed as f64)),
    }
    if let Some(rate) = rate.filter(|rate| *rate > 0.0) {
        parts.push(format!("{}/s", bytes(rate)));
        if let (Some(total), Some(percent)) = (snapshot.total, percent)
            && percent < 100
        {
            let remaining = (total.saturating_sub(snapshot.completed)) as f64 / rate;
            parts.push(duration(remaining));
        }
    }
    parts.join("  ")
}

fn bytes(value: f64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = value;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", value.round() as u64, UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn duration(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    let (hours, minutes, seconds) = (total / 3600, (total % 3600) / 60, total % 60);
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use uuid::Uuid;

    use super::{ProgressMode, ProgressRenderer, percent_of, stage_line, stage_slug};
    use crate::core::task::{TaskOperationType, TaskSnapshot, TaskState};

    fn snapshot(id: Uuid, completed: u64, total: Option<u64>, state: TaskState) -> TaskSnapshot {
        TaskSnapshot {
            id,
            job: None,
            created_seq: 1,
            label: "Installing assets".into(),
            operation_type: TaskOperationType::InstanceInstall,
            state,
            retryable: false,
            attempt: 1,
            completed,
            total,
            failure: None,
        }
    }

    #[test]
    fn mode_selection_prefers_silence_then_json() {
        assert_eq!(
            ProgressMode::choose(false, true, true),
            ProgressMode::Silent
        );
        assert_eq!(ProgressMode::choose(true, true, true), ProgressMode::Silent);
        assert_eq!(
            ProgressMode::choose(true, false, true),
            ProgressMode::Ndjson
        );
        assert_eq!(ProgressMode::choose(false, false, true), ProgressMode::Bar);
        assert_eq!(
            ProgressMode::choose(false, false, false),
            ProgressMode::Line
        );
    }

    #[test]
    fn percentage_is_clamped_and_absent_without_a_total() {
        assert_eq!(percent_of(0, Some(100)), Some(0));
        assert_eq!(percent_of(50, Some(100)), Some(50));
        assert_eq!(percent_of(120, Some(100)), Some(100));
        assert_eq!(percent_of(10, None), None);
        assert_eq!(percent_of(10, Some(0)), None);
    }

    #[test]
    fn bar_mode_rewrites_one_line_and_never_shows_a_raw_percentage_over_100() {
        let id = Uuid::new_v4();
        let mut renderer = ProgressRenderer::new(ProgressMode::Bar, id, "install");
        let start = Instant::now();

        let first = renderer
            .on_snapshot_at(&snapshot(id, 0, Some(1000), TaskState::Running), start)
            .expect("start line");
        let second = renderer
            .on_snapshot_at(
                &snapshot(id, 2000, Some(1000), TaskState::Running),
                start + Duration::from_secs(1),
            )
            .expect("progress line");

        assert!(first.starts_with('\r'), "{first:?}");
        assert!(second.contains("100%"), "{second:?}");
        assert!(!second.contains("200%"), "{second:?}");
    }

    #[test]
    fn line_mode_throttles_to_five_percent_or_two_seconds() {
        let id = Uuid::new_v4();
        let mut renderer = ProgressRenderer::new(ProgressMode::Line, id, "install");
        let start = Instant::now();
        renderer.on_snapshot_at(&snapshot(id, 0, Some(100), TaskState::Running), start);

        // 1% and 0.5s later: too small a step, too little time.
        assert!(
            renderer
                .on_snapshot_at(
                    &snapshot(id, 1, Some(100), TaskState::Running),
                    start + Duration::from_millis(500)
                )
                .is_none()
        );
        // 6%: the step alone is enough.
        assert!(
            renderer
                .on_snapshot_at(
                    &snapshot(id, 6, Some(100), TaskState::Running),
                    start + Duration::from_millis(600)
                )
                .is_some()
        );
        // Back to a 1% step, but more than two seconds have passed.
        assert!(
            renderer
                .on_snapshot_at(
                    &snapshot(id, 7, Some(100), TaskState::Running),
                    start + Duration::from_secs(3)
                )
                .is_some()
        );
    }

    #[test]
    fn line_mode_never_emits_a_carriage_return() {
        let id = Uuid::new_v4();
        let mut renderer = ProgressRenderer::new(ProgressMode::Line, id, "install");
        let start = Instant::now();
        let mut lines = Vec::new();
        for step in [0, 10, 50, 100] {
            if let Some(line) = renderer.on_snapshot_at(
                &snapshot(id, step, Some(100), TaskState::Running),
                start + Duration::from_secs(step),
            ) {
                lines.push(line);
            }
        }
        assert!(!lines.is_empty());
        assert!(lines.iter().all(|line| !line.contains('\r')), "{lines:?}");
        // One event per line: the writer does not add a separator, so each
        // rendered event has to end with a newline of its own.
        assert!(
            lines.iter().all(|line| line.ends_with('\n')),
            "every line must be terminated: {lines:?}"
        );
        let joined = lines.concat();
        assert_eq!(joined.lines().count(), lines.len(), "{joined:?}");
    }

    #[test]
    fn ndjson_mode_emits_one_object_per_line() {
        let id = Uuid::new_v4();
        let mut renderer = ProgressRenderer::new(ProgressMode::Ndjson, id, "install");
        let start = Instant::now();
        let mut events = Vec::new();
        for (done, state, at) in [
            (10, TaskState::Running, 0),
            (60, TaskState::Running, 2),
            (100, TaskState::Succeeded, 4),
        ] {
            if let Some(line) = renderer.on_snapshot_at(
                &snapshot(id, done, Some(100), state),
                start + Duration::from_secs(at),
            ) {
                events.push(line);
            }
        }

        let joined = events.concat();
        let lines = joined.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), events.len(), "one event per line: {joined:?}");
        let values = lines
            .iter()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("valid json"))
            .collect::<Vec<_>>();
        assert_eq!(values[0]["event"], "start");
        assert_eq!(values[0]["done"], 10);
        assert_eq!(values[0]["total"], 100);
        assert_eq!(values[2]["event"], "done");
        assert_eq!(values[2]["state"], "succeeded");
        assert!(events.iter().all(|line| !line.contains('\r')));
    }

    #[test]
    fn a_finished_task_reports_its_state() {
        let id = Uuid::new_v4();
        let mut renderer = ProgressRenderer::new(ProgressMode::Ndjson, id, "install");
        let start = Instant::now();
        renderer.on_snapshot_at(&snapshot(id, 0, Some(10), TaskState::Running), start);

        let done = renderer
            .on_snapshot_at(
                &snapshot(id, 10, Some(10), TaskState::Succeeded),
                start + Duration::from_secs(1),
            )
            .expect("done event");

        let value: serde_json::Value = serde_json::from_str(&done).expect("valid json");
        assert_eq!(value["event"], "done");
        assert_eq!(value["state"], "succeeded");
    }

    #[test]
    fn silent_mode_says_nothing() {
        let id = Uuid::new_v4();
        let mut renderer = ProgressRenderer::new(ProgressMode::Silent, id, "install");
        assert!(
            renderer
                .on_snapshot(&snapshot(id, 50, Some(100), TaskState::Running))
                .is_none()
        );
    }

    #[test]
    fn snapshots_of_other_tasks_are_ignored() {
        let mine = Uuid::new_v4();
        let other = Uuid::new_v4();
        let mut renderer = ProgressRenderer::new(ProgressMode::Bar, mine, "install");
        assert!(
            renderer
                .on_snapshot(&snapshot(other, 50, Some(100), TaskState::Running))
                .is_none()
        );
    }

    #[test]
    fn the_rate_average_moves_towards_new_samples() {
        let id = Uuid::new_v4();
        let mut renderer = ProgressRenderer::new(ProgressMode::Ndjson, id, "install");
        let start = Instant::now();

        renderer.on_snapshot_at(&snapshot(id, 0, Some(10_000), TaskState::Running), start);
        let steady = renderer
            .on_snapshot_at(
                &snapshot(id, 1000, Some(10_000), TaskState::Running),
                start + Duration::from_secs(1),
            )
            .expect("a line after one second");
        let steady: serde_json::Value = serde_json::from_str(&steady).expect("json");
        assert_eq!(steady["rate"], 1000);

        let spiked = renderer
            .on_snapshot_at(
                &snapshot(id, 5001, Some(10_000), TaskState::Running),
                start + Duration::from_secs(2),
            )
            .expect("a line after the spike");
        let spiked: serde_json::Value = serde_json::from_str(&spiked).expect("json");
        // 0.3 * 4001 + 0.7 * 1000 ≈ 1900 rather than jumping to 4001.
        let rate = spiked["rate"].as_u64().expect("rate");
        assert!((1500..=2100).contains(&rate), "{rate}");
    }

    #[test]
    fn a_stage_line_obeys_the_same_mode_contract_as_progress() {
        use crate::core::launch::session::LaunchStage;

        assert_eq!(
            stage_line(ProgressMode::Silent, LaunchStage::SelectingJava),
            None
        );

        let plain = stage_line(ProgressMode::Line, LaunchStage::PreparingNatives)
            .expect("a line in line mode");
        assert_eq!(plain, "preparing natives");
        assert!(
            !plain.contains('{'),
            "plain mode must not emit JSON: {plain}"
        );

        let event = stage_line(ProgressMode::Ndjson, LaunchStage::SelectingJava)
            .expect("an event in ndjson mode");
        let parsed: serde_json::Value = serde_json::from_str(&event).expect("valid json");
        assert_eq!(parsed["event"], "stage");
        assert_eq!(parsed["stage"], "selecting_java");
        assert_eq!(parsed["label"], "selecting Java");
    }

    #[test]
    fn every_stage_has_a_stable_slug() {
        use crate::core::launch::session::LaunchStage;

        let stages = [
            (LaunchStage::ResolvingAccount, "resolving_account"),
            (LaunchStage::SelectingJava, "selecting_java"),
            (
                LaunchStage::DownloadingJava { major: 21 },
                "downloading_java",
            ),
            (LaunchStage::PreparingNatives, "preparing_natives"),
            (LaunchStage::BuildingCommand, "building_command"),
            (LaunchStage::VerifyingFiles, "verifying_files"),
        ];
        for (stage, slug) in stages {
            assert_eq!(stage_slug(stage), slug);
        }
    }
}
