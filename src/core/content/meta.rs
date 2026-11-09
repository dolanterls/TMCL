//! Metadata readers for the four content kinds.
//!
//! Mods ship at least four different descriptors in the wild, so the reader
//! tries them in a fixed order and falls back to the file name. A descriptor
//! that cannot be read never fails the whole scan: the entry is still listed.

use std::path::Path;

use serde_json::Value;

use crate::core::content::ContentMeta;

const FABRIC: &str = "fabric.mod.json";
const NEOFORGE: &str = "META-INF/neoforge.mods.toml";
const FORGE: &str = "META-INF/mods.toml";
const LEGACY_FORGE: &str = "mcmod.info";
const PACK: &str = "pack.mcmeta";

/// Read mod metadata, newest descriptor format first.
pub fn parse_mod(path: &Path) -> Option<ContentMeta> {
    let mut meta = if let Some(meta) = read_entry(path, FABRIC).and_then(|bytes| fabric(&bytes)) {
        meta
    } else if let Some(meta) = [NEOFORGE, FORGE]
        .iter()
        .find_map(|descriptor| read_entry(path, descriptor).and_then(|bytes| forge(&bytes)))
    {
        meta
    } else {
        read_entry(path, LEGACY_FORGE).and_then(|bytes| legacy_forge(&bytes))?
    };
    // `${file.jarVersion}` and empty versions are common in Forge builds. The
    // jar manifest still carries the real version, so use it instead of
    // leaving the column blank.
    if let ContentMeta::Mod { version, .. } = &mut meta
        && version.is_empty()
    {
        *version = manifest_version(path).unwrap_or_default();
    }
    Some(meta)
}

/// `Implementation-Version`, with `Specification-Version` as the fallback.
fn manifest_version(path: &Path) -> Option<String> {
    let bytes = read_entry(path, "META-INF/MANIFEST.MF")?;
    let text = std::str::from_utf8(&bytes).ok()?;
    let mut fields = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix(' ') {
            current.push_str(rest);
        } else {
            if !current.is_empty() {
                fields.push(std::mem::take(&mut current));
            }
            current.push_str(line);
        }
    }
    if !current.is_empty() {
        fields.push(current);
    }
    for key in ["Implementation-Version", "Specification-Version"] {
        let found = fields.iter().find_map(|field| {
            let (name, value) = field.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case(key)
                .then(|| value.trim().to_owned())
        });
        if let Some(value) = found.filter(|value| !value.is_empty() && !value.contains("${")) {
            return Some(value);
        }
    }
    None
}

pub fn parse_pack(path: &Path) -> ContentMeta {
    let Some(bytes) = read_entry(path, PACK) else {
        return ContentMeta::Unknown;
    };
    let Ok(json) = serde_json::from_slice::<Value>(&bytes) else {
        return ContentMeta::Unknown;
    };
    let pack = json.get("pack");
    let description = pack
        .and_then(|pack| pack.get("description"))
        .map(flatten_text_component)
        .unwrap_or_default();
    let pack_format = pack
        .and_then(|pack| pack.get("pack_format"))
        .and_then(Value::as_u64)
        .map(|value| value as u32);
    ContentMeta::ResourcePack {
        description,
        pack_format,
    }
}

pub fn parse_shader(path: &Path) -> ContentMeta {
    // A shader pack is recognised by a `shaders/` folder. Reading the central
    // directory is enough; no entry is decompressed.
    if !archive_has_prefix(path, "shaders/") {
        return ContentMeta::Unknown;
    }
    ContentMeta::Shader {
        description: read_entry(path, "shaders/shader.properties")
            .or_else(|| read_entry(path, "shaders.properties"))
            .map(|bytes| one_line_comment(&bytes))
            .unwrap_or_default(),
    }
}

/// Jar/zip entry lookup that treats a missing entry as "not this format".
fn read_entry(path: &Path, name: &str) -> Option<Vec<u8>> {
    crate::core::loader::archive::read_entry(path, name).ok()
}

fn archive_has_prefix(path: &Path, prefix: &str) -> bool {
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };
    let Ok(archive) = zip::ZipArchive::new(file) else {
        return false;
    };
    archive.file_names().any(|name| name.starts_with(prefix))
}

fn fabric(bytes: &[u8]) -> Option<ContentMeta> {
    let json = serde_json::from_slice::<Value>(bytes).ok()?;
    let id = json.get("id").and_then(Value::as_str)?.to_owned();
    let name = json
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| id.clone());
    Some(ContentMeta::Mod {
        id,
        name,
        version: clean_version(
            json.get("version")
                .and_then(Value::as_str)
                .map(str::to_owned),
        ),
        authors: string_list(json.get("authors")),
        description: json
            .get("description")
            .map(flatten_text_component)
            .unwrap_or_default(),
        depends: json
            .get("depends")
            .and_then(Value::as_object)
            .map(|map| map.keys().cloned().collect())
            .unwrap_or_default(),
    })
}

fn forge(bytes: &[u8]) -> Option<ContentMeta> {
    let text = std::str::from_utf8(bytes).ok()?;
    // Only the first `[[mods]]` block matters for a launcher list. Cutting at the
    // next table header is essential: dependency blocks repeat `modId` (`forge`,
    // `minecraft`), and they win whenever the mod's own keys carry a trailing
    // comment.
    let block = first_mod_block(text)?;
    let id = toml_string(block, "modId").filter(|value| !value.contains("${"))?;
    let name = toml_string(block, "displayName")
        .filter(|value| !value.contains("${"))
        .unwrap_or_else(|| id.clone());
    let authors = toml_string(block, "authors").unwrap_or_default();
    Some(ContentMeta::Mod {
        id,
        name,
        version: clean_version(toml_string(block, "version")),
        authors: authors
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect(),
        description: toml_string(block, "description").unwrap_or_default(),
        depends: Vec::new(),
    })
}

/// Body of the first `[[mods]]` table, stopping before the next table header.
fn first_mod_block(text: &str) -> Option<&str> {
    let header_line = text
        .lines()
        .position(|line| line.trim_start().starts_with("[[mods]]"))?;
    let start = text
        .split_inclusive('\n')
        .take(header_line + 1)
        .map(str::len)
        .sum::<usize>();
    let rest = &text[start..];
    let mut end = rest.len();
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_start().starts_with('[') {
            end = offset;
            break;
        }
        offset += line.len();
    }
    Some(&rest[..end])
}

/// ${file.jarVersion} and friends are build-time placeholders, not versions.
/// Showing them in the list is worse than showing nothing.
fn clean_version(value: Option<String>) -> String {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty() && !value.contains("${"))
        .unwrap_or_default()
}

fn legacy_forge(bytes: &[u8]) -> Option<ContentMeta> {
    let json = serde_json::from_slice::<Value>(bytes).ok()?;
    let first = json.as_array()?.first()?.clone();
    let id = first
        .get("modid")
        .and_then(Value::as_str)
        .or_else(|| first.get("modId").and_then(Value::as_str))?
        .to_owned();
    let name = first
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| id.clone());
    Some(ContentMeta::Mod {
        id,
        name,
        version: clean_version(
            first
                .get("version")
                .and_then(Value::as_str)
                .map(str::to_owned),
        ),
        authors: string_list(first.get("authorList"))
            .into_iter()
            .chain(string_list(first.get("authors")))
            .collect(),
        description: first
            .get("description")
            .map(flatten_text_component)
            .unwrap_or_default(),
        depends: Vec::new(),
    })
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| match item {
                Value::String(text) => Some(text.clone()),
                other => other.get("name").and_then(Value::as_str).map(str::to_owned),
            })
            .collect(),
        Some(Value::String(text)) => vec![text.clone()],
        _ => Vec::new(),
    }
}

/// `description` is a text component: a plain string, `{"text": ...}` or
/// `{"translate": ...}`. Take the readable part, keep the raw JSON otherwise.
pub fn flatten_text_component(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Object(map) => {
            for key in ["text", "translate", "fallback"] {
                if let Some(Value::String(text)) = map.get(key) {
                    return text.clone();
                }
            }
            String::new()
        }
        Value::Array(items) => items
            .iter()
            .map(flatten_text_component)
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

/// Minimal `key = "value"` reader for the TOML descriptors.
///
/// A full TOML parser would be a new dependency for four keys; the descriptors
/// only ever use basic strings here. Both triple-quoted forms are supported,
/// including the multi-line shape Forge templates use:
///
/// ```toml
/// description='''
///    Optimisation mod
///    '''
/// ```
///
/// A bare `'''` on its own line must not fall through to the single-quote branch:
/// that produces `"'"`, which shows up as a lone apostrophe in the workspace note
/// strip (40 of 97 mods in one real instance).
///
/// Inline `# comments` are common in Forge templates (`modId="x" #mandatory`) and
/// must be removed before the value is quoted-checked, otherwise the reader skips
/// to the next `modId`, which is the dependency's `forge` or `minecraft`.
fn toml_string(text: &str, key: &str) -> Option<String> {
    let lines = text.lines().collect::<Vec<_>>();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index].trim();
        index += 1;
        if line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if name.trim() != key {
            continue;
        }
        let value = strip_toml_comment(value).trim();
        for delimiter in ["'''", "\"\"\""] {
            let Some(rest) = value.strip_prefix(delimiter) else {
                continue;
            };
            // One line: `'''text'''`.
            if let Some(inner) = rest.strip_suffix(delimiter) {
                return Some(inner.to_owned());
            }
            // Multi-line: everything up to the line that closes the string.
            let mut body = rest.to_owned();
            while index < lines.len() {
                let line = lines[index];
                index += 1;
                if let Some(end) = line.trim_end().strip_suffix(delimiter) {
                    body.push('\n');
                    body.push_str(end);
                    break;
                }
                body.push('\n');
                body.push_str(line);
            }
            // TOML drops the newline that follows the opening delimiter, and
            // the note strip renders one line anyway.
            let body = body.trim_matches('\n').trim();
            let body = body.split_whitespace().collect::<Vec<_>>().join(" ");
            return Some(body);
        }
        if let Some(inner) = value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
        {
            return Some(inner.replace("\\\"", "\"").replace("\\\\", "\\"));
        }
        if let Some(inner) = value
            .strip_prefix('\'')
            .and_then(|value| value.strip_suffix('\''))
        {
            // An unterminated triple quote is not a single-quoted string.
            if inner.contains("'''") {
                return None;
            }
            return Some(inner.to_owned());
        }
    }
    None
}

/// Removes a trailing `# comment`, ignoring `#` inside quoted strings.
fn strip_toml_comment(value: &str) -> &str {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, character) in value.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match (quote, character) {
            (Some('"'), '\\') => escaped = true,
            (Some(current), character) if character == current => quote = None,
            (None, '"') | (None, '\'') => quote = Some(character),
            (None, '#') => return &value[..index],
            _ => {}
        }
    }
    value
}

/// First commented line of a shader properties file, used as a description.
fn one_line_comment(bytes: &[u8]) -> String {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return String::new();
    };
    text.lines()
        .map(str::trim)
        .find(|line| line.starts_with('#'))
        .map(|line| line.trim_start_matches('#').trim().to_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{parse_mod, parse_pack, parse_shader, toml_string};
    use crate::core::content::ContentMeta;
    use std::io::Write;
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("tmcl-content-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    fn write_zip(path: &PathBuf, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).expect("create archive");
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for (name, body) in entries {
            zip.start_file(*name, options).expect("start entry");
            zip.write_all(body).expect("write entry");
        }
        zip.finish().expect("finish archive");
    }

    #[test]
    fn fabric_metadata_is_parsed() {
        let root = temp_dir("fabric");
        let jar = root.join("example.jar");
        write_zip(
            &jar,
            &[(
                "fabric.mod.json",
                br#"{"id":"example","name":"Example Mod","version":"1.2.3","authors":["a"],"description":"demo","depends":{"minecraft":"*"}}"#,
            )],
        );
        let meta = parse_mod(&jar).expect("fabric metadata");
        match meta {
            ContentMeta::Mod {
                id,
                authors,
                description,
                depends,
                ..
            } => {
                assert_eq!(id, "example");
                assert_eq!(authors, ["a"]);
                assert_eq!(description, "demo");
                assert_eq!(depends, ["minecraft"]);
            }
            other => panic!("unexpected metadata: {other:?}"),
        }
        std::fs::remove_dir_all(root).expect("clean temp dir");
    }

    #[test]
    fn forge_metadata_is_parsed() {
        let root = temp_dir("forge");
        let jar = root.join("create.jar");
        let toml = b"modLoader=\"javafml\"\nloaderVersion=\"[47,)\"\n[[mods]]\nmodId=\"create\"\nversion=\"0.5.1\"\ndisplayName=\"Create\"\ndescription=\"Contraptions\"\nauthors=\"simibubi\"\n";
        write_zip(&jar, &[("META-INF/mods.toml", toml)]);
        match parse_mod(&jar).expect("forge metadata") {
            ContentMeta::Mod {
                id,
                authors,
                description,
                ..
            } => {
                assert_eq!(id, "create");
                assert_eq!(authors, ["simibubi"]);
                assert_eq!(description, "Contraptions");
            }
            other => panic!("unexpected metadata: {other:?}"),
        }
        std::fs::remove_dir_all(root).expect("clean temp dir");
    }

    #[test]
    fn neoforge_metadata_is_parsed() {
        let root = temp_dir("neoforge");
        let jar = root.join("neo.jar");
        write_zip(
            &jar,
            &[("META-INF/neoforge.mods.toml", b"[[mods]]\nmodId=\"neo\"\n")],
        );
        assert!(matches!(
            parse_mod(&jar),
            Some(ContentMeta::Mod { id, .. }) if id == "neo"
        ));
        std::fs::remove_dir_all(root).expect("clean temp dir");
    }

    #[test]
    fn legacy_mcmod_info_is_parsed() {
        let root = temp_dir("legacy");
        let jar = root.join("old.jar");
        write_zip(
            &jar,
            &[(
                "mcmod.info",
                br#"[{"modid":"old","name":"Old","version":"1.0","description":"d","authorList":["x"]}]"#,
            )],
        );
        assert!(matches!(
            parse_mod(&jar),
            Some(ContentMeta::Mod { id, .. }) if id == "old"
        ));
        std::fs::remove_dir_all(root).expect("clean temp dir");
    }

    #[test]
    fn unreadable_jar_returns_none() {
        let root = temp_dir("broken");
        let jar = root.join("broken.jar");
        std::fs::write(&jar, b"not a zip").expect("write broken file");
        assert!(parse_mod(&jar).is_none());
        std::fs::remove_dir_all(root).expect("clean temp dir");
    }

    #[test]
    fn pack_mcmeta_is_parsed() {
        let root = temp_dir("pack");
        let zip_path = root.join("demo.zip");
        write_zip(
            &zip_path,
            &[(
                "pack.mcmeta",
                br#"{"pack":{"pack_format":15,"description":{"text":"Demo"}}}"#,
            )],
        );
        match parse_pack(&zip_path) {
            ContentMeta::ResourcePack {
                description,
                pack_format,
            } => {
                assert_eq!(description, "Demo");
                assert_eq!(pack_format, Some(15));
            }
            other => panic!("unexpected metadata: {other:?}"),
        }
        std::fs::remove_dir_all(root).expect("clean temp dir");
    }

    #[test]
    fn shader_zip_detects_shaders_dir() {
        let root = temp_dir("shader");
        let zip_path = root.join("shader.zip");
        write_zip(
            &zip_path,
            &[
                ("shaders/final.fsh", b"void main() {}"),
                (
                    "shaders/shader.properties",
                    b"# Soft shadows\nsunPathRotation=0\n",
                ),
            ],
        );
        assert!(matches!(
            parse_shader(&zip_path),
            ContentMeta::Shader { description } if description == "Soft shadows"
        ));
        std::fs::remove_dir_all(root).expect("clean temp dir");
    }

    #[test]
    fn forge_metadata_ignores_inline_comments_and_dependencies() {
        let root = temp_dir("forge-comments");
        let jar = root.join("accelerated-rendering.jar");
        // Real Forge templates annotate every key and repeat `modId` in the
        // dependency tables. Neither may reach the parsed identity.
        let toml = br#"modLoader="javafml" #mandatory
loaderVersion="[47,)" #mandatory
[[mods]] #mandatory
modId="acceleratedrendering" #mandatory
version="1.0.14-1.20.1-alpha" #mandatory
displayName="Accelerated Rendering" #mandatory
authors="Argon4W" #optional
description='''Fast vertex transform and caching.'''
[[dependencies.acceleratedrendering]]
    modId="forge" #mandatory
    mandatory=true
[[dependencies.acceleratedrendering]]
    modId="minecraft"
    mandatory=true
"#;
        write_zip(&jar, &[("META-INF/mods.toml", toml)]);
        match parse_mod(&jar).expect("forge metadata") {
            ContentMeta::Mod {
                id,
                name,
                version,
                authors,
                description,
                ..
            } => {
                assert_eq!(id, "acceleratedrendering");
                assert_eq!(name, "Accelerated Rendering");
                assert_eq!(version, "1.0.14-1.20.1-alpha");
                assert_eq!(authors, ["Argon4W"]);
                assert_eq!(description, "Fast vertex transform and caching.");
            }
            other => panic!("unexpected metadata: {other:?}"),
        }
        std::fs::remove_dir_all(root).expect("clean temp dir");
    }

    #[test]
    fn placeholder_versions_are_hidden_without_a_manifest() {
        let root = temp_dir("forge-placeholder");
        let jar = root.join("placeholder.jar");
        let toml = b"[[mods]]\nmodId=\"example\" #mandatory\nversion=\"${file.jarVersion}\" #mandatory\ndisplayName=\"Example\" #mandatory\n";
        write_zip(&jar, &[("META-INF/mods.toml", toml)]);
        match parse_mod(&jar).expect("forge metadata") {
            ContentMeta::Mod {
                id, name, version, ..
            } => {
                assert_eq!(id, "example");
                assert_eq!(name, "Example");
                assert!(version.is_empty(), "{version:?}");
            }
            other => panic!("unexpected metadata: {other:?}"),
        }
        std::fs::remove_dir_all(root).expect("clean temp dir");
    }

    #[test]
    fn placeholder_versions_fall_back_to_the_manifest() {
        let root = temp_dir("forge-manifest");
        let jar = root.join("mouse-tweaks.jar");
        let toml = b"[[mods]]\nmodId=\"mouse-tweaks\" #mandatory\nversion=\"${file.jarVersion}\" #mandatory\ndisplayName=\"Mouse Tweaks\" #mandatory\n";
        let manifest = b"Manifest-Version: 1.0\r\nImplementation-Title: MouseTweaks\r\nImplementation-Version: 2.25.1\r\n";
        write_zip(
            &jar,
            &[
                ("META-INF/mods.toml", toml),
                ("META-INF/MANIFEST.MF", manifest),
            ],
        );
        match parse_mod(&jar).expect("forge metadata") {
            ContentMeta::Mod { version, .. } => assert_eq!(version, "2.25.1"),
            other => panic!("unexpected metadata: {other:?}"),
        }
        std::fs::remove_dir_all(root).expect("clean temp dir");
    }

    #[test]
    fn toml_reader_handles_quotes_and_comments() {
        let text =
            "[[mods]]\nmodId = \"my#mod\" # the # is inside the string\nversion = '1.0' # plain\n";
        assert_eq!(toml_string(text, "modId").as_deref(), Some("my#mod"));
        assert_eq!(toml_string(text, "version").as_deref(), Some("1.0"));
        assert_eq!(toml_string(text, "missing"), None);
    }

    #[test]
    fn multiline_triple_quoted_descriptions_are_read_in_full() {
        // The shape Forge templates use: the delimiter alone on its own line.
        let text = "[[mods]]\nmodId=\"badoptimizations\"\ndescription='''\n   Optimization mod that focuses on things other than rendering\n   '''\n";
        assert_eq!(
            toml_string(text, "description").as_deref(),
            Some("Optimization mod that focuses on things other than rendering")
        );

        // The double-quoted variant, with several lines.
        let text = "[[mods]]\ndescription=\"\"\"\n   First line\n   Second line\n   \"\"\"\n";
        assert_eq!(
            toml_string(text, "description").as_deref(),
            Some("First line Second line")
        );

        // One-line triple quotes keep working.
        let text = "[[mods]]\ndescription='''Fast vertex caching.'''\n";
        assert_eq!(
            toml_string(text, "description").as_deref(),
            Some("Fast vertex caching.")
        );
    }

    #[test]
    fn an_unterminated_triple_quote_is_not_a_lone_apostrophe() {
        // A single-quoted parse yields "'", which the workspace draws as a stray
        // apostrophe.
        let text = "[[mods]]\ndescription='''\n";
        assert_ne!(toml_string(text, "description").as_deref(), Some("'"));
    }
}
