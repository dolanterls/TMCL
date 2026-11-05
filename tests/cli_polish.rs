//! End-to-end checks of the polish commands: completions, diagnostics and
//! `update`.
//!
//! The release endpoint is a local server, so `update` is covered without
//! reaching GitHub. `update apply` is only exercised up to the confirmation
//! prompt: replacing the test binary with a download would leave the test
//! runner without its own executable.

mod support;

use std::path::PathBuf;
use std::process::{Command, Output};

use support::Server;

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "tmcl-polish-{tag}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let instance = root.join("game").join("versions").join("demo");
        std::fs::create_dir_all(&instance).expect("instance dir");
        std::fs::create_dir_all(root.join("appdata")).expect("appdata");
        std::fs::write(instance.join("demo.jar"), b"jar").expect("jar");
        std::fs::write(
            instance.join("demo.json"),
            r#"{"id":"1.21.1","mainClass":"net.minecraft.client.main.Main","arguments":{"game":[],"jvm":[]}}"#,
        )
        .expect("client json");
        std::fs::write(
            instance.join("tmcl.json"),
            br#"{"id":"11111111-1111-4111-8111-111111111111","name":"demo","game_directory_id":"22222222-2222-4222-8222-222222222222","minecraft_version":"1.21.1","loader":"Vanilla","stats":{"launch_count":0,"play_time_seconds":0}}"#,
        )
        .expect("metadata");
        let sandbox = Self { root };
        let status = sandbox
            .run(&[
                "config",
                "game-dir",
                "add",
                &sandbox.game().display().to_string(),
                "--json",
            ])
            .status;
        assert!(status.success(), "registering the game directory failed");
        sandbox
    }

    fn game(&self) -> PathBuf {
        self.root.join("game")
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_with_env(args, &[])
    }

    fn run_with_env(&self, args: &[&str], envs: &[(&str, &str)]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tmcl"));
        command.env("APPDATA", self.root.join("appdata")).args(args);
        // Keep the developer's TMCL_LOG out of the assertions.
        command.env_remove("TMCL_LOG");
        for (name, value) in envs {
            command.env(name, value);
        }
        command.output().expect("run tmcl")
    }

    fn json(&self, args: &[&str]) -> serde_json::Value {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "{} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
            panic!(
                "{} did not print JSON ({error}): {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stdout)
            )
        })
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn completions_are_generated_for_each_shell() {
    let sandbox = Sandbox::new("completions");

    let bash = sandbox.run(&["completions", "bash"]);
    assert!(bash.status.success());
    let script = String::from_utf8_lossy(&bash.stdout);
    assert!(script.contains("tmcl"), "no command name in the script");
    assert!(script.contains("complete"), "not a bash completion script");

    let powershell = sandbox.run(&["completions", "powershell"]);
    assert!(powershell.status.success());
    let script = String::from_utf8_lossy(&powershell.stdout);
    assert!(
        script.contains("Register-ArgumentCompleter"),
        "not a PowerShell completion script"
    );

    // The shell name is a closed set; a typo must not produce an empty script.
    let unknown = sandbox.run(&["completions", "tcsh"]);
    assert_eq!(unknown.status.code(), Some(2));
}

#[test]
fn verbose_writes_http_diagnostics_to_stderr_and_not_to_stdout() {
    let sandbox = Sandbox::new("verbose");
    let server = Server::start(vec![("/sodium.jar", b"jar!", "application/java-archive")]);

    let output = sandbox.run(&[
        "content",
        "add",
        "demo",
        &server.url("/sodium.jar"),
        "--kind",
        "mod",
        "--verbose",
        "--json",
    ]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("[tmcl http]"), "{stderr}");
    assert!(stderr.contains("/sodium.jar"), "{stderr}");
    // stdout stays parseable: that is the whole point of sending diagnostics to
    // stderr.
    let parsed: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout is still JSON");
    assert_eq!(parsed["name"], "sodium.jar");
}

#[test]
fn without_the_switch_diagnostics_stay_quiet() {
    let sandbox = Sandbox::new("quiet-diagnostics");
    let server = Server::start(vec![("/lithium.jar", b"jar!", "application/java-archive")]);

    let output = sandbox.run(&[
        "content",
        "add",
        "demo",
        &server.url("/lithium.jar"),
        "--kind",
        "mod",
        "--json",
    ]);

    assert!(output.status.success());
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("[tmcl http]"),
        "diagnostics must be off by default"
    );
}

#[test]
fn tmcl_log_turns_the_same_diagnostics_on() {
    let sandbox = Sandbox::new("tmcl-log");
    let server = Server::start(vec![("/ferrite.jar", b"jar!", "application/java-archive")]);

    let output = sandbox.run_with_env(
        &[
            "content",
            "add",
            "demo",
            &server.url("/ferrite.jar"),
            "--kind",
            "mod",
        ],
        &[("TMCL_LOG", "debug")],
    );

    assert!(output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("[tmcl http]"),
        "TMCL_LOG=debug should enable diagnostics: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn update_check_needs_an_endpoint_and_says_how_to_give_one() {
    let sandbox = Sandbox::new("update-endpoint");

    let output = sandbox.run(&["update", "check"]);

    assert_eq!(output.status.code(), Some(4), "version errors exit with 4");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("TMCL_UPDATE_URL"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn the_old_update_check_name_still_works_and_warns() {
    let sandbox = Sandbox::new("update-alias");
    let release = release_json();

    let output = sandbox.run(&["update-check", "--url", &release.url("/latest"), "--json"]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("now `update check`"), "{stderr}");
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json");
    assert_eq!(parsed["version"], "v9.9.9");
}

#[test]
fn update_apply_dry_run_names_the_asset_and_targets_this_executable() {
    let sandbox = Sandbox::new("update-plan");
    let release = release_json();

    let planned = sandbox.json(&[
        "update",
        "apply",
        "--url",
        &release.url("/latest"),
        "--dry-run",
        "--json",
    ]);

    assert_eq!(planned["applied"], false, "{planned}");
    assert_eq!(planned["latest"], "v9.9.9", "{planned}");
    assert_eq!(planned["asset"], asset_name(), "{planned}");
    // The plan always names the file it would replace.
    assert!(planned["target"].as_str().is_some(), "{planned}");
}

#[test]
fn update_apply_refuses_to_replace_the_executable_without_confirmation() {
    let sandbox = Sandbox::new("update-consent");
    let release = release_json();

    let output = sandbox.run(&["update", "apply", "--url", &release.url("/latest")]);

    assert_eq!(
        output.status.code(),
        Some(2),
        "no --yes, no --dry-run: refuse"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--yes"), "{stderr}");
    assert!(stderr.contains("--dry-run"), "{stderr}");
}

/// The release both tests above describe: one asset, built for this machine.
fn release_json() -> Server {
    let name = asset_name();
    let json = format!(
        r#"{{"tag_name":"v9.9.9","html_url":"https://example/releases/v9.9.9","assets":[{{"name":"{name}","browser_download_url":"http://127.0.0.1:9/unused","size":1024}}]}}"#
    );
    let body: &'static [u8] = Box::leak(json.into_bytes().into_boxed_slice());
    Server::start(vec![("/latest", body, "application/json")])
}

fn asset_name() -> String {
    format!(
        "tmcl-{}-{}.exe",
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}
