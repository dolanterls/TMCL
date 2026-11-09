//! Brand header, gradient rule, sidebar, footer and status toasts.

use crate::tui::app::view::sheet::Sheet;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::sidebar;
use super::tone::Palette;
use super::*;
use crate::tui::action::Action;
use crate::tui::palette;
use crate::tui::widgets::kit as widgets;

/// Brand row: `◆ TMCL` on the left, account/instance context on the right.
pub(in crate::tui::app) fn header(
    app: &App,
    frame: &mut Frame<'_>,
    regions: &ChromeRegions,
    t: f64,
) {
    let pal = app.creeper_palette();
    let area = regions.header;
    if area.is_empty() {
        return;
    }
    frame.render_widget(Block::default().style(Style::default().bg(pal.bg)), area);

    let context = context_spans(app, t);
    let context_used: u16 = context
        .iter()
        .map(|span| palette::width(&span.content) as u16)
        .sum();
    let context_width = if regions.context.is_empty() {
        context_used.saturating_add(1).min(area.width / 2)
    } else {
        regions.context.width
    };
    let left_width = area.width.saturating_sub(context_width + 1);

    let mut brand = vec![
        Span::styled(
            " ◆ ",
            Style::default()
                .fg(palette::mix(pal.lime, pal.green, palette::pulse(t, 1.3)))
                .bg(pal.bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "TMCL",
            Style::default()
                .fg(pal.lime)
                .bg(pal.bg)
                .add_modifier(Modifier::BOLD),
        ),
    ];
    if left_width >= 30 {
        brand.push(Span::styled(
            format!(" v{}", env!("CARGO_PKG_VERSION")),
            Style::default().fg(pal.text_faint).bg(pal.bg),
        ));
        brand.push(Span::styled(
            "  · creeper",
            Style::default()
                .fg(palette::mix(pal.green, pal.cyan, 0.45))
                .bg(pal.bg),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(brand)).style(Style::default().bg(pal.bg)),
        Rect::new(area.x, area.y, left_width, 1),
    );

    if !context.is_empty() {
        let width = context_width.min(area.width);
        frame.render_widget(
            Paragraph::new(Line::from(context))
                .alignment(Alignment::Right)
                .style(Style::default().bg(pal.bg)),
            Rect::new(area.right().saturating_sub(width), area.y, width, 1),
        );
    }
}

fn context_spans(app: &App, t: f64) -> Vec<Span<'static>> {
    let pal = app.creeper_palette();
    let mut spans = Vec::new();
    let account = app.current_account();
    let ready = account.is_some();
    spans.push(Span::styled(
        if ready { "● " } else { "○ " },
        Style::default()
            .fg(if ready {
                palette::mix(pal.green, pal.bg, 0.25 * (1.0 - palette::pulse(t, 2.0)))
            } else {
                pal.red
            })
            .bg(pal.bg),
    ));
    spans.push(Span::styled(
        account.map_or_else(
            || app.i18n.text("header.no_account"),
            |account| account.username,
        ),
        Style::default()
            .fg(if ready { pal.text } else { pal.text_dim })
            .bg(pal.bg)
            .add_modifier(Modifier::BOLD),
    ));
    let instance = app
        .instances
        .current_id
        .and_then(|id| app.instances.items.iter().find(|item| item.id == id));
    if let Some(instance) = instance {
        spans.push(Span::styled(
            "  · ",
            Style::default().fg(pal.text_faint).bg(pal.bg),
        ));
        spans.push(Span::styled(
            palette::truncate(&instance.version, 12),
            Style::default()
                .fg(palette::mix(pal.cyan, pal.text, 0.2))
                .bg(pal.bg),
        ));
        spans.push(Span::styled(
            format!(
                " {}",
                palette::truncate(
                    &instance.loader_version.as_ref().map_or_else(
                        || app.loader_label(&instance.loader),
                        |version| format!("{} {version}", app.loader_label(&instance.loader)),
                    ),
                    16
                )
            ),
            Style::default().fg(pal.purple).bg(pal.bg),
        ));
    }
    spans.push(Span::styled(" ", Style::default().bg(pal.bg)));
    spans
}

/// Full-width gradient rule under the brand row.
pub(in crate::tui::app) fn rule(app: &App, frame: &mut Frame<'_>, regions: &ChromeRegions, t: f64) {
    let pal = app.creeper_palette();
    let area = Rect::new(
        regions.header.x,
        regions.header.y.saturating_add(1),
        regions.header.width,
        1,
    );
    if area.is_empty() {
        return;
    }
    frame.render_widget(
        Paragraph::new(Line::from(widgets::rule_spans(area.width, t)))
            .style(Style::default().bg(pal.bg)),
        area,
    );
}

/// Sidebar card: block logo, vertical navigation and a compact runtime strip.
/// Fills the column between the sidebar card and the page with the card
/// surface.
///
/// The two are `pal.surface` cards one column apart; leaving that column on the
/// page background drew a black strip down the whole body beside every page.
pub(in crate::tui::app) fn gutter(frame: &mut Frame<'_>, regions: &ChromeRegions, pal: &Palette) {
    let sidebar = regions.tabs;
    let width = regions.main.x.saturating_sub(sidebar.right());
    if width == 0 || regions.body.height == 0 {
        return;
    }
    frame.render_widget(
        Block::default().style(Style::default().bg(pal.surface)),
        Rect::new(sidebar.right(), regions.body.y, width, regions.body.height),
    );
}

pub(in crate::tui::app) fn sidebar(
    app: &App,
    frame: &mut Frame<'_>,
    regions: &ChromeRegions,
    t: f64,
) {
    let pal = app.creeper_palette();
    let area = regions.tabs;
    if area.width < 4 || area.height < 4 {
        return;
    }
    let accent = app.page_accent();
    let block = widgets::frame_card(accent, false, t, pal.surface);
    frame.render_widget(block, area);
    let layout = sidebar::nav_layout(area);
    let inner = layout.inner;
    if inner.is_empty() {
        return;
    }

    if !layout.compact && inner.height >= 20 {
        for (offset, line) in sidebar::logo_lines(pal, t, inner.width)
            .into_iter()
            .enumerate()
        {
            frame.render_widget(
                Paragraph::new(line).style(Style::default().bg(pal.surface)),
                Rect::new(
                    inner.x,
                    inner.y.saturating_add(offset as u16),
                    inner.width,
                    1,
                ),
            );
        }
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" ", Style::default().bg(pal.surface)),
                Span::styled(
                    "TMCL",
                    Style::default()
                        .fg(palette::mix(pal.lime, pal.text, 0.1))
                        .bg(pal.surface)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "  launcher",
                    Style::default().fg(pal.text_faint).bg(pal.surface),
                ),
            ]))
            .style(Style::default().bg(pal.surface)),
            Rect::new(inner.x, inner.y.saturating_add(4), inner.width, 1),
        );
    }

    let labels = app.tab_labels(regions.mode);
    let rects = sidebar::nav_rects(&layout, PAGES.len());
    for (index, page) in PAGES.iter().enumerate() {
        let Some(row) = rects.get(index).copied() else {
            break;
        };
        // Registration uses the rectangle the loop is about to draw into.
        app.hitmap.add(row, Action::Nav(*page));
        let active = *page == app.page;
        if layout.compact {
            let bg = if active {
                pal.fill(app.page_accent(), 0.18)
            } else {
                pal.surface
            };
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        if active { "▌" } else { " " },
                        Style::default().fg(app.page_accent()).bg(bg),
                    ),
                    Span::styled(
                        format!(" {} ", page_icon(*page)),
                        Style::default()
                            .fg(if active {
                                app.page_accent()
                            } else {
                                pal.text_faint
                            })
                            .bg(bg)
                            .add_modifier(if active {
                                Modifier::BOLD
                            } else {
                                Modifier::empty()
                            }),
                    ),
                ]))
                .style(Style::default().bg(bg)),
                row,
            );
        } else {
            let label = labels
                .get(index)
                .cloned()
                .unwrap_or_else(|| app.i18n.text(page.title_key()));
            frame.render_widget(
                Paragraph::new(sidebar::nav_line(
                    pal,
                    page_icon(*page),
                    &label,
                    active,
                    inner.width,
                    app.page_accent(),
                    t,
                ))
                .style(Style::default().bg(pal.surface)),
                row,
            );
        }
    }

    if inner.height >= 14 && inner.width >= 12 {
        let running = app
            .task_snapshots
            .iter()
            .filter(|task| {
                matches!(
                    task.state,
                    crate::core::task::TaskState::Running | crate::core::task::TaskState::Paused
                )
            })
            .count();
        let top = inner.bottom().saturating_sub(3);
        let lines = vec![
            Line::from(vec![
                Span::styled(" ◈ ", Style::default().fg(pal.gold).bg(pal.surface)),
                Span::styled(
                    format!("{} {}", running, app.i18n.text("sidebar.running")),
                    Style::default().fg(pal.text_dim).bg(pal.surface),
                ),
            ]),
            Line::from(vec![
                Span::styled(" ◆ ", Style::default().fg(pal.cyan).bg(pal.surface)),
                Span::styled(
                    format!(
                        "{} {}",
                        app.instances.items.len(),
                        app.i18n.text("sidebar.instances")
                    ),
                    Style::default().fg(pal.text_dim).bg(pal.surface),
                ),
            ]),
            Line::from(vec![Span::styled(
                format!(" ↻ {}", palette::format_elapsed(app.anim_time() as u64)),
                Style::default().fg(pal.text_faint).bg(pal.surface),
            )]),
        ];
        frame.render_widget(
            Paragraph::new(lines).style(Style::default().bg(pal.surface)),
            Rect::new(inner.x, top, inner.width, 3),
        );
    }
}

/// Two-band footer, mirroring the Hot-RS launcher: key hints on top, a live
/// status/log line underneath.
///
/// The status band keeps its own row, so a long hint set cannot push the last
/// log message off screen; the layout grows downwards instead.
pub(in crate::tui::app) fn footer(
    app: &App,
    frame: &mut Frame<'_>,
    regions: &ChromeRegions,
    _t: f64,
) {
    let pal = app.creeper_palette();
    let area = regions.footer;
    if area.is_empty() {
        return;
    }
    frame.render_widget(
        Block::default().style(Style::default().bg(pal.surface)),
        area,
    );

    let status_area = Rect::new(
        area.x,
        area.y + area.height.saturating_sub(1),
        area.width,
        1,
    );
    let hints_area = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1));
    if !hints_area.is_empty() {
        hint_rows(app, frame, hints_area);
    }
    status_band(app, frame, status_area);
}

/// Key caps laid out from the left across the rows above the status band.
///
/// The whole footer shares the `pal.surface` fill: key caps on the near-black
/// canvas colour read as a separate black bar under the cards.
fn hint_rows(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let pal = app.creeper_palette();
    let rows = area.height.max(1);
    let mut remaining = app.hints();
    for row in 0..rows {
        if remaining.is_empty() {
            break;
        }
        let rect = Rect::new(area.x, area.y.saturating_add(row), area.width, 1);
        let last = row + 1 == rows;

        if last {
            // Last row: drop the least important hints until the rest fits.
            let mut items = remaining.clone();
            while items.len() > 1 && hints_width(app, &items) > usize::from(rect.width) {
                let removable =
                    (0..items.len() - 1).max_by_key(|index| (items[*index].priority, *index));
                match removable {
                    Some(index) => {
                        items.remove(index);
                    }
                    None => break,
                }
            }
            frame.render_widget(
                Paragraph::new(hints_line(app, &items))
                    .alignment(Alignment::Left)
                    .style(Style::default().bg(pal.surface)),
                rect,
            );
            break;
        }

        // Earlier rows take as many hints as fit; the rest flow to the next row.
        let mut used = 0usize;
        let mut take = 0usize;
        for (index, hint) in remaining.iter().enumerate() {
            let width_needed = hint_width(app, hint) + usize::from(index > 0);
            if used + width_needed > usize::from(rect.width) {
                break;
            }
            used += width_needed;
            take += 1;
        }
        take = take.max(1).min(remaining.len());
        // Never leave a lone hint on the following row: a single `q Quit`
        // under a full line reads like a mistake instead of a wrap.
        if remaining.len() - take == 1 && take > 1 {
            take -= 1;
        }
        frame.render_widget(
            Paragraph::new(hints_line(app, &remaining[..take]))
                .alignment(Alignment::Left)
                .style(Style::default().bg(pal.surface)),
            rect,
        );
        remaining.drain(..take);
    }
}

/// Status band: state glyph and message on the left, uptime on the right.
fn status_band(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let pal = app.creeper_palette();
    if area.is_empty() {
        return;
    }
    frame.render_widget(
        Block::default().style(Style::default().bg(pal.surface)),
        area,
    );

    let uptime = format!(" ↻ {} ", palette::format_elapsed(app.anim_time() as u64));
    let uptime_width = u16::try_from(palette::width(&uptime))
        .unwrap_or(u16::MAX)
        .min(area.width);
    let text_width = area.width.saturating_sub(uptime_width);
    if text_width > 0 {
        let kind = app.status.kind();
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" ", Style::default().bg(pal.surface)),
                Span::styled(
                    format!("{} ", widgets::status_glyph(kind)),
                    Style::default()
                        .fg(widgets::status_color(kind))
                        .bg(pal.surface)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    palette::truncate(
                        app.status.as_str(),
                        usize::from(text_width.saturating_sub(3)),
                    ),
                    Style::default().fg(pal.text_dim).bg(pal.surface),
                ),
            ]))
            .style(Style::default().bg(pal.surface)),
            Rect::new(area.x, area.y, text_width, 1),
        );
    }
    if uptime_width > 0 {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                uptime,
                Style::default().fg(pal.text_faint).bg(pal.surface),
            )))
            .alignment(Alignment::Right)
            .style(Style::default().bg(pal.surface)),
            Rect::new(
                area.right().saturating_sub(uptime_width),
                area.y,
                uptime_width,
                1,
            ),
        );
    }
}

/// Width of one hint as drawn by [`hints_line`]: ` key ` plus ` label `.
fn hint_width(app: &App, hint: &crate::tui::chrome::Hint) -> usize {
    palette::width(&hint.key) + 2 + palette::width(&app.i18n.text_or_raw(&hint.action)) + 2
}

/// Total width of a hint row, including its one-cell separators.
pub(in crate::tui::app) fn hints_width(app: &App, hints: &[crate::tui::chrome::Hint]) -> usize {
    let mut total = 0;
    for (index, hint) in hints.iter().enumerate() {
        if index > 0 {
            total += 1;
        }
        total += hint_width(app, hint);
    }
    total
}

fn hints_line(app: &App, hints: &[crate::tui::chrome::Hint]) -> Line<'static> {
    let pal = app.creeper_palette();
    let mut spans = Vec::new();
    for (index, hint) in hints.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(" ", Style::default().bg(pal.surface)));
        }
        spans.extend(widgets::hint(
            &hint.key,
            &app.i18n.text_or_raw(&hint.action),
            pal.surface,
        ));
    }
    Line::from(spans)
}

/// Recent status messages as cards above the footer, sliding in and fading out.
/// Rendering-only state: the status machine itself is untouched.
///
/// Up to three distinct messages stack upwards; repeating the same text does not
/// enqueue a new card.
pub(in crate::tui::app) fn toast(app: &App, frame: &mut Frame<'_>, t: f64) {
    if !app.motion {
        return;
    }
    let pal = app.creeper_palette();
    let ttl = crate::tui::app::state::toast::TOAST_TTL;
    let entries = crate::tui::app::state::toast::collect(app, ttl);
    for (slot, entry) in entries.iter().rev().enumerate() {
        draw_toast(
            pal,
            frame,
            &entry.text,
            entry.kind,
            entry.age_seconds(),
            ttl.as_secs_f64(),
            slot,
            t,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_toast(
    pal: &Palette,
    frame: &mut Frame<'_>,
    text: &str,
    kind: crate::tui::widgets::status::StatusKind,
    age: f64,
    ttl: f64,
    slot: usize,
    t: f64,
) {
    let area = frame.area();
    let width = area.width.saturating_sub(6).min(46);
    if width < 20 {
        return;
    }
    let entry = (age / 0.22).clamp(0.0, 1.0);
    let exit = ((age - (ttl - 0.45)) / 0.45).clamp(0.0, 1.0);
    let slide = (1.0 - palette::ease_out_back(entry)) * f64::from(width) * 0.5;
    let x = area
        .x
        .saturating_add(area.width)
        .saturating_sub(width + 2)
        .saturating_sub(slide as u16);
    let stack_offset = 3u16.saturating_mul(slot as u16);
    let Some(y) = (area.y + area.height.saturating_sub(4)).checked_sub(stack_offset) else {
        return;
    };
    if y <= area.y {
        return;
    }
    let rect = Rect::new(x, y, width, 3);

    let base = widgets::status_color(kind);
    let color = palette::mix(base, pal.bg, exit * 0.85);
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(
            Style::default()
                .fg(palette::mix(
                    color,
                    pal.bg,
                    0.2 + palette::pulse(t, 2.0) * 0.1,
                ))
                .bg(pal.surface),
        )
        .style(Style::default().bg(pal.surface));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let line = Line::from(vec![
        Span::styled(
            format!(" {} ", widgets::status_glyph(kind)),
            Style::default()
                .fg(color)
                .bg(pal.surface)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            palette::truncate(text, usize::from(inner.width.saturating_sub(4))),
            Style::default()
                .fg(palette::mix(pal.text, pal.bg, exit * 0.6))
                .bg(pal.surface),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(pal.surface)),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
}
