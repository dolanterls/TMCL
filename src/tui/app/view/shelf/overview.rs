//! The overview page: the current instance's cover and the facts around it.
//!
//! No cards and no dashboard tiles. The cover is the identity; everything else
//! is a label/value row, the way the instances page sets its metadata.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Paragraph};

use super::tone::Palette;
use super::widgets;
use super::*;
use crate::core::task::TaskState;
use crate::tui::app::state::data::{format_play_time, localize_instance_status};
use crate::tui::palette::truncate;
use crate::tui::widgets::cover;

impl App {
    pub(in crate::tui::app) fn draw_shelf_overview(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &Palette,
    ) {
        if area.width < 24 || area.height < 6 {
            return;
        }
        let current = self
            .instances
            .current_id
            .and_then(|id| self.instances.items.iter().find(|item| item.id == id));
        let Some(item) = current else {
            frame.render_widget(
                Paragraph::new(self.i18n.text("overview.no_instances"))
                    .wrap(ratatui::widgets::Wrap { trim: true })
                    .style(Style::default().fg(pal.dim).bg(pal.bg)),
                Rect::new(area.x, area.y + 1, area.width, area.height - 1),
            );
            return;
        };

        let wide = area.width >= 90 && area.height >= 26;
        let side = if area.height >= 34 { 48 } else { 40 };
        let mut facts_x = area.x;
        let mut facts_width = area.width;
        let mut facts_y = area.y;

        if wide {
            let cover_rect = Rect::new(area.x, area.y + 1, side, side / 2);
            let cover = self
                .covers
                .borrow_mut()
                .ensure(&item.path)
                .unwrap_or_else(|| {
                    let seed = item.id.as_u128() as u64 ^ (item.id.as_u128() >> 64) as u64;
                    widgets::placeholder(seed, u32::from(side), self.shelf_palette())
                });
            cover::render(frame.buffer_mut(), cover_rect, &cover, pal.bg);

            let bar = Rect::new(cover_rect.x, cover_rect.bottom(), cover_rect.width, 1);
            frame.render_widget(Block::default().style(Style::default().bg(pal.accent)), bar);
            frame.render_widget(
                Paragraph::new(Span::styled(
                    format!(
                        " {}",
                        truncate(&item.name, usize::from(cover_rect.width).saturating_sub(1))
                    ),
                    Style::default()
                        .fg(pal.on_accent)
                        .bg(pal.accent)
                        .add_modifier(Modifier::BOLD),
                )),
                bar,
            );
            facts_x = cover_rect.right().saturating_add(3);
            facts_width = area.right().saturating_sub(facts_x);
            facts_y = area.y + 1;

            // The launch hint sits beside the name bar, on its row: under the
            // cover it read as a caption of the picture and cost the column a
            // row it needs for the facts.
            if facts_width >= 12 {
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        self.i18n.text("overview.enter_launch"),
                        Style::default().fg(pal.faint).bg(pal.bg),
                    )),
                    Rect::new(facts_x, bar.y, facts_width, 1),
                );
            }
        }

        if facts_width < 20 {
            return;
        }
        let loader = self.instance_loader_text(item);
        let account = self
            .current_account()
            .map_or_else(|| self.i18n.text("common.none"), |account| account.username);
        let java = self
            .config
            .java
            .selected_path
            .as_ref()
            .and_then(|path| self.java.iter().find(|java| &java.executable == path))
            .map_or_else(
                || self.i18n.text("overview.no_runtime"),
                |java| format!("Java {}", java.major),
            );
        // The instance the launcher will start says `当前`, not `就绪`: the
        // same swap classic, creeper, nightlight and paper make.
        let (status, status_color) = if self.shelf_marks_current(item) {
            (self.i18n.text("common.current"), pal.accent)
        } else {
            (localize_instance_status(&item.status, &self.i18n), pal.text)
        };
        let facts = [
            (
                self.i18n.text("overview.version"),
                item.version.clone(),
                pal.text,
            ),
            (self.i18n.text("overview.loader"), loader, pal.text),
            (self.i18n.text("overview.status"), status, status_color),
            (
                self.i18n.text("overview.launches_time"),
                format!(
                    "{}  ·  {}",
                    item.launch_count,
                    format_play_time(item.play_time_seconds)
                ),
                pal.text,
            ),
            (
                self.i18n.text("overview.current_account"),
                account,
                pal.text,
            ),
            (self.i18n.text("shell.java"), java.clone(), pal.text),
        ];
        for (row, (label, value, color)) in facts.iter().enumerate() {
            let y = facts_y.saturating_add(row as u16);
            if y >= area.bottom() {
                return;
            }
            self.shelf_fact(
                frame,
                Rect::new(facts_x, y, facts_width, 1),
                label,
                value,
                *color,
                pal,
            );
        }

        let stats_y = facts_y.saturating_add(facts.len() as u16 + 2);
        let running = self
            .task_snapshots
            .iter()
            .filter(|task| matches!(task.state, TaskState::Running))
            .count();
        let stats = [
            (
                self.i18n.text("paper.stat.instances"),
                self.instances.items.len().to_string(),
            ),
            (self.i18n.text("paper.stat.tasks"), running.to_string()),
            (
                self.i18n.text("shell.account"),
                self.accounts.accounts.len().to_string(),
            ),
            (self.i18n.text("shell.java"), java.clone()),
        ];
        for (row, (label, value)) in stats.iter().enumerate() {
            let y = stats_y.saturating_add(row as u16);
            if y >= area.bottom() {
                break;
            }
            self.shelf_fact(
                frame,
                Rect::new(facts_x, y, facts_width, 1),
                label,
                value,
                pal.text,
                pal,
            );
        }
    }

    fn shelf_fact(
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
            Paragraph::new(ratatui::text::Line::from(vec![
                Span::styled(label.to_owned(), Style::default().fg(pal.faint).bg(pal.bg)),
                Span::styled(
                    format!("  {}", truncate(value, room)),
                    Style::default().fg(value_color).bg(pal.bg),
                ),
            ]))
            .style(Style::default().bg(pal.bg)),
            area,
        );
    }
}
