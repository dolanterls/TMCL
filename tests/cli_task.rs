//! `task` commands against records written by an earlier run.
//!
//! The records are built with the same `TaskRecord` the engine persists, so the
//! fixtures cannot drift from the on-disk schema.

mod support;

use std::process::{Command, Stdio};
use std::time::Duration;

use support::Sandbox;
use tmcl::core::task::{JavaRuntimeSpec, TaskOperation, TaskRecord, TaskState};
use uuid::Uuid;

fn operation() -> TaskOperation {
    TaskOperation::JavaRuntime(JavaRuntimeSpec {
        data_root: std::env::temp_dir().join("tmcl-task-fixture-runtime"),
        major: 17,
        download_snapshot: Default::default(),
    })
}

fn record(state: TaskState, retryable: bool, failure: Option<&str>) -> TaskRecord {
    let mut record = TaskRecord::new(Uuid::new_v4(), operation(), 1);
    record.state = state;
    record.retryable = retryable;
    record.failure = failure.map(str::to_owned);
    record
}

fn write(sandbox: &Sandbox, record: &TaskRecord) {
    std::fs::create_dir_all(sandbox.tasks_dir()).expect("tasks dir");
    std::fs::write(
        sandbox.tasks_dir().join(format!("{}.json", record.id)),
        serde_json::to_vec(&record.persisted()).expect("encode"),
    )
    .expect("write task");
}

fn task_file(sandbox: &Sandbox, id: Uuid) -> std::path::PathBuf {
    sandbox.tasks_dir().join(format!("{id}.json"))
}

#[test]
fn task_list_filters_by_state() {
    let sandbox = Sandbox::new("task-list");
    let succeeded = record(TaskState::Succeeded, true, None);
    let failed = record(TaskState::Failed, true, Some("download failed"));
    write(&sandbox, &succeeded);
    write(&sandbox, &failed);

    let only_failed = sandbox.json(&["task", "list", "--state", "failed", "--json"]);
    let rows = only_failed.as_array().expect("array");
    assert_eq!(rows.len(), 1, "{only_failed}");
    assert_eq!(rows[0]["id"], failed.id.to_string());

    let only_done = sandbox.json(&["task", "list", "--state", "done", "--json"]);
    assert_eq!(only_done.as_array().expect("array").len(), 1, "{only_done}");

    let all = sandbox.json(&["task", "list", "--state", "all", "--json"]);
    assert_eq!(all.as_array().expect("array").len(), 2, "{all}");
}

#[test]
fn task_retry_refuses_a_task_that_is_not_retryable() {
    let sandbox = Sandbox::new("task-retry");
    let mut failed = record(TaskState::Failed, false, Some("cannot resume"));
    failed.retryable = false;
    write(&sandbox, &failed);

    let output = sandbox.run(&["task", "retry", &failed.id.to_string()]);

    assert_eq!(output.status.code(), Some(3), "download errors exit with 3");
    let message = String::from_utf8_lossy(&output.stderr);
    assert!(message.contains("only retryable"), "{message}");
}

#[test]
fn task_watch_reports_a_finished_task_without_waiting() {
    let sandbox = Sandbox::new("task-watch-done");
    let failed = record(TaskState::Failed, true, Some("boom"));
    write(&sandbox, &failed);

    let output = sandbox.run(&["task", "watch", &failed.id.to_string()]);

    assert_eq!(output.status.code(), Some(3), "a failed task exits 3");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("boom"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn task_cancel_needs_confirmation_and_removes_the_record() {
    let sandbox = Sandbox::new("task-cancel");
    let failed = record(TaskState::Failed, true, Some("stuck"));
    write(&sandbox, &failed);
    let id = failed.id.to_string();

    let refused = sandbox.run(&["task", "cancel", &id]);
    assert_eq!(refused.status.code(), Some(2), "no --yes means no");
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("--yes"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert!(
        task_file(&sandbox, failed.id).is_file(),
        "nothing may be removed"
    );

    let planned = sandbox.json(&["task", "cancel", &id, "--dry-run", "--json"]);
    assert_eq!(planned["applied"], false, "{planned}");
    assert!(task_file(&sandbox, failed.id).is_file());

    let removed = sandbox.json(&["task", "cancel", &id, "--yes", "--json"]);
    assert_eq!(removed["deleted"], true, "{removed}");
    assert!(
        !task_file(&sandbox, failed.id).exists(),
        "the record should be gone"
    );
}

#[test]
fn task_watch_explains_when_the_task_belongs_to_another_process() {
    let sandbox = Sandbox::new("task-watch-other");
    // `TaskRecord::new` stamps this (the test) process as the owner, which is
    // exactly the "another launcher is running it" case.
    let running = record(TaskState::Running, true, None);
    write(&sandbox, &running);

    let mut child = Command::new(env!("CARGO_BIN_EXE_tmcl"))
        .env("APPDATA", sandbox.appdata())
        .args(["task", "watch", &running.id.to_string()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn tmcl");
    std::thread::sleep(Duration::from_millis(1500));
    child.kill().expect("stop the watcher");
    let output = child.wait_with_output().expect("output");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("runs in another process"),
        "a viewer must not pretend to follow another process's task: {stderr}"
    );
}
