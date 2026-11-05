use crate::core::error::{Error, Result};
use crate::core::version::models::VersionManifest;

pub fn parse_manifest(bytes: &[u8]) -> Result<VersionManifest> {
    serde_json::from_slice(bytes)
        .map_err(|error| Error::Version(format!("parse Minecraft version manifest: {error}")))
}

#[cfg(test)]
mod tests {
    use super::parse_manifest;

    #[test]
    fn parses_release_snapshot_and_ignores_unknown_fields() {
        let manifest = parse_manifest(include_bytes!(
            "../../../fixtures/version_manifests/version_manifest_v2.json"
        ))
        .expect("parse fixture");

        assert_eq!(manifest.latest.release, "1.21.1");
        assert_eq!(manifest.versions.len(), 2);
        assert_eq!(manifest.versions[0].kind, "release");
        assert_eq!(manifest.versions[1].kind, "snapshot");
        assert_eq!(manifest.versions[0].sha1.as_deref(), Some("release-sha1"));
    }
}
