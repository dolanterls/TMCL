//! Status toast bookkeeping, shared by every preset.
//!
//! Only the queue lives here: which statuses are worth a card, how many are
//! kept, and when they expire. Drawing stays with the preset that draws it,
//! whether it slides in (`creeper`) or appears at once (`nightlight`).

use std::time::{Duration, Instant};

use crate::tui::widgets::status::StatusKind;

use crate::tui::app::App;

/// How long a status message stays on screen as a toast.
pub(in crate::tui::app) const TOAST_TTL: Duration = Duration::from_secs(5);

/// One toast that should be on screen right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::tui::app) struct Toast {
    pub text: String,
    pub kind: StatusKind,
    /// Seconds since the message first appeared; drives the exit animation.
    pub age_millis: u64,
}

impl Toast {
    pub(in crate::tui::app) fn age_seconds(&self) -> f64 {
        self.age_millis as f64 / 1000.0
    }
}

/// Update the queue from the current status and return what is still visible.
///
/// Routine progress messages stay in the footer; only outcomes worth noticing
/// float up as cards. Otherwise every list refresh would pop a toast.
pub(in crate::tui::app) fn collect(app: &App, ttl: Duration) -> Vec<Toast> {
    let mut log = app.status_toasts.borrow_mut();
    let text = app.status.as_str();
    let worth_a_toast = matches!(
        app.status.kind(),
        StatusKind::Completed | StatusKind::Warning | StatusKind::Failed
    );
    if worth_a_toast && !text.is_empty() && !log.back().is_some_and(|entry| entry.0 == text) {
        log.push_back((text.to_owned(), app.status.kind(), Instant::now()));
    }
    while log.len() > 3 {
        log.pop_front();
    }
    // Drop expired cards but keep the newest one: it is the memory that stops
    // an unchanged status from being announced again on the next frame.
    while log.len() > 1 && log.front().is_some_and(|entry| entry.2.elapsed() > ttl) {
        log.pop_front();
    }
    log.iter()
        .filter(|entry| entry.2.elapsed() <= ttl)
        .map(|(text, kind, born)| Toast {
            text: text.clone(),
            kind: *kind,
            age_millis: born.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routine_statuses_never_become_toasts() {
        let mut app = App::new();
        app.status = crate::tui::app::Status::info("scanning");
        assert!(collect(&app, TOAST_TTL).is_empty());
    }

    #[test]
    fn an_outcome_is_queued_once() {
        let mut app = App::new();
        app.status = crate::tui::app::Status::success("done");
        let first = collect(&app, TOAST_TTL);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].text, "done");
        // The same text on the next frame must not stack up.
        assert_eq!(collect(&app, TOAST_TTL).len(), 1);
    }

    #[test]
    fn the_queue_keeps_at_most_three_entries() {
        let mut app = App::new();
        for index in 0..5 {
            app.status = crate::tui::app::Status::success(format!("done {index}"));
            collect(&app, TOAST_TTL);
        }
        assert_eq!(app.status_toasts.borrow().len(), 3);
    }
}
