use std::path::PathBuf;

use tmcl::core::download::{DownloadSourceMode, DownloadSourceSnapshot};
use tmcl::core::task::{
    ArtifactLedger, InstanceInstallSpec, JavaRuntimeSpec, LoaderSpec, TaskAction, TaskOperation,
    TaskOperationType, TaskRecord, TaskState,
};
use uuid::Uuid;

fn instance_operation() -> TaskOperation {
    TaskOperation::InstanceInstall(InstanceInstallSpec {
        instance_id: Uuid::new_v4(),
        game_directory: PathBuf::from("game"),
        instance_directory: PathBuf::from("game/instances/test"),
        instance_name: "test".into(),
        minecraft_version: "1.21.1".into(),
        loader: LoaderSpec::Fabric {
            version: "0.16.10".into(),
        },
        java_executable: Some(PathBuf::from("java/bin/java.exe")),
        download_snapshot: DownloadSourceSnapshot::new(
            DownloadSourceMode::Mirror,
            DownloadSourceMode::Official,
        ),
    })
}

#[test]
fn operation_round_trips_all_retry_data() {
    let operation = instance_operation();
    let encoded = serde_json::to_vec(&operation).expect("serialize operation");
    let decoded: TaskOperation = serde_json::from_slice(&encoded).expect("deserialize operation");

    assert_eq!(decoded, operation);
}

#[test]
fn task_id_survives_retry_and_retry_resets_attempt_state() {
    let mut task = TaskRecord::new(Uuid::new_v4(), instance_operation(), 10);
    let id = task.id;

    task.start().expect("start");
    task.progress(12, Some(20)).expect("progress");
    task.fail("temporary network failure", true).expect("fail");
    assert!(task.retryable);

    assert_eq!(
        task.prepare_retry().expect("prepare retry"),
        TaskAction::Retrying
    );
    assert_eq!(task.id, id);
    assert_eq!(task.state, TaskState::Queued);
    assert_eq!(task.attempt, 1);
    assert_eq!(task.completed, 0);
    assert_eq!(task.total, None);
    assert_eq!(task.failure, None);
    assert!(!task.retryable);
}

#[test]
fn pause_resume_and_progress_follow_the_state_table() {
    let mut task = TaskRecord::new(
        Uuid::new_v4(),
        TaskOperation::JavaRuntime(JavaRuntimeSpec {
            data_root: PathBuf::from("runtime"),
            major: 21,
            download_snapshot: Default::default(),
        }),
        10,
    );

    assert_eq!(
        task.pause_or_resume().expect("pause queued"),
        TaskAction::Paused
    );
    assert_eq!(task.state, TaskState::Paused);
    assert_eq!(
        task.pause_or_resume().expect("resume queued"),
        TaskAction::Queued
    );
    task.start().expect("start");
    assert_eq!(
        task.pause_or_resume().expect("pause running"),
        TaskAction::Paused
    );
    assert!(task.progress(1, Some(2)).is_err());
    assert_eq!(
        task.pause_or_resume().expect("resume running"),
        TaskAction::Running
    );
    task.progress(1, Some(2)).expect("progress after resume");

    task.pause_or_resume().expect("pause before completion");
    task.succeed()
        .expect("a runner may finish while a pause is being applied");
    assert_eq!(task.state, TaskState::Succeeded);
}

#[test]
fn terminal_states_and_artifact_ownership_are_explicit() {
    let mut task = TaskRecord::new(Uuid::new_v4(), instance_operation(), 10);
    task.start().expect("start");
    task.record_owned_temporary(PathBuf::from("game/.part"));
    task.record_committed(PathBuf::from("game/versions/test.jar"));
    task.record_shared("libraries/a.jar", PathBuf::from("game/libraries/a.jar"));
    task.succeed().expect("succeed");

    assert_eq!(task.state, TaskState::Succeeded);
    assert!(task.prepare_retry().is_err());
    assert_eq!(
        task.artifacts,
        ArtifactLedger {
            owned_temporary: vec![PathBuf::from("game/.part")],
            committed: vec![PathBuf::from("game/versions/test.jar")],
            shared: vec![(
                "libraries/a.jar".into(),
                PathBuf::from("game/libraries/a.jar"),
            )],
        }
    );
}

#[test]
fn snapshots_expose_kind_label_and_retryability_without_runtime_handles() {
    let mut task = TaskRecord::new(Uuid::new_v4(), instance_operation(), 10);
    task.start().expect("start");
    task.fail("transient", true).expect("fail");

    let snapshot = task.snapshot();
    assert_eq!(snapshot.operation_type, TaskOperationType::InstanceInstall);
    assert_eq!(snapshot.label, "Install test (1.21.1, fabric)");
    assert_eq!(snapshot.state, TaskState::Failed);
    assert!(snapshot.retryable);
    assert_eq!(snapshot.completed, 0);
}

#[test]
fn interrupted_unrecoverable_tasks_remain_non_retryable() {
    let mut task = TaskRecord::new(
        Uuid::new_v4(),
        TaskOperation::Unrecoverable {
            label: "legacy install".into(),
            reason: "missing instance directory".into(),
        },
        10,
    );
    task.start().expect("start migrated task");

    assert!(task.mark_interrupted());
    assert_eq!(task.state, TaskState::Failed);
    assert!(!task.retryable);
    assert!(
        task.failure
            .as_deref()
            .is_some_and(|message| message.contains("重新创建"))
    );
}

#[test]
fn planned_totals_are_known_before_the_first_progress_report() {
    let files = TaskOperation::ModpackFiles(tmcl::core::task::ModpackFilesSpec {
        instance_id: Uuid::new_v4(),
        instance_root: PathBuf::from("game/versions/pack"),
        cache_root: PathBuf::from("cache"),
        files: vec![
            tmcl::core::task::ModpackFileSpec {
                path: "mods/a.jar".into(),
                urls: vec!["https://example.invalid/a.jar".into()],
                sha1: None,
                size: Some(10),
            },
            tmcl::core::task::ModpackFileSpec {
                path: "mods/b.jar".into(),
                urls: vec!["https://example.invalid/b.jar".into()],
                sha1: None,
                size: Some(32),
            },
        ],
        download_snapshot: DownloadSourceSnapshot::default(),
    });
    assert_eq!(files.planned_total(), Some(42));

    // One unknown size makes the sum unknowable; the step then reports its
    // real total as the files land.
    let partial = match files {
        TaskOperation::ModpackFiles(mut spec) => {
            spec.files[1].size = None;
            TaskOperation::ModpackFiles(spec)
        }
        other => other,
    };
    assert_eq!(partial.planned_total(), None);
    assert_eq!(instance_operation().planned_total(), None);
}

#[test]
fn a_progress_report_that_overshoots_its_total_is_clamped() {
    let mut task = TaskRecord::new(Uuid::new_v4(), instance_operation(), 10);
    task.start().expect("start");
    task.progress(120, Some(100))
        .expect("a lagging total must not fail the task");
    assert_eq!(task.completed, 100);
    assert_eq!(task.total, Some(100));
}
