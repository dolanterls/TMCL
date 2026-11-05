use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::core::config::storage::write_json_atomic;
use crate::core::error::{Error, Result};

use super::{PersistedTaskRecord, TaskOperation, TaskRecord};

const TASK_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone)]
pub struct JsonTaskStore {
    directory: PathBuf,
}

impl JsonTaskStore {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn load(&self) -> Result<Vec<TaskRecord>> {
        fs::create_dir_all(&self.directory)?;
        let mut paths = fs::read_dir(&self.directory)?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
            .collect::<Vec<_>>();
        paths.sort();

        let mut records = HashMap::<Uuid, TaskRecord>::new();
        for path in paths {
            let bytes = fs::read(&path)?;
            let value: Value = match serde_json::from_slice(&bytes) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let Ok(mut record) = self.decode(value) else {
                continue;
            };
            let replace = records
                .get(&record.id)
                .is_none_or(|current| record.updated_at >= current.updated_at);
            if replace {
                record.updated_at = record.updated_at.max(record.created_at);
                records.insert(record.id, record);
            }
        }

        let mut records = records.into_values().collect::<Vec<_>>();
        records.sort_by_key(|record| record.id);
        Ok(records)
    }

    pub fn upsert(&self, record: &TaskRecord) -> Result<()> {
        fs::create_dir_all(&self.directory)?;
        let path = self.path(record.id);
        let persisted = record.persisted();
        if persisted.schema_version != TASK_SCHEMA_VERSION {
            return Err(Error::Download(format!(
                "cannot persist unsupported task schema: {}",
                persisted.schema_version
            )));
        }
        write_json_atomic(&path, &persisted)
    }

    pub fn remove(&self, id: Uuid) -> Result<()> {
        let path = self.path(id);
        remove_if_exists(&path)?;
        remove_if_exists(&path.with_extension("cancel"))?;
        Ok(())
    }

    pub fn path(&self, id: Uuid) -> PathBuf {
        self.directory.join(format!("{id}.json"))
    }

    fn decode(&self, value: Value) -> Result<TaskRecord> {
        let Some(version) = value.get("schema_version").and_then(Value::as_u64) else {
            let legacy: LegacyTaskRecord = serde_json::from_value(value)?;
            return unrecoverable_record(
                legacy.id,
                legacy.label,
                "旧任务格式不再自动迁移，请重新创建任务",
            );
        };
        if version == TASK_SCHEMA_VERSION as u64 {
            let persisted: PersistedTaskRecord = serde_json::from_value(value)?;
            return TaskRecord::from_persisted(persisted);
        }
        let id = value
            .get("id")
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
            .ok_or_else(|| Error::Download("旧任务缺少有效任务 ID".into()))?;
        let label = value
            .get("label")
            .and_then(Value::as_str)
            .unwrap_or("旧任务")
            .to_owned();
        unrecoverable_record(id, label, format!("不支持的任务 schema version {version}"))
    }
}

fn unrecoverable_record(id: Uuid, label: String, reason: impl Into<String>) -> Result<TaskRecord> {
    let mut record = TaskRecord::new(
        id,
        TaskOperation::Unrecoverable {
            label,
            reason: reason.into(),
        },
        now(),
    );
    record.start()?;
    record.fail("旧任务不可恢复，请重新创建任务", false)?;
    Ok(record)
}

fn remove_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

#[derive(Debug, Deserialize)]
struct LegacyTaskRecord {
    id: Uuid,
    #[serde(default = "default_label")]
    label: String,
}

fn default_label() -> String {
    "Task".into()
}
