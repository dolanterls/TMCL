use std::path::{Path, PathBuf};

/// Strips Minecraft formatting codes from a name that becomes a file name.
///
/// A few resource packs are published with their in-game colour codes left in
/// the file name (`§7Stay True§0 1.21.1.zip`). The game hides them, but every
/// file manager and every `mods`/`resourcepacks` listing shows the raw `§7`,
/// so the name is cleaned before it is used as a path segment. Control
/// characters go too: they are invisible and `validate_file_name` rejects them
/// anyway.
pub fn sanitize_file_name(name: &str) -> String {
    let mut cleaned = String::with_capacity(name.len());
    let mut characters = name.chars();
    while let Some(character) = characters.next() {
        if character == '\u{a7}' {
            // A code is the section sign plus exactly one formatting
            // character; a trailing sign stands for nothing and is dropped.
            characters.next();
            continue;
        }
        if !character.is_control() {
            cleaned.push(character);
        }
    }
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return name.trim().to_owned();
    }
    // `§7.zip` cleans to `.zip`, which has no stem and would hide the pack.
    if cleaned.starts_with('.') && !name.trim_start().starts_with('.') {
        return format!("pack{cleaned}");
    }
    cleaned.to_owned()
}

pub fn validate_file_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return Err("file name cannot be empty or a path segment".into());
    }
    if trimmed.contains(['/', '\\', ':']) || trimmed.chars().any(|ch| ch.is_control()) {
        return Err("file name contains a path separator or control character".into());
    }
    if trimmed.ends_with([' ', '.']) {
        return Err("file name cannot end with a space or dot".into());
    }
    let stem = trimmed
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ]
    .contains(&stem.as_str())
    {
        return Err("file name is a reserved Windows device name".into());
    }
    Ok(())
}

pub fn check_available(path: &Path) -> Result<(), String> {
    if path.exists() {
        Err(format!("file already exists: {}", path.display()))
    } else {
        Ok(())
    }
}

pub fn normalize_target(path: &Path) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .ok_or_else(|| "target has no parent directory".to_owned())?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "target has no valid file name".to_owned())?;
    validate_file_name(name)?;
    Ok(parent.join(name))
}

#[cfg(test)]
mod tests {
    use super::{sanitize_file_name, validate_file_name};

    #[test]
    fn rejects_traversal_and_reserved_names() {
        assert!(validate_file_name("..\\outside").is_err());
        assert!(validate_file_name("CON.jar").is_err());
        assert!(validate_file_name("good.jar").is_ok());
    }

    #[test]
    fn formatting_codes_are_dropped_from_a_published_name() {
        assert_eq!(
            sanitize_file_name("\u{a7}7Stay True\u{a7}0 1.21.1.zip"),
            "Stay True 1.21.1.zip"
        );
        // A trailing sign belongs to no code and is removed with the rest.
        assert_eq!(sanitize_file_name("pack\u{a7}"), "pack");
        // Nothing to clean stays exactly as it was.
        assert_eq!(sanitize_file_name("Sodium 0.6.jar"), "Sodium 0.6.jar");
    }

    #[test]
    fn a_name_that_was_only_codes_does_not_become_a_hidden_file() {
        assert_eq!(sanitize_file_name("\u{a7}7.zip"), "pack.zip");
        // A name the author wrote with a leading dot is left alone.
        assert_eq!(sanitize_file_name(".hidden.zip"), ".hidden.zip");
        // And an empty result keeps the original rather than inventing one.
        assert_eq!(sanitize_file_name("\u{a7}7"), "\u{a7}7");
    }

    #[test]
    fn a_cleaned_name_is_still_valid() {
        assert!(validate_file_name(&sanitize_file_name("\u{a7}7Stay True\u{a7}0.zip")).is_ok());
    }
}
