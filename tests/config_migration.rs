use std::sync::atomic::{AtomicU32, Ordering};

use tmcl::core::config::migrations::load_and_migrate;
use tmcl::core::config::models::CURRENT_SCHEMA_VERSION;

/// A folder no other test in this binary can pick.
///
/// A timestamp alone is not enough: the Windows clock ticks in milliseconds, so
/// two tests that start together got the same path and one deleted the other's
/// fixture mid-run. The counter is per process and the pid separates binaries.
fn temp_path() -> std::path::PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    std::env::temp_dir().join(format!(
        "tmcl-migration-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
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
    // `diagnosis` is newer than the fixture; the container-level
    // `#[serde(default)]` has to fill it in, and the default is "on".
    assert!(config.diagnosis.auto_open);
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

/// A player who turns the card off must stay off: the field is written back
/// like any other, and reading it again must not resurrect the default.
#[test]
fn the_diagnosis_switch_survives_a_round_trip() {
    use tmcl::core::config::models::Config;

    let mut config = Config::default();
    config.diagnosis.auto_open = false;
    let json = serde_json::to_string(&config).expect("serialize");
    let back: Config = serde_json::from_str(&json).expect("deserialize");
    assert!(!back.diagnosis.auto_open);
}
