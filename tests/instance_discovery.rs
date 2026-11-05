use std::time::{SystemTime, UNIX_EPOCH};

use tmcl::core::instance::{
    InstanceLoader, delete, discover, path::normalize_game_directory, registry, rename, service,
};
use uuid::Uuid;

fn temp_root() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "tmcl-instance-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos()
    ))
}

#[test]
fn discovers_only_complete_version_entries_and_child_candidates() {
    let root = temp_root();
    let complete = root.join("versions/1.21.1");
    let incomplete = root.join("versions/1.20.6");
    std::fs::create_dir_all(&complete).expect("create complete instance");
    std::fs::create_dir_all(&incomplete).expect("create incomplete instance");
    std::fs::write(complete.join("1.21.1.jar"), b"jar").expect("jar");
    std::fs::write(complete.join("1.21.1.json"), b"{}").expect("json");
    std::fs::write(incomplete.join("1.20.6.jar"), b"jar").expect("jar");

    let result = discover(&root).expect("discover instances");
    assert_eq!(result.instances.len(), 1);
    assert_eq!(result.instances[0].id, "1.21.1");
    assert!(result.child_candidates.is_empty());
    std::fs::remove_dir_all(&root).expect("clean test root");

    let parent = temp_root();
    std::fs::create_dir_all(parent.join(".minecraft")).expect("create child candidate");
    let result = discover(&parent).expect("discover candidate");
    assert_eq!(
        result.child_candidates,
        vec![
            normalize_game_directory(&parent)
                .expect("normalized parent")
                .join(".minecraft")
        ]
    );
    std::fs::remove_dir_all(parent).expect("clean candidate root");
}

#[test]
fn first_scan_creates_metadata_and_second_scan_keeps_uuid_and_unknown_fields() {
    let root = temp_root();
    let instance_dir = root.join("versions/1.21.1");
    std::fs::create_dir_all(&instance_dir).expect("create instance");
    std::fs::write(instance_dir.join("1.21.1.jar"), b"jar").expect("jar");
    std::fs::write(instance_dir.join("1.21.1.json"), b"{}").expect("json");
    let game_directory_id = Uuid::new_v4();

    let first = registry::scan(&root, game_directory_id).expect("first scan");
    let id = first[0].id;
    assert!(instance_dir.join("tmcl.json").is_file());

    let mut json: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(instance_dir.join("tmcl.json")).expect("metadata"),
    )
    .expect("metadata JSON");
    json["future_field"] = serde_json::json!("preserved");
    std::fs::write(
        instance_dir.join("tmcl.json"),
        serde_json::to_vec_pretty(&json).expect("serialize metadata"),
    )
    .expect("update metadata");

    let second = registry::scan(&root, game_directory_id).expect("second scan");
    assert_eq!(second[0].id, id);
    assert_eq!(second[0].extra["future_field"], "preserved");
    assert!(matches!(second[0].loader, InstanceLoader::Vanilla));
    std::fs::remove_dir_all(root).expect("clean test root");
}

#[test]
fn create_rename_import_and_delete_keep_game_data_safe() {
    let root = temp_root();
    let game_id = Uuid::new_v4();
    std::fs::create_dir_all(&root).expect("create game directory");
    let created =
        service::create_instance(&root, game_id, "survival", "1.21.1").expect("create instance");
    let old_dir = created.path.clone();
    std::fs::write(old_dir.join("survival.jar"), b"jar").expect("client jar");
    std::fs::write(old_dir.join("survival.json"), b"{}").expect("client json");
    std::fs::create_dir_all(old_dir.join("saves")).expect("saves");
    std::fs::create_dir_all(old_dir.join("mods")).expect("mods");

    let renamed = rename::rename_instance(&old_dir, "renamed").expect("rename instance");
    assert_eq!(renamed.id, created.id);
    assert!(renamed.path.join("renamed.jar").is_file());
    assert!(renamed.path.join("renamed.json").is_file());
    assert!(renamed.path.join("saves").is_dir());

    let imported =
        service::import_instance(&renamed.path, game_id, "1.21.1").expect("import instance");
    assert_eq!(imported.id, renamed.id);
    delete::delete_metadata(&root, &renamed.path).expect("delete metadata");
    assert!(!renamed.path.join("tmcl.json").exists());
    assert!(renamed.path.join("saves").is_dir());
    assert!(renamed.path.join("mods").is_dir());
    assert!(service::validate_name("../escape").is_err());
    assert!(delete::delete_metadata(&root, &root).is_err());

    std::fs::remove_dir_all(root).expect("clean test root");
}
