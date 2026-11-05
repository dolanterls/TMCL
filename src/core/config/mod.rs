pub mod log_retention;
pub mod migrations;
pub mod models;
pub mod paths;
pub mod storage;

pub use models::{CURRENT_SCHEMA_VERSION, Config};
pub use paths::DataPaths;

use crate::core::error::Result;

pub fn load_or_default(paths: &DataPaths) -> Result<Config> {
    if paths.config_file.exists() {
        migrations::load_and_migrate(&paths.config_file)
    } else {
        let config = Config::default();
        storage::write_json_atomic(&paths.config_file, &config)?;
        Ok(config)
    }
}
