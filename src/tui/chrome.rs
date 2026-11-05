//! Binsider-style chrome: header box, body frame, key hints and popups.
//!
//! Layout conventions follow binsider `src/tui/ui.rs` (commit 7f0633d1):
//! one-cell margin, a three-row bordered header with a centered
//! `|name-version|` title, tabs on the left and right-aligned context, a
//! bordered body, and key hints on the last line of the body.

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
};

use crate::tui::palette::truncate;
use crate::tui::shell::ChromeRegions;
use crate::tui::theme::Theme;
use crate::tui::widgets::status::StatusKind;

/// One key hint rendered as `[key→ label]`.
#[derive(Debug, Clone)]
pub struct Hint {
    pub key: String,
    pub action: String,
    pub priority: u8,
}

impl Hint {
    pub fn new(key: impl Into<String>, action: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            action: action.into(),
            priority: 10,
        }
    }

    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }
}

/// Returns the rectangle of every tab, in wide characters.
pub fn tab_rects(area: Rect, labels: &[String]) -> Vec<Rect> {
    let mut rects = Vec::with_capacity(labels.len());
    let mut x = area.x.saturating_add(1);
    for (index, label) in labels.iter().enumerate() {
        let width = Line::from(label.as_str()).width() as u16;
        rects.push(Rect::new(x, area.y, width, 1));
        x = x.saturating_add(width);
        if index + 1 < labels.len() {
            x = x.saturating_add(3);
        }
    }
    rects
}

pub fn render_header(
    frame: &mut Frame<'_>,
    regions: &ChromeRegions,
    labels: &[String],
    active: Option<usize>,
    context: &[Span<'static>],
    theme: Theme,
) -> Vec<Rect> {
    frame.render_widget(
        Block::bordered()
            .border_style(Style::default().fg(theme.dim))
            .title_alignment(Alignment::Center)
            .title(Line::from(vec![
                Span::styled("|", theme.dim()),
                Span::styled("tmcl", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled("-", theme.dim()),
                Span::styled(env!("CARGO_PKG_VERSION"), Style::default().fg(theme.dim)),
                Span::styled("|", theme.dim()),
            ])),
        regions.header,
    );

    let rects = tab_rects(regions.tabs, labels);
    let mut spans = vec![Span::raw(" ")];
    for (index, label) in labels.iter().enumerate() {
        let style = if Some(index) == active {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.label)
        };
        spans.push(Span::styled(label.clone(), style));
        if index + 1 < labels.len() {
            spans.push(Span::styled(" │ ", theme.dim()));
        } else {
            spans.push(Span::raw(" "));
        }
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), regions.tabs);

    if !regions.context.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(context.to_vec())).alignment(Alignment::Right),
            regions.context,
        );
    }

    rects
}

pub fn render_body(frame: &mut Frame<'_>, regions: &ChromeRegions, theme: Theme) {
    frame.render_widget(
        Block::bordered().border_style(Style::default().fg(theme.dim)),
        regions.body,
    );
}

pub fn render_footer(
    frame: &mut Frame<'_>,
    area: Rect,
    hints: &[Hint],
    status: Option<(&str, StatusKind)>,
    i18n: &crate::i18n::I18n,
    theme: Theme,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    // Two bands whenever there is room: key hints on top, the status line on
    // the bottom row. This is the same shape `creeper` and `nightlight` draw,
    // so the status bar does not jump when the preset changes — and the status
    // stops sharing a line with the hints, where it could only ever get a
    // third of the width and ended up next to a wall of key caps.
    if area.height >= 2 {
        let hints_area = Rect::new(area.x, area.y, area.width, area.height - 1);
        let status_area = Rect::new(area.x, area.y + area.height - 1, area.width, 1);
        render_hints(frame, hints_area, hints, i18n, theme, Alignment::Left);
        render_status(frame, status_area, status, theme, status_area.width);
        return;
    }

    // One row left (a terminal that barely fits the frame): the status keeps
    // the left edge and the hints get what remains.
    let mut hint_area = area;
    let mut has_status = false;
    if let Some((status, kind)) = status.filter(|(status, _)| !status.is_empty()) {
        let width = (area.width / 3).clamp(8, 40);
        render_status(
            frame,
            Rect::new(area.x, area.y, width, 1),
            Some((status, kind)),
            theme,
            width,
        );
        has_status = true;
        let offset = width.saturating_add(1);
        hint_area = Rect::new(
            area.x.saturating_add(offset),
            area.y,
            area.width.saturating_sub(offset),
            1,
        );
    }
    let alignment = if has_status {
        Alignment::Right
    } else {
        Alignment::Center
    };
    render_hints(frame, hint_area, hints, i18n, theme, alignment);
}

/// Status band: the message, truncated to the row instead of being wrapped.
fn render_status(
    frame: &mut Frame<'_>,
    area: Rect,
    status: Option<(&str, StatusKind)>,
    theme: Theme,
    width: u16,
) {
    let Some((status, _kind)) = status.filter(|(status, _)| !status.is_empty()) else {
        return;
    };
    let text = truncate(status, usize::from(width));
    frame.render_widget(Paragraph::new(Span::styled(text, theme.dim())), area);
}

/// Key caps, dropping the least important ones until the row fits.
fn render_hints(
    frame: &mut Frame<'_>,
    area: Rect,
    hints: &[Hint],
    i18n: &crate::i18n::I18n,
    theme: Theme,
    alignment: Alignment,
) {
    if area.is_empty() {
        return;
    }
    let available = area.width;
    let mut items = hints.iter().collect::<Vec<_>>();
    loop {
        let line = hint_line(&items, i18n, theme);
        if line.width() as u16 <= available || items.len() <= 1 {
            frame.render_widget(Paragraph::new(line).alignment(alignment), area);
            break;
        }
        // Drop the least important hint first (highest priority number),
        // never the last entry (quit). Tab uses priority 1, page actions 10.
        let removable =
            (0..items.len().saturating_sub(1)).max_by_key(|index| (items[*index].priority, *index));
        match removable {
            Some(index) => {
                items.remove(index);
            }
            None => {
                frame.render_widget(Paragraph::new(line).alignment(alignment), area);
                break;
            }
        }
    }
}

fn hint_line(hints: &[&Hint], i18n: &crate::i18n::I18n, theme: Theme) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, hint) in hints.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled("[", theme.dim()));
        spans.push(Span::styled(hint.key.clone(), theme.key_style()));
        spans.push(Span::styled("→ ", theme.dim()));
        spans.push(Span::raw(i18n.text_or_raw(&hint.action)));
        spans.push(Span::styled("]", theme.dim()));
    }
    Line::from(spans)
}

pub fn panel(title: &str, focused: bool, theme: Theme) -> Block<'static> {
    Block::bordered()
        .border_style(if focused {
            Style::default().fg(theme.accent)
        } else {
            Style::default().fg(theme.dim)
        })
        .title_alignment(Alignment::Center)
        .title(Line::from(vec![
            Span::styled("|", theme.dim()),
            Span::styled(title.to_string(), theme.accent_style()),
            Span::styled("|", theme.dim()),
        ]))
}

/// Clears and frames a centered popup, returning its inner area.
pub fn popup_rect(area: Rect) -> Rect {
    popup_rect_sized(area, area.height.saturating_sub(2).clamp(6, 16))
}

/// Same popup geometry with an explicit content height.
///
/// Dialogs that know how many rows they need ask for a shorter card instead of
/// always taking the 16-row maximum, which left a large empty band under a
/// three-row confirmation.
pub fn popup_rect_sized(area: Rect, height: u16) -> Rect {
    let width = area.width.saturating_sub(4).clamp(20, 76);
    popup_rect_measured(area, width, height)
}

/// Same popup geometry with both dimensions chosen by the caller.
///
/// The keyboard help measures its own rows: a card that always took the full
/// 76 columns left two thirds of the box empty once the width followed the
/// content.
pub fn popup_rect_measured(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.clamp(20, 76).min(area.width);
    let height = height
        .min(area.height.saturating_sub(2))
        .max(5)
        .min(area.height);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}

pub fn popup(frame: &mut Frame<'_>, area: Rect, title: &str, theme: Theme) -> Rect {
    let rect = popup_rect(area);
    frame.render_widget(Clear, rect);
    let block = panel(title, true, theme);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    inner
}

/// [`popup`] with an explicit height, for panels that know how many rows they
/// need.
pub fn popup_sized(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    theme: Theme,
    height: u16,
) -> Rect {
    let rect = popup_rect_sized(area, height);
    frame.render_widget(Clear, rect);
    let block = panel(title, true, theme);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    inner
}

/// Compact centered dialog for a single-line prompt: wide enough for a search
/// term, short enough to read as a small form instead of a full page.
pub fn prompt_rect(area: Rect) -> Rect {
    let width = area.width.saturating_sub(8).clamp(24, 60);
    // Title + field + spacer + help = four inner rows; five total would clip
    // the help line in the creeper preset.
    let height = 6.min(area.height);
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width, height)
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use ratatui::text::Span;

    use super::{Hint, render_header, tab_rects};
    use crate::core::config::models::Language;
    use crate::i18n::I18n;
    use crate::tui::shell::layout_chrome;
    use crate::tui::theme::Theme;

    #[test]
    fn chrome_head_places_tabs_and_context_inside_a_three_row_box() {
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                let regions = layout_chrome(frame.area());
                render_header(
                    frame,
                    &regions,
                    &["Overview".into(), "Instances".into(), "Download".into()],
                    Some(1),
                    &[Span::raw("Vanilla 1.21.1 · Alex")],
                    Theme::default(),
                );
            })
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(text.contains("tmcl"));
        assert!(text.contains("Instances"));
        assert!(text.contains("Vanilla 1.21.1 · Alex"));
    }

    #[test]
    fn tab_rects_are_contiguous_and_inside_the_tab_area() {
        let area = Rect::new(2, 2, 80, 1);
        let rects = tab_rects(area, &["Overview".into(), "Instances".into()]);
        assert_eq!(rects[0].x, 3);
        assert_eq!(rects[1].x, rects[0].right() + 3);
        assert!(rects[1].right() <= area.right());
    }

    #[test]
    fn hint_keeps_key_and_label_separated() {
        let hint = Hint::new("Enter", "Launch");
        assert_eq!(hint.key, "Enter");
        assert_eq!(hint.action, "Launch");
    }

    #[test]
    fn footer_keeps_primary_and_quit_when_crowded() {
        let backend = TestBackend::new(32, 1);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                crate::tui::chrome::render_footer(
                    frame,
                    frame.area(),
                    &[
                        Hint::new("Enter", "Details"),
                        Hint::new("a", "Alpha"),
                        Hint::new("b", "Beta"),
                        Hint::new("q", "Quit"),
                    ],
                    None,
                    &I18n::new(Language::English),
                    Theme::default(),
                );
            })
            .expect("draw footer");
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("Details"), "{text}");
        assert!(text.contains("Quit"), "{text}");
        assert!(!text.contains("Alpha"), "{text}");
    }

    #[test]
    fn footer_keeps_tab_and_quit_when_narrow() {
        let backend = TestBackend::new(48, 1);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                crate::tui::chrome::render_footer(
                    frame,
                    frame.area(),
                    &[
                        Hint::new("Enter", "Details"),
                        Hint::new("r", "Rename"),
                        Hint::new("Tab", "Next page").with_priority(1),
                        Hint::new("q", "Quit").with_priority(0),
                    ],
                    None,
                    &I18n::new(Language::English),
                    Theme::default(),
                );
            })
            .expect("draw footer");
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("Next page"), "{text}");
        assert!(text.contains("Quit"), "{text}");
        assert!(!text.contains("Rename"), "{text}");
    }

    #[test]
    fn popup_stays_inside_80x24() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                let inner =
                    crate::tui::chrome::popup(frame, frame.area(), "Details", Theme::default());
                assert!(inner.width > 0);
                assert!(inner.height > 0);
            })
            .expect("draw popup");
    }
}

#[cfg(test)]
mod geometry_tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::panel;
    use crate::tui::theme::Theme;

    #[test]
    fn popup_is_centered() {
        let rect = super::popup_rect(ratatui::layout::Rect::new(0, 0, 80, 24));
        assert_eq!(rect.x + rect.width / 2, 40);
        assert_eq!(rect.y + rect.height / 2, 12);
    }

    #[test]
    fn panel_title_uses_pipes_and_accent() {
        let theme = Theme::default();
        let backend = TestBackend::new(24, 3);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                frame.render_widget(panel("Title", true, theme), frame.area());
            })
            .expect("draw panel");
        let buffer = terminal.backend().buffer();
        let text = buffer
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("|Title|"), "{text}");
        assert!((0..24).any(|x| buffer[(x, 0)].fg == theme.accent));
    }
}

#[cfg(test)]
mod color_tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::{Hint, render_footer};
    use crate::core::config::models::Language;
    use crate::i18n::I18n;
    use crate::tui::theme::Theme;
    use crate::tui::widgets::status::StatusKind;

    #[test]
    fn footer_keys_are_yellow_and_status_stays_dim() {
        let theme = Theme::default();
        let hints = [Hint::new("q", "Quit")];
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                render_footer(
                    frame,
                    frame.area(),
                    &hints,
                    Some(("Ready", StatusKind::Info)),
                    &I18n::new(Language::English),
                    theme,
                );
            })
            .expect("draw footer");
        let buffer = terminal.backend().buffer();
        let key_colored = (0..80).any(|x| buffer[(x, 0)].fg == theme.key);
        let status_dim = (0..80).any(|x| buffer[(x, 0)].fg == theme.dim);
        assert!(key_colored, "footer key should use the key color");
        assert!(status_dim, "footer status should stay dim");
    }
}

#[cfg(test)]
mod footer_status_tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::{Hint, render_footer};
    use crate::core::config::models::Language;
    use crate::i18n::I18n;
    use crate::tui::theme::Theme;
    use crate::tui::widgets::status::StatusKind;

    #[test]
    fn footer_hints_right_align_when_status_present() {
        let theme = Theme::default();
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                render_footer(
                    frame,
                    frame.area(),
                    &[
                        Hint::new("Enter", "action.details"),
                        Hint::new("q", "common.quit").with_priority(0),
                    ],
                    Some(("Ready", StatusKind::Info)),
                    &I18n::new(Language::English),
                    theme,
                );
            })
            .expect("draw footer");
        let buffer = terminal.backend().buffer();
        let first_bracket = (0..80).find(|x| buffer[(*x, 0)].symbol() == "[");
        assert!(
            first_bracket.is_some_and(|x| x >= 27),
            "hints must not overlap the status segment"
        );
    }

    fn two_band_footer(width: u16, status: &str) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(width, 2);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                render_footer(
                    frame,
                    frame.area(),
                    &[
                        Hint::new("q", "common.quit"),
                        Hint::new("Enter", "action.details"),
                    ],
                    Some((status, StatusKind::Info)),
                    &I18n::new(Language::English),
                    Theme::default(),
                );
            })
            .expect("draw footer");
        terminal.backend().buffer().clone()
    }

    fn row_text(buffer: &ratatui::buffer::Buffer, y: u16) -> String {
        let width = buffer.area.width;
        (0..width)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>()
    }

    /// Two bands: key hints on top, the status line underneath. Before this the
    /// status shared the row with the hints and could only use `width / 3`.
    #[test]
    fn footer_puts_the_hints_above_the_status_line() {
        let buffer = two_band_footer(60, "Saved");
        let hints = row_text(&buffer, 0);
        let status = row_text(&buffer, 1);
        assert!(hints.starts_with("[q→ Quit]"), "{hints:?}");
        assert!(
            !hints.contains("Saved"),
            "the status must not join the hints"
        );
        assert!(status.starts_with("Saved"), "{status:?}");
        assert!(
            !status.contains('['),
            "the hints must not bleed into the status row: {status:?}"
        );
    }

    /// A long message owns its whole row now, and stops at the edge with an
    /// ellipsis instead of being cut into a third of the width.
    #[test]
    fn a_long_status_is_truncated_to_its_own_row() {
        let message = "Failed to export the launcher log bundle: access denied";
        let buffer = two_band_footer(40, message);
        let status = row_text(&buffer, 1);
        assert!(status.trim_end().ends_with('…'), "{status:?}");
        assert!(
            status.trim_end().chars().count() > 20,
            "40 columns should carry more than the old 13-cell slice: {status:?}"
        );
        assert!(
            row_text(&buffer, 0).starts_with("[q→ Quit]"),
            "the hint row must survive a long message"
        );
    }
}
