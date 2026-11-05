//! Native library extraction.
//!
//! Minecraft ships platform natives as classifier jars inside `libraries/`.
//! LWJGL 3 (1.13+) can unpack those itself, but LWJGL 2 (1.12.2 and older)
//! loads `lwjgl.dll`/`liblwjgl.so` from `java.library.path`, so every launch
//! has to materialise them. Instances imported from other launchers happen to
//! carry a natives folder, which is why the gap stayed invisible for a while;
//! an instance TMCL created has none.
//!
//! The extraction is idempotent: a stamp file records which archives (and
//! hashes) produced the directory, so a second launch does no work and a
//! changed archive triggers a clean re-extract.

use std::fs::{self, File};
use std::io::copy;
use std::path::{Path, PathBuf};

use crate::core::error::{Error, Result};
use crate::core::loader::client_json::ClientJson;

/// Bump when the extraction rules change; it invalidates every stamp.
const FORMAT_VERSION: u32 = 1;
const STAMP_NAME: &str = ".tmcl-natives.stamp";
/// Natives never need these; keeping them out mirrors what a launcher folder
/// looks like after an official launch.
const ALWAYS_SKIP: [&str; 2] = ["META-INF/", ".git"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeArchive {
    /// Absolute path of the classifier jar.
    pub path: PathBuf,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    /// Entry prefixes the metadata says not to extract.
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativePlan {
    /// Where the native files belong (`<instance>/.tmcl-natives`).
    pub directory: PathBuf,
    pub archives: Vec<NativeArchive>,
}

impl NativePlan {
    /// Whether there is anything to unpack. A version without natives (some
    /// server-only or stripped metadata) keeps the launcher-side folder out of
    /// the picture and lets the existing directory fallback decide.
    pub fn is_empty(&self) -> bool {
        self.archives.is_empty()
    }
}

/// Collects the native archives for this client.
pub fn plan(instance_dir: &Path, game_directory: &Path, client: &ClientJson) -> NativePlan {
    let mut archives = client
        .native_artifacts()
        .into_iter()
        .map(|(library, artifact)| NativeArchive {
            path: game_directory.join(&artifact.path),
            sha1: artifact.sha1.clone(),
            size: artifact.size,
            exclude: library
                .extract
                .as_ref()
                .map(|extract| extract.exclude.clone())
                .unwrap_or_default(),
        })
        .collect::<Vec<_>>();
    // Stable order keeps the stamp stable across runs.
    archives.sort_by(|left, right| left.path.cmp(&right.path));
    archives.dedup_by(|left, right| left.path == right.path);
    NativePlan {
        directory: instance_dir.join(".tmcl-natives"),
        archives,
    }
}

/// Extracts the plan's archives into [`NativePlan::directory`].
///
/// Returns the directory the game should put on `java.library.path`. When the
/// plan is empty the directory is returned but not created: `command.rs` then
/// falls back to a folder another launcher already prepared.
pub fn prepare(instance_dir: &Path, game_directory: &Path, client: &ClientJson) -> Result<PathBuf> {
    let plan = plan(instance_dir, game_directory, client);
    if plan.is_empty() {
        return Ok(plan.directory);
    }
    prepare_plan(&plan)
}

pub fn prepare_plan(plan: &NativePlan) -> Result<PathBuf> {
    let stamp = plan.directory.join(STAMP_NAME);
    let expected = fingerprint(&plan.archives);
    if plan.directory.is_dir() && fs::read_to_string(&stamp).is_ok_and(|value| value == expected) {
        return Ok(plan.directory.clone());
    }

    // A stamp mismatch means the jars changed or the folder is half-written.
    // Starting from an empty directory is the only way to know a stale DLL is
    // not left behind.
    if plan.directory.is_dir() {
        fs::remove_dir_all(&plan.directory).map_err(|error| {
            Error::Launch(format!(
                "clear native directory {}: {error}",
                plan.directory.display()
            ))
        })?;
    } else if plan.directory.exists() {
        fs::remove_file(&plan.directory).map_err(|error| {
            Error::Launch(format!(
                "clear native path {}: {error}",
                plan.directory.display()
            ))
        })?;
    }
    fs::create_dir_all(&plan.directory).map_err(|error| {
        Error::Launch(format!(
            "create native directory {}: {error}",
            plan.directory.display()
        ))
    })?;

    for archive in &plan.archives {
        extract_archive(archive, &plan.directory)?;
    }
    fs::write(&stamp, &expected).map_err(|error| {
        Error::Launch(format!("write native stamp {}: {error}", stamp.display()))
    })?;
    Ok(plan.directory.clone())
}

fn extract_archive(archive: &NativeArchive, destination: &Path) -> Result<()> {
    let file = File::open(&archive.path).map_err(|error| {
        Error::Launch(format!(
            "open native archive {}: {error}",
            archive.path.display()
        ))
    })?;
    let mut zip = zip::ZipArchive::new(file).map_err(|error| {
        Error::Launch(format!(
            "read native archive {}: {error}",
            archive.path.display()
        ))
    })?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|error| {
            Error::Launch(format!(
                "read native entry in {}: {error}",
                archive.path.display()
            ))
        })?;
        let Some(relative) = entry.enclosed_name() else {
            return Err(Error::Launch(format!(
                "native archive {} contains an unsafe path",
                archive.path.display()
            )));
        };
        if entry.is_dir() || should_skip(&relative, &archive.exclude) {
            continue;
        }
        let target = destination.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                Error::Launch(format!(
                    "create native folder {}: {error}",
                    parent.display()
                ))
            })?;
        }
        let mut output = File::create(&target).map_err(|error| {
            Error::Launch(format!("write native file {}: {error}", target.display()))
        })?;
        copy(&mut entry, &mut output).map_err(|error| {
            Error::Launch(format!("extract native file {}: {error}", target.display()))
        })?;
    }
    Ok(())
}

fn should_skip(relative: &Path, exclude: &[String]) -> bool {
    let name = relative.to_string_lossy().replace('\\', "/");
    if ALWAYS_SKIP
        .iter()
        .any(|prefix| name == *prefix || name.starts_with(prefix))
    {
        return true;
    }
    // `extract.exclude` is a prefix list, not a glob.
    if exclude
        .iter()
        .any(|prefix| name == *prefix || name.starts_with(prefix))
    {
        return true;
    }
    name.ends_with(".sha1") || name.ends_with(".git")
}

fn fingerprint(archives: &[NativeArchive]) -> String {
    let mut value = format!("v{FORMAT_VERSION}\n");
    for archive in archives {
        value.push_str(&archive.path.to_string_lossy());
        value.push('\t');
        value.push_str(archive.sha1.as_deref().unwrap_or(""));
        value.push('\t');
        if let Some(size) = archive.size {
            value.push_str(&size.to_string());
        }
        // Metadata is not always present (instances written by other
        // launchers), so the file itself is part of the fingerprint too.
        if let Ok(metadata) = fs::metadata(&archive.path) {
            value.push('\t');
            value.push_str(&metadata.len().to_string());
            if let Ok(modified) = metadata.modified()
                && let Ok(age) = modified.duration_since(std::time::UNIX_EPOCH)
            {
                value.push('\t');
                value.push_str(&age.as_secs().to_string());
            }
        }
        value.push('\n');
    }
    value
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::{Path, PathBuf};

    use super::prepare;
    use crate::core::loader::client_json::parse;

    fn temp_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("tmcl-natives-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("versions/pack")).expect("instance dir");
        root
    }

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("library dir");
        let file = std::fs::File::create(path).expect("zip file");
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for (name, content) in entries {
            zip.start_file(*name, options).expect("zip entry");
            zip.write_all(content).expect("zip body");
        }
        zip.finish().expect("zip finish");
    }

    /// The classifier name the running platform will look for.
    fn native_classifier() -> String {
        let arch = if std::env::consts::ARCH.contains("64") {
            "64"
        } else {
            "32"
        };
        format!("natives-{}-{arch}", std::env::consts::OS)
    }

    fn client(zip_name: &str) -> crate::core::loader::client_json::ClientJson {
        client_with_size(zip_name, 0)
    }

    fn client_with_size(zip_name: &str, size: u64) -> crate::core::loader::client_json::ClientJson {
        let os = std::env::consts::OS;
        let size = if size == 0 {
            String::new()
        } else {
            format!(r#","size":{size}"#)
        };
        let json = format!(
            r#"{{"id":"pack","mainClass":"net.minecraft.Main",
                 "libraries":[{{"name":"org.lwjgl:lwjgl:2.9.4",
                   "natives":{{"{os}":"natives-{os}-${{arch}}"}},
                   "extract":{{"exclude":["excluded/"]}},
                   "downloads":{{"classifiers":{{"{}":{{"path":"libraries/{zip_name}","url":"https://example.invalid/{zip_name}"{size}}}}}}}}}]}}"#,
            native_classifier()
        );
        parse(json.as_bytes()).expect("client json")
    }

    #[test]
    fn extracts_native_archives_and_skips_metadata_and_excluded_entries() {
        let root = temp_root();
        let zip_name = "lwjgl-natives.jar";
        write_zip(
            &root.join("libraries").join(zip_name),
            &[
                ("lwjgl.dll", b"dll"),
                ("META-INF/MANIFEST.MF", b"manifest"),
                ("lib.sha1", b"hash"),
                ("excluded/broken.dll", b"broken"),
            ],
        );
        let client = client(zip_name);

        let directory = prepare(&root.join("versions/pack"), &root, &client).expect("extract");

        assert_eq!(directory, root.join("versions/pack/.tmcl-natives"));
        assert!(directory.join("lwjgl.dll").is_file());
        assert!(!directory.join("META-INF/MANIFEST.MF").exists());
        assert!(!directory.join("lib.sha1").exists());
        assert!(!directory.join("excluded/broken.dll").exists());
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn a_second_prepare_reuses_the_extracted_directory() {
        let root = temp_root();
        let zip_name = "lwjgl-natives.jar";
        write_zip(
            &root.join("libraries").join(zip_name),
            &[("lwjgl.dll", b"dll")],
        );
        let client = client(zip_name);
        let instance = root.join("versions/pack");

        let directory = prepare(&instance, &root, &client).expect("first extract");
        std::fs::write(directory.join("marker"), b"keep").expect("marker");
        prepare(&instance, &root, &client).expect("second prepare");

        assert!(
            directory.join("marker").is_file(),
            "an unchanged plan must not wipe the directory"
        );
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn a_changed_archive_forces_a_clean_reextract() {
        let root = temp_root();
        let zip_name = "lwjgl-natives.jar";
        let archive = root.join("libraries").join(zip_name);
        write_zip(&archive, &[("lwjgl.dll", b"old")]);
        let instance = root.join("versions/pack");
        prepare(&instance, &root, &client(zip_name)).expect("first extract");
        let directory = instance.join(".tmcl-natives");
        std::fs::write(directory.join("stale.dll"), b"stale").expect("stale file");

        // Re-downloading a native jar changes the metadata fingerprint; the
        // directory has to be rebuilt so no stale DLL survives.
        write_zip(&archive, &[("lwjgl.dll", b"new")]);
        prepare(&instance, &root, &client_with_size(zip_name, 999)).expect("re-extract");

        assert!(!directory.join("stale.dll").exists(), "stale file survived");
        assert_eq!(
            std::fs::read(directory.join("lwjgl.dll")).expect("dll"),
            b"new"
        );
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn an_instance_without_natives_keeps_the_directory_absent() {
        let root = temp_root();
        let client = parse(br#"{"id":"pack","mainClass":"net.minecraft.Main","libraries":[]}"#)
            .expect("client json");
        let directory = prepare(&root.join("versions/pack"), &root, &client).expect("prepare");
        assert!(!directory.exists(), "nothing to extract, nothing to create");
        std::fs::remove_dir_all(root).expect("clean");
    }
}
