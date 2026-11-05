//! The gate every destructive command goes through.
//!
//! Three ways to say yes, one way to say no:
//!
//! | `--yes` | `--dry-run` | stderr is a terminal | result |
//! |---|---|---|---|
//! | no | no | yes | print the plan, ask, read one line |
//! | no | no | no | refuse, exit 2 |
//! | yes | any | any | proceed |
//! | any | yes | any | print the plan, change nothing |
//!
//! The plan text has to carry numbers ("1,284 files, 312 MB"), because "this
//! cannot be undone" tells nobody what they are about to lose.

use std::io::{self, Write};

use crate::core::error::{Error, Result};

#[derive(Debug, Clone, Copy)]
pub struct Consent {
    pub yes: bool,
    pub dry_run: bool,
    /// stderr is a terminal, so a prompt can be answered.
    pub interactive: bool,
}

impl Consent {
    /// `yes` comes from the command's own `--yes`: the flag is only offered by
    /// commands that can destroy something, so it does not belong in the global
    /// option set.
    pub fn new(yes: bool, output: &crate::cli::output::OutputOptions) -> Self {
        Self {
            yes,
            dry_run: output.dry_run,
            interactive: crate::cli::output::OutputOptions::progress_is_terminal(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Proceed,
    /// The caller must print its plan and change nothing.
    Dry,
}

/// Decide without touching standard input or output.
///
/// `answer` is only consulted when a prompt would have been shown, which makes
/// the whole table testable.
pub fn decide(consent: &Consent, action: &str, answer: Option<&str>) -> Result<Decision> {
    if consent.dry_run {
        return Ok(Decision::Dry);
    }
    if consent.yes {
        return Ok(Decision::Proceed);
    }
    if !consent.interactive {
        return Err(refusal(action));
    }
    match answer.map(|value| value.trim().to_ascii_lowercase()) {
        Some(value) if value == "y" || value == "yes" => Ok(Decision::Proceed),
        _ => Err(cancelled(action)),
    }
}

/// Ask, then decide.
///
/// Everything here writes to stderr: the plan is part of the conversation with
/// the user, not part of the command's result. Keeping it off stdout is what
/// lets `--json` emit exactly one JSON value even when a prompt is shown.
pub fn require(consent: &Consent, action: &str, plan: impl FnOnce() -> String) -> Result<Decision> {
    let plan_text = plan();
    let prompts = consent.interactive && !consent.yes && !consent.dry_run;
    if consent.interactive && !consent.yes {
        eprintln!("{plan_text}");
    }
    let answer = if prompts { Some(read_answer()?) } else { None };
    let decision = decide(consent, action, answer.as_deref())?;
    if decision == Decision::Dry {
        if !consent.interactive {
            eprintln!("{plan_text}");
        }
        eprintln!("Dry run: nothing was changed.");
    }
    Ok(decision)
}

fn read_answer() -> Result<String> {
    eprint!("Proceed? [y/N] ");
    io::stderr().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(answer)
}

fn refusal(action: &str) -> Error {
    Error::Config(format!(
        "refusing to {action}: pass --yes to proceed, or --dry-run to see the plan first"
    ))
}

fn cancelled(action: &str) -> Error {
    Error::Config(format!("{action} cancelled"))
}

#[cfg(test)]
mod tests {
    use super::{Consent, Decision, decide};

    fn consent(yes: bool, dry_run: bool, interactive: bool) -> Consent {
        Consent {
            yes,
            dry_run,
            interactive,
        }
    }

    #[test]
    fn a_non_interactive_caller_must_choose_yes_or_dry_run() {
        let error = decide(&consent(false, false, false), "delete the instance", None)
            .expect_err("refused");

        let message = error.to_string();
        assert!(message.contains("--yes"), "{message}");
        assert!(message.contains("--dry-run"), "{message}");
    }

    #[test]
    fn dry_run_wins_over_everything() {
        assert_eq!(
            decide(&consent(true, true, true), "delete", Some("n")).expect("dry"),
            Decision::Dry
        );
    }

    #[test]
    fn yes_proceeds_without_reading_an_answer() {
        assert_eq!(
            decide(&consent(true, false, true), "delete", None).expect("proceed"),
            Decision::Proceed
        );
    }

    #[test]
    fn an_interactive_answer_decides() {
        assert_eq!(
            decide(&consent(false, false, true), "delete", Some("y")).expect("yes"),
            Decision::Proceed
        );
        assert_eq!(
            decide(&consent(false, false, true), "delete", Some("YES")).expect("yes"),
            Decision::Proceed
        );
        assert!(decide(&consent(false, false, true), "delete", Some("n")).is_err());
        assert!(decide(&consent(false, false, true), "delete", Some("")).is_err());
        assert!(decide(&consent(false, false, true), "delete", None).is_err());
    }
}
