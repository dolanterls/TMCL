use clap::{Args, Subcommand};
use uuid::Uuid;

#[derive(Debug, Subcommand)]
pub enum TaskCommand {
    /// List persisted tasks
    List(TaskListArgs),
    /// Follow a task until it ends
    Watch(TaskWatchArgs),
    /// Retry a failed task
    Retry(TaskRetryArgs),
    /// Stop a task and remove its record
    Cancel(TaskCancelArgs),
}

#[derive(Debug, Args)]
pub struct TaskListArgs {
    #[arg(
        long,
        value_name = "STATE",
        value_parser = ["all", "queued", "running", "done", "failed", "paused", "cancelled"],
        default_value = "all"
    )]
    pub state: String,
}

#[derive(Debug, Args)]
pub struct TaskWatchArgs {
    #[arg(value_name = "TASK_ID")]
    pub id: Uuid,
}

#[derive(Debug, Args)]
pub struct TaskRetryArgs {
    #[arg(value_name = "TASK_ID")]
    pub id: Uuid,
}

#[derive(Debug, Args)]
pub struct TaskCancelArgs {
    #[arg(value_name = "TASK_ID")]
    pub id: Uuid,
    #[arg(long, help = "Skip the confirmation prompt")]
    pub yes: bool,
}
