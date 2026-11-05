//! Dry-run must describe without touching anything.
//!
//! Every assertion here checks two things at once: the plan is printed as JSON,
//! and the world is exactly as it was before the command ran.

use std::path::PathBuf;
use std::process::{Command, Output};

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("tmcl-cli-dry-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("appdata")).expect("appdata");
        std::fs::create_dir_all(root.join("game").join("versions").join("demo"))
            .expect("instance dir");
        // `discover` only accepts a directory that has both the jar and the
        // version JSON, so an "incomplete" fixture still needs the jar.
        std::fs::write(
            root.join("game")
                .join("versions")
                .join("demo")
                .join("demo.jar"),
            b"jar",
        )
        .expect("jar");
        std::fs::write(
            root.join("game").join("versions").join("demo").join("demo.json"),
            br#"{"id":"1.21.1","mainClass":"net.minecraft.client.main.Main","arguments":{"game":[],"jvm":[]}}"#,
        )
        .expect("client json");
        std::fs::write(
            root.join("game").join("versions").join("demo").join("tmcl.json"),
            br#"{"id":"11111111-1111-4111-8111-111111111111","name":"demo","game_directory_id":"22222222-2222-4222-8222-222222222222","minecraft_version":"1.21.1","loader":"Vanilla","stats":{"launch_count":0,"play_time_seconds":0}}"#,
        )
        .expect("metadata");
        Self { root }
    }

    fn game(&self) -> PathBuf {
        self.root.join("game")
    }

    fn instance(&self) -> PathBuf {
        self.game().join("versions").join("demo")
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_tmcl"))
            .env("APPDATA", self.root.join("appdata"))
            .args(args)
            .output()
            .expect("run tmcl")
    }

    fn json(&self, args: &[&str]) -> serde_json::Value {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "{} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("JSON output")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn delete_dry_run_prints_the_plan_and_keeps_every_file() {
    let sandbox = Sandbox::new("delete");
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

    let plan = sandbox.json(&["instance", "delete", "demo", "--dry-run", "--json"]);

    assert_eq!(plan["applied"], false);
    assert!(
        plan["plan"]
            .as_str()
            .is_some_and(|text| text.contains("delete")),
        "{plan}"
    );
    assert!(sandbox.instance().join("tmcl.json").is_file());
}

#[test]
fn unregister_dry_run_keeps_the_metadata() {
    let sandbox = Sandbox::new("unregister");
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

    let plan = sandbox.json(&["instance", "unregister", "demo", "--dry-run", "--json"]);

    assert_eq!(plan["applied"], false);
    assert!(sandbox.instance().join("tmcl.json").is_file());
}

#[test]
fn repair_dry_run_lists_the_missing_files() {
    let sandbox = Sandbox::new("repair");
    // A library the version JSON names but the disk lacks is what makes the
    // instance incomplete; the client jar has to stay for discovery to work.
    std::fs::write(
        sandbox.instance().join("demo.json"),
        br#"{"id":"1.21.1","mainClass":"net.minecraft.client.main.Main","libraries":[{"name":"example:lib:1.0","downloads":{"artifact":{"path":"libraries/example/lib-1.0.jar","url":"https://example.invalid/lib.jar","sha1":"0000000000000000000000000000000000000000","size":1}}}],"arguments":{"game":[],"jvm":[]}}"#,
    )
    .expect("client json with a library");
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

    let plan = sandbox.json(&["instance", "repair", "demo", "--dry-run", "--json"]);

    assert_eq!(plan["applied"], false);
    let missing = plan["missing"].as_u64().expect("missing count");
    assert!(missing >= 1, "{plan}");
    assert!(
        !sandbox
            .game()
            .join("libraries")
            .join("example")
            .join("lib-1.0.jar")
            .exists(),
        "dry-run must not download"
    );
}

#[test]
fn export_dry_run_writes_no_archive() {
    let sandbox = Sandbox::new("export");
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
    let target = sandbox.root.join("pack");

    let plan = sandbox.json(&[
        "instance",
        "export",
        "demo",
        "--output",
        &target.display().to_string(),
        "--dry-run",
        "--json",
    ]);

    assert_eq!(plan["applied"], false);
    assert!(!target.with_extension("zip").exists());
}

#[test]
fn create_dry_run_writes_no_instance() {
    let sandbox = Sandbox::new("create");
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

    let plan = sandbox.json(&[
        "instance",
        "create",
        "fresh",
        "--version",
        "1.21.1",
        "--dry-run",
        "--json",
    ]);

    assert_eq!(plan["applied"], false);
    assert!(!sandbox.game().join("versions").join("fresh").exists());
}
