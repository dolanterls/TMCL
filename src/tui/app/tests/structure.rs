//! Structural tests: the directory layout is part of the contract.
//!
//! Adding a preset means adding one directory under `app/view/` and one enum
//! variant. These tests read the tree off disk, so a shortcut fails the build
//! instead of being discovered months later: a second top-level folder for a
//! preset, a preset reaching into a sibling's files, a "shared" file that moved
//! into one preset.

use std::fs;
use std::path::{Path, PathBuf};

use crate::core::config::models::UiPreset;

fn tui_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tui")
}

/// Directories directly under `path`, sorted, ignoring anything that is not a
/// directory.
fn subdirectories(path: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn rust_files(path: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![path.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in fs::read_dir(&current)
            .unwrap_or_else(|error| panic!("read {}: {error}", current.display()))
            .filter_map(|entry| entry.ok())
        {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }
    files
}

/// One directory per preset, no more and no fewer.
///
/// This is what keeps a new preset from inventing its own top-level folder: the
/// only place it can live is `app/view/<code>/`, and it has to be a real
/// `UiPreset` variant to exist here at all.
#[test]
fn every_view_directory_is_a_registered_preset() {
    let mut expected: Vec<String> = UiPreset::ALL
        .iter()
        .map(|preset| preset.code().to_owned())
        .collect();
    expected.sort();
    assert_eq!(
        subdirectories(&tui_root().join("app/view")),
        expected,
        "app/view must hold exactly one directory per UiPreset variant"
    );
}

/// `src/tui` itself stays shallow: the shared kit and the models, nothing else.
///
/// A new directory here starts the same mess as before, when one preset became
/// three differently-named top-level folders.
#[test]
fn the_tui_root_holds_only_the_shared_layers() {
    assert_eq!(
        subdirectories(&tui_root()),
        ["app", "modal", "pages", "widgets"],
        "a preset belongs in `app/view/<code>`, not at the `tui` root"
    );
}

/// A preset draws with the shared kit or with its own files, never with a
/// sibling's.
///
/// Two presets needing the same thing is the signal to move it up into
/// `crate::tui` (`palette`, `widgets`, `chrome`) or into `app/view`'s shared
/// helpers, not to import across preset boundaries.
#[test]
fn a_preset_never_references_another_preset() {
    let view = tui_root().join("app/view");
    let presets: Vec<String> = UiPreset::ALL
        .iter()
        .map(|preset| preset.code().to_owned())
        .collect();

    let mut violations = Vec::new();
    for preset in &presets {
        let directory = view.join(preset);
        for file in rust_files(&directory) {
            let source = fs::read_to_string(&file).expect("read source");
            for other in presets.iter().filter(|other| *other != preset) {
                let needle = format!("view::{other}::");
                if source.contains(&needle) {
                    violations.push(format!("{} references {needle}", file.display()));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "presets must not import each other:\n{}",
        violations.join("\n")
    );
}
