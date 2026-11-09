//! Preparing a launch.
//!
//! Both front ends go through here: pick Java, resolve memory, refresh the session
//! token, unpack natives, build the command line. Keeping the sequence in one place
//! is what stops the CLI and the TUI from drifting, which they did while each
//! assembled its own copy.
//!
//! What stays outside: authlib injection (it depends on the process lifetime)
//! and starting the JVM (that is the front end's decision about waiting).

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

use uuid::Uuid;

use crate::core::account::microsoft::oauth::MicrosoftOAuth;
use crate::core::account::microsoft::session::{LaunchToken, launch_token};
use crate::core::account::models::{Account, AccountKind};
use crate::core::account::offline::create_account;
use crate::core::account::secrets::SecretStore;
use crate::core::account::store::AccountStore;
use crate::core::config::models::{Config, MemoryMode};
use crate::core::config::paths::DataPaths;
use crate::core::error::{Error, Result};
use crate::core::instance::models::InstanceConfig;
use crate::core::instance::overrides::{EffectiveLaunch, JavaChoice, LaunchOverrides};
use crate::core::instance::resolve::ResolvedInstance;
use crate::core::java::scan::{discover_default, discover_one};
use crate::core::java::{JavaInstallation, minimum_java_major, select_java_preferring};
use crate::core::launch::command::{self, CommandSpec, LaunchOptions};
use crate::core::launch::{memory, natives, preflight};
use crate::core::loader::client_json::{self, ClientJson};
use crate::core::version::http::HttpClient;

/// How far along a launch is. Emitted before the work starts, so a slow step is
/// explainable while it runs rather than after it finishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchStage {
    ResolvingAccount,
    SelectingJava,
    DownloadingJava { major: u32 },
    PreparingNatives,
    BuildingCommand,
    VerifyingFiles,
}

pub struct LaunchRequest {
    pub instance: ResolvedInstance,
    /// `None` falls back to the configured current account, then a local
    /// offline account.
    pub account_id: Option<Uuid>,
    /// Runtimes the caller already knows about. `None` makes `prepare` scan.
    pub java_installations: Option<Vec<JavaInstallation>>,
    /// An explicitly chosen runtime, probed by `prepare`.
    pub java_override: Option<PathBuf>,
    /// Whether a missing runtime may be downloaded.
    ///
    /// The CLI passes `config.java.auto_download`; the TUI passes `false` and
    /// keeps its existing behaviour.
    pub allow_java_download: bool,
    /// Version JSON to use instead of `<instance>/<name>.json`.
    pub client_json: Option<PathBuf>,
    pub min_memory_mb: Option<u32>,
    pub max_memory_mb: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fullscreen: Option<bool>,
}

#[derive(Debug)]
pub struct PreparedLaunch {
    pub spec: CommandSpec,
    pub log_path: PathBuf,
    pub account: Account,
    pub token: String,
    pub java: JavaInstallation,
    pub game_directory: PathBuf,
    pub client: ClientJson,
}

/// Everything `build_command` needs. A struct rather than twelve positional
/// arguments, which is what the previous two implementations looked like.
pub struct CommandRequest<'a> {
    pub instance_directory: &'a Path,
    pub instance_name: &'a str,
    pub minecraft_version: &'a str,
    pub game_directory: &'a Path,
    pub config: &'a Config,
    pub overrides: &'a LaunchOverrides,
    pub java_installations: &'a [JavaInstallation],
    pub java_override: Option<&'a Path>,
    pub account: &'a Account,
    pub token: &'a str,
    pub client: &'a ClientJson,
    pub logs_dir: &'a Path,
    pub min_memory_mb: Option<u32>,
    pub max_memory_mb: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fullscreen: Option<bool>,
}

/// Resolve the account, refresh its token when needed, and build the command.
pub async fn prepare<F>(
    request: LaunchRequest,
    paths: &DataPaths,
    config: &Config,
    mut on_stage: F,
) -> Result<PreparedLaunch>
where
    F: FnMut(LaunchStage),
{
    on_stage(LaunchStage::ResolvingAccount);
    let mut account = resolve_account(request.account_id, paths, config)?;
    let token = match account.kind {
        AccountKind::Offline => "0".to_owned(),
        AccountKind::Microsoft => refresh_microsoft_token(paths, &mut account).await?,
    };

    let instance = &request.instance;
    let client = load_client(instance, request.client_json.as_deref())?;

    let required = required_java_major(&client, &instance.config.minecraft_version)?;
    let installations = match request.java_installations.clone() {
        Some(list) => list,
        None => discover_default(
            &paths.runtime_dir,
            config.game_directories.clone(),
            config.java.selected_path.clone(),
        ),
    };

    on_stage(LaunchStage::SelectingJava);
    let effective = EffectiveLaunch::resolve(config, &instance.config.overrides);
    let java = match select_java(
        &installations,
        request.java_override.as_deref(),
        &effective,
        required,
        config,
        &instance.config.minecraft_version,
    ) {
        Ok(java) => java,
        Err(_error) if request.allow_java_download && request.java_override.is_none() => {
            on_stage(LaunchStage::DownloadingJava { major: required });
            download_java(paths, config, required).await?
        }
        Err(error) => return Err(error),
    };

    let (spec, log_path) = assemble(
        CommandRequest {
            instance_directory: &instance.config.path,
            instance_name: &instance.config.name,
            minecraft_version: &instance.config.minecraft_version,
            game_directory: &instance.game_directory,
            config,
            overrides: &instance.config.overrides,
            java_installations: &installations,
            java_override: request.java_override.as_deref(),
            account: &account,
            token: &token,
            client: &client,
            logs_dir: &paths.game_logs_dir,
            min_memory_mb: request.min_memory_mb,
            max_memory_mb: request.max_memory_mb,
            width: request.width,
            height: request.height,
            fullscreen: request.fullscreen,
        },
        &java,
        &mut on_stage,
    )?;

    Ok(PreparedLaunch {
        spec,
        log_path,
        account,
        token,
        java,
        game_directory: instance.game_directory.clone(),
        client,
    })
}

/// Build the command line for an already-known account and token.
///
/// Used by "export launch script", which must work without refreshing a token
/// (it substitutes a placeholder) and without the launch-time checks that
/// `prepare` performs.
pub fn build_command(request: CommandRequest<'_>) -> Result<(CommandSpec, PathBuf)> {
    let effective = EffectiveLaunch::resolve(request.config, request.overrides);
    let required = required_java_major(request.client, request.minecraft_version)?;
    let java = select_java(
        request.java_installations,
        request.java_override,
        &effective,
        required,
        request.config,
        request.minecraft_version,
    )?;
    assemble(request, &java, &mut |_| {})
}

fn assemble(
    request: CommandRequest<'_>,
    java: &JavaInstallation,
    on_stage: &mut dyn FnMut(LaunchStage),
) -> Result<(CommandSpec, PathBuf)> {
    let effective = EffectiveLaunch::resolve(request.config, request.overrides);
    let libraries = request
        .client
        .classpath_libraries(&command::launch_features())?;
    // A number on the command line means manual memory, exactly like the CLI's
    // `--min-memory` did: otherwise the automatic recommendation would silently
    // replace the value the user typed.
    let manual_memory = request.min_memory_mb.is_some() || request.max_memory_mb.is_some();
    let memory = memory::resolve(
        if manual_memory {
            MemoryMode::Manual
        } else {
            effective.memory_mode
        },
        request.min_memory_mb.unwrap_or(effective.min_memory_mb),
        request.max_memory_mb.unwrap_or(effective.max_memory_mb),
        memory::detect(),
    )?;

    on_stage(LaunchStage::PreparingNatives);
    // Legacy versions (LWJGL 2) load natives from `java.library.path`;
    // `command::build` picks `.tmcl-natives` as soon as it exists.
    natives::prepare(
        request.instance_directory,
        request.game_directory,
        request.client,
    )?;

    on_stage(LaunchStage::BuildingCommand);
    let spec = command::build(
        request.instance_directory,
        request.game_directory,
        java,
        request.account,
        request.client,
        &libraries,
        request.token,
        &LaunchOptions {
            min_memory_mb: memory.minimum_mb,
            max_memory_mb: memory.maximum_mb,
            width: request.width.unwrap_or(effective.width),
            height: request.height.unwrap_or(effective.height),
            fullscreen: request.fullscreen.unwrap_or(effective.fullscreen),
            jvm_args: effective.jvm_args.clone(),
            extra_game_args: effective.game_args.clone(),
            server_address: effective.server_address.clone(),
        },
    )?;

    on_stage(LaunchStage::VerifyingFiles);
    let missing = preflight::missing_files(&spec);
    if !missing.is_empty() {
        return Err(Error::Launch(missing_files_message(&missing)));
    }

    let log_path = launch_log_path(request.logs_dir, request.instance_name, now_secs());
    Ok((spec, log_path))
}

/// The file this launch writes its captured output to.
///
/// The instance name plus a UTC stamp identifies a run by eye. A bare UUID does
/// not: neither the diagnosis bundle nor "open the log" can say which run a file
/// belonged to. The stamp is UTC because a local-time conversion would need a
/// time-zone database for one line of a file name.
fn launch_log_path(logs_dir: &Path, instance_name: &str, now_secs: u64) -> PathBuf {
    let base = format!("{}-{}", safe_file_stem(instance_name), utc_stamp(now_secs));
    let candidate = logs_dir.join(format!("{base}.log"));
    if !candidate.exists() {
        return candidate;
    }
    for suffix in 2..=9 {
        let path = logs_dir.join(format!("{base}-{suffix}.log"));
        if !path.exists() {
            return path;
        }
    }
    logs_dir.join(format!("{}.log", Uuid::new_v4()))
}

/// Characters Windows refuses in a file name, plus whitespace, become `_`.
///
/// `pub(crate)` because `tmcl diagnose` has to derive the same prefix when it
/// looks for an instance's log: the name on disk and the name the search
/// computes must come from one function, or a rename breaks the lookup.
pub(crate) fn safe_file_stem(name: &str) -> String {
    let mut out = String::new();
    for character in name.chars().take(40) {
        if character.is_control() || character.is_whitespace() || "\\/:*?\"<>|".contains(character)
        {
            out.push('_');
        } else {
            out.push(character);
        }
    }
    if out.trim_matches('_').is_empty() {
        "instance".to_owned()
    } else {
        out
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

/// `yyyymmdd-HHMMSS` in UTC.
pub(crate) fn utc_stamp(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let rest = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        rest / 3600,
        (rest / 60) % 60,
        rest % 60
    )
}

/// Howard Hinnant's `civil_from_days`, the standard way to turn a day count
/// into a date without pulling in a calendar crate.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (year + i64::from(month <= 2), month, day)
}

fn required_java_major(client: &ClientJson, minecraft_version: &str) -> Result<u32> {
    match client.java_version.as_ref() {
        Some(version) => Ok(version.major_version),
        None => minimum_java_major(minecraft_version),
    }
}

fn select_java(
    installations: &[JavaInstallation],
    java_override: Option<&Path>,
    effective: &EffectiveLaunch,
    required: u32,
    config: &Config,
    minecraft_version: &str,
) -> Result<JavaInstallation> {
    if let Some(path) = java_override {
        return discover_one(path);
    }
    match &effective.java {
        JavaChoice::Fixed(path) => installations
            .iter()
            .find(|java| &java.executable == path && java.major >= required)
            .cloned()
            .ok_or_else(|| {
                Error::Java(format!(
                    "the selected Java runtime is missing or does not satisfy Minecraft {minecraft_version} (needs Java {required}+): {}",
                    path.display()
                ))
            }),
        JavaChoice::Auto => {
            select_java_preferring(installations, required, config.java.selected_path.as_deref())
                .cloned()
                .ok_or_else(|| java_missing(required, installations, minecraft_version))
        }
    }
}

fn java_missing(
    required: u32,
    installations: &[JavaInstallation],
    minecraft_version: &str,
) -> Error {
    let detected = installations
        .iter()
        .map(|java| java.major.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    if detected.is_empty() {
        Error::Java(format!(
            "Minecraft {minecraft_version} requires Java {required}+, but no Java runtime was detected"
        ))
    } else {
        Error::Java(format!(
            "Minecraft {minecraft_version} requires Java {required}+; detected Java major version(s): {detected}"
        ))
    }
}

fn missing_files_message(missing: &[PathBuf]) -> String {
    let listed = missing
        .iter()
        .take(3)
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let remaining = missing.len().saturating_sub(3);
    let suffix = if remaining > 0 {
        format!(" (+{remaining} more)")
    } else {
        String::new()
    };
    format!(
        "{} files required by this instance are missing: {listed}{suffix}; verify or re-download the instance",
        missing.len()
    )
}

/// The version JSON a launch reads, inheriting from its parent when needed.
///
/// `client_json` wins over `<instance>/<name>.json` so a caller can test a
/// version file before it is installed.
pub fn load_client(instance: &ResolvedInstance, client_json: Option<&Path>) -> Result<ClientJson> {
    let client_path = client_json.map(Path::to_path_buf).unwrap_or_else(|| {
        instance
            .config
            .path
            .join(format!("{}.json", instance.config.name))
    });
    client_json::parse(&std::fs::read(&client_path)?)?.resolve_inheritance(&instance.game_directory)
}

/// Account a launch uses: the explicit id, else the configured current one,
/// else a local offline account.
pub fn resolve_account(
    account_id: Option<Uuid>,
    paths: &DataPaths,
    config: &Config,
) -> Result<Account> {
    let id = account_id.or_else(|| {
        config
            .state
            .current_account_id
            .as_deref()
            .and_then(|value| Uuid::parse_str(value).ok())
    });
    match id {
        Some(id) => AccountStore::new(paths.accounts_file.clone())
            .load()?
            .into_iter()
            .find(|account| account.id == id)
            .ok_or_else(|| Error::Account(format!("account not found: {id}"))),
        None => create_account("Player", None),
    }
}

async fn refresh_microsoft_token(paths: &DataPaths, account: &mut Account) -> Result<String> {
    let store = SecretStore::new(paths.root.join("secrets"));
    let oauth = MicrosoftOAuth::new(HttpClient::new()?);
    // Reuse the token from the previous launch while it is still valid; the
    // full chain is six requests and Xbox Live rate-limits `login_with_xbox`.
    let token = match launch_token(&oauth, &store, account, unix_seconds()).await {
        Ok(token) => token,
        // A rejected refresh token can never be traded again: revoked, expired,
        // or issued to another client id. Keeping it on disk would leave the
        // account page promising `自动续期` and every launch spending a round trip
        // to be told the same thing, so the secret is retired here. The error still
        // reaches the player, and the row reads `需重新登录`.
        Err(error @ Error::AccountExpired(_)) => {
            retire_credential(&store, account);
            let _ = persist_account(paths, account);
            return Err(error);
        }
        Err(error) => return Err(error),
    };
    match token {
        LaunchToken::Cached(token) => Ok(token),
        LaunchToken::Refreshed(session) => {
            account.username = session.username.clone();
            account.access_expires_at = Some(unix_seconds() + session.expires_in);
            if let Some(refresh_token) = session.refresh_token.as_deref() {
                account.secret_ref = Some(store.save(account.id, refresh_token.as_bytes())?);
            }
            persist_account(paths, account)?;
            Ok(session.access_token.clone())
        }
    }
}

/// Drop a credential that cannot be traded again.
///
/// Both halves matter: the secret file goes, and `secret_ref` stops pointing at
/// it. Clearing only the file leaves a dangling reference and the account keeps
/// claiming it can renew itself.
fn retire_credential(store: &SecretStore, account: &mut Account) {
    if let Some(reference) = account.secret_ref.take() {
        store.clear(&reference);
    }
    account.access_expires_at = None;
}

/// Write this account back into `accounts.json`, keeping the other rows.
fn persist_account(paths: &DataPaths, account: &Account) -> Result<()> {
    let account_store = AccountStore::new(&paths.accounts_file);
    let mut accounts = account_store.load()?;
    if let Some(saved) = accounts.iter_mut().find(|saved| saved.id == account.id) {
        *saved = account.clone();
        account_store.save(&accounts)?;
    }
    Ok(())
}

async fn download_java(paths: &DataPaths, config: &Config, major: u32) -> Result<JavaInstallation> {
    let installed = crate::core::java::runtime::install_runtime_with_snapshot(
        &HttpClient::with_timeout(config.downloads.timeout_secs)?,
        &paths.root,
        major,
        &config.download_snapshot(paths),
        &AtomicBool::new(false),
        None,
        |_, _| Ok(()),
    )
    .await?;
    discover_one(&installed.executable)
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

/// The instance directory, name and version a launch uses, from metadata.
pub fn instance_parts(instance: &InstanceConfig) -> (&Path, &str, &str) {
    (&instance.path, &instance.name, &instance.minecraft_version)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{
        CommandRequest, LaunchRequest, LaunchStage, build_command, prepare, retire_credential,
    };
    use crate::core::account::models::Account;
    use crate::core::account::offline::create_account;
    use crate::core::config::models::Config;
    use crate::core::config::paths::DataPaths;
    use crate::core::instance::models::InstanceConfig;
    use crate::core::instance::overrides::LaunchOverrides;
    use crate::core::instance::resolve::{RefSource, ResolvedInstance};

    /// A credential AAD will not trade again is removed, not retried forever.
    ///
    /// `invalid_grant` is permanent: the token was revoked, expired, or issued
    /// for another client id. Leaving it on disk made the account page promise
    /// `自动续期` for a renewal that could never happen, and spent a network
    /// round trip on every launch to be told so.
    #[test]
    fn a_dead_refresh_token_is_retired_from_the_store() {
        use crate::core::account::secrets::SecretStore;

        let root = std::env::temp_dir().join(format!("tmcl-retire-{}", uuid::Uuid::new_v4()));
        let store = SecretStore::new(root.join("secrets"));
        let id = uuid::Uuid::new_v4();
        let reference = store.save(id, b"dead-token").expect("store token");
        let mut account = Account {
            id,
            kind: crate::core::account::models::AccountKind::Microsoft,
            username: "Alex".into(),
            uuid: uuid::Uuid::new_v4(),
            skin: None,
            secret_ref: Some(reference.clone()),
            access_expires_at: Some(1),
        };

        retire_credential(&store, &mut account);

        assert!(account.secret_ref.is_none(), "the reference must go too");
        assert!(account.access_expires_at.is_none(), "no cached token left");
        assert!(
            store.load(&reference).is_err(),
            "the secret file must be gone, not just unlinked"
        );
        std::fs::remove_dir_all(root).ok();
    }

    use crate::core::java::JavaInstallation;
    use crate::core::loader::client_json;

    const CLIENT: &str = r#"{"id":"1.21.1","mainClass":"net.minecraft.client.main.Main","arguments":{"game":[],"jvm":[]}}"#;

    struct Fixture {
        root: PathBuf,
        logs: PathBuf,
        game: PathBuf,
        instance: ResolvedInstance,
        java: JavaInstallation,
        account: Account,
        config: Config,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    /// An instance whose client jar and Java binary exist, so the pre-launch
    /// check has nothing to complain about.
    fn fixture(tag: &str) -> Fixture {
        let root =
            std::env::temp_dir().join(format!("tmcl-session-{tag}-{}", uuid::Uuid::new_v4()));
        let game = root.join("game");
        let instance_dir = game.join("versions").join("demo");
        std::fs::create_dir_all(&instance_dir).expect("instance dir");
        std::fs::write(instance_dir.join("demo.jar"), b"jar").expect("client jar");
        std::fs::write(instance_dir.join("demo.json"), CLIENT).expect("client json");
        let java_path = root.join("bin").join("java.exe");
        std::fs::create_dir_all(java_path.parent().expect("java dir")).expect("bin");
        std::fs::write(&java_path, b"java").expect("java");

        let config = InstanceConfig::new(&instance_dir, uuid::Uuid::new_v4(), "demo", "1.21.1");
        let logs = root.join("logs");
        Fixture {
            root,
            logs,
            game: game.clone(),
            instance: ResolvedInstance {
                config,
                game_directory: game,
                source: RefSource::Path,
            },
            java: JavaInstallation {
                executable: java_path,
                version: "21.0.1".into(),
                major: 21,
                vendor: Some("test".into()),
                is_jdk: false,
                lts: true,
            },
            account: create_account("Alex", None).expect("offline account"),
            config: Config::default(),
        }
    }

    fn client() -> client_json::ClientJson {
        client_json::parse(CLIENT.as_bytes()).expect("parse client")
    }

    fn data_dir(fixture: &Fixture) -> DataPaths {
        let paths = DataPaths::from_appdata(&fixture.root);
        paths.ensure_dirs().expect("data dirs");
        paths
    }

    fn request<'a>(
        fixture: &'a Fixture,
        client: &'a client_json::ClientJson,
        config: &'a Config,
        account: &'a Account,
        overrides: &'a LaunchOverrides,
    ) -> CommandRequest<'a> {
        CommandRequest {
            instance_directory: &fixture.instance.config.path,
            instance_name: "demo",
            minecraft_version: "1.21.1",
            game_directory: &fixture.game,
            config,
            overrides,
            java_installations: std::slice::from_ref(&fixture.java),
            java_override: None,
            account,
            token: "0",
            client,
            logs_dir: &fixture.logs,
            min_memory_mb: None,
            max_memory_mb: None,
            width: None,
            height: None,
            fullscreen: None,
        }
    }

    #[test]
    fn a_complete_instance_builds_a_command() {
        let fixture = fixture("build");
        let client = client();
        let overrides = LaunchOverrides::default();
        let (spec, log_path) = build_command(request(
            &fixture,
            &client,
            &fixture.config,
            &fixture.account,
            &overrides,
        ))
        .expect("build");

        assert!(spec.args.iter().any(|argument| argument == "--username"));
        assert!(log_path.starts_with(&fixture.logs));
        assert!(log_path.extension().is_some_and(|value| value == "log"));
    }

    #[test]
    fn an_override_that_is_not_a_runtime_is_rejected() {
        let fixture = fixture("override");
        let client = client();
        let overrides = LaunchOverrides::default();
        let missing = fixture.root.join("nowhere").join("java.exe");
        let mut command = request(
            &fixture,
            &client,
            &fixture.config,
            &fixture.account,
            &overrides,
        );
        command.java_override = Some(&missing);

        let error = build_command(command).expect_err("rejected");

        assert!(error.to_string().contains("does not exist"), "{error}");
    }

    #[test]
    fn a_missing_runtime_names_the_detected_versions() {
        let fixture = fixture("missing-java");
        let client = client();
        let overrides = LaunchOverrides::default();
        let mut command = request(
            &fixture,
            &client,
            &fixture.config,
            &fixture.account,
            &overrides,
        );
        command.java_installations = &[];

        let error = build_command(command).expect_err("no runtime");

        let message = error.to_string();
        assert!(message.contains("requires Java"), "{message}");
        assert!(message.contains("no Java runtime"), "{message}");
    }

    #[test]
    fn manual_memory_overrides_the_global_value() {
        let fixture = fixture("memory");
        let client = client();
        let overrides = LaunchOverrides::default();
        let mut command = request(
            &fixture,
            &client,
            &fixture.config,
            &fixture.account,
            &overrides,
        );
        command.min_memory_mb = Some(2048);
        command.max_memory_mb = Some(6144);

        let (spec, _) = build_command(command).expect("build");

        assert!(
            spec.args.iter().any(|argument| argument == "-Xmx6144M"),
            "{:?}",
            spec.args
        );
        assert!(
            spec.args.iter().any(|argument| argument == "-Xms2048M"),
            "{:?}",
            spec.args
        );
    }

    #[test]
    fn window_size_overrides_reach_the_command() {
        let fixture = fixture("window");
        let client = client();
        let overrides = LaunchOverrides::default();
        let mut command = request(
            &fixture,
            &client,
            &fixture.config,
            &fixture.account,
            &overrides,
        );
        command.width = Some(1920);
        command.height = Some(1080);

        let (spec, _) = build_command(command).expect("build");

        assert!(
            spec.args.windows(2).any(|pair| pair == ["--width", "1920"]),
            "{:?}",
            spec.args
        );
        assert!(
            spec.args
                .windows(2)
                .any(|pair| pair == ["--height", "1080"]),
            "{:?}",
            spec.args
        );
    }

    #[test]
    fn an_explicit_override_is_tried_before_a_fixed_java_choice() {
        use crate::core::config::models::JavaSelectionMode;

        let mut fixture = fixture("override-precedence");
        let client = client();
        let fixed_path = fixture.root.join("fixed").join("java.exe");
        std::fs::create_dir_all(fixed_path.parent().expect("fixed dir")).expect("fixed dir");
        std::fs::write(&fixed_path, b"java").expect("fixed java");
        let fixed = JavaInstallation {
            executable: fixed_path.clone(),
            version: "21.0.9".into(),
            major: 21,
            vendor: Some("fixed".into()),
            is_jdk: true,
            lts: true,
        };
        fixture.config.java.selection_mode = JavaSelectionMode::Fixed;
        fixture.config.java.selected_path = Some(fixed_path);

        // A missing override proves the order: if the fixed choice ran first,
        // this command would have been built from it.
        let missing = fixture.root.join("gone").join("java.exe");
        let overrides = LaunchOverrides::default();
        let candidates = [fixed, fixture.java.clone()];
        let mut command = request(
            &fixture,
            &client,
            &fixture.config,
            &fixture.account,
            &overrides,
        );
        command.java_installations = &candidates;
        command.java_override = Some(&missing);

        let error = build_command(command).expect_err("the override is tried first");

        let message = error.to_string();
        assert!(message.contains("does not exist"), "{message}");
        assert!(
            message.contains(&missing.display().to_string()),
            "{message}"
        );
    }

    #[test]
    fn a_missing_client_jar_is_named_before_the_game_starts() {
        let fixture = fixture("missing-jar");
        std::fs::remove_file(fixture.instance.config.path.join("demo.jar")).expect("remove jar");
        let client = client();
        let overrides = LaunchOverrides::default();

        let error = build_command(request(
            &fixture,
            &client,
            &fixture.config,
            &fixture.account,
            &overrides,
        ))
        .expect_err("missing jar");

        assert!(error.to_string().contains("files required"), "{error}");
    }

    fn launch_request(fixture: &Fixture, installations: Vec<JavaInstallation>) -> LaunchRequest {
        LaunchRequest {
            instance: fixture.instance.clone(),
            account_id: None,
            java_installations: Some(installations),
            java_override: None,
            allow_java_download: false,
            client_json: None,
            min_memory_mb: None,
            max_memory_mb: None,
            width: None,
            height: None,
            fullscreen: None,
        }
    }

    #[tokio::test]
    async fn prepare_reports_every_stage_in_order() {
        let fixture = fixture("stages");
        let paths = data_dir(&fixture);
        let mut stages = Vec::new();

        let prepared = prepare(
            launch_request(&fixture, vec![fixture.java.clone()]),
            &paths,
            &Config::default(),
            |stage| stages.push(stage),
        )
        .await
        .expect("prepare");

        assert_eq!(
            stages,
            [
                LaunchStage::ResolvingAccount,
                LaunchStage::SelectingJava,
                LaunchStage::PreparingNatives,
                LaunchStage::BuildingCommand,
                LaunchStage::VerifyingFiles,
            ]
        );
        assert_eq!(
            prepared.account.kind,
            crate::core::account::models::AccountKind::Offline
        );
        assert_eq!(prepared.token, "0");
    }

    #[tokio::test]
    async fn prepare_falls_back_to_an_offline_account() {
        let fixture = fixture("offline");
        let paths = data_dir(&fixture);

        let prepared = prepare(
            launch_request(&fixture, vec![fixture.java.clone()]),
            &paths,
            &Config::default(),
            |_| {},
        )
        .await
        .expect("prepare");

        assert_eq!(prepared.account.username, "Player");
        assert_eq!(prepared.token, "0");
    }

    #[tokio::test]
    async fn prepare_does_not_scan_when_the_caller_supplies_an_empty_list() {
        let fixture = fixture("supplied");
        let paths = data_dir(&fixture);

        // An empty list means "these are all of them": `prepare` must not fall
        // back to scanning the machine, which would make this test depend on
        // whatever Java happens to be installed.
        let error = prepare(
            launch_request(&fixture, Vec::new()),
            &paths,
            &Config::default(),
            |_| {},
        )
        .await
        .expect_err("no runtime");

        assert!(error.to_string().contains("no Java runtime"), "{error}");
    }

    #[tokio::test]
    async fn prepare_refuses_to_download_when_not_allowed() {
        let fixture = fixture("no-download");
        let paths = data_dir(&fixture);

        let error = prepare(
            launch_request(&fixture, Vec::new()),
            &paths,
            &Config::default(),
            |_| {},
        )
        .await
        .expect_err("refused");

        // Reaching the network would have produced a different error; the
        // message proves the download branch was not taken.
        assert!(error.to_string().contains("requires Java"), "{error}");
    }

    #[tokio::test]
    async fn an_explicit_client_json_wins_over_the_instance_metadata() {
        let fixture = fixture("client-json");
        let paths = data_dir(&fixture);
        let other = fixture.root.join("other.json");
        std::fs::write(
            &other,
            br#"{"id":"1.20.1","mainClass":"net.minecraft.client.main.Main","arguments":{"game":[],"jvm":[]}}"#,
        )
        .expect("other client json");
        let mut request = launch_request(&fixture, vec![fixture.java.clone()]);
        request.client_json = Some(other);

        let prepared = prepare(request, &paths, &Config::default(), |_| {})
            .await
            .expect("prepare");

        assert_eq!(prepared.client.id, "1.20.1");
    }

    #[test]
    fn instance_parts_surfaces_the_three_launch_inputs() {
        let config = InstanceConfig::new("C:/demo", uuid::Uuid::nil(), "demo", "1.21.1");
        let (path, name, version) = super::instance_parts(&config);
        assert_eq!(path, Path::new("C:/demo"));
        assert_eq!(name, "demo");
        assert_eq!(version, "1.21.1");
    }

    #[test]
    fn the_log_name_carries_the_instance_and_a_utc_stamp() {
        // 2026-11-08T11:02:11Z, which is 19:02 in UTC+8.
        let stamp = super::utc_stamp(1_794_135_731);
        assert_eq!(stamp, "20261108-110211");
        assert_eq!(super::utc_stamp(0), "19700101-000000");
        let path = super::launch_log_path(Path::new("logs"), "生存 存档", 1_794_135_731);
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("生存_存档-20261108-110211.log")
        );
    }

    #[test]
    fn a_second_launch_in_the_same_second_gets_a_suffix() {
        let root = std::env::temp_dir().join(format!("tmcl-log-name-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("dir");
        let first = super::launch_log_path(&root, "demo", 1_794_135_731);
        std::fs::write(&first, b"x").expect("first log");
        let second = super::launch_log_path(&root, "demo", 1_794_135_731);
        assert_ne!(first, second);
        assert!(
            second
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with("-2.log"))
        );
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn a_name_that_is_all_separators_falls_back_to_instance() {
        assert_eq!(super::safe_file_stem("  //  "), "instance");
        assert_eq!(super::safe_file_stem("   "), "instance");
        assert_eq!(super::safe_file_stem("a/b:c"), "a_b_c");
    }
}
