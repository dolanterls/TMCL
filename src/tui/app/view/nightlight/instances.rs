//! Instances page: filter chips, instance cards, optional detail card.
//! Design inspiration: nightlightd (GPL-3.0-or-later).
//! This file is an independent implementation; no source code is copied.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::tui::action::ScrollTarget;
use crate::tui::app::state::format_play_time;
use crate::tui::app::view::nightlight::tone::Palette;
use crate::tui::app::view::nightlight::widgets::{self, card, card_inner, pill};
use crate::tui::hit::RowKind;
use crate::tui::palette::{fit, truncate};
use crate::tui::widgets::rows::{RowLayout, RowWindow};

use super::{App, Page};

/// Card height: name, version, path.
const CARD_ROWS: u16 = 3;
const CARD_GAP: u16 = 1;

pub(in crate::tui::app) fn layout(area: Rect) -> [Rect; 2] {
    let inner = area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .spacing(1)
        .split(inner);
    [rows[0], rows[1]]
}

impl App {
    pub(in crate::tui::app) fn draw_nightlight_instances(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &Palette,
    ) {
        let [pills, body] = layout(area);
        let labels = self.instance_filter_labels();
        self.draw_pill_row(
            frame,
            pills,
            &labels,
            self.instance_filter,
            Page::Instances,
            pal,
        );

        self.hitmap
            .scroll(body, ScrollTarget::Rows(RowKind::Instances));
        let visible = self.visible_instance_indices();
        if visible.is_empty() {
            card(frame, body, &self.i18n.text("page.instances"), pal);
            let inner = card_inner(body);
            if !inner.is_empty() {
                frame.render_widget(
                    Paragraph::new(self.i18n.text("instances.empty_page"))
                        .style(Style::default().fg(pal.muted).bg(pal.surface)),
                    inner,
                );
            }
            return;
        }
        let selected = visible
            .iter()
            .position(|index| *index == self.instances.focused)
            .unwrap_or(0);
        let split = self.instance_detail_areas(body);
        let list_area = split.map_or(body, |(list, _)| list);
        self.instance_list(frame, list_area, &visible, selected, pal);
        if let Some((_, pane)) = split {
            self.hitmap.scroll(pane, ScrollTarget::Pane);
            self.instance_detail_card(frame, pane, pal);
        }
    }

    fn instance_list(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        visible: &[usize],
        selected: usize,
        pal: &Palette,
    ) {
        let inner = card(frame, area, &self.i18n.text("page.instances"), pal);
        if inner.height < 2 {
            return;
        }
        // Header row: total count and the position of the selected card.
        let count = format!(" {}  ·  {}/{} ", visible.len(), selected + 1, visible.len());
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                count,
                Style::default().fg(pal.faint).bg(pal.surface),
            ))),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );

        let rows = Rect::new(
            inner.x,
            inner.y + 1,
            inner.width,
            inner.height.saturating_sub(1),
        );
        let layout = self.row_windows.resolve(
            RowWindow::Instances,
            RowLayout::cards_bare(rows, visible.len(), selected, CARD_ROWS, CARD_GAP),
        );
        self.hitmap.rows(layout, RowKind::Instances);
        let stride = CARD_ROWS + CARD_GAP;
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
            let y = rows.y.saturating_add(visual as u16 * stride);
            let height = CARD_ROWS.min(rows.bottom().saturating_sub(y));
            if height == 0 {
                break;
            }
            let active = start + visual == selected;
            let current = self.instances.current_id == Some(item.id);
            self.instance_card(
                frame,
                Rect::new(rows.x, y, rows.width, height),
                item,
                active,
                current,
                pal,
            );
        }
    }

    fn instance_card(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        item: &crate::tui::pages::instances::InstanceSummary,
        active: bool,
        current: bool,
        pal: &Palette,
    ) {
        let background = if active { pal.selection } else { pal.surface };
        frame.render_widget(
            Block::default().style(Style::default().bg(background)),
            area,
        );
        let (status, installing) = if current {
            (self.i18n.text("common.current"), false)
        } else {
            (
                localize_status(&item.status, &self.i18n),
                item.status
                    .eq_ignore_ascii_case(crate::tui::app::state::data::INSTALLING),
            )
        };
        // A half-installed instance has to look busy, not settled: the badge is
        // the only place the list says whether it can be played.
        let color = if current {
            pal.ok
        } else if installing {
            pal.accent
        } else {
            pal.muted
        };
        let badge = pill(&status, color, pal);
        let used = 4 + widgets::width(&status) + 2;
        let name_width = usize::from(area.width).saturating_sub(used).max(6);
        let mut head = vec![
            Span::styled(
                if active { "▎ " } else { "  " },
                Style::default().fg(pal.accent).bg(background),
            ),
            Span::styled(
                truncate(&item.name, name_width),
                Style::default()
                    .fg(pal.text)
                    .bg(background)
                    .add_modifier(Modifier::BOLD),
            ),
        ];
        push_right(&mut head, badge, usize::from(area.width), background);
        frame.render_widget(
            Paragraph::new(Line::from(head)).style(Style::default().bg(background)),
            Rect::new(area.x, area.y, area.width, 1),
        );
        if area.height >= 2 {
            let mut line = vec![
                Span::styled(
                    if active { "▎  " } else { "   " },
                    Style::default().fg(pal.accent).bg(background),
                ),
                Span::styled(
                    fit(&item.version, 18),
                    Style::default().fg(pal.accent2).bg(background),
                ),
                Span::styled("  ", Style::default().bg(background)),
                Span::styled(
                    truncate(
                        &item.loader_version.as_ref().map_or_else(
                            || self.loader_label(&item.loader),
                            |version| format!("{} {version}", self.loader_label(&item.loader)),
                        ),
                        20,
                    ),
                    Style::default().fg(pal.muted).bg(background),
                ),
            ];
            push_right(
                &mut line,
                Span::styled(
                    format_play_time(item.play_time_seconds),
                    Style::default().fg(pal.faint).bg(background),
                ),
                usize::from(area.width).saturating_sub(1),
                background,
            );
            frame.render_widget(
                Paragraph::new(Line::from(line)).style(Style::default().bg(background)),
                Rect::new(area.x, area.y + 1, area.width, 1),
            );
        }
        if area.height >= 3 {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        if active { "▎  " } else { "   " },
                        Style::default().fg(pal.accent).bg(background),
                    ),
                    Span::styled(
                        truncate(
                            &crate::core::instance::path::display_path(&item.path),
                            usize::from(area.width).saturating_sub(4),
                        ),
                        Style::default().fg(pal.faint).bg(background),
                    ),
                ]))
                .style(Style::default().bg(background)),
                Rect::new(area.x, area.y + 2, area.width, 1),
            );
        }
    }

    fn instance_detail_card(&self, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
        card(frame, area, &self.i18n.text("hint.details"), pal);
        let inner = card_inner(area);
        if inner.is_empty() {
            return;
        }
        // The clamp used by the wheel lives on `App`; a pane that never sets it
        // swallows every notch because the maximum stays at zero.
        let max = widgets::detail_pane(frame, inner, self.detail_lines(), self.pane_scroll, pal);
        self.pane_max_scroll.set(max);
    }
}

fn push_right(
    spans: &mut Vec<Span<'static>>,
    right: Span<'static>,
    width: usize,
    bg: ratatui::style::Color,
) {
    let used = spans
        .iter()
        .map(|span| widgets::width(&span.content))
        .sum::<usize>()
        + widgets::width(&right.content);
    if width > used {
        spans.push(Span::styled(
            " ".repeat(width - used),
            Style::default().bg(bg),
        ));
    }
    spans.push(right);
}

fn localize_status(status: &str, i18n: &crate::i18n::I18n) -> String {
    let key = format!("status.{}", status.to_ascii_lowercase().replace(' ', "_"));
    let text = i18n.text(&key);
    if text == key { status.to_owned() } else { text }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::layout;

    #[test]
    fn the_instances_page_reserves_a_chip_row_above_the_list() {
        let [pills, body] = layout(Rect::new(0, 0, 93, 38));
        assert_eq!(pills.height, 1);
        assert_eq!(pills.y, 1);
        assert_eq!(body.y, 3);
        assert_eq!(body.bottom(), 37);
        assert!(pills.right() <= 93);
    }
}
