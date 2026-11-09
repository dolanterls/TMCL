//! Turning a downloaded modpack archive into a usable instance.
//!
//! The order matters and is the same for every caller: read the pack's metadata
//! and resolve its file list first, and only then touch the disk. A pack that
//! lists CurseForge ids without an available provider must fail *before* an
//! instance directory exists, otherwise a half-built instance is left behind for
//! the user to clean up by hand.
//!
//! `plan` is the read-only half: `--dry-run` stops there.

use std::path::{Path, PathBuf};

use crate::core::config::storage::write_json_atomic;
use crate::core::error::{Error, Result};
use crate::core::instance::models::InstanceConfig;
use crate::core::resource::modpack::{
    ModpackMetadata, ModpackResolution, read_modpack_metadata, resolve_files,
};
use crate::core::resource::provider::ResourceProvider;
use crate::core::task::ModpackFileSpec;

/// What a pack contains, before anything is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModpackPlan {
    pub metadata: ModpackMetadata,
    /// Files still to download, relative to the instance.
    pub files: Vec<ModpackFileSpec>,
    /// Manifest entries that could not be resolved; the install still runs.
    pub unresolved: Vec<String>,
}

/// Reads `archive` and resolves its file list.
///
/// `provider` is only consulted for CurseForge packs, which list project/file
/// ids instead of URLs.
pub async fn plan(archive: &Path, provider: Option<&dyn ResourceProvider>) -> Result<ModpackPlan> {
    let read_path = archive.to_path_buf();
    let metadata = tokio::task::spawn_blocking(move || read_modpack_metadata(&read_path))
        .await
        .map_err(join_failure)??;
    let resolution = resolve_files(metadata.files.clone(), provider).await?;
    Ok(ModpackPlan {
        files: to_specs(&resolution)?,
        unresolved: resolution.unresolved,
        metadata,
    })
}

/// Where a pack should go and what to call the instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallRequest {
    /// The game directory the new instance belongs to.
    pub game_directory: PathBuf,
    /// Instance name, also the folder under `versions/`.
    pub name: String,
    /// Local copy of the pack archive. Removed once extraction succeeded.
    pub archive: PathBuf,
    /// Artwork of the project this pack came from.
    ///
    /// Only the download centre knows it; a CLI install or a locally picked
    /// archive passes `None` and the instance keeps whatever the pack itself
    /// shipped.
    pub icon_url: Option<String>,
}

/// Everything the caller needs to finish the install.
#[derive(Debug, Clone)]
pub struct PreparedModpack {
    pub instance: InstanceConfig,
    pub metadata: ModpackMetadata,
    /// Files still to download, relative to the instance.
    pub files: Vec<ModpackFileSpec>,
    pub unresolved: Vec<String>,
}

/// Reads `request.archive` and builds the instance around it.
///
/// This is the only place that touches the network for an icon: a `--dry-run`
/// stops in [`plan`] and never sends a request, and a failed icon never turns
/// a successful install into a failed one.
pub async fn prepare(
    request: InstallRequest,
    provider: Option<&dyn ResourceProvider>,
) -> Result<PreparedModpack> {
    let ModpackPlan {
        metadata,
        files,
        unresolved,
    } = plan(&request.archive, provider).await?;
    let archive = request.archive.clone();
    let game_directory = request.game_directory.clone();
    let name = request.name.clone();
    let icon_url = request.icon_url.clone();
    let instance_files = files.clone();
    let minecraft_version = metadata.minecraft_version.clone();
    let loader = metadata.loader.clone();
    let loader_version = metadata.loader_version.clone();
    let instance = tokio::task::spawn_blocking(move || {
        build_instance(
            &game_directory,
            &name,
            &minecraft_version,
            &loader,
            &loader_version,
            &archive,
            &instance_files,
        )
    })
    .await
    .map_err(join_failure)??;

    if let Some(url) = icon_url.as_deref() {
        // The instance exists and is complete at this point, so a slow icon
        // host delays the reply, never the instance. Bounded by `ICON_TIMEOUT`.
        seed_icon(&instance.path, url).await;
    }

    Ok(PreparedModpack {
        instance,
        metadata,
        files,
        unresolved,
    })
}

/// Applies a pack archive to an instance that already exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateRequest {
    /// Metadata of the instance being updated, for the compatibility check.
    pub instance: InstanceConfig,
    pub archive: PathBuf,
}

#[derive(Debug, Clone)]
pub struct PreparedUpdate {
    /// Version label written to the manifest: the archive's file stem.
    pub version: String,
    pub files: Vec<ModpackFileSpec>,
    pub metadata: ModpackMetadata,
    pub unresolved: Vec<String>,
}

/// Reads the new archive and refuses it before anything is touched when the
/// pack targets another Minecraft version or loader.
pub async fn prepare_update(
    request: UpdateRequest,
    provider: Option<&dyn ResourceProvider>,
) -> Result<PreparedUpdate> {
    let plan = plan(&request.archive, provider).await?;
    crate::core::resource::modpack_update::ensure_compatible(&request.instance, &plan.metadata)?;
    Ok(PreparedUpdate {
        version: request
            .archive
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned(),
        files: plan.files,
        metadata: plan.metadata,
        unresolved: plan.unresolved,
    })
}

/// The files a resolution still has to download.
pub fn to_specs(resolution: &ModpackResolution) -> Result<Vec<ModpackFileSpec>> {
    resolution
        .files
        .iter()
        .map(|file| {
            let path = file.path.clone().ok_or_else(|| {
                Error::Download("a resolved modpack file has no destination".into())
            })?;
            Ok(ModpackFileSpec {
                path,
                urls: file.urls.clone(),
                sha1: file.sha1.clone(),
                size: file.size,
            })
        })
        .collect()
}

fn build_instance(
    game_directory: &Path,
    name: &str,
    minecraft_version: &str,
    loader: &str,
    loader_version: &str,
    archive: &Path,
    files: &[ModpackFileSpec],
) -> Result<InstanceConfig> {
    let (loader, _) = crate::core::loader::parse_install_loader(loader, loader_version)?;
    let id = crate::core::instance::path::game_directory_id(game_directory);
    let mut instance = crate::core::instance::service::create_instance(
        game_directory,
        id,
        name,
        minecraft_version,
    )?;
    instance.loader = loader;
    write_json_atomic(&instance.path.join("tmcl.json"), &instance)?;
    let overrides = match crate::core::resource::install::extract_modpack_archive(
        archive,
        &crate::core::resource::compatibility::content_root(&instance),
    ) {
        Ok(overrides) => overrides,
        Err(error) => {
            // The instance is ours and unusable: remove it rather than leave a
            // folder that looks installed but has no files.
            let _ = std::fs::remove_dir_all(&instance.path);
            return Err(error);
        }
    };
    // Record which files the pack owns. Without this an update cannot tell a
    // pack file from one the player added.
    let version = archive
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_owned();
    crate::core::resource::modpack_update::write_manifest(
        &instance.path,
        &crate::core::resource::modpack_update::PackManifest {
            version,
            files: files.iter().map(|file| file.path.clone()).collect(),
            overrides,
        },
    )?;
    let _ = std::fs::remove_file(archive);
    Ok(instance)
}

fn join_failure(error: tokio::task::JoinError) -> Error {
    Error::Download(format!("modpack preparation task failed: {error}"))
}

/// How long the artwork may take before the install moves on without it.
///
/// The default client waits a minute, which is fine for a jar and far too long
/// for a decoration.
const ICON_TIMEOUT_SECS: u64 = 15;

/// Largest body accepted for an icon. Real artwork is a few hundred kilobytes;
/// the cap only stops a wrong URL from streaming something else into the
/// instance.
const MAX_ICON_BYTES: u64 = 8 * 1024 * 1024;

/// Downloads the pack's artwork into `<instance>/icon.png`.
///
/// Best effort by contract: a non-PNG URL, a 404, a body that does not decode. Each
/// is logged and dropped, because a cover is decoration and must never fail an
/// install that otherwise worked.
async fn seed_icon(instance: &Path, url: &str) {
    if let Err(error) = fetch_icon(instance, url).await {
        crate::core::diagnostics::diagnostic!("modpack", "icon {url}: {error}");
    }
}

async fn fetch_icon(instance: &Path, url: &str) -> Result<()> {
    if !is_png_url(url) {
        return Err(Error::Cover(format!("{url}: not a .png URL")));
    }
    let client = crate::core::version::http::HttpClient::with_timeout(ICON_TIMEOUT_SECS)?;
    let mut response = client.get_response(url).await?;
    if let Some(length) = response.content_length()
        && length > MAX_ICON_BYTES
    {
        return Err(Error::Cover(format!("{url}: {length} bytes is too large")));
    }
    // Chunked, not `bytes()`: without a `Content-Length` the cap below would
    // only be consulted after the whole body had already been buffered.
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| Error::Network(format!("read {url}: {error}")))?
    {
        if u64::try_from(body.len() + chunk.len()).unwrap_or(u64::MAX) > MAX_ICON_BYTES {
            return Err(Error::Cover(format!("{url}: body is too large")));
        }
        body.extend_from_slice(&chunk);
    }

    // Written beside the target and published only once it decodes: a truncated
    // or non-PNG body would otherwise become an `icon.png` that `candidates()`
    // happily reports and that shadows every world screenshot for good.
    let temporary = instance.join("icon.png.part");
    std::fs::write(&temporary, &body)?;
    if let Err(error) = crate::core::cover::Cover::load(&temporary) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    if let Err(error) = std::fs::rename(&temporary, instance.join("icon.png")) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(())
}

/// Whether `url` names a PNG file.
///
/// The extension is read from the path segment only: project artwork is served
/// as `…/hash.png`, but the same host may append a cache buster, and
/// `ends_with(".png")` would then skip a perfectly good icon.
fn is_png_url(url: &str) -> bool {
    let path = url.split(['?', '#']).next().unwrap_or_default();
    path.rsplit('.')
        .next()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::{InstallRequest, is_png_url, plan, prepare};
    use crate::core::resource::modpack_update::read_manifest;

    /// A one-file Modrinth-format pack pointing at `url`.
    fn write_pack(path: &std::path::Path, url: &str) {
        let index = format!(
            r#"{{"formatVersion":1,"game":"minecraft","versionId":"1.0.0","name":"Demo","dependencies":{{"minecraft":"1.21.1","fabric-loader":"0.16.5"}},"files":[{{"path":"mods/demo.jar","hashes":{{}},"downloads":["{url}"],"fileSize":4}}]}}"#
        );
        let file = std::fs::File::create(path).expect("create pack");
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("modrinth.index.json", options)
            .expect("index");
        zip.write_all(index.as_bytes()).expect("write index");
        zip.finish().expect("finish");
    }

    fn root(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("tmcl-pack-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("game")).expect("game");
        root
    }

    /// A one-pixel PNG of `rgba`, the smallest thing `Cover::load` accepts.
    fn png_bytes(rgba: [u8; 4]) -> Vec<u8> {
        let mut buffer = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut buffer, 1, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("png header");
            writer.write_image_data(&rgba).expect("png data");
            writer.finish().expect("finish png");
        }
        buffer
    }

    /// A pack that ships its own `icon.png` in `overrides/`, the way a pack
    /// with its own artwork does.
    fn write_pack_with_icon(path: &std::path::Path, url: &str, rgba: [u8; 4]) {
        let index = format!(
            r#"{{"formatVersion":1,"game":"minecraft","versionId":"1.0.0","name":"Demo","dependencies":{{"minecraft":"1.21.1","fabric-loader":"0.16.5"}},"files":[{{"path":"mods/demo.jar","hashes":{{}},"downloads":["{url}"],"fileSize":4}}]}}"#
        );
        let file = std::fs::File::create(path).expect("create pack");
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("modrinth.index.json", options)
            .expect("index");
        zip.write_all(index.as_bytes()).expect("write index");
        zip.start_file("overrides/icon.png", options)
            .expect("override icon");
        zip.write_all(&png_bytes(rgba)).expect("write icon");
        zip.finish().expect("finish");
    }

    /// Serves one response on `127.0.0.1` and hands back the URL to ask for.
    ///
    /// The path ends in `.png` because the installer checks it before opening a
    /// connection; the server itself ignores it.
    fn serve(status: u16, body: Vec<u8>) -> (String, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        let address = listener.local_addr().expect("address");
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("connection");
            // Read the request so the response cannot arrive mid-header.
            let mut buffer = [0u8; 2048];
            let _ = stream.read(&mut buffer);
            let reason = if status == 200 { "OK" } else { "Not Found" };
            let head = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(head.as_bytes()).expect("headers");
            stream.write_all(&body).expect("body");
            stream.flush().expect("flush");
        });
        (format!("http://{address}/art/icon.png"), handle)
    }

    fn cover_pixel(instance: &std::path::Path) -> Option<[u8; 4]> {
        crate::core::cover::Cover::load(&instance.join("icon.png"))
            .ok()
            .map(|cover| cover.pixel(0, 0))
    }

    #[tokio::test]
    async fn a_plan_lists_the_files_without_writing_anything() {
        let root = root("plan");
        let archive = root.join("demo-1.0.0.mrpack");
        write_pack(&archive, "http://example/demo.jar");

        let plan = plan(&archive, None).await.expect("plan");

        assert_eq!(plan.files.len(), 1);
        assert_eq!(plan.files[0].path, "mods/demo.jar");
        assert_eq!(plan.metadata.minecraft_version, "1.21.1");
        assert_eq!(plan.metadata.loader, "fabric");
        // Nothing was created: planning is read-only.
        assert!(!root.join("game").join("versions").exists());
        assert!(archive.is_file(), "planning does not consume the archive");
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[tokio::test]
    async fn a_pack_becomes_an_instance_with_a_manifest() {
        let root = root("install");
        let game = root.join("game");
        let archive = root.join("demo-1.0.0.mrpack");
        write_pack(&archive, "http://example/demo.jar");

        let prepared = prepare(
            InstallRequest {
                game_directory: game.clone(),
                name: "packed".into(),
                archive: archive.clone(),
                icon_url: None,
            },
            None,
        )
        .await
        .expect("prepare");

        assert_eq!(prepared.instance.name, "packed");
        assert_eq!(prepared.instance.minecraft_version, "1.21.1");
        assert_eq!(
            prepared.instance.loader,
            crate::core::instance::models::InstanceLoader::Fabric {
                version: "0.16.5".into()
            }
        );
        assert_eq!(prepared.files.len(), 1);
        assert!(prepared.instance.path.join("tmcl.json").is_file());

        let manifest = read_manifest(&prepared.instance.path).expect("manifest");
        assert_eq!(manifest.version, "demo-1.0.0");
        assert_eq!(manifest.files, ["mods/demo.jar"]);
        // The staging archive is consumed; the caller owns the original.
        assert!(!archive.exists());
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[tokio::test]
    async fn a_pack_that_needs_a_provider_without_one_leaves_no_instance() {
        let root = root("cf");
        let game = root.join("game");
        let archive = root.join("cf.zip");
        // CurseForge manifest: files are ids, not URLs.
        let manifest = r#"{"minecraft":{"version":"1.21.1","modLoaders":[{"id":"fabric-0.16.5","primary":true}]},"files":[{"projectID":123,"fileID":456,"required":true}],"manifestType":"minecraftModpack","manifestVersion":1,"name":"CF","version":"1.0"}"#;
        let file = std::fs::File::create(&archive).expect("create");
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("manifest.json", zip::write::SimpleFileOptions::default())
            .expect("entry");
        zip.write_all(manifest.as_bytes()).expect("write");
        zip.finish().expect("finish");

        let error = prepare(
            InstallRequest {
                game_directory: game.clone(),
                name: "cfpack".into(),
                archive,
                icon_url: None,
            },
            None,
        )
        .await
        .expect_err("needs CurseForge");

        assert!(error.to_string().contains("CurseForge"), "{error}");
        assert!(
            !game.join("versions").join("cfpack").exists(),
            "no half-built instance"
        );
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[tokio::test]
    async fn the_project_artwork_becomes_the_instance_cover() {
        let root = root("icon");
        let game = root.join("game");
        let archive = root.join("demo-1.0.0.mrpack");
        write_pack(&archive, "http://example/demo.jar");
        let (url, server) = serve(200, png_bytes([10, 20, 30, 255]));

        let prepared = prepare(
            InstallRequest {
                game_directory: game.clone(),
                name: "artwork".into(),
                archive,
                icon_url: Some(url),
            },
            None,
        )
        .await
        .expect("prepare");
        server.join().expect("server");

        assert_eq!(
            cover_pixel(&prepared.instance.path),
            Some([10, 20, 30, 255]),
            "the pack's artwork must land as icon.png"
        );
        assert!(
            !prepared.instance.path.join("icon.png.part").exists(),
            "the temporary file must not survive a successful download"
        );
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[tokio::test]
    async fn the_downloaded_artwork_replaces_the_pack_icon() {
        let root = root("icon-override");
        let game = root.join("game");
        let archive = root.join("demo-1.0.0.mrpack");
        // The pack ships a red icon; the project page shows a blue one. The
        // project wins: it is the picture the player picked the pack by.
        write_pack_with_icon(&archive, "http://example/demo.jar", [200, 0, 0, 255]);
        let (url, server) = serve(200, png_bytes([0, 0, 200, 255]));

        let prepared = prepare(
            InstallRequest {
                game_directory: game.clone(),
                name: "override".into(),
                archive,
                icon_url: Some(url),
            },
            None,
        )
        .await
        .expect("prepare");
        server.join().expect("server");

        assert_eq!(cover_pixel(&prepared.instance.path), Some([0, 0, 200, 255]));
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[tokio::test]
    async fn a_broken_icon_link_leaves_the_install_alone() {
        let root = root("icon-404");
        let game = root.join("game");
        let archive = root.join("demo-1.0.0.mrpack");
        write_pack(&archive, "http://example/demo.jar");
        let (url, server) = serve(404, Vec::new());

        let prepared = prepare(
            InstallRequest {
                game_directory: game.clone(),
                name: "broken".into(),
                archive,
                icon_url: Some(url),
            },
            None,
        )
        .await
        .expect("a failed cover must not fail the install");
        server.join().expect("server");

        assert!(prepared.instance.path.join("tmcl.json").is_file());
        assert_eq!(cover_pixel(&prepared.instance.path), None);
        std::fs::remove_dir_all(root).expect("clean");
    }

    /// A 200 that is not a picture is the nastiest case: writing it through
    /// would leave an `icon.png` that `candidates()` reports as a cover and that
    /// hides every world screenshot behind a decode failure.
    #[tokio::test]
    async fn a_body_that_does_not_decode_never_becomes_an_icon() {
        let root = root("icon-html");
        let game = root.join("game");
        let archive = root.join("demo-1.0.0.mrpack");
        write_pack(&archive, "http://example/demo.jar");
        let (url, server) = serve(200, b"<html>not a picture</html>".to_vec());

        let prepared = prepare(
            InstallRequest {
                game_directory: game.clone(),
                name: "html".into(),
                archive,
                icon_url: Some(url),
            },
            None,
        )
        .await
        .expect("prepare");
        server.join().expect("server");

        assert_eq!(cover_pixel(&prepared.instance.path), None);
        assert!(
            !prepared.instance.path.join("icon.png.part").exists(),
            "a rejected body must not leave its temporary file behind"
        );
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[tokio::test]
    async fn a_non_png_url_is_never_requested() {
        use std::net::TcpListener;

        let root = root("icon-jpeg");
        let game = root.join("game");
        let archive = root.join("demo-1.0.0.mrpack");
        write_pack(&archive, "http://example/demo.jar");
        // A listener nobody answers for: if the installer opened a connection
        // the request would be sitting in the accept queue below.
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        let port = listener.local_addr().expect("address").port();

        let prepared = prepare(
            InstallRequest {
                game_directory: game.clone(),
                name: "jpeg".into(),
                archive,
                icon_url: Some(format!("http://127.0.0.1:{port}/art/cover.jpg")),
            },
            None,
        )
        .await
        .expect("prepare");

        listener.set_nonblocking(true).expect("nonblocking");
        assert!(
            listener.accept().is_err(),
            "a .jpg URL must be refused before any request"
        );
        assert_eq!(cover_pixel(&prepared.instance.path), None);
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn only_png_paths_are_considered() {
        assert!(is_png_url("https://cdn.modrinth.com/data/x/hash.png"));
        assert!(is_png_url("https://cdn.modrinth.com/data/x/hash.PNG"));
        // Cache busters and fragments are not part of the file name.
        assert!(is_png_url("https://cdn.modrinth.com/data/x/hash.png?v=2"));
        assert!(is_png_url("https://cdn.modrinth.com/data/x/hash.png#top"));
        assert!(!is_png_url("https://cdn.modrinth.com/data/x/hash.jpg"));
        assert!(!is_png_url("https://cdn.modrinth.com/data/x/hash"));
        assert!(!is_png_url("https://cdn.modrinth.com"));
        assert!(!is_png_url(""));
    }
}
