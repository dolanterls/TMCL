//! Pre-launch check of the files a launch command depends on.
//!
//! Metadata is a promise; the disk is the truth. When a version JSON names a
//! jar that was never downloaded, the JVM would fail with an unrelated message
//! ("Could not find or load main class", "Failed to create module launcher"),
//! which reads as a launcher bug. Checking first turns that into a named cause
//! that can be repaired.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::core::launch::classpath;
use crate::core::launch::command::CommandSpec;
use crate::core::launch::plan::{FileRole, LaunchPlan};

/// A file the launch depends on that is not on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissingFile {
    /// Absolute path the JVM will be pointed at.
    pub path: PathBuf,
    /// Path relative to the game directory, when the file came from metadata.
    pub relative: Option<PathBuf>,
    pub role: FileRole,
}

/// Checks the files a plan says are required, without building a command.
pub fn inspect_plan(plan: &LaunchPlan) -> Vec<MissingFile> {
    let mut missing = plan
        .required
        .iter()
        .filter(|file| !file.path.is_file())
        .map(|file| MissingFile {
            path: file.path.clone(),
            relative: file.relative.clone(),
            role: file.role,
        })
        .collect::<Vec<_>>();
    missing.sort_by(|left, right| left.path.cmp(&right.path));
    missing.dedup_by(|left, right| left.path == right.path);
    missing
}

/// Full check: the Java binary, every planned file, and the module path entries
/// that only appear after argument expansion.
pub fn inspect(spec: &CommandSpec, plan: &LaunchPlan) -> Vec<MissingFile> {
    let mut missing = Vec::new();
    if !spec.program.is_file() {
        missing.push(MissingFile {
            path: spec.program.clone(),
            relative: None,
            role: FileRole::Java,
        });
    }
    missing.extend(inspect_plan(plan));
    for entry in module_path_entries(&spec.args).unwrap_or_default() {
        if !entry.is_file() {
            missing.push(MissingFile {
                path: entry,
                relative: None,
                role: FileRole::Library,
            });
        }
    }
    missing.sort_by(|left, right| left.path.cmp(&right.path));
    missing.dedup_by(|left, right| left.path == right.path);
    missing
}

/// Every file the command points at that is not on disk.
///
/// Module-path launches (Forge/NeoForge 1.17+) depend on the `-p` entries, so
/// those are what gets checked; classic launches depend on the classpath.
pub fn missing_files(spec: &CommandSpec) -> Vec<PathBuf> {
    let mut missing = Vec::new();
    if !spec.program.is_file() {
        missing.push(spec.program.clone());
    }

    let module_path = module_path_entries(&spec.args);
    let entries = module_path.unwrap_or_else(|| {
        spec.classpath
            .split(classpath::separator())
            .filter(|entry| !entry.is_empty())
            .map(PathBuf::from)
            .collect()
    });
    for entry in entries {
        if !entry.is_file() {
            missing.push(entry);
        }
    }
    missing.sort();
    missing.dedup();
    missing
}

fn module_path_entries(args: &[String]) -> Option<Vec<PathBuf>> {
    let index = args
        .iter()
        .position(|argument| argument == "-p" || argument == "--module-path")?;
    let value = args.get(index + 1)?;
    Some(
        value
            .split(classpath::separator())
            .filter(|entry| !entry.is_empty())
            .map(PathBuf::from)
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{inspect, inspect_plan, missing_files};
    use crate::core::launch::classpath;
    use crate::core::launch::command::CommandSpec;
    use crate::core::launch::plan::{FileRole, LaunchPlan, RequiredFile};

    fn spec(program: &Path, classpath: String, extra: &[&str]) -> CommandSpec {
        CommandSpec {
            program: program.to_path_buf(),
            args: ["-cp", &classpath]
                .into_iter()
                .chain(extra.iter().copied())
                .map(str::to_owned)
                .collect(),
            classpath,
            current_dir: PathBuf::new(),
        }
    }

    fn temp_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("tmcl-preflight-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("temp root");
        root
    }

    #[test]
    fn a_complete_install_reports_nothing() {
        let root = temp_root();
        let java = root.join("java.exe");
        let jar = root.join("client.jar");
        std::fs::write(&java, b"java").expect("java");
        std::fs::write(&jar, b"jar").expect("jar");
        let classpath = jar.to_string_lossy().into_owned();

        let missing = missing_files(&spec(&java, classpath, &[]));

        assert!(missing.is_empty(), "{missing:?}");
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn missing_client_jar_and_java_are_named() {
        let root = temp_root();
        let java = root.join("java.exe");
        let jar = root.join("client.jar");

        let missing = missing_files(&spec(&java, jar.to_string_lossy().into_owned(), &[]));

        assert_eq!(missing, vec![jar, java]);
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn module_path_launches_are_checked_instead_of_the_classpath() {
        let root = temp_root();
        let java = root.join("java.exe");
        let module = root.join("bootstrap.jar");
        std::fs::write(&java, b"java").expect("java");
        std::fs::write(&module, b"jar").expect("module");
        let separator = classpath::separator();
        let module_path = format!(
            "{}{separator}{}",
            module.to_string_lossy(),
            root.join("missing.jar").to_string_lossy()
        );

        let missing = missing_files(&spec(
            &java,
            "ignored-classpath-entry.jar".into(),
            &["-p", &module_path],
        ));

        assert_eq!(missing, vec![root.join("missing.jar")]);
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn inspect_plan_labels_missing_client_jar_and_library() {
        let root = temp_root();
        let client_jar = root.join("versions/pack/pack.jar");
        let library = root.join("libraries/example/library-1.0.jar");
        std::fs::create_dir_all(root.join("versions/pack")).expect("pack dir");
        std::fs::write(&client_jar, b"jar").expect("client jar");
        let plan = LaunchPlan {
            strategy: crate::core::launch::plan::Strategy::Rebuild,
            loader: crate::core::loader::detect::LoaderInfo {
                kind: crate::core::loader::LoaderKind::Vanilla,
                version: None,
                required_java_major: Some(17),
            },
            java_major: 17,
            required: vec![
                RequiredFile {
                    path: client_jar,
                    relative: None,
                    role: FileRole::ClientJar,
                },
                RequiredFile {
                    path: library.clone(),
                    relative: Some(PathBuf::from("libraries/example/library-1.0.jar")),
                    role: FileRole::Library,
                },
            ],
        };

        let missing = inspect_plan(&plan);

        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].path, library);
        assert_eq!(missing[0].role, FileRole::Library);
        assert!(missing[0].relative.is_some());
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn inspect_checks_java_plan_files_and_module_path_entries() {
        let root = temp_root();
        let java = root.join("java.exe");
        let library = root.join("libraries/example/library-1.0.jar");
        let module = root.join("libraries/example/module.jar");
        let plan = LaunchPlan {
            strategy: crate::core::launch::plan::Strategy::Rebuild,
            loader: crate::core::loader::detect::LoaderInfo {
                kind: crate::core::loader::LoaderKind::Forge,
                version: None,
                required_java_major: Some(17),
            },
            java_major: 17,
            required: vec![RequiredFile {
                path: library.clone(),
                relative: Some(PathBuf::from("libraries/example/library-1.0.jar")),
                role: FileRole::Library,
            }],
        };
        let spec = CommandSpec {
            program: java.clone(),
            args: vec!["-p".into(), module.to_string_lossy().into_owned()],
            classpath: String::new(),
            current_dir: root.clone(),
        };

        let missing = inspect(&spec, &plan);

        assert_eq!(missing.len(), 3, "{missing:?}");
        assert!(
            missing
                .iter()
                .any(|file| file.path == java && file.role == FileRole::Java)
        );
        assert!(
            missing
                .iter()
                .any(|file| file.path == library && file.role == FileRole::Library)
        );
        assert!(
            missing
                .iter()
                .any(|file| file.path == module && file.role == FileRole::Library)
        );
        std::fs::remove_dir_all(root).expect("clean");
    }
}
