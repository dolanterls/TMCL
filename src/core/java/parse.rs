use std::path::{Path, PathBuf};

use crate::core::error::{Error, Result};
use crate::core::java::models::JavaInstallation;

pub fn parse_release(executable: impl Into<PathBuf>, content: &str) -> Result<JavaInstallation> {
    let executable = executable.into();
    let version = release_value(content, "JAVA_VERSION")
        .ok_or_else(|| Error::Java("release file has no JAVA_VERSION".into()))?;
    let vendor = release_value(content, "IMPLEMENTOR");
    let runtime_name = release_value(content, "JAVA_RUNTIME_NAME").unwrap_or_default();
    Ok(JavaInstallation {
        executable,
        major: parse_major(&version)?,
        lts: is_lts(parse_major(&version)?),
        version,
        vendor,
        is_jdk: runtime_name.contains("JDK"),
    })
}

pub fn parse_command_output(
    executable: impl Into<PathBuf>,
    output: &str,
) -> Result<JavaInstallation> {
    let executable = executable.into();
    let version = output
        .lines()
        .find_map(|line| {
            line.split_once("java.version =")
                .map(|(_, value)| value.trim())
        })
        .or_else(|| {
            output
                .split_once("version \"")
                .and_then(|(_, rest)| rest.split('"').next())
        })
        .ok_or_else(|| Error::Java("Java version is missing from command output".into()))?
        .to_owned();
    let vendor = output.lines().find_map(|line| {
        line.split_once("java.vendor =")
            .map(|(_, value)| value.trim().to_owned())
    });
    let major = parse_major(&version)?;
    Ok(JavaInstallation {
        executable,
        version,
        major,
        vendor,
        is_jdk: output.contains("java.home") && output.contains("jdk"),
        lts: is_lts(major),
    })
}

pub fn parse_probe(
    executable: impl Into<PathBuf>,
    release: Option<&str>,
    command_output: Option<&str>,
) -> Result<JavaInstallation> {
    if let Some(release) = release {
        return parse_release(executable, release);
    }
    parse_command_output(
        executable,
        command_output.ok_or_else(|| Error::Java("Java probe produced no output".into()))?,
    )
}

fn release_value(content: &str, key: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let (name, value) = line.split_once('=')?;
        (name.trim() == key).then(|| value.trim().trim_matches('"').to_owned())
    })
}

fn parse_major(version: &str) -> Result<u32> {
    let version = version.trim();
    let major = if let Some(rest) = version.strip_prefix("1.") {
        rest.split(['.', '_', '-']).next()
    } else {
        version.split(['.', '-', '_']).next()
    }
    .and_then(|value| value.parse().ok())
    .ok_or_else(|| Error::Java(format!("invalid Java version: {version}")))?;
    if major == 0 {
        return Err(Error::Java(format!(
            "invalid Java major version: {version}"
        )));
    }
    Ok(major)
}

fn is_lts(major: u32) -> bool {
    matches!(major, 8 | 11 | 17 | 21 | 25)
}

#[allow(dead_code)]
fn release_path(executable: &Path) -> PathBuf {
    executable
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new("."))
        .join("release")
}

#[cfg(test)]
mod tests {
    use super::{parse_command_output, parse_probe, parse_release};

    #[test]
    fn parses_java_8_11_17_21_and_25() {
        for (version, major) in [
            ("1.8.0_392", 8),
            ("11.0.22", 11),
            ("17.0.10", 17),
            ("21.0.4", 21),
            ("25.0.1", 25),
        ] {
            let java = parse_release("java.exe", &format!("JAVA_VERSION=\"{version}\""))
                .expect("parse Java release");
            assert_eq!(java.major, major);
        }
    }

    #[test]
    fn release_file_has_priority_and_command_stderr_is_supported() {
        let release = parse_probe(
            "release-java.exe",
            Some("JAVA_VERSION=\"21.0.4\"\nIMPLEMENTOR=\"Eclipse Adoptium\""),
            Some("java.version = 17.0.10"),
        )
        .expect("release probe");
        assert_eq!(release.major, 21);
        assert_eq!(release.vendor.as_deref(), Some("Eclipse Adoptium"));

        let command = parse_command_output(
            "command-java.exe",
            "java.vendor = Microsoft\njava.version = 17.0.11\nopenjdk version \"17.0.11\"",
        )
        .expect("command probe");
        assert_eq!(command.major, 17);
        assert_eq!(command.vendor.as_deref(), Some("Microsoft"));
    }

    #[test]
    fn rejects_invalid_version() {
        assert!(parse_release("java.exe", "JAVA_VERSION=\"not-a-version\"").is_err());
    }
}
