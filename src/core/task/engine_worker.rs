use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use uuid::Uuid;

use crate::core::error::{Error, Result};

use super::context::{TaskContext, TaskContextSink};
use super::engine::{ActiveTask, EngineInner, ResourceKey, RunKey, TaskEngine};
use super::engine_support::{cleanup_owned, lock_state, publish_locked};
use super::model::{
    ArtifactLedger, InstanceInstallSpec, JavaRuntimeSpec, ResourceInstallSpec, ResourceSaveAsSpec,
    TaskOperation, TaskOutcome, TaskState,
};

const TASK_CHECKPOINT_INTERVAL: Duration = Duration::from_secs(1);
const TASK_SNAPSHOT_INTERVAL: Duration = Duration::from_millis(100);

fn resource_key(operation: &TaskOperation) -> Option<ResourceKey> {
    match operation {
        TaskOperation::InstanceInstall(InstanceInstallSpec { instance_id, .. })
        | TaskOperation::InstanceRepair(super::model::InstanceRepairSpec { instance_id, .. }) => {
            Some(ResourceKey::Instance(*instance_id))
        }
        TaskOperation::JavaRuntime(JavaRuntimeSpec { data_root, .. }) => {
            Some(ResourceKey::Java(data_root.clone()))
        }
        TaskOperation::ResourceInstall(ResourceInstallSpec { instance_id, .. }) => {
            Some(ResourceKey::Instance(*instance_id))
        }
        TaskOperation::ModpackInstall(spec) => {
            Some(ResourceKey::Path(normalize_lock_path(&spec.staging_path)))
        }
        TaskOperation::ModpackFiles(spec) => {
            Some(ResourceKey::Path(normalize_lock_path(&spec.instance_root)))
        }
        TaskOperation::ResourceSaveAs(ResourceSaveAsSpec { destination, .. }) => {
            Some(ResourceKey::Path(normalize_lock_path(destination)))
        }
        TaskOperation::Unrecoverable { .. } => None,
    }
}

fn normalize_lock_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

impl TaskEngine {
    pub(crate) fn spawn_worker(&self, id: Uuid, generation: u64) {
        let engine = self.clone();
        tokio::spawn(async move {
            engine.run_worker(id, generation).await;
        });
    }

    async fn run_worker(&self, id: Uuid, generation: u64) {
        let Some((operation, active, depends_on)) = self.active_operation(id, generation) else {
            return;
        };
        let context = TaskContext {
            id,
            generation,
            cancelled: active.cancelled.clone(),
            paused: active.paused.subscribe(),
            sink: self.inner.clone(),
            progress_gate: Default::default(),
        };

        // Park until the step this task is chained behind has finished. Doing
        // it before the semaphore keeps a waiting step from occupying a slot,
        // and doing it in the worker (not at submit time) keeps the task on
        // disk, so a restart still knows the pack was queued.
        if let Some(dependency) = depends_on
            && dependency != id
            && let Err(error) = self.await_dependency(dependency).await
        {
            self.complete(id, generation, Err(error)).await;
            return;
        }

        let permit = loop {
            if context.is_cancelled() {
                self.complete(id, generation, Err(Error::Download("task deleted".into())))
                    .await;
                return;
            }
            if let Err(error) = context.wait_if_paused().await {
                self.complete(id, generation, Err(error)).await;
                return;
            }
            match self.inner.semaphore.clone().try_acquire_owned() {
                Ok(permit) => match self.mark_running(id, generation) {
                    Ok(true) => break permit,
                    Ok(false) => {
                        drop(permit);
                        if self.active_operation(id, generation).is_none() {
                            self.complete(
                                id,
                                generation,
                                Err(Error::Download("task deleted".into())),
                            )
                            .await;
                            return;
                        }
                    }
                    Err(error) => {
                        drop(permit);
                        self.complete(id, generation, Err(error)).await;
                        return;
                    }
                },
                Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        };

        let resource_guard = if let Some(key) = resource_key(&operation) {
            Some(self.resource_lock(key).lock_owned().await)
        } else {
            None
        };
        if let Err(error) = context.wait_if_paused().await {
            drop(resource_guard);
            drop(permit);
            self.complete(id, generation, Err(error)).await;
            return;
        }
        if context.is_cancelled() {
            drop(resource_guard);
            drop(permit);
            self.complete(id, generation, Err(Error::Download("task deleted".into())))
                .await;
            return;
        }

        let result = self.inner.runner.run(operation, context).await;
        drop(resource_guard);
        drop(permit);
        self.complete(id, generation, result).await;
    }

    fn active_operation(
        &self,
        id: Uuid,
        generation: u64,
    ) -> Option<(TaskOperation, ActiveTask, Option<Uuid>)> {
        let state = self.inner.state.lock().ok()?;
        let active = state.active.get(&id)?.clone();
        if active.generation != generation {
            return None;
        }
        let record = state.records.get(&id)?;
        Some((record.operation.clone(), active, record.depends_on))
    }

    /// Waits for a chained predecessor and turns its outcome into this task's.
    async fn await_dependency(&self, dependency: Uuid) -> Result<()> {
        match self.wait(dependency).await {
            Ok(TaskOutcome::Succeeded) => Ok(()),
            Ok(TaskOutcome::Failed { message, .. } | TaskOutcome::Cancelled { message, .. }) => {
                Err(Error::Download(format!(
                    "previous step did not finish: {message}"
                )))
            }
            // The predecessor is gone (deleted or pruned from the store);
            // there is nothing left to wait for.
            Err(_) => Ok(()),
        }
    }

    fn mark_running(&self, id: Uuid, generation: u64) -> Result<bool> {
        let mut state = lock_state(&self.inner.state)?;
        let Some(active) = state.active.get(&id) else {
            return Ok(false);
        };
        if active.generation != generation || *active.paused.borrow() {
            return Ok(false);
        }
        let record = state
            .records
            .get_mut(&id)
            .ok_or_else(|| Error::Download(format!("task not found: {id}")))?;
        if record.state != TaskState::Queued {
            return Ok(false);
        }
        record.start()?;
        self.inner.store.upsert(record)?;
        publish_locked(&self.inner, &state);
        Ok(true)
    }

    async fn complete(&self, id: Uuid, generation: u64, result: Result<()>) {
        let key = RunKey { id, generation };
        let (outcome, tombstone) = {
            let mut state = match self.inner.state.lock() {
                Ok(state) => state,
                Err(_) => return,
            };
            if let Some(tombstone) = state.tombstones.remove(&key) {
                state.checkpoints.remove(&key);
                state.progress_publications.remove(&key);
                (None, Some(tombstone))
            } else {
                let Some(active) = state.active.get(&id) else {
                    return;
                };
                if active.generation != generation {
                    return;
                }
                let cancelled = active.cancelled.load(Ordering::Acquire);
                state.active.remove(&id);
                state.checkpoints.remove(&key);
                state.progress_publications.remove(&key);
                let Some(record) = state.records.get_mut(&id) else {
                    return;
                };
                let outcome = if cancelled {
                    let message = "task cancelled";
                    if record.cancel(message, true).is_err() {
                        return;
                    }
                    TaskOutcome::Cancelled {
                        message: message.into(),
                        retryable: true,
                    }
                } else {
                    match result {
                        Ok(()) => {
                            if record.succeed().is_err() {
                                return;
                            }
                            TaskOutcome::Succeeded
                        }
                        Err(error) => {
                            let retryable = error.is_retryable();
                            let message = error.to_string();
                            if record.fail(message.clone(), retryable).is_err() {
                                return;
                            }
                            TaskOutcome::Failed { message, retryable }
                        }
                    }
                };
                let settled = format!("{:?}", record.state);
                if self.inner.store.upsert(record).is_err() {
                    return;
                }
                crate::core::diagnostics::diagnostic!("task", "{id} -> {settled}");
                publish_locked(&self.inner, &state);
                let waiters = state.waiters.remove(&id).unwrap_or_default();
                for waiter in waiters {
                    let _ = waiter.send(outcome.clone());
                }
                (Some(outcome), None)
            }
        };

        if let Some(tombstone) = tombstone {
            let _ = tombstone.cancelled;
            let cleanup_errors = cleanup_owned(&tombstone.artifacts.owned_temporary);
            if !cleanup_errors.is_empty() {
                eprintln!(
                    "task {id} cleanup failed after deletion: {}",
                    cleanup_errors.join("; ")
                );
            }
        }
        let _ = outcome;
    }

    fn resource_lock(&self, key: ResourceKey) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self
            .inner
            .resource_locks
            .lock()
            .expect("task resource lock poisoned");
        locks
            .entry(key)
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }
}

impl TaskContextSink for EngineInner {
    fn report_progress(
        &self,
        id: Uuid,
        generation: u64,
        completed: u64,
        total: Option<u64>,
    ) -> Result<()> {
        let key = RunKey { id, generation };
        let mut state = lock_state(&self.state)?;
        let Some(active) = state.active.get(&id) else {
            return if state.tombstones.contains_key(&key) {
                Ok(())
            } else {
                Err(Error::Download(format!("task not found: {id}")))
            };
        };
        if active.generation != generation {
            return Ok(());
        }
        let checkpoint = state
            .checkpoints
            .get(&key)
            .is_none_or(|instant| instant.elapsed() >= TASK_CHECKPOINT_INTERVAL);
        let publish = state
            .progress_publications
            .get(&key)
            .is_none_or(|instant| instant.elapsed() >= TASK_SNAPSHOT_INTERVAL);
        let record = {
            let record = state
                .records
                .get_mut(&id)
                .ok_or_else(|| Error::Download(format!("task not found: {id}")))?;
            if record.state != TaskState::Running {
                return Ok(());
            }
            record.progress(completed, total)?;
            (checkpoint || publish).then(|| record.clone())
        };
        if checkpoint {
            state.checkpoints.insert(key.clone(), Instant::now());
            self.store.upsert(
                record
                    .as_ref()
                    .expect("checkpoint requires a task record snapshot"),
            )?;
        }
        if publish {
            state.progress_publications.insert(key, Instant::now());
            publish_locked(self, &state);
        }
        Ok(())
    }

    fn record_owned(&self, id: Uuid, generation: u64, path: PathBuf) -> Result<()> {
        self.update_artifacts(id, generation, |artifacts| artifacts.own_temporary(path))
    }

    fn record_committed(&self, id: Uuid, generation: u64, path: PathBuf) -> Result<()> {
        self.update_artifacts(id, generation, |artifacts| artifacts.commit(path))
    }

    fn record_shared(&self, id: Uuid, generation: u64, key: String, path: PathBuf) -> Result<()> {
        self.update_artifacts(id, generation, |artifacts| {
            artifacts.reference_shared(key, path)
        })
    }
}

impl EngineInner {
    fn update_artifacts(
        &self,
        id: Uuid,
        generation: u64,
        update: impl FnOnce(&mut ArtifactLedger),
    ) -> Result<()> {
        let key = RunKey { id, generation };
        let mut state = lock_state(&self.state)?;
        if let Some(active) = state.active.get(&id) {
            if active.generation != generation {
                return Ok(());
            }
            let checkpoint = state
                .checkpoints
                .get(&key)
                .is_none_or(|instant| instant.elapsed() >= TASK_CHECKPOINT_INTERVAL);
            let snapshot = {
                let record = state
                    .records
                    .get_mut(&id)
                    .ok_or_else(|| Error::Download(format!("task not found: {id}")))?;
                update(&mut record.artifacts);
                checkpoint.then(|| record.clone())
            };
            if let Some(snapshot) = snapshot {
                state.checkpoints.insert(key, Instant::now());
                self.store.upsert(&snapshot)?;
            }
            return Ok(());
        }
        if let Some(tombstone) = state.tombstones.get_mut(&key) {
            update(&mut tombstone.artifacts);
            return Ok(());
        }
        Err(Error::Download(format!("task not found: {id}")))
    }
}
