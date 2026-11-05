use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::core::download::DownloadSourceSnapshot;
use crate::core::error::{Error, Result};
use crate::core::resource::model::{ResourceKind, ResourceSource};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoaderSpec {
    Vanilla,
    Fabric { version: String },
    Quilt { version: String },
    Forge { version: String },
    NeoForge { version: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceInstallSpec {
    pub instance_id: Uuid,
    pub game_directory: PathBuf,
    pub instance_directory: PathBuf,
    pub instance_name: String,
    pub minecraft_version: String,
    pub loader: LoaderSpec,
    pub java_executable: Option<PathBuf>,
    #[serde(default)]
    pub download_snapshot: DownloadSourceSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JavaRuntimeSpec {
    pub data_root: PathBuf,
    pub major: u32,
    #[serde(default)]
    pub download_snapshot: DownloadSourceSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceInstallSpec {
    pub instance_id: Uuid,
    pub instance_root: PathBuf,
    pub destination: PathBuf,
    pub cache_root: PathBuf,
    pub source: ResourceSource,
    pub kind: ResourceKind,
    pub project_id: String,
    pub file_id: String,
    pub file_name: String,
    pub candidate_urls: Vec<String>,
    pub expected_sha1: Option<String>,
    pub expected_size: Option<u64>,
    #[serde(default)]
    pub download_snapshot: DownloadSourceSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceSaveAsSpec {
    pub destination: PathBuf,
    pub cache_root: PathBuf,
    pub source: ResourceSource,
    pub kind: ResourceKind,
    pub project_id: String,
    pub file_id: String,
    pub file_name: String,
    pub candidate_urls: Vec<String>,
    pub expected_sha1: Option<String>,
    pub expected_size: Option<u64>,
    #[serde(default)]
    pub download_snapshot: DownloadSourceSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModpackInstallSpec {
    pub staging_path: PathBuf,
    pub cache_root: PathBuf,
    pub source: ResourceSource,
    pub project_id: String,
    pub file_id: String,
    pub file_name: String,
    pub candidate_urls: Vec<String>,
    pub expected_sha1: Option<String>,
    pub expected_size: Option<u64>,
    #[serde(default)]
    pub download_snapshot: DownloadSourceSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModpackFileSpec {
    pub path: String,
    pub urls: Vec<String>,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModpackFilesSpec {
    pub instance_id: Uuid,
    pub instance_root: PathBuf,
    pub cache_root: PathBuf,
    pub files: Vec<ModpackFileSpec>,
    #[serde(default)]
    pub download_snapshot: DownloadSourceSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceRepairSpec {
    pub instance_id: Uuid,
    pub instance_name: String,
    pub instance_directory: PathBuf,
    pub game_directory: PathBuf,
    pub cache_root: PathBuf,
    pub missing: Vec<crate::core::launch::preflight::MissingFile>,
    #[serde(default)]
    pub download_snapshot: DownloadSourceSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "spec")]
pub enum TaskOperation {
    InstanceInstall(InstanceInstallSpec),
    InstanceRepair(InstanceRepairSpec),
    JavaRuntime(JavaRuntimeSpec),
    ResourceInstall(ResourceInstallSpec),
    ResourceSaveAs(ResourceSaveAsSpec),
    ModpackInstall(ModpackInstallSpec),
    ModpackFiles(ModpackFilesSpec),
    Unrecoverable { label: String, reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskOperationType {
    InstanceInstall,
    InstanceRepair,
    JavaRuntime,
    ResourceInstall,
    ResourceSaveAs,
    ModpackInstall,
    ModpackFiles,
    Unrecoverable,
}

impl TaskOperation {
    pub fn operation_type(&self) -> TaskOperationType {
        match self {
            Self::InstanceInstall(_) => TaskOperationType::InstanceInstall,
            Self::InstanceRepair(_) => TaskOperationType::InstanceRepair,
            Self::JavaRuntime(_) => TaskOperationType::JavaRuntime,
            Self::ResourceInstall(_) => TaskOperationType::ResourceInstall,
            Self::ResourceSaveAs(_) => TaskOperationType::ResourceSaveAs,
            Self::ModpackInstall(_) => TaskOperationType::ModpackInstall,
            Self::ModpackFiles(_) => TaskOperationType::ModpackFiles,
            Self::Unrecoverable { .. } => TaskOperationType::Unrecoverable,
        }
    }

    /// Bytes this operation already knows it will move, available before the
    /// first progress report.
    ///
    /// A queued step with a known size keeps the job row's bar meaningful
    /// instead of leaving it indeterminate until the step starts.
    pub fn planned_total(&self) -> Option<u64> {
        match self {
            Self::ModpackInstall(spec) => spec.expected_size,
            Self::ResourceInstall(spec) => spec.expected_size,
            Self::ResourceSaveAs(spec) => spec.expected_size,
            Self::ModpackFiles(spec) => spec.files.iter().map(|file| file.size).sum(),
            _ => None,
        }
    }

    /// The instance this operation is filling, when it fills one.
    ///
    /// The instance list asks this so a half-built instance does not read
    /// "ready": a modpack writes the version jar and json before the mods, so
    /// on disk alone it looks finished from the first minute.
    pub fn instance_id(&self) -> Option<Uuid> {
        match self {
            Self::InstanceInstall(spec) => Some(spec.instance_id),
            Self::InstanceRepair(spec) => Some(spec.instance_id),
            Self::ModpackFiles(spec) => Some(spec.instance_id),
            _ => None,
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::InstanceInstall(spec) => format!(
                "Install {} ({}, {})",
                spec.instance_name,
                spec.minecraft_version,
                spec.loader.label()
            ),
            Self::InstanceRepair(spec) => format!(
                "Repair {} ({} file(s))",
                spec.instance_name,
                spec.missing.len()
            ),
            Self::JavaRuntime(spec) => format!("Install Mojang Java {}", spec.major),
            Self::ResourceInstall(spec) => format!(
                "Install {} {} ({})",
                spec.kind.label(),
                spec.project_id,
                spec.file_name
            ),
            Self::ResourceSaveAs(spec) => format!(
                "Save {} {} as {}",
                spec.kind.label(),
                spec.project_id,
                spec.file_name
            ),
            Self::ModpackInstall(spec) => {
                format!("Download modpack {} ({})", spec.project_id, spec.file_name)
            }
            Self::ModpackFiles(spec) => {
                format!("Install {} modpack files", spec.files.len())
            }
            Self::Unrecoverable { label, .. } => label.clone(),
        }
    }
}

impl LoaderSpec {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Vanilla => "vanilla",
            Self::Fabric { .. } => "fabric",
            Self::Quilt { .. } => "quilt",
            Self::Forge { .. } => "forge",
            Self::NeoForge { .. } => "neoforge",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskState {
    Queued,
    Running,
    Paused,
    Succeeded,
    Failed,
    Cancelled,
}

impl TaskState {
    pub fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running | Self::Paused)
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactLedger {
    /// Partial files this run owns and must delete if it is cancelled.
    ///
    /// The only field that has to survive a restart: cleanup after a crash
    /// reads it back. It stays small on purpose - at most one entry per
    /// in-flight download.
    #[serde(default)]
    pub owned_temporary: Vec<PathBuf>,
    /// Files this run moved into place. Session bookkeeping only.
    ///
    /// Persisting it made a modpack install write a 1.1 MB task record every
    /// second (5231 committed files, one entry each) while holding the task
    /// state mutex. Nothing reads it back, so it never reaches the disk.
    #[serde(skip)]
    pub committed: Vec<PathBuf>,
    /// Files this run only referenced rather than produced. Session
    /// bookkeeping only, and skipped for the same reason as `committed`.
    #[serde(skip)]
    pub shared: Vec<(String, PathBuf)>,
}

impl ArtifactLedger {
    fn add_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
        if !paths.contains(&path) {
            paths.push(path);
        }
    }

    pub fn own_temporary(&mut self, path: PathBuf) {
        Self::add_unique(&mut self.owned_temporary, path);
    }

    pub fn commit(&mut self, path: PathBuf) {
        Self::add_unique(&mut self.committed, path);
    }

    pub fn reference_shared(&mut self, key: String, path: PathBuf) {
        if !self.shared.iter().any(|item| item.0 == key) {
            self.shared.push((key, path));
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRecord {
    pub id: Uuid,
    pub operation: TaskOperation,
    /// Which multi-step job this task belongs to, if any.
    ///
    /// A modpack install is a download, a loader install and a file install
    /// running as three tasks. They share a job id so the task list can show
    /// them as one row; the tasks themselves stay independent, retryable and
    /// cancellable on their own.
    pub job: Option<Uuid>,
    /// A step that must finish successfully before this one may start.
    ///
    /// The modpack file install writes into the instance the loader step is
    /// still filling, so it is queued immediately (it survives a restart) but
    /// its worker parks until the loader step lands.
    pub depends_on: Option<Uuid>,
    pub state: TaskState,
    pub retryable: bool,
    pub attempt: u32,
    pub completed: u64,
    pub total: Option<u64>,
    pub failure: Option<String>,
    pub created_at: u64,
    /// Submission order, used to keep a job's steps in the order they were
    /// queued. `created_at` has second resolution and a job's steps are
    /// submitted in the same second, so it cannot order them.
    pub created_seq: u64,
    pub updated_at: u64,
    pub artifacts: ArtifactLedger,
    paused_from_running: bool,
    /// Process that owns this task. A second launcher process must not treat
    /// this record as abandoned while that process is still working on it.
    pub owner_pid: Option<u32>,
    /// Start time of the owning process, used to tell a live owner from a
    /// recycled process id. `None` on platforms where it cannot be read.
    pub owner_started_at: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedTaskRecord {
    pub schema_version: u32,
    pub id: Uuid,
    pub operation: TaskOperation,
    #[serde(default)]
    pub job: Option<Uuid>,
    #[serde(default)]
    pub depends_on: Option<Uuid>,
    pub state: TaskState,
    #[serde(default)]
    pub retryable: bool,
    #[serde(default)]
    pub attempt: u32,
    #[serde(default)]
    pub completed: u64,
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub failure: Option<String>,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub created_seq: u64,
    #[serde(default)]
    pub updated_at: u64,
    #[serde(default)]
    pub artifacts: ArtifactLedger,
    #[serde(default)]
    pub paused_from_running: bool,
    #[serde(default)]
    pub owner_pid: Option<u32>,
    #[serde(default)]
    pub owner_started_at: Option<u64>,
}

impl From<&TaskRecord> for PersistedTaskRecord {
    fn from(task: &TaskRecord) -> Self {
        Self {
            schema_version: 2,
            id: task.id,
            operation: task.operation.clone(),
            job: task.job,
            depends_on: task.depends_on,
            state: task.state,
            retryable: task.retryable,
            attempt: task.attempt,
            completed: task.completed,
            total: task.total,
            failure: task.failure.clone(),
            created_at: task.created_at,
            created_seq: task.created_seq,
            updated_at: task.updated_at,
            artifacts: task.artifacts.clone(),
            paused_from_running: task.paused_from_running,
            owner_pid: task.owner_pid,
            owner_started_at: task.owner_started_at,
        }
    }
}

impl TryFrom<PersistedTaskRecord> for TaskRecord {
    type Error = Error;

    fn try_from(value: PersistedTaskRecord) -> Result<Self> {
        if value.schema_version != 2 {
            return Err(Error::Download(format!(
                "unsupported task schema version: {}",
                value.schema_version
            )));
        }
        Ok(Self {
            id: value.id,
            operation: value.operation,
            job: value.job,
            depends_on: value.depends_on,
            state: value.state,
            retryable: value.retryable,
            attempt: value.attempt,
            completed: value.completed,
            total: value.total,
            failure: value.failure,
            created_at: value.created_at,
            created_seq: value.created_seq,
            updated_at: value.updated_at,
            artifacts: value.artifacts,
            paused_from_running: value.paused_from_running,
            owner_pid: value.owner_pid,
            owner_started_at: value.owner_started_at,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskAction {
    Queued,
    Running,
    Paused,
    Retrying,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskSnapshot {
    pub id: Uuid,
    /// Shared by every step of one job; `None` for a standalone task.
    pub job: Option<Uuid>,
    /// Submission order across all tasks, so a job's steps list in the order
    /// they were queued rather than in UUID order.
    pub created_seq: u64,
    pub label: String,
    pub operation_type: TaskOperationType,
    pub state: TaskState,
    pub retryable: bool,
    pub attempt: u32,
    pub completed: u64,
    pub total: Option<u64>,
    pub failure: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskOutcome {
    Succeeded,
    Failed { message: String, retryable: bool },
    Cancelled { message: String, retryable: bool },
}

impl TaskRecord {
    pub fn new(id: Uuid, operation: TaskOperation, now: u64) -> Self {
        Self {
            id,
            operation,
            job: None,
            depends_on: None,
            state: TaskState::Queued,
            retryable: false,
            attempt: 0,
            completed: 0,
            total: None,
            failure: None,
            created_at: now,
            created_seq: 0,
            updated_at: now,
            artifacts: ArtifactLedger::default(),
            paused_from_running: false,
            owner_pid: Some(std::process::id()),
            owner_started_at: crate::platform::windows::process_started_at(std::process::id()),
        }
    }

    /// Whether the process that created this record is still running.
    ///
    /// Records written before owner tracking existed have no owner and count as
    /// abandoned: they can only have come from a build that is no longer
    /// running this code.
    pub fn owner_is_alive(&self) -> bool {
        match (self.owner_pid, self.owner_started_at) {
            (Some(pid), Some(started)) => {
                crate::platform::windows::process_started_at(pid) == Some(started)
            }
            (Some(pid), None) => crate::platform::windows::process_is_alive(pid),
            (None, _) => false,
        }
    }

    /// Tags the record as part of a multi-step job. The engine sets this when
    /// the caller submits with one; the operation itself stays job-agnostic.
    #[must_use]
    pub fn with_job(mut self, job: Option<Uuid>) -> Self {
        self.job = job;
        self
    }

    /// Stamps the submission order the engine assigned to this task.
    #[must_use]
    pub fn with_seq(mut self, seq: u64) -> Self {
        self.created_seq = seq;
        self
    }

    /// Chains this task behind another one; see [`TaskRecord::depends_on`].
    #[must_use]
    pub fn with_dependency(mut self, dependency: Option<Uuid>) -> Self {
        self.depends_on = dependency;
        self
    }

    pub fn from_persisted(value: PersistedTaskRecord) -> Result<Self> {
        value.try_into()
    }

    pub fn persisted(&self) -> PersistedTaskRecord {
        self.into()
    }

    pub fn snapshot(&self) -> TaskSnapshot {
        TaskSnapshot {
            id: self.id,
            job: self.job,
            created_seq: self.created_seq,
            label: self.operation.label(),
            operation_type: self.operation.operation_type(),
            state: self.state,
            retryable: self.retryable,
            attempt: self.attempt,
            completed: self.completed,
            total: self.total,
            failure: self.failure.clone(),
        }
    }

    pub fn start(&mut self) -> Result<()> {
        if self.state != TaskState::Queued {
            return Err(invalid_transition(self.state, TaskState::Queued));
        }
        self.state = TaskState::Running;
        self.attempt = self.attempt.saturating_add(1);
        self.failure = None;
        self.retryable = false;
        self.updated_at = now();
        Ok(())
    }

    pub fn pause_or_resume(&mut self) -> Result<TaskAction> {
        let action = match self.state {
            TaskState::Queued => {
                self.paused_from_running = false;
                self.state = TaskState::Paused;
                TaskAction::Paused
            }
            TaskState::Running => {
                self.paused_from_running = true;
                self.state = TaskState::Paused;
                TaskAction::Paused
            }
            TaskState::Paused => {
                self.state = if self.paused_from_running {
                    TaskState::Running
                } else {
                    TaskState::Queued
                };
                TaskAction::from_state(self.state)
            }
            actual => return Err(invalid_transition(actual, TaskState::Running)),
        };
        self.updated_at = now();
        Ok(action)
    }

    pub fn progress(&mut self, completed: u64, total: Option<u64>) -> Result<()> {
        if self.state != TaskState::Running {
            return Err(invalid_transition(self.state, TaskState::Running));
        }
        // Progress is a report, not an assertion. A total that is learned while
        // the run is in flight (a file whose metadata carried no size) can lag
        // one report behind the byte count, and failing the whole task over a
        // bar that reads 101% would be a far worse outcome than a clamped bar.
        let completed = total.map_or(completed, |total| completed.min(total));
        self.completed = completed;
        self.total = total;
        self.updated_at = now();
        Ok(())
    }

    pub fn succeed(&mut self) -> Result<()> {
        if !matches!(self.state, TaskState::Running | TaskState::Paused) {
            return Err(invalid_transition(self.state, TaskState::Running));
        }
        self.state = TaskState::Succeeded;
        self.retryable = false;
        self.failure = None;
        self.updated_at = now();
        Ok(())
    }

    pub fn fail(&mut self, message: impl Into<String>, retryable: bool) -> Result<()> {
        // A queued task can fail before it ever runs: a chained step whose
        // predecessor failed never gets to start, and it must not sit in the
        // queue forever.
        if !matches!(
            self.state,
            TaskState::Queued | TaskState::Running | TaskState::Paused
        ) {
            return Err(invalid_transition(self.state, TaskState::Running));
        }
        self.state = TaskState::Failed;
        self.retryable = retryable;
        self.failure = Some(message.into());
        self.paused_from_running = false;
        self.updated_at = now();
        Ok(())
    }

    pub fn cancel(&mut self, message: impl Into<String>, retryable: bool) -> Result<()> {
        if !self.state.is_active() {
            return Err(invalid_transition(self.state, TaskState::Queued));
        }
        self.state = TaskState::Cancelled;
        self.retryable = retryable;
        self.failure = Some(message.into());
        self.paused_from_running = false;
        self.updated_at = now();
        Ok(())
    }

    pub fn mark_interrupted(&mut self) -> bool {
        if !self.state.is_active() {
            return false;
        }
        self.state = TaskState::Failed;
        let recoverable = !matches!(self.operation, TaskOperation::Unrecoverable { .. });
        self.retryable = recoverable;
        self.failure = Some(if recoverable {
            "Interrupted when TMCL exited".into()
        } else {
            "旧任务缺少完整操作参数，需要重新创建任务".into()
        });
        self.paused_from_running = false;
        self.updated_at = now();
        true
    }

    pub fn prepare_retry(&mut self) -> Result<TaskAction> {
        if !matches!(self.state, TaskState::Failed | TaskState::Cancelled) || !self.retryable {
            return Err(Error::Download(
                "only retryable failed or cancelled tasks can be retried".into(),
            ));
        }
        self.state = TaskState::Queued;
        self.retryable = false;
        self.completed = 0;
        self.total = None;
        self.failure = None;
        self.paused_from_running = false;
        self.updated_at = now();
        Ok(TaskAction::Retrying)
    }

    pub fn record_owned_temporary(&mut self, path: PathBuf) {
        self.artifacts.own_temporary(path);
    }

    pub fn record_committed(&mut self, path: PathBuf) {
        self.artifacts.commit(path);
    }

    pub fn record_shared(&mut self, key: impl Into<String>, path: PathBuf) {
        self.artifacts.reference_shared(key.into(), path);
    }
}

impl TaskAction {
    fn from_state(state: TaskState) -> Self {
        match state {
            TaskState::Queued => Self::Queued,
            TaskState::Running => Self::Running,
            TaskState::Paused => Self::Paused,
            _ => unreachable!("pause/resume only produces active states"),
        }
    }
}

fn invalid_transition(actual: TaskState, expected: TaskState) -> Error {
    Error::Download(format!(
        "invalid task transition from {actual:?}; expected {expected:?}"
    ))
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use uuid::Uuid;

    use super::{InstanceRepairSpec, TaskOperation, TaskOperationType};

    fn repair_spec() -> InstanceRepairSpec {
        InstanceRepairSpec {
            instance_id: Uuid::new_v4(),
            instance_name: "pack".into(),
            instance_directory: PathBuf::from("C:/game/versions/pack"),
            game_directory: PathBuf::from("C:/game"),
            cache_root: PathBuf::from("C:/cache"),
            missing: Vec::new(),
            download_snapshot: Default::default(),
        }
    }

    #[test]
    fn repair_operation_has_its_own_type_and_label() {
        let operation = TaskOperation::InstanceRepair(repair_spec());

        assert_eq!(
            operation.operation_type(),
            TaskOperationType::InstanceRepair
        );
        assert_eq!(operation.label(), "Repair pack (0 file(s))");

        let record = super::TaskRecord::new(Uuid::new_v4(), operation, 0);
        assert_eq!(
            record.operation.operation_type(),
            TaskOperationType::InstanceRepair
        );
        assert_eq!(record.state, super::TaskState::Queued);
    }

    /// The job id has to survive a restart: the grouped task list is rebuilt
    /// from the persisted records alone.
    #[test]
    fn a_job_id_survives_persistence() {
        let job = Uuid::new_v4();
        let record = super::TaskRecord::new(
            Uuid::new_v4(),
            TaskOperation::InstanceRepair(repair_spec()),
            0,
        )
        .with_job(Some(job));
        let encoded = serde_json::to_string(&record.persisted()).expect("serialize");
        let decoded =
            super::TaskRecord::from_persisted(serde_json::from_str(&encoded).expect("deserialize"))
                .expect("restore");
        assert_eq!(decoded.job, Some(job));
        assert_eq!(decoded.snapshot().job, Some(job));
    }

    /// Records written before the field existed must still load, as standalone
    /// tasks.
    #[test]
    fn records_without_a_job_load_as_standalone() {
        let record = super::TaskRecord::new(
            Uuid::new_v4(),
            TaskOperation::InstanceRepair(repair_spec()),
            0,
        );
        let mut value = serde_json::to_value(record.persisted()).expect("serialize");
        value.as_object_mut().expect("object").remove("job");
        let restored =
            super::TaskRecord::from_persisted(serde_json::from_value(value).expect("deserialize"))
                .expect("restore");
        assert_eq!(restored.job, None);
    }

    #[test]
    fn repair_operation_round_trips_through_serde() {
        let operation = TaskOperation::InstanceRepair(repair_spec());
        let encoded = serde_json::to_string(&operation).expect("serialize");
        let decoded: TaskOperation = serde_json::from_str(&encoded).expect("deserialize");
        assert_eq!(decoded.operation_type(), TaskOperationType::InstanceRepair);
    }
}
