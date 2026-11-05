use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio::sync::{Semaphore, oneshot, watch};
use uuid::Uuid;

use crate::core::error::{Error, Result};

use super::engine_support::{
    cleanup_owned, lock_state, now, outcome_from_record, publish_locked, snapshots_from,
};
use super::model::{
    ArtifactLedger, TaskAction, TaskOperation, TaskOutcome, TaskRecord, TaskSnapshot, TaskState,
};
use super::runner::TaskRunner;
use super::store::JsonTaskStore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteResult {
    pub action: TaskAction,
    pub cleanup_pending: bool,
    pub cleanup_errors: Vec<String>,
}

#[derive(Clone)]
pub struct TaskEngine {
    pub(crate) inner: Arc<EngineInner>,
}

pub(crate) struct EngineInner {
    pub(crate) store: JsonTaskStore,
    pub(crate) runner: Arc<dyn TaskRunner>,
    pub(crate) state: Mutex<EngineState>,
    pub(crate) snapshots: watch::Sender<Vec<TaskSnapshot>>,
    pub(crate) semaphore: Arc<Semaphore>,
    pub(crate) resource_locks: Mutex<HashMap<ResourceKey, Arc<tokio::sync::Mutex<()>>>>,
    /// Next submission sequence number; see [`TaskRecord::created_seq`].
    pub(crate) next_seq: AtomicU64,
}

pub(crate) struct EngineState {
    pub(crate) records: HashMap<Uuid, TaskRecord>,
    pub(crate) active: HashMap<Uuid, ActiveTask>,
    pub(crate) tombstones: HashMap<RunKey, Tombstone>,
    pub(crate) waiters: HashMap<Uuid, Vec<oneshot::Sender<TaskOutcome>>>,
    pub(crate) checkpoints: HashMap<RunKey, Instant>,
    pub(crate) progress_publications: HashMap<RunKey, Instant>,
}

#[derive(Clone)]
pub(crate) struct ActiveTask {
    pub(crate) generation: u64,
    pub(crate) cancelled: Arc<AtomicBool>,
    pub(crate) paused: watch::Sender<bool>,
}

pub(crate) struct Tombstone {
    pub(crate) cancelled: Arc<AtomicBool>,
    pub(crate) artifacts: ArtifactLedger,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RunKey {
    pub(crate) id: Uuid,
    pub(crate) generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum ResourceKey {
    Instance(Uuid),
    Java(PathBuf),
    Path(PathBuf),
}

impl TaskEngine {
    pub fn open(
        store: JsonTaskStore,
        runner: Arc<dyn TaskRunner>,
        max_concurrency: usize,
    ) -> Result<Self> {
        let mut records = HashMap::new();
        for mut record in store.load()? {
            // Only an abandoned task is interrupted. A record owned by a live
            // process belongs to another launcher that is still working on it,
            // and marking it failed here would corrupt that task's state.
            if record.state.is_active() && !record.owner_is_alive() && record.mark_interrupted() {
                store.upsert(&record)?;
            }
            records.insert(record.id, record);
        }
        // Tasks restored from disk keep their order; new ones go after them.
        let next_seq = records
            .values()
            .map(|record| record.created_seq)
            .max()
            .map_or(1, |seq| seq.saturating_add(1));
        let snapshots = snapshots_from(&records);
        let (snapshot_sender, _) = watch::channel(snapshots);
        Ok(Self {
            inner: Arc::new(EngineInner {
                store,
                runner,
                state: Mutex::new(EngineState {
                    records,
                    active: HashMap::new(),
                    tombstones: HashMap::new(),
                    waiters: HashMap::new(),
                    checkpoints: HashMap::new(),
                    progress_publications: HashMap::new(),
                }),
                snapshots: snapshot_sender,
                semaphore: Arc::new(Semaphore::new(max_concurrency.max(1))),
                resource_locks: Mutex::new(HashMap::new()),
                next_seq: AtomicU64::new(next_seq),
            }),
        })
    }

    pub fn submit(&self, operation: TaskOperation) -> Result<Uuid> {
        self.submit_with_job(operation, None)
    }

    /// Submits a task that belongs to a multi-step job. The job id is what the
    /// task list groups on; it never changes the task's own lifecycle.
    pub fn submit_with_job(&self, operation: TaskOperation, job: Option<Uuid>) -> Result<Uuid> {
        self.submit_with_job_after(operation, job, None)
    }

    /// Submits a task that must not start until `after` has succeeded.
    ///
    /// The task is written to disk immediately - so a restart still knows
    /// about it - but its worker parks until the predecessor reports a
    /// terminal state. That keeps two steps of one install from writing into
    /// the same instance directory at the same time.
    pub fn submit_with_job_after(
        &self,
        operation: TaskOperation,
        job: Option<Uuid>,
        after: Option<Uuid>,
    ) -> Result<Uuid> {
        if matches!(operation, TaskOperation::Unrecoverable { .. }) {
            return Err(Error::Download(
                "unrecoverable migrated tasks cannot be submitted".into(),
            ));
        }

        let id = Uuid::new_v4();
        let seq = self.inner.next_seq.fetch_add(1, Ordering::Relaxed);
        let mut record = TaskRecord::new(id, operation, now())
            .with_job(job)
            .with_seq(seq)
            .with_dependency(after);
        record.total = record.operation.planned_total();
        self.inner.store.upsert(&record)?;

        let (paused, _) = watch::channel(false);
        let active = ActiveTask {
            generation: 0,
            cancelled: Arc::new(AtomicBool::new(false)),
            paused,
        };
        {
            let mut state = lock_state(&self.inner.state)?;
            state.records.insert(id, record);
            state.active.insert(id, active);
            publish_locked(&self.inner, &state);
        }
        crate::core::diagnostics::diagnostic!("task", "submitted {id} (job {job:?})");
        self.spawn_worker(id, 0);
        Ok(id)
    }

    pub fn snapshots(&self) -> Vec<TaskSnapshot> {
        self.inner.snapshots.borrow().iter().cloned().collect()
    }

    /// Subscribes to task snapshots.
    ///
    /// The channel value sits behind a read/write lock: a `Ref` from
    /// `Receiver::borrow` must never be alive while calling back into the
    /// engine (`operation`, `snapshots`, `submit`, ...). Publishers hold
    /// [`EngineInner::state`] while they take the channel's write lock, so a
    /// caller that keeps a borrow and then asks for `state` deadlocks both
    /// sides. Clone the value out first, as the TUI loop does.
    pub fn subscribe(&self) -> watch::Receiver<Vec<TaskSnapshot>> {
        self.inner.snapshots.subscribe()
    }

    /// Whether this engine is the one running the task.
    ///
    /// Snapshots of a task started by another process exist (they are read from
    /// disk) but never update: only the owning process receives progress
    /// reports, so a viewer must not wait on one it does not own.
    pub fn is_locally_running(&self, id: Uuid) -> bool {
        lock_state(&self.inner.state).is_ok_and(|state| state.active.contains_key(&id))
    }

    pub fn operation(&self, id: Uuid) -> Option<TaskOperation> {
        self.inner
            .state
            .lock()
            .ok()?
            .records
            .get(&id)
            .map(|record| record.operation.clone())
    }

    pub fn updated_at(&self, id: Uuid) -> Option<u64> {
        self.inner
            .state
            .lock()
            .ok()?
            .records
            .get(&id)
            .map(|record| record.updated_at)
    }

    pub async fn pause_or_resume(&self, id: Uuid) -> Result<TaskAction> {
        self.pause_or_resume_now(id)
    }

    pub fn pause_or_resume_now(&self, id: Uuid) -> Result<TaskAction> {
        let mut spawn = None;
        let action;
        {
            let mut state = lock_state(&self.inner.state)?;
            let current_state = state
                .records
                .get(&id)
                .map(|record| record.state)
                .ok_or_else(|| Error::Download(format!("task not found: {id}")))?;
            match current_state {
                TaskState::Failed | TaskState::Cancelled => {
                    let (task_action, generation) = {
                        let record = state
                            .records
                            .get_mut(&id)
                            .ok_or_else(|| Error::Download(format!("task not found: {id}")))?;
                        let task_action = record.prepare_retry()?;
                        let generation = record.attempt as u64 + 1;
                        self.inner.store.upsert(record)?;
                        (task_action, generation)
                    };
                    action = task_action;
                    let (paused, _) = watch::channel(false);
                    state.active.insert(
                        id,
                        ActiveTask {
                            generation,
                            cancelled: Arc::new(AtomicBool::new(false)),
                            paused,
                        },
                    );
                    publish_locked(&self.inner, &state);
                    spawn = Some(generation);
                }
                TaskState::Queued | TaskState::Running | TaskState::Paused => {
                    let active =
                        state.active.get(&id).cloned().ok_or_else(|| {
                            Error::Download(format!("task control not found: {id}"))
                        })?;
                    let (task_action, paused) = {
                        let record = state
                            .records
                            .get_mut(&id)
                            .ok_or_else(|| Error::Download(format!("task not found: {id}")))?;
                        let task_action = record.pause_or_resume()?;
                        let paused = record.state == TaskState::Paused;
                        self.inner.store.upsert(record)?;
                        (task_action, paused)
                    };
                    action = task_action;
                    active
                        .paused
                        .send(paused)
                        .map_err(|_| Error::Download("task pause control closed".into()))?;
                    publish_locked(&self.inner, &state);
                }
                TaskState::Succeeded => {
                    return Err(Error::Download(
                        "succeeded task cannot be paused or retried".into(),
                    ));
                }
            }
        }
        if let Some(generation) = spawn {
            self.spawn_worker(id, generation);
        }
        Ok(action)
    }

    pub async fn retry(&self, id: Uuid) -> Result<TaskAction> {
        self.retry_now(id)
    }

    pub fn retry_now(&self, id: Uuid) -> Result<TaskAction> {
        let can_retry = {
            let state = lock_state(&self.inner.state)?;
            let record = state
                .records
                .get(&id)
                .ok_or_else(|| Error::Download(format!("task not found: {id}")))?;
            matches!(record.state, TaskState::Failed | TaskState::Cancelled) && record.retryable
        };
        if !can_retry {
            return Err(Error::Download(
                "only retryable failed or cancelled tasks can be retried".into(),
            ));
        }
        self.pause_or_resume_now(id)
    }

    pub async fn delete(&self, id: Uuid) -> Result<DeleteResult> {
        self.delete_now(id)
    }

    pub fn delete_now(&self, id: Uuid) -> Result<DeleteResult> {
        let (active, record, waiters) = {
            let mut state = lock_state(&self.inner.state)?;
            self.inner.store.remove(id)?;
            let record = state
                .records
                .remove(&id)
                .ok_or_else(|| Error::Download(format!("task not found: {id}")))?;
            let active = state.active.remove(&id);
            let waiters = state.waiters.remove(&id).unwrap_or_default();
            if let Some(active_task) = active.as_ref() {
                let key = RunKey {
                    id,
                    generation: active_task.generation,
                };
                state.tombstones.insert(
                    key,
                    Tombstone {
                        cancelled: active_task.cancelled.clone(),
                        artifacts: record.artifacts.clone(),
                    },
                );
                active_task.cancelled.store(true, Ordering::Release);
                let _ = active_task.paused.send(false);
            }
            publish_locked(&self.inner, &state);
            (active, record, waiters)
        };

        for waiter in waiters {
            let _ = waiter.send(TaskOutcome::Cancelled {
                message: "task deleted".into(),
                retryable: false,
            });
        }

        if active.is_some() {
            return Ok(DeleteResult {
                action: TaskAction::Deleted,
                cleanup_pending: true,
                cleanup_errors: Vec::new(),
            });
        }

        let cleanup_errors = if record.state == TaskState::Succeeded {
            Vec::new()
        } else {
            cleanup_owned(&record.artifacts.owned_temporary)
        };
        Ok(DeleteResult {
            action: TaskAction::Deleted,
            cleanup_pending: false,
            cleanup_errors,
        })
    }

    pub async fn wait(&self, id: Uuid) -> Result<TaskOutcome> {
        let receiver = {
            let mut state = lock_state(&self.inner.state)?;
            let record = state
                .records
                .get(&id)
                .ok_or_else(|| Error::Download(format!("task not found: {id}")))?;
            if record.state.is_terminal() {
                return Ok(outcome_from_record(record));
            }
            let (sender, receiver) = oneshot::channel();
            state.waiters.entry(id).or_default().push(sender);
            receiver
        };
        receiver
            .await
            .map_err(|_| Error::Download(format!("task ended without an outcome: {id}")))
    }
}
