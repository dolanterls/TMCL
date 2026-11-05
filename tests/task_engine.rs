use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use tmcl::core::error::Error;
use tmcl::core::task::store::JsonTaskStore;
use tmcl::core::task::{
    InstanceInstallSpec, JavaRuntimeSpec, LoaderSpec, TaskAction, TaskContext, TaskEngine,
    TaskOperation, TaskOutcome, TaskRecord, TaskRunner, TaskState,
};
use tokio::sync::Notify;
use uuid::Uuid;

fn temp_root() -> PathBuf {
    std::env::temp_dir().join(format!("tmcl-task-engine-{}", Uuid::new_v4()))
}

fn java_operation() -> TaskOperation {
    TaskOperation::JavaRuntime(JavaRuntimeSpec {
        data_root: temp_root().join("runtime"),
        major: 25,
        download_snapshot: Default::default(),
    })
}

fn instance_operation(instance_id: Uuid) -> TaskOperation {
    TaskOperation::InstanceInstall(InstanceInstallSpec {
        instance_id,
        game_directory: temp_root().join("game"),
        instance_directory: temp_root().join("instance"),
        instance_name: format!("instance-{instance_id}"),
        minecraft_version: "1.21.1".into(),
        loader: LoaderSpec::Vanilla,
        java_executable: None,
        download_snapshot: Default::default(),
    })
}

struct ScriptedRunner {
    calls: AtomicUsize,
    fail_first: usize,
}

impl ScriptedRunner {
    fn successful() -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicUsize::new(0),
            fail_first: 0,
        })
    }

    fn fail_once() -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicUsize::new(0),
            fail_first: 1,
        })
    }

    fn fail_times(times: usize) -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicUsize::new(0),
            fail_first: times,
        })
    }
}

impl TaskRunner for ScriptedRunner {
    fn run(&self, _operation: TaskOperation, context: TaskContext) -> tmcl::core::task::TaskFuture {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let fail_first = self.fail_first;
        Box::pin(async move {
            context.report_progress(10, Some(10))?;
            if call < fail_first {
                return Err(Error::Network("temporary failure".into()));
            }
            Ok(())
        })
    }
}

struct WaitingRunner {
    started: Arc<Notify>,
    release: Arc<AtomicBool>,
    ignore_cancellation: bool,
}

struct ControlledRunner {
    active: Arc<AtomicUsize>,
    max_active: Arc<AtomicUsize>,
    started: Arc<AtomicUsize>,
    started_notify: Arc<Notify>,
    release: Arc<AtomicBool>,
}

impl ControlledRunner {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            active: Arc::new(AtomicUsize::new(0)),
            max_active: Arc::new(AtomicUsize::new(0)),
            started: Arc::new(AtomicUsize::new(0)),
            started_notify: Arc::new(Notify::new()),
            release: Arc::new(AtomicBool::new(false)),
        })
    }

    fn release(&self) {
        self.release.store(true, Ordering::Release);
    }

    async fn wait_until_started(&self, count: usize) {
        loop {
            let notified = self.started_notify.notified();
            if self.started.load(Ordering::Acquire) >= count {
                return;
            }
            notified.await;
        }
    }
}

impl TaskRunner for ControlledRunner {
    fn run(&self, _operation: TaskOperation, context: TaskContext) -> tmcl::core::task::TaskFuture {
        let active = self.active.clone();
        let max_active = self.max_active.clone();
        let started = self.started.clone();
        let release = self.release.clone();
        let started_notify = self.started_notify.clone();
        let current = active.fetch_add(1, Ordering::AcqRel) + 1;
        max_active.fetch_max(current, Ordering::AcqRel);
        started.fetch_add(1, Ordering::AcqRel);
        started_notify.notify_waiters();
        Box::pin(async move {
            while !release.load(Ordering::Acquire) {
                context.wait_if_paused().await?;
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            active.fetch_sub(1, Ordering::AcqRel);
            Ok(())
        })
    }
}

impl WaitingRunner {
    fn new(ignore_cancellation: bool) -> Arc<Self> {
        Arc::new(Self {
            started: Arc::new(Notify::new()),
            release: Arc::new(AtomicBool::new(false)),
            ignore_cancellation,
        })
    }

    fn release(&self) {
        self.release.store(true, Ordering::SeqCst);
    }
}

impl TaskRunner for WaitingRunner {
    fn run(&self, _operation: TaskOperation, context: TaskContext) -> tmcl::core::task::TaskFuture {
        let started = self.started.clone();
        let release = self.release.clone();
        let ignore_cancellation = self.ignore_cancellation;
        Box::pin(async move {
            started.notify_waiters();
            loop {
                if !ignore_cancellation && context.is_cancelled() {
                    return Err(Error::Download("download cancelled".into()));
                }
                context.wait_if_paused().await?;
                if release.load(Ordering::SeqCst) {
                    return Ok(());
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
    }
}

struct ArtifactRunner {
    committed: PathBuf,
    shared: PathBuf,
}

impl TaskRunner for ArtifactRunner {
    fn run(&self, _operation: TaskOperation, context: TaskContext) -> tmcl::core::task::TaskFuture {
        let committed = self.committed.clone();
        let shared = self.shared.clone();
        Box::pin(async move {
            context.commit_file(committed)?;
            context.reference_shared_file("shared", shared)?;
            Ok(())
        })
    }
}

struct FailingArtifactRunner {
    owned: PathBuf,
    shared: PathBuf,
}

impl TaskRunner for FailingArtifactRunner {
    fn run(&self, _operation: TaskOperation, context: TaskContext) -> tmcl::core::task::TaskFuture {
        let owned = self.owned.clone();
        let shared = self.shared.clone();
        Box::pin(async move {
            context.own_temporary_file(owned)?;
            context.reference_shared_file("shared", shared)?;
            Err(Error::Network("temporary failure".into()))
        })
    }
}

async fn wait_for_state(engine: &TaskEngine, id: Uuid, state: TaskState) {
    for _ in 0..200 {
        if engine
            .snapshots()
            .iter()
            .any(|snapshot| snapshot.id == id && snapshot.state == state)
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!(
        "task {id} did not reach {state:?}: {:?}",
        engine.snapshots()
    );
}

#[tokio::test]
async fn submit_persists_before_runner_and_wait_returns_outcome() {
    let root = temp_root();
    let store = JsonTaskStore::new(root.join("tasks"));
    let runner = ScriptedRunner::successful();
    let engine = TaskEngine::open(store.clone(), runner.clone(), 2).expect("open engine");

    let id = engine.submit(java_operation()).expect("submit task");
    assert!(store.path(id).is_file());
    assert_eq!(engine.wait(id).await.expect("wait"), TaskOutcome::Succeeded);
    assert_eq!(runner.calls.load(Ordering::SeqCst), 1);

    std::fs::remove_dir_all(root).expect("cleanup");
}

#[tokio::test]
async fn failed_retry_reuses_one_id_and_one_record() {
    let root = temp_root();
    let store = JsonTaskStore::new(root.join("tasks"));
    let runner = ScriptedRunner::fail_once();
    let engine = TaskEngine::open(store.clone(), runner, 2).expect("open engine");
    let id = engine.submit(java_operation()).expect("submit task");

    assert!(matches!(
        engine.wait(id).await.expect("wait first attempt"),
        TaskOutcome::Failed {
            retryable: true,
            ..
        }
    ));
    assert_eq!(
        engine.pause_or_resume(id).await.expect("retry"),
        TaskAction::Retrying
    );
    assert_eq!(
        engine.wait(id).await.expect("wait retry"),
        TaskOutcome::Succeeded
    );
    let records = store.load().expect("load records");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].id, id);
    assert_eq!(records[0].attempt, 2);

    std::fs::remove_dir_all(root).expect("cleanup");
}

#[tokio::test]
async fn repeated_retries_keep_one_id_and_one_record() {
    let root = temp_root();
    let store = JsonTaskStore::new(root.join("tasks"));
    let runner = ScriptedRunner::fail_times(4);
    let engine = TaskEngine::open(store.clone(), runner, 1).expect("open engine");
    let id = engine.submit(java_operation()).expect("submit task");

    for _ in 0..4 {
        assert!(matches!(
            engine.wait(id).await.expect("wait failed attempt"),
            TaskOutcome::Failed {
                retryable: true,
                ..
            }
        ));
        assert_eq!(engine.retry(id).await.expect("retry"), TaskAction::Retrying);
    }
    assert_eq!(
        engine.wait(id).await.expect("wait success"),
        TaskOutcome::Succeeded
    );

    let records = store.load().expect("load records");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].id, id);
    assert_eq!(records[0].attempt, 5);

    std::fs::remove_dir_all(root).expect("cleanup");
}

#[tokio::test]
async fn repeated_open_does_not_duplicate_records_or_resume_work() {
    let root = temp_root();
    let directory = root.join("tasks");
    let store = JsonTaskStore::new(directory.clone());
    let engine =
        TaskEngine::open(store.clone(), ScriptedRunner::successful(), 2).expect("open engine");
    let id = engine.submit(java_operation()).expect("submit task");
    engine.wait(id).await.expect("wait");
    drop(engine);

    for _ in 0..4 {
        let reopened = TaskEngine::open(
            JsonTaskStore::new(directory.clone()),
            ScriptedRunner::successful(),
            2,
        )
        .expect("reopen engine");
        assert_eq!(reopened.snapshots().len(), 1);
        assert_eq!(reopened.snapshots()[0].id, id);
    }

    std::fs::remove_dir_all(root).expect("cleanup");
}

#[tokio::test]
async fn restarting_running_task_marks_one_interrupted_retryable_record() {
    let root = temp_root();
    let directory = root.join("tasks");
    let store = JsonTaskStore::new(directory.clone());
    let engine = TaskEngine::open(store.clone(), WaitingRunner::new(false), 1).expect("open");
    let id = engine.submit(java_operation()).expect("submit");
    wait_for_state(&engine, id, TaskState::Running).await;
    drop(engine);

    // A restart means the owning process is gone. Dropping the engine does not
    // stop the process, so the record has to be re-stamped as abandoned for the
    // reopen to see what a real restart would see.
    let mut record = store.load().expect("load records").remove(0);
    record.owner_pid = Some(u32::MAX - 7);
    record.owner_started_at = Some(1);
    store.upsert(&record).expect("mark abandoned");

    let reopened = TaskEngine::open(
        JsonTaskStore::new(directory.clone()),
        ScriptedRunner::successful(),
        1,
    )
    .expect("reopen");
    let snapshots = reopened.snapshots();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].state, TaskState::Failed);
    assert!(snapshots[0].retryable);
    assert!(
        snapshots[0]
            .failure
            .as_deref()
            .is_some_and(|message| message.contains("Interrupted"))
    );

    std::fs::remove_dir_all(root).expect("cleanup");
}

#[tokio::test]
async fn pause_resume_updates_control_without_creating_a_second_task() {
    let root = temp_root();
    let store = JsonTaskStore::new(root.join("tasks"));
    let runner = WaitingRunner::new(false);
    let engine = TaskEngine::open(store, runner.clone(), 1).expect("open");
    let id = engine.submit(java_operation()).expect("submit");
    wait_for_state(&engine, id, TaskState::Running).await;

    assert_eq!(
        engine.pause_or_resume(id).await.expect("pause"),
        TaskAction::Paused
    );
    assert_eq!(engine.snapshots()[0].state, TaskState::Paused);
    assert_eq!(
        engine.pause_or_resume(id).await.expect("resume"),
        TaskAction::Running
    );
    runner.release();
    assert_eq!(engine.wait(id).await.expect("wait"), TaskOutcome::Succeeded);
    assert_eq!(engine.snapshots().len(), 1);

    std::fs::remove_dir_all(root).expect("cleanup");
}

#[tokio::test]
async fn resource_locks_serialize_one_instance_but_allow_other_instances_to_run() {
    let root = temp_root();
    let runner = ControlledRunner::new();
    let engine =
        TaskEngine::open(JsonTaskStore::new(root.join("tasks")), runner.clone(), 2).expect("open");
    let instance_id = Uuid::new_v4();
    let first = engine
        .submit(instance_operation(instance_id))
        .expect("submit first");
    let second = engine
        .submit(instance_operation(instance_id))
        .expect("submit second");

    runner.wait_until_started(1).await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(runner.max_active.load(Ordering::Acquire), 1);
    runner.release();
    assert_eq!(
        engine.wait(first).await.expect("wait first"),
        TaskOutcome::Succeeded
    );
    assert_eq!(
        engine.wait(second).await.expect("wait second"),
        TaskOutcome::Succeeded
    );

    let other_runner = ControlledRunner::new();
    let other_engine = TaskEngine::open(
        JsonTaskStore::new(root.join("other-tasks")),
        other_runner.clone(),
        2,
    )
    .expect("open other engine");
    let third = other_engine
        .submit(instance_operation(Uuid::new_v4()))
        .expect("submit third");
    let fourth = other_engine
        .submit(instance_operation(Uuid::new_v4()))
        .expect("submit fourth");
    other_runner.wait_until_started(2).await;
    assert_eq!(other_runner.max_active.load(Ordering::Acquire), 2);
    other_runner.release();
    assert_eq!(
        other_engine.wait(third).await.expect("wait third"),
        TaskOutcome::Succeeded
    );
    assert_eq!(
        other_engine.wait(fourth).await.expect("wait fourth"),
        TaskOutcome::Succeeded
    );

    std::fs::remove_dir_all(root).expect("cleanup");
}

#[tokio::test]
async fn deleting_running_task_hides_it_and_late_worker_cannot_restore_it() {
    let root = temp_root();
    let directory = root.join("tasks");
    let store = JsonTaskStore::new(directory.clone());
    let runner = WaitingRunner::new(true);
    let engine = TaskEngine::open(store.clone(), runner.clone(), 1).expect("open");
    let id = engine.submit(java_operation()).expect("submit");
    wait_for_state(&engine, id, TaskState::Running).await;
    let waiter = {
        let engine = engine.clone();
        tokio::spawn(async move { engine.wait(id).await })
    };
    tokio::task::yield_now().await;
    assert_eq!(
        engine.delete(id).await.expect("delete").action,
        TaskAction::Deleted
    );
    assert_eq!(
        waiter.await.expect("waiter task").expect("deleted outcome"),
        TaskOutcome::Cancelled {
            message: "task deleted".into(),
            retryable: false,
        }
    );
    assert!(engine.snapshots().is_empty());
    assert!(!store.path(id).exists());

    runner.release();
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert!(engine.snapshots().is_empty());
    assert!(store.load().expect("load after delete").is_empty());

    std::fs::remove_dir_all(root).expect("cleanup");
}

#[tokio::test]
async fn deleting_success_keeps_committed_and_shared_files() {
    let root = temp_root();
    let committed = root.join("game").join("client.jar");
    let shared = root.join("game").join("libraries").join("x.jar");
    std::fs::create_dir_all(committed.parent().expect("committed parent")).expect("mkdir");
    std::fs::create_dir_all(shared.parent().expect("shared parent")).expect("mkdir");
    std::fs::write(&committed, b"client").expect("write committed");
    std::fs::write(&shared, b"shared").expect("write shared");

    let store = JsonTaskStore::new(root.join("tasks"));
    let engine = TaskEngine::open(
        store,
        Arc::new(ArtifactRunner {
            committed: committed.clone(),
            shared: shared.clone(),
        }),
        1,
    )
    .expect("open");
    let id = engine.submit(java_operation()).expect("submit");
    engine.wait(id).await.expect("wait");
    engine.delete(id).await.expect("delete");

    assert!(committed.is_file());
    assert!(shared.is_file());
    std::fs::remove_dir_all(root).expect("cleanup");
}

#[tokio::test]
async fn deleting_failed_task_cleans_owned_temporary_and_keeps_shared_file() {
    let root = temp_root();
    let owned = root.join("instance").join("download.part");
    let shared = root.join("game").join("libraries").join("shared.jar");
    std::fs::create_dir_all(owned.parent().expect("owned parent")).expect("mkdir owned");
    std::fs::create_dir_all(shared.parent().expect("shared parent")).expect("mkdir shared");
    std::fs::write(&owned, b"partial").expect("write owned");
    std::fs::write(&shared, b"shared").expect("write shared");

    let store = JsonTaskStore::new(root.join("tasks"));
    let engine = TaskEngine::open(
        store,
        Arc::new(FailingArtifactRunner {
            owned: owned.clone(),
            shared: shared.clone(),
        }),
        1,
    )
    .expect("open");
    let id = engine.submit(java_operation()).expect("submit");
    assert!(matches!(
        engine.wait(id).await.expect("wait failure"),
        TaskOutcome::Failed {
            retryable: true,
            ..
        }
    ));

    engine.delete(id).await.expect("delete");
    assert!(!owned.exists());
    assert!(shared.is_file());

    std::fs::remove_dir_all(root).expect("cleanup");
}

#[tokio::test]
async fn deleting_cancelled_task_cleans_owned_temporary() {
    let root = temp_root();
    let owned = root.join("instance").join("cancelled.part");
    std::fs::create_dir_all(owned.parent().expect("owned parent")).expect("mkdir");
    std::fs::write(&owned, b"partial").expect("write owned");

    let store = JsonTaskStore::new(root.join("tasks"));
    let id = Uuid::new_v4();
    let mut record = TaskRecord::new(id, java_operation(), 1);
    record.cancel("cancelled", true).expect("cancel");
    record.record_owned_temporary(owned.clone());
    store.upsert(&record).expect("persist cancelled");
    let engine = TaskEngine::open(store, ScriptedRunner::successful(), 1).expect("open");

    assert_eq!(engine.snapshots()[0].state, TaskState::Cancelled);
    engine.delete(id).await.expect("delete");
    assert!(!owned.exists());

    std::fs::remove_dir_all(root).expect("cleanup");
}

#[tokio::test]
async fn progress_publishes_latest_snapshot_without_replaying_old_events() {
    let root = temp_root();
    let store = JsonTaskStore::new(root.join("tasks"));
    let runner = ProgressRunner;
    let engine = TaskEngine::open(store, Arc::new(runner), 1).expect("open");
    let updates = engine.subscribe();
    let id = engine.submit(java_operation()).expect("submit");

    engine.wait(id).await.expect("wait");
    let current = updates.borrow().clone();
    assert_eq!(current.len(), 1);
    assert_eq!(current[0].state, TaskState::Succeeded);
    assert_eq!(current[0].completed, 100);
    assert!(updates.has_changed().is_ok());

    std::fs::remove_dir_all(root).expect("cleanup");
}

#[tokio::test]
async fn coalesces_high_frequency_progress_snapshots() {
    let root = temp_root();
    let engine = TaskEngine::open(
        JsonTaskStore::new(root.join("tasks")),
        Arc::new(YieldingProgressRunner),
        1,
    )
    .expect("open");
    let mut updates = engine.subscribe();
    let id = engine.submit(java_operation()).expect("submit");
    let mut update_count = 0;

    loop {
        updates.changed().await.expect("task snapshot channel");
        update_count += 1;
        if updates.borrow()[0].state == TaskState::Succeeded {
            break;
        }
    }

    engine.wait(id).await.expect("wait");
    assert!(
        update_count < 15,
        "progress snapshots were not coalesced: {update_count} updates"
    );
    std::fs::remove_dir_all(root).expect("cleanup");
}

struct ProgressRunner;

impl TaskRunner for ProgressRunner {
    fn run(&self, _operation: TaskOperation, context: TaskContext) -> tmcl::core::task::TaskFuture {
        Box::pin(async move {
            for completed in 1..=100 {
                context.report_progress(completed, Some(100))?;
            }
            Ok(())
        })
    }
}

struct YieldingProgressRunner;

impl TaskRunner for YieldingProgressRunner {
    fn run(&self, _operation: TaskOperation, context: TaskContext) -> tmcl::core::task::TaskFuture {
        Box::pin(async move {
            for completed in 1..=30 {
                context.report_progress(completed, Some(30))?;
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            Ok(())
        })
    }
}

/// Blocks the first task until released, and records the order tasks start in.
struct ChainRunner {
    order: std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>,
    hold: std::sync::Arc<AtomicBool>,
}

impl TaskRunner for ChainRunner {
    fn run(&self, operation: TaskOperation, _context: TaskContext) -> tmcl::core::task::TaskFuture {
        let label = match operation {
            TaskOperation::JavaRuntime(_) => "first",
            TaskOperation::InstanceInstall(_) => "second",
            _ => "other",
        };
        let order = self.order.clone();
        let hold = self.hold.clone();
        Box::pin(async move {
            order.lock().expect("order").push(label);
            if label == "first" {
                while hold.load(Ordering::Acquire) {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            }
            Ok(())
        })
    }
}

#[tokio::test]
async fn a_chained_task_waits_for_its_predecessor() {
    let root = temp_root();
    let store = JsonTaskStore::new(root.join("tasks"));
    let order = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let hold = std::sync::Arc::new(AtomicBool::new(true));
    let runner = std::sync::Arc::new(ChainRunner {
        order: order.clone(),
        hold: hold.clone(),
    });
    let engine = TaskEngine::open(store, runner, 3).expect("open engine");

    let first = engine.submit(java_operation()).expect("submit first");
    let second = engine
        .submit_with_job_after(instance_operation(Uuid::new_v4()), None, Some(first))
        .expect("submit second");

    // Long enough that a worker ignoring the dependency would have started.
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        order.lock().expect("order").as_slice(),
        &["first"],
        "the chained task started before its predecessor finished"
    );

    hold.store(false, Ordering::Release);
    assert_eq!(
        engine.wait(first).await.expect("wait first"),
        TaskOutcome::Succeeded
    );
    assert_eq!(
        engine.wait(second).await.expect("wait second"),
        TaskOutcome::Succeeded
    );
    assert_eq!(
        order.lock().expect("order").as_slice(),
        &["first", "second"]
    );

    std::fs::remove_dir_all(root).expect("cleanup");
}

#[tokio::test]
async fn a_chained_task_fails_without_running_when_its_predecessor_fails() {
    let root = temp_root();
    let store = JsonTaskStore::new(root.join("tasks"));
    let runner = ScriptedRunner::fail_once();
    let engine = TaskEngine::open(store, runner.clone(), 3).expect("open engine");

    let first = engine.submit(java_operation()).expect("submit first");
    let second = engine
        .submit_with_job_after(instance_operation(Uuid::new_v4()), None, Some(first))
        .expect("submit second");

    assert!(matches!(
        engine.wait(first).await.expect("wait first"),
        TaskOutcome::Failed { .. }
    ));
    match engine.wait(second).await.expect("wait second") {
        TaskOutcome::Failed { message, .. } => {
            assert!(message.contains("previous step"), "{message}");
        }
        other => panic!("chained task should fail with its predecessor: {other:?}"),
    }
    assert_eq!(
        runner.calls.load(Ordering::SeqCst),
        1,
        "the chained task must not run after its predecessor failed"
    );

    std::fs::remove_dir_all(root).expect("cleanup");
}

/// A record left behind by a process that is gone must be closed on open.
#[tokio::test]
async fn an_abandoned_running_task_is_interrupted_on_open() {
    use tmcl::core::task::PersistedTaskRecord;

    let root = temp_root();
    let directory = root.join("tasks");
    let store = JsonTaskStore::new(directory.clone());
    let record = TaskRecord::new(Uuid::new_v4(), java_operation(), 1).persisted();
    let abandoned = PersistedTaskRecord {
        state: TaskState::Running,
        // No such process; `process_started_at` returns `None` for it.
        owner_pid: Some(u32::MAX - 7),
        owner_started_at: Some(1),
        ..record
    };
    std::fs::create_dir_all(&directory).expect("tasks dir");
    std::fs::write(
        directory.join(format!("{}.json", abandoned.id)),
        serde_json::to_vec(&abandoned).expect("encode"),
    )
    .expect("write record");

    let engine = TaskEngine::open(
        JsonTaskStore::new(directory.clone()),
        ScriptedRunner::successful(),
        3,
    )
    .expect("open engine");

    let snapshots = engine.snapshots();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].state, TaskState::Failed);
    assert!(
        snapshots[0].retryable,
        "an interrupted task stays retryable"
    );

    let stored = store.load().expect("reload");
    assert_eq!(stored[0].state, TaskState::Failed);
    std::fs::remove_dir_all(root).expect("cleanup");
}

/// A record owned by a live process belongs to another launcher instance and
/// must survive a second process opening the same store.
#[tokio::test]
async fn a_task_owned_by_a_live_process_is_left_alone() {
    use tmcl::core::task::PersistedTaskRecord;

    let root = temp_root();
    let directory = root.join("tasks");
    let store = JsonTaskStore::new(directory.clone());
    let record = TaskRecord::new(Uuid::new_v4(), java_operation(), 1).persisted();
    let owned = PersistedTaskRecord {
        state: TaskState::Running,
        owner_pid: Some(std::process::id()),
        owner_started_at: tmcl::platform::windows::process_started_at(std::process::id()),
        ..record
    };
    std::fs::create_dir_all(&directory).expect("tasks dir");
    std::fs::write(
        directory.join(format!("{}.json", owned.id)),
        serde_json::to_vec(&owned).expect("encode"),
    )
    .expect("write record");

    let engine = TaskEngine::open(
        JsonTaskStore::new(directory.clone()),
        ScriptedRunner::successful(),
        3,
    )
    .expect("open engine");

    let snapshots = engine.snapshots();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(
        snapshots[0].state,
        TaskState::Running,
        "another process must not close a task that is still being worked on"
    );
    let stored = store.load().expect("reload");
    assert_eq!(stored[0].state, TaskState::Running);
    std::fs::remove_dir_all(root).expect("cleanup");
}
