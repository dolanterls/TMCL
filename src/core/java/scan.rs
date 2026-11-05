use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::core::instance::path::deduplicate;
use crate::core::java::models::JavaInstallation;
use crate::core::java::parse::parse_probe;
use crate::platform::windows;

pub fn discover(candidates: impl IntoIterator<Item = PathBuf>) -> Vec<JavaInstallation> {
    let mut seen = HashSet::new();
    let mut installations = Vec::new();
    for executable in deduplicate(candidates) {
        let normalized = executable
            .canonicalize()
            .unwrap_or_else(|_| executable.clone());
        if !seen.insert(normalized.clone()) {
            continue;
        }
        let release = release_content(&normalized);
        let command_output = release.is_none().then(|| command_version(&normalized));
        let java = parse_probe(
            normalized.clone(),
            release.as_deref(),
            command_output.as_deref(),
        );
        if let Ok(java) = java {
            installations.push(java);
        }
    }
    installations.sort_by(|left, right| {
        is_launcher_shim(&left.executable)
            .cmp(&is_launcher_shim(&right.executable))
            .then_with(|| right.major.cmp(&left.major))
            .then_with(|| left.executable.cmp(&right.executable))
    });
    installations
}

pub fn discover_one(
    executable: impl Into<PathBuf>,
) -> crate::core::error::Result<JavaInstallation> {
    let mut executable = executable.into();
    if executable.is_dir() {
        executable = executable
            .join("bin")
            .join(if cfg!(windows) { "java.exe" } else { "java" });
    }
    if !executable.is_file() {
        return Err(crate::core::error::Error::Java(format!(
            "Java executable does not exist: {}",
            executable.display()
        )));
    }
    let normalized = executable
        .canonicalize()
        .unwrap_or_else(|_| executable.clone());
    let release = release_content(&normalized);
    let command_output = release.is_none().then(|| command_version(&normalized));
    parse_probe(normalized, release.as_deref(), command_output.as_deref())
}

pub fn discover_default(
    tmcl_runtime: &Path,
    game_directories: impl IntoIterator<Item = PathBuf>,
    configured: impl IntoIterator<Item = PathBuf>,
) -> Vec<JavaInstallation> {
    let mut candidates = windows::java_candidates();
    candidates.extend(configured);
    candidates.extend(
        game_directories
            .into_iter()
            .map(|path| path.join("runtime")),
    );
    // PCL2, the official launcher and HMCL keep their managed runtimes in a
    // per-user Minecraft directory rather than inside the game directory.
    // Without these roots a 1.12.2 instance only sees the system Java (17+)
    // and fails before Minecraft can even log a stack trace.
    for variable in ["APPDATA", "LOCALAPPDATA"] {
        if let Some(appdata) = std::env::var_os(variable) {
            candidates.push(PathBuf::from(appdata).join(".minecraft").join("runtime"));
        }
    }
    candidates.push(tmcl_runtime.to_path_buf());
    discover(expand_java_candidates(candidates))
}

fn expand_java_candidates(candidates: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    candidates
        .into_iter()
        .flat_map(|path| {
            if path.is_file() {
                vec![path]
            } else {
                let direct = path
                    .join("bin")
                    .join(if cfg!(windows) { "java.exe" } else { "java" });
                if direct.is_file() {
                    vec![direct]
                } else {
                    let path_entry = path.join(if cfg!(windows) { "java.exe" } else { "java" });
                    if path_entry.is_file() {
                        return vec![path_entry];
                    }
                    // Runtime roots such as `.minecraft/runtime` contain one
                    // directory per JRE (`jre-legacy`, `java-runtime-delta`).
                    let mut runtimes = Vec::new();
                    if let Ok(entries) = std::fs::read_dir(&path) {
                        for entry in entries.flatten() {
                            let candidate = entry.path().join("bin").join(if cfg!(windows) {
                                "java.exe"
                            } else {
                                "java"
                            });
                            if candidate.is_file() {
                                runtimes.push(candidate);
                            }
                        }
                    }
                    runtimes
                }
            }
        })
        .collect()
}

fn is_launcher_shim(executable: &Path) -> bool {
    let value = executable.to_string_lossy().to_ascii_lowercase();
    value.contains(r"\common files\oracle\java\javapath")
        || value.contains("/common files/oracle/java/javapath")
}

fn release_content(executable: &Path) -> Option<String> {
    let release = executable.parent()?.parent()?.join("release");
    std::fs::read_to_string(release).ok()
}

fn command_version(executable: &Path) -> String {
    let output = Command::new(executable).arg("-version").output();
    let Ok(output) = output else {
        return String::new();
    };
    let mut combined = output.stdout;
    combined.extend(output.stderr);
    String::from_utf8_lossy(&combined).into_owned()
}

#[cfg(test)]
mod tests {
    use super::{discover_one, expand_java_candidates, is_launcher_shim};

    #[test]
    fn manually_added_java_can_be_discovered_from_an_executable_or_home_directory() {
        let root = std::env::temp_dir().join(format!("tmcl-java-add-{}", uuid::Uuid::new_v4()));
        let executable = root
            .join("bin")
            .join(if cfg!(windows) { "java.exe" } else { "java" });
        std::fs::create_dir_all(executable.parent().expect("java bin")).expect("create java tree");
        std::fs::write(&executable, b"java").expect("create java executable");
        std::fs::write(
            root.join("release"),
            b"JAVA_VERSION=\"21.0.1\"\nIMPLEMENTOR=\"Test Vendor\"",
        )
        .expect("write release file");

        let from_executable = discover_one(&executable).expect("discover executable");
        let from_home = discover_one(&root).expect("discover home");
        assert_eq!(from_executable.major, 21);
        assert_eq!(from_home.executable, from_executable.executable);

        std::fs::remove_dir_all(root).expect("clean java tree");
    }

    #[test]
    fn runtime_roots_expand_managed_jres() {
        let root = std::env::temp_dir().join(format!("tmcl-java-runtime-{}", uuid::Uuid::new_v4()));
        let runtime = root.join("runtime");
        let legacy = runtime.join("jre-legacy").join("bin").join("java.exe");
        std::fs::create_dir_all(legacy.parent().expect("runtime bin")).expect("create runtime");
        std::fs::write(&legacy, b"java").expect("create java");
        let expanded = expand_java_candidates([runtime]);
        assert_eq!(expanded, vec![legacy]);
        std::fs::remove_dir_all(root).expect("clean runtime");
    }
    #[test]
    fn launcher_shims_are_ranked_after_real_runtimes() {
        assert!(is_launcher_shim(std::path::Path::new(
            r"C:\Program Files\Common Files\Oracle\Java\javapath_target_1\java.exe"
        )));
        assert!(!is_launcher_shim(std::path::Path::new(
            r"D:\soft\java\bin\java.exe"
        )));
    }
}
