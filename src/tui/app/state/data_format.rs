//! Parsing and formatting helpers split out of `data.rs`.

use super::*;

pub(in crate::tui::app) fn parse_install_loader(
    loader: &str,
    loader_version: &str,
) -> Result<(InstanceLoader, InstallLoader)> {
    crate::core::loader::parse_install_loader(loader, loader_version)
}

pub(in crate::tui::app) fn parse_positive_u32(
    values: &[String],
    index: usize,
    field: &str,
) -> Result<u32> {
    let value = required_value(values, index, field)?
        .parse::<u32>()
        .map_err(|_| Error::Config(format!("{field} must be a positive integer")))?;
    if value == 0 {
        return Err(Error::Config(format!("{field} must be greater than zero")));
    }
    Ok(value)
}

pub(in crate::tui::app) fn source_mode_label(mode: DownloadSourceMode) -> &'static str {
    match mode {
        DownloadSourceMode::Default => "DEFAULT",
        DownloadSourceMode::Official => "OFFICIAL",
        DownloadSourceMode::Mirror => "MIRROR",
    }
}

/// `3,827,502` — download counts read better with a separator.
pub(in crate::tui::app) fn format_count(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Human byte size in the units people read: `980 B`, `512.0 KB`, `132.7 MB`.
///
/// One decimal from KB up, and the 1024 step means a value can never print as
/// `1024.0 KB` — it rolls into the next unit instead. This is the only size
/// formatter in the TUI; the pages used to carry five near-copies that
/// disagreed about `MB` versus `MiB`.
pub(in crate::tui::app) fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// `132.7/132.7 MB` when both ends share a unit, `765.7 MB` while the total is
/// still unknown. A task row prints this instead of raw byte counts, which ran
/// to nine digits and left no room for the bar.
pub(in crate::tui::app) fn format_progress_bytes(completed: u64, total: Option<u64>) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let Some(total) = total.filter(|total| *total > 0) else {
        return format_size(completed);
    };
    let mut scale = 1.0_f64;
    let mut unit = 0;
    while (completed.max(total) as f64) / scale >= 1024.0 && unit + 1 < UNITS.len() {
        scale *= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        return format!("{completed}/{total} B");
    }
    format!(
        "{:.1}/{:.1} {}",
        completed as f64 / scale,
        total as f64 / scale,
        UNITS[unit]
    )
}

/// `12.3 MB/s`, using the same 1024-based units as the byte counts next to it.
pub(in crate::tui::app) fn format_speed(bytes_per_second: u64) -> String {
    const UNITS: [&str; 5] = ["B/s", "KB/s", "MB/s", "GB/s", "TB/s"];
    let mut value = bytes_per_second as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes_per_second} B/s")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// `2m 14s` / `45s` / `1h 03m`, rounded up so it never reads `0s` while bytes
/// are still moving.
pub(in crate::tui::app) fn format_eta(remaining: u64, bytes_per_second: u64) -> Option<String> {
    if bytes_per_second == 0 {
        return None;
    }
    let seconds = remaining.div_ceil(bytes_per_second);
    Some(if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3_600 {
        format!("{}m {:02}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h {:02}m", seconds / 3_600, (seconds % 3_600) / 60)
    })
}

/// `12.3 MB/s · 2m 14s`; the ETA appears only when the total is known.
pub(in crate::tui::app) fn format_transfer(
    completed: u64,
    total: Option<u64>,
    bytes_per_second: u64,
) -> Option<String> {
    if bytes_per_second == 0 {
        return None;
    }
    let speed = format_speed(bytes_per_second);
    match total.filter(|total| *total > completed) {
        Some(total) => {
            format_eta(total - completed, bytes_per_second).map(|eta| format!("{speed} · {eta}"))
        }
        None => Some(speed),
    }
}

/// Progress of one task as a ratio, or `None` when it cannot be known yet.
///
/// A finished task counts as complete even when its operation never reported a
/// total: the work is done, and a dashed "unknown" bar on a successful row read
/// as if it were still running.
pub(in crate::tui::app) fn task_ratio(
    state: crate::core::task::TaskState,
    completed: u64,
    total: Option<u64>,
) -> Option<f64> {
    if state == crate::core::task::TaskState::Succeeded {
        return Some(1.0);
    }
    // A job that is still running never reads 100%: the last bytes of a file
    // are not the end of the work, and a full bar next to a spinner reads as
    // "nothing is happening" instead of "almost done".
    total
        .filter(|total| *total > 0)
        .map(|total| (completed as f64 / total as f64).clamp(0.0, 0.99))
}

/// Four cells, right aligned, so the percentages of a task list line up as a
/// column: `100%`, ` 42%`, `  0%`, and `   —` when the total is unknown.
pub(in crate::tui::app) fn percent_label(ratio: Option<f64>) -> String {
    match ratio {
        Some(ratio) => format!("{:>3.0}%", (ratio.clamp(0.0, 1.0) * 100.0).round()),
        None => format!("{:>4}", "—"),
    }
}

pub(in crate::tui::app) fn java_supported_minecraft(major: u32) -> String {
    match major {
        0..=8 => "≤ 1.16".into(),
        9..=16 => "1.17".into(),
        17..=20 => "1.18–1.20".into(),
        _ => "≥ 1.21".into(),
    }
}

pub(in crate::tui::app) fn cycle_enum<T: Copy + PartialEq, const N: usize>(
    current: T,
    direction: isize,
    values: [T; N],
) -> T {
    let values = values.into_iter().collect::<Vec<_>>();
    let index = values
        .iter()
        .position(|value| *value == current)
        .unwrap_or(0);
    let next = if direction < 0 {
        index.checked_sub(1).unwrap_or(values.len() - 1)
    } else {
        (index + 1) % values.len()
    };
    values[next]
}

pub(in crate::tui::app) fn cycle_download_source(
    current: DownloadSourceMode,
    direction: isize,
) -> DownloadSourceMode {
    let values = [
        DownloadSourceMode::Default,
        DownloadSourceMode::Official,
        DownloadSourceMode::Mirror,
    ];
    let index = values
        .iter()
        .position(|value| *value == current)
        .unwrap_or(0);
    let next = if direction < 0 {
        index.checked_sub(1).unwrap_or(values.len() - 1)
    } else {
        (index + 1) % values.len()
    };
    values[next]
}

pub(in crate::tui::app) fn validate_server_address(value: &str) -> Result<()> {
    let Some((host, port)) = value.rsplit_once(':') else {
        return Err(Error::Config("server address must use host:port".into()));
    };
    if host.trim().is_empty() {
        return Err(Error::Config("server address host cannot be empty".into()));
    }
    let port = port
        .parse::<u16>()
        .map_err(|_| Error::Config("server address port must be 1-65535".into()))?;
    if port == 0 {
        return Err(Error::Config("server address port must be 1-65535".into()));
    }
    Ok(())
}

pub(in crate::tui::app) fn split_args(value: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut token_started = false;
    let mut quote = None;
    let mut characters = value.chars().peekable();

    while let Some(character) = characters.next() {
        match quote {
            Some('"') => match character {
                '"' => quote = None,
                '\\' if matches!(characters.peek(), Some('"' | '\\')) => {
                    current.push(characters.next().expect("peeked escape character"));
                    token_started = true;
                }
                _ => {
                    current.push(character);
                    token_started = true;
                }
            },
            Some('\'') => {
                if character == '\'' {
                    quote = None;
                } else {
                    current.push(character);
                    token_started = true;
                }
            }
            None if character.is_whitespace() => {
                if token_started {
                    args.push(std::mem::take(&mut current));
                    token_started = false;
                }
            }
            None if matches!(character, '"' | '\'') => {
                quote = Some(character);
                token_started = true;
            }
            None => {
                current.push(character);
                token_started = true;
            }
            Some(_) => unreachable!("only quote characters are stored"),
        }
    }

    if token_started {
        args.push(current);
    }
    args
}

pub(in crate::tui::app) fn join_args(args: &[String]) -> String {
    args.iter()
        .map(|argument| {
            if argument.is_empty()
                || argument
                    .chars()
                    .any(|character| character.is_whitespace() || matches!(character, '"'))
            {
                let escaped = argument.replace('\\', "\\\\").replace('"', "\\\"");
                format!("\"{escaped}\"")
            } else {
                argument.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(in crate::tui::app) fn format_age(updated_at: u64, now: u64) -> String {
    let delta = now.saturating_sub(updated_at);
    if delta < 60 {
        format!("{delta}s")
    } else if delta < 3_600 {
        format!("{}m", delta / 60)
    } else if delta < 86_400 {
        format!("{}h", delta / 3_600)
    } else {
        format!("{}d", delta / 86_400)
    }
}
