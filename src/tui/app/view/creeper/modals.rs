//! Overlays for the `creeper` preset: detail popups, help and modals.
//!
//! Only the visual layer lives here. `Modal::handle_key` / `handle_mouse` are
//! shared with the `classic` preset, so a dialog cannot behave differently
//! depending on which preset draws it.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Scrollbar, ScrollbarOrientation, Wrap};

use super::tone::Palette;
use super::*;
use crate::tui::app::view::{
    detail_paragraph, help_scrollbar, pane_scrollbar_state, split_detail_label,
};
use crate::tui::palette;
use crate::tui::widgets::kit as widgets;

/// Centred popup card, cleared underneath. Returns the inner rect.
pub(in crate::tui::app) fn popup(
    pal: &Palette,
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    accent: ratatui::style::Color,
    t: f64,
    height: u16,
) -> Rect {
    let width = area.width.saturating_sub(6).clamp(24, 78).min(area.width);
    let height = height
        .min(area.height.saturating_sub(2))
        .clamp(5, area.height.max(5));
    let rect = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, rect);
    let block = widgets::card(title, accent, true, t, pal.surface);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    inner
}

/// `label: value` rows with an aligned label column, used by detail surfaces.
///
/// Values wrap instead of being cut, and prose paragraphs are wrapped at the
/// card width, so a resource's full description is readable without leaving
/// the page. Long rows are pre-wrapped because the pane scrolls by line count.
fn labelled_lines(app: &App, max_width: usize, bg: Color) -> Vec<Line<'static>> {
    let pal = app.creeper_palette();
    let raw = app
        .detail_lines()
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    let label_width = raw
        .iter()
        .filter_map(|line| split_detail_label(line).map(|(label, _)| label))
        .map(|label| palette::width(label) + 1)
        .max()
        .unwrap_or(0)
        .min(max_width / 2);
    let value_width = max_width.saturating_sub(label_width + 2).max(1);
    let body = Style::default().fg(pal.text_dim).bg(bg);
    let value = Style::default().fg(pal.text).bg(bg);
    let heading = Style::default()
        .fg(palette::mix(pal.text, app.page_accent(), 0.3))
        .bg(bg)
        .add_modifier(Modifier::BOLD);
    let hint = Style::default()
        .fg(palette::mix(pal.gold, pal.text_dim, 0.55))
        .bg(bg);

    let mut lines = Vec::new();
    let mut heading_drawn = false;
    for text in raw {
        if let Some(paragraph) = detail_paragraph(&text) {
            for chunk in palette::wrap(&format!(" {paragraph}"), max_width) {
                lines.push(Line::from(Span::styled(chunk, body)));
            }
        } else if let Some((label, value_text)) = split_detail_label(&text) {
            for (index, chunk) in palette::wrap(value_text, value_width)
                .into_iter()
                .enumerate()
            {
                let mut spans = Vec::new();
                if index == 0 {
                    spans.push(Span::styled(
                        palette::pad(&format!("{label}:"), label_width),
                        body,
                    ));
                    spans.push(Span::styled("  ", Style::default().bg(bg)));
                } else {
                    // Continuation rows line up under the value column.
                    spans.push(Span::styled(
                        " ".repeat(label_width + 2),
                        Style::default().bg(bg),
                    ));
                }
                spans.push(Span::styled(chunk, value));
                lines.push(Line::from(spans));
            }
        } else if text.is_empty() {
            lines.push(Line::default());
        } else if text.starts_with("▸ ") {
            for chunk in palette::wrap(&text, max_width) {
                lines.push(Line::from(Span::styled(chunk, hint)));
            }
        } else if !heading_drawn {
            heading_drawn = true;
            for chunk in palette::wrap(&format!(" {text}"), max_width) {
                lines.push(Line::from(Span::styled(chunk, heading)));
            }
        } else {
            for chunk in palette::wrap(&format!(" {text}"), max_width) {
                lines.push(Line::from(Span::styled(chunk, body)));
            }
        }
    }
    lines
}

/// Full-screen detail popup.
pub(in crate::tui::app) fn detail(app: &App, frame: &mut Frame<'_>, t: f64) {
    let pal = app.creeper_palette();
    let area = frame.area();
    let title = app.i18n.text("common.details");
    // The detail pane scrolls, so it asks for the full available card.
    let height = area.height.saturating_sub(4).clamp(5, 24);
    let inner = popup(
        app.creeper_palette(),
        frame,
        area,
        &title,
        app.page_accent(),
        t,
        height,
    );
    if inner.is_empty() {
        return;
    }
    let lines = labelled_lines(app, usize::from(inner.width), pal.surface);
    let max_scroll = lines.len().saturating_sub(usize::from(inner.height));
    let scroll = app.detail_scroll.min(max_scroll);
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0))
            .wrap(Wrap { trim: false })
            .style(Style::default().bg(pal.surface)),
        inner,
    );
}

/// Keyboard help with the current page's hints first.
pub(in crate::tui::app) fn help(app: &App, frame: &mut Frame<'_>, t: f64) {
    let pal = app.creeper_palette();
    let title = app.i18n.text("help.keyboard");
    let help = app.help_lines();
    let layout = app.help_layout(frame.area(), &help);
    frame.render_widget(Clear, layout.rect);
    let block = widgets::card(&title, pal.cyan, true, t, pal.surface);
    frame.render_widget(block, layout.rect);
    if layout.body.is_empty() {
        return;
    }
    let room = usize::from(layout.body.width).saturating_sub(3);
    let lines: Vec<Line<'static>> = help
        .into_iter()
        .map(|line| match line {
            HelpLine::Header(title) => Line::from(Span::styled(
                title,
                Style::default()
                    .fg(pal.cyan)
                    .bg(pal.surface)
                    .add_modifier(Modifier::BOLD),
            )),
            HelpLine::Row { key, action } => {
                let mut spans = Vec::new();
                if key.is_empty() {
                    // Mouse rows have no key; indent them to the label column
                    // instead of starting the sentence at the card edge.
                    spans.push(Span::styled("   ", Style::default().bg(pal.surface)));
                } else {
                    spans.push(widgets::key_cap(&key));
                    spans.push(Span::styled(" ", Style::default().bg(pal.surface)));
                }
                // The capsule is ` key ` plus a space, so the action only gets
                // what is left of the card.
                let used = if key.is_empty() {
                    3
                } else {
                    palette::width(&key) + 3
                };
                spans.push(Span::styled(
                    palette::truncate(&action, room.saturating_sub(used)),
                    Style::default().fg(pal.text).bg(pal.surface),
                ));
                Line::from(spans)
            }
            HelpLine::Blank => Line::from(""),
        })
        .collect();
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((u16::try_from(layout.scroll).unwrap_or(u16::MAX), 0))
            .style(Style::default().bg(pal.surface)),
        layout.body,
    );
    help_scrollbar(
        frame,
        &layout,
        Style::default().fg(pal.text_dim).bg(pal.surface),
    );
}

/// Inline detail card used by the instances and download pages.
pub(in crate::tui::app) fn detail_pane(app: &App, frame: &mut Frame<'_>, area: Rect, _t: f64) {
    let pal = app.creeper_palette();
    if area.width < 10 || area.height < 3 {
        return;
    }
    let title = app.i18n.text("common.details");
    let accent = app.page_accent();
    let block = widgets::accent_card(&title, accent, pal.surface_alt);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.is_empty() {
        return;
    }
    // Keep the last column free for the scrollbar so wrapped lines are never
    // clipped by it.
    let text_area = Rect {
        width: inner.width.saturating_sub(1),
        ..inner
    };
    let lines = labelled_lines(app, usize::from(text_area.width), pal.surface_alt);
    let total = lines.len();
    let max_scroll = total.saturating_sub(usize::from(text_area.height));
    app.pane_max_scroll.set(max_scroll);
    let scroll = app.pane_scroll.min(max_scroll);
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0))
            .style(Style::default().bg(pal.surface_alt)),
        text_area,
    );
    // Long descriptions wrap below the fold; show that the card scrolls.
    if total > usize::from(text_area.height) {
        let mut state = pane_scrollbar_state(total, usize::from(text_area.height), scroll);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"))
                .track_symbol(Some("│"))
                .thumb_symbol("█")
                .track_style(
                    Style::default()
                        .fg(palette::dim(pal.border, 0.35))
                        .bg(pal.surface_alt),
                )
                .thumb_style(
                    Style::default()
                        .fg(palette::mix(accent, pal.text, 0.25))
                        .bg(pal.surface_alt),
                ),
            inner,
            &mut state,
        );
    }
}

/// Modal host: forms, pickers, prompts and confirmations.
pub(in crate::tui::app) fn render(
    app: &App,
    frame: &mut Frame<'_>,
    modal: &Modal,
    t: f64,
    hits: &crate::tui::hit::Hitmap,
) {
    let pal = app.creeper_palette();
    let area = modal.area(frame.area());
    frame.render_widget(Clear, area);
    modal.draw_creeper(frame, &app.i18n, app.creeper_palette(), area, t, hits);
    let _ = pal.text_faint;
}
