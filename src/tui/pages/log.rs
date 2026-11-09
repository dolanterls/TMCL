//! The log buffer: the lines, the viewport into them, and the rows they take.
//!
//! Wrapping 4000 lines into rows is a `WordWrapper` sweep over every grapheme,
//! and the painter needs the same number twice (clamp the scroll, then draw).
//! Measured at ~400 ms per frame, which froze the page while the game logged.
//! A line is immutable and its row count depends only on the width, so it is
//! measured once and kept; the viewport hands out only the lines it shows.

use std::cell::{Ref, RefCell};
use std::collections::VecDeque;

use ratatui::widgets::{Paragraph, Wrap};

/// Rows each line occupies at a given width.
///
/// `base` is the absolute index of `counts[0]`. The buffer drops lines from the
/// front once it is full, and absolute indices keep the cache aligned with the
/// lines without re-measuring the whole buffer.
#[derive(Debug, Clone, Default)]
struct RowCounts {
    width: u16,
    clip: bool,
    base: usize,
    counts: VecDeque<u32>,
}

/// The rows a painter needs, and nothing more.
#[derive(Debug, Clone, Default)]
pub struct LogWindow {
    /// Lines covering the viewport, starting at `skip_rows` rows into the first.
    pub lines: Vec<String>,
    /// Rows of `lines[0]` that sit above the viewport.
    pub skip_rows: usize,
    /// Rows the whole (filtered) buffer takes at this width.
    pub total_rows: usize,
    /// Row the viewport starts on, counted from the top of the buffer.
    pub from_row: usize,
}

#[derive(Debug, Clone)]
pub struct LogBuffer {
    lines: VecDeque<String>,
    max_lines: usize,
    /// Row the viewport starts on, counted from the top; `None` follows the last
    /// row. Only this field decides whether the page is following, so no flag can
    /// be toggled behind the reader's back.
    top: Option<usize>,
    /// Lines dropped from the front so far. Absolute line index = `dropped` +
    /// index into `lines`.
    dropped: usize,
    rows: RefCell<RowCounts>,
}

impl LogBuffer {
    pub fn new(max_lines: usize) -> Self {
        Self {
            lines: VecDeque::new(),
            max_lines: max_lines.max(1),
            top: None,
            dropped: 0,
            rows: RefCell::new(RowCounts::default()),
        }
    }

    pub fn append(&mut self, line: impl Into<String>) {
        self.lines.push_back(line.into());
        while self.lines.len() > self.max_lines {
            self.lines.pop_front();
            self.dropped += 1;
            // Keep the measurements aligned with the lines instead of throwing
            // them away: at capacity every append drops one, and re-measuring
            // 4000 lines per incoming line is not affordable.
            let rows = self.rows.get_mut();
            if rows.counts.pop_front().is_some() {
                rows.base += 1;
            }
        }
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        let rows = self.rows.get_mut();
        rows.counts.clear();
        rows.base = self.dropped;
    }

    /// Move the viewport by `delta` rows (positive is down), clamped to the
    /// buffer.
    ///
    /// Sitting on the last row *is* following, so the label is derived from the
    /// viewport rather than from a flag that some other input flips. A notch
    /// that cannot move — the buffer is shorter than the page — therefore
    /// leaves the state alone instead of claiming `已暂停` while the view sits
    /// on the newest line.
    pub fn scroll(&mut self, delta: isize, max_offset: usize) {
        let current = self.top.unwrap_or(max_offset).min(max_offset);
        let next = if delta.is_negative() {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current.saturating_add(delta as usize).min(max_offset)
        };
        self.top = if next >= max_offset { None } else { Some(next) };
    }

    /// Follow the tail, or stop where the viewport is.
    ///
    /// Pausing pins the row that is on screen now; the view then stays put while
    /// lines arrive, which is what a reader who asked for `暂停` wants. Measuring
    /// from the bottom would let the page slide away as the game keeps logging.
    pub fn set_follow(&mut self, follow: bool, max_offset: usize) {
        self.top = if follow {
            None
        } else {
            Some(self.top.unwrap_or(max_offset).min(max_offset))
        };
    }

    pub fn toggle_follow(&mut self, max_offset: usize) {
        self.set_follow(!self.follow(), max_offset);
    }

    pub fn follow(&self) -> bool {
        self.top.is_none()
    }

    /// Rows the whole (filtered) buffer takes at `width`.
    pub fn total_rows(&self, query: &str, width: u16, clip: bool) -> usize {
        let counts = self.counts(width, clip);
        self.lines
            .iter()
            .enumerate()
            .filter(|(_, line)| query.is_empty() || line.contains(query))
            .map(|(index, _)| counts.rows_at(index))
            .sum()
    }

    /// The lines covering the viewport.
    ///
    /// `clip` says one row per line: a painter that truncates instead of wrapping
    /// (the two sheet presets) has rows that are lines.
    pub fn window(&self, query: &str, width: u16, height: u16, clip: bool) -> LogWindow {
        let counts = self.counts(width, clip);
        let total_rows = self
            .lines
            .iter()
            .enumerate()
            .filter(|(_, line)| query.is_empty() || line.contains(query))
            .map(|(index, _)| counts.rows_at(index))
            .sum::<usize>();
        let max_offset = total_rows.saturating_sub(usize::from(height));
        let from_row = self.top.unwrap_or(max_offset).min(max_offset);
        let mut window = LogWindow {
            lines: Vec::new(),
            skip_rows: 0,
            total_rows,
            from_row,
        };
        if height == 0 || total_rows == 0 {
            return window;
        }
        let mut seen = 0usize;
        let mut started = false;
        let mut budget = usize::from(height);
        for (index, line) in self.lines.iter().enumerate() {
            if !(query.is_empty() || line.contains(query)) {
                continue;
            }
            let rows = counts.rows_at(index);
            if !started {
                if seen + rows <= from_row {
                    seen += rows;
                    continue;
                }
                window.skip_rows = from_row - seen;
                started = true;
            }
            // Tabs and other control characters are stripped here, at the one
            // place the page hands text to a `Paragraph`: a tab survives as a
            // cell symbol and the backend prints a real one, which moves the
            // terminal to the next tab stop and pushes the rest of the row out
            // of the widget.
            window.lines.push(crate::tui::text::printable(line));
            let used = rows
                - if window.lines.len() == 1 {
                    window.skip_rows
                } else {
                    0
                };
            budget = budget.saturating_sub(used);
            if budget == 0 {
                break;
            }
        }
        window
    }

    /// The measurement cache, extended to cover every line at `width`.
    fn counts(&self, width: u16, clip: bool) -> Ref<'_, RowCounts> {
        {
            let mut cache = self.rows.borrow_mut();
            if cache.width != width || cache.clip != clip || cache.base < self.dropped {
                cache.width = width;
                cache.clip = clip;
                cache.base = self.dropped;
                cache.counts.clear();
            }
            let end = self.dropped + self.lines.len();
            while cache.base + cache.counts.len() < end {
                let index = cache.base + cache.counts.len() - self.dropped;
                let rows = self
                    .lines
                    .get(index)
                    .map_or(1, |line| rows_of(line, width, clip));
                cache.counts.push_back(rows);
            }
        }
        self.rows.borrow()
    }

    pub fn search(&self, query: &str) -> Vec<&str> {
        self.lines
            .iter()
            .filter(|line| line.contains(query))
            .map(String::as_str)
            .collect()
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn matching_lines(&self, query: &str) -> Vec<String> {
        self.lines
            .iter()
            .filter(|line| query.is_empty() || line.contains(query))
            .cloned()
            .collect()
    }

    pub fn contains_query(&self, query: &str) -> bool {
        query.is_empty() || self.lines.iter().any(|line| line.contains(query))
    }
}

impl RowCounts {
    /// Rows line `index` takes. A line that was never measured (the cache is
    /// rebuilt lazily) still answers 1, which is what an empty buffer needs.
    fn rows_at(&self, index: usize) -> usize {
        self.counts
            .get(index)
            .map_or(1, |rows| *rows as usize)
            .max(1)
    }
}

/// Rows one line takes at `width`, measured with the same wrapper the painter
/// uses, so the arithmetic that maps rows back to lines cannot drift from what is
/// drawn.
fn rows_of(line: &str, width: u16, clip: bool) -> u32 {
    if clip || width == 0 {
        return 1;
    }
    // Measured on the same string the view prints (see `window`), so the
    // scroll math and the drawing cannot disagree about a line's length.
    let line = crate::tui::text::printable(line);
    let rows = Paragraph::new(line)
        .wrap(Wrap { trim: false })
        .line_count(width)
        .max(1);
    u32::try_from(rows).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::LogBuffer;

    fn buffer(lines: &[&str], max: usize) -> LogBuffer {
        let mut buffer = LogBuffer::new(max);
        for line in lines {
            buffer.append(*line);
        }
        buffer
    }

    /// A tab in a log line must not reach the `Paragraph`: it would survive as
    /// a cell symbol, and the terminal would turn it into a tab stop that
    /// pushes the rest of the row out of the widget.
    #[test]
    fn a_tab_never_reaches_the_paragraph() {
        let buffer = buffer(&["[stdout] \tMod ID: 'exposure'"], 10);
        let window = buffer.window("", 40, 5, false);
        let line = window.lines.first().expect("one line");
        assert!(
            !line.chars().any(char::is_control),
            "control character in {line:?}"
        );
        assert!(line.contains("Mod ID"), "{line:?}");
    }

    #[test]
    fn keeps_latest_lines_and_supports_search_and_scroll() {
        let mut buffer = LogBuffer::new(2);
        buffer.append("first");
        buffer.append("second");
        buffer.append("third");
        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer.search("third"), vec!["third"]);
        buffer.scroll(-10, 1);
        assert_eq!(buffer.window("", 20, 1, true).from_row, 0);
        assert!(!buffer.follow());
        buffer.scroll(10, 1);
        assert!(buffer.follow());
        buffer.clear();
        assert_eq!(buffer.len(), 0);
    }

    #[test]
    fn scroll_up_moves_to_older_rows_and_scroll_down_returns_to_latest() {
        let buffer = buffer(&["line 0", "line 1", "line 2", "line 3", "line 4"], 10);
        let height = 1;
        let max = buffer.total_rows("", 20, true) - height;
        assert_eq!(max, 4);
        let mut buffer = buffer;

        buffer.scroll(-1, max);
        assert_eq!(buffer.window("", 20, 1, true).from_row, 3);
        buffer.scroll(-10, max);
        assert_eq!(buffer.window("", 20, 1, true).from_row, 0);
        buffer.scroll(1, max);
        assert_eq!(buffer.window("", 20, 1, true).from_row, 1);
        buffer.scroll(10, max);
        assert_eq!(buffer.window("", 20, 1, true).from_row, 4);
        assert!(buffer.follow());
    }

    #[test]
    fn manual_scroll_pauses_follow_and_returning_to_bottom_resumes_it() {
        let mut buffer = buffer(&["line 0", "line 1", "line 2", "line 3", "line 4"], 10);
        assert!(buffer.follow());
        buffer.scroll(-1, 4);
        assert!(!buffer.follow());
        buffer.scroll(10, 4);
        assert!(buffer.follow());
        buffer.set_follow(false, 4);
        assert!(!buffer.follow());
        buffer.toggle_follow(4);
        assert!(buffer.follow());
    }

    /// A notch that cannot move must not claim the reader paused.
    ///
    /// Setting `follow = false` when the offset is already 0 would label the page
    /// `已暂停` with the newest line on screen, and the next notch down would
    /// "resume" a page that never stopped.
    #[test]
    fn a_wheel_notch_that_cannot_move_leaves_follow_alone() {
        let mut buffer = buffer(&["line 0", "line 1"], 10);
        let max = buffer.total_rows("", 20, true).saturating_sub(10);
        assert_eq!(max, 0, "two lines fit a ten-row page");

        buffer.scroll(-3, max);
        assert!(
            buffer.follow(),
            "nothing moved, so the page is still on the tail"
        );
        buffer.scroll(3, max);
        assert!(buffer.follow());
    }

    /// Pausing holds the row that is on screen, not the distance to the bottom.
    #[test]
    fn pausing_pins_the_viewport_while_lines_arrive() {
        let mut buffer = buffer(&["line 0", "line 1", "line 2", "line 3", "line 4"], 10);
        // A three-row page, sitting on the tail: rows 2..4.
        assert_eq!(buffer.window("", 20, 3, true).from_row, 2);
        buffer.toggle_follow(2);
        assert!(!buffer.follow());

        for index in 5..10 {
            buffer.append(format!("line {index}"));
        }
        let window = buffer.window("", 20, 3, true);
        assert_eq!(window.from_row, 2, "the viewport must not slide");
        assert_eq!(window.lines, ["line 2", "line 3", "line 4"]);
    }

    /// The viewport hands the painter one page, however big the buffer is.
    #[test]
    fn the_window_holds_a_page_and_a_multi_row_line_is_skipped_by_row() {
        let mut buffer = LogBuffer::new(100);
        buffer.append("short");
        // 40 characters at width 10 wraps to four rows.
        buffer.append("0123456789".repeat(4));
        buffer.append("tail");

        let window = buffer.window("", 10, 2, false);
        assert_eq!(window.total_rows, 1 + 4 + 1);
        // Following: the last two rows, which are the wrapped line's last row
        // and `tail`.
        assert_eq!(window.from_row, 4);
        assert_eq!(window.skip_rows, 3);
        assert_eq!(window.lines, ["0123456789".repeat(4), "tail".to_owned()]);
    }

    /// A full buffer hands the painter one page, not 4000 lines.
    ///
    /// Wrapping every line twice per frame costs ~400 ms on a 4000-line buffer and
    /// freezes the page while the game logs.
    #[test]
    fn a_full_buffer_hands_the_painter_one_page() {
        let mut buffer = LogBuffer::new(4000);
        let text = "[stdout] a body long enough to wrap on a narrow page, twice over";
        for index in 0..4000 {
            buffer.append(format!("{text} {index}"));
        }

        let window = buffer.window("", 60, 20, false);
        assert!(
            window.total_rows > 4000,
            "each line must wrap, else this proves nothing: {}",
            window.total_rows
        );
        assert!(
            window.lines.len() <= 20,
            "{} lines for a 20-row page",
            window.lines.len()
        );
    }

    #[test]
    fn a_filter_only_counts_matching_lines() {
        let buffer = buffer(&["alpha", "beta", "alpha again"], 10);
        assert_eq!(buffer.total_rows("alpha", 20, true), 2);
        let window = buffer.window("alpha", 20, 10, true);
        assert_eq!(window.lines, ["alpha", "alpha again"]);
        assert_eq!(window.from_row, 0);
    }
}
