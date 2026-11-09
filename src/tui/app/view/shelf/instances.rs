//! The instance page: a text list on the left, the selected cover and its
//! facts on the right.
//!
//! No cover wall: a grid needs `←/→`, and the instance page has no such binding.
//! One column plus a hero is what the shared key map supports.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use super::tone::Palette;
use super::widgets;
use super::*;
use crate::core::instance::path::display_path;
use crate::tui::action::{Action, PageAct};
use crate::tui::app::state::data::{compact_play_time, localize_instance_status};
use crate::tui::hit::RowKind;
use crate::tui::palette::mix;
use crate::tui::shell::LayoutMode;
use crate::tui::widgets::cover;
use crate::tui::widgets::rows::{RowLayout, RowWindow};

const CARD_HEIGHT: u16 = 3;
const CARD_GAP: u16 = 1;

impl App {
    pub(in crate::tui::app) fn draw_shelf_instances(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &Palette,
    ) {
        let (list, detail) = self.shelf_instances_rects(area);
        self.shelf_instance_list(frame, list, pal);
        if !detail.is_empty() {
            self.shelf_instance_detail(frame, detail, pal);
        }
    }

    fn shelf_instances_rects(&self, area: Rect) -> (Rect, Rect) {
        let mode = self.current_regions().mode;
        if mode == LayoutMode::Narrow || area.width < 90 {
            return (area, Rect::default());
        }
        let list_width = area.width.saturating_mul(56) / 100;
        let list = Rect::new(area.x, area.y, list_width, area.height);
        let detail_x = area
            .x
            .saturating_add(list_width)
            .saturating_add(1)
            .min(area.right());
        let detail = Rect::new(
            detail_x,
            area.y,
            area.right().saturating_sub(detail_x),
            area.height,
        );
        (list, detail)
    }

    fn shelf_instance_list(&self, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
        if area.height < 2 || area.width < 8 {
            return;
        }
        let count = self.instances.items.len();
        let header = self.i18n.format(
            "settings.directory_instances",
            &[("count", &count.to_string())],
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                header,
                Style::default().fg(pal.faint).bg(pal.bg),
            )))
            .style(Style::default().bg(pal.bg)),
            Rect::new(area.x, area.y, area.width, 1),
        );
        if count == 0 {
            frame.render_widget(
                Paragraph::new(self.i18n.text("overview.no_instances"))
                    .wrap(ratatui::widgets::Wrap { trim: true })
                    .style(Style::default().fg(pal.dim).bg(pal.bg)),
                Rect::new(area.x, area.y + 2, area.width, area.height - 2),
            );
            return;
        }

        let cards = Rect::new(
            area.x,
            area.y.saturating_add(1),
            area.width,
            area.height.saturating_sub(1),
        );
        let layout = self.row_windows.resolve(
            RowWindow::Instances,
            RowLayout::cards_with_gap(cards, count, self.instances.focused, CARD_HEIGHT, CARD_GAP),
        );
        self.hitmap.rows(layout, RowKind::Instances);
        for (index, rect) in layout.rows() {
            let Some(item) = self.instances.items.get(index) else {
                continue;
            };
            self.paint_instance_card(frame, rect, item, index == self.instances.focused, pal);
        }
    }

    fn paint_instance_card(
        &self,
        frame: &mut Frame<'_>,
        rect: Rect,
        item: &crate::tui::pages::instances::InstanceSummary,
        selected: bool,
        pal: &Palette,
    ) {
        let loader = match &item.loader_version {
            Some(version) => format!("{} {version}", self.loader_label(&item.loader)),
            None => self.loader_label(&item.loader),
        };
        let (text, secondary, background) = if selected {
            (
                pal.on_accent,
                mix(pal.on_accent, pal.accent, 0.35),
                pal.accent,
            )
        } else {
            (pal.text, pal.dim, pal.bg)
        };

        let text_width = if selected {
            let glow = 8.min(rect.width / 3);
            let solid = rect.width.saturating_sub(glow);
            frame.render_widget(
                Block::default().style(Style::default().bg(pal.accent)),
                Rect::new(rect.x, rect.y, solid, rect.height),
            );
            for step in 0..glow {
                let t = f64::from(step + 1) / f64::from(glow + 1);
                frame.render_widget(
                    Block::default().style(Style::default().bg(mix(pal.accent, pal.bg, t))),
                    Rect::new(
                        rect.x.saturating_add(solid).saturating_add(step),
                        rect.y,
                        1,
                        rect.height,
                    ),
                );
            }
            solid
        } else {
            rect.width
        };
        if text_width < 4 || rect.height == 0 {
            return;
        }

        let room = usize::from(text_width).saturating_sub(1);
        // The launcher marks the instance `Enter` will start. Without the word
        // the row still reads `Ready` after the key is pressed and nothing on
        // the page says it took: classic, creeper, nightlight and paper all
        // swap the status for `当前`, and shelf was the one that did not.
        let is_current = self.shelf_marks_current(item);
        let status = if is_current {
            self.i18n.text("common.current")
        } else {
            localize_instance_status(&item.status, &self.i18n)
        };
        let mut lines = vec![Line::from(Span::styled(
            format!(" {}", crate::tui::palette::truncate(&item.name, room)),
            Style::default()
                .fg(text)
                .bg(background)
                .add_modifier(Modifier::BOLD),
        ))];
        if rect.height >= 2 {
            lines.push(Line::from(Span::styled(
                format!(
                    " {} · {}",
                    item.version,
                    crate::tui::palette::truncate(
                        &loader,
                        room.saturating_sub(item.version.len() + 3)
                    )
                ),
                Style::default().fg(secondary).bg(background),
            )));
        }
        if rect.height >= 3 {
            // BOLD is the only emphasis a selected row can take: its band is
            // already `ACCENT`, so an accent-coloured status would vanish.
            let mut style = Style::default().fg(secondary).bg(background);
            if is_current {
                style = style.add_modifier(Modifier::BOLD);
            }
            lines.push(Line::from(Span::styled(
                format!(
                    " {} · {}",
                    status,
                    compact_play_time(item.play_time_seconds)
                ),
                style,
            )));
        }
        frame.render_widget(
            Paragraph::new(lines).style(Style::default().bg(background)),
            Rect::new(rect.x, rect.y, text_width, rect.height),
        );
    }

    fn shelf_instance_detail(&self, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
        if area.width < 20 || area.height < 6 {
            return;
        }
        let inner = widgets::panel(frame, area, &self.i18n.text("page.instances"), pal);
        let Some(item) = self.instances.focused_item() else {
            frame.render_widget(
                Paragraph::new(self.i18n.text("overview.no_instances"))
                    .wrap(ratatui::widgets::Wrap { trim: true })
                    .style(Style::default().fg(pal.dim).bg(pal.bg)),
                inner,
            );
            return;
        };

        let mode = self.current_regions().mode;
        let side = u16::try_from(widgets::cover_side(mode, inner.width)).unwrap_or(0);
        let mut y = inner.y;
        if side > 0 && inner.height >= side / 2 + 6 {
            let rows = side / 2;
            let cover_rect = Rect::new(
                inner.x + (inner.width.saturating_sub(side)) / 2,
                inner.y,
                side,
                rows,
            );
            let cover = self
                .covers
                .borrow_mut()
                .ensure(&item.path)
                .unwrap_or_else(|| {
                    let seed = item.id.as_u128() as u64 ^ (item.id.as_u128() >> 64) as u64;
                    widgets::placeholder(seed, u32::from(side), self.shelf_palette())
                });
            cover::render(frame.buffer_mut(), cover_rect, &cover, pal.bg);
            self.hitmap.add(
                cover_rect,
                Action::Page {
                    page: Page::Instances,
                    act: PageAct::Activate(self.instances.focused),
                },
            );
            y = cover_rect.bottom();
        }

        let bar = Rect::new(inner.x, y, inner.width, 1);
        frame.render_widget(Block::default().style(Style::default().bg(pal.accent)), bar);
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!(
                    " {}",
                    crate::tui::palette::truncate(
                        &item.name,
                        usize::from(inner.width).saturating_sub(1)
                    )
                ),
                Style::default()
                    .fg(pal.on_accent)
                    .bg(pal.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            bar,
        );
        // The bar carries the name, so it is part of the same target: the
        // contract is "cover or name bar opens the instance", not "only the
        // picture does".
        self.hitmap.add(
            bar,
            Action::Page {
                page: Page::Instances,
                act: PageAct::Activate(self.instances.focused),
            },
        );
        y = y.saturating_add(1);

        let loader = match &item.loader_version {
            Some(version) => format!("{} {version}", self.loader_label(&item.loader)),
            None => self.loader_label(&item.loader),
        };
        // Same rule as the list beside it: the instance the launcher will
        // start says `当前` instead of its stored status, in the accent so the
        // one word that changes after `Enter` is the one that stands out.
        let (status, status_color) = if self.shelf_marks_current(item) {
            (self.i18n.text("common.current"), pal.accent)
        } else {
            (localize_instance_status(&item.status, &self.i18n), pal.text)
        };
        let left = [
            (
                self.i18n.text("workspace.overview.version"),
                item.version.clone(),
                pal.text,
            ),
            (
                self.i18n.text("workspace.overview.loader"),
                loader,
                pal.text,
            ),
            (self.i18n.text("task.status"), status, status_color),
        ];
        let right = [
            (
                self.i18n.text("workspace.overview.launch_count"),
                item.launch_count.to_string(),
                pal.text,
            ),
            (
                self.i18n.text("workspace.overview.play_time"),
                compact_play_time(item.play_time_seconds),
                pal.text,
            ),
            (
                self.i18n.text("workspace.overview.path"),
                display_path(&item.path),
                pal.text,
            ),
        ];
        let hint_y = inner.bottom().saturating_sub(1);
        // Two columns need room for a label, the gap and a loader version. Below
        // that the right column cut `NeoForge 21.1.247` down to `NeoForge 21.1.2…`
        // at the reference size, losing the version the panel exists for. Narrow
        // columns stack the six facts instead.
        let mut facts_bottom = y;
        if inner.width >= 56 {
            let column = inner.width.saturating_sub(1) / 2;
            let right_x = inner.x.saturating_add(column).saturating_add(1);
            let right_width = inner.width.saturating_sub(column).saturating_sub(1);
            for (
                row,
                ((left_label, left_value, left_color), (right_label, right_value, right_color)),
            ) in left.iter().zip(right.iter()).enumerate()
            {
                let row_y = y.saturating_add(row as u16);
                if row_y >= hint_y {
                    break;
                }
                self.paint_meta(
                    frame,
                    Rect::new(inner.x, row_y, column, 1),
                    left_label,
                    left_value,
                    *left_color,
                    pal,
                );
                self.paint_meta(
                    frame,
                    Rect::new(right_x, row_y, right_width, 1),
                    right_label,
                    right_value,
                    *right_color,
                    pal,
                );
                facts_bottom = row_y.saturating_add(1);
            }
        } else {
            // Six facts in the order the wide layout prints them: left column
            // first, then the right one.
            let stack = left.iter().chain(right.iter());
            for (row, (label, value, color)) in stack.enumerate() {
                let row_y = y.saturating_add(row as u16);
                if row_y >= hint_y {
                    break;
                }
                self.paint_meta(
                    frame,
                    Rect::new(inner.x, row_y, inner.width, 1),
                    label,
                    value,
                    *color,
                    pal,
                );
                facts_bottom = row_y.saturating_add(1);
            }
        }

        let panel = Rect::new(
            inner.x,
            facts_bottom,
            inner.width,
            hint_y.saturating_sub(facts_bottom),
        );
        self.shelf_world_panel(frame, panel, &item.path, pal);

        if inner.height >= 2 {
            frame.render_widget(
                Paragraph::new(Span::styled(
                    self.i18n.text("shelf.hint"),
                    Style::default().fg(pal.faint).bg(pal.bg),
                )),
                Rect::new(inner.x, hint_y, inner.width, 1),
            );
        }
    }

    /// `Worlds 3` plus the newest three names, between the facts and the hint.
    ///
    /// A summary, not the workspace list: the count and the last few worlds are
    /// what tells a player whether the instance has been used. It disappears
    /// rather than shrink when the panel is too short to hold a header and one
    /// row.
    fn shelf_world_panel(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        instance: &std::path::Path,
        pal: &Palette,
    ) {
        if area.height < 2 || area.width < 24 {
            return;
        }
        let worlds = self.worlds.borrow_mut().ensure(instance);
        // An instance nobody has played has nothing to summarise: a lone
        // `Worlds 0` cost a row and, pinned to the bottom, left a gap that read
        // as missing content.
        if worlds.is_empty() {
            return;
        }
        // Sits directly above the hint rather than under the facts: the panel is
        // a footer for the column, and a gap above a hint reads as a missing row.
        let rows = u16::try_from(worlds.len().min(3)).unwrap_or(0);
        let height = rows + 1;
        let area = Rect::new(
            area.x,
            area.bottom().saturating_sub(height).max(area.y),
            area.width,
            height.min(area.height),
        );
        let label = self.i18n.text("workspace.category.worlds");
        let count = worlds.len().to_string();
        let count_width = u16::try_from(crate::tui::palette::width(&count)).unwrap_or(0);
        let header = Rect::new(area.x, area.y, area.width, 1);
        frame.render_widget(
            Paragraph::new(Span::styled(
                crate::tui::palette::truncate(&label, usize::from(area.width)),
                Style::default()
                    .fg(pal.dim)
                    .bg(pal.bg)
                    .add_modifier(Modifier::BOLD),
            )),
            header,
        );
        if count_width < area.width {
            frame.render_widget(
                Paragraph::new(Span::styled(
                    count,
                    Style::default().fg(pal.faint).bg(pal.bg),
                ))
                .alignment(Alignment::Right),
                Rect::new(area.x + area.width - count_width, area.y, count_width, 1),
            );
        }

        let now = crate::tui::app::state::data::unix_seconds();
        let rows = rows.min(area.height.saturating_sub(1));
        for (index, world) in worlds.iter().take(usize::from(rows)).enumerate() {
            let row = Rect::new(area.x, area.y + 1 + index as u16, area.width, 1);
            let age =
                crate::tui::app::state::data_format::format_age(world.modified_seconds(), now);
            let age_width = u16::try_from(crate::tui::palette::width(&age)).unwrap_or(0);
            let name_width = row.width.saturating_sub(age_width + 1);
            frame.render_widget(
                Paragraph::new(Span::styled(
                    crate::tui::palette::truncate(&world.name, usize::from(name_width)),
                    Style::default().fg(pal.text).bg(pal.bg),
                )),
                Rect::new(row.x, row.y, name_width, 1),
            );
            frame.render_widget(
                Paragraph::new(Span::styled(age, Style::default().fg(pal.faint).bg(pal.bg)))
                    .alignment(Alignment::Right),
                Rect::new(row.x + name_width, row.y, row.width - name_width, 1),
            );
        }
    }

    fn paint_meta(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        label: &str,
        value: &str,
        value_color: ratatui::style::Color,
        pal: &Palette,
    ) {
        let label_width = crate::tui::palette::width(label);
        let room = usize::from(area.width)
            .saturating_sub(label_width + 2)
            .max(4);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(label.to_owned(), Style::default().fg(pal.faint).bg(pal.bg)),
                Span::styled(
                    format!("  {}", crate::tui::palette::truncate(value, room)),
                    Style::default().fg(value_color).bg(pal.bg),
                ),
            ]))
            .style(Style::default().bg(pal.bg)),
            area,
        );
    }
}
