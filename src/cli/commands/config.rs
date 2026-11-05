use serde_json::Value;
use std::collections::BTreeMap;

use crate::cli::args::{ConfigCommand, GameDirCommand, JavaSelectCommand};
use crate::cli::output::OutputOptions;
use crate::core::config::models::{Config, JavaSelectionMode};
use crate::core::config::paths::DataPaths;
use crate::core::config::storage::write_json_atomic;
use crate::core::error::{Error, Result};
use crate::core::instance::path::normalize_game_directory;

use super::common::load_config;

pub(super) fn run(command: ConfigCommand, output: OutputOptions) -> Result<()> {
    let paths = DataPaths::discover()?;
    paths.ensure_dirs()?;
    match command {
        ConfigCommand::Path => output.print(&serde_json::json!({"root": paths.root}), || {
            paths.root.display().to_string()
        }),
        ConfigCommand::List => {
            let config = load_config(&paths)?;
            let entries = flatten(&config)?;
            output.print(&entries, || {
                entries
                    .iter()
                    .map(|(key, value)| format!("{key} = {}", render(value)))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
        }
        ConfigCommand::Get(args) => {
            let config = load_config(&paths)?;
            let entries = flatten(&config)?;
            let value = entries
                .get(&args.key)
                .ok_or_else(|| unknown_key(&args.key, &entries))?;
            output.print(value, || render(value))
        }
        ConfigCommand::Set(args) => {
            let mut config = load_config(&paths)?;
            let mut root = serde_json::to_value(&config)?;
            let current = lookup(&root, &args.key);
            let value = match current {
                Some(Value::Bool(_)) => Value::Bool(parse_bool(&args.value)?),
                Some(Value::Number(_)) => Value::Number(parse_number(&args.value)?),
                // Strings and nulls both accept the raw text; serde decides
                // whether the target field can hold it.
                Some(Value::String(_)) | Some(Value::Null) => Value::String(args.value.clone()),
                Some(_) => {
                    return Err(Error::Config(format!(
                        "{} is a list or object; it cannot be set from the command line",
                        args.key
                    )));
                }
                None => return Err(unknown_key(&args.key, &flatten(&config)?)),
            };
            assign(&mut root, &args.key, value)?;
            config = serde_json::from_value(root)
                .map_err(|error| Error::Config(format!("{}: {error}", args.key)))?;
            write_json_atomic(&paths.config_file, &config)?;
            let entries = flatten(&config)?;
            let applied = entries.get(&args.key).cloned().unwrap_or(Value::Null);
            output.print(&applied, || format!("{} = {}", args.key, render(&applied)))
        }
        ConfigCommand::GameDir { command } => game_dir(command, &paths, output),
        ConfigCommand::Java { command } => java_select(command, &paths, output),
    }
}

fn game_dir(command: GameDirCommand, paths: &DataPaths, output: OutputOptions) -> Result<()> {
    let mut config = load_config(paths)?;
    match command {
        GameDirCommand::List => output.print(&config.game_directories, || {
            if config.game_directories.is_empty() {
                return "No game directories configured.".to_owned();
            }
            config
                .game_directories
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join("\n")
        }),
        GameDirCommand::Add(args) => {
            let path = normalize_game_directory(args.path)?;
            if !config.game_directories.contains(&path) {
                config.game_directories.push(path.clone());
                write_json_atomic(&paths.config_file, &config)?;
            }
            output.print(&serde_json::json!({"path": path}), || {
                format!("Configured game directory {}", path.display())
            })
        }
        GameDirCommand::Remove(args) => {
            let typed = args.path.clone();
            let target = normalize_game_directory(&typed).unwrap_or(typed.clone());
            let before = config.game_directories.len();
            config
                .game_directories
                .retain(|path| path != &target && path != &typed);
            if config.game_directories.len() == before {
                return Err(Error::Config(format!(
                    "not a configured game directory: {}",
                    args.path.display()
                )));
            }
            write_json_atomic(&paths.config_file, &config)?;
            output.print(&serde_json::json!({"path": target}), || {
                format!("Removed game directory {}", target.display())
            })
        }
    }
}

fn java_select(command: JavaSelectCommand, paths: &DataPaths, output: OutputOptions) -> Result<()> {
    let mut config = load_config(paths)?;
    match command {
        JavaSelectCommand::Auto => {
            config.java.selection_mode = JavaSelectionMode::Automatic;
            write_json_atomic(&paths.config_file, &config)?;
            output.print(
                &serde_json::json!({"selection": "auto", "path": config.java.selected_path}),
                || "Launches will pick the best Java runtime found on this machine.".to_owned(),
            )
        }
        JavaSelectCommand::Path(args) => {
            let java = crate::core::java::scan::discover_one(&args.path)?;
            config.java.selection_mode = JavaSelectionMode::Fixed;
            config.java.selected_path = Some(java.executable.clone());
            write_json_atomic(&paths.config_file, &config)?;
            output.print(
                &serde_json::json!({
                    "selection": "fixed",
                    "path": java.executable,
                    "major": java.major,
                    "version": java.version,
                }),
                || {
                    format!(
                        "Launches will use Java {} at {}",
                        java.major,
                        java.executable.display()
                    )
                },
            )
        }
    }
}

/// Every leaf of the config as `dotted.key -> value`.
///
/// Derived from the serialized form instead of a hand-written key table: a new
/// field appears here the moment it appears in `Config`, and cannot drift.
fn flatten(config: &Config) -> Result<BTreeMap<String, Value>> {
    let mut out = BTreeMap::new();
    walk("", &serde_json::to_value(config)?, &mut out);
    Ok(out)
}

fn walk(prefix: &str, value: &Value, out: &mut BTreeMap<String, Value>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                walk(&path, child, out);
            }
        }
        other => {
            out.insert(prefix.to_owned(), other.clone());
        }
    }
}

fn lookup<'a>(root: &'a Value, key: &str) -> Option<&'a Value> {
    let mut current = root;
    for segment in key.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

fn assign(root: &mut Value, key: &str, value: Value) -> Result<()> {
    let (parent, leaf) = match key.rsplit_once('.') {
        Some((parent, leaf)) => (parent, leaf),
        None => {
            return Err(Error::Config(
                "setting a top-level object is not supported; name a leaf key".into(),
            ));
        }
    };
    let mut current = root;
    for segment in parent.split('.') {
        current = current
            .get_mut(segment)
            .ok_or_else(|| Error::Config(format!("unknown setting: {key}")))?;
    }
    let object = current
        .as_object_mut()
        .ok_or_else(|| Error::Config(format!("unknown setting: {key}")))?;
    if !object.contains_key(leaf) {
        return Err(Error::Config(format!("unknown setting: {key}")));
    }
    object.insert(leaf.to_owned(), value);
    Ok(())
}

fn parse_bool(raw: &str) -> Result<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Ok(true),
        "false" | "no" | "off" | "0" => Ok(false),
        _ => Err(Error::Config(format!("expected a boolean, got {raw:?}"))),
    }
}

fn parse_number(raw: &str) -> Result<serde_json::Number> {
    let trimmed = raw.trim();
    if let Ok(value) = trimmed.parse::<i64>() {
        return Ok(value.into());
    }
    if let Ok(value) = trimmed.parse::<u64>() {
        return Ok(value.into());
    }
    trimmed
        .parse::<f64>()
        .map_err(|_| Error::Config(format!("expected a number, got {raw:?}")))
        .and_then(|value| {
            serde_json::Number::from_f64(value)
                .ok_or_else(|| Error::Config(format!("not a finite number: {raw:?}")))
        })
}

/// Points at the closest real key so a typo is one edit away from working.
fn unknown_key(key: &str, entries: &BTreeMap<String, Value>) -> Error {
    let suggestion = entries
        .keys()
        .map(|candidate| (distance(key, candidate), candidate))
        .filter(|(distance, _)| *distance <= 2)
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, candidate)| candidate.clone());
    match suggestion {
        Some(candidate) => {
            Error::Config(format!("unknown setting: {key}; did you mean {candidate}?"))
        }
        None => Error::Config(format!("unknown setting: {key}")),
    }
}

fn distance(left: &str, right: &str) -> usize {
    let left = left.chars().collect::<Vec<_>>();
    let right = right.chars().collect::<Vec<_>>();
    let mut previous = (0..=right.len()).collect::<Vec<_>>();
    for (i, a) in left.iter().enumerate() {
        let mut current = vec![i + 1];
        for (j, b) in right.iter().enumerate() {
            let substitution = previous[j] + usize::from(a != b);
            current.push((previous[j + 1] + 1).min(current[j] + 1).min(substitution));
        }
        previous = current;
    }
    previous[right.len()]
}

fn render(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => "null".to_owned(),
        other => other.to_string(),
    }
}
