//! `tmcl diagnose` end to end.
//!
//! The unit tests cover the rules; this covers the part that only breaks when the
//! pieces are wired together: the sandbox launcher's own data directory, the
//! newest-log lookup, and the bundle that comes out of `--export`.

mod support;

use std::path::Path;

use support::Sandbox;

/// A log whose class-file version cannot run on the configured Java.
const OLD_JAVA_LOG: &str = "\
[stdout] [19:02:11] [main/INFO]: Loading Minecraft 1.21.1 with Fabric Loader 0.16.9
[stdout] [19:02:12] [main/INFO]: Loading 12 mods
[stderr] java.lang.UnsupportedClassVersionError: net/fabricmc/loader/impl/launch/knot/KnotClient has been compiled by a more recent version of the Java Runtime (class file version 61.0), this version of the Java Runtime only recognizes class file versions up to 52.0
[stderr] [19:02:12] [main/ERROR]: A fatal error has been detected
";

fn sandbox_with_run(tag: &str) -> Sandbox {
    let sandbox = Sandbox::new(tag);
    assert!(
        sandbox
            .run(&[
                "config",
                "game-dir",
                "add",
                &sandbox.game().display().to_string()
            ])
            .status
            .success()
    );

    let logs = sandbox.root.join("TMCL").join("logs").join("game");
    std::fs::create_dir_all(&logs).expect("log dir");
    std::fs::write(logs.join("demo-20261108-190211.log"), OLD_JAVA_LOG).expect("log");

    let reports = sandbox.instance().join("crash-reports");
    std::fs::create_dir_all(&reports).expect("crash dir");
    std::fs::write(
        reports.join("crash-2026-11-08_19.02.12-client.txt"),
        "---- Minecraft Crash Report ----\n// Why did you do that?\n",
    )
    .expect("crash report");
    sandbox
}

#[test]
fn diagnose_names_the_cause_and_says_what_it_read() {
    let sandbox = sandbox_with_run("diagnose-json");

    let printed = sandbox.json(&["diagnose", "demo", "--json"]);
    assert_eq!(printed["primary"]["rule"], "JavaTooOld", "{printed}");
    assert_eq!(printed["stage"], "NeverStarted", "{printed}");
    assert_eq!(printed["exit"], "Crashed", "{printed}");
    assert_eq!(printed["unknown"], false, "{printed}");

    let sources = printed["sources"].as_array().expect("sources");
    assert_eq!(sources.len(), 4, "{printed}");
    assert_eq!(sources[0]["present"], true, "{printed}");
    assert_eq!(sources[1]["present"], true, "{printed}");
    assert!(
        sources[1]["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("crash-2026-11-08_19.02.12-client.txt")),
        "{printed}"
    );
}

/// The human-readable output must carry the same verdict; a player who pipes
/// the command into a paste does not get JSON.
#[test]
fn the_plain_output_leads_with_the_verdict() {
    let sandbox = sandbox_with_run("diagnose-text");
    let output = sandbox.run(&["diagnose", "demo"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("TMCL crash diagnosis"), "{text}");
    assert!(text.contains("Java"), "{text}");
    assert!(text.contains("61"), "the required major is missing: {text}");
    assert!(
        text.contains("Read"),
        "the report does not list its inputs: {text}"
    );
}

#[test]
fn export_writes_a_bundle_that_opens() {
    let sandbox = sandbox_with_run("diagnose-export");
    let target = sandbox.root.join("out").join("survival.zip");

    let output = sandbox.run(&[
        "diagnose",
        "demo",
        "--export",
        &target.display().to_string(),
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(target.is_file(), "no bundle at {}", target.display());

    let mut archive =
        zip::ZipArchive::new(std::fs::File::open(&target).expect("open bundle")).expect("zip");
    let names = (0..archive.len())
        .map(|index| archive.by_index(index).expect("entry").name().to_owned())
        .collect::<Vec<_>>();
    for expected in [
        "report.txt",
        "game.log",
        "crash-report.txt",
        "instance.json",
    ] {
        assert!(names.iter().any(|name| name == expected), "{names:?}");
    }

    let mut report = String::new();
    {
        use std::io::Read;
        archive
            .by_name("report.txt")
            .expect("report")
            .read_to_string(&mut report)
            .expect("read report");
    }
    assert!(report.contains("Java"), "{report}");

    // A second export to the same path replaces the first instead of failing.
    let again = sandbox.run(&[
        "diagnose",
        "demo",
        "--export",
        &target.display().to_string(),
    ]);
    assert!(
        again.status.success(),
        "the second export failed: {}",
        String::from_utf8_lossy(&again.stderr)
    );
    assert!(target.is_file());
}

#[test]
fn a_missing_log_is_an_error_not_an_empty_verdict() {
    let sandbox = Sandbox::new("diagnose-no-log");
    assert!(
        sandbox
            .run(&[
                "config",
                "game-dir",
                "add",
                &sandbox.game().display().to_string()
            ])
            .status
            .success()
    );

    let output = sandbox.run(&["diagnose", "demo"]);
    assert!(!output.status.success(), "a missing log must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no game log"), "{stderr}");

    let explicit = sandbox.run(&[
        "diagnose",
        "demo",
        "--log",
        &sandbox.root.join("nope.log").display().to_string(),
    ]);
    assert!(!explicit.status.success());
    assert!(
        String::from_utf8_lossy(&explicit.stderr).contains("log file not found"),
        "{}",
        String::from_utf8_lossy(&explicit.stderr)
    );
}

/// `--log` has to win over the newest file: a player who exported a log from
/// another machine should be able to point at it.
#[test]
fn an_explicit_log_is_read_instead_of_the_newest_one() {
    let sandbox = sandbox_with_run("diagnose-explicit");
    let other = sandbox.root.join("elsewhere.log");
    std::fs::write(
        &other,
        "[stdout] [19:02:11] [main/INFO]: Setting user: Player\n\
         [stdout] [19:02:12] [main/INFO]: Backend library: LWJGL version 3.3.3\n\
         [stderr] java.lang.OutOfMemoryError: Java heap space\n",
    )
    .expect("other log");

    // A later session left its own log behind. It sits at the end of any
    // concatenation, so before `LogScope::File` it took the tail bonus and won.
    let latest = sandbox.instance().join("logs").join("latest.log");
    std::fs::create_dir_all(latest.parent().expect("logs dir")).expect("logs dir");
    std::fs::write(
        &latest,
        "[stdout] [20:10:00] [Render thread/ERROR]: Pixel format not accelerated\n",
    )
    .expect("latest log");

    let printed = sandbox.json(&[
        "diagnose",
        "demo",
        "--log",
        &other.display().to_string(),
        "--json",
    ]);
    assert_eq!(printed["primary"]["rule"], "OutOfMemory", "{printed}");
    assert_eq!(printed["stage"], "InGame", "{printed}");
    assert_eq!(
        Path::new(printed["sources"][0]["path"].as_str().expect("path")),
        other
    );
    // And nothing else: the instance's own latest.log / crash report belong to
    // a different session, and appending them lets that session outrank the
    // file the player pointed at.
    assert_eq!(
        printed["sources"].as_array().expect("sources").len(),
        1,
        "{printed}"
    );
}

/// Every fixture must produce a verdict through the real binary, not only
/// through the unit-test helper.
///
/// Two bugs hide behind that difference: mixing `--log` with the instance's
/// `latest.log`, and classifying a fatal sentence without the English word
/// "exception" as a killed process. The fixture guard forces `Crashed`, so neither
/// shows up there.
#[test]
fn every_fixture_produces_a_verdict_through_the_binary() {
    let sandbox = sandbox_with_run("diagnose-fixtures");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("core")
        .join("diagnose")
        .join("fixtures");
    let mut checked = 0;
    let mut entries = std::fs::read_dir(&fixtures)
        .expect("fixtures directory")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "txt"))
        .collect::<Vec<_>>();
    entries.sort();

    for path in entries {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned();
        let printed = sandbox.json(&[
            "diagnose",
            "demo",
            "--log",
            &path.display().to_string(),
            "--json",
        ]);
        if name == "clean.txt" {
            assert_eq!(printed["exit"], "Clean", "{name}: {printed}");
            assert!(printed["primary"].is_null(), "{name}: {printed}");
            continue;
        }
        // The two weight-2 rules only say "the loader printed something". They
        // are meant to appear as alternatives inside a real crash, not to turn
        // a log into one on their own.
        if matches!(
            name.as_str(),
            "fabric_warnings.txt" | "loader_reported_mod_failure.txt"
        ) {
            assert_eq!(printed["exit"], "UserKilled", "{name}: {printed}");
            assert!(printed["primary"].is_null(), "{name}: {printed}");
            continue;
        }
        assert!(
            !printed["primary"].is_null(),
            "{name} produced no verdict: {printed}"
        );
        checked += 1;
    }
    assert_eq!(checked, 55, "expected 55 verdicts, saw {checked}");
}
