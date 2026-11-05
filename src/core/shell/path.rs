//! Is the launcher reachable from a new shell?
//!
//! `tmcl` lives wherever the user put the executable, so unless that directory
//! is on `PATH` the command has to be spelled out in full. This module answers
//! "is it there", and can add or remove exactly one entry without touching the
//! rest of the value.
//!
//! Two details are deliberate:
//!
//! - The user `Path` is edited, never the machine one. Writing `HKLM` needs an
//!   elevated process, and a launcher that runs unelevated should not ask for
//!   UAC just to make its own command work.
//! - Entries are compared after removing invisible characters. A `Path` copied
//!   from a chat window can carry a bidi mark (`U+202A`) that is invisible in
//!   the editor and turns the entry into a directory that does not exist; it
//!   looks present and works for nobody.

use std::path::{Path, PathBuf};

use crate::core::error::{Error, Result};

/// Characters that render as nothing but change the string.
///
/// Zero-width spaces and joiners, bidi embeddings/overrides/isolates, and the
/// byte-order mark. All of them have been seen in pasted `Path` entries.
const INVISIBLE: [char; 12] = [
    '\u{200B}', '\u{200C}', '\u{200D}', '\u{200E}', '\u{200F}', '\u{202A}', '\u{202B}', '\u{202C}',
    '\u{202D}', '\u{202E}', '\u{2060}', '\u{FEFF}',
];

/// What the current `Path` says about our directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Presence {
    /// An entry spells the directory exactly.
    Present,
    /// An entry is the same directory once invisible characters and a stray
    /// separator are removed — Windows resolves the literal string, so it does
    /// not work.
    Broken(String),
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathStatus {
    pub directory: PathBuf,
    pub presence: Presence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathChange {
    /// False when the value already said what it should.
    pub changed: bool,
    pub before: String,
    pub after: String,
    pub directory: PathBuf,
}

/// The directory holding this executable, which is what belongs on `PATH`.
pub fn launcher_directory() -> Result<PathBuf> {
    let executable = std::env::current_exe()
        .map_err(|error| Error::Config(format!("locate this executable: {error}")))?;
    executable
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| Error::Config("the executable has no directory".into()))
}

/// Strips what Windows would not ignore: surrounding quotes and blanks, the
/// invisible characters above, and one trailing separator.
pub fn normalise(entry: &str) -> String {
    let trimmed = entry
        .trim()
        .trim_matches('"')
        .chars()
        .filter(|character| !INVISIBLE.contains(character))
        .collect::<String>();
    let mut cleaned = trimmed.trim().to_owned();
    while cleaned.len() > 3 && (cleaned.ends_with('\\') || cleaned.ends_with('/')) {
        cleaned.pop();
    }
    cleaned
}

/// Splits a `Path` value into entries, dropping blanks.
pub fn entries(value: &str) -> Vec<String> {
    value
        .split(';')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_owned)
        .collect()
}

pub fn serialise(entries: &[String]) -> String {
    entries.join(";")
}

/// Does `entries` already point at `directory`, and does that entry work?
pub fn classify(entries: &[String], directory: &Path) -> Presence {
    let wanted = normalise(&directory.to_string_lossy());
    for entry in entries {
        let cleaned = normalise(entry);
        if cleaned.eq_ignore_ascii_case(&wanted) {
            // A trailing separator is harmless: Windows accepts both spellings.
            let mut canonical = cleaned.clone();
            canonical.push('\\');
            return if entry_eq(entry, &cleaned) || entry_eq(entry, &canonical) {
                Presence::Present
            } else {
                Presence::Broken(entry.clone())
            };
        }
    }
    Presence::Missing
}

fn entry_eq(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right)
}

/// Where `tmcl` lives and whether a new shell could find it.
pub fn status() -> Result<PathStatus> {
    let directory = launcher_directory()?;
    let value = crate::platform::windows::user_path_value()?;
    let presence = classify(&entries(&value), &directory);
    Ok(PathStatus {
        directory,
        presence,
    })
}

/// Adds the launcher directory, repairing a damaged spelling of it.
pub fn add() -> Result<PathChange> {
    let directory = launcher_directory()?;
    let before = crate::platform::windows::user_path_value()?;
    let (list, changed) = add_entry(&entries(&before), &directory);
    let after = if changed {
        serialise(&list)
    } else {
        // Nothing to do: report the value as it is, not as it would have been
        // re-serialised. Rebuilding the string would drop a trailing `;` or the
        // spaces around separators and make `changed` true for a no-op.
        before.clone()
    };
    if changed {
        crate::platform::windows::set_user_path_value(&after)?;
        crate::platform::windows::broadcast_environment_change();
    }
    Ok(PathChange {
        changed,
        before,
        after,
        directory,
    })
}

/// Removes every spelling of the launcher directory.
pub fn remove() -> Result<PathChange> {
    let directory = launcher_directory()?;
    let before = crate::platform::windows::user_path_value()?;
    let (list, changed) = remove_entry(&entries(&before), &directory);
    let after = if changed {
        serialise(&list)
    } else {
        before.clone()
    };
    if changed {
        crate::platform::windows::set_user_path_value(&after)?;
        crate::platform::windows::broadcast_environment_change();
    }
    Ok(PathChange {
        changed,
        before,
        after,
        directory,
    })
}

/// The entry list after an `add`, and whether it differs from the input.
///
/// `changed` means "this directory was added or repaired", not "the rebuilt
/// string differs": a value such as `C:\a;D:\b;` re-serialises to `C:\a;D:\b`,
/// and reporting that as a change would print "Added …" for a directory that
/// was already there.
fn add_entry(list: &[String], directory: &Path) -> (Vec<String>, bool) {
    let wanted = normalise(&directory.to_string_lossy());
    let mut list = list.to_vec();
    match classify(&list, directory) {
        Presence::Present => (list, false),
        Presence::Broken(broken) => match list.iter_mut().find(|entry| **entry == broken) {
            Some(slot) => {
                *slot = wanted;
                (list, true)
            }
            // `classify` only reports a `Broken` entry it found in this list.
            None => (list, false),
        },
        Presence::Missing => {
            list.push(wanted);
            (list, true)
        }
    }
}

/// The entry list after a `remove`, and whether anything was actually removed.
fn remove_entry(list: &[String], directory: &Path) -> (Vec<String>, bool) {
    let wanted = normalise(&directory.to_string_lossy());
    let kept = list
        .iter()
        .filter(|entry| !normalise(entry).eq_ignore_ascii_case(&wanted))
        .cloned()
        .collect::<Vec<_>>();
    let changed = kept.len() != list.len();
    (kept, changed)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{Presence, add_entry, classify, entries, normalise, remove_entry, serialise};

    #[test]
    fn invisible_characters_are_dropped_from_an_entry() {
        // The exact failure users hit: the entry looks right and resolves to a
        // directory that does not exist.
        let dirty = "\u{202A}D:\\game\\pcl\\";
        assert_eq!(normalise(dirty), "D:\\game\\pcl");
        assert_eq!(normalise("  \"D:\\games\"  "), "D:\\games");
        // A drive root keeps its separator.
        assert_eq!(normalise("C:\\"), "C:\\");
    }

    #[test]
    fn entries_skip_blank_items() {
        assert_eq!(
            entries("C:\\a;; D:\\b ;"),
            vec!["C:\\a".to_owned(), "D:\\b".to_owned()]
        );
        assert_eq!(serialise(&entries("C:\\a;;D:\\b")), "C:\\a;D:\\b");
    }

    #[test]
    fn a_damaged_entry_is_broken_not_present() {
        let directory = Path::new("D:\\game\\pcl");
        let dirty = vec![
            "C:\\Windows".to_owned(),
            "\u{202A}D:\\game\\pcl\\".to_owned(),
        ];

        assert_eq!(
            classify(&dirty, directory),
            Presence::Broken("\u{202A}D:\\game\\pcl\\".to_owned())
        );
    }

    #[test]
    fn a_clean_entry_counts_as_present_in_either_spelling() {
        let directory = Path::new("D:\\game\\pcl");
        for entry in ["D:\\game\\pcl", "D:\\game\\pcl\\", "d:\\game\\pcl"] {
            assert_eq!(
                classify(&[entry.to_owned()], directory),
                Presence::Present,
                "{entry}"
            );
        }
    }

    #[test]
    fn an_unrelated_path_is_missing() {
        let directory = Path::new("D:\\game\\pcl");
        assert_eq!(
            classify(&["C:\\Windows".to_owned()], directory),
            Presence::Missing
        );
    }

    #[test]
    fn adding_an_entry_that_is_already_there_reports_no_change() {
        // A trailing `;` disappears when the value is re-serialised. That must
        // not count as a change: nothing was added.
        let list = entries("C:\\a;D:\\game\\pcl;");
        let (after, changed) = add_entry(&list, Path::new("D:\\game\\pcl"));

        assert!(!changed);
        assert_eq!(after, list);
    }

    #[test]
    fn adding_repairs_a_damaged_entry_without_growing_the_list() {
        let list = entries("C:\\a;\u{202A}D:\\game\\pcl\\");
        let (after, changed) = add_entry(&list, Path::new("D:\\game\\pcl"));

        assert!(changed);
        assert_eq!(after, ["C:\\a", "D:\\game\\pcl"]);
        assert_eq!(after.len(), list.len());
    }

    #[test]
    fn adding_appends_when_the_entry_is_missing() {
        let list = entries("C:\\a");
        let (after, changed) = add_entry(&list, Path::new("D:\\game\\pcl"));

        assert!(changed);
        assert_eq!(after, ["C:\\a", "D:\\game\\pcl"]);
    }

    #[test]
    fn removing_an_absent_entry_reports_no_change() {
        let list = entries("C:\\a;D:\\b;");
        let (after, changed) = remove_entry(&list, Path::new("E:\\gone"));

        assert!(!changed);
        assert_eq!(after, list);
    }

    #[test]
    fn removing_drops_every_spelling_of_the_directory() {
        let list = entries("C:\\a;D:\\game\\pcl;\u{202A}d:\\game\\pcl\\;D:\\b");
        let (after, changed) = remove_entry(&list, Path::new("D:\\game\\pcl"));

        assert!(changed);
        assert_eq!(after, ["C:\\a", "D:\\b"]);
    }
}
