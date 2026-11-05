use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use uuid::Uuid;

use crate::core::error::{Error, Result};

use super::engine::{EngineInner, EngineState};
use super::model::{TaskOutcome, TaskRecord, TaskSnapshot, TaskState};

pub(crate) fn lock_state(
    state: &Mutex<EngineState>,
) -> Result<std::sync::MutexGuard<'_, EngineState>> {
    state
        .lock()
        .map_err(|_| Error::Download("task state lock poisoned".into()))
}

pub(crate) fn publish_locked(inner: &EngineInner, state: &EngineState) {
    inner.snapshots.send_replace(snapshots_from(&state.records));
}

pub(crate) fn snapshots_from(records: &HashMap<Uuid, TaskRecord>) -> Vec<TaskSnapshot> {
    let mut snapshots = records
        .values()
        .map(TaskRecord::snapshot)
        .collect::<Vec<_>>();
    // Queue order, not UUID order: the three steps of one modpack install are
    // submitted within the same second, so only the sequence keeps them in the
    // order the user watched them start.
    snapshots.sort_by_key(|snapshot| (snapshot.created_seq, snapshot.id));
    snapshots
}

pub(crate) fn outcome_from_record(record: &TaskRecord) -> TaskOutcome {
    match record.state {
        TaskState::Succeeded => TaskOutcome::Succeeded,
        TaskState::Cancelled => TaskOutcome::Cancelled {
            message: record
                .failure
                .clone()
                .unwrap_or_else(|| "task cancelled".into()),
            retryable: record.retryable,
        },
        TaskState::Failed => TaskOutcome::Failed {
            message: record
                .failure
                .clone()
                .unwrap_or_else(|| "task failed".into()),
            retryable: record.retryable,
        },
        TaskState::Queued | TaskState::Running | TaskState::Paused => {
            unreachable!("non-terminal task passed to outcome_from_record")
        }
    }
}

pub(crate) fn cleanup_owned(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .filter_map(|path| match std::fs::remove_file(path) {
            Ok(()) => None,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => Some(format!("{}: {error}", path.display())),
        })
        .collect()
}

pub(crate) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}
