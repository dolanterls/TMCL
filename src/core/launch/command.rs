use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::core::account::models::{Account, AccountKind};
use crate::core::error::{Error, Result};
use crate::core::java::JavaInstallation;
use crate::core::launch::auth::account_arguments;
use crate::core::launch::classpath;
use crate::core::launch::template::{expand, redact_token};
use crate::core::loader::client_json::ClientJson;

#[derive(Debug, Clone)]
pub struct LaunchOptions {
    pub min_memory_mb: u32,
    pub max_memory_mb: u32,
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
    pub jvm_args: Vec<String>,
    pub extra_game_args: Vec<String>,
    pub server_address: Option<String>,
}

impl Default for LaunchOptions {
    fn default() -> Self {
        Self {
            min_memory_mb: 1024,
            max_memory_mb: 4096,
            width: 1280,
            height: 720,
            fullscreen: false,
            jvm_args: Vec::new(),
            extra_game_args: Vec::new(),
            server_address: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub classpath: String,
    pub current_dir: PathBuf,
}

pub fn launch_features() -> BTreeMap<String, bool> {
    BTreeMap::from([
        ("has_custom_resolution".into(), true),
        ("has_quick_plays_support".into(), true),
        ("is_demo_user".into(), false),
        ("is_quick_play_multiplayer".into(), false),
        ("is_quick_play_realms".into(), false),
        ("is_quick_play_singleplayer".into(), false),
    ])
}

impl CommandSpec {
    pub fn display(&self, secret: &str) -> String {
        std::iter::once(self.program.to_string_lossy().into_owned())
            .chain(self.args.iter().cloned())
            .map(|value| redact_token(&value, secret))
            .map(|value| quote(&value))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build(
    instance_dir: &Path,
    game_directory: &Path,
    java: &JavaInstallation,
    account: &Account,
    client: &ClientJson,
    libraries: &[PathBuf],
    token: &str,
    options: &LaunchOptions,
) -> Result<CommandSpec> {
    let mut values = BTreeMap::new();
    values.insert("auth_player_name".into(), account.username.clone());
    values.insert("auth_uuid".into(), account.uuid.simple().to_string());
    values.insert("auth_access_token".into(), token.into());
    values.insert(
        "user_type".into(),
        match account.kind {
            AccountKind::Offline => "legacy",
            AccountKind::Microsoft => "msa",
        }
        .into(),
    );
    values.insert("clientid".into(), String::new());
    values.insert("auth_xuid".into(), String::new());
    values.insert("user_properties".into(), "{}".into());
    values.insert("version_type".into(), client.version_type.clone());
    values.insert(
        "version_name".into(),
        instance_version_name(instance_dir, &client.id),
    );
    values.insert(
        "game_directory".into(),
        instance_dir.to_string_lossy().into_owned(),
    );
    values.insert(
        "assets_root".into(),
        game_directory.join("assets").to_string_lossy().into_owned(),
    );
    values.insert(
        "assets_index_name".into(),
        client
            .asset_index
            .as_ref()
            .map(|asset| asset.id.clone())
            .unwrap_or_default(),
    );
    if let Some(logging) = client
        .logging
        .as_ref()
        .and_then(|logging| logging.client.as_ref())
    {
        values.insert(
            "path".into(),
            game_directory
                .join("assets")
                .join("log_configs")
                .join(logging.file.id.as_deref().unwrap_or(&logging.file.sha1))
                .to_string_lossy()
                .into_owned(),
        );
    }
    let game_assets = game_directory.join("assets/virtual/legacy");
    values.insert(
        "game_assets".into(),
        game_assets.to_string_lossy().into_owned(),
    );
    values.insert(
        "library_directory".into(),
        game_directory
            .join("libraries")
            .to_string_lossy()
            .into_owned(),
    );
    values.insert(
        "classpath_separator".into(),
        classpath::separator().to_string(),
    );
    for name in [
        "quickPlayPath",
        "quickPlaySingleplayer",
        "quickPlayMultiplayer",
        "quickPlayRealms",
    ] {
        values.insert(name.into(), String::new());
    }
    let mut classpath_libraries = libraries.to_vec();
    if let Some(optifine) = legacy_optifine_jar(instance_dir, client) {
        if !classpath_libraries.contains(&optifine) {
            classpath_libraries.push(optifine);
        }
    }
    let client_jar = client_jar_path(instance_dir, client);
    // PCL2 writes `${primary_jar_name}` into Forge's `-DignoreList`; for an
    // instance the primary jar is the client jar on disk.
    values.insert(
        "primary_jar_name".into(),
        client_jar
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("{}.jar", client.id)),
    );
    classpath_libraries.push(client_jar);
    let classpath = classpath::build(game_directory, &classpath_libraries);
    values.insert("classpath".into(), classpath.clone());
    values.insert(
        "natives_directory".into(),
        natives_directory(instance_dir)
            .to_string_lossy()
            .into_owned(),
    );
    values.insert("launcher_name".into(), "TMCL".into());
    values.insert("launcher_version".into(), env!("CARGO_PKG_VERSION").into());
    values.insert("resolution_width".into(), options.width.to_string());
    values.insert("resolution_height".into(), options.height.to_string());

    let features = launch_features();
    let mut jvm_arguments = client.jvm_arguments(&features)?;
    while let Some(position) = jvm_arguments
        .windows(2)
        .position(|pair| pair[0] == "-cp" && pair[1] == "${classpath}")
    {
        jvm_arguments.drain(position..position + 2);
    }
    let mut args = jvm_arguments
        .into_iter()
        .map(|argument| expand(&argument, &values))
        .collect::<Result<Vec<_>>>()?;
    args.extend([
        format!("-Xms{}M", options.min_memory_mb),
        format!("-Xmx{}M", options.max_memory_mb),
        "${natives_directory}".into(),
    ]);
    let native_index = args.len() - 1;
    let native_template = args[native_index].clone();
    args[native_index] = format!("-Djava.library.path={native_template}");
    args.extend(options.jvm_args.clone());
    args.push(client.main_class.clone());
    // Pass the classpath explicitly as well as through CLASSPATH. Java 8 on
    // Windows can ignore the environment variable in some launch contexts;
    // every working launcher passes -cp before the main class.
    if let Some(main_class) = args.iter().position(|arg| arg == &client.main_class) {
        args.splice(main_class..main_class, ["-cp".into(), classpath.clone()]);
    }
    let mut game_args = client
        .game_arguments(&features)?
        .into_iter()
        .map(|argument| expand(&argument, &values))
        .collect::<Result<Vec<_>>>()?;
    let mut default_options = account_arguments(account, token);
    default_options.extend([
        "--width".into(),
        options.width.to_string(),
        "--height".into(),
        options.height.to_string(),
    ]);
    append_missing_options(&mut game_args, &default_options);
    if options.fullscreen {
        game_args.push("--fullscreen".into());
    }
    if let Some(address) = options.server_address.as_deref() {
        if let Some((host, port)) = address.rsplit_once(':') {
            append_missing_options(
                &mut game_args,
                &[
                    "--server".into(),
                    host.to_owned(),
                    "--port".into(),
                    port.to_owned(),
                ],
            );
        }
    }
    game_args.extend(
        options
            .extra_game_args
            .iter()
            .map(|argument| expand(argument, &values))
            .collect::<Result<Vec<_>>>()?,
    );
    args.extend(game_args);
    args = args
        .into_iter()
        .map(|argument| expand(&argument, &values))
        .collect::<Result<Vec<_>>>()?;
    let args = without_empty_values(args);
    if java.executable.as_os_str().is_empty() {
        return Err(Error::Java("Java executable path is empty".into()));
    }
    Ok(CommandSpec {
        program: java.executable.clone(),
        args,
        classpath,
        current_dir: instance_dir.to_path_buf(),
    })
}

fn quote(value: &str) -> String {
    if value.contains(' ') {
        format!("\"{value}\"")
    } else {
        value.into()
    }
}

/// Options this launcher deliberately leaves empty.
///
/// They come from a version JSON template (`--quickPlayPath ${quickPlayPath}`)
/// that expects the launcher to have a value; TMCL does not, so the pair would
/// reach the game as `--quickPlayPath ""`. The vanilla entry point parses with
/// jopt-simple, which reads an empty value as "no value given" and exits before
/// the window opens (`Option quickPlayPath requires an argument`). Forge's and
/// Fabric's entry points accept the same command line, so a modded instance
/// started while a vanilla one could not.
const EMPTY_WHEN_UNSET: [&str; 7] = [
    "--clientId",
    "--xuid",
    "--assetIndex",
    "--quickPlayPath",
    "--quickPlaySingleplayer",
    "--quickPlayMultiplayer",
    "--quickPlayRealms",
];

/// Drops `--option ""` pairs, and stray empty words next to anything else.
///
/// An unknown placeholder from another launcher (`--demo ${hmcl_only}`) expands
/// to nothing too, but there the option may be a switch, so only the empty word
/// goes and the switch stays.
fn without_empty_values(args: Vec<String>) -> Vec<String> {
    let mut kept = Vec::with_capacity(args.len());
    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        if args.get(index + 1).is_some_and(String::is_empty) {
            // `--demo ${hmcl_only}` keeps `--demo`; the empty word is not an
            // argument the game can use.
            if !EMPTY_WHEN_UNSET.contains(&argument.as_str()) {
                kept.push(argument.clone());
            }
            index += 2;
            continue;
        }
        kept.push(argument.clone());
        index += 1;
    }
    kept
}

/// Client jar installed for this instance.
///
/// Instances written by other launchers do not all keep `<instance>.jar`: the
/// vanilla/HMCL layout reuses `versions/<parent>/<parent>.jar`, while PCL2 copies
/// the jar next to the version JSON. Prefer whatever exists on disk, so a valid
/// install is never rejected over a file name.
pub(crate) fn client_jar_path(instance_dir: &Path, client: &ClientJson) -> PathBuf {
    let mut candidates = Vec::new();
    if let Some(name) = instance_dir.file_name() {
        candidates.push(instance_dir.join(format!("{}.jar", name.to_string_lossy())));
    }
    candidates.push(instance_dir.join(format!("{}.jar", client.id)));
    if let Some(game_directory) = instance_dir.parent().and_then(Path::parent) {
        for id in &client.inherited_ids {
            candidates.push(
                game_directory
                    .join("versions")
                    .join(id)
                    .join(format!("{id}.jar")),
            );
        }
    }
    if let Some(path) = candidates.iter().find(|path| path.is_file()) {
        return path.clone();
    }
    // Anything single sitting next to the version JSON is the client jar.
    let mut jars = std::fs::read_dir(instance_dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("jar"))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if jars.len() == 1 {
        return jars.remove(0);
    }
    candidates
        .into_iter()
        .next()
        .expect("client jar always has a candidate path")
}

fn append_missing_options(arguments: &mut Vec<String>, defaults: &[String]) {
    for option in defaults.chunks_exact(2) {
        if !arguments.iter().any(|argument| argument == &option[0]) {
            arguments.extend_from_slice(option);
        }
    }
}

fn natives_directory(instance_dir: &Path) -> PathBuf {
    let name = instance_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let candidates = [
        instance_dir.join(".tmcl-natives"),
        instance_dir.join(format!("{name}-natives")),
        instance_dir.join("natives"),
    ];
    for candidate in candidates {
        if candidate.is_dir()
            && std::fs::read_dir(&candidate)
                .ok()
                .is_some_and(|mut entries| entries.next().is_some())
        {
            return candidate;
        }
    }
    instance_dir.join(".tmcl-natives")
}

fn legacy_optifine_jar(instance_dir: &Path, client: &ClientJson) -> Option<PathBuf> {
    // Legacy Forge + OptiFine uses a LaunchWrapper tweaker
    // (`optifine.OptiFineForgeTweaker`). The tweaker class lives in the
    // OptiFine jar, which the version JSON lists as a library without a
    // downloadable artifact; the launcher has to put the jar from `mods` on
    // the classpath explicitly.
    if !client
        .main_class
        .to_ascii_lowercase()
        .contains("launchwrapper")
    {
        return None;
    }
    let mods = instance_dir.join("mods");
    for library in &client.libraries {
        let Some(version) = library.name.strip_prefix("optifine:OptiFine:") else {
            continue;
        };
        for candidate in [
            mods.join(format!("OptiFine_{version}.jar")),
            mods.join(format!("OptiFine_{version}.jar").to_ascii_lowercase()),
        ] {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let entries = std::fs::read_dir(&mods).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
        if name.starts_with("optifine") && name.ends_with(".jar") {
            return Some(path);
        }
    }
    None
}

fn instance_version_name(instance_dir: &Path, client_id: &str) -> String {
    instance_dir
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| client_id.to_owned())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{instance_version_name, without_empty_values};

    #[test]
    fn an_option_with_an_empty_value_is_dropped_whole() {
        let args = without_empty_values(vec![
            "--username".into(),
            "Alex".into(),
            "--quickPlayPath".into(),
            String::new(),
            "--clientId".into(),
            String::new(),
            "--xuid".into(),
            String::new(),
            "--versionType".into(),
            "release".into(),
        ]);

        assert_eq!(args, ["--username", "Alex", "--versionType", "release"]);
        assert!(!args.iter().any(String::is_empty));
    }

    #[test]
    fn a_flag_without_a_value_survives() {
        // `--demo` is a switch, not an option: it must not be paired with the
        // next flag and dropped.
        let args = without_empty_values(vec![
            "--demo".into(),
            "--versionType".into(),
            "release".into(),
        ]);

        assert_eq!(args, ["--demo", "--versionType", "release"]);
    }

    #[test]
    fn an_unknown_placeholder_leaves_its_switch_alone() {
        // `--demo ${hmcl_only_variable}` expands to `--demo ""`; the switch is
        // real and the empty word is not.
        let args = without_empty_values(vec!["--demo".into(), String::new()]);

        assert_eq!(args, ["--demo"]);
    }

    #[test]
    fn existing_launcher_natives_are_reused() {
        let root = std::env::temp_dir().join(format!("tmcl-natives-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("Pack").join("Pack-natives")).expect("natives");
        std::fs::write(
            root.join("Pack").join("Pack-natives").join("lwjgl.dll"),
            b"dll",
        )
        .expect("dll");
        assert_eq!(
            super::natives_directory(&root.join("Pack")),
            root.join("Pack").join("Pack-natives")
        );
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn the_managed_natives_folder_wins_over_an_imported_one() {
        let root = std::env::temp_dir().join(format!("tmcl-natives-{}", uuid::Uuid::new_v4()));
        let instance = root.join("Pack");
        // The instance came from another launcher and already has a folder;
        // extraction created the managed one next to it.
        for directory in ["Pack-natives", ".tmcl-natives"] {
            std::fs::create_dir_all(instance.join(directory)).expect("natives dir");
            std::fs::write(instance.join(directory).join("lwjgl.dll"), b"dll").expect("dll");
        }
        assert_eq!(
            super::natives_directory(&instance),
            instance.join(".tmcl-natives")
        );
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn optifine_tweaker_jar_is_added_for_legacy_forge() {
        use crate::core::loader::client_json::ClientJson;

        let root = std::env::temp_dir().join(format!("tmcl-optifine-{}", uuid::Uuid::new_v4()));
        let mods = root.join("mods");
        std::fs::create_dir_all(&mods).expect("mods");
        let jar = mods.join("OptiFine_1.12.2_HD_U_G5.jar");
        std::fs::write(&jar, b"jar").expect("jar");
        let client: ClientJson = serde_json::from_str(
            r#"{"id":"1.12.2c","mainClass":"net.minecraft.launchwrapper.Launch","libraries":[{"name":"optifine:OptiFine:1.12.2_HD_U_G5"}]}"#,
        )
        .expect("client");
        assert_eq!(super::legacy_optifine_jar(&root, &client), Some(jar));
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn version_name_matches_the_instance_jar_for_forge_ignore_lists() {
        assert_eq!(
            instance_version_name(
                Path::new(r"D:\games\versions\Zombie Invade 100 Days 2.3"),
                "1.20.1-forge-47.4.10"
            ),
            "Zombie Invade 100 Days 2.3"
        );
        assert_eq!(instance_version_name(Path::new(""), "1.21.1"), "1.21.1");
    }
}
