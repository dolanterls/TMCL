//! Loader detection from launch metadata.
//!
//! Version names and folder names lie: modpacks rename folders, PCL2 rewrites
//! `id`, and imported instances carry the loader name in three different
//! places. `mainClass` plus the library coordinates are what the game actually
//! runs, so they are the only inputs here.

use crate::core::loader::LoaderKind;
use crate::core::loader::client_json::ClientJson;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoaderInfo {
    pub kind: LoaderKind,
    /// Loader version, e.g. `47.4.22` for Forge or `0.15.11` for Fabric.
    pub version: Option<String>,
    pub required_java_major: Option<u32>,
}

pub fn detect(client: &ClientJson) -> LoaderInfo {
    let main_class = client.main_class.to_ascii_lowercase();
    let libraries = client
        .libraries
        .iter()
        .map(|library| library.name.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let has_library = |prefix: &str| libraries.iter().any(|name| name.starts_with(prefix));

    let kind = if main_class.contains("bootstraplauncher") {
        if has_library("net.neoforged") {
            LoaderKind::NeoForge
        } else {
            LoaderKind::Forge
        }
    } else if main_class.contains("launchwrapper") {
        if has_library("net.minecraftforge") {
            LoaderKind::Forge
        } else {
            LoaderKind::Vanilla
        }
    } else if main_class.contains("quiltmc") || has_library("org.quiltmc") {
        // Quilt's main class also contains "KnotClient", so it is checked
        // before Fabric or every Quilt instance would read as Fabric.
        LoaderKind::Quilt
    } else if main_class.contains("knotclient") || has_library("net.fabricmc") {
        LoaderKind::Fabric
    } else {
        LoaderKind::Vanilla
    };

    LoaderInfo {
        kind,
        version: loader_version(&libraries, kind),
        required_java_major: client
            .java_version
            .as_ref()
            .map(|version| version.major_version)
            .or_else(|| default_java_major(kind, &client.id)),
    }
}

fn loader_version(libraries: &[String], kind: LoaderKind) -> Option<String> {
    let prefixes: &[&str] = match kind {
        LoaderKind::Forge => &["net.minecraftforge:forge:", "net.minecraftforge:fmlloader:"],
        LoaderKind::NeoForge => &[
            "net.neoforged:neoforge:",
            "net.neoforged.fancymodloader:loader:",
        ],
        LoaderKind::Fabric => &["net.fabricmc:fabric-loader:"],
        LoaderKind::Quilt => &["org.quiltmc:quilt-loader:"],
        LoaderKind::Vanilla => &[],
    };
    for prefix in prefixes {
        if let Some(name) = libraries.iter().find(|name| name.starts_with(prefix)) {
            return name[prefix.len()..].split(':').next().map(str::to_owned);
        }
    }
    None
}

/// Java major version a loader needs when the version JSON does not say.
///
/// The JSON written by the official launcher always carries `javaVersion`; this
/// table only covers instances from other launchers that dropped the field.
fn default_java_major(kind: LoaderKind, minecraft_version: &str) -> Option<u32> {
    let (major, minor, patch) = parse_version(minecraft_version)?;
    let legacy_forge = matches!(kind, LoaderKind::Forge) && (major, minor) < (1, 17);
    let java_21 = (major, minor) > (1, 20) || ((major, minor) == (1, 20) && patch >= 5);
    Some(if legacy_forge || (major, minor) <= (1, 16) {
        8
    } else if java_21 {
        // 1.20.5 was the first release that required Java 21.
        21
    } else {
        17
    })
}

fn parse_version(value: &str) -> Option<(u32, u32, u32)> {
    let mut parts = value.split(['.', '-', '_']);
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::detect;
    use crate::core::loader::LoaderKind;
    use crate::core::loader::client_json::parse;

    fn info(json: &str) -> super::LoaderInfo {
        detect(&parse(json.as_bytes()).expect("client JSON"))
    }

    #[test]
    fn detects_forge_neoforge_fabric_quilt_and_vanilla_from_main_class() {
        let forge = info(
            r#"{"id":"1.20.1-forge-47.4.22","mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher","javaVersion":{"majorVersion":17},"libraries":[{"name":"net.minecraftforge:forge:1.20.1-47.4.22"}]}"#,
        );
        assert_eq!(forge.kind, LoaderKind::Forge);
        assert_eq!(forge.version.as_deref(), Some("1.20.1-47.4.22"));
        assert_eq!(forge.required_java_major, Some(17));

        let neoforge = info(
            r#"{"id":"1.21.1-neoforge-21.1.247","mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher","libraries":[{"name":"net.neoforged:neoforge:21.1.247"}]}"#,
        );
        assert_eq!(neoforge.kind, LoaderKind::NeoForge);
        assert_eq!(neoforge.required_java_major, Some(21));

        let fabric = info(
            r#"{"id":"1.21.1","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient","libraries":[{"name":"net.fabricmc:fabric-loader:0.15.11"}]}"#,
        );
        assert_eq!(fabric.kind, LoaderKind::Fabric);
        assert_eq!(fabric.version.as_deref(), Some("0.15.11"));
        assert_eq!(fabric.required_java_major, Some(21));

        let quilt = info(
            r#"{"id":"1.20.1","mainClass":"org.quiltmc.loader.impl.launch.knot.QuiltKnotClient","libraries":[{"name":"org.quiltmc:quilt-loader:0.24.0"}]}"#,
        );
        assert_eq!(quilt.kind, LoaderKind::Quilt);
        assert_eq!(quilt.required_java_major, Some(17));

        let legacy_forge = info(
            r#"{"id":"1.12.2-forge1.12.2-14.23.5.2860","mainClass":"net.minecraft.launchwrapper.Launch","libraries":[{"name":"net.minecraftforge:forge:1.12.2-14.23.5.2860"}]}"#,
        );
        assert_eq!(legacy_forge.kind, LoaderKind::Forge);
        assert_eq!(legacy_forge.required_java_major, Some(8));

        let vanilla = info(r#"{"id":"1.12.2","mainClass":"net.minecraft.client.main.Main"}"#);
        assert_eq!(vanilla.kind, LoaderKind::Vanilla);
        assert_eq!(vanilla.required_java_major, Some(8));
    }

    #[test]
    fn java_version_from_the_json_wins_over_the_builtin_table() {
        let info = info(
            r#"{"id":"1.20.1","mainClass":"net.minecraft.client.main.Main","javaVersion":{"majorVersion":21}}"#,
        );
        assert_eq!(info.required_java_major, Some(21));
    }

    #[test]
    fn java_21_starts_at_1_20_5_when_the_json_has_no_java_version() {
        let before = info(r#"{"id":"1.20.4","mainClass":"net.minecraft.client.main.Main"}"#);
        assert_eq!(before.required_java_major, Some(17));

        let after = info(r#"{"id":"1.20.5","mainClass":"net.minecraft.client.main.Main"}"#);
        assert_eq!(after.required_java_major, Some(21));

        let next = info(r#"{"id":"1.21","mainClass":"net.minecraft.client.main.Main"}"#);
        assert_eq!(next.required_java_major, Some(21));
    }
}
