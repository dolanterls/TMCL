//! `shelf` detail and help: the same two overlays, on a slab of `SURFACE`.
//!
//! Geometry is still the shared one — `chrome::popup_rect` for the detail card,
//! `help_rect` / `help_body` for the reference — so scrolling, hit testing and
//! the wheel all keep working. Only the frame changes: a rounded border in
//! `FAINT`, the surface colour behind it, and an accent title.

use ratatui::Frame;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Wrap};

use super::tone::Palette;
use super::*;
use crate::tui::action::ScrollTarget;
use crate::tui::hit::Layer;

impl App {
    pub(in crate::tui::app) fn draw_shelf_detail(&self, frame: &mut Frame<'_>) {
        let pal = self.shelf_palette();
        let rect = crate::tui::chrome::popup_rect(frame.area());
        frame.render_widget(Clear, rect);
        let block = shelf_slab(&self.i18n.text("common.details"), pal);
        let inner = block.inner(rect);
        frame.render_widget(block, rect);
        if inner.is_empty() {
            return;
        }
        self.hitmap
            .scroll_on(Layer::Detail, inner, ScrollTarget::DetailBody);
        let lines = self.styled_detail_lines(self.shelf_theme());
        let max_scroll = self.detail_max_scroll();
        let scroll = self.detail_scroll.min(max_scroll);
        frame.render_widget(
            Paragraph::new(lines)
                .scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0))
                .wrap(Wrap { trim: false }),
            inner,
        );
        crate::tui::app::view::margin_scrollbar(
            frame,
            inner,
            max_scroll.saturating_add(usize::from(inner.height)),
            scroll,
            pal.faint,
            pal.accent,
            pal.surface,
        );
    }

    pub(in crate::tui::app) fn draw_shelf_help(&self, frame: &mut Frame<'_>) {
        let pal = self.shelf_palette();
        let help = self.help_lines();
        let layout = self.help_layout(frame.area(), &help);
        frame.render_widget(Clear, layout.rect);
        let block = shelf_slab(&self.i18n.text("help.keyboard"), pal);
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
                        .fg(pal.accent)
                        .bg(pal.surface)
                        .add_modifier(Modifier::BOLD),
                )),
                HelpLine::Row { key, action } => Line::from(vec![
                    Span::styled(
                        crate::tui::palette::pad(&key, HELP_KEY_COLUMN),
                        Style::default().fg(pal.accent_light).bg(pal.surface),
                    ),
                    Span::raw(crate::tui::palette::truncate(&action, room)),
                ]),
                HelpLine::Blank => Line::from(""),
            })
            .collect();
        frame.render_widget(
            Paragraph::new(lines)
                .style(Style::default().fg(pal.text).bg(pal.surface))
                .scroll((u16::try_from(layout.scroll).unwrap_or(u16::MAX), 0)),
            layout.body,
        );
        crate::tui::app::view::margin_scrollbar(
            frame,
            layout.body,
            layout.total,
            layout.scroll,
            pal.faint,
            pal.accent,
            pal.surface,
        );
    }
}

/// The slab both overlays sit on: rounded, `SURFACE`, accent title.
fn shelf_slab(title: &str, pal: &Palette) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(pal.faint).bg(pal.surface))
        .title(Line::from(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(pal.accent)
                .bg(pal.surface)
                .add_modifier(Modifier::BOLD),
        )))
        .style(Style::default().bg(pal.surface))
}
