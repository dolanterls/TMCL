//! End-to-end checks of `content`, `trash`, `world` and `mod`.
//!
//! The binary is run for real, so what is asserted is what a user sees: exit codes,
//! stdout shape and the files left on disk. Each test owns a folder with a launcher
//! of its own, and the launcher keeps its data next to itself.

mod support;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const CLIENT: &str = r#"{"id":"1.21.1","mainClass":"net.minecraft.client.main.Main","arguments":{"game":[],"jvm":[]}}"#;

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let root = support::sandbox_root(&format!("tmcl-content-{tag}"));
        support::install_launcher(&root);
        let instance = root.join("game").join("versions").join("demo");
        std::fs::create_dir_all(&instance).expect("instance dir");
        std::fs::write(instance.join("demo.jar"), b"jar").expect("jar");
        std::fs::write(instance.join("demo.json"), CLIENT).expect("client json");
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

    /// Path inside the instance.
    fn path(&self, relative: &str) -> PathBuf {
        self.instance().join(relative)
    }

    fn write(&self, relative: &str, contents: &[u8]) -> PathBuf {
        let path = self.path(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("parent dir");
        std::fs::write(&path, contents).expect("write");
        path
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

/// Root compound → `Data` with a name, a version, a timestamp and a game mode.
fn level_dat(name: &str) -> Vec<u8> {
    fn tag(out: &mut Vec<u8>, kind: u8, key: &str) {
        out.push(kind);
        out.extend((key.len() as u16).to_be_bytes());
        out.extend(key.as_bytes());
    }
    fn string(out: &mut Vec<u8>, value: &str) {
        out.extend((value.len() as u16).to_be_bytes());
        out.extend(value.as_bytes());
    }

    let mut data = Vec::new();
    tag(&mut data, 8, "LevelName");
    string(&mut data, name);
    tag(&mut data, 4, "LastPlayed");
    // 2023-11-14T22:13:20Z, in milliseconds, which is what the game writes.
    data.extend(1_700_000_000_000i64.to_be_bytes());
    tag(&mut data, 3, "GameType");
    data.extend(1i32.to_be_bytes());
    tag(&mut data, 10, "Version");
    tag(&mut data, 8, "Name");
    string(&mut data, "1.20.1");
    data.push(0);
    data.push(0);

    let mut root = vec![10];
    root.extend(0u16.to_be_bytes());
    tag(&mut root, 10, "Data");
    root.extend(data);
    root.push(0);
    root
}

#[test]
fn content_list_matches_the_workspace_order_and_filters_case_insensitively() {
    let sandbox = Sandbox::new("list");
    sandbox.write("mods/zeta.jar", b"z");
    sandbox.write("mods/alpha.jar", b"a");
    sandbox.write("mods/beta.jar.disabled", b"b");

    let rows = sandbox.json(&["content", "list", "demo", "--kind", "mod", "--json"]);
    let names = rows
        .as_array()
        .expect("array")
        .iter()
        .map(|row| row["file"].as_str().expect("file").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(names, ["alpha.jar", "beta.jar", "zeta.jar"], "{rows}");

    let beta = rows
        .as_array()
        .expect("array")
        .iter()
        .find(|row| row["file"] == "beta.jar")
        .expect("beta");
    assert_eq!(beta["enabled"], false, "the .disabled marker must show");

    let filtered = sandbox.json(&[
        "content", "list", "demo", "--kind", "mod", "--filter", "ALP", "--json",
    ]);
    let filtered = filtered.as_array().expect("array");
    assert_eq!(filtered.len(), 1, "{filtered:?}");
    assert_eq!(filtered[0]["file"], "alpha.jar");
}

#[test]
fn disabling_and_enabling_a_mod_renames_the_file() {
    let sandbox = Sandbox::new("toggle-mod");
    sandbox.write("mods/sodium.jar", b"s");

    sandbox.json(&[
        "content",
        "disable",
        "demo",
        "sodium.jar",
        "--kind",
        "mod",
        "--json",
    ]);
    assert!(!sandbox.path("mods/sodium.jar").exists());
    assert!(sandbox.path("mods/sodium.jar.disabled").is_file());

    sandbox.json(&[
        "content",
        "enable",
        "demo",
        "sodium.jar",
        "--kind",
        "mod",
        "--json",
    ]);
    assert!(sandbox.path("mods/sodium.jar").is_file());
    assert!(!sandbox.path("mods/sodium.jar.disabled").exists());
}

#[test]
fn a_resource_pack_is_switched_in_options_txt_without_touching_other_keys() {
    let sandbox = Sandbox::new("toggle-pack");
    sandbox.write("resourcepacks/fancy.zip", b"p");
    sandbox.write(
        "options.txt",
        b"version:3955\nfov:0.5\nresourcePacks:[\"vanilla\"]\nlang:zh_cn\n",
    );

    sandbox.json(&[
        "content",
        "enable",
        "demo",
        "fancy.zip",
        "--kind",
        "pack",
        "--json",
    ]);
    let text = std::fs::read_to_string(sandbox.path("options.txt")).expect("options");
    assert!(
        text.contains("resourcePacks:[\"vanilla\",\"file/fancy.zip\"]"),
        "{text}"
    );
    assert!(
        text.contains("fov:0.5") && text.contains("lang:zh_cn"),
        "{text}"
    );

    sandbox.json(&[
        "content",
        "disable",
        "demo",
        "fancy.zip",
        "--kind",
        "pack",
        "--json",
    ]);
    let text = std::fs::read_to_string(sandbox.path("options.txt")).expect("options");
    assert!(text.contains("resourcePacks:[\"vanilla\"]"), "{text}");
    // The file itself never moves: packs are switched by the config, not by
    // the `.disabled` marker.
    assert!(sandbox.path("resourcepacks/fancy.zip").is_file());
}

#[test]
fn removing_moves_the_file_to_the_trash_and_restore_brings_it_back() {
    let sandbox = Sandbox::new("remove");
    sandbox.write("mods/sodium.jar", b"payload");

    let removed = sandbox.json(&[
        "content",
        "remove",
        "demo",
        "sodium.jar",
        "--kind",
        "mod",
        "--json",
    ]);
    assert!(removed["trash"].is_string(), "{removed}");
    assert!(!sandbox.path("mods/sodium.jar").exists());

    let listed = sandbox.json(&["trash", "list", "demo", "--json"]);
    let entries = listed.as_array().expect("array");
    assert_eq!(entries.len(), 1, "{listed}");
    assert_eq!(entries[0]["name"], "sodium.jar");

    sandbox.json(&["trash", "restore", "demo", "sodium.jar", "--json"]);
    assert_eq!(
        std::fs::read(sandbox.path("mods/sodium.jar")).expect("restored"),
        b"payload"
    );

    sandbox.json(&["trash", "purge", "demo", "--yes", "--json"]);
    assert!(!sandbox.path(".tmcl-trash").exists());
}

#[test]
fn purge_without_confirmation_refuses_and_keeps_the_file() {
    let sandbox = Sandbox::new("purge-refused");
    sandbox.write("mods/sodium.jar", b"s");

    let output = sandbox.run(&[
        "content",
        "remove",
        "demo",
        "sodium.jar",
        "--kind",
        "mod",
        "--purge",
    ]);

    assert_eq!(
        output.status.code(),
        Some(2),
        "no --yes, no --dry-run: refuse"
    );
    let message = String::from_utf8_lossy(&output.stderr);
    assert!(message.contains("--yes"), "{message}");
    assert!(message.contains("--dry-run"), "{message}");
    assert!(
        sandbox.path("mods/sodium.jar").is_file(),
        "nothing may be deleted"
    );
}

#[test]
fn adding_a_local_jar_copies_it_into_mods() {
    let sandbox = Sandbox::new("add-file");
    let source = sandbox.root.join("downloads").join("lithium.jar");
    std::fs::create_dir_all(source.parent().expect("dir")).expect("downloads");
    std::fs::write(&source, b"jar").expect("write source");

    let added = sandbox.json(&[
        "content",
        "add",
        "demo",
        &source.display().to_string(),
        "--kind",
        "mod",
        "--json",
    ]);

    assert_eq!(added["name"], "lithium.jar");
    assert_eq!(
        std::fs::read(sandbox.path("mods/lithium.jar")).expect("copied"),
        b"jar"
    );
    // A copy, not a move.
    assert!(source.is_file());
}

#[test]
fn adding_from_a_url_names_the_file_after_the_last_segment() {
    let sandbox = Sandbox::new("add-url");
    let body: &[u8] = b"jar-bytes";
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
    let address = listener.local_addr().expect("address");
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("connection");
        let mut buffer = [0u8; 2048];
        let _ = stream.read(&mut buffer).expect("request");
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/java-archive\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(response.as_bytes()).expect("headers");
        stream.write_all(body).expect("body");
    });

    let url = format!("http://{address}/files/sodium.jar?download=1");
    let added = sandbox.json(&["content", "add", "demo", &url, "--kind", "mod", "--json"]);
    server.join().expect("server");

    assert_eq!(added["name"], "sodium.jar");
    assert_eq!(
        std::fs::read(sandbox.path("mods/sodium.jar")).expect("downloaded"),
        body
    );
}

#[test]
fn adding_a_file_that_already_exists_is_refused() {
    let sandbox = Sandbox::new("add-collision");
    sandbox.write("mods/sodium.jar", b"installed");
    let source = sandbox.root.join("sodium.jar");
    std::fs::write(&source, b"new").expect("write source");

    let output = sandbox.run(&[
        "content",
        "add",
        "demo",
        &source.display().to_string(),
        "--kind",
        "mod",
    ]);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("already exists"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read(sandbox.path("mods/sodium.jar")).expect("kept"),
        b"installed"
    );
}

#[test]
fn ordering_moves_a_pack_in_options_txt_and_refuses_mods() {
    let sandbox = Sandbox::new("order");
    sandbox.write("resourcepacks/a.zip", b"a");
    sandbox.write("resourcepacks/b.zip", b"b");
    sandbox.write(
        "options.txt",
        b"resourcePacks:[\"vanilla\",\"file/a.zip\",\"file/b.zip\"]\n",
    );

    // `b` is last (drawn on top), so moving it up swaps it with `a`.
    sandbox.json(&[
        "content", "order", "demo", "b.zip", "--kind", "pack", "--up", "--json",
    ]);
    let text = std::fs::read_to_string(sandbox.path("options.txt")).expect("options");
    assert!(
        text.contains("resourcePacks:[\"vanilla\",\"file/b.zip\",\"file/a.zip\"]"),
        "{text}"
    );

    sandbox.write("mods/sodium.jar", b"s");
    let refused = sandbox.run(&[
        "content",
        "order",
        "demo",
        "sodium.jar",
        "--kind",
        "pack",
        "--up",
    ]);
    assert_eq!(refused.status.code(), Some(2), "mods have no order");
}

#[test]
fn worlds_are_listed_with_their_level_metadata() {
    let sandbox = Sandbox::new("world-list");
    sandbox.write(
        "saves/Birthday World/level.dat",
        &level_dat("Birthday World"),
    );
    sandbox.write("saves/Birthday World/region/r.0.0.mca", b"region");
    sandbox.write("saves/Broken/level.dat", b"not nbt");

    let rows = sandbox.json(&["world", "list", "demo", "--json"]);
    let worlds = rows.as_array().expect("array");
    assert_eq!(worlds.len(), 2, "{rows}");

    let birthday = worlds
        .iter()
        .find(|row| row["name"] == "Birthday World")
        .expect("named world");
    assert_eq!(birthday["version"], "1.20.1");
    assert_eq!(birthday["game_mode"], "creative");
    assert_eq!(birthday["parsed"], true);
    assert!(birthday["size"].as_u64().unwrap_or(0) > 0);

    let broken = worlds
        .iter()
        .find(|row| row["folder"] == "Broken")
        .expect("broken world");
    assert_eq!(
        broken["parsed"], false,
        "an unreadable level.dat is not fatal"
    );

    let info = sandbox.json(&["world", "info", "demo", "Birthday World", "--json"]);
    assert_eq!(info["level_name"], "Birthday World", "{info}");
    assert_eq!(info["version"], "1.20.1", "{info}");
    assert_eq!(info["game_mode"], "creative", "{info}");
    assert_eq!(info["hardcore"], false, "{info}");
    assert!(
        info["last_played"].is_number(),
        "the fixture writes a timestamp: {info}"
    );
}

#[test]
fn world_info_reports_nulls_instead_of_failing_on_a_broken_level_dat() {
    let sandbox = Sandbox::new("world-info-broken");
    sandbox.write("saves/Broken/level.dat", b"not nbt");

    let info = sandbox.json(&["world", "info", "demo", "Broken", "--json"]);

    assert_eq!(info["parsed"], false);
    assert!(info["version"].is_null(), "{info}");
    assert!(info["last_played"].is_null(), "{info}");
    assert_eq!(info["name"], "Broken");
}

#[test]
fn a_world_can_be_backed_up_listed_restored_and_cloned() {
    let sandbox = Sandbox::new("world-backup");
    sandbox.write("saves/World/level.dat", &level_dat("World"));
    sandbox.write("saves/World/region/r.0.0.mca", b"original");

    let backup = sandbox.json(&["world", "backup", "demo", "World", "--json"]);
    let archive = Path::new(backup["archive"].as_str().expect("archive"));
    assert!(archive.is_file(), "{backup}");

    let listed = sandbox.json(&["world", "backups", "demo", "World", "--json"]);
    let entries = listed.as_array().expect("array");
    assert_eq!(entries.len(), 1, "{listed}");
    let stamp = entries[0]["stamp"].as_str().expect("stamp").to_owned();

    // Change the world, then put the backup back.
    sandbox.write("saves/World/region/r.0.0.mca", b"changed");

    // Restoring overwrites the save, so it needs the same gate as the others.
    let refused = sandbox.run(&["world", "restore", "demo", "World", "--from", &stamp]);
    assert_eq!(refused.status.code(), Some(2), "no --yes means no");
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("--yes"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(
        std::fs::read(sandbox.path("saves/World/region/r.0.0.mca")).expect("still changed"),
        b"changed",
        "the refused restore must not touch the world"
    );

    sandbox.json(&[
        "world", "restore", "demo", "World", "--from", &stamp, "--yes", "--json",
    ]);
    assert_eq!(
        std::fs::read(sandbox.path("saves/World/region/r.0.0.mca")).expect("restored"),
        b"original"
    );

    let cloned = sandbox.json(&["world", "clone", "demo", "World", "--json"]);
    let clone = Path::new(cloned["clone"].as_str().expect("clone"));
    assert!(clone.join("level.dat").is_file(), "{cloned}");
    assert_ne!(clone, sandbox.path("saves/World"));
}

#[test]
fn removing_a_world_needs_confirmation_and_goes_to_the_trash() {
    let sandbox = Sandbox::new("world-remove");
    sandbox.write("saves/World/level.dat", &level_dat("World"));

    let refused = sandbox.run(&["world", "remove", "demo", "World"]);
    assert_eq!(refused.status.code(), Some(2));
    assert!(sandbox.path("saves/World").is_dir());

    sandbox.json(&["world", "remove", "demo", "World", "--yes", "--json"]);
    assert!(!sandbox.path("saves/World").exists());
    let trashed = sandbox.json(&["trash", "list", "demo", "--json"]);
    assert_eq!(
        trashed.as_array().expect("array")[0]["original"],
        sandbox.path("saves/World").display().to_string()
    );
}

#[test]
fn data_packs_are_listed_from_inside_their_world() {
    let sandbox = Sandbox::new("datapacks");
    sandbox.write("saves/World/level.dat", &level_dat("World"));
    sandbox.write("saves/World/datapacks/terrain.zip", b"pack");

    let rows = sandbox.json(&["world", "datapack", "list", "demo", "World", "--json"]);
    let packs = rows.as_array().expect("array");
    assert_eq!(packs.len(), 1, "{rows}");
    assert_eq!(packs[0]["file"], "terrain.zip");

    sandbox.json(&[
        "world",
        "datapack",
        "disable",
        "demo",
        "World",
        "terrain.zip",
        "--json",
    ]);
    assert!(
        sandbox
            .path("saves/World/datapacks/terrain.zip.disabled")
            .is_file()
    );

    // A world that does not exist is an error, not an empty list.
    let missing = sandbox.run(&["world", "datapack", "list", "demo", "Nowhere"]);
    assert_eq!(missing.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&missing.stderr).contains("no world named Nowhere"),
        "{}",
        String::from_utf8_lossy(&missing.stderr)
    );
}

#[test]
fn checking_for_updates_with_no_mods_reports_nothing_and_stays_offline() {
    let sandbox = Sandbox::new("updates-empty");

    // No jars means the check returns before any request is made, so this is
    // safe to run without a network. The populated path is covered by
    // `core::content::update`'s local-server test.
    let rows = sandbox.json(&["mod", "check-updates", "demo", "--json"]);
    assert_eq!(rows, serde_json::json!([]));

    let output = sandbox.run(&["mod", "update", "demo", "sodium.jar", "--yes"]);
    assert_eq!(output.status.code(), Some(3), "download errors exit with 3");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("no update for sodium.jar"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn a_dry_run_prints_the_plan_and_changes_nothing() {
    let sandbox = Sandbox::new("dry-run");
    sandbox.write("mods/sodium.jar", b"s");
    sandbox.write("saves/World/level.dat", &level_dat("World"));

    let planned = sandbox.json(&[
        "content",
        "remove",
        "demo",
        "sodium.jar",
        "--kind",
        "mod",
        "--purge",
        "--dry-run",
        "--json",
    ]);
    assert_eq!(planned["applied"], false, "{planned}");
    assert!(sandbox.path("mods/sodium.jar").is_file());

    let world = sandbox.json(&[
        "world",
        "remove",
        "demo",
        "World",
        "--yes",
        "--dry-run",
        "--json",
    ]);
    assert_eq!(world["applied"], false, "{world}");
    assert!(sandbox.path("saves/World").is_dir());
}

#[test]
fn updating_a_mod_asks_for_confirmation_before_the_lookup() {
    let sandbox = Sandbox::new("mod-update-consent");
    sandbox.write("mods/sodium.jar", b"s");

    let refused = sandbox.run(&["mod", "update", "demo", "sodium.jar"]);

    assert_eq!(refused.status.code(), Some(2), "no --yes means no");
    let message = String::from_utf8_lossy(&refused.stderr);
    assert!(message.contains("--yes"), "{message}");
    // The gate runs before the Modrinth lookup, so this test never needs the
    // network; a request would only be reachable with --yes or a terminal.
}

#[test]
fn purging_the_trash_needs_confirmation_and_keeps_the_files() {
    let sandbox = Sandbox::new("trash-purge-gate");
    sandbox.write("mods/sodium.jar", b"s");
    sandbox.json(&[
        "content",
        "remove",
        "demo",
        "sodium.jar",
        "--kind",
        "mod",
        "--json",
    ]);

    let refused = sandbox.run(&["trash", "purge", "demo"]);

    assert_eq!(refused.status.code(), Some(2), "no --yes means no");
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("--yes"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    let entries = sandbox.json(&["trash", "list", "demo", "--json"]);
    assert_eq!(
        entries.as_array().expect("array").len(),
        1,
        "the trashed file must survive a refused purge: {entries}"
    );
}
