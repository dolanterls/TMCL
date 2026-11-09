//! Optional diagnostics on stderr.
//!
//! The launcher has no logging framework, and a dependency for a dozen lines
//! would be more machinery than the feature deserves. What is here is a global
//! switch plus one macro; the interesting part is *where* the hooks are, not how
//! they write.
//!
//! Diagnostics never go to stdout: `tmcl … --json | jq` must keep working while
//! someone is debugging it. The TUI does not enable them either, because stderr is
//! the screen while it runs and one diagnostic line is a torn frame.

use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(false);

/// Turns diagnostics on or off for the rest of the process.
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Whether the environment asks for diagnostics (`TMCL_LOG=debug|trace|1`).
///
/// Anything that is not explicitly off counts as on: a user who sets
/// `TMCL_LOG=info` wants to see what happens, and inventing a level system for
/// two call sites would be pretending to more than exists.
pub fn wanted_by_environment() -> bool {
    match std::env::var("TMCL_LOG") {
        Ok(value) => !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "" | "0" | "off" | "false" | "none"
        ),
        Err(_) => false,
    }
}

/// Writes one diagnostic line, when diagnostics are on.
pub fn emit(scope: &str, message: std::fmt::Arguments<'_>) {
    if enabled() {
        eprintln!("[tmcl {scope}] {message}");
    }
}

/// `diagnostic!("http", "GET {url}")`: a no-op unless diagnostics are on.
///
/// The formatting arguments are evaluated only when the switch is on, so a hot path
/// does not pay for messages nobody asked to see.
macro_rules! diagnostic {
    ($scope:literal, $($arg:tt)*) => {
        if $crate::core::diagnostics::enabled() {
            $crate::core::diagnostics::emit($scope, format_args!($($arg)*));
        }
    };
}

pub(crate) use diagnostic;

#[cfg(test)]
mod tests {
    use super::{emit, enabled, set_enabled, wanted_by_environment};

    /// The switch is process-wide, so this is one test rather than several that
    /// would race each other over the same atomic.
    ///
    /// `TMCL_LOG` is read but never written here: mutating the environment is
    /// unsafe in edition 2024 and would reach the other tests.
    #[test]
    fn the_switch_controls_whether_a_line_is_written() {
        set_enabled(false);
        assert!(!enabled());
        // The macro is a plain `if`: evaluating its arguments while disabled
        // must be harmless.
        diagnostic!("test", "{}", 1 + 1);

        set_enabled(true);
        assert!(enabled());
        emit("test", format_args!("enabled"));

        set_enabled(false);
        assert!(!enabled());

        // Whatever the environment says, asking must not panic or block.
        let _ = wanted_by_environment();
    }
}
