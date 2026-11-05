use std::path::Path;

use crate::core::error::{Error, Result};
use crate::core::java::models::JavaInstallation;

pub fn minimum_java_major(minecraft_version: &str) -> Result<u32> {
    let mut parts = minecraft_version
        .split('.')
        .filter_map(|part| part.parse::<u32>().ok());
    let major = parts
        .next()
        .ok_or_else(|| Error::Java(format!("invalid Minecraft version: {minecraft_version}")))?;
    let minor = parts.next().unwrap_or(0);
    Ok(if major == 1 {
        match minor {
            0..=16 => 8,
            17 => 16,
            18..=20 => 17,
            _ => 21,
        }
    } else {
        21
    })
}

pub fn select_java_preferring<'a>(
    candidates: &'a [JavaInstallation],
    required_major: u32,
    preferred: Option<&Path>,
) -> Option<&'a JavaInstallation> {
    if let Some(path) = preferred
        && let Some(java) = candidates
            .iter()
            .find(|java| java.executable == path && java.major == required_major)
    {
        return Some(java);
    }
    select_java(candidates, required_major).ok()
}

pub fn select_java(
    candidates: &[JavaInstallation],
    required_major: u32,
) -> Result<&JavaInstallation> {
    candidates
        .iter()
        .find(|java| java.major == required_major)
        .or_else(|| {
            candidates
                .iter()
                .filter(|java| java.major > required_major)
                .min_by_key(|java| java.major)
        })
        .ok_or_else(|| Error::Java(format!("no Java runtime satisfies major {required_major}")))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{minimum_java_major, select_java, select_java_preferring};
    use crate::core::java::JavaInstallation;

    fn java(major: u32) -> JavaInstallation {
        JavaInstallation {
            executable: PathBuf::from(format!("java-{major}")),
            version: major.to_string(),
            major,
            vendor: None,
            is_jdk: true,
            lts: true,
        }
    }

    #[test]
    fn chooses_exact_then_smallest_higher_version() {
        let candidates = vec![java(21), java(17), java(25)];
        assert_eq!(select_java(&candidates, 17).expect("exact").major, 17);
        assert_eq!(select_java(&candidates, 18).expect("higher").major, 21);
        assert!(select_java(&candidates, 26).is_err());
    }

    #[test]
    fn prefers_the_configured_exact_runtime() {
        let mut preferred = java(17);
        preferred.executable = PathBuf::from("preferred-java-17");
        let candidates = vec![java(8), java(17), preferred.clone()];
        assert_eq!(
            select_java_preferring(&candidates, 17, Some(&preferred.executable))
                .expect("preferred")
                .executable,
            preferred.executable
        );
        assert_eq!(
            select_java_preferring(&candidates, 8, Some(&preferred.executable),)
                .expect("java 8")
                .major,
            8
        );
    }

    #[test]
    fn maps_minecraft_versions_to_minimum_java() {
        assert_eq!(minimum_java_major("1.16.5").expect("Java 8"), 8);
        assert_eq!(minimum_java_major("1.17.1").expect("Java 16"), 16);
        assert_eq!(minimum_java_major("1.20.4").expect("Java 17"), 17);
        assert_eq!(minimum_java_major("1.21.1").expect("Java 21"), 21);
    }
}
