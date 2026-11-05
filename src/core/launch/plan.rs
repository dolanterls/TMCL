//! Launch planning: what to run, and which files have to exist first.
//!
//! The plan is pure data. It is produced before any process is spawned, so the
//! launcher can repair what is missing and tests can assert on the decision
//! without touching the file system beyond the fixtures.

use std::path::{Path, PathBuf};

use crate::core::launch::classpath;
use crate::core::launch::command::{client_jar_path, launch_features};
use crate::core::loader::LoaderKind;
use serde::{Deserialize, Serialize};

use crate::core::loader::client_json::ClientJson;
use crate::core::loader::detect::{self, LoaderInfo};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Strategy {
    /// Forge/NeoForge shipped `win_args.txt` / `unix_args.txt` from the official
    /// installer; they already contain the resolved module path.
    ArgsFile(PathBuf),
    /// Rebuild the command line from the version JSON plus loader patches.
    Rebuild,
    /// Minimum viable command. Reserved for M2; not produced by [`plan`] yet.
    Minimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileRole {
    Java,
    ClientJar,
    Library,
    Asset,
    Native,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequiredFile {
    pub path: PathBuf,
    /// Path relative to the game directory, used to match library metadata.
    pub relative: Option<PathBuf>,
    pub role: FileRole,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPlan {
    pub strategy: Strategy,
    pub loader: LoaderInfo,
    pub java_major: u32,
    pub required: Vec<RequiredFile>,
}

pub fn plan(instance_dir: &Path, game_directory: &Path, client: &ClientJson) -> LaunchPlan {
    let loader = detect::detect(client);
    let mut required = Vec::new();

    required.push(RequiredFile {
        path: client_jar_path(instance_dir, client),
        relative: None,
        role: FileRole::ClientJar,
    });

    let features = launch_features();
    for library in client.classpath_libraries(&features).unwrap_or_default() {
        required.push(RequiredFile {
            path: game_directory.join(&library),
            relative: Some(library),
            role: FileRole::Library,
        });
    }

    for entry in module_path_entries(game_directory, client) {
        if required.iter().any(|file| file.path == entry) {
            continue;
        }
        required.push(RequiredFile {
            path: entry.clone(),
            relative: entry
                .strip_prefix(game_directory)
                .ok()
                .map(Path::to_path_buf),
            role: FileRole::Library,
        });
    }

    LaunchPlan {
        strategy: strategy(game_directory, &loader),
        java_major: loader.required_java_major.unwrap_or(8),
        loader,
        required,
    }
}

fn strategy(game_directory: &Path, loader: &LoaderInfo) -> Strategy {
    args_file(game_directory, loader).map_or(Strategy::Rebuild, Strategy::ArgsFile)
}

/// Official Forge/NeoForge installers write the resolved JVM arguments next to
/// the loader library. When that file exists it is the most faithful source.
fn args_file(game_directory: &Path, loader: &LoaderInfo) -> Option<PathBuf> {
    let root = match loader.kind {
        LoaderKind::Forge => "libraries/net/minecraftforge/forge",
        LoaderKind::NeoForge => "libraries/net/neoforged/neoforge",
        LoaderKind::Fabric | LoaderKind::Quilt | LoaderKind::Vanilla => return None,
    };
    let version = loader.version.as_deref()?;
    let directory = game_directory.join(root).join(version);
    let candidates: &[&str] = if cfg!(windows) {
        &["win_args.txt", "unix_args.txt"]
    } else {
        &["unix_args.txt", "win_args.txt"]
    };
    candidates
        .iter()
        .map(|name| directory.join(name))
        .find(|path| path.is_file())
}

fn module_path_entries(game_directory: &Path, client: &ClientJson) -> Vec<PathBuf> {
    let features = launch_features();
    let Ok(arguments) = client.jvm_arguments(&features) else {
        return Vec::new();
    };
    let Some(value) = arguments
        .iter()
        .position(|argument| argument == "-p" || argument == "--module-path")
        .and_then(|index| arguments.get(index + 1))
    else {
        return Vec::new();
    };
    let libraries = game_directory
        .join("libraries")
        .to_string_lossy()
        .into_owned();
    let expanded = value.replace("${library_directory}", &libraries);
    expanded
        .split(classpath::separator())
        .filter(|entry| !entry.is_empty() && !entry.contains("${"))
        .map(PathBuf::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{FileRole, Strategy, plan};
    use crate::core::loader::client_json::parse;

    fn temp_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("tmcl-plan-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("versions/pack")).expect("version dir");
        root
    }

    #[test]
    fn plans_pcl2_forge_instance_with_rebuild_strategy() {
        let root = temp_root();
        let instance = root.join("versions/pack");
        std::fs::write(instance.join("pack.jar"), b"jar").expect("jar");
        let client = parse(
            br#"{"id":"1.20.1-forge-47.4.22","mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher","javaVersion":{"majorVersion":17},"libraries":[{"name":"net.minecraftforge:forge:1.20.1-47.4.22","downloads":{"artifact":{"path":"net/minecraftforge/forge/1.20.1-47.4.22/forge-1.20.1-47.4.22-client.jar","url":"https://example.invalid/forge.jar"}}}],"arguments":{"game":[],"jvm":["-p","${library_directory}/cpw/mods/bootstraplauncher/1.1.2/bootstraplauncher-1.1.2.jar"]}}"#,
        )
        .expect("client JSON");

        let plan = plan(&instance, &root, &client);

        assert_eq!(plan.strategy, Strategy::Rebuild);
        assert_eq!(plan.java_major, 17);
        assert!(
            plan.required
                .iter()
                .any(|file| file.role == FileRole::ClientJar && file.path.ends_with("pack.jar"))
        );
        assert!(
            plan.required
                .iter()
                .any(|file| file.path.ends_with("forge-1.20.1-47.4.22-client.jar"))
        );
        assert!(
            plan.required
                .iter()
                .any(|file| file.path.ends_with("bootstraplauncher-1.1.2.jar")),
            "{:?}",
            plan.required
        );
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn plans_fabric_instance_with_rebuild_strategy() {
        let root = temp_root();
        let instance = root.join("versions/pack");
        std::fs::write(instance.join("pack.jar"), b"jar").expect("jar");
        let client = parse(
            br#"{"id":"1.21.1","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient","libraries":[{"name":"net.fabricmc:fabric-loader:0.15.11","downloads":{"artifact":{"path":"net/fabricmc/fabric-loader/0.15.11/fabric-loader-0.15.11.jar","url":"https://example.invalid/loader.jar"}}}],"arguments":{"game":[],"jvm":[]}}"#,
        )
        .expect("client JSON");

        let plan = plan(&instance, &root, &client);

        // Minimal is reserved for M2; M1 always rebuilds the command line.
        assert_eq!(plan.strategy, Strategy::Rebuild);
        assert_eq!(plan.java_major, 21);
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn official_args_file_wins_when_present() {
        let root = temp_root();
        let instance = root.join("versions/pack");
        let args = root.join("libraries/net/minecraftforge/forge/1.20.1-47.4.22");
        std::fs::create_dir_all(&args).expect("args dir");
        let args_file = args.join(if cfg!(windows) {
            "win_args.txt"
        } else {
            "unix_args.txt"
        });
        std::fs::write(&args_file, "-p\nmodule").expect("args file");
        let client = parse(
            br#"{"id":"1.20.1-forge-47.4.22","mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher","libraries":[{"name":"net.minecraftforge:forge:1.20.1-47.4.22"}],"arguments":{"game":[],"jvm":[]}}"#,
        )
        .expect("client JSON");

        let plan = plan(&instance, &root, &client);

        assert_eq!(plan.strategy, Strategy::ArgsFile(args_file));
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn module_path_entries_come_from_the_launch_arguments() {
        let root = temp_root();
        let instance = root.join("versions/pack");
        let client = parse(
            br#"{"id":"1.20.1-forge-47.4.22","mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher","libraries":[{"name":"net.minecraftforge:forge:1.20.1-47.4.22"}],"arguments":{"game":[],"jvm":["-p","${library_directory}/a.jar;${library_directory}/b.jar"]}}"#,
        )
        .expect("client JSON");

        let plan = plan(&instance, &root, &client);
        let module_entries = plan
            .required
            .iter()
            .filter(|file| file.path.ends_with("a.jar") || file.path.ends_with("b.jar"))
            .count();

        assert_eq!(module_entries, 2);
        assert!(plan.required.iter().any(|file| {
            file.relative
                .as_ref()
                .is_some_and(|relative| relative == Path::new("libraries/a.jar"))
        }));
        std::fs::remove_dir_all(root).expect("clean");
    }
}
