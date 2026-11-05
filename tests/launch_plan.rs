//! Launch plans for the five instance layouts TMCL has to accept.

use std::path::{Path, PathBuf};

use tmcl::core::launch::plan::{FileRole, Strategy, plan};
use tmcl::core::loader::LoaderKind;
use tmcl::core::loader::client_json::parse;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/launch")
            .join(name),
    )
    .expect("launch fixture")
}

/// Writes `versions/<id>/<id>.json` plus placeholder jars into a temp game dir.
fn game_dir(files: &[(&str, Vec<u8>)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("tmcl-launch-plan-{}", uuid::Uuid::new_v4()));
    for (relative, bytes) in files {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().expect("fixture parent")).expect("fixture dir");
        std::fs::write(path, bytes).expect("fixture file");
    }
    root
}

#[test]
fn pcl2_forge_instance_rebuilds_with_module_path_and_client_jar() {
    let root = game_dir(&[
        (
            "versions/BattleArmory/BattleArmory.json",
            fixture("pcl2-forge-1.20.1.json"),
        ),
        ("versions/BattleArmory/BattleArmory.jar", b"jar".to_vec()),
    ]);
    let instance = root.join("versions/BattleArmory");
    let client = parse(&fixture("pcl2-forge-1.20.1.json"))
        .expect("json")
        .resolve_inheritance(&root)
        .expect("inheritance");

    let plan = plan(&instance, &root, &client);

    assert_eq!(plan.strategy, Strategy::Rebuild);
    assert_eq!(plan.loader.kind, LoaderKind::Forge);
    assert_eq!(plan.java_major, 17);
    assert_eq!(
        plan.required.len(),
        3,
        "client jar + forge client jar + bootstraplauncher"
    );
    assert_eq!(
        plan.loader.version.as_deref(),
        Some("1.20.1-47.4.22"),
        "loader version comes from the library coordinates"
    );
    assert!(plan.required.iter().any(|file| {
        file.role == FileRole::ClientJar && file.path.ends_with("BattleArmory.jar")
    }));
    assert!(
        plan.required
            .iter()
            .any(|file| file.path.ends_with("forge-1.20.1-47.4.22-client.jar"))
    );
    assert!(
        plan.required
            .iter()
            .any(|file| file.path.ends_with("bootstraplauncher-1.1.2.jar")),
        "module path entries are required files"
    );
    std::fs::remove_dir_all(root).expect("clean");
}

#[test]
fn hmcl_instance_without_its_own_jar_uses_the_inherited_one() {
    let root = game_dir(&[
        (
            "versions/1.20.1/1.20.1.json",
            fixture("hmcl-parent-1.20.1.json"),
        ),
        ("versions/1.20.1/1.20.1.jar", b"jar".to_vec()),
        (
            "versions/forge-1.20.1/forge-1.20.1.json",
            fixture("hmcl-child-forge.json"),
        ),
    ]);
    let instance = root.join("versions/forge-1.20.1");
    let client = parse(&fixture("hmcl-child-forge.json"))
        .expect("json")
        .resolve_inheritance(&root)
        .expect("inheritance");

    let plan = plan(&instance, &root, &client);

    assert_eq!(plan.loader.kind, LoaderKind::Forge);
    assert_eq!(plan.java_major, 17);
    assert_eq!(
        plan.required.len(),
        3,
        "client jar + inherited brigadier + forge universal"
    );
    let client_jar = plan
        .required
        .iter()
        .find(|file| file.role == FileRole::ClientJar)
        .expect("client jar");
    assert!(
        client_jar.path.ends_with("versions/1.20.1/1.20.1.jar"),
        "{}",
        client_jar.path.display()
    );
    std::fs::remove_dir_all(root).expect("clean");
}

#[test]
fn fabric_and_vanilla_instances_plan_without_loader_arguments() {
    let root = game_dir(&[
        ("versions/fabric/fabric.json", fixture("fabric-1.21.1.json")),
        ("versions/fabric/fabric.jar", b"jar".to_vec()),
        (
            "versions/legacy/legacy.json",
            fixture("vanilla-1.12.2.json"),
        ),
        ("versions/legacy/legacy.jar", b"jar".to_vec()),
    ]);

    let fabric = parse(&fixture("fabric-1.21.1.json"))
        .expect("fabric json")
        .resolve_inheritance(&root)
        .expect("inheritance");
    let fabric_plan = plan(&root.join("versions/fabric"), &root, &fabric);
    assert_eq!(fabric_plan.loader.kind, LoaderKind::Fabric);
    assert_eq!(fabric_plan.java_major, 21);
    assert_eq!(fabric_plan.strategy, Strategy::Rebuild);
    assert_eq!(fabric_plan.required.len(), 2, "client jar + fabric loader");

    let vanilla = parse(&fixture("vanilla-1.12.2.json"))
        .expect("vanilla json")
        .resolve_inheritance(&root)
        .expect("inheritance");
    let vanilla_plan = plan(&root.join("versions/legacy"), &root, &vanilla);
    assert_eq!(vanilla_plan.loader.kind, LoaderKind::Vanilla);
    assert_eq!(
        vanilla_plan.java_major, 8,
        "the builtin table covers instances without javaVersion"
    );
    assert_eq!(vanilla_plan.strategy, Strategy::Rebuild);
    assert_eq!(vanilla_plan.required.len(), 1, "client jar only");
    std::fs::remove_dir_all(root).expect("clean");
}

#[test]
fn legacy_forge_with_optifine_plans_for_java_8() {
    let root = game_dir(&[
        (
            "versions/legacy-forge/legacy-forge.json",
            fixture("forge-optifine-1.12.2.json"),
        ),
        ("versions/legacy-forge/legacy-forge.jar", b"jar".to_vec()),
    ]);
    let instance = root.join("versions/legacy-forge");
    let client = parse(&fixture("forge-optifine-1.12.2.json"))
        .expect("json")
        .resolve_inheritance(&root)
        .expect("inheritance");

    let plan = plan(&instance, &root, &client);

    assert_eq!(plan.loader.kind, LoaderKind::Forge);
    assert_eq!(
        plan.loader.version.as_deref(),
        Some("1.12.2-14.23.5.2860"),
        "legacy Forge version comes from the library coordinates"
    );
    assert_eq!(plan.java_major, 8);
    assert_eq!(plan.strategy, Strategy::Rebuild);
    assert_eq!(plan.required.len(), 2, "client jar + forge universal");
    assert!(plan.required.iter().any(|file| {
        file.role == FileRole::ClientJar && file.path.ends_with("legacy-forge.jar")
    }));
    assert!(plan.required.iter().any(|file| {
        file.path
            .ends_with("forge-1.12.2-14.23.5.2860-universal.jar")
    }));
    std::fs::remove_dir_all(root).expect("clean");
}

#[test]
fn official_args_file_takes_priority_over_rebuilding() {
    let root = game_dir(&[
        ("versions/pack/pack.json", fixture("pcl2-forge-1.20.1.json")),
        ("versions/pack/pack.jar", b"jar".to_vec()),
        (
            "libraries/net/minecraftforge/forge/1.20.1-47.4.22/win_args.txt",
            b"-p\nmodule".to_vec(),
        ),
        (
            "libraries/net/minecraftforge/forge/1.20.1-47.4.22/unix_args.txt",
            b"-p\nmodule".to_vec(),
        ),
    ]);
    let instance = root.join("versions/pack");
    let client = parse(&fixture("pcl2-forge-1.20.1.json"))
        .expect("json")
        .resolve_inheritance(&root)
        .expect("inheritance");

    let plan = plan(&instance, &root, &client);

    assert!(
        matches!(plan.strategy, Strategy::ArgsFile(_)),
        "{:?}",
        plan.strategy
    );
    assert_eq!(
        plan.required.len(),
        3,
        "the same dependencies are planned regardless of strategy"
    );
    std::fs::remove_dir_all(root).expect("clean");
}
