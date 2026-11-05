//! Where a command's result goes and how it is decorated.
//!
//! Two independent terminal tests decide two different things:
//!
//! | test | controls | why |
//! |---|---|---|
//! | `stdout.is_terminal()` | colour | `tmcl ... > out.txt` must not contain escape sequences |
//! | `stderr.is_terminal()` | progress shape | progress is written to stderr |
//!
//! Deriving one from the other breaks the common `> file` case: stdout is
//! redirected while stderr is still the terminal.

use std::io::IsTerminal;

use serde::Serialize;

use crate::cli::args::Cli;
use crate::core::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Plain,
    Error,
    Warning,
    Success,
    Accent,
}

impl Style {
    fn code(self) -> &'static str {
        match self {
            Self::Plain => "",
            Self::Error => "\u{1b}[31m",
            Self::Warning => "\u{1b}[33m",
            Self::Success => "\u{1b}[32m",
            Self::Accent => "\u{1b}[36m",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OutputOptions {
    pub json: bool,
    /// Hide progress output; the final result still prints.
    pub quiet: bool,
    /// Print the plan and change nothing.
    pub dry_run: bool,
    /// Decided from `--no-color` and whether stdout is a terminal.
    pub color: bool,
}

impl OutputOptions {
    pub fn from_cli(cli: &Cli) -> Self {
        Self {
            json: cli.json,
            quiet: cli.quiet,
            dry_run: cli.dry_run,
            color: !cli.no_color && std::io::stdout().is_terminal(),
        }
    }

    /// Whether progress may use carriage returns. Kept here so both the
    /// renderer and the commands ask the same question.
    pub fn progress_is_terminal() -> bool {
        std::io::stderr().is_terminal()
    }

    /// The shape progress output takes for this invocation.
    pub fn progress_mode(&self) -> crate::cli::progress::ProgressMode {
        crate::cli::progress::ProgressMode::choose(
            self.json,
            self.quiet,
            Self::progress_is_terminal(),
        )
    }

    pub fn paint(&self, text: &str, style: Style) -> String {
        if !self.color || style == Style::Plain {
            return text.to_owned();
        }
        format!("{}{text}\u{1b}[0m", style.code())
    }

    pub fn print<T: Serialize>(&self, value: &T, text: impl FnOnce() -> String) -> Result<()> {
        if self.json {
            println!("{}", serde_json::to_string_pretty(value)?);
        } else {
            println!("{}", text());
        }
        Ok(())
    }

    pub fn error_message(&self, error: &Error) -> String {
        if self.json {
            serde_json::json!({
                "error": error.to_string(),
                "category": format!("{:?}", error.category()),
            })
            .to_string()
        } else {
            error.to_string()
        }
    }
}

/// Terminal columns `text` occupies, counting CJK and emoji as two cells.
///
/// `format!("{:<width$}")` counts characters, so a table padded with it puts a
/// Chinese name two cells to the left of an ASCII one per character. Lists are
/// read with the eye, so they are padded by display width instead.
pub fn width(text: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(text)
}

/// `text` followed by spaces up to `cells` terminal columns.
pub fn pad(text: &str, cells: usize) -> String {
    let used = width(text);
    if used >= cells {
        text.to_owned()
    } else {
        format!("{text}{}", " ".repeat(cells - used))
    }
}

/// Spaces followed by `text`, for a right-aligned numeric column.
pub fn pad_start(text: &str, cells: usize) -> String {
    let used = width(text);
    if used >= cells {
        text.to_owned()
    } else {
        format!("{}{text}", " ".repeat(cells - used))
    }
}

/// Columns a table may use, or `None` when stdout is redirected.
///
/// A file has no width, and a script reading the output wants the whole path,
/// so the only place a line is cut is a terminal that would otherwise wrap it.
pub fn table_width() -> Option<usize> {
    if !std::io::stdout().is_terminal() {
        return None;
    }
    crossterm::terminal::size()
        .ok()
        .map(|(columns, _)| usize::from(columns))
}

/// `text` cut from the left so it fits `cells`, keeping the tail.
///
/// The end of a path names the instance; the beginning only names the game
/// directory, which the reader already knows.
pub fn truncate_start(text: &str, cells: usize) -> String {
    if width(text) <= cells {
        return text.to_owned();
    }
    if cells == 0 {
        return String::new();
    }
    let mut kept = Vec::new();
    let mut used = width("…");
    for character in text.chars().rev() {
        let cells_of = unicode_width::UnicodeWidthChar::width(character).unwrap_or(0);
        if used + cells_of > cells {
            break;
        }
        used += cells_of;
        kept.push(character);
    }
    kept.reverse();
    let mut out = String::from("…");
    out.extend(kept);
    out
}

/// `text` cut from the right so it fits `cells`, keeping the head.
pub fn truncate_end(text: &str, cells: usize) -> String {
    if width(text) <= cells {
        return text.to_owned();
    }
    if cells == 0 {
        return String::new();
    }
    let mut kept = String::new();
    let mut used = width("…");
    for character in text.chars() {
        let cells_of = unicode_width::UnicodeWidthChar::width(character).unwrap_or(0);
        if used + cells_of > cells {
            break;
        }
        used += cells_of;
        kept.push(character);
    }
    kept.push('…');
    kept
}

#[cfg(test)]
mod tests {
    use super::{OutputOptions, Style};

    fn options(color: bool) -> OutputOptions {
        OutputOptions {
            json: false,
            quiet: false,
            dry_run: false,
            color,
        }
    }

    #[test]
    fn plain_text_never_carries_escape_sequences() {
        assert_eq!(options(false).paint("done", Style::Success), "done");
        assert!(!options(false).paint("x", Style::Error).contains('\u{1b}'));
    }

    #[test]
    fn colour_wraps_and_resets() {
        let painted = options(true).paint("failed", Style::Error);
        assert!(painted.starts_with("\u{1b}[31m"), "{painted:?}");
        assert!(painted.ends_with("\u{1b}[0m"), "{painted:?}");
        assert!(painted.contains("failed"));
    }

    #[test]
    fn plain_style_stays_untouched_even_with_colour() {
        assert_eq!(options(true).paint("value", Style::Plain), "value");
    }

    #[test]
    fn a_captured_stdout_is_never_coloured() {
        use clap::Parser;

        // The test harness captures stdout, so this is the non-TTY case: the
        // decision has to happen in `from_cli`, not in `paint`.
        let cli =
            crate::cli::args::Cli::try_parse_from(["tmcl", "instance", "list"]).expect("parse");
        let options = OutputOptions::from_cli(&cli);
        assert!(!options.color, "a redirected stdout must not be coloured");
        assert!(!options.paint("x", Style::Error).contains('\u{1b}'));
    }

    #[test]
    fn quiet_and_json_decide_the_progress_shape() {
        let quiet = OutputOptions {
            quiet: true,
            ..options(false)
        };
        assert_eq!(
            quiet.progress_mode(),
            crate::cli::progress::ProgressMode::Silent
        );

        let json = OutputOptions {
            json: true,
            ..options(false)
        };
        // stderr is captured by the test harness, so a JSON invocation lands on
        // Ndjson regardless of the terminal.
        assert_eq!(
            json.progress_mode(),
            crate::cli::progress::ProgressMode::Ndjson
        );
    }

    #[test]
    fn json_errors_carry_the_category() {
        let error = crate::core::error::Error::Config("bad config".into());
        let json = OutputOptions {
            json: true,
            ..options(false)
        };
        let message = json.error_message(&error);
        assert!(message.starts_with('{'), "{message}");
        assert!(message.contains("\"category\""), "{message}");
        assert!(message.contains("bad config"), "{message}");
    }

    #[test]
    fn display_width_counts_double_width_characters_as_two_cells() {
        assert_eq!(super::width("abc"), 3);
        assert_eq!(super::width("中文"), 4);
        assert_eq!(super::width("中文abc"), 7);
        assert_eq!(super::width("🎮"), 2);

        // Padding uses cells, not characters, so the next column starts at the
        // same place for both kinds of name.
        assert_eq!(super::width(&super::pad("中文", 6)), 6);
        assert_eq!(super::width(&super::pad("abcdef", 6)), 6);
        assert_eq!(super::pad("abcdef", 3), "abcdef");
        assert_eq!(super::pad_start("1 B", 5), "  1 B");
    }

    #[test]
    fn truncation_keeps_the_end_of_a_path() {
        assert_eq!(super::truncate_start("abcdef", 4), "…def");
        assert_eq!(super::truncate_start("abcdef", 6), "abcdef");
        assert_eq!(super::truncate_start("abcdef", 7), "abcdef");
        // Two cells per character, so the ellipsis plus two characters fill
        // five cells exactly.
        assert_eq!(super::truncate_start("中文路径", 5), "…路径");
        assert_eq!(super::width(&super::truncate_start("中文路径", 5)), 5);
        assert_eq!(super::truncate_start("abc", 0), "");
    }

    #[test]
    fn truncation_keeps_the_head_of_a_name() {
        assert_eq!(super::truncate_end("abcdef", 4), "abc…");
        assert_eq!(super::truncate_end("abcdef", 6), "abcdef");
        assert_eq!(super::truncate_end("中文路径", 5), "中文…");
        assert_eq!(super::width(&super::truncate_end("中文路径", 5)), 5);
        assert_eq!(super::truncate_end("abc", 0), "");
        // A cell too narrow for the ellipsis still has to fit.
        assert_eq!(super::truncate_end("abcdef", 1), "…");
    }
}
