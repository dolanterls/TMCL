use std::path::{Path, PathBuf};

use serde_json::json;
use tmcl::core::task::store::JsonTaskStore;
use tmcl::core::task::{
    JavaRuntimeSpec, PersistedTaskRecord, TaskOperation, TaskRecord, TaskState,
};
use uuid::Uuid;

fn temp_root() -> PathBuf {
    std::env::temp_dir().join(format!("tmcl-task-store-{}", Uuid::new_v4()))
}

fn java_operation() -> TaskOperation {
    TaskOperation::JavaRuntime(JavaRuntimeSpec {
        data_root: PathBuf::from("runtime"),
        major: 25,
        download_snapshot: Default::default(),
    })
}

fn write_json(path: &Path, value: serde_json::Value) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&value).expect("serialize fixture"),
    )
    .expect("write fixture");
}

#[test]
fn new_records_round_trip_and_are_written_as_schema_two() {
    let root = temp_root();
    let store = JsonTaskStore::new(root.join("tasks"));
    let id = Uuid::new_v4();
    let task = TaskRecord::new(id, java_operation(), 10);

    store.upsert(&task).expect("persist task");
    let records = store.load().expect("load task");

    assert_eq!(records, vec![task]);
    let value: PersistedTaskRecord = serde_json::from_slice(
        &std::fs::read(root.join("tasks").join(format!("{id}.json"))).expect("read record"),
    )
    .expect("schema envelope");
    assert_eq!(value.schema_version, 2);

    std::fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn duplicate_ids_are_collapsed_using_the_newer_record() {
    let root = temp_root();
    let directory = root.join("tasks");
    let id = Uuid::new_v4();
    let older = TaskRecord::new(id, java_operation(), 10);
    let mut newer = TaskRecord::new(id, java_operation(), 20);
    newer.start().expect("start newer");

    write_json(
        &directory.join("a.json"),
        serde_json::to_value(older.persisted()).expect("serialize older"),
    );
    write_json(
        &directory.join("b.json"),
        serde_json::to_value(newer.persisted()).expect("serialize newer"),
    );

    let records = JsonTaskStore::new(directory).load().expect("load records");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].updated_at, newer.updated_at);
    assert_eq!(records[0].state, TaskState::Running);

    std::fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn legacy_java_records_are_loaded_as_unrecoverable_without_rewrite() {
    let root = temp_root();
    let directory = root.join("tasks");
    let id = Uuid::new_v4();
    let value = json!({
        "id": id,
        "instance_id": Uuid::new_v4(),
        "label": "Install Mojang Java 25",
        "kind": {"JavaRuntime": {"major": 25}},
        "state": "Failed",
        "attempt": 2,
        "completed": 12,
        "total": 100,
        "failure_message": "network",
        "paused": false,
        "downloaded_files": ["runtime/.part"],
        "referenced_files": [],
        "retryable_failure": true
    });
    write_json(&directory.join(format!("{id}.json")), value);

    let store = JsonTaskStore::new(directory.clone());
    let original = std::fs::read(directory.join(format!("{id}.json"))).expect("read original");
    let records = store.load().expect("load legacy Java task");

    assert_eq!(records.len(), 1);
    assert!(matches!(
        records[0].operation,
        TaskOperation::Unrecoverable { .. }
    ));
    assert!(!records[0].retryable);
    assert_eq!(
        std::fs::read(directory.join(format!("{id}.json"))).expect("read after"),
        original
    );
    assert!(!directory.join(format!("{id}.json.legacy")).exists());

    std::fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn legacy_instance_without_context_is_preserved_but_not_retryable() {
    let root = temp_root();
    let directory = root.join("tasks");
    let id = Uuid::new_v4();
    let instance_id = Uuid::new_v4();
    write_json(
        &directory.join(format!("{id}.json")),
        json!({
            "id": id,
            "instance_id": instance_id,
            "label": "Install demo (1.21.1, neoforge)",
            "kind": "InstanceInstall",
            "state": "Failed",
            "retryable_failure": true,
            "downloaded_files": [],
            "referenced_files": []
        }),
    );

    let records = JsonTaskStore::new(directory)
        .load()
        .expect("load legacy record");
    assert_eq!(records.len(), 1);
    assert!(matches!(
        records[0].operation,
        TaskOperation::Unrecoverable { .. }
    ));
    assert!(!records[0].retryable);
    assert!(
        records[0]
            .failure
            .as_deref()
            .is_some_and(|message| message.contains("重新创建"))
    );

    std::fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn legacy_instance_with_context_is_not_reconstructed() {
    let root = temp_root();
    let directory = root.join("tasks");
    let id = Uuid::new_v4();
    let instance_id = Uuid::new_v4();
    write_json(
        &directory.join(format!("{id}.json")),
        json!({
            "id": id,
            "instance_id": instance_id,
            "label": "Install demo (1.21.1, neoforge)",
            "kind": "InstanceInstall",
            "state": "Failed",
            "retryable_failure": true,
            "downloaded_files": [],
            "referenced_files": []
        }),
    );

    let records = JsonTaskStore::new(directory)
        .load()
        .expect("load legacy instance task");

    assert_eq!(records.len(), 1);
    assert!(matches!(
        records[0].operation,
        TaskOperation::Unrecoverable { .. }
    ));
    assert!(!records[0].retryable);

    std::fs::remove_dir_all(root).expect("cleanup");
}
