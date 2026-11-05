use crate::cli::args::VersionCommand;
use crate::cli::output::OutputOptions;
use crate::core::config::paths::DataPaths;
use crate::core::download::DownloadSource;
use crate::core::error::Result;
use crate::core::version::cache::{VersionCache, list_versions};
use crate::core::version::http::HttpClient;
use crate::core::version::models::VersionSummary;

pub(super) async fn run(command: VersionCommand, output: OutputOptions) -> Result<()> {
    match command {
        VersionCommand::List(args) => {
            let paths = DataPaths::discover()?;
            paths.ensure_dirs()?;
            let catalog = list_versions(
                &HttpClient::new()?,
                &[DownloadSource::Official, DownloadSource::Bmclapi],
                &VersionCache::new(paths.cache_dir.join("version_manifest_v2.json")),
            )
            .await?;
            let versions = catalog
                .manifest
                .versions
                .into_iter()
                .filter(|version| args.kind == "all" || version.kind == args.kind)
                .collect::<Vec<_>>();
            let result = VersionList {
                kind: args.kind,
                from_cache: catalog.from_cache,
                versions,
            };
            output.print(&result, || {
                result
                    .versions
                    .iter()
                    .map(|version| format!("{}  {}", version.id, version.kind))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
        }
    }
}

#[derive(Debug, serde::Serialize)]
struct VersionList {
    kind: String,
    from_cache: bool,
    versions: Vec<VersionSummary>,
}
