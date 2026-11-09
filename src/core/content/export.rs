//! Export an instance as a self-contained modpack zip.
//!
//! The archive carries `mods/`, `config/`, a snapshot of `tmcl.json` and a
//! `manifest.json` shaped like a CurseForge manifest, so the result can be fed
//! straight back into the launcher's modpack importer. The manifest's `files`
//! array stays empty on purpose: the mods are inside the archive, not fetched
//! from a CDN, which is what makes the export usable offline.
//!
//! The `tmcl.json` entry is reference material for other tools. Importers must not
//! copy it over the destination instance: `extract_modpack_archive` skips that name
//! so a pack cannot overwrite the identity of the instance it is installed into.

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::core::error::{Error, Result};
use crate::core::instance::models::{InstanceConfig, InstanceLoader};

pub const MANIFEST_NAME: &str = "manifest.json";
pub const METADATA_NAME: &str = "tmcl.json";
/// Directories copied verbatim into the archive, in this order.
const CONTENT_DIRS: [&str; 2] = ["mods", "config"];
/// Guard against a symlink loop while walking the instance directory.
const MAX_DEPTH: u8 = 12;

/// Pack `instance_root` into `target`.
///
/// `target` may be a directory (the archive is then named after the instance)
/// or a file path without an extension (`.zip` is appended). Existing files are
/// never replaced: an export is not worth losing a previous one over.
pub fn export_instance(
    instance_root: &Path,
    instance: &InstanceConfig,
    target: &Path,
) -> Result<PathBuf> {
    let target = normalize_export_target(target, &instance.name)?;
    if target.exists() {
        return Err(Error::Config(format!(
            "the export target already exists: {}",
            target.display()
        )));
    }
    let root = instance_root
        .canonicalize()
        .unwrap_or_else(|_| instance_root.to_path_buf());
    let parent = target
        .parent()
        .and_then(|parent| parent.canonicalize().ok());
    if parent.is_some_and(|parent| parent.starts_with(&root)) {
        // The archive would be inside the tree it packs, and its `.part` file
        // could be picked up by the walk while it is still being written.
        return Err(Error::Config(
            "the export target must be outside the instance directory".into(),
        ));
    }
    let manifest = manifest(instance_root, instance);
    let encoded = serde_json::to_vec_pretty(&manifest)?;

    // Write beside the destination first: a failure halfway through must not
    // leave a truncated archive where the user expects a complete one.
    let file_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::Config("export target has no file name".into()))?;
    let staging = target.with_file_name(format!(".{file_name}.part"));
    let result = write_archive(instance_root, &staging, &encoded);
    if let Err(error) = result {
        let _ = fs::remove_file(&staging);
        return Err(error);
    }
    fs::rename(&staging, &target).inspect_err(|_| {
        let _ = fs::remove_file(&staging);
    })?;
    Ok(target)
}

/// Resolve what the player typed into a concrete `.zip` path.
pub fn normalize_export_target(input: &Path, instance_name: &str) -> Result<PathBuf> {
    let mut path = input.to_path_buf();
    if path.is_dir() {
        path = path.join(suggested_archive_name(instance_name));
    } else if path.extension().is_none() {
        path.set_extension("zip");
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::Config("export target has no file name".into()))?
        .to_owned();
    crate::core::resource::conflict::validate_file_name(&name).map_err(Error::Config)?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !parent.is_dir() {
        return Err(Error::Config(format!(
            "export directory does not exist: {}",
            parent.display()
        )));
    }
    Ok(if parent == Path::new(".") {
        PathBuf::from(name)
    } else {
        parent.join(name)
    })
}

/// File name an export gets when the player picks a directory.
pub fn suggested_archive_name(instance_name: &str) -> String {
    let stem = instance_name
        .chars()
        .map(|character| {
            if character.is_control() || r#"<>:"/\|?*"#.contains(character) {
                '-'
            } else {
                character
            }
        })
        .collect::<String>();
    let stem = stem.trim().trim_end_matches('.').trim();
    if stem.is_empty() {
        "modpack.zip".to_owned()
    } else {
        format!("{stem}.zip")
    }
}

/// CurseForge-shaped manifest with a `tmcl` block for the extra details.
fn manifest(instance_root: &Path, instance: &InstanceConfig) -> serde_json::Value {
    let (kind, version) = loader_parts(&instance.loader);
    let mod_loaders = match version {
        Some(version) => json!([{ "id": format!("{kind}-{version}"), "primary": true }]),
        None => json!([]),
    };
    json!({
        "manifestType": "minecraftModpack",
        "manifestVersion": 1,
        "name": instance.name,
        "version": "1.0.0",
        "author": "TMCL",
        "minecraft": {
            "version": instance.minecraft_version,
            "modLoaders": mod_loaders,
        },
        // Mods are bundled; nothing to download.
        "files": [],
        "tmcl": {
            "format": 1,
            "instance": instance.name,
            "minecraft_version": instance.minecraft_version,
            "loader": { "kind": kind, "version": version },
            "mods": mod_names(instance_root),
        },
    })
}

fn loader_parts(loader: &InstanceLoader) -> (&'static str, Option<&str>) {
    match loader {
        InstanceLoader::Vanilla => ("vanilla", None),
        InstanceLoader::Fabric { version } => ("fabric", Some(version.as_str())),
        InstanceLoader::Quilt { version } => ("quilt", Some(version.as_str())),
        InstanceLoader::Forge { version } => ("forge", Some(version.as_str())),
        InstanceLoader::NeoForge { version } => ("neoforge", Some(version.as_str())),
    }
}

/// File names of everything the mods directory holds, disabled entries
/// included: the `.disabled` suffix is part of the state being exported.
fn mod_names(instance_root: &Path) -> Vec<String> {
    let directory = instance_root.join("mods");
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut names = entries
        .flatten()
        .filter(|entry| entry.path().is_file())
        .filter(|entry| crate::core::content::ContentKind::Mod.accepts(&entry.path()))
        .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn write_archive(instance_root: &Path, staging: &Path, manifest: &[u8]) -> Result<()> {
    let file = File::create(staging)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);
    zip.start_file(MANIFEST_NAME, options)
        .map_err(io::Error::other)?;
    io::Write::write_all(&mut zip, manifest)?;
    let metadata = instance_root.join(METADATA_NAME);
    if metadata.is_file() {
        zip.start_file(METADATA_NAME, options)
            .map_err(io::Error::other)?;
        let mut source = File::open(&metadata)?;
        io::copy(&mut source, &mut zip)?;
    }
    for directory in CONTENT_DIRS {
        let source = instance_root.join(directory);
        add_tree(&mut zip, &source, &source, directory, 0, options)?;
    }
    zip.finish().map_err(io::Error::other)?;
    Ok(())
}

fn add_tree(
    zip: &mut zip::ZipWriter<File>,
    source: &Path,
    current: &Path,
    prefix: &str,
    depth: u8,
    options: zip::write::SimpleFileOptions,
) -> Result<()> {
    if depth > MAX_DEPTH {
        return Ok(());
    }
    let Ok(entries) = fs::read_dir(current) else {
        // A missing `mods/` or `config/` is normal on a fresh instance.
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(relative) = path.strip_prefix(source) else {
            continue;
        };
        let name = format!("{prefix}/{}", relative.to_string_lossy().replace('\\', "/"));
        if path.is_dir() {
            zip.add_directory(format!("{name}/"), options)
                .map_err(io::Error::other)?;
            add_tree(zip, source, &path, prefix, depth + 1, options)?;
        } else if path.is_file() {
            zip.start_file(name, options).map_err(io::Error::other)?;
            let mut file = File::open(&path)?;
            io::copy(&mut file, zip)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{export_instance, normalize_export_target};
    use crate::core::instance::models::{InstanceConfig, InstanceLoader};
    use std::path::{Path, PathBuf};

    struct TempRoot(PathBuf);

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn instance(tag: &str) -> (TempRoot, PathBuf, InstanceConfig) {
        let base = TempRoot(
            std::env::temp_dir().join(format!("tmcl-export-{tag}-{}", uuid::Uuid::new_v4())),
        );
        // One level down, so an export target can sit beside the instance
        // without tripping the "outside the instance" guard.
        let root = base.0.join("instance");
        std::fs::create_dir_all(root.join("mods")).expect("create mods");
        std::fs::create_dir_all(root.join("config/nested")).expect("create config");
        std::fs::write(root.join("mods/alpha.jar"), b"alpha").expect("write mod");
        std::fs::write(root.join("mods/beta.jar.disabled"), b"beta").expect("write mod");
        std::fs::write(root.join("mods/readme.txt"), b"skip").expect("write readme");
        std::fs::write(root.join("config/demo.txt"), b"config").expect("write config");
        std::fs::write(root.join("config/nested/deep.txt"), b"deep").expect("write config");
        let mut config = InstanceConfig::new(&root, uuid::Uuid::new_v4(), "demo", "1.20.1");
        config.loader = InstanceLoader::Forge {
            version: "47.4.22".into(),
        };
        crate::core::config::storage::write_json_atomic(&root.join("tmcl.json"), &config)
            .expect("write metadata");
        let path = root.clone();
        (base, path, config)
    }

    fn zip_names(path: &Path) -> Vec<String> {
        let file = std::fs::File::open(path).expect("open zip");
        let mut archive = zip::ZipArchive::new(file).expect("read zip");
        (0..archive.len())
            .map(|index| archive.by_index(index).expect("entry").name().to_owned())
            .collect()
    }

    #[test]
    fn export_contains_manifest_and_mods() {
        let (_root, root, config) = instance("contents");
        let target = root.parent().expect("base").join("demo.zip");
        let archive = export_instance(&root, &config, &target).expect("export");
        assert_eq!(archive, target);

        let names = zip_names(&archive);
        for expected in [
            "manifest.json",
            "tmcl.json",
            "mods/alpha.jar",
            "mods/beta.jar.disabled",
            "mods/readme.txt",
            "config/demo.txt",
            "config/nested/deep.txt",
        ] {
            assert!(
                names.contains(&expected.to_owned()),
                "{expected} missing: {names:?}"
            );
        }

        let file = std::fs::File::open(&archive).expect("open archive");
        let mut zip = zip::ZipArchive::new(file).expect("read archive");
        let mut text = String::new();
        std::io::Read::read_to_string(
            &mut zip.by_name("manifest.json").expect("manifest"),
            &mut text,
        )
        .expect("read manifest");
        let manifest: serde_json::Value = serde_json::from_str(&text).expect("parse manifest");
        assert_eq!(manifest["name"], "demo");
        assert_eq!(manifest["minecraft"]["version"], "1.20.1");
        assert_eq!(
            manifest["minecraft"]["modLoaders"][0]["id"],
            "forge-47.4.22"
        );
        assert_eq!(manifest["tmcl"]["mods"][0], "alpha.jar");
        assert_eq!(manifest["tmcl"]["mods"][1], "beta.jar.disabled");
        assert_eq!(
            manifest["tmcl"]["mods"]
                .as_array()
                .expect("mods array")
                .len(),
            2,
            "the manifest lists mods, not every file in the directory"
        );
    }

    #[test]
    fn export_is_readable_as_a_modpack() {
        let (_root, root, config) = instance("import");
        let target = root.parent().expect("base").join("demo.zip");
        let archive = export_instance(&root, &config, &target).expect("export");
        let metadata =
            crate::core::resource::modpack::read_modpack_metadata(&archive).expect("metadata");
        assert_eq!(metadata.minecraft_version, "1.20.1");
        assert_eq!(metadata.loader, "forge");
        assert_eq!(metadata.loader_version, "47.4.22");
        // The mods travel inside the archive, so the importer has nothing to
        // download and must keep `files` empty.
        assert!(metadata.files.is_empty(), "{:?}", metadata.files);
    }

    #[test]
    fn export_never_overwrites_an_existing_archive() {
        let (_root, root, config) = instance("overwrite");
        let target = root.parent().expect("base").join("demo.zip");
        std::fs::write(&target, b"keep").expect("write existing");
        assert!(export_instance(&root, &config, &target).is_err());
        assert_eq!(std::fs::read(&target).expect("read target"), b"keep");
    }

    #[test]
    fn export_target_must_be_outside_the_instance() {
        let (_root, root, config) = instance("inside");
        let target = root.join("mods/pack.zip");
        let error = export_instance(&root, &config, &target).expect_err("inside target");
        assert!(error.to_string().contains("outside"), "{error}");
        assert!(!target.exists());
    }

    #[test]
    fn export_target_accepts_directories_and_bare_names() {
        let (_root, root, _config) = instance("targets");
        let base = root.parent().expect("base");
        let directory = base.join("out");
        std::fs::create_dir_all(&directory).expect("create out");
        assert_eq!(
            normalize_export_target(&directory, "demo").expect("directory"),
            directory.join("demo.zip")
        );
        assert_eq!(
            normalize_export_target(&base.join("pack"), "demo").expect("bare name"),
            base.join("pack.zip")
        );
        assert_eq!(
            normalize_export_target(&base.join("pack.zip"), "demo").expect("file"),
            base.join("pack.zip")
        );
    }
}
