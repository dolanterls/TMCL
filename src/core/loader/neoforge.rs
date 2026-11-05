use std::path::PathBuf;

use crate::core::error::Result;
use crate::core::loader::forge::{ForgeInstallPlan, install_plan as forge_install_plan};

pub fn installer_coordinate(minecraft_version: &str, version: &str) -> String {
    if version.starts_with(minecraft_version) {
        version.into()
    } else {
        format!("{minecraft_version}-{version}")
    }
}

pub fn install_plan(
    minecraft_version: &str,
    version: &str,
    installer_url: &str,
    installer_path: PathBuf,
    profile: &[u8],
) -> Result<ForgeInstallPlan> {
    let coordinate = installer_coordinate(minecraft_version, version);
    forge_install_plan(
        minecraft_version,
        &coordinate,
        installer_url,
        installer_path,
        profile,
    )
}

#[cfg(test)]
mod tests {
    use super::installer_coordinate;

    #[test]
    fn supports_old_and_new_neoforge_names() {
        assert_eq!(installer_coordinate("1.20.1", "47.1.0"), "1.20.1-47.1.0");
        assert_eq!(
            installer_coordinate("1.21.1", "1.21.1-21.1.1"),
            "1.21.1-21.1.1"
        );
    }
}
