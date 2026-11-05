use std::path::Path;

use crate::core::config::storage::{read_json, write_json_atomic};
use crate::core::error::Result;
use crate::core::instance::models::InstanceConfig;

pub fn record_session(instance_directory: &Path, play_time_seconds: u64) -> Result<InstanceConfig> {
    let metadata = instance_directory.join("tmcl.json");
    let mut instance: InstanceConfig = read_json(&metadata)?;
    instance.stats.launch_count = instance.stats.launch_count.saturating_add(1);
    instance.stats.play_time_seconds = instance
        .stats
        .play_time_seconds
        .saturating_add(play_time_seconds);
    write_json_atomic(&metadata, &instance)?;
    Ok(instance)
}

#[cfg(test)]
mod tests {
    use super::record_session;
    use crate::core::config::storage::write_json_atomic;
    use crate::core::instance::models::InstanceConfig;

    #[test]
    fn records_launch_count_and_play_time_atomically() {
        let root = std::env::temp_dir().join(format!("tmcl-stats-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("instance directory");
        let mut instance = InstanceConfig::new(&root, uuid::Uuid::new_v4(), "test", "1.21.1");
        instance.stats.launch_count = 2;
        write_json_atomic(&root.join("tmcl.json"), &instance).expect("write instance metadata");
        let updated = record_session(&root, 45).expect("record session");
        assert_eq!(updated.stats.launch_count, 3);
        assert_eq!(updated.stats.play_time_seconds, 45);
        std::fs::remove_dir_all(root).expect("clean test directory");
    }
}
