pub mod archive;
pub mod client_json;
pub mod detect;
pub mod fabric;
pub mod forge;
pub mod neoforge;
pub mod service;
pub mod vanilla;

use crate::core::error::{Error, Result};
use crate::core::instance::models::InstanceLoader;
use crate::core::loader::service::InstallLoader;

/// Reads a loader the way a modpack manifest writes it.
///
/// Two spellings exist in the wild: `fabric` with the build in a separate
/// field, and `fabric:0.16.5` with both in one string. An explicit version
/// wins over the embedded one, and a modded loader without any version is an
/// error rather than a guess: installing "some" Fabric build breaks the pack
/// the moment it is launched.
pub fn parse_install_loader(
    loader: &str,
    loader_version: &str,
) -> Result<(InstanceLoader, InstallLoader)> {
    let (kind, embedded) = loader.split_once(':').unwrap_or((loader, ""));
    let version = if loader_version.trim().is_empty() {
        embedded.trim()
    } else {
        loader_version.trim()
    };
    match kind.trim().to_ascii_lowercase().as_str() {
        "vanilla" => Ok((InstanceLoader::Vanilla, InstallLoader::Vanilla)),
        "fabric" | "quilt" | "forge" | "neoforge" if version.is_empty() => {
            Err(Error::Loader(format!(
                "{} requires an exact loader version; use e.g. fabric:0.16.5",
                kind.trim()
            )))
        }
        "fabric" => Ok((
            InstanceLoader::Fabric {
                version: version.into(),
            },
            InstallLoader::Fabric {
                version: version.into(),
            },
        )),
        "quilt" => Ok((
            InstanceLoader::Quilt {
                version: version.into(),
            },
            InstallLoader::Quilt {
                version: version.into(),
            },
        )),
        "forge" => Ok((
            InstanceLoader::Forge {
                version: version.into(),
            },
            InstallLoader::Forge {
                version: version.into(),
            },
        )),
        "neoforge" => Ok((
            InstanceLoader::NeoForge {
                version: version.into(),
            },
            InstallLoader::NeoForge {
                version: version.into(),
            },
        )),
        other => Err(Error::Loader(format!("unsupported loader: {other}"))),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoaderKind {
    Vanilla,
    Fabric,
    Quilt,
    Forge,
    NeoForge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryPlan {
    pub coordinate: String,
    pub path: std::path::PathBuf,
    pub url: String,
}

pub fn library_plan(coordinate: &str, base_url: &str) -> Option<LibraryPlan> {
    let path = maven_path(coordinate)?;
    Some(LibraryPlan {
        coordinate: coordinate.into(),
        url: format!(
            "{}/{}",
            base_url.trim_end_matches('/'),
            path.to_string_lossy().replace('\\', "/")
        ),
        path,
    })
}

pub fn maven_path(coordinate: &str) -> Option<std::path::PathBuf> {
    let (coordinate, extension) = coordinate.rsplit_once('@').unwrap_or((coordinate, "jar"));
    let mut parts = coordinate.split(':');
    let group = parts.next()?;
    let artifact = parts.next()?;
    let version = parts.next()?;
    let classifier = parts.next();
    if parts.next().is_some()
        || [group, artifact, version]
            .iter()
            .any(|part| part.is_empty() || part == &"." || part == &"..")
        || [group, artifact, version]
            .iter()
            .any(|part| part.contains(['/', '\\']))
        || classifier.is_some_and(|part| part.is_empty() || part.contains(['/', '\\']))
        || extension.is_empty()
        || extension.contains(['/', '\\'])
    {
        return None;
    }
    let mut path = std::path::PathBuf::from(group.replace('.', "/"));
    path.push(artifact);
    path.push(version);
    let file = match classifier {
        Some(classifier) => format!("{artifact}-{version}-{classifier}.{extension}"),
        None => format!("{artifact}-{version}.{extension}"),
    };
    Some(path.join(file))
}

#[cfg(test)]
mod tests {
    use super::maven_path;

    #[test]
    fn supports_classifier_extensions_and_rejects_path_injection() {
        assert_eq!(
            maven_path("net.minecraftforge:forge:1.20.1-47.2.0:clientdata@lzma"),
            Some(std::path::PathBuf::from(
                "net/minecraftforge/forge/1.20.1-47.2.0/forge-1.20.1-47.2.0-clientdata.lzma"
            ))
        );
        assert_eq!(maven_path("group:../escape:1.0"), None);
    }
}

#[cfg(test)]
mod loader_tests {
    use super::parse_install_loader;
    use crate::core::instance::models::InstanceLoader;
    use crate::core::loader::service::InstallLoader;

    #[test]
    fn a_version_can_be_embedded_in_the_loader_name() {
        let (instance, install) = parse_install_loader("fabric:0.16.5", "").expect("embedded");
        assert_eq!(
            instance,
            InstanceLoader::Fabric {
                version: "0.16.5".into()
            }
        );
        assert_eq!(
            install,
            InstallLoader::Fabric {
                version: "0.16.5".into()
            }
        );
    }

    #[test]
    fn an_explicit_version_wins_over_the_embedded_one() {
        let (instance, _) = parse_install_loader("forge:47.1.0", "47.2.0").expect("explicit");
        assert_eq!(
            instance,
            InstanceLoader::Forge {
                version: "47.2.0".into()
            }
        );
    }

    #[test]
    fn vanilla_ignores_the_version() {
        let (instance, install) = parse_install_loader("Vanilla", "whatever").expect("vanilla");
        assert_eq!(instance, InstanceLoader::Vanilla);
        assert_eq!(install, InstallLoader::Vanilla);
    }

    #[test]
    fn a_modded_loader_without_a_version_is_refused() {
        let error = parse_install_loader("neoforge", "").expect_err("no version");
        assert!(
            error
                .to_string()
                .contains("requires an exact loader version")
        );
        assert!(parse_install_loader("unknown", "1.0").is_err());
    }
}
