use std::future::Future;
use std::pin::Pin;

use crate::core::error::{Error, Result};
use crate::core::java::runtime::install_runtime_with_snapshot;
use crate::core::loader::service::{InstallLoader, InstallRequest, install_with_control};
use crate::core::resource::install::{
    download_modpack, install_modpack_files, install_resource, save_world,
};
use crate::core::version::http::HttpClient;

use super::context::TaskContext;
use super::model::{InstanceInstallSpec, LoaderSpec, TaskOperation};

pub type TaskFuture = Pin<Box<dyn Future<Output = Result<()>> + Send + 'static>>;

pub trait TaskRunner: Send + Sync {
    fn run(&self, operation: TaskOperation, context: TaskContext) -> TaskFuture;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MinecraftTaskRunner;

impl TaskRunner for MinecraftTaskRunner {
    fn run(&self, operation: TaskOperation, context: TaskContext) -> TaskFuture {
        Box::pin(async move {
            match operation {
                TaskOperation::InstanceInstall(spec) => run_instance_install(spec, context).await,
                TaskOperation::InstanceRepair(spec) => run_instance_repair(spec, context).await,
                TaskOperation::JavaRuntime(spec) => run_java_runtime(spec, context).await,
                TaskOperation::ResourceInstall(spec) => install_resource(spec, context).await,
                TaskOperation::ResourceSaveAs(spec) => save_world(spec, context).await,
                TaskOperation::ModpackInstall(spec) => download_modpack(spec, context).await,
                TaskOperation::ModpackFiles(spec) => install_modpack_files(spec, context).await,
                TaskOperation::Unrecoverable { reason, .. } => Err(Error::Download(format!(
                    "cannot execute migrated task: {reason}"
                ))),
            }
        })
    }
}

pub fn install_loader(spec: &LoaderSpec) -> InstallLoader {
    match spec {
        LoaderSpec::Vanilla => InstallLoader::Vanilla,
        LoaderSpec::Fabric { version } => InstallLoader::Fabric {
            version: version.clone(),
        },
        LoaderSpec::Quilt { version } => InstallLoader::Quilt {
            version: version.clone(),
        },
        LoaderSpec::Forge { version } => InstallLoader::Forge {
            version: version.clone(),
        },
        LoaderSpec::NeoForge { version } => InstallLoader::NeoForge {
            version: version.clone(),
        },
    }
}

pub fn task_loader(loader: &InstallLoader) -> LoaderSpec {
    match loader {
        InstallLoader::Vanilla => LoaderSpec::Vanilla,
        InstallLoader::Fabric { version } => LoaderSpec::Fabric {
            version: version.clone(),
        },
        InstallLoader::Quilt { version } => LoaderSpec::Quilt {
            version: version.clone(),
        },
        InstallLoader::Forge { version } => LoaderSpec::Forge {
            version: version.clone(),
        },
        InstallLoader::NeoForge { version } => LoaderSpec::NeoForge {
            version: version.clone(),
        },
    }
}

async fn run_instance_install(spec: InstanceInstallSpec, context: TaskContext) -> Result<()> {
    let client = HttpClient::with_timeout(spec.download_snapshot.timeout_secs)?;
    let request = InstallRequest {
        game_directory: spec.game_directory,
        instance_directory: spec.instance_directory,
        instance_name: spec.instance_name,
        minecraft_version: spec.minecraft_version,
        loader: install_loader(&spec.loader),
        download_snapshot: spec.download_snapshot.clone(),
    };
    install_with_control(
        &client,
        &request,
        spec.java_executable.as_deref(),
        context.cancellation_flag(),
        Some(&context),
        |completed, total| context.report_progress(completed, total),
    )
    .await
    .map(|_| ())
}

async fn run_instance_repair(
    spec: super::model::InstanceRepairSpec,
    context: TaskContext,
) -> Result<()> {
    crate::core::instance::repair::repair(
        &spec,
        Some(&context),
        context.cancellation_flag(),
        |completed, total| context.report_progress(completed, total),
    )
    .await
    .map(|_| ())
}

async fn run_java_runtime(spec: super::model::JavaRuntimeSpec, context: TaskContext) -> Result<()> {
    let client = HttpClient::with_timeout(spec.download_snapshot.timeout_secs)?;
    install_runtime_with_snapshot(
        &client,
        &spec.data_root,
        spec.major,
        &spec.download_snapshot,
        context.cancellation_flag(),
        Some(&context),
        |completed, total| context.report_progress(completed, total),
    )
    .await
    .map(|_| ())
}
