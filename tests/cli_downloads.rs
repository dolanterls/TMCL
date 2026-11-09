//! End-to-end checks of the download centre.
//!
//! `search`, `project` and `install` talk to Modrinth by design, so the offline
//! assertions here cover what can be decided without a network: the source
//! availability check, argument guards, and the plan shape. Modpack installation is
//! different: a pack is a local file listing URLs, so a crafted `.mrpack` whose URLs
//! point at a local server exercises the whole path, downloads included.

mod support;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use support::Server;

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let root = support::sandbox_root(&format!("tmcl-downloads-{tag}"));
        support::install_launcher(&root);
        let instance = root.join("game").join("versions").join("demo");
        std::fs::create_dir_all(&instance).expect("instance dir");
        std::fs::write(instance.join("demo.jar"), b"jar").expect("jar");
        std::fs::write(
            instance.join("demo.json"),
            r#"{"id":"1.21.1","mainClass":"net.minecraft.client.main.Main","arguments":{"game":[],"jvm":[]}}"#,
        )
        .expect("client json");
        std::fs::write(
            instance.join("tmcl.json"),
            br#"{"id":"11111111-1111-4111-8111-111111111111","name":"demo","game_directory_id":"22222222-2222-4222-8222-222222222222","minecraft_version":"1.21.1","loader":"Vanilla","stats":{"launch_count":0,"play_time_seconds":0}}"#,
        )
        .expect("metadata");
        let sandbox = Self { root };
        let status = sandbox
            .run(&[
                "config",
                "game-dir",
                "add",
                &sandbox.game().display().to_string(),
                "--json",
            ])
            .status;
        assert!(status.success(), "registering the game directory failed");
        sandbox
    }

    fn game(&self) -> PathBuf {
        self.root.join("game")
    }

    fn instance(&self) -> PathBuf {
        self.game().join("versions").join("demo")
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(support::launcher_at(&self.root))
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
        serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
            panic!(
                "{} did not print JSON ({error}): {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stdout)
            )
        })
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A one-file Modrinth-format pack. `dependencies` decides the loader.
fn write_pack(path: &Path, dependencies: &str, file_path: &str, url: &str) {
    let index = format!(
        r#"{{"formatVersion":1,"game":"minecraft","versionId":"1.0.0","name":"Demo","dependencies":{dependencies},"files":[{{"path":"{file_path}","hashes":{{}},"downloads":["{url}"],"fileSize":4}}]}}"#
    );
    let file = std::fs::File::create(path).expect("create pack");
    let mut zip = zip::ZipWriter::new(file);
    zip.start_file(
        "modrinth.index.json",
        zip::write::SimpleFileOptions::default(),
    )
    .expect("index");
    zip.write_all(index.as_bytes()).expect("write index");
    zip.finish().expect("finish");
}

#[test]
fn a_source_without_an_api_key_explains_itself() {
    let sandbox = Sandbox::new("no-key");

    // CurseForge needs a key compiled in; without one the provider says so
    // before a single request is made.
    let output = sandbox.run(&["search", "sodium", "--source", "curseforge", "--json"]);

    assert_eq!(output.status.code(), Some(3), "download errors exit with 3");
    let message = String::from_utf8_lossy(&output.stderr);
    assert!(message.contains("CurseForge"), "{message}");
}

#[test]
fn installing_a_pack_kind_points_at_the_modpack_command() {
    let sandbox = Sandbox::new("pack-kind");

    let output = sandbox.run(&["install", "some-pack", "--kind", "modpack"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("tmcl modpack install"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn updating_a_pack_instance_that_has_no_manifest_is_refused() {
    let sandbox = Sandbox::new("no-manifest");

    let output = sandbox.run(&["modpack", "update", "demo", "--from", "whatever.mrpack"]);

    assert_eq!(output.status.code(), Some(2));
    let message = String::from_utf8_lossy(&output.stderr);
    assert!(
        message.contains("not installed from a modpack"),
        "{message}"
    );
}

#[test]
fn a_local_pack_is_planned_without_writing_anything() {
    let sandbox = Sandbox::new("pack-plan");
    let archive = sandbox.root.join("demo-1.0.0.mrpack");
    write_pack(
        &archive,
        r#"{"minecraft":"1.21.1","fabric-loader":"0.16.5"}"#,
        "mods/demo.jar",
        "http://127.0.0.1:9/never",
    );

    let planned = sandbox.json(&[
        "modpack",
        "install",
        &archive.display().to_string(),
        "--name",
        "packed",
        "--dry-run",
        "--json",
    ]);

    assert_eq!(planned["applied"], false, "{planned}");
    assert_eq!(planned["plan"]["files"], 1, "{planned}");
    assert_eq!(planned["plan"]["minecraft_version"], "1.21.1", "{planned}");
    assert_eq!(planned["plan"]["loader"], "fabric", "{planned}");
    assert!(
        !sandbox.game().join("versions").join("packed").exists(),
        "a dry run must not create the instance"
    );
}

#[test]
fn a_local_pack_install_creates_an_instance_and_downloads_its_files() {
    let sandbox = Sandbox::new("pack-install");
    let server = Server::start(vec![("/pack.jar", b"mod!", "application/java-archive")]);
    let archive = sandbox.root.join("demo-1.0.0.mrpack");
    write_pack(
        &archive,
        r#"{"minecraft":"1.21.1","fabric-loader":"0.16.5"}"#,
        "mods/demo.jar",
        &server.url("/pack.jar"),
    );

    let installed = sandbox.json(&[
        "modpack",
        "install",
        &archive.display().to_string(),
        "--name",
        "packed",
        "--json",
    ]);

    assert_eq!(installed["name"], "packed");
    assert_eq!(installed["files"], 1, "{installed}");
    let instance = sandbox.game().join("versions").join("packed");
    assert!(instance.join("tmcl.json").is_file(), "metadata written");
    assert_eq!(
        std::fs::read(instance.join("mods/demo.jar")).expect("downloaded"),
        b"mod!"
    );
    let manifest = std::fs::read(instance.join(".tmcl-pack.json")).expect("manifest");
    assert!(
        String::from_utf8_lossy(&manifest).contains("mods/demo.jar"),
        "the pack's files are recorded so an update can replace them"
    );
    // The user's archive is untouched; only our staging copy is consumed.
    assert!(archive.is_file());
}

#[test]
fn applying_a_second_pack_replaces_the_first_packs_files() {
    let sandbox = Sandbox::new("pack-apply");
    let first_server = Server::start(vec![("/pack.jar", b"one!", "application/java-archive")]);
    let first = sandbox.root.join("first.mrpack");
    write_pack(
        &first,
        r#"{"minecraft":"1.21.1"}"#,
        "mods/one.jar",
        &first_server.url("/pack.jar"),
    );
    sandbox.json(&[
        "modpack",
        "install",
        &first.display().to_string(),
        "--instance",
        "demo",
        "--json",
    ]);
    assert!(sandbox.instance().join("mods/one.jar").is_file());

    let second_server = Server::start(vec![("/pack.jar", b"two!", "application/java-archive")]);
    let second = sandbox.root.join("second.mrpack");
    write_pack(
        &second,
        r#"{"minecraft":"1.21.1"}"#,
        "mods/two.jar",
        &second_server.url("/pack.jar"),
    );
    let applied = sandbox.json(&[
        "modpack",
        "update",
        "demo",
        "--from",
        &second.display().to_string(),
        "--json",
    ]);

    assert_eq!(applied["files"], 1, "{applied}");
    assert_eq!(applied["removed"], 1, "the pack dropped one.jar");
    assert!(sandbox.instance().join("mods/two.jar").is_file());
    assert!(
        !sandbox.instance().join("mods/one.jar").exists(),
        "the old pack file is gone"
    );
    // Removed, not destroyed: it is one `trash restore` away.
    let trashed = sandbox.json(&["trash", "list", "demo", "--json"]);
    assert!(
        trashed
            .as_array()
            .expect("array")
            .iter()
            .any(|row| row["name"] == "one.jar"),
        "{trashed}"
    );
}
