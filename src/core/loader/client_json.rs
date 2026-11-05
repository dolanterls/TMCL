use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::core::error::{Error, Result};

#[derive(Debug, Clone, Deserialize)]
pub struct ClientJson {
    pub id: String,
    #[serde(rename = "inheritsFrom", default)]
    pub inherits_from: Option<String>,
    #[serde(rename = "type", default = "default_version_type")]
    pub version_type: String,
    #[serde(rename = "mainClass")]
    pub main_class: String,
    #[serde(rename = "minecraftArguments", default)]
    pub minecraft_arguments: Option<String>,
    #[serde(default)]
    pub arguments: Option<ModernArguments>,
    #[serde(default)]
    pub libraries: Vec<ClientLibrary>,
    #[serde(rename = "assetIndex", default)]
    pub asset_index: Option<AssetIndex>,
    #[serde(default)]
    pub logging: Option<ClientLogging>,
    #[serde(default)]
    pub downloads: ClientDownloads,
    #[serde(rename = "javaVersion", default)]
    pub java_version: Option<JavaVersion>,
    /// Ids of the inherited versions, nearest parent first. Filled in by
    /// [`ClientJson::resolve_inheritance`]; never part of the JSON on disk.
    #[serde(skip)]
    pub inherited_ids: Vec<String>,
}

fn default_version_type() -> String {
    "release".into()
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ClientDownloads {
    #[serde(default)]
    pub client: Option<ClientArtifact>,
    #[serde(default)]
    pub client_mappings: Option<ClientArtifact>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClientArtifact {
    pub url: String,
    pub sha1: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClientLogging {
    #[serde(default)]
    pub client: Option<LoggingClient>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingClient {
    pub argument: String,
    pub file: ClientArtifact,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndex {
    pub id: String,
    pub url: String,
    #[serde(default)]
    pub sha1: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JavaVersion {
    #[serde(rename = "majorVersion")]
    pub major_version: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClientLibrary {
    pub name: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub rules: Vec<ArgumentRule>,
    #[serde(default)]
    pub natives: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub extract: Option<LibraryExtract>,
    #[serde(default)]
    pub downloads: ClientLibraryDownloads,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ClientLibraryDownloads {
    #[serde(default)]
    pub artifact: Option<ClientLibraryArtifact>,
    #[serde(default)]
    pub classifiers: BTreeMap<String, ClientLibraryArtifact>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LibraryExtract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClientLibraryArtifact {
    pub path: String,
    pub url: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModernArguments {
    #[serde(default)]
    pub game: Vec<RawArgument>,
    #[serde(default)]
    pub jvm: Vec<RawArgument>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum RawArgument {
    String(String),
    Conditional {
        rules: Vec<ArgumentRule>,
        value: ArgumentValue,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ArgumentValue {
    String(String),
    Strings(Vec<String>),
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArgumentRule {
    #[serde(default = "default_rule_action")]
    pub action: String,
    #[serde(default)]
    pub os: Option<ArgumentOsRule>,
    #[serde(default)]
    pub features: BTreeMap<String, bool>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct ArgumentOsRule {
    pub name: Option<String>,
    pub arch: Option<String>,
}

fn default_rule_action() -> String {
    "allow".into()
}

impl ClientJson {
    /// Merge this version patch over its inherited parent metadata.
    /// Scalar child values win; list-like arguments and libraries preserve
    /// parent entries and then apply the child entries in declaration order.
    pub fn inherit(mut self, parent: &Self) -> Self {
        if self.minecraft_arguments.is_none() {
            self.minecraft_arguments = parent.minecraft_arguments.clone();
        }
        self.arguments = merge_arguments(parent.arguments.as_ref(), self.arguments.take());
        self.libraries = merge_libraries(&parent.libraries, self.libraries);
        if self.asset_index.is_none() {
            self.asset_index = parent.asset_index.clone();
        }
        if self.logging.is_none() {
            self.logging = parent.logging.clone();
        }
        if self.downloads.client.is_none() {
            self.downloads.client = parent.downloads.client.clone();
        }
        if self.downloads.client_mappings.is_none() {
            self.downloads.client_mappings = parent.downloads.client_mappings.clone();
        }
        if self.java_version.is_none() {
            self.java_version = parent.java_version.clone();
        }
        self
    }

    /// Resolve `inheritsFrom` links from `versions/<id>/<id>.json` safely.
    pub fn resolve_inheritance(self, game_directory: &Path) -> Result<Self> {
        let mut chain = vec![self];
        let mut visited = HashSet::new();
        loop {
            let current = chain.last().expect("inheritance chain has current entry");
            let Some(parent_id) = current.inherits_from.as_deref() else {
                break;
            };
            if !safe_version_id(parent_id) {
                return Err(Error::Version(format!(
                    "unsafe inherited version id: {parent_id:?}"
                )));
            }
            if !visited.insert(parent_id.to_owned()) {
                return Err(Error::Version(format!(
                    "cyclic client version inheritance at {parent_id}"
                )));
            }
            let path = version_json_path(game_directory, parent_id);
            let parent = parse(&std::fs::read(&path).map_err(|error| {
                Error::Version(format!(
                    "read inherited client JSON {}: {error}",
                    path.display()
                ))
            })?)?;
            chain.push(parent);
        }

        let ids = chain
            .iter()
            .map(|version| version.id.clone())
            .collect::<Vec<_>>();
        let mut resolved = chain.pop().expect("inheritance chain is not empty");
        while let Some(patch) = chain.pop() {
            resolved = patch.inherit(&resolved);
        }
        // `ids[0]` is this version itself; the rest are its parents, nearest first.
        resolved.inherited_ids = ids.into_iter().skip(1).collect();
        // Maven-only entries (`"name": "net.fabricmc:fabric-loader:0.15.11"`)
        // carry a repository URL instead of a `downloads` block. Their path is
        // derivable from the coordinate, and every consumer downstream - the
        // classpath, the missing-file check, the repairer - reads
        // `downloads.artifact`. Normalising here covers all of them, including
        // instances installed by another launcher.
        resolved.normalize_maven_libraries();
        Ok(resolved)
    }

    pub fn classpath_libraries(
        &self,
        features: &BTreeMap<String, bool>,
    ) -> Result<Vec<std::path::PathBuf>> {
        let mut libraries = Vec::new();
        for library in &self.libraries {
            if !rules_allow(&library.rules, features)? {
                continue;
            }
            if let Some(artifact) = &library.downloads.artifact {
                libraries.push(std::path::PathBuf::from("libraries").join(&artifact.path));
            } else if let Some(path) = crate::core::loader::maven_path(&library.name) {
                // Belt and braces: a library without a `downloads` block still
                // names its own jar. Dropping it silently removed whole mod
                // loaders from the classpath, and the JVM then reported a
                // missing main class instead of a missing file.
                libraries.push(std::path::PathBuf::from("libraries").join(path));
            }
        }
        Ok(libraries)
    }

    pub fn active_libraries(
        &self,
        features: &BTreeMap<String, bool>,
    ) -> Result<Vec<&ClientLibrary>> {
        let mut active = Vec::new();
        for library in &self.libraries {
            if rules_allow(&library.rules, features)? {
                active.push(library);
            }
        }
        Ok(active)
    }

    pub fn normalize_maven_libraries(&mut self) {
        for library in &mut self.libraries {
            if library.downloads.artifact.is_some() {
                continue;
            }
            let Some(base_url) = library.url.as_deref() else {
                continue;
            };
            let Some(path) = crate::core::loader::maven_path(&library.name) else {
                continue;
            };
            let relative_path = path.to_string_lossy().replace('\\', "/");
            library.downloads.artifact = Some(ClientLibraryArtifact {
                path: relative_path.clone(),
                url: format!("{}/{relative_path}", base_url.trim_end_matches('/')),
                sha1: library.sha1.clone(),
                size: library.size,
            });
        }
    }

    pub fn native_artifacts(&self) -> Vec<(&ClientLibrary, &ClientLibraryArtifact)> {
        let features = launch_features();
        let mut result = Vec::new();
        for library in &self.libraries {
            if !rules_allow(&library.rules, &features).unwrap_or(false) {
                continue;
            }
            let Some(template) = library
                .natives
                .as_ref()
                .and_then(|natives| natives.get(std::env::consts::OS))
            else {
                continue;
            };
            let arch = if std::env::consts::ARCH.contains("64") {
                "64"
            } else {
                "32"
            };
            let classifier = template.replace("${arch}", arch);
            if let Some(artifact) = library.downloads.classifiers.get(&classifier) {
                result.push((library, artifact));
            }
        }
        result
    }

    pub fn game_arguments(&self, features: &BTreeMap<String, bool>) -> Result<Vec<String>> {
        if let Some(arguments) = &self.arguments {
            return resolve_arguments(&arguments.game, features);
        }
        Ok(self
            .minecraft_arguments
            .as_deref()
            .unwrap_or_default()
            .split_whitespace()
            .map(str::to_owned)
            .collect())
    }

    pub fn jvm_arguments(&self, features: &BTreeMap<String, bool>) -> Result<Vec<String>> {
        self.arguments
            .as_ref()
            .map(|arguments| resolve_arguments(&arguments.jvm, features))
            .unwrap_or_else(|| Ok(Vec::new()))
    }
}

pub fn safe_version_id(id: &str) -> bool {
    !id.trim().is_empty()
        && id != "."
        && id != ".."
        && !id
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':' | '\0'))
}

pub fn version_json_path(game_directory: &Path, id: &str) -> PathBuf {
    game_directory
        .join("versions")
        .join(id)
        .join(format!("{id}.json"))
}

fn launch_features() -> BTreeMap<String, bool> {
    BTreeMap::from([
        ("has_custom_resolution".into(), true),
        ("has_quick_plays_support".into(), true),
        ("is_demo_user".into(), false),
        ("is_quick_play_multiplayer".into(), false),
        ("is_quick_play_realms".into(), false),
        ("is_quick_play_singleplayer".into(), false),
    ])
}

fn merge_arguments(
    parent: Option<&ModernArguments>,
    child: Option<ModernArguments>,
) -> Option<ModernArguments> {
    match (parent, child) {
        (None, None) => None,
        (Some(parent), None) => Some(parent.clone()),
        (None, Some(child)) => Some(child),
        (Some(parent), Some(mut child)) => {
            let mut game = parent.game.clone();
            game.append(&mut child.game);
            let mut jvm = parent.jvm.clone();
            jvm.append(&mut child.jvm);
            Some(ModernArguments { game, jvm })
        }
    }
}

fn merge_libraries(parent: &[ClientLibrary], child: Vec<ClientLibrary>) -> Vec<ClientLibrary> {
    let mut merged = parent.to_vec();
    for library in child {
        if let Some(existing) = merged.iter_mut().find(|item| item.name == library.name) {
            *existing = library;
        } else {
            merged.push(library);
        }
    }
    merged
}

fn resolve_arguments(
    arguments: &[RawArgument],
    features: &BTreeMap<String, bool>,
) -> Result<Vec<String>> {
    let mut result = Vec::new();
    for argument in arguments {
        match argument {
            RawArgument::String(value) => result.push(value.clone()),
            RawArgument::Conditional { rules, value } => {
                if rules_allow(rules, features)? {
                    match value {
                        ArgumentValue::String(value) => result.push(value.clone()),
                        ArgumentValue::Strings(values) => result.extend(values.clone()),
                    }
                }
            }
        }
    }
    Ok(result)
}

pub(crate) fn rules_allow(
    rules: &[ArgumentRule],
    features: &BTreeMap<String, bool>,
) -> Result<bool> {
    let mut allowed = rules.is_empty();
    for rule in rules {
        if !argument_rule_matches(rule, features) {
            continue;
        }
        allowed = match rule.action.to_ascii_lowercase().as_str() {
            "allow" => true,
            "disallow" => false,
            action => {
                return Err(Error::Version(format!(
                    "unknown client argument rule action: {action}"
                )));
            }
        };
    }
    Ok(allowed)
}

fn argument_rule_matches(rule: &ArgumentRule, features: &BTreeMap<String, bool>) -> bool {
    if let Some(os) = &rule.os {
        if let Some(name) = &os.name {
            let expected = if name.eq_ignore_ascii_case("osx") {
                "macos"
            } else {
                name.as_str()
            };
            if !expected.eq_ignore_ascii_case(std::env::consts::OS) {
                return false;
            }
        }
        if let Some(arch) = &os.arch
            && !arch.eq_ignore_ascii_case(std::env::consts::ARCH)
        {
            return false;
        }
    }
    rule.features
        .iter()
        .all(|(name, expected)| features.get(name) == Some(expected))
}

pub fn parse(bytes: &[u8]) -> Result<ClientJson> {
    serde_json::from_slice(bytes)
        .map_err(|error| Error::Version(format!("parse client JSON: {error}")))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{parse, safe_version_id};

    #[test]
    fn supports_modern_arguments_and_legacy_minecraft_arguments() {
        let modern = parse(
            br#"{"id":"1.21.1","mainClass":"net.minecraft.Main","arguments":{"game":["--demo",{"rules":[{"features":{"is_demo_user":true}}],"value":"--demo-user"}]}}"#,
        )
        .expect("modern client JSON");
        let mut features = BTreeMap::new();
        features.insert("is_demo_user".into(), true);
        assert_eq!(
            modern.game_arguments(&features).expect("modern arguments"),
            vec!["--demo", "--demo-user"]
        );

        let legacy = parse(
            br#"{"id":"1.12.2","mainClass":"net.minecraft.Main","minecraftArguments":"--username player --gameDir game"}"#,
        )
        .expect("legacy client JSON");
        assert_eq!(
            legacy
                .game_arguments(&BTreeMap::new())
                .expect("legacy arguments")
                .len(),
            4
        );
    }

    #[test]
    fn maven_only_libraries_reach_the_classpath() {
        // Another launcher writes loader libraries as a coordinate plus a
        // repository URL, with no `downloads` block. Dropping those removed
        // the mod loader itself from the classpath, and the JVM then failed
        // with a missing main class instead of a missing file.
        let client = parse(
            br#"{"id":"pack","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient",
                 "libraries":[
                   {"name":"net.fabricmc:fabric-loader:0.15.11","url":"https://maven.fabricmc.net/"},
                   {"name":"com.google.guava:guava:31.1-jre",
                    "downloads":{"artifact":{"path":"com/google/guava/guava/31.1-jre/guava-31.1-jre.jar",
                                     "url":"https://repo1.maven.org/maven2/com/google/guava/guava/31.1-jre/guava-31.1-jre.jar"}}}
                 ]}"#,
        )
        .expect("client JSON");
        let libraries = client
            .classpath_libraries(&BTreeMap::new())
            .expect("classpath");

        assert!(
            libraries.iter().any(|path| path
                .ends_with("net/fabricmc/fabric-loader/0.15.11/fabric-loader-0.15.11.jar")),
            "loader jar missing from {libraries:?}"
        );
        assert!(
            libraries
                .iter()
                .any(|path| path.ends_with("guava-31.1-jre.jar"))
        );
    }

    #[test]
    fn maven_only_libraries_are_normalised_when_inheritance_is_resolved() {
        let root = std::env::temp_dir().join(format!("tmcl-maven-{}", uuid::Uuid::new_v4()));
        let parent = root.join("versions").join("1.20.1");
        std::fs::create_dir_all(&parent).expect("parent dir");
        std::fs::write(
            parent.join("1.20.1.json"),
            br#"{"id":"1.20.1","mainClass":"net.minecraft.Main",
                 "libraries":[{"name":"net.fabricmc:fabric-loader:0.15.11","url":"https://maven.fabricmc.net/"}]}"#,
        )
        .expect("parent json");
        std::fs::create_dir_all(root.join("versions").join("pack")).expect("pack dir");
        std::fs::write(
            root.join("versions").join("pack").join("pack.json"),
            br#"{"id":"pack","inheritsFrom":"1.20.1","mainClass":"net.minecraft.Main"}"#,
        )
        .expect("pack json");
        let bytes = std::fs::read(root.join("versions").join("pack").join("pack.json"))
            .expect("pack bytes");
        let resolved = parse(&bytes)
            .expect("parse")
            .resolve_inheritance(&root)
            .expect("resolve");

        let library = resolved
            .libraries
            .iter()
            .find(|library| library.name.starts_with("net.fabricmc:fabric-loader"))
            .expect("loader library kept");
        let artifact = library
            .downloads
            .artifact
            .as_ref()
            .expect("maven coordinate became a downloadable artifact");
        assert_eq!(
            artifact.path,
            "net/fabricmc/fabric-loader/0.15.11/fabric-loader-0.15.11.jar"
        );
        assert_eq!(
            artifact.url,
            "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.15.11/fabric-loader-0.15.11.jar"
        );

        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn parses_camel_case_java_version_metadata() {
        let client = parse(
            br#"{"id":"1.21.1","mainClass":"net.minecraft.Main","javaVersion":{"component":"java-runtime-gamma","majorVersion":21}}"#,
        )
        .expect("client JSON with Java version metadata");

        assert_eq!(client.java_version.expect("Java version").major_version, 21);
    }

    #[test]
    fn preserves_client_version_type_for_launch_arguments() {
        let release =
            parse(br#"{"id":"1.21.1","type":"release","mainClass":"net.minecraft.Main"}"#)
                .expect("release client JSON");
        let snapshot =
            parse(br#"{"id":"26w10a","type":"snapshot","mainClass":"net.minecraft.Main"}"#)
                .expect("snapshot client JSON");

        assert_eq!(release.version_type, "release");
        assert_eq!(snapshot.version_type, "snapshot");
    }

    #[test]
    fn merges_inherited_client_metadata_without_losing_loader_overrides() {
        let parent = parse(br#"{"id":"1.21.1","mainClass":"net.minecraft.Main","arguments":{"game":["--version","${version_name}"],"jvm":["-Dparent=true"]},"libraries":[{"name":"org.example:base:1.0","downloads":{"artifact":{"path":"org/example/base/1.0/base-1.0.jar","url":"https://example.test/base.jar"}}}],"assetIndex":{"id":"test","url":"https://example.test/assets.json"},"javaVersion":{"majorVersion":21}}"#).expect("parent");
        let patch = parse(br#"{"id":"1.21.1-fabric","inheritsFrom":"1.21.1","type":"release","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient","arguments":{"game":["--loader","fabric"],"jvm":["-Dloader=true"]},"libraries":[{"name":"net.fabricmc:fabric-loader:0.16.5","downloads":{"artifact":{"path":"net/fabricmc/fabric-loader/0.16.5/fabric-loader-0.16.5.jar","url":"https://example.test/fabric.jar"}}}]}"#).expect("loader patch");

        let merged = patch.inherit(&parent);

        assert_eq!(merged.inherits_from.as_deref(), Some("1.21.1"));
        assert_eq!(
            merged.main_class,
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        );
        assert_eq!(
            merged
                .java_version
                .as_ref()
                .expect("inherited Java")
                .major_version,
            21
        );
        assert_eq!(
            merged.asset_index.as_ref().expect("inherited assets").id,
            "test"
        );
        assert_eq!(merged.libraries.len(), 2);
        let arguments = merged
            .game_arguments(&BTreeMap::new())
            .expect("merged args");
        assert_eq!(
            arguments,
            ["--version", "${version_name}", "--loader", "fabric"]
        );
        assert_eq!(
            merged
                .jvm_arguments(&BTreeMap::new())
                .expect("merged JVM args"),
            ["-Dparent=true", "-Dloader=true"]
        );
    }

    #[test]
    fn rejects_inheritance_cycles_and_unsafe_version_ids() {
        let patch =
            parse(br#"{"id":"a","inheritsFrom":"../b","mainClass":"main"}"#).expect("patch");
        assert!(
            patch
                .inherits_from
                .as_deref()
                .is_some_and(|id| !safe_version_id(id))
        );
        assert!(!safe_version_id("../b"));
        assert!(safe_version_id("1.21.1-fabric"));
    }

    #[test]
    fn resolves_parent_client_json_from_game_version_directory() {
        let root = std::env::temp_dir().join(format!("tmcl-inherits-{}", uuid::Uuid::new_v4()));
        let parent_directory = root.join("versions/1.21.1");
        std::fs::create_dir_all(&parent_directory).expect("create parent version");
        std::fs::write(
            parent_directory.join("1.21.1.json"),
            br#"{"id":"1.21.1","mainClass":"net.minecraft.Main","javaVersion":{"majorVersion":21}}"#,
        )
        .expect("write parent version");
        let patch =
            parse(br#"{"id":"fabric","inheritsFrom":"1.21.1","mainClass":"net.fabricmc.Main"}"#)
                .expect("loader patch");

        let resolved = patch.resolve_inheritance(&root).expect("resolve parent");

        assert_eq!(resolved.main_class, "net.fabricmc.Main");
        assert_eq!(
            resolved.java_version.expect("inherited Java").major_version,
            21
        );
        std::fs::remove_dir_all(root).expect("clean test root");
    }

    #[test]
    fn filters_conditional_jvm_arguments_by_operating_system() {
        let client = parse(
            br#"{"id":"1.21.1","mainClass":"net.minecraft.Main","arguments":{"jvm":[{"rules":[{"action":"allow","os":{"name":"osx"}}],"value":"-XstartOnFirstThread"},{"rules":[{"action":"allow","os":{"name":"windows"}}],"value":"-Dtmcl.platform=windows"}]}}"#,
        )
        .expect("client JSON with operating-system rules");
        let arguments = client
            .jvm_arguments(&BTreeMap::new())
            .expect("resolve JVM arguments");

        assert_eq!(
            arguments
                .iter()
                .any(|argument| argument == "-XstartOnFirstThread"),
            cfg!(target_os = "macos")
        );
        assert_eq!(
            arguments
                .iter()
                .any(|argument| argument == "-Dtmcl.platform=windows"),
            cfg!(windows)
        );
    }
}
