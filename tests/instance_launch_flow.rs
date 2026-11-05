use std::path::PathBuf;

use tmcl::core::account::offline::create_account;
use tmcl::core::instance::discovery::discover;
use tmcl::core::instance::service::create_instance;
use tmcl::core::java::models::JavaInstallation;
use tmcl::core::launch::command::{LaunchOptions, build};
use tmcl::core::loader::client_json;

#[test]
fn discovered_instance_produces_a_redacted_offline_launch_command() {
    let root = std::env::temp_dir().join(format!("tmcl-flow-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("game directory");
    let game_id = uuid::Uuid::new_v4();
    let instance = create_instance(&root, game_id, "flow", "1.21.1").expect("instance");
    std::fs::write(instance.path.join("flow.jar"), b"jar").expect("client jar");
    std::fs::write(
        instance.path.join("flow.json"),
        br#"{"id":"1.21.1","mainClass":"net.minecraft.client.main.Main","arguments":{"game":[],"jvm":[]}}"#,
    )
    .expect("client json");

    let found = discover(&root).expect("discover instances");
    assert_eq!(found.instances.len(), 1);
    let client = client_json::parse(
        br#"{"id":"1.21.1","mainClass":"net.minecraft.client.main.Main","arguments":{"game":[],"jvm":[]}}"#,
    )
    .expect("parse client");
    let account = create_account("Alex", None).expect("offline account");
    let java = JavaInstallation {
        executable: PathBuf::from("java.exe"),
        version: "21.0.1".into(),
        major: 21,
        vendor: Some("test".into()),
        is_jdk: false,
        lts: true,
    };
    let spec = build(
        &instance.path,
        &root,
        &java,
        &account,
        &client,
        &[],
        "secret-token",
        &LaunchOptions::default(),
    )
    .expect("build launch command");
    assert!(spec.args.iter().any(|argument| argument == "--username"));
    assert!(!spec.display("secret-token").contains("secret-token"));

    std::fs::remove_dir_all(root).expect("clean flow");
}
