//! Resolve a user-supplied instance reference to a concrete instance.
//!
//! Every front end used to address an instance by its own rule: the CLI took a
//! filesystem path, the TUI took whatever row the list had focused. That made
//! `instance launch` require `versions\<name>` spelled out, and made the two
//! ends disagree about what "the current instance" means.
//!
//! This module owns the single rule:
//!
//! | Form | Test | Example |
//! |---|---|---|
//! | `@last` | literal | `@last` |
//! | `+3` | `+` followed by digits | `+3` |
//! | number | all digits | `3` |
//! | UUID | `Uuid::parse_str` succeeds | `3f2a…` |
//! | path | contains a separator, or names an existing directory | `./versions/demo` |
//! | name | everything else | `demo` |
//!
//! A number is the position in `instance list`. It is a position, not an
//! identity, so it moves when the list changes; a full name never does. For a
//! bare number an instance *named* `123` wins over the 123rd row, because
//! renaming an instance must not make it unaddressable. `+123` skips that check
//! and always means the position.
//!
//! The sigil is `+` and not `@` or `#`: PowerShell treats any `@word` argument
//! as splatting and silently drops it — `instance launch @9` there would launch
//! the current instance instead of the ninth — and `#` starts a comment. `+`
//! survives cmd, bash and PowerShell unchanged.
//!
//! A name must match exactly, or be an unambiguous prefix of one (case and
//! full-width differences ignored). Ambiguity is an error, never a first-match:
//! silently picking one of two same-named instances across two game directories
//! would eventually delete the wrong one.

use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::core::config::models::Config;
use crate::core::error::{Error, Result};
use crate::core::instance::config::load_or_create;
use crate::core::instance::models::InstanceConfig;
use crate::core::instance::path::{game_directory_id, game_directory_of, normalize_game_directory};
use crate::core::instance::registry::scan_excluding;

/// What the reference looked like before anything was read from disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstanceRef {
    Last,
    /// One-based position in `instance list`; a same-named instance wins.
    Index(usize),
    /// `+n`: the position, whether or not an instance is named that.
    ForcedIndex(usize),
    Id(Uuid),
    Path(PathBuf),
    Name(String),
}

/// Which rule produced a [`ResolvedInstance`]. Reported by `instance show` so a
/// surprising match is explainable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefSource {
    Last,
    Index,
    Id,
    Path,
    Name,
}

#[derive(Debug, Clone)]
pub struct ResolvedInstance {
    /// Instance metadata, with `path` filled in.
    pub config: InstanceConfig,
    pub game_directory: PathBuf,
    pub source: RefSource,
}

/// Classify a reference without touching the disk.
pub fn parse(reference: &str) -> InstanceRef {
    let reference = reference.trim();
    if reference.eq_ignore_ascii_case("@last") {
        return InstanceRef::Last;
    }
    // `+3`: the position, written out. `+` is the one sigil that reaches the
    // program unchanged from cmd, bash and PowerShell alike.
    if let Some(digits) = reference.strip_prefix('+')
        && !digits.is_empty()
        && digits.bytes().all(|byte| byte.is_ascii_digit())
    {
        return InstanceRef::ForcedIndex(digits.parse().unwrap_or(usize::MAX));
    }
    // All digits: a position, unless an instance is actually called that.
    if !reference.is_empty() && reference.bytes().all(|byte| byte.is_ascii_digit()) {
        return InstanceRef::Index(reference.parse().unwrap_or(usize::MAX));
    }
    if let Ok(id) = Uuid::parse_str(reference) {
        return InstanceRef::Id(id);
    }
    // `:` catches the drive-relative `C:demo` form on Windows; a name cannot
    // contain it there, and on other platforms it is an ordinary character.
    if reference.contains('\\') || reference.contains('/') || reference.contains(':') {
        return InstanceRef::Path(PathBuf::from(reference));
    }
    InstanceRef::Name(reference.to_owned())
}

/// Resolve against the configured game directories.
pub fn resolve(reference: &str, config: &Config) -> Result<ResolvedInstance> {
    resolve_with_game_dir(reference, config, None)
}

/// Resolve against `game_dir` when given, the configured directories otherwise.
pub fn resolve_with_game_dir(
    reference: &str,
    config: &Config,
    game_dir: Option<&Path>,
) -> Result<ResolvedInstance> {
    let reference = reference.trim();
    if reference.is_empty() {
        return Err(Error::Config("instance reference is empty".into()));
    }

    let parsed = parse(reference);
    if let InstanceRef::Path(path) = &parsed {
        return resolve_path(path);
    }
    if let InstanceRef::Name(name) = &parsed
        && Path::new(name).is_dir()
    {
        // A bare name that is also a directory is a path: the user typed
        // something that exists, so honour the filesystem over the list.
        return resolve_path(Path::new(name));
    }

    let directories = search_directories(config, game_dir);
    let candidates = scan_all(&directories, config)?;
    match parsed {
        InstanceRef::Last => {
            let mut resolved = resolve_last(&candidates, config)?;
            resolved.source = RefSource::Last;
            Ok(resolved)
        }
        InstanceRef::Id(id) => {
            let mut resolved = candidates
                .into_iter()
                .find(|candidate| candidate.config.id == id)
                .ok_or_else(|| not_found(reference, &directories))?;
            resolved.source = RefSource::Id;
            Ok(resolved)
        }
        InstanceRef::Index(index) => {
            // An instance actually named `9` outranks the ninth row: a name is
            // stable, a position is not. `+9` is how you insist on the row.
            if let Some(named) = candidates.iter().find(|candidate| {
                candidate.config.name == reference
                    || directory_name(&candidate.config.path) == Some(reference)
            }) {
                let mut resolved = named.clone();
                resolved.source = RefSource::Name;
                return Ok(resolved);
            }
            let mut resolved = resolve_index(&candidates, index)?;
            resolved.source = RefSource::Index;
            Ok(resolved)
        }
        InstanceRef::ForcedIndex(index) => {
            let mut resolved = resolve_index(&candidates, index)?;
            resolved.source = RefSource::Index;
            Ok(resolved)
        }
        InstanceRef::Name(name) => resolve_name(&candidates, &name, reference, &directories),
        // Handled before the scan; keeping the arm keeps the match exhaustive.
        InstanceRef::Path(_) => unreachable!("path references return before scanning"),
    }
}

/// Every instance the configured directories currently hold.
pub fn list_all(config: &Config) -> Result<Vec<ResolvedInstance>> {
    list_all_in(config, None)
}

/// Same, but a caller may narrow the search to one game directory.
pub fn list_all_in(config: &Config, game_dir: Option<&Path>) -> Result<Vec<ResolvedInstance>> {
    scan_all(&search_directories(config, game_dir), config)
}

/// The game directories to search, skipping ones that no longer exist.
///
/// A removed game directory must not break name resolution: `instance list`
/// already tolerates it, and erroring here would make every command unusable
/// until the config was hand-edited.
fn search_directories(config: &Config, game_dir: Option<&Path>) -> Vec<PathBuf> {
    let directories = match game_dir {
        Some(path) => vec![path.to_path_buf()],
        None => config.game_directories.clone(),
    };
    directories
        .into_iter()
        .filter(|path| path.is_dir())
        .filter_map(|path| normalize_game_directory(&path).ok())
        .collect()
}

fn scan_all(directories: &[PathBuf], config: &Config) -> Result<Vec<ResolvedInstance>> {
    let mut instances = Vec::new();
    for directory in directories {
        let id = game_directory_id(directory);
        for instance in scan_excluding(directory, id, &config.excluded_instances)? {
            instances.push(ResolvedInstance {
                config: instance,
                game_directory: directory.clone(),
                source: RefSource::Name,
            });
        }
    }
    // `read_dir` hands the entries back in whatever order the filesystem keeps
    // them, so without this the numbers `instance list` prints would move
    // between runs and `3` would mean something different each time.
    instances.sort_by(|left, right| {
        fold(&left.config.name)
            .cmp(&fold(&right.config.name))
            .then_with(|| left.config.path.cmp(&right.config.path))
    });
    Ok(instances)
}

/// `<n>`: the n-th row of `instance list`, one-based.
fn resolve_index(candidates: &[ResolvedInstance], index: usize) -> Result<ResolvedInstance> {
    if index == 0 {
        return Err(Error::Config(
            "instance numbers start at 1; run `tmcl instance list` to see the list".into(),
        ));
    }
    candidates.get(index - 1).cloned().ok_or_else(|| {
        Error::Config(format!(
            "no instance number {index}; the list has {} entries (run `tmcl instance list`)",
            candidates.len()
        ))
    })
}

fn resolve_path(path: &Path) -> Result<ResolvedInstance> {
    if !path.is_dir() {
        return Err(Error::Config(format!(
            "instance directory does not exist: {}",
            path.display()
        )));
    }
    let game_directory = game_directory_of(path)?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            Error::Config(format!(
                "instance directory has no name: {}",
                path.display()
            ))
        })?;
    // Refuse anything that carries neither TMCL metadata nor a version JSON:
    // otherwise a typo turns into a brand-new instance pointing at an empty
    // folder.
    if !path.join("tmcl.json").is_file() && !path.join(format!("{name}.json")).is_file() {
        return Err(Error::Config(format!(
            "not a TMCL instance (no tmcl.json and no {name}.json): {}",
            path.display()
        )));
    }
    let id = game_directory_id(&game_directory);
    let config = load_or_create(path, id, "")?;
    Ok(ResolvedInstance {
        config,
        game_directory,
        source: RefSource::Path,
    })
}

fn resolve_last(candidates: &[ResolvedInstance], config: &Config) -> Result<ResolvedInstance> {
    let current = config.state.current_instance_id.as_deref().ok_or_else(|| {
        Error::Config(
            "no current instance is set; run `tmcl instance list` and pick one first".into(),
        )
    })?;
    let id = Uuid::parse_str(current).map_err(|_| {
        Error::Config(format!(
            "current instance id is not a valid UUID: {current}"
        ))
    })?;
    candidates
        .iter()
        .find(|candidate| candidate.config.id == id)
        .cloned()
        .ok_or_else(|| {
            Error::Config(format!(
                "the current instance is no longer installed ({id}); run `tmcl instance list`"
            ))
        })
}

fn resolve_name(
    candidates: &[ResolvedInstance],
    name: &str,
    reference: &str,
    directories: &[PathBuf],
) -> Result<ResolvedInstance> {
    let exact = candidates
        .iter()
        .filter(|candidate| {
            candidate.config.name == name || directory_name(&candidate.config.path) == Some(name)
        })
        .collect::<Vec<_>>();
    // `launch battle` for "BattleArmory TACZ 26.0.5". Only tried when nothing
    // matched exactly, so a full name is never reinterpreted as a prefix of
    // something longer.
    let matches = if exact.is_empty() {
        let wanted = fold(name);
        candidates
            .iter()
            .filter(|candidate| {
                fold(&candidate.config.name).starts_with(&wanted)
                    || directory_name(&candidate.config.path)
                        .is_some_and(|directory| fold(directory).starts_with(&wanted))
            })
            .collect::<Vec<_>>()
    } else {
        exact
    };
    match matches.len() {
        0 => Err(not_found(reference, directories)),
        1 => Ok(matches[0].clone()),
        _ => {
            let listed = matches
                .iter()
                .map(|candidate| {
                    format!(
                        "{}  ({})",
                        candidate.config.name,
                        candidate.config.path.display()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n  ");
            Err(Error::Config(format!(
                "instance reference is ambiguous: {reference} matches {} instances; use a path or a number instead:\n  {listed}",
                matches.len()
            )))
        }
    }
}

fn directory_name(path: &Path) -> Option<&str> {
    path.file_name().and_then(|name| name.to_str())
}

/// Lower case, with the full-width forms folded onto ASCII.
///
/// Instance names carry characters that look identical in a terminal but are
/// not (`Ⅱ` U+2161 against two `I`s), and a launch script may spell a name in
/// either case. Folding both sides is what makes an unambiguous prefix match
/// safe to offer.
fn fold(text: &str) -> String {
    text.chars()
        .flat_map(|character| match character {
            '\u{FF01}'..='\u{FF5E}' => char::from_u32(character as u32 - 0xFEE0)
                .map(|folded| folded.to_lowercase())
                .unwrap_or_else(|| character.to_lowercase()),
            '\u{3000}' => ' '.to_lowercase(),
            other => other.to_lowercase(),
        })
        .collect()
}

fn not_found(reference: &str, directories: &[PathBuf]) -> Error {
    let searched = if directories.is_empty() {
        "no game directories are configured; run `tmcl config game-dir add <path>`".to_owned()
    } else {
        directories
            .iter()
            .map(|directory| directory.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    Error::Config(format!(
        "instance not found: {reference}; searched: {searched}"
    ))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{InstanceRef, RefSource, parse, resolve, resolve_with_game_dir};
    use crate::core::config::models::Config;

    /// A game directory holding one discoverable instance per name.
    ///
    /// `discover` only accepts a directory that has both `<name>.jar` and
    /// `<name>.json`, so a fixture that skipped either would silently produce
    /// an empty list.
    fn game_dir(tag: &str, names: &[&str]) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("tmcl-resolve-{tag}-{}", uuid::Uuid::new_v4()));
        for name in names {
            let instance = root.join("versions").join(name);
            std::fs::create_dir_all(&instance).expect("instance dir");
            std::fs::write(instance.join(format!("{name}.jar")), b"jar").expect("jar");
            std::fs::write(
                instance.join(format!("{name}.json")),
                br#"{"id":"1.21.1","mainClass":"net.minecraft.client.main.Main","arguments":{"game":[],"jvm":[]}}"#,
            )
            .expect("client json");
        }
        std::fs::create_dir_all(&root).expect("game dir");
        root
    }

    fn config_with(directories: &[&Path]) -> Config {
        Config {
            game_directories: directories.iter().map(|path| path.to_path_buf()).collect(),
            ..Config::default()
        }
    }

    #[test]
    fn classifies_the_four_reference_forms() {
        assert_eq!(parse("@last"), InstanceRef::Last);
        assert_eq!(parse("3"), InstanceRef::Index(3));
        assert_eq!(parse("+3"), InstanceRef::ForcedIndex(3));
        // A name may start with `@`; only digits make a number.
        assert_eq!(parse("@home"), InstanceRef::Name("@home".into()));
        assert_eq!(parse("+home"), InstanceRef::Name("+home".into()));
        let id = uuid::Uuid::new_v4();
        assert_eq!(parse(&id.to_string()), InstanceRef::Id(id));
        assert!(matches!(parse("./versions/demo"), InstanceRef::Path(_)));
        assert!(matches!(
            parse(r"C:\games\versions\demo"),
            InstanceRef::Path(_)
        ));
        assert_eq!(parse("survival"), InstanceRef::Name("survival".into()));
    }

    #[test]
    fn resolves_a_name_to_the_only_match() {
        let root = game_dir("unique", &["survival"]);
        let config = config_with(&[&root]);

        let resolved = resolve("survival", &config).expect("resolve by name");

        assert_eq!(resolved.source, RefSource::Name);
        assert_eq!(resolved.config.name, "survival");
        assert_eq!(
            resolved.game_directory,
            crate::core::instance::path::normalize_game_directory(&root).expect("normalized")
        );
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn a_name_in_two_game_directories_is_ambiguous() {
        let first = game_dir("amb-a", &["dup"]);
        let second = game_dir("amb-b", &["dup"]);
        let config = config_with(&[&first, &second]);

        let error = resolve("dup", &config).expect_err("ambiguous");

        let message = error.to_string();
        assert!(message.contains("ambiguous"), "{message}");
        assert!(message.contains(&first.display().to_string()), "{message}");
        assert!(message.contains(&second.display().to_string()), "{message}");
        std::fs::remove_dir_all(first).expect("clean");
        std::fs::remove_dir_all(second).expect("clean");
    }

    #[test]
    fn last_follows_the_configured_current_instance() {
        let first = game_dir("last-a", &["alpha"]);
        let second = game_dir("last-b", &["beta"]);
        let mut config = config_with(&[&first, &second]);
        let beta = resolve_with_game_dir("beta", &config, Some(&second)).expect("find beta");
        config.state.current_instance_id = Some(beta.config.id.to_string());

        let resolved = resolve("@last", &config).expect("resolve @last");

        assert_eq!(resolved.source, RefSource::Last);
        assert_eq!(resolved.config.name, "beta");
        std::fs::remove_dir_all(first).expect("clean");
        std::fs::remove_dir_all(second).expect("clean");
    }

    #[test]
    fn last_without_a_current_instance_points_at_instance_list() {
        let root = game_dir("last-none", &["alpha"]);
        let config = config_with(&[&root]);

        let error = resolve("@last", &config).expect_err("no current instance");

        assert!(error.to_string().contains("instance list"), "{error}");
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn excluded_instances_are_invisible_to_name_lookup() {
        let root = game_dir("excluded", &["hidden"]);
        let mut config = config_with(&[&root]);
        config.excluded_instances = vec![root.join("versions").join("hidden")];

        let error = resolve("hidden", &config).expect_err("excluded");

        assert!(error.to_string().contains("not found"), "{error}");
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn a_path_resolves_without_being_configured() {
        let root = game_dir("bypath", &["outside"]);
        let config = Config::default();

        let resolved = resolve(
            &root.join("versions").join("outside").display().to_string(),
            &config,
        )
        .expect("resolve by path");

        assert_eq!(resolved.source, RefSource::Path);
        assert_eq!(resolved.config.name, "outside");
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn a_directory_without_metadata_or_a_version_json_is_refused() {
        let root =
            std::env::temp_dir().join(format!("tmcl-resolve-empty-{}", uuid::Uuid::new_v4()));
        let instance = root.join("versions").join("empty");
        std::fs::create_dir_all(&instance).expect("instance dir");

        let error =
            resolve(&instance.display().to_string(), &Config::default()).expect_err("refused");

        assert!(error.to_string().contains("not a TMCL instance"), "{error}");
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn missing_game_directories_do_not_break_resolution() {
        let root = game_dir("missing-dir", &["kept"]);
        let mut config = config_with(&[&root]);
        config
            .game_directories
            .push(PathBuf::from(r"D:\definitely\not\here"));

        let resolved = resolve("kept", &config).expect("resolve past the missing directory");

        assert_eq!(resolved.config.name, "kept");
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn a_number_picks_the_nth_row_of_the_list() {
        // Created out of order on purpose: the number follows the printed
        // order, not the order the fixtures were written in.
        let root = game_dir("index", &["bravo", "alpha", "charlie"]);
        let config = config_with(&[&root]);

        assert_eq!(resolve("1", &config).expect("first").config.name, "alpha");
        assert_eq!(resolve("3", &config).expect("third").config.name, "charlie");
        let listed = super::list_all(&config).expect("list");
        assert_eq!(listed[1].config.name, "bravo");
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn a_numeric_name_wins_over_the_row_number() {
        // The fixture is named `3` and sorts first, so the name and the number
        // would disagree: the name has to win, or renaming an instance to a
        // number would make it unaddressable.
        let root = game_dir("numeric-name", &["alpha", "bravo", "3"]);
        let config = config_with(&[&root]);

        let resolved = resolve("3", &config).expect("numeric name");

        assert_eq!(resolved.config.name, "3");
        assert_eq!(resolved.source, RefSource::Name);

        // `+3` is how you insist on the third row instead.
        let forced = resolve("+3", &config).expect("forced position");
        assert_eq!(forced.config.name, "bravo");
        assert_eq!(forced.source, RefSource::Index);
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn a_number_outside_the_list_says_how_many_there_are() {
        let root = game_dir("index-range", &["only"]);
        let config = config_with(&[&root]);

        let zero = resolve("0", &config).expect_err("zero is not one-based");
        assert!(zero.to_string().contains("start at 1"), "{zero}");

        let beyond = resolve("9", &config).expect_err("out of range");
        assert!(
            beyond.to_string().contains("the list has 1 entries"),
            "{beyond}"
        );
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn an_unambiguous_prefix_resolves() {
        let root = game_dir("prefix", &["BattleArmory TACZ 26.0.5", "ArdaCraft"]);
        let config = config_with(&[&root]);

        let resolved = resolve("arda", &config).expect("prefix");

        assert_eq!(resolved.config.name, "ArdaCraft");
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn an_ambiguous_prefix_lists_the_candidates() {
        let root = game_dir(
            "prefix-ambiguous",
            &["FarmingTales one", "FarmingTales two"],
        );
        let config = config_with(&[&root]);

        let error = resolve("farming", &config).expect_err("ambiguous");

        let message = error.to_string();
        assert!(message.contains("ambiguous"), "{message}");
        assert!(message.contains("FarmingTales one"), "{message}");
        assert!(message.contains("FarmingTales two"), "{message}");
        std::fs::remove_dir_all(root).expect("clean");
    }
}
