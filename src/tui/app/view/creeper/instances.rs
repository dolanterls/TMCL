//! Instances page: filter pills, instance cards and the detail card.

use crate::tui::app::view::sheet::Sheet;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use super::*;
use crate::tui::hit::RowKind;
use crate::tui::palette;
use crate::tui::widgets::kit as widgets;

/// Version column in an instance card. Wide enough for imported names like
/// `FarmingTales_Forge v1.4.3`; the loader starts at the same column so short
/// versions do not collapse the row.
const VERSION_COLUMN: usize = 30;

impl App {
    pub(in crate::tui::app) fn draw_creeper_instances(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        t: f64,
    ) {
        let pal = self.creeper_palette();
        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(area);
        self.filter_pills(frame, sections[0]);

        let visible = self.visible_instance_indices();
        if visible.is_empty() {
            widgets::empty_card(
                frame,
                sections[1],
                &self.i18n.text("page.instances"),
                &self.i18n.text("instances.empty_page"),
                pal.green,
                t,
            );
            return;
        }
        let selected = visible
            .iter()
            .position(|index| *index == self.instances.focused)
            .unwrap_or(0);
        let split = self.instance_detail_areas(sections[1]);
        let list_area = split.map_or(sections[1], |(list, _)| list);
        self.instance_cards(frame, list_area, &visible, selected, t);
        if let Some((_, pane)) = split {
            self.hitmap
                .scroll(pane, crate::tui::action::ScrollTarget::Pane);
            self.creeper_detail_pane(frame, pane, t);
        }
    }

    fn filter_pills(&self, frame: &mut Frame<'_>, area: Rect) {
        let pal = self.creeper_palette();
        let labels = self.instance_filter_labels();
        let spans = widgets::pill_row(&labels, Some(self.instance_filter), pal.green, pal.surface);
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(pal.surface)),
            area,
        );
        for (index, rect) in widgets::pill_rects(&labels, area).iter().enumerate() {
            self.hitmap
                .add(*rect, Action::filter(Page::Instances, index));
        }
    }

    fn instance_cards(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        visible: &[usize],
        selected: usize,
        t: f64,
    ) {
        let pal = self.creeper_palette();
        let block = widgets::with_counter(
            widgets::card(
                &self.i18n.text("page.instances"),
                pal.green,
                true,
                t,
                pal.surface,
            ),
            selected + 1,
            visible.len(),
        );
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.height < 2 || inner.width < 12 {
            return;
        }
        let card_height = self.creeper_card_height();
        let gap = 1;
        let stride = card_height.saturating_add(gap);
        let layout = self.row_windows.resolve(
            RowWindow::Instances,
            RowLayout::cards_with_gap(area, visible.len(), selected, card_height, gap),
        );
        self.hitmap.rows(layout, RowKind::Instances);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" ", Style::default().bg(pal.surface)),
                Span::styled(
                    format!("{} {}", visible.len(), self.i18n.text("page.instances")),
                    Style::default().fg(pal.text_faint).bg(pal.surface),
                ),
                Span::styled(
                    format!("  ·  [ ] {}", self.i18n.text("action.change_filter")),
                    Style::default()
                        .fg(palette::dim(pal.text_faint, 0.35))
                        .bg(pal.surface),
                ),
            ]))
            .style(Style::default().bg(pal.surface)),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );

        let start = layout.offset();
        for (visual, position) in visible
            .iter()
            .skip(start)
            .take(layout.visible_rows())
            .enumerate()
        {
            let Some(item) = self.instances.items.get(*position) else {
                continue;
            };
            let y = inner
                .y
                .saturating_add(1)
                .saturating_add(visual as u16 * stride);
            let height = card_height.min(inner.bottom().saturating_sub(y));
            if height == 0 {
                break;
            }
            let active = start + visual == selected;
            let bg = if active {
                pal.fill(pal.green, 0.2)
            } else if visual % 2 == 1 {
                pal.fill(pal.surface_alt, 0.5)
            } else {
                pal.surface
            };
            frame.render_widget(
                Block::default().style(Style::default().bg(bg)),
                Rect::new(inner.x, y, inner.width, height),
            );
            let current = self.instances.current_id == Some(item.id);
            let kind = if current {
                StatusKind::Current
            } else {
                instance_status_kind(&item.status)
            };
            let status = if current {
                self.i18n.text("common.current")
            } else {
                localize_instance_status(&item.status, &self.i18n)
            };
            let loader = item.loader_version.as_ref().map_or_else(
                || self.loader_label(&item.loader),
                |version| format!("{} {version}", self.loader_label(&item.loader)),
            );
            let pill = widgets::status_pill(kind, status, bg);
            let pill_width = pill.width();
            let name_width = usize::from(inner.width)
                .saturating_sub(3 + pill_width)
                .max(8);
            let mut head = vec![
                // The marker has to be repeated on every row of the card:
                // drawing it once leaves a stub above the selected card.
                Span::styled(
                    if active { "▌ " } else { "  " },
                    Style::default()
                        .fg(palette::mix(pal.green, pal.text, 0.15))
                        .bg(bg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    palette::truncate(&item.name, name_width),
                    Style::default()
                        .fg(if active {
                            pal.text
                        } else {
                            palette::mix(pal.text, pal.text_dim, 0.2)
                        })
                        .bg(bg)
                        .add_modifier(Modifier::BOLD),
                ),
            ];
            push_right(&mut head, pill, usize::from(inner.width), bg);
            frame.render_widget(
                Paragraph::new(Line::from(head)).style(Style::default().bg(bg)),
                Rect::new(inner.x, y, inner.width, 1),
            );
            if height >= 2 {
                let mut spans = vec![
                    Span::styled(
                        if active { "▌  " } else { "   " },
                        Style::default()
                            .fg(palette::mix(pal.green, pal.text, 0.15))
                            .bg(bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                    // Fixed column so a long loader cannot squeeze the version
                    // out of the row; `fit` pads shorter versions instead.
                    // Shrink it only when the card is too narrow for the
                    // loader (20) plus the play time (12).
                    Span::styled(
                        palette::fit(
                            &item.version,
                            VERSION_COLUMN
                                .min(usize::from(inner.width).saturating_sub(39))
                                .max(12),
                        ),
                        Style::default()
                            .fg(palette::mix(pal.green, pal.text, 0.25))
                            .bg(bg),
                    ),
                    Span::styled("  ", Style::default().bg(bg)),
                    Span::styled(
                        palette::truncate(&loader, 24),
                        Style::default().fg(pal.text_dim).bg(bg),
                    ),
                ];
                push_right(
                    &mut spans,
                    Span::styled(
                        format!(
                            "{} {}",
                            format_play_time(item.play_time_seconds),
                            self.i18n.text("overview.played")
                        ),
                        Style::default().fg(pal.text_faint).bg(bg),
                    ),
                    // One cell of breathing room before the card border; the
                    // status pill already carries its own trailing space.
                    usize::from(inner.width).saturating_sub(1),
                    bg,
                );
                frame.render_widget(
                    Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)),
                    Rect::new(inner.x, y + 1, inner.width, 1),
                );
            }
            if height >= 3 {
                frame.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::styled(
                            if active { "▌  " } else { "   " },
                            Style::default()
                                .fg(palette::mix(pal.green, pal.text, 0.15))
                                .bg(bg)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            palette::truncate(
                                &display_path(&item.path),
                                usize::from(inner.width).saturating_sub(4),
                            ),
                            Style::default().fg(pal.text_faint).bg(bg),
                        ),
                    ]))
                    .style(Style::default().bg(bg)),
                    Rect::new(inner.x, y + 2, inner.width, 1),
                );
            }
        }
    }
}

/// Append `right` flush with the right edge of a `width`-cell line.
fn push_right(spans: &mut Vec<Span<'static>>, right: Span<'static>, width: usize, bg: Color) {
    let used = spans.iter().map(|span| span.width()).sum::<usize>() + right.width();
    if width > used {
        spans.push(Span::styled(
            " ".repeat(width - used),
            Style::default().bg(bg),
        ));
    }
    spans.push(right);
}
