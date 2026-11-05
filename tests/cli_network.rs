//! Tests that talk to the real services.
//!
//! Ignored by default: the rest of the suite stays offline. Run them
//! deliberately before a release:
//!
//! ```text
//! cargo test --test cli_network -- --ignored --nocapture
//! ```
//!
//! They use the same fixed project as the manual smoke checks: Sodium
//! (`AANobbMI`) for mods and Stay True (`6Yj8BvGb`) for resource packs. Both
//! are long-lived projects with 1.21.1 builds; if either is removed upstream,
//! update the id here rather than deleting the test.

mod support;

use std::path::{Path, PathBuf};

use support::Sandbox;

fn instance_with(sandbox: &Sandbox, loader: &str) {
    std::fs::write(
        sandbox.instance().join("tmcl.json"),
        format!(
            r#"{{"id":"11111111-1111-4111-8111-111111111111","name":"demo","game_directory_id":"22222222-2222-4222-8222-222222222222","minecraft_version":"1.21.1","loader":{loader},"stats":{{"launch_count":0,"play_time_seconds":0}}}}"#
        ),
    )
    .expect("metadata");
    sandbox.run(&[
        "config",
        "game-dir",
        "add",
        &sandbox.game().display().to_string(),
    ]);
}

fn only_file(directory: &Path) -> PathBuf {
    let mut entries = std::fs::read_dir(directory)
        .expect("directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 1, "expected one file in {directory:?}");
    entries.remove(0)
}

#[test]
#[ignore = "needs network: Modrinth search"]
fn search_and_project_describe_a_known_mod() {
    let sandbox = Sandbox::new("net-search");

    let rows = sandbox.json(&[
        "search", "sodium", "--kind", "mod", "--limit", "3", "--json",
    ]);
    let rows = rows.as_array().expect("array");
    assert!(!rows.is_empty(), "Modrinth returned nothing");
    let id = rows[0]["id"].as_str().expect("id");
    assert!(rows[0]["downloads"].is_number(), "{}", rows[0]);

    let project = sandbox.json(&["project", id, "--kind", "mod", "--json"]);
    assert!(
        project["project"]["description"].is_string(),
        "the project block carries the description: {project}"
    );
    assert!(
        !project["files"].as_array().expect("files").is_empty(),
        "{project}"
    );
}

#[test]
#[ignore = "needs network: Mojang version manifest"]
fn version_list_filters_by_channel() {
    let sandbox = Sandbox::new("net-versions");

    let listed = sandbox.json(&["version", "list", "--type", "release", "--json"]);

    let versions = listed["versions"].as_array().expect("versions");
    assert!(!versions.is_empty(), "{listed}");
    for version in versions {
        assert_eq!(version["type"], "release", "{version}");
    }
}

#[test]
#[ignore = "needs network: Modrinth + download"]
fn a_resource_pack_installs_into_a_fresh_instance() {
    let sandbox = Sandbox::new("net-pack");
    instance_with(&sandbox, r#""Vanilla""#);

    let printed = sandbox.json(&[
        "install",
        "6Yj8BvGb",
        "--kind",
        "pack",
        "--instance",
        "demo",
        "--json",
    ]);

    let installed = printed["installed"].as_array().expect("installed");
    assert!(!installed.is_empty(), "{printed}");
    let installed = only_file(&sandbox.instance().join("resourcepacks"));
    assert_eq!(
        installed.extension().and_then(|value| value.to_str()),
        Some("zip"),
        "{installed:?}"
    );
}

#[test]
#[ignore = "needs network: Modrinth + download"]
fn a_mod_installs_into_a_fresh_fabric_instance() {
    let sandbox = Sandbox::new("net-mod");
    // No `mods/` directory yet: the install has to create it.
    instance_with(&sandbox, r#"{"Fabric":{"version":"0.16.5"}}"#);

    let printed = sandbox.json(&[
        "install",
        "AANobbMI",
        "--kind",
        "mod",
        "--instance",
        "demo",
        "--json",
    ]);

    let installed = only_file(&sandbox.instance().join("mods"));
    assert!(
        installed
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|name| name.ends_with(".jar")),
        "{printed}"
    );
}
