//! Tasks page.

use std::collections::HashMap;
use std::time::Instant;

use super::*;

use crate::core::task::{TaskOperationType, TaskSnapshot, TaskState};

/// One row of the task list.
///
/// A row stands for a whole job: one task, or the several tasks that share a
/// `job` id. Progress is summed over the members and the state is the most
/// severe one, so a three-step modpack install reads as one bar instead of
/// three unrelated lines.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::tui::app) struct TaskRow {
    /// Snapshot index of the step the row reports (its label and failure).
    pub head: usize,
    /// Every snapshot behind the row, in creation order.
    pub members: Vec<usize>,
    pub label: String,
    pub state: TaskState,
    pub completed: u64,
    pub total: Option<u64>,
    pub failure: Option<String>,
    pub retryable: bool,
    pub attempt: u32,
    pub operation_type: TaskOperationType,
    /// 1-based position of `head` inside the job.
    pub step: usize,
    pub steps: usize,
    /// Aggregate download speed of the running members, when one is known.
    pub bytes_per_second: Option<u64>,
}

impl TaskRow {
    fn new(snapshots: &[TaskSnapshot], members: Vec<usize>) -> Self {
        let first = members.first().copied().unwrap_or(0);
        let state = aggregate_state(snapshots, &members);
        // The row reports the step that is still moving; once everything has
        // finished it reports the last one, so the label is the end result.
        let head = members
            .iter()
            .copied()
            .find(|index| {
                snapshots
                    .get(*index)
                    .is_none_or(|snapshot| snapshot.state != TaskState::Succeeded)
            })
            .unwrap_or_else(|| members.last().copied().unwrap_or(first));
        let step = members
            .iter()
            .position(|index| *index == head)
            .map_or(1, |position| position + 1);
        let completed = members
            .iter()
            .filter_map(|index| snapshots.get(*index))
            .map(|snapshot| snapshot.completed)
            .sum();
        let all_known = members
            .iter()
            .filter_map(|index| snapshots.get(*index))
            .all(|snapshot| snapshot.total.is_some());
        let total = if all_known {
            Some(
                members
                    .iter()
                    .filter_map(|index| snapshots.get(*index))
                    .filter_map(|snapshot| snapshot.total)
                    .sum(),
            )
        } else if state == TaskState::Succeeded {
            // A finished job reads as full even when one step never reported a
            // total; a dashed bar on a completed install would look unfinished.
            Some(completed)
        } else {
            None
        };
        let reported = snapshots.get(head);
        // The loader step names the instance; the download step names an
        // archive. Prefer the former whenever the job has one.
        let label = members
            .iter()
            .filter_map(|index| snapshots.get(*index))
            .find(|snapshot| snapshot.operation_type == TaskOperationType::InstanceInstall)
            .or(reported)
            .map(|snapshot| snapshot.label.clone())
            .unwrap_or_default();
        Self {
            head,
            members: members.clone(),
            label,
            state,
            completed,
            total,
            failure: reported.and_then(|snapshot| snapshot.failure.clone()),
            retryable: members
                .iter()
                .filter_map(|index| snapshots.get(*index))
                .any(|snapshot| snapshot.retryable),
            attempt: reported.map_or(0, |snapshot| snapshot.attempt),
            operation_type: reported.map_or(TaskOperationType::Unrecoverable, |snapshot| {
                snapshot.operation_type
            }),
            step,
            steps: members.len(),
            bytes_per_second: None,
        }
    }

    /// `12.3 MB/s · 2m 14s` while the row is moving; nothing otherwise.
    pub(in crate::tui::app) fn transfer_label(&self) -> Option<String> {
        if self.state != TaskState::Running {
            return None;
        }
        let speed = self.bytes_per_second?;
        format_transfer(self.completed, self.total, speed)
    }
}

/// The most severe state of a job: a failure anywhere means the row failed,
/// a running step means it is running, and only an all-succeeded job is done.
fn aggregate_state(snapshots: &[TaskSnapshot], members: &[usize]) -> TaskState {
    let states: Vec<TaskState> = members
        .iter()
        .filter_map(|index| snapshots.get(*index))
        .map(|snapshot| snapshot.state)
        .collect();
    for (wanted, state) in [
        (TaskState::Failed, TaskState::Failed),
        (TaskState::Running, TaskState::Running),
        (TaskState::Paused, TaskState::Paused),
        (TaskState::Queued, TaskState::Queued),
        (TaskState::Cancelled, TaskState::Cancelled),
    ] {
        if states.contains(&state) {
            return wanted;
        }
    }
    TaskState::Succeeded
}

/// Samples per task, so the task list can show how fast the bytes arrive.
///
/// This lives in the UI on purpose: speed is presentation state, not task
/// state, and persisting it would put a clock into the task store. A sample
/// older than [`SPEED_STALE`] reports nothing rather than a speed from minutes
/// ago.
#[derive(Debug, Default)]
pub(in crate::tui::app) struct SpeedTracker {
    samples: HashMap<Uuid, SpeedSample>,
}

const SPEED_STALE: std::time::Duration = std::time::Duration::from_secs(3);
const SPEED_WINDOW: f64 = 0.2;

#[derive(Debug)]
struct SpeedSample {
    completed: u64,
    at: Instant,
    updated_at: Instant,
    bytes_per_second: f64,
}

impl SpeedTracker {
    pub(in crate::tui::app) fn observe(&mut self, id: Uuid, completed: u64, at: Instant) {
        let Some(sample) = self.samples.get_mut(&id) else {
            self.samples.insert(
                id,
                SpeedSample {
                    completed,
                    at,
                    updated_at: at,
                    bytes_per_second: 0.0,
                },
            );
            return;
        };
        // A retry restarts the counter; compare against a fresh baseline.
        if completed < sample.completed {
            *sample = SpeedSample {
                completed,
                at,
                updated_at: at,
                bytes_per_second: 0.0,
            };
            return;
        }
        let elapsed = at.saturating_duration_since(sample.at).as_secs_f64();
        if elapsed < SPEED_WINDOW {
            return;
        }
        let delta = completed.saturating_sub(sample.completed) as f64;
        let instant = delta / elapsed;
        sample.bytes_per_second = if sample.bytes_per_second <= f64::EPSILON {
            instant
        } else {
            // Exponentially weighted: one slow chunk must not collapse the
            // estimate, and one fast one must not spike it.
            sample.bytes_per_second * 0.7 + instant * 0.3
        };
        sample.completed = completed;
        sample.at = at;
        sample.updated_at = at;
    }

    pub(in crate::tui::app) fn speed(&self, id: Uuid, now: Instant) -> Option<u64> {
        let sample = self.samples.get(&id)?;
        if now.saturating_duration_since(sample.updated_at) > SPEED_STALE {
            return None;
        }
        (sample.bytes_per_second >= 1.0)
            .then(|| sample.bytes_per_second.round().clamp(0.0, u64::MAX as f64) as u64)
    }

    pub(in crate::tui::app) fn forget(&mut self, id: Uuid) {
        self.samples.remove(&id);
    }
}

impl App {
    /// Localized name of a task's operation kind, for the card's third line.
    ///
    /// The card falls back to the operation name when there is no failure
    /// message; printing `TaskOperationType` through `Debug` put `InstallMod`
    /// and `JavaRuntime` in the middle of a Chinese page.
    pub(in crate::tui::app) fn task_operation_label(
        &self,
        operation: crate::core::task::TaskOperationType,
    ) -> String {
        use crate::core::task::TaskOperationType;

        let key = match operation {
            TaskOperationType::InstanceInstall => "task.operation.instance_install",
            TaskOperationType::InstanceRepair => "task.operation.instance_repair",
            TaskOperationType::JavaRuntime => "task.operation.java_runtime",
            TaskOperationType::ResourceInstall => "task.operation.resource_install",
            TaskOperationType::ResourceSaveAs => "task.operation.resource_save_as",
            TaskOperationType::ModpackInstall => "task.operation.modpack_install",
            TaskOperationType::ModpackFiles => "task.operation.modpack_files",
            TaskOperationType::Unrecoverable => "task.operation.unrecoverable",
        };
        self.i18n.text(key)
    }

    pub(in crate::tui::app) fn tasks_hints(&self) -> Vec<crate::tui::chrome::Hint> {
        let action = match self.selected_task().map(|task| task.state) {
            Some(TaskState::Failed | TaskState::Cancelled) => "action.retry",
            Some(TaskState::Paused) => "action.resume",
            _ => "action.pause",
        };
        vec![
            self.hint("Enter", "action.details"),
            self.hint("p", action),
            self.hint("d", "action.delete"),
            self.hint("[ ]", "action.change_filter"),
        ]
    }

    pub(in crate::tui::app) fn task_key_state(&self) -> crate::tui::command::TaskKeyState {
        use crate::tui::command::TaskKeyState;
        match self.selected_task().map(|task| task.state) {
            Some(TaskState::Failed | TaskState::Cancelled) => TaskKeyState::Retry,
            Some(TaskState::Paused) => TaskKeyState::Resume,
            Some(TaskState::Queued | TaskState::Running) => TaskKeyState::Pause,
            _ => TaskKeyState::None,
        }
    }

    pub(in crate::tui::app) fn set_task_selection(&mut self, index: usize) {
        let visible = self.visible_task_indices();
        self.task_selected = index.min(visible.len().saturating_sub(1));
        self.task_selected_id = self
            .task_snapshots
            .get(
                visible
                    .get(self.task_selected)
                    .copied()
                    .unwrap_or(usize::MAX),
            )
            .map(|snapshot| snapshot.id);
    }

    pub(in crate::tui::app) fn set_task_selection_by_id(&mut self, id: Uuid) {
        if let Some(index) = self.task_rows().iter().position(|row| {
            row.members.iter().any(|member| {
                self.task_snapshots
                    .get(*member)
                    .is_some_and(|snapshot| snapshot.id == id)
            })
        }) {
            self.set_task_selection(index);
        }
    }

    /// One row per job. Tasks that share a `job` id — the download, the loader
    /// install and the file install of one modpack — collapse into a single row
    /// with aggregate progress, because listing them apart read as a pile of
    /// unrelated work.
    pub(in crate::tui::app) fn task_rows(&self) -> Vec<TaskRow> {
        self.all_task_rows()
            .into_iter()
            .filter(|row| self.task_filter.matches_state(row.state))
            .collect()
    }

    fn all_task_rows(&self) -> Vec<TaskRow> {
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut slots: std::collections::HashMap<Uuid, usize> = std::collections::HashMap::new();
        for (index, snapshot) in self.task_snapshots.iter().enumerate() {
            match snapshot.job {
                Some(job) => match slots.get(&job) {
                    Some(slot) => groups[*slot].push(index),
                    None => {
                        slots.insert(job, groups.len());
                        groups.push(vec![index]);
                    }
                },
                None => groups.push(vec![index]),
            }
        }
        let now = Instant::now();
        groups
            .into_iter()
            .map(|members| {
                let speed = members
                    .iter()
                    .filter_map(|index| self.task_snapshots.get(*index))
                    .filter(|snapshot| snapshot.state == TaskState::Running)
                    .filter_map(|snapshot| self.speed_tracker.speed(snapshot.id, now))
                    .reduce(u64::saturating_add);
                let mut row = TaskRow::new(&self.task_snapshots, members);
                row.bytes_per_second = speed;
                row
            })
            .collect()
    }

    /// Row index of the selected task, used by the keys that act on a job.
    pub(in crate::tui::app) fn selected_task_row(&self) -> Option<TaskRow> {
        self.task_rows().into_iter().find(|row| {
            self.task_snapshots
                .get(row.head)
                .is_some_and(|snapshot| Some(snapshot.id) == self.task_selected_id)
        })
    }

    pub(in crate::tui::app) fn task_filter_count(&self, filter: TaskFilter) -> usize {
        self.all_task_rows()
            .iter()
            .filter(|row| filter.matches_state(row.state))
            .count()
    }

    /// One snapshot index per row: selection, scrolling and tests all work in
    /// row space, and every row is represented by the step it reports.
    pub(in crate::tui::app) fn visible_task_indices(&self) -> Vec<usize> {
        self.task_rows().into_iter().map(|row| row.head).collect()
    }

    /// The step that speaks for the selected row.
    ///
    /// Selection lives in row space: the id remembers the row across refreshes,
    /// but the row's head decides what the keys mean. Reading the remembered
    /// snapshot instead would keep answering for a step that already finished.
    pub(in crate::tui::app) fn selected_task(&self) -> Option<&TaskSnapshot> {
        let rows = self.task_rows();
        let row = self
            .task_selected_id
            .and_then(|id| {
                rows.iter().find(|row| {
                    self.task_snapshots
                        .get(row.head)
                        .is_some_and(|snapshot| snapshot.id == id)
                })
            })
            .or_else(|| rows.get(self.task_selected))?;
        self.task_snapshots.get(row.head)
    }

    /// Every task behind the selected row. Keys that act on a job — pause,
    /// retry, delete — apply to all of them, so a job never loses a step.
    pub(in crate::tui::app) fn selected_job_tasks(&self) -> Vec<&TaskSnapshot> {
        self.selected_task_row()
            .map(|row| {
                row.members
                    .iter()
                    .filter_map(|index| self.task_snapshots.get(*index))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(in crate::tui::app) fn task_page_sections(area: Rect) -> [Rect; 2] {
        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(area);
        [sections[0], sections[1]]
    }

    pub(in crate::tui::app) fn task_tabs(&self) -> TabBar {
        TabBar::new(
            TaskFilter::ALL
                .iter()
                .map(|filter| {
                    let count = self.task_filter_count(*filter);
                    format!("{} ({count})", self.i18n.text(filter.label_key()))
                })
                .collect(),
        )
    }

    /// Labels of the `creeper` filter pills. Drawing and hit testing both use
    /// this list, so the chips and their click boxes cannot drift apart.
    pub(in crate::tui::app) fn creeper_task_filter_labels(&self) -> Vec<String> {
        TaskFilter::ALL
            .iter()
            .map(|filter| {
                let count = self.task_filter_count(*filter);
                format!("{} {count}", self.i18n.text(filter.label_key()))
            })
            .collect()
    }

    pub(in crate::tui::app) fn change_task_filter(&mut self, delta: isize) {
        let previous_id = self.task_selected_id;
        self.task_filter = self.task_filter.next(delta);
        let rows = self.task_rows();
        let selected = previous_id.and_then(|id| {
            rows.iter().position(|row| {
                self.task_snapshots
                    .get(row.head)
                    .is_some_and(|snapshot| snapshot.id == id)
            })
        });
        if let Some(selected) = selected {
            self.set_task_selection(selected);
        } else {
            self.set_task_selection(0);
        }
    }

    pub(in crate::tui::app) fn pause_selected_task(&mut self) {
        self.apply_job_action(|engine, id, state| match state {
            TaskState::Failed | TaskState::Cancelled => engine.retry_now(id),
            TaskState::Queued | TaskState::Running | TaskState::Paused => {
                engine.pause_or_resume_now(id)
            }
            TaskState::Succeeded => Err(Error::Download(
                "succeeded task cannot be paused or retried".into(),
            )),
        });
    }

    pub(in crate::tui::app) fn resume_selected_task(&mut self) {
        self.apply_job_action(|engine, id, state| {
            if state == TaskState::Paused {
                engine.pause_or_resume_now(id)
            } else {
                Err(Error::Download("task is not paused".into()))
            }
        });
    }

    pub(in crate::tui::app) fn retry_selected_task(&mut self) {
        self.apply_job_action(|engine, id, state| match state {
            TaskState::Failed | TaskState::Cancelled => engine.retry_now(id),
            _ => Err(Error::Download("task is not retryable".into())),
        });
    }

    /// Runs one action over every step of the selected row.
    ///
    /// A row can stand for a three-step modpack install, and pausing just the
    /// step that happens to be running would leave the job half stopped. Each
    /// step decides for itself what the action means (a queued step pauses, a
    /// failed one retries), and a step that cannot take it is skipped.
    fn apply_job_action(
        &mut self,
        action: impl Fn(
            &crate::core::task::TaskEngine,
            Uuid,
            TaskState,
        ) -> crate::core::error::Result<crate::core::task::TaskAction>,
    ) {
        let tasks: Vec<(Uuid, TaskState)> = self
            .selected_job_tasks()
            .iter()
            .map(|task| (task.id, task.state))
            .collect();
        if tasks.is_empty() {
            self.status = Status::info(self.i18n.text("status.no_tasks_action"));
            return;
        }
        let mut applied: Option<(Uuid, crate::core::task::TaskAction)> = None;
        let mut last_error = None;
        for (id, state) in tasks {
            match action(&self.task_engine, id, state) {
                Ok(action) => {
                    if applied.is_none() {
                        applied = Some((id, action));
                    }
                }
                Err(error) => last_error = Some(error),
            }
        }
        self.apply_task_snapshots(self.task_engine.snapshots());
        match applied {
            Some((id, action)) => {
                self.status = Status::info(self.i18n.format(
                    "status.task_action",
                    &[("id", &id.to_string()), ("action", &format!("{action:?}"))],
                ));
            }
            None => {
                if let Some(error) = last_error {
                    self.status = Status::error(self.i18n.format(
                        "status.task_action_failed",
                        &[("error", &error.to_string())],
                    ));
                }
            }
        }
    }

    /// Deletes the whole selected row: every step of the job. Leaving a step
    /// behind would keep a modpack's download alive after its install was
    /// deleted, or the other way round.
    pub(in crate::tui::app) fn delete_selected_task(&mut self) {
        let ids: Vec<Uuid> = self
            .selected_job_tasks()
            .iter()
            .map(|task| task.id)
            .collect();
        if ids.is_empty() {
            self.status = Status::info(self.i18n.text("status.no_tasks_delete"));
            return;
        }
        let mut deleted: Option<(Uuid, crate::core::task::DeleteResult)> = None;
        let mut last_error = None;
        for task_id in ids {
            let modpack_staging = match self.task_engine.operation(task_id) {
                Some(TaskOperation::ModpackInstall(spec)) => Some(spec.staging_path),
                _ => None,
            };
            match self.task_engine.delete_now(task_id) {
                Ok(result) => {
                    if let Some(staging) = modpack_staging {
                        let same_staging = matches!(
                            self.modal.as_ref(),
                            Some(Modal::Form { values, action: FormAction::InstallModpack, .. })
                                if values.first().is_some_and(|value| value == &staging.display().to_string())
                        );
                        let _ = std::fs::remove_file(&staging);
                        if same_staging {
                            self.modal = None;
                        }
                    }
                    deleted = Some((task_id, result));
                }
                Err(error) => last_error = Some(error),
            }
        }
        self.apply_task_snapshots(self.task_engine.snapshots());
        match deleted {
            Some((task_id, result)) => {
                let cleanup = if result.cleanup_pending {
                    self.i18n.text("task.cleanup_pending")
                } else if result.cleanup_errors.is_empty() {
                    self.i18n.text("task.cleanup_done")
                } else {
                    self.i18n.text("task.cleanup_partial")
                };
                let details = if result.cleanup_errors.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", result.cleanup_errors.join("; "))
                };
                self.status = Status::info(self.i18n.format(
                    "status.task_deleted",
                    &[
                        ("id", &task_id.to_string()),
                        ("cleanup", &cleanup),
                        ("details", &details),
                    ],
                ));
            }
            None => {
                if let Some(error) = last_error {
                    self.status = Status::error(self.i18n.format(
                        "status.task_delete_failed",
                        &[("error", &error.to_string())],
                    ));
                }
            }
        }
    }
}

pub(crate) fn command(
    _mode: crate::tui::command::Mode,
    key: crossterm::event::KeyEvent,
    task_action: crate::tui::command::TaskKeyState,
) -> crate::tui::command::Command {
    use crate::tui::command::{Command, TaskKeyState};
    use crossterm::event::KeyCode;

    let plain = key.modifiers.is_empty();
    match key.code {
        KeyCode::Char('p') if plain => match task_action {
            TaskKeyState::Retry => Command::Retry,
            TaskKeyState::Resume => Command::Resume,
            TaskKeyState::Pause | TaskKeyState::None => Command::Pause,
        },
        KeyCode::Char('d') if plain => Command::Delete,
        KeyCode::Char('[') | KeyCode::Char('h') | KeyCode::Left => Command::PrevBlock,
        KeyCode::Char(']') | KeyCode::Char('l') | KeyCode::Right => Command::NextBlock,
        _ => Command::Nothing,
    }
}
