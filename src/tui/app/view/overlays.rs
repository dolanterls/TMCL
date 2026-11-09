//! Detail and help overlays shared by every preset.
//!
//! The painters take the theme they should paint with rather than reading
//! `self.theme`: classic passes its own, shelf passes its palette, and the copy
//! stays identical.

use ratatui::Frame;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use super::*;
use crate::tui::action::ScrollTarget;
use crate::tui::hit::Layer;

impl App {
    pub(in crate::tui::app) fn draw_help_overlay_with(&self, frame: &mut Frame<'_>, theme: Theme) {
        let help = self.help_lines();
        let layout = self.help_layout(frame.area(), &help);
        frame.render_widget(ratatui::widgets::Clear, layout.rect);
        let block = crate::tui::chrome::panel(&self.i18n.text("help.keyboard"), true, theme);
        frame.render_widget(block, layout.rect);
        if layout.body.is_empty() {
            return;
        }
        let room = usize::from(layout.body.width).saturating_sub(HELP_KEY_COLUMN);
        let lines: Vec<Line<'static>> = help
            .into_iter()
            .map(|line| match line {
                HelpLine::Header(title) => Line::from(Span::styled(
                    title,
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                )),
                HelpLine::Row { key, action } => Line::from(vec![
                    Span::styled(
                        crate::tui::palette::pad(&key, HELP_KEY_COLUMN),
                        theme.key_style(),
                    ),
                    Span::raw(crate::tui::palette::truncate(&action, room)),
                ]),
                HelpLine::Blank => Line::from(""),
            })
            .collect();
        frame.render_widget(
            Paragraph::new(lines).scroll((u16::try_from(layout.scroll).unwrap_or(u16::MAX), 0)),
            layout.body,
        );
        help_scrollbar(frame, &layout, Style::default().fg(theme.dim));
    }

    pub(in crate::tui::app) fn draw_detail_overlay(&self, frame: &mut Frame<'_>, theme: Theme) {
        let inner = crate::tui::chrome::popup(
            frame,
            frame.area(),
            &self.i18n.text("common.details"),
            theme,
        );
        self.hitmap
            .scroll_on(Layer::Detail, inner, ScrollTarget::DetailBody);
        let lines = self.styled_detail_lines(theme, theme.fg);
        // Wrapping makes a row occupy more than one screen line; ask the same
        // painter the wheel uses so draw and input cannot disagree.
        let max_scroll = self.detail_max_scroll();
        let scroll = self.detail_scroll.min(max_scroll);
        frame.render_widget(
            Paragraph::new(lines)
                .scroll((scroll as u16, 0))
                .wrap(Wrap { trim: false }),
            inner,
        );
    }

    /// The record, typeset for `theme`, with its values in `body_ink`.
    ///
    /// `body_ink` is the ink the preset paints a list row with. Taking it as an
    /// argument rather than reading `theme.fg` keeps the panel looking like part
    /// of the page: `shelf` prints its browsing lists in `dim`, and a record in
    /// heading ink would not match them. `classic` passes its `fg`, the terminal
    /// default its rows also use.
    pub(in crate::tui::app) fn styled_detail_lines(
        &self,
        theme: Theme,
        body_ink: Color,
    ) -> Vec<Line<'static>> {
        let raw = self
            .detail_lines()
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();
        let width = raw
            .iter()
            .filter_map(|line| split_detail_label(line).map(|(label, _)| label))
            .map(|label| Line::from(label).width())
            .max()
            .unwrap_or(0);
        // Every value paints its own ink. Leaving them `raw` handed them the
        // terminal's default foreground, which is the one colour the preset
        // cannot see: `shelf`'s light page came out mid grey on white, and the
        // same cell read fine on `classic` only because that theme's `fg` *is*
        // the terminal default.
        let body = Style::default().fg(body_ink);
        raw.into_iter()
            .map(|text| {
                if let Some(paragraph) = detail_paragraph(&text) {
                    Line::from(Span::styled(paragraph.to_owned(), body))
                } else if let Some((label, value)) = split_detail_label(&text) {
                    let label_width = Line::from(label).width();
                    let padding = " ".repeat(width.saturating_sub(label_width));
                    Line::from(vec![
                        Span::styled(label.to_owned(), theme.label_style()),
                        Span::styled(padding, body),
                        Span::styled(": ", theme.dim()),
                        Span::styled(value.to_owned(), body),
                    ])
                } else if text.is_empty() {
                    Line::default()
                } else {
                    Line::from(Span::styled(text, body))
                }
            })
            .collect()
    }
}
