//! Preset views: one directory per preset, shared pieces one level up. A preset
//! directory holds only what that preset alone uses; anything two presets need
//! lives in `crate::tui`. Presets never reference each other, and both rules are
//! enforced by `crate::tui::app::tests::structure`.
//!
//! Every file here is a child of `crate::tui::app`, so it reads `App`'s private
//! fields directly instead of threading a view model through.

pub(crate) mod classic;
/// Per-page dispatch, the shared layout geometry it needs, and the `classic`
/// frame skeleton that sits between them.
mod diagnosis;
mod dispatch;
pub(crate) mod nightlight;
/// Detail and help painters shared by every preset, parameterised by theme.
mod overlays;
/// Paper: a light page with hairline rules. Overview and instances are drawn
/// here; every other page falls back to `classic` for now.
pub(crate) mod paper;
/// What each preset says about its own paint: the fills `跟随终端` withdraws.
pub(crate) mod sheet;
/// Shelf: a cover-led library. Deep indigo, one lavender accent, no cards.
pub(crate) mod shelf;
/// The instance workspace page, shared by every preset that draws one.
pub(in crate::tui::app) mod workspace;

use ratatui::style::Color;

use super::*;
use crate::tui::palette::{BLUE, CYAN, GOLD, GREEN, LIME, PINK, PURPLE};

const PARAGRAPH_MARK: char = '\u{200b}';

/// Splits a `label: value` detail row. Chinese catalogs use the full-width
/// colon, so both separators are accepted; the label never keeps its colon.
pub(in crate::tui::app) fn split_detail_label(text: &str) -> Option<(&str, &str)> {
    text.split_once(": ").or_else(|| text.split_once('：'))
}

/// Strips the paragraph mark from a detail row, if present.
pub(in crate::tui::app) fn detail_paragraph(text: &str) -> Option<&str> {
    text.strip_prefix(PARAGRAPH_MARK)
}

/// Scrollbar state for a line-based detail pane.
///
/// Ratatui spreads the thumb over `content + viewport` units, so a document
/// scrolled by line has to report `max_scroll + 1` as its content length.
/// Passing the raw line count makes the thumb almost as long as the track and
/// leaves it at the top even after the pane reached its last line.
pub(in crate::tui::app) fn pane_scrollbar_state(
    total: usize,
    viewport: usize,
    scroll: usize,
) -> ScrollbarState {
    let max_scroll = total.saturating_sub(viewport);
    ScrollbarState::new(max_scroll.saturating_add(1))
        .position(scroll.min(max_scroll))
        .viewport_content_length(viewport)
}

/// One line of the keyboard help.
///
/// The content is built once for all three presets and only styled per preset,
/// so a key added to one help page cannot be missing from another.
pub(in crate::tui::app) enum HelpLine {
    Header(String),
    Row { key: String, action: String },
    Blank,
}

impl HelpLine {
    /// Cells the row takes when painted, key column included.
    fn cells(&self) -> usize {
        match self {
            Self::Header(title) => crate::tui::palette::width(title),
            // `max` rather than a sum: a longer key widens the column for every
            // row, which is what the painter's padding does too.
            Self::Row { key, action } => {
                HELP_KEY_COLUMN.max(crate::tui::palette::width(key))
                    + 1
                    + crate::tui::palette::width(action)
            }
            Self::Blank => 0,
        }
    }
}

/// Column the action text starts at: the widest key (`Tab / ⇧Tab`) plus room
/// to breathe. Shared so the three presets line up and so the width
/// measurement below matches what gets painted.
pub(in crate::tui::app) const HELP_KEY_COLUMN: usize = 12;

/// Rows of help text one card shows before it scrolls.
///
/// The reference lists every binding of the current page. Sizing the card to
/// all of them pushed the box past the bottom of the terminal and left the
/// last section sitting on the border, so the height stops here and the rest
/// scrolls.
const HELP_VIEWPORT_ROWS: u16 = 18;

/// Cells between the card edge and the text: the border (or the card's own
/// padding) plus one column of breathing room on each side.
const HELP_PAD: u16 = 2;

/// One column reserved for the scrollbar on the right.
const HELP_GUTTER: u16 = 1;

/// Where the help card sits and how far its body can scroll.
pub(in crate::tui::app) struct HelpLayout {
    pub rect: Rect,
    pub body: Rect,
    pub scroll: usize,
    pub max_scroll: usize,
    pub total: usize,
}

/// Card rectangle for [`HelpLayout`], measured from the rows it will hold.
///
/// Width follows the longest row: a fixed 76-column card left two thirds of
/// the box empty on every page with short labels. Height stops at
/// [`HELP_VIEWPORT_ROWS`], so the card is the same size whether the page lists
/// six keys or twenty-six.
///
/// The head takes three rows — one of air, the title, a hairline — which is
/// what a `paper` dialog spends on its own head; the bordered presets get the
/// same extra row of air under their title.
pub(in crate::tui::app) fn help_rect(area: Rect, lines: &[HelpLine]) -> Rect {
    let widest = lines.iter().map(HelpLine::cells).max().unwrap_or(0);
    let width = u16::try_from(widest)
        .unwrap_or(u16::MAX)
        .saturating_add(HELP_PAD * 2 + HELP_GUTTER);
    let rows = u16::try_from(lines.len())
        .unwrap_or(u16::MAX)
        .min(HELP_VIEWPORT_ROWS);
    crate::tui::chrome::popup_rect_measured(area, width, rows.saturating_add(HELP_PAD * 2 + 1))
}

/// Text viewport inside [`help_rect`].
///
/// Two cells of gutter on each side, three rows down for the title and the
/// hairline under it, two rows up from the bottom.
fn help_body(rect: Rect) -> Rect {
    Rect {
        x: rect.x.saturating_add(HELP_PAD),
        y: rect.y.saturating_add(3),
        width: rect.width.saturating_sub(HELP_PAD * 2 + HELP_GUTTER),
        height: rect.height.saturating_sub(5),
    }
}

/// Right-hand scrollbar for an overflowing help body, in the card's gutter.
pub(in crate::tui::app) fn help_scrollbar(
    frame: &mut Frame<'_>,
    layout: &HelpLayout,
    style: ratatui::style::Style,
) {
    if layout.max_scroll == 0 || layout.body.height == 0 {
        return;
    }
    let mut state =
        pane_scrollbar_state(layout.total, usize::from(layout.body.height), layout.scroll);
    let track = Rect::new(layout.body.right(), layout.body.y, 1, layout.body.height);
    frame.render_stateful_widget(
        ratatui::widgets::Scrollbar::new(ratatui::widgets::ScrollbarOrientation::VerticalRight)
            .style(style)
            .begin_symbol(None)
            .end_symbol(None),
        track,
        &mut state,
    );
}

/// A scrollbar one column past `body`, with a subdued track and an accent thumb.
///
/// `paper` and `shelf` put it in the page margin rather than inside the text: a
/// record that stops at the bottom edge gives no sign that the wheel continues
/// it, and an in-body bar costs one column of text.
pub(in crate::tui::app) fn margin_scrollbar(
    frame: &mut Frame<'_>,
    body: Rect,
    total: usize,
    scroll: usize,
    track: Color,
    thumb: Color,
    bg: Color,
) {
    let viewport = usize::from(body.height);
    if body.height == 0 || total <= viewport {
        return;
    }
    let x = body.right().min(frame.area().right().saturating_sub(1));
    let rail = Rect::new(x, body.y, 1, body.height);
    let mut state = pane_scrollbar_state(total, viewport, scroll);
    frame.render_stateful_widget(
        ratatui::widgets::Scrollbar::new(ratatui::widgets::ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"))
            .thumb_symbol("▌")
            .track_style(ratatui::style::Style::default().fg(track).bg(bg))
            .thumb_style(ratatui::style::Style::default().fg(thumb).bg(bg)),
        rail,
        &mut state,
    );
}

/// Which band of the task list a job belongs to.
///
/// `paper` and `shelf` both set a task list as groups of entries, so the
/// grouping lives here rather than in either preset's folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::tui::app) enum TaskGroup {
    Running,
    Queued,
    Finished,
}

/// A log line's severity, spelled as a letter by every preset that prints one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::tui::app) enum LogLevel {
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub(in crate::tui::app) fn letter(self) -> &'static str {
        match self {
            Self::Info => "I",
            Self::Warn => "W",
            Self::Error => "E",
        }
    }

    /// The colour a preset paints this level with. The three inks are passed in
    /// because each preset owns its palette; the mapping is shared.
    pub(in crate::tui::app) fn color_in(self, info: Color, warn: Color, error: Color) -> Color {
        match self {
            Self::Info => info,
            Self::Warn => warn,
            Self::Error => error,
        }
    }
}

/// `[stderr] boot failed` → (`E`, `boot failed`).
///
/// Shared by `paper` and `shelf`: two log pages disagreeing about which lines
/// count as errors would make the same buffer read differently per skin.
pub(in crate::tui::app) fn split_log_level(line: &str) -> (LogLevel, &str) {
    for (prefix, level) in [
        ("[stderr]", LogLevel::Error),
        ("[crash]", LogLevel::Error),
        ("[stdout]", LogLevel::Info),
    ] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return (level, rest.trim_start());
        }
    }
    let lower = line.to_ascii_lowercase();
    if lower.contains("warn") {
        (LogLevel::Warn, line)
    } else if lower.contains("error") || lower.contains("failed") {
        (LogLevel::Error, line)
    } else {
        (LogLevel::Info, line)
    }
}

impl TaskGroup {
    pub(in crate::tui::app) fn of(state: crate::core::task::TaskState) -> Self {
        use crate::core::task::TaskState;
        match state {
            TaskState::Running | TaskState::Paused => Self::Running,
            TaskState::Queued => Self::Queued,
            TaskState::Succeeded | TaskState::Failed | TaskState::Cancelled => Self::Finished,
        }
    }

    pub(in crate::tui::app) fn label_key(self) -> &'static str {
        match self {
            Self::Running => "task.running",
            Self::Queued => "task.queued",
            Self::Finished => "task.finished",
        }
    }
}

impl App {
    /// The keyboard reference shown by `?`.
    ///
    /// Three sections: what works everywhere, what this page adds, and what the
    /// mouse does. The page section comes from `page_hints()`, so the help is
    /// contextual instead of a wall of every binding in the app.
    ///
    /// Keys are not de-duplicated against the general section: `Enter` means
    /// "activate" everywhere and something specific on a page (`Details` on the
    /// instance list), and dropping the page's meaning would hide the one the
    /// user is looking for.
    pub(in crate::tui::app) fn help_lines(&self) -> Vec<HelpLine> {
        let text = |key: &str| self.i18n.text(key);
        let mut lines = vec![HelpLine::Header(text("help.section.general"))];
        let general = [
            ("Tab / ⇧Tab", "help.switch_page"),
            ("↑ ↓ / j k", "help.move"),
            ("Enter", "help.activate"),
            ("Esc", "help.back_cancel"),
            ("q", "help.quit_when_idle"),
            ("?", "help.contextual"),
        ];
        for (key, action) in general {
            lines.push(HelpLine::Row {
                key: key.to_owned(),
                action: text(action),
            });
        }

        let page: Vec<(String, String)> = self
            .page_hints()
            .into_iter()
            .chain(self.help_only_hints())
            .map(|hint| (hint.key, self.i18n.text_or_raw(&hint.action)))
            .collect();
        if !page.is_empty() {
            lines.push(HelpLine::Blank);
            lines.push(HelpLine::Header(text("help.section.page")));
            for (key, action) in page {
                lines.push(HelpLine::Row { key, action });
            }
        }

        // The diagnosis card has no tab of its own, so the reference is the
        // only place its keys are written down before something crashes.
        lines.push(HelpLine::Blank);
        lines.push(HelpLine::Header(text("help.section.diagnosis")));
        for (key, action) in [
            ("F3", "help.diagnosis_toggle"),
            ("Enter", "help.diagnosis_first"),
            ("E", "help.diagnosis_export"),
            ("L", "help.diagnosis_locate"),
        ] {
            lines.push(HelpLine::Row {
                key: key.to_owned(),
                action: text(action),
            });
        }

        lines.push(HelpLine::Blank);
        lines.push(HelpLine::Header(text("help.section.mouse")));
        for action in ["help.click_sidebar", "help.click_row", "help.mouse_wheel"] {
            lines.push(HelpLine::Row {
                key: String::new(),
                action: text(action),
            });
        }
        lines
    }

    /// Keys a page answers to that the footer does not print.
    ///
    /// `PageUp/PageDown` turns the download centre list and the status line
    /// already names them. A footer cap would push the hint band onto a second
    /// row at 120 columns, so the key is listed in the help reference instead.
    pub(in crate::tui::app) fn help_only_hints(&self) -> Vec<crate::tui::chrome::Hint> {
        match self.page {
            Page::DownloadCenter => vec![self.hint("PgUp/PgDn", "hint.page")],
            _ => Vec::new(),
        }
    }
}

impl App {
    /// Geometry, scroll window and wheel target for the keyboard help.
    pub(in crate::tui::app) fn help_layout(&self, area: Rect, lines: &[HelpLine]) -> HelpLayout {
        // One card for every preset: width follows the longest row, height
        // stops at `HELP_VIEWPORT_ROWS`. Printing the reference inline, as
        // `paper` did, hides the running head and the key band that close it.
        let rect = help_rect(area, lines);
        let body = help_body(rect);
        let total = lines.len();
        let max_scroll = total.saturating_sub(usize::from(body.height));
        self.hitmap.scroll_on(
            crate::tui::hit::Layer::Help,
            rect,
            crate::tui::action::ScrollTarget::HelpBody,
        );
        HelpLayout {
            rect,
            body,
            scroll: self.help_scroll.min(max_scroll),
            max_scroll,
            total,
        }
    }

    /// The same window, for the code that runs between frames.
    ///
    /// The key handler and the footer hint need the maximum without a paint, and
    /// both derive it from the same functions the painter uses.
    fn help_window(&self) -> (u16, usize) {
        let lines = self.help_lines();
        let area = Rect::new(0, 0, self.terminal_width, self.terminal_height);
        let body = help_body(help_rect(area, &lines));
        let max_scroll = lines.len().saturating_sub(usize::from(body.height));
        (body.height, max_scroll)
    }

    pub(in crate::tui::app) fn help_max_scroll(&self) -> usize {
        self.help_window().1
    }

    /// Rows one PageUp/PageDown covers: the viewport minus one row of overlap,
    /// so a section header is never the only thing on screen.
    pub(in crate::tui::app) fn help_page_rows(&self) -> isize {
        (self.help_window().0 as isize).saturating_sub(1).max(1)
    }
}

impl App {
    pub(in crate::tui::app) fn preset(&self) -> UiPreset {
        self.config.state.ui.preset
    }

    /// Refresh period for the event loop.
    pub(crate) fn refresh_millis(&self) -> u64 {
        250
    }

    /// Seconds since launch, or a frozen 0 when motion is disabled.
    pub(in crate::tui::app) fn anim_time(&self) -> f64 {
        if self.motion {
            self.started.elapsed().as_secs_f64()
        } else {
            0.0
        }
    }

    pub(in crate::tui::app) fn page_accent(&self) -> Color {
        match self.preset() {
            UiPreset::Classic => page_accent(self.page),
            UiPreset::Nightlight => self.nightlight_palette().accent,
            UiPreset::Paper => self.paper_palette().accent,
            UiPreset::Shelf => self.shelf_palette().accent,
        }
    }

    /// Cells the channel column takes: its own header or the widest channel
    /// label, whichever is longer. `Stable` / `Beta` / `Alpha` are six cells;
    /// the Chinese labels are six too.
    ///
    /// Shared by every preset that prints the file table: two presets measuring
    /// their own columns would drift apart, and the same file would sit at two
    /// different offsets depending on the skin.
    pub(in crate::tui::app) fn channel_column_width(&self) -> u16 {
        (crate::tui::palette::width(&self.i18n.text("download.channel")) as u16).clamp(8, 12)
    }

    /// Cells the dependency column takes: its header, or room for a two-digit
    /// count.
    pub(in crate::tui::app) fn dependencies_column_width(&self) -> u16 {
        (crate::tui::palette::width(&self.i18n.text("download.dependencies")) as u16).max(3)
    }

    /// Width of the compatibility column: the longest status word.
    ///
    /// A fixed ten cells cut `Incompatible` (twelve) to `Incompati…` and gave
    /// three-cell Chinese labels ten. Both cases are measured instead.
    pub(in crate::tui::app) fn compatibility_column_width(&self) -> u16 {
        [
            "download.compatible",
            "download.unknown_compatibility",
            "download.incompatible",
        ]
        .iter()
        .map(|key| crate::tui::palette::width(&self.i18n.text(key)) as u16)
        .max()
        .unwrap_or(10)
        .max(6)
    }

    /// `微软` / `离线`.
    pub(in crate::tui::app) fn account_kind_label(
        &self,
        kind: crate::core::account::models::AccountKind,
    ) -> String {
        use crate::core::account::models::AccountKind;
        self.i18n.text(match kind {
            AccountKind::Microsoft => "account.kind.microsoft",
            AccountKind::Offline => "account.kind.offline",
        })
    }

    /// What the credential line says about an account.
    ///
    /// `access_expires_at` belongs to the cached access token, not to the
    /// account: it lasts about a day and the next launch trades the stored
    /// refresh token for a new one. The expired case therefore reports whether
    /// the account can sign in again without help, not whether the cached token
    /// has run out.
    ///
    /// Shared by every preset that lists accounts, so two of them cannot
    /// disagree about when a session expires.
    pub(in crate::tui::app) fn account_expiry_label(
        &self,
        account: &crate::core::account::models::Account,
    ) -> String {
        use crate::core::account::models::AccountKind;
        if account.kind == AccountKind::Offline {
            return self.i18n.text("account.session_local");
        }
        match account.access_expires_at {
            Some(at) if at > unix_seconds() => {
                // Floor, not ceil: `有效 5 小时` with 5h01m left is one minute
                // pessimistic, while `有效 6 小时` is fifty-nine minutes
                // optimistic and reads as a promise the token cannot keep.
                let hours = ((at - unix_seconds()) / 3_600).max(1);
                self.i18n
                    .format("account.session_valid", &[("hours", &hours.to_string())])
            }
            // The token is spent, but the refresh token is on disk: renewing is
            // automatic and needs nothing from the player.
            _ if account.secret_ref.is_some() => self.i18n.text("account.session_renewable"),
            // No refresh token on disk: this is the only state that needs a
            // fresh sign-in.
            _ => self.i18n.text("account.session_reauth"),
        }
    }

    /// `微软 · 有效 1 小时`, the one-line form used by the paper directory.
    pub(in crate::tui::app) fn account_detail_line(
        &self,
        account: &crate::core::account::models::Account,
    ) -> String {
        format!(
            "{} · {}",
            self.account_kind_label(account.kind),
            self.account_expiry_label(account)
        )
    }
}

pub(in crate::tui::app) fn page_accent(page: Page) -> Color {
    match page {
        Page::Overview => LIME,
        Page::Instances => GREEN,
        Page::DownloadCenter => CYAN,
        Page::Tasks => PURPLE,
        Page::Accounts => PINK,
        Page::Logs => GOLD,
        Page::Settings => BLUE,
    }
}

pub(in crate::tui::app) fn page_icon(page: Page) -> &'static str {
    match page {
        Page::Overview => "◈",
        Page::Instances => "▤",
        Page::DownloadCenter => "⇣",
        Page::Tasks => "◷",
        Page::Accounts => "◉",
        Page::Logs => "≡",
        // Geometric Shapes has native glyphs in Consolas, so the icon renders
        // at the same optical size as the rest. `⚙`/`⌘`/`❖` all come from
        // blocks Consolas does not cover: Windows Terminal falls back to a
        // symbol font and draws them noticeably larger, even at one cell wide.
        Page::Settings => "◫",
    }
}
