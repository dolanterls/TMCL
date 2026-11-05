use std::time::{SystemTime, UNIX_EPOCH};

use tmcl::core::config::migrations::load_and_migrate;
use tmcl::core::config::models::CURRENT_SCHEMA_VERSION;

fn temp_path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "tmcl-migration-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos()
    ))
}

#[test]
fn loads_legacy_fixture_without_rewriting_or_backup() {
    let path = temp_path().join("config.json");
    std::fs::create_dir_all(path.parent().expect("fixture parent")).expect("create test dir");
    std::fs::write(&path, include_str!("../fixtures/config/v0.json")).expect("write fixture");

    let config = load_and_migrate(&path).expect("migrate fixture");
    assert_eq!(config.schema_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(
        config.downloads.version_list_source,
        tmcl::core::download::DownloadSourceMode::Default
    );
    assert_eq!(
        config.downloads.file_download_source,
        tmcl::core::download::DownloadSourceMode::Default
    );
    assert_eq!(config.game_directories.len(), 1);
    assert!(config.java.auto_download);
    assert_eq!(config.logs.retention_days, 14);
    assert_eq!(
        std::fs::read(&path).expect("read original config"),
        include_bytes!("../fixtures/config/v0.json")
    );
    assert!(
        !std::fs::read_dir(path.parent().expect("test dir"))
            .expect("list test dir")
            .any(|entry| entry
                .expect("directory entry")
                .path()
                .extension()
                .is_some_and(|ext| ext == "bak"))
    );

    std::fs::remove_dir_all(path.parent().expect("test dir")).expect("clean test dir");
}

#[test]
fn malformed_json_is_not_overwritten() {
    let path = temp_path().join("config.json");
    std::fs::create_dir_all(path.parent().expect("fixture parent")).expect("create test dir");
    let original = b"{not-json";
    std::fs::write(&path, original).expect("write malformed config");

    assert!(load_and_migrate(&path).is_err());
    assert_eq!(
        std::fs::read(&path).expect("read malformed config"),
        original
    );

    std::fs::remove_dir_all(path.parent().expect("test dir")).expect("clean test dir");
}
