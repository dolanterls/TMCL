//! End-to-end checks of the command surface.
//!
//! These run the real binary so exit codes, stdout and file side effects are
//! covered, not just the library underneath. Each test owns a temporary
//! `APPDATA`, so they never touch the developer's launcher data.

mod support;

use std::path::Path;

use support::Sandbox;

#[test]
fn a_registered_instance_can_be_listed_shown_exported_and_unregistered() {
    let sandbox = Sandbox::new("flow");

    assert!(
        sandbox
            .run(&[
                "config",
                "game-dir",
                "add",
                &sandbox.game().display().to_string()
            ])
            .status
            .success()
    );

    let listed = sandbox.json(&["instance", "list", "--json"]);
    let rows = listed.as_array().expect("array of instances");
    assert_eq!(rows.len(), 1, "{listed}");
    assert_eq!(rows[0]["name"], "demo");
    assert_eq!(rows[0]["status"], "ready");
    assert_eq!(rows[0]["missing_files"], 0);

    let shown = sandbox.json(&["instance", "show", "demo", "--json"]);
    assert_eq!(shown["name"], "demo");
    assert_eq!(shown["minecraft_version"], "1.21.1");
    assert_eq!(shown["loader"], "Vanilla");

    let target = sandbox.root.join("out");
    let exported = sandbox.json(&[
        "instance",
        "export",
        "demo",
        "--output",
        &target.display().to_string(),
        "--json",
    ]);
    let archive = Path::new(exported["path"].as_str().expect("path"));
    assert!(archive.is_file(), "{exported}");

    let removed = sandbox.json(&["instance", "unregister", "demo", "--yes", "--json"]);
    assert_eq!(removed["unregistered"], true);
    assert!(!sandbox.instance().join("tmcl.json").exists());
    // The files stay: unregister only forgets the instance.
    assert!(sandbox.instance().join("demo.jar").is_file());
    assert_eq!(
        sandbox.json(&["instance", "list", "--json"]),
        serde_json::json!([])
    );
}

#[test]
fn delete_refuses_without_confirmation_and_keeps_the_instance() {
    let sandbox = Sandbox::new("delete-refused");
    assert!(
        sandbox
            .run(&[
                "config",
                "game-dir",
                "add",
                &sandbox.game().display().to_string()
            ])
            .status
            .success()
    );

    let output = sandbox.run(&["instance", "delete", "demo"]);

    assert_eq!(output.status.code(), Some(2), "config errors exit with 2");
    let message = String::from_utf8_lossy(&output.stderr);
    assert!(message.contains("--yes"), "{message}");
    assert!(message.contains("--dry-run"), "{message}");
    assert!(sandbox.instance().is_dir(), "nothing may be deleted");
}

#[test]
fn delete_with_yes_removes_the_directory() {
    let sandbox = Sandbox::new("delete-yes");
    assert!(
        sandbox
            .run(&[
                "config",
                "game-dir",
                "add",
                &sandbox.game().display().to_string()
            ])
            .status
            .success()
    );

    let deleted = sandbox.json(&["instance", "delete", "demo", "--yes", "--json"]);

    assert_eq!(deleted["deleted"], true);
    assert!(!sandbox.instance().exists());
}

#[test]
fn an_unknown_reference_fails_with_a_helpful_message() {
    let sandbox = Sandbox::new("missing");

    let output = sandbox.run(&["instance", "show", "not-here"]);

    assert_eq!(output.status.code(), Some(2));
    let message = String::from_utf8_lossy(&output.stderr);
    assert!(message.contains("not found"), "{message}");
    assert!(message.contains("searched"), "{message}");
}

#[test]
fn config_get_and_set_round_trip_through_the_file() {
    let sandbox = Sandbox::new("config");

    let printed = sandbox.json(&["config", "get", "downloads.retry_count", "--json"]);
    assert!(printed.is_number(), "{printed}");

    sandbox.json(&["config", "set", "downloads.retry_count", "7", "--json"]);
    let reread = sandbox.json(&["config", "get", "downloads.retry_count", "--json"]);
    assert_eq!(reread, serde_json::json!(7));

    let unknown = sandbox.run(&["config", "set", "downloads.retry_cuont", "1"]);
    assert_eq!(unknown.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&unknown.stderr).contains("retry_count"),
        "a typo should suggest the real key"
    );
}

#[test]
fn list_status_separates_ready_from_incomplete() {
    let sandbox = Sandbox::new("status");
    sandbox.run(&[
        "config",
        "game-dir",
        "add",
        &sandbox.game().display().to_string(),
    ]);

    // A second instance whose version JSON names a library the disk lacks.
    let broken = sandbox.game().join("versions").join("broken");
    std::fs::create_dir_all(&broken).expect("broken dir");
    std::fs::write(broken.join("broken.jar"), b"jar").expect("jar");
    std::fs::write(
        broken.join("broken.json"),
        br#"{"id":"1.21.1","mainClass":"net.minecraft.client.main.Main","libraries":[{"name":"org.example:missing:1.0","downloads":{"artifact":{"path":"libraries/org/example/missing/1.0/missing-1.0.jar","url":"http://example/m.jar","size":1}}}],"arguments":{"game":[],"jvm":[]}}"#,
    )
    .expect("version json");

    let incomplete = sandbox.json(&["instance", "list", "--status", "incomplete", "--json"]);
    let rows = incomplete.as_array().expect("array");
    assert_eq!(rows.len(), 1, "{incomplete}");
    assert_eq!(rows[0]["name"], "broken");
    assert_eq!(rows[0]["status"], "incomplete");
    assert_eq!(rows[0]["missing_files"], 1);

    let ready = sandbox.json(&["instance", "list", "--status", "ready", "--json"]);
    let rows = ready.as_array().expect("array");
    assert_eq!(rows.len(), 1, "{ready}");
    assert_eq!(rows[0]["name"], "demo");
}

#[test]
fn rename_moves_the_directory_and_updates_the_metadata() {
    let sandbox = Sandbox::new("rename");
    sandbox.run(&[
        "config",
        "game-dir",
        "add",
        &sandbox.game().display().to_string(),
    ]);

    let renamed = sandbox.json(&["instance", "rename", "demo", "survival", "--json"]);

    assert_eq!(renamed["name"], "survival");
    assert_eq!(renamed["old_name"], "demo");
    assert!(
        !sandbox.instance().exists(),
        "the old directory must be gone after a rename"
    );
    let moved = sandbox.game().join("versions").join("survival");
    assert!(
        moved.join("survival.jar").is_file(),
        "version files follow the name"
    );
    let metadata = std::fs::read_to_string(moved.join("tmcl.json")).expect("metadata");
    assert!(metadata.contains("\"survival\""), "{metadata}");
    assert_eq!(
        sandbox.json(&["instance", "show", "survival", "--json"])["name"],
        "survival"
    );
}

#[test]
fn import_adopts_a_version_directory_under_a_new_name() {
    let sandbox = Sandbox::new("import");
    sandbox.run(&[
        "config",
        "game-dir",
        "add",
        &sandbox.game().display().to_string(),
    ]);
    let source = sandbox.game().join("versions").join("imported");
    std::fs::create_dir_all(&source).expect("source dir");
    std::fs::write(source.join("imported.jar"), b"jar").expect("jar");
    std::fs::write(source.join("imported.json"), support::CLIENT).expect("client json");

    let imported = sandbox.json(&[
        "instance",
        "import",
        &source.display().to_string(),
        "--version",
        "1.21.1",
        "--name",
        "adopted",
        "--json",
    ]);

    assert_eq!(imported["name"], "adopted");
    let target = sandbox.game().join("versions").join("adopted");
    assert!(
        target.join("adopted.jar").is_file(),
        "files are renamed with it"
    );
    let metadata = std::fs::read_to_string(target.join("tmcl.json")).expect("metadata");
    assert!(metadata.contains("\"adopted\""), "{metadata}");
}

#[test]
fn verify_counts_existing_files_and_names_missing_ones() {
    let sandbox = Sandbox::new("verify");
    sandbox.run(&[
        "config",
        "game-dir",
        "add",
        &sandbox.game().display().to_string(),
    ]);
    let library = sandbox
        .game()
        .join("libraries")
        .join("org/example/present/1.0/present-1.0.jar");
    std::fs::create_dir_all(library.parent().expect("parent")).expect("library dir");
    std::fs::write(&library, b"lib").expect("library");
    std::fs::write(
        sandbox.instance().join("demo.json"),
        br#"{"id":"1.21.1","mainClass":"net.minecraft.client.main.Main","libraries":[{"name":"org.example:present:1.0","downloads":{"artifact":{"path":"org/example/present/1.0/present-1.0.jar","url":"http://example/p.jar","size":3}}}],"arguments":{"game":[],"jvm":[]}}"#,
    )
    .expect("version json");

    let intact = sandbox.json(&["instance", "verify", "demo", "--json"]);
    assert_eq!(intact["checked"], 1, "{intact}");
    assert_eq!(intact["missing"], serde_json::json!([]), "{intact}");

    std::fs::remove_file(&library).expect("remove library");
    let broken = sandbox.json(&["instance", "verify", "demo", "--json"]);
    assert_eq!(broken["checked"], 0, "{broken}");
    let missing = broken["missing"].as_array().expect("array");
    assert_eq!(missing.len(), 1, "{broken}");
    assert_eq!(missing[0]["reason"], "library org.example:present:1.0");
}

#[test]
fn java_scan_reports_a_count_and_the_runtimes_it_found() {
    let sandbox = Sandbox::new("java-scan");

    let scan = sandbox.json(&["java", "scan", "--json"]);

    let count = scan["count"].as_u64().expect("count");
    let runtimes = scan["runtimes"].as_array().expect("runtimes");
    assert_eq!(count, runtimes.len() as u64, "{scan}");
    for runtime in runtimes {
        assert!(runtime["major"].is_u64(), "{scan}");
        assert!(runtime["path"].is_string(), "{scan}");
    }
}

#[test]
fn export_refuses_to_overwrite_an_existing_archive() {
    let sandbox = Sandbox::new("export-overwrite");
    sandbox.run(&[
        "config",
        "game-dir",
        "add",
        &sandbox.game().display().to_string(),
    ]);
    let target = sandbox.root.join("out.zip");
    std::fs::write(&target, b"first").expect("existing archive");

    let refused = sandbox.run(&[
        "instance",
        "export",
        "demo",
        "--output",
        &target.display().to_string(),
    ]);

    assert_eq!(refused.status.code(), Some(2), "a config error exits 2");
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("already exists"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(
        std::fs::read(&target).expect("original archive"),
        b"first",
        "the existing file must not be replaced"
    );
}

#[test]
fn exporting_a_launch_script_reports_the_file() {
    let sandbox = Sandbox::new("export-script");
    sandbox.run(&[
        "config",
        "game-dir",
        "add",
        &sandbox.game().display().to_string(),
    ]);
    // Building the command probes a real runtime. Reuse whatever this machine
    // has instead of assuming a major version; skip where none is installed.
    let javas = sandbox.json(&["java", "list", "--json"]);
    let Some(java) = javas
        .as_array()
        .and_then(|list| list.first())
        .and_then(|entry| entry["executable"].as_str())
    else {
        eprintln!("skipping: no Java runtime on this machine");
        return;
    };
    let script = sandbox.root.join("launch.bat");

    let printed = sandbox.json(&[
        "instance",
        "launch",
        "demo",
        "--java",
        java,
        "--export-script",
        &script.display().to_string(),
        "--json",
    ]);

    assert_eq!(printed["format"], "bat", "{printed}");
    assert!(printed["warnings"].is_array(), "{printed}");
    assert_eq!(Path::new(printed["path"].as_str().expect("path")), script);
    let contents = std::fs::read_to_string(&script).expect("script");
    assert!(contents.starts_with("@echo off"), "{contents}");
}

#[test]
fn config_set_accepts_the_enum_spelling_and_rejects_anything_else() {
    let sandbox = Sandbox::new("config-enum");

    let set = sandbox.json(&[
        "config",
        "set",
        "downloads.file_download_source",
        "MIRROR",
        "--json",
    ]);
    assert_eq!(set, "MIRROR");
    assert_eq!(
        sandbox.json(&["config", "get", "downloads.file_download_source", "--json"]),
        "MIRROR"
    );

    let refused = sandbox.run(&["config", "set", "downloads.file_download_source", "mirror"]);
    assert_eq!(refused.status.code(), Some(2));
    let message = String::from_utf8_lossy(&refused.stderr);
    assert!(message.contains("MIRROR"), "{message}");
    assert!(message.contains("DEFAULT"), "{message}");
}

#[test]
fn a_game_directory_can_be_removed_again() {
    let sandbox = Sandbox::new("game-dir-remove");
    let game = sandbox.game().display().to_string();

    sandbox.run(&["config", "game-dir", "add", &game]);
    assert_eq!(
        sandbox.json(&["config", "game-dir", "list", "--json"]),
        serde_json::json!([game.clone()])
    );

    let removed = sandbox.run(&["config", "game-dir", "remove", &game]);
    assert!(
        removed.status.success(),
        "{}",
        String::from_utf8_lossy(&removed.stderr)
    );
    assert_eq!(
        sandbox.json(&["config", "game-dir", "list", "--json"]),
        serde_json::json!([])
    );
}

#[test]
fn java_scan_and_java_list_report_the_same_runtimes() {
    let sandbox = Sandbox::new("java-agree");

    let scan = sandbox.json(&["java", "scan", "--json"]);
    let list = sandbox.json(&["java", "list", "--json"]);

    fn comparable(path: &str) -> String {
        path.trim_start_matches(r"\\?\")
            .replace('/', "\\")
            .to_ascii_lowercase()
    }

    let scanned = scan["runtimes"]
        .as_array()
        .expect("runtimes")
        .iter()
        .filter_map(|runtime| runtime["path"].as_str())
        .map(comparable)
        .collect::<std::collections::BTreeSet<_>>();
    let listed = list
        .as_array()
        .expect("installations")
        .iter()
        .filter_map(|runtime| runtime["executable"].as_str())
        .map(comparable)
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(scanned, listed, "scan and list must agree");
}

#[test]
fn repairing_a_complete_instance_downloads_nothing() {
    let sandbox = Sandbox::new("repair-complete");
    assert!(
        sandbox
            .run(&[
                "config",
                "game-dir",
                "add",
                &sandbox.game().display().to_string()
            ])
            .status
            .success()
    );

    let report = sandbox.json(&["instance", "repair", "demo", "--json"]);

    assert_eq!(report["missing"], 0, "{report}");
    assert_eq!(report["downloaded"], 0, "{report}");
}

#[test]
fn an_instance_can_be_addressed_by_its_list_number() {
    let sandbox = Sandbox::new("index");
    assert!(
        sandbox
            .run(&[
                "config",
                "game-dir",
                "add",
                &sandbox.game().display().to_string()
            ])
            .status
            .success()
    );

    let listed = sandbox.json(&["instance", "list", "--json"]);
    assert_eq!(listed[0]["index"], 1, "{listed}");

    let report = sandbox.json(&["instance", "show", "1", "--json"]);
    assert_eq!(report["name"], "demo", "{report}");
    // `show` names one instance; a position would be meaningless there.
    assert!(report.get("index").is_none(), "{report}");
}
