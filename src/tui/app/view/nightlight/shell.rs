//! Shell drawing for the `nightlight` preset: sidebar, hairline, footer, toast.
//!
//! There is no banner. The brand sits on the sidebar's second row (one row of
//! air above it), the footer spans the whole width below the sidebar, and the
//! only rule in the interface sits directly above it.
//! Design inspiration: nightlightd (GPL-3.0-or-later).
//! This file is an independent implementation; no source code is copied.

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use crate::tui::action::Action;
use crate::tui::app::state::nav::PAGES;
use crate::tui::app::view::nightlight::live;
use crate::tui::app::view::nightlight::sidebar;
use crate::tui::app::view::nightlight::tone::Palette;
use crate::tui::app::view::nightlight::widgets::{self, width};
use crate::tui::app::view::page_icon;
use crate::tui::palette::{format_elapsed, truncate};
use crate::tui::shell::ChromeRegions;
use crate::tui::widgets::status::StatusKind;

use super::App;

/// Sidebar: brand, numbered navigation, bottom summary. The column is painted
/// on the page colour, never inside a card.
pub(in crate::tui::app) fn sidebar_panel(
    app: &App,
    frame: &mut Frame<'_>,
    regions: &ChromeRegions,
    pal: &Palette,
) {
    let column = Rect::new(
        regions.header.x,
        regions.body.y,
        regions.header.width,
        regions.body.height,
    );
    if column.width < 4 || column.height < 4 {
        return;
    }
    frame.render_widget(Block::default().style(Style::default().bg(pal.bg)), column);
    // The wordmark gets a band of its own: four rows of block glyphs when the
    // column is wide enough, a single line on a rail.
    sidebar::draw_brand(
        frame,
        Rect::new(
            column.x,
            column.y.saturating_add(sidebar::BRAND_TOP),
            column.width,
            sidebar::brand_rows(column),
        ),
        pal,
    );

    let labels = app.tab_labels(crate::tui::shell::LayoutMode::Full);
    for (index, rect) in sidebar::nav_rects(column, PAGES.len())
        .into_iter()
        .enumerate()
    {
        let Some(page) = PAGES.get(index) else {
            continue;
        };
        let label = labels.get(index).map(String::as_str).unwrap_or_default();
        let active = *page == app.page;
        frame.render_widget(
            Paragraph::new(sidebar::nav_line(
                index,
                label,
                page_icon(*page),
                active,
                column.width,
                pal,
            )),
            rect,
        );
        // Drawing and hit-testing read the same rectangle.
        app.hitmap.add(rect, Action::Nav(*page));
    }

    let (phase, clock) = match app.nightlight_clock() {
        Some((minute, clock)) => (app.i18n.text(live::phase_at(minute)), clock),
        None => (app.i18n.text("nightlight.phase.day"), "--:--".to_owned()),
    };
    let running = app
        .task_snapshots
        .iter()
        .filter(|snapshot| snapshot.state.is_active())
        .count();
    let running_label = app.i18n.text("sidebar.running");
    // The six-column rail has no room for the summary; the clock alone still
    // fits and is the one piece worth keeping.
    let summary = if column.width < 12 {
        vec![Line::from(Span::styled(
            format!(" {clock}"),
            Style::default().fg(pal.faint).bg(pal.bg),
        ))]
    } else {
        sidebar::summary(
            &phase,
            &clock,
            Some((running, running_label.as_str())),
            &format!("v{}", crate::version()),
            &app.i18n
                .text(app.config.state.ui.nightlight_theme.label_key()),
            &format_elapsed(app.anim_time() as u64),
            pal,
        )
    };
    let start_y = column.bottom().saturating_sub(summary.len() as u16);
    for (offset, line) in summary.into_iter().enumerate() {
        frame.render_widget(
            Paragraph::new(line),
            Rect::new(
                column.x,
                start_y.saturating_add(offset as u16),
                column.width,
                1,
            ),
        );
    }
}

/// The single rule in the preset, one row above the footer and spanning the
/// full width so it passes under the sidebar too.
pub(in crate::tui::app) fn hairline(frame: &mut Frame<'_>, regions: &ChromeRegions, pal: &Palette) {
    let Some(y) = regions.footer.y.checked_sub(1) else {
        return;
    };
    if y < regions.body.y {
        return;
    }
    let line = Line::from(Span::styled(
        "─".repeat(usize::from(regions.footer.width)),
        Style::default().fg(pal.faint).bg(pal.bg),
    ));
    frame.render_widget(
        Paragraph::new(line),
        Rect::new(regions.footer.x, y, regions.footer.width, 1),
    );
}

/// Footer: status on the left, key hints on the right. With two rows the hints
/// take the first one and the status sits underneath.
pub(in crate::tui::app) fn footer(
    app: &App,
    frame: &mut Frame<'_>,
    regions: &ChromeRegions,
    pal: &Palette,
) {
    let area = regions.footer;
    if area.is_empty() {
        return;
    }
    frame.render_widget(Block::default().style(Style::default().bg(pal.bg)), area);

    if area.height >= 2 {
        let hints_area = Rect::new(area.x, area.y, area.width, 1);
        let hints = fitted_hints(app, hints_area.width.saturating_sub(2));
        frame.render_widget(
            Paragraph::new(hints_line(app, &hints, pal)).alignment(Alignment::Right),
            hints_area,
        );
        // Two rows give the status a full line; no truncation is needed.
        frame.render_widget(
            Paragraph::new(status_line(app, pal, usize::from(area.width))),
            Rect::new(area.x, area.y + area.height - 1, area.width, 1),
        );
        return;
    }

    // One row: the hints keep a fixed minimum for the status, and the status is
    // truncated into whatever is left. Fitting the hints against the full
    // message let a long failure text take the whole line; the row count is
    // fixed here, so the page never reflows.
    let hint_budget = area.width.saturating_sub(super::STATUS_MIN_WIDTH);
    let hints = hints_line(app, &fitted_hints(app, hint_budget), pal);
    let hints_width = line_width(&hints) as u16;
    if hints_width > 0 {
        frame.render_widget(
            Paragraph::new(hints).alignment(Alignment::Right),
            Rect::new(
                area.x
                    .saturating_add(area.width)
                    .saturating_sub(hints_width),
                area.y,
                hints_width,
                1,
            ),
        );
    }
    let status_width = area.width.saturating_sub(hints_width);
    frame.render_widget(
        Paragraph::new(status_line(app, pal, usize::from(status_width))),
        Rect::new(area.x, area.y, status_width, 1),
    );
}

/// `● 就绪`, the glyph carrying the state colour.
///
/// `budget` is the width the footer can spare. The glyph survives and the
/// message gives way with an ellipsis, so a long error cannot push the key hints
/// off the line. The session uptime lives in the sidebar summary.
fn status_line(app: &App, pal: &Palette, budget: usize) -> Line<'static> {
    let kind = app.status.kind();
    // " " + glyph + " " before the message.
    const PREFIX: usize = 3;
    let room = budget.saturating_sub(PREFIX);
    let message = truncate(app.status.as_str(), room);
    let mut spans = vec![
        Span::styled(" ", Style::default().bg(pal.bg)),
        Span::styled(
            format!("{} ", kind_glyph(kind)),
            Style::default()
                .fg(kind_color(kind, pal))
                .bg(pal.bg)
                .add_modifier(Modifier::BOLD),
        ),
    ];
    if !message.is_empty() {
        spans.push(Span::styled(
            message,
            Style::default().fg(pal.muted).bg(pal.bg),
        ));
    }
    Line::from(spans)
}

fn kind_glyph(kind: StatusKind) -> &'static str {
    match kind {
        StatusKind::Failed | StatusKind::Missing => "✕",
        StatusKind::Warning => "!",
        StatusKind::Running | StatusKind::Current => "▶",
        StatusKind::Queued => "◷",
        StatusKind::Completed | StatusKind::Ready => "●",
        StatusKind::Info => "·",
    }
}

fn kind_color(kind: StatusKind, pal: &Palette) -> Color {
    match kind {
        StatusKind::Failed | StatusKind::Missing => pal.err,
        StatusKind::Warning => pal.accent2,
        StatusKind::Running | StatusKind::Current => pal.accent,
        StatusKind::Completed | StatusKind::Ready => pal.ok,
        StatusKind::Queued | StatusKind::Info => pal.muted,
    }
}

/// Hints that fit in `available` cells, dropping the lowest priority first.
fn fitted_hints(app: &App, available: u16) -> Vec<crate::tui::chrome::Hint> {
    let hints = app.hints();
    crate::tui::chrome::fitted_hints(&hints, |items| {
        items
            .iter()
            .map(|hint| hint_width(app, hint))
            .sum::<usize>()
            <= usize::from(available)
    })
    .into_iter()
    .cloned()
    .collect()
}

fn hint_width(app: &App, hint: &crate::tui::chrome::Hint) -> usize {
    width(&hint.key) + 1 + width(&app.i18n.text(&hint.action)) + 3
}

fn hints_line(app: &App, hints: &[crate::tui::chrome::Hint], pal: &Palette) -> Line<'static> {
    let mut spans = Vec::new();
    for hint in hints {
        spans.push(Span::styled(
            hint.key.clone(),
            Style::default()
                .fg(pal.accent)
                .bg(pal.bg)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {}   ", app.i18n.text(&hint.action)),
            Style::default().fg(pal.muted).bg(pal.bg),
        ));
    }
    Line::from(spans)
}

fn line_width(line: &Line<'_>) -> usize {
    line.spans.iter().map(|span| width(&span.content)).sum()
}

/// Transient status cards above the footer, newest at the bottom.
pub(in crate::tui::app) fn toast(
    app: &App,
    frame: &mut Frame<'_>,
    regions: &ChromeRegions,
    pal: &Palette,
) {
    if !app.motion {
        return;
    }
    let ttl = crate::tui::app::state::toast::TOAST_TTL;
    let entries = crate::tui::app::state::toast::collect(app, ttl);
    for (slot, entry) in entries.iter().rev().enumerate() {
        let width = frame.area().width.saturating_sub(8).min(46);
        if width < 20 {
            return;
        }
        let offset = 3u16.saturating_mul(slot as u16);
        // One row higher than the footer's own height would allow: the hairline
        // lives at `footer.y - 1` and is the only rule in the interface, so a
        // toast must not paint over it.
        let Some(y) = regions.footer.y.checked_sub(4 + offset) else {
            continue;
        };
        let rect = Rect::new(frame.area().right().saturating_sub(width + 2), y, width, 3);
        frame.render_widget(Clear, rect);
        frame.render_widget(
            Block::default().style(Style::default().bg(pal.raised)),
            rect,
        );
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!("{} ", kind_glyph(entry.kind)),
                    Style::default()
                        .fg(kind_color(entry.kind, pal))
                        .bg(pal.raised)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    truncate(&entry.text, usize::from(width).saturating_sub(4)),
                    Style::default().fg(pal.text).bg(pal.raised),
                ),
            ]))
            .style(Style::default().bg(pal.raised)),
            Rect::new(rect.x + 2, rect.y + 1, width.saturating_sub(3), 1),
        );
    }
}

/// Too small: one card with the minimum size, plus the footer.
pub(in crate::tui::app) fn too_small(
    app: &App,
    frame: &mut Frame<'_>,
    regions: &ChromeRegions,
    pal: &Palette,
) {
    let area = regions.body;
    let width = area.width.clamp(24, 58).min(area.width);
    let height = area.height.clamp(3, 5).min(area.height);
    let panel = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let inner = widgets::card(frame, panel, "TMCL", pal);
    frame.render_widget(
        // The shared string still says 80x24, which is classic's minimum; this
        // preset renders from 64x18, so it gets its own.
        Paragraph::new(app.i18n.text("layout.too_small.nightlight"))
            .alignment(Alignment::Center)
            .style(Style::default().fg(pal.muted).bg(pal.surface)),
        inner,
    );
    footer(app, frame, regions, pal);
}
