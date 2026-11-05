use std::path::PathBuf;

use tmcl::core::account::models::AccountKind;
use tmcl::core::account::offline::create_account;
use tmcl::core::java::JavaInstallation;
use tmcl::core::launch::command::{LaunchOptions, build};
use tmcl::core::loader::client_json::parse;

#[test]
fn builds_windows_command_and_redacts_secret_in_display() {
    let client = parse(
        br#"{"id":"1.21.1","mainClass":"net.minecraft.Main","arguments":{"game":["--username","player"],"jvm":["-DgameDir=game"]}}"#,
    )
    .expect("client JSON");
    let account = create_account("Alex", None).expect("account");
    let java = JavaInstallation {
        executable: PathBuf::from("C:/Java/bin/java.exe"),
        version: "21.0.4".into(),
        major: 21,
        vendor: Some("Test".into()),
        is_jdk: true,
        lts: true,
    };
    let spec = build(
        std::path::Path::new("C:/Games/instances/test"),
        std::path::Path::new("C:/Games"),
        &java,
        &account,
        &client,
        &[PathBuf::from("libraries/example.jar")],
        "secret-token",
        &LaunchOptions::default(),
    )
    .expect("command");
    assert_eq!(spec.current_dir, PathBuf::from("C:/Games/instances/test"));
    let classpath_flag = spec
        .args
        .iter()
        .position(|argument| argument == "-cp")
        .expect("-cp before the main class");
    assert_eq!(spec.args[classpath_flag + 1], spec.classpath);
    let library_path = spec
        .classpath
        .split(tmcl::core::launch::classpath::separator())
        .next()
        .expect("classpath library entry");
    assert!(PathBuf::from(library_path).ends_with("libraries/example.jar"));
    assert!(spec.display("secret-token").contains("C:/Java"));
    assert!(!spec.display("secret-token").contains("secret-token"));
}

#[test]
fn launch_classpath_deduplicates_libraries_and_includes_the_client_jar() {
    let client = parse(
        br#"{"id":"test","mainClass":"net.minecraft.Main","arguments":{"game":[],"jvm":[]}}"#,
    )
    .expect("client JSON");
    let account = create_account("Alex", None).expect("account");
    let java = JavaInstallation {
        executable: PathBuf::from("C:/Java/bin/java.exe"),
        version: "21.0.4".into(),
        major: 21,
        vendor: None,
        is_jdk: true,
        lts: true,
    };
    let repeated_library = PathBuf::from("libraries/example.jar");

    let spec = build(
        std::path::Path::new("C:/Games/instances/test"),
        std::path::Path::new("C:/Games"),
        &java,
        &account,
        &client,
        &[repeated_library.clone(), repeated_library],
        "offline-token",
        &LaunchOptions::default(),
    )
    .expect("command");

    let entries = spec
        .classpath
        .split(tmcl::core::launch::classpath::separator())
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 2, "duplicate libraries must be removed");
    assert!(PathBuf::from(entries[0]).ends_with("libraries/example.jar"));
    assert!(PathBuf::from(entries[1]).ends_with("test/test.jar"));
}

#[test]
fn classpath_libraries_follow_os_rules_and_keep_regular_native_library_artifacts() {
    let other_os = if std::env::consts::OS == "windows" {
        "linux"
    } else {
        "windows"
    };
    let json = format!(
        r#"{{"id":"test","mainClass":"net.minecraft.Main","libraries":[
            {{"name":"common:library:1","downloads":{{"artifact":{{"path":"common.jar","url":"https://example.test/common.jar","sha1":"a"}}}}}},
            {{"name":"current:library:1","rules":[{{"action":"allow","os":{{"name":"{}"}}}}],"downloads":{{"artifact":{{"path":"current.jar","url":"https://example.test/current.jar","sha1":"b"}}}}}},
            {{"name":"other:library:1","rules":[{{"action":"allow","os":{{"name":"{}"}}}}],"downloads":{{"artifact":{{"path":"other.jar","url":"https://example.test/other.jar","sha1":"c"}}}}}},
            {{"name":"native:library:1","natives":{{"windows":"natives-windows"}},"downloads":{{"artifact":{{"path":"native.jar","url":"https://example.test/native.jar","sha1":"d"}}}}}}
        ]}}"#,
        std::env::consts::OS,
        other_os
    );
    let client = parse(json.as_bytes()).expect("client JSON");

    let libraries = client
        .classpath_libraries(&std::collections::BTreeMap::new())
        .expect("allowed classpath libraries");

    assert_eq!(
        libraries,
        [
            PathBuf::from("libraries/common.jar"),
            PathBuf::from("libraries/current.jar"),
            PathBuf::from("libraries/native.jar")
        ]
    );
}

#[test]
fn resolves_neoforge_library_paths_under_the_libraries_directory() {
    let client = parse(
        br#"{"id":"1.21.1","mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher","libraries":[{"name":"cpw.mods:modlauncher:11.0.5","downloads":{"artifact":{"path":"cpw/mods/modlauncher/11.0.5/modlauncher-11.0.5.jar","url":"https://example.test/modlauncher.jar"}}}]}"#,
    )
    .expect("client JSON");
    let libraries = client
        .classpath_libraries(&std::collections::BTreeMap::new())
        .expect("classpath libraries");
    let account = create_account("Alex", None).expect("account");
    let java = JavaInstallation {
        executable: PathBuf::from("C:/Java/bin/java.exe"),
        version: "21.0.4".into(),
        major: 21,
        vendor: None,
        is_jdk: true,
        lts: true,
    };

    let spec = build(
        std::path::Path::new("C:/Games/instances/1.21.1"),
        std::path::Path::new("C:/Games"),
        &java,
        &account,
        &client,
        &libraries,
        "offline-token",
        &LaunchOptions::default(),
    )
    .expect("launch command");

    let modlauncher_path = spec
        .classpath
        .split(tmcl::core::launch::classpath::separator())
        .next()
        .expect("ModLauncher classpath entry");
    assert!(
        PathBuf::from(modlauncher_path)
            .ends_with("libraries/cpw/mods/modlauncher/11.0.5/modlauncher-11.0.5.jar")
    );
}

#[test]
fn does_not_duplicate_auth_or_resolution_options_already_in_version_arguments() {
    let client = parse(
        br#"{"id":"1.21.1","mainClass":"net.minecraft.Main","arguments":{"game":["--username","${auth_player_name}","--uuid","${auth_uuid}","--accessToken","${auth_access_token}","--userType","${user_type}","--width","${resolution_width}","--height","${resolution_height}"],"jvm":[]}}"#,
    )
    .expect("client JSON");
    let account = create_account("Alex", None).expect("account");
    let java = JavaInstallation {
        executable: PathBuf::from("C:/Java/bin/java.exe"),
        version: "21.0.4".into(),
        major: 21,
        vendor: None,
        is_jdk: true,
        lts: true,
    };

    let spec = build(
        std::path::Path::new("C:/Games/instances/test"),
        std::path::Path::new("C:/Games"),
        &java,
        &account,
        &client,
        &[],
        "offline-token",
        &LaunchOptions::default(),
    )
    .expect("launch command");

    for option in [
        "--username",
        "--uuid",
        "--accessToken",
        "--userType",
        "--width",
        "--height",
    ] {
        assert_eq!(
            spec.args
                .iter()
                .filter(|argument| argument.as_str() == option)
                .count(),
            1,
            "option {option} must appear only once"
        );
    }
}

#[test]
fn server_defaults_fill_missing_arguments_without_overriding_explicit_values() {
    let client = parse(
        br#"{"id":"1.21.1","mainClass":"net.minecraft.Main","arguments":{"game":["--server","explicit.example","--port","25565"],"jvm":[]}}"#,
    )
    .expect("client JSON");
    let account = create_account("Alex", None).expect("account");
    let java = JavaInstallation {
        executable: PathBuf::from("C:/Java/bin/java.exe"),
        version: "21.0.4".into(),
        major: 21,
        vendor: None,
        is_jdk: true,
        lts: true,
    };
    let options = LaunchOptions {
        server_address: Some("configured.example:24454".into()),
        ..LaunchOptions::default()
    };

    let spec = build(
        std::path::Path::new("C:/Games/instances/test"),
        std::path::Path::new("C:/Games"),
        &java,
        &account,
        &client,
        &[],
        "offline-token",
        &options,
    )
    .expect("command");

    assert!(
        spec.args
            .windows(2)
            .any(|pair| pair == ["--server", "explicit.example"])
    );
    assert!(spec.args.windows(2).any(|pair| pair == ["--port", "25565"]));
    assert_eq!(spec.args.iter().filter(|arg| *arg == "--server").count(), 1);
    assert_eq!(spec.args.iter().filter(|arg| *arg == "--port").count(), 1);
}

#[test]
fn quick_play_arguments_use_sjmcl_feature_defaults() {
    let client = parse(
        br#"{"id":"test","mainClass":"net.minecraft.Main","arguments":{"game":[
            {"rules":[{"action":"allow","features":{"has_quick_plays_support":true}}],"value":"--quickPlayPath"},
            {"rules":[{"action":"allow","features":{"is_quick_play_multiplayer":false}}],"value":"--not-joining-a-server"}
        ],"jvm":[]}}"#,
    )
    .expect("client JSON");
    let account = create_account("Alex", None).expect("account");
    let java = JavaInstallation {
        executable: PathBuf::from("C:/Java/bin/java.exe"),
        version: "21.0.4".into(),
        major: 21,
        vendor: None,
        is_jdk: true,
        lts: true,
    };

    let spec = build(
        std::path::Path::new("C:/Games/instances/test"),
        std::path::Path::new("C:/Games"),
        &java,
        &account,
        &client,
        &[],
        "offline-token",
        &LaunchOptions::default(),
    )
    .expect("command");

    assert!(
        spec.args
            .iter()
            .any(|argument| argument == "--quickPlayPath")
    );
    assert!(
        spec.args
            .iter()
            .any(|argument| argument == "--not-joining-a-server")
    );
}

#[test]
fn expands_neoforge_launch_variables_and_microsoft_credentials() {
    let client = parse(
        br#"{"id":"1.21.1","type":"release","mainClass":"net.minecraft.Main","arguments":{"game":["--clientId","${clientid}","--xuid","${auth_xuid}","--userType","${user_type}","--versionType","${version_type}","--libraryDirectory","${library_directory}","--separator","${classpath_separator}","--quickPlayPath","${quickPlayPath}"],"jvm":["-cp","${classpath}"]}}"#,
    )
    .expect("client JSON");
    let mut account = create_account("Alex", None).expect("account");
    account.kind = AccountKind::Microsoft;
    let java = JavaInstallation {
        executable: PathBuf::from("C:/Java/bin/java.exe"),
        version: "21.0.4".into(),
        major: 21,
        vendor: None,
        is_jdk: true,
        lts: true,
    };
    let spec = build(
        std::path::Path::new("C:/Games/instances/test"),
        std::path::Path::new("C:/Games"),
        &java,
        &account,
        &client,
        &[],
        "minecraft-access-token",
        &LaunchOptions::default(),
    )
    .expect("Microsoft command with NeoForge placeholders");

    let classpath_flag = spec
        .args
        .iter()
        .position(|argument| argument == "-cp")
        .expect("-cp before the main class");
    assert_eq!(spec.args[classpath_flag + 1], spec.classpath);
    assert!(PathBuf::from(&spec.classpath).ends_with("test/test.jar"));
    // Placeholders with no value in this launcher (`--clientId`, `--xuid`,
    // `--quickPlayPath`) are dropped instead of sent as empty strings: the
    // vanilla entry point rejects an empty value and exits before the window
    // opens.
    assert!(!spec.args.iter().any(String::is_empty));
    assert!(!spec.args.iter().any(|argument| argument == "--clientId"));
    assert!(!spec.args.iter().any(|argument| argument == "--xuid"));
    assert!(
        !spec
            .args
            .iter()
            .any(|argument| argument == "--quickPlayPath")
    );
    assert!(
        spec.args
            .windows(2)
            .any(|pair| pair == ["--userType", "msa"])
    );
    assert!(
        spec.args
            .windows(2)
            .any(|pair| pair == ["--versionType", "release"])
    );
    let library_directory = PathBuf::from("C:/Games")
        .join("libraries")
        .to_string_lossy()
        .into_owned();
    assert!(
        spec.args
            .windows(2)
            .any(|pair| { pair[0] == "--libraryDirectory" && pair[1] == library_directory })
    );
    assert!(spec.args.windows(2).any(|pair| {
        pair[0] == "--separator"
            && pair[1] == tmcl::core::launch::classpath::separator().to_string()
    }));
    assert!(
        spec.args
            .windows(2)
            .any(|pair| pair == ["--accessToken", "minecraft-access-token"])
    );
}

fn test_java() -> JavaInstallation {
    JavaInstallation {
        executable: PathBuf::from("C:/Java/bin/java.exe"),
        version: "17.0.12".into(),
        major: 17,
        vendor: None,
        is_jdk: true,
        lts: true,
    }
}

#[test]
fn forge_ignore_list_from_pcl2_expands_both_jar_placeholders() {
    let client = parse(
        br#"{"id":"1.20.1-forge-47.4.22","mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher","arguments":{"game":[],"jvm":["-DignoreList=forge-,${version_name}.jar,${primary_jar_name}"]}}"#,
    )
    .expect("client JSON");
    let account = create_account("Alex", None).expect("account");
    let spec = build(
        std::path::Path::new("C:/Games/versions/BattleArmory TACZ"),
        std::path::Path::new("C:/Games"),
        &test_java(),
        &account,
        &client,
        &[],
        "offline-token",
        &LaunchOptions::default(),
    )
    .expect("command");

    assert!(
        spec.args.iter().any(|argument| argument
            == "-DignoreList=forge-,BattleArmory TACZ.jar,BattleArmory TACZ.jar"),
        "{:?}",
        spec.args
    );
}

#[test]
fn placeholders_from_foreign_launchers_are_dropped_instead_of_failing() {
    let client = parse(
        br#"{"id":"1.20.1","mainClass":"net.minecraft.Main","arguments":{"game":["--demo", "${hmcl_only_variable}"],"jvm":["-Dfoo=${another_launcher_var}"]}}"#,
    )
    .expect("client JSON");
    let account = create_account("Alex", None).expect("account");
    let spec = build(
        std::path::Path::new("C:/Games/instances/test"),
        std::path::Path::new("C:/Games"),
        &test_java(),
        &account,
        &client,
        &[],
        "offline-token",
        &LaunchOptions::default(),
    )
    .expect("command");

    assert!(spec.args.contains(&"--demo".to_owned()));
    assert!(
        !spec.args.iter().any(|argument| argument.contains("${")),
        "{:?}",
        spec.args
    );
}

#[test]
fn missing_instance_jar_falls_back_to_the_inherited_version_jar() {
    let root = std::env::temp_dir().join(format!("tmcl-inherit-{}", uuid::Uuid::new_v4()));
    let versions = root.join("versions");
    std::fs::create_dir_all(versions.join("Parent")).expect("parent dir");
    std::fs::create_dir_all(versions.join("Child")).expect("child dir");
    std::fs::write(versions.join("Parent/Parent.jar"), b"jar").expect("parent jar");
    std::fs::write(
        versions.join("Child/Child.json"),
        br#"{"id":"Child","inheritsFrom":"Parent","mainClass":"net.minecraft.Main","arguments":{"game":[],"jvm":[]}}"#,
    )
    .expect("child json");
    std::fs::write(
        versions.join("Parent/Parent.json"),
        br#"{"id":"Parent","mainClass":"net.minecraft.Main","arguments":{"game":[],"jvm":[]}}"#,
    )
    .expect("parent json");

    let client = parse(&std::fs::read(versions.join("Child/Child.json")).expect("child"))
        .expect("parse")
        .resolve_inheritance(&root)
        .expect("resolve inheritance");
    let spec = build(
        &versions.join("Child"),
        &root,
        &test_java(),
        &create_account("Alex", None).expect("account"),
        &client,
        &[],
        "offline-token",
        &LaunchOptions::default(),
    )
    .expect("command");

    // HMCL and the vanilla layout keep the jar on the inherited version; the
    // launcher must not insist on `<instance>.jar`.
    assert!(spec.classpath.ends_with("Parent.jar"), "{}", spec.classpath);
    std::fs::remove_dir_all(root).expect("clean");
}
