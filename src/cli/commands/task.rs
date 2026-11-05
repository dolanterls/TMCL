use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;

use crate::cli::args::TaskCommand;
use crate::cli::consent::{Consent, Decision, require};
use crate::cli::output::OutputOptions;
use crate::cli::progress::{ProgressRenderer, Watch};
use crate::core::config::paths::DataPaths;
use crate::core::error::{Error, Result};
use crate::core::task::store::JsonTaskStore;
use crate::core::task::{MinecraftTaskRunner, TaskEngine, TaskSnapshot, TaskState};

pub(super) async fn run(command: TaskCommand, output: OutputOptions) -> Result<()> {
    let paths = DataPaths::discover()?;
    paths.ensure_dirs()?;
    let store = JsonTaskStore::new(paths.tasks_dir.clone());
    let engine = TaskEngine::open(store.clone(), Arc::new(MinecraftTaskRunner), 3)?;

    match command {
        TaskCommand::List(args) => {
            let tasks = engine
                .snapshots()
                .into_iter()
                .filter(|task| matches_filter(&args.state, task.state))
                .collect::<Vec<_>>();
            output.print(&tasks, || format_tasks(&tasks))
        }
        TaskCommand::Watch(args) => watch(args.id, &engine, &store, &output).await,
        TaskCommand::Retry(args) => {
            engine.retry(args.id).await?;
            output.print(&serde_json::json!({"id": args.id, "retried": true}), || {
                format!("Retried task {}", args.id)
            })
        }
        TaskCommand::Cancel(args) => {
            let consent = Consent::new(args.yes, &output);
            let action = format!("cancel task {}", args.id);
            // Cancelling removes the record as well: there is no "stopped but
            // remembered" state in the engine, and the TUI behaves the same way.
            let plan = format!("Will stop task {} and delete its record.", args.id);
            if require(&consent, &action, || plan.clone())? == Decision::Dry {
                // Same shape as the other destructive commands: the plan on
                // stdout when asked for JSON, and no side effect.
                return output.print(
                    &serde_json::json!({
                        "id": args.id,
                        "plan": plan,
                        "applied": false,
                    }),
                    || plan.clone(),
                );
            }
            let result = engine.delete(args.id).await?;
            output.print(
                &serde_json::json!({
                    "id": args.id,
                    "deleted": true,
                    "cleanup_pending": result.cleanup_pending,
                    "cleanup_errors": result.cleanup_errors,
                }),
                || format!("Cancelled task {}", args.id),
            )
        }
    }
}

/// Follows a task to its end.
///
/// Only a task this process is running publishes progress; for one started
/// elsewhere the best available view is the checkpoint it writes to disk.
async fn watch(
    id: Uuid,
    engine: &TaskEngine,
    store: &JsonTaskStore,
    output: &OutputOptions,
) -> Result<()> {
    let snapshot = engine
        .snapshots()
        .into_iter()
        .find(|task| task.id == id)
        .ok_or_else(|| Error::Download(format!("task not found: {id}")))?;

    if snapshot.state.is_active() && !engine.is_locally_running(id) {
        return watch_on_disk(id, store, output).await;
    }
    if snapshot.state.is_terminal() {
        return report_terminal(&snapshot, output);
    }

    let mut renderer = ProgressRenderer::new(output.progress_mode(), id, "task");
    let outcome = Watch::new(id)
        .run(engine, &mut renderer, |line| {
            eprint!("{line}");
            let _ = std::io::Write::flush(&mut std::io::stderr());
        })
        .await?;
    match outcome {
        crate::core::task::TaskOutcome::Succeeded => output
            .print(&serde_json::json!({"id": id, "state": "succeeded"}), || {
                format!("Task {id} succeeded")
            }),
        crate::core::task::TaskOutcome::Failed { message, .. } => Err(Error::Download(message)),
        crate::core::task::TaskOutcome::Cancelled { message, .. } => Err(Error::Download(message)),
    }
}

async fn watch_on_disk(id: Uuid, store: &JsonTaskStore, output: &OutputOptions) -> Result<()> {
    if !output.quiet {
        eprintln!(
            "task {id} runs in another process; showing the snapshot it writes to disk every 2s"
        );
    }
    loop {
        let snapshot = store
            .load()?
            .into_iter()
            .find(|record| record.id == id)
            .map(|record| record.snapshot())
            .ok_or_else(|| Error::Download(format!("task not found: {id}")))?;
        if snapshot.state.is_terminal() {
            return report_terminal(&snapshot, output);
        }
        if !output.quiet {
            eprintln!("{}", describe(&snapshot));
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

fn report_terminal(snapshot: &TaskSnapshot, output: &OutputOptions) -> Result<()> {
    let json = serde_json::json!({
        "id": snapshot.id,
        "state": slug(snapshot.state),
        "failure": snapshot.failure,
    });
    match snapshot.state {
        TaskState::Succeeded => output.print(&json, || format!("Task {} succeeded", snapshot.id)),
        _ => {
            let message = snapshot
                .failure
                .clone()
                .unwrap_or_else(|| format!("task {}", slug(snapshot.state)));
            Err(Error::Download(message))
        }
    }
}

fn matches_filter(filter: &str, state: TaskState) -> bool {
    match filter {
        "all" => true,
        // "running" covers paused work too: the TUI groups them the same way.
        "running" => matches!(state, TaskState::Running | TaskState::Paused),
        "done" => state == TaskState::Succeeded,
        "failed" => matches!(state, TaskState::Failed | TaskState::Cancelled),
        other => slug(state) == other,
    }
}

fn slug(state: TaskState) -> &'static str {
    match state {
        TaskState::Queued => "queued",
        TaskState::Running => "running",
        TaskState::Paused => "paused",
        TaskState::Succeeded => "succeeded",
        TaskState::Failed => "failed",
        TaskState::Cancelled => "cancelled",
    }
}

fn describe(snapshot: &TaskSnapshot) -> String {
    match snapshot.total {
        Some(total) if total > 0 => format!(
            "{}  {}  {}/{} ({}%)",
            snapshot.id,
            snapshot.label,
            snapshot.completed,
            total,
            snapshot.completed.min(total) * 100 / total
        ),
        _ => format!(
            "{}  {}  {}",
            snapshot.id, snapshot.label, snapshot.completed
        ),
    }
}

fn format_tasks(tasks: &[TaskSnapshot]) -> String {
    if tasks.is_empty() {
        return "No tasks.".to_owned();
    }
    tasks
        .iter()
        .map(|task| format!("{}  {:<12}  {}", task.id, slug(task.state), task.label))
        .collect::<Vec<_>>()
        .join("\n")
}
