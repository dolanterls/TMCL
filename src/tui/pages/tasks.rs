use crate::core::task::TaskSnapshot;
use crate::i18n::I18n;
use ratatui::text::Line;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskFilter {
    All,
    Running,
    Queued,
    Done,
    Failed,
}

impl TaskFilter {
    pub const ALL: [Self; 5] = [
        Self::All,
        Self::Running,
        Self::Queued,
        Self::Done,
        Self::Failed,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Running => "Running",
            Self::Queued => "Queued",
            Self::Done => "Done",
            Self::Failed => "Failed",
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::All => "task.filter_all",
            Self::Running => "task.filter_running",
            Self::Queued => "task.filter_queued",
            Self::Done => "task.filter_done",
            Self::Failed => "task.filter_failed",
        }
    }

    pub fn matches(self, snapshot: &TaskSnapshot) -> bool {
        self.matches_state(snapshot.state)
    }

    /// The same rule, applied to the state a row reports. A row can stand for
    /// several tasks, so it is tested by its aggregate state rather than by any
    /// one snapshot's.
    pub fn matches_state(self, state: crate::core::task::TaskState) -> bool {
        use crate::core::task::TaskState;
        match self {
            Self::All => true,
            Self::Running => matches!(state, TaskState::Running | TaskState::Paused),
            Self::Queued => state == TaskState::Queued,
            Self::Done => state == TaskState::Succeeded,
            Self::Failed => matches!(state, TaskState::Failed | TaskState::Cancelled),
        }
    }

    pub fn next(self, delta: isize) -> Self {
        let current = Self::ALL
            .iter()
            .position(|filter| *filter == self)
            .unwrap_or(0);
        let next = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current
                .saturating_add(delta as usize)
                .min(Self::ALL.len() - 1)
        };
        Self::ALL[next]
    }

    pub fn indices(self, snapshots: &[TaskSnapshot]) -> Vec<usize> {
        snapshots
            .iter()
            .enumerate()
            .filter_map(|(index, snapshot)| self.matches(snapshot).then_some(index))
            .collect()
    }
}

pub struct TaskPage;

impl TaskPage {
    pub fn rows(snapshots: &[TaskSnapshot]) -> impl Iterator<Item = &TaskSnapshot> {
        snapshots.iter()
    }

    pub fn render(snapshots: &[TaskSnapshot], selected: usize, i18n: &I18n) -> Vec<Line<'static>> {
        if snapshots.is_empty() {
            return vec![Line::from(i18n.text("task.no_tasks"))];
        }

        std::iter::once(Line::from(i18n.text("task.tasks_hint")))
            .chain(snapshots.iter().enumerate().map(|(index, task)| {
                let marker = if index == selected { "> " } else { "  " };
                let failure = task
                    .failure
                    .as_deref()
                    .map_or(String::new(), |message| format!("  error: {message}"));
                Line::from(format!(
                    "{marker}{}  {}  {}/{}{}{}",
                    task.label,
                    task_state_label(task.state, i18n),
                    task.completed,
                    task.total.map_or("?".into(), |total| total.to_string()),
                    if task.retryable {
                        format!("  [{}]", i18n.text("task.retryable"))
                    } else {
                        String::new()
                    },
                    failure
                ))
            }))
            .collect()
    }
}

fn task_state_label(state: crate::core::task::TaskState, i18n: &I18n) -> String {
    let key = match state {
        crate::core::task::TaskState::Queued => "task.queued",
        crate::core::task::TaskState::Running => "task.running",
        crate::core::task::TaskState::Paused => "task.paused",
        crate::core::task::TaskState::Succeeded => "task.completed",
        crate::core::task::TaskState::Failed => "task.failed",
        crate::core::task::TaskState::Cancelled => "task.cancelled",
    };
    i18n.text(key)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::core::task::{JavaRuntimeSpec, TaskOperation, TaskRecord, TaskState};

    use super::{TaskFilter, TaskPage};

    #[test]
    fn renderer_consumes_only_latest_snapshots() {
        let task = TaskRecord::new(
            uuid::Uuid::new_v4(),
            TaskOperation::JavaRuntime(JavaRuntimeSpec {
                data_root: PathBuf::from("runtime"),
                major: 21,
                download_snapshot: Default::default(),
            }),
            1,
        );
        let snapshots = vec![task.snapshot()];
        assert_eq!(TaskPage::rows(&snapshots).count(), 1);
        assert!(
            TaskPage::render(
                &snapshots,
                0,
                &crate::i18n::I18n::new(crate::core::config::models::Language::English),
            )[1]
            .to_string()
            .contains("Install Mojang Java 21")
        );
        assert_eq!(snapshots[0].state, TaskState::Queued);
    }

    #[test]
    fn renderer_includes_failure_reason_for_retryable_tasks() {
        let mut task = TaskRecord::new(
            uuid::Uuid::new_v4(),
            TaskOperation::JavaRuntime(JavaRuntimeSpec {
                data_root: PathBuf::from("runtime"),
                major: 21,
                download_snapshot: Default::default(),
            }),
            1,
        );
        task.start().expect("start");
        task.fail("network timeout", true).expect("fail");
        let rendered = TaskPage::render(
            &[task.snapshot()],
            0,
            &crate::i18n::I18n::new(crate::core::config::models::Language::English),
        );

        assert!(rendered[1].to_string().contains("network timeout"));
    }

    #[test]
    fn task_filters_use_semantic_state_groups_and_preserve_source_indices() {
        let mut queued = TaskRecord::new(
            uuid::Uuid::new_v4(),
            TaskOperation::JavaRuntime(JavaRuntimeSpec {
                data_root: PathBuf::from("runtime"),
                major: 21,
                download_snapshot: Default::default(),
            }),
            1,
        );
        queued.start().expect("start");
        let mut failed = TaskRecord::new(
            uuid::Uuid::new_v4(),
            TaskOperation::JavaRuntime(JavaRuntimeSpec {
                data_root: PathBuf::from("runtime"),
                major: 17,
                download_snapshot: Default::default(),
            }),
            1,
        );
        failed.start().expect("start");
        failed.fail("failed", false).expect("fail");
        let snapshots = vec![queued.snapshot(), failed.snapshot()];

        assert_eq!(TaskFilter::Running.indices(&snapshots), vec![0]);
        assert_eq!(TaskFilter::Failed.indices(&snapshots), vec![1]);
        assert!(TaskFilter::Done.indices(&snapshots).is_empty());
    }
}
