use std::path::PathBuf;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JavaInstallation {
    pub executable: PathBuf,
    pub version: String,
    pub major: u32,
    pub vendor: Option<String>,
    pub is_jdk: bool,
    pub lts: bool,
}
