//! Overview page: hero row (instance and account), a metrics row, then an
//! activity card that grows to the bottom of the page.
//!
//! The activity card carries the running task and the tail of the launcher log;
//! it always fills the remaining height so the page never ends in a band of page
//! colour.
//! Design inspiration: nightlightd (GPL-3.0-or-later).
//! This file is an independent implementation; no source code is copied.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::tui::action::{Action, Metric, PageAct};
use crate::tui::app::view::nightlight::tone::Palette;
use crate::tui::app::view::nightlight::widgets::{self, card, card_inner, stat};
use crate::tui::pages::instances::InstanceSummary;
use crate::tui::palette::truncate;

use crate::tui::app::state::data::compact_play_time;

use super::{App, Page};

/// Regions of the overview page. Cards are separated by one row/column, and the
/// activity card always owns whatever height is left.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::tui::app) struct OverviewLayout {
    pub instance: Rect,
    pub account: Rect,
    pub stats: [Rect; 4],
    pub activity: Rect,
}

/// Rows a quadrant glyph occupies: two pixels per cell row, eight per glyph.
const BIG_NUMBER_ROWS: u16 = 4;

pub(in crate::tui::app) fn layout(area: Rect) -> OverviewLayout {
    let inner = area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            // Tall hero with big numbers; short terminals drop it to the name
            // rows so the activity card still gets height.
            // name + detail + gap + 4 glyph rows + label, plus the card padding.
            Constraint::Length(if inner.height >= 22 { 10 } else { 5 }),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .spacing(1)
        .split(inner);
    // Judged on the content column, not the terminal: at 120 columns the
    // sidebar leaves 93 cells, which still fits both hero cards.
    let narrow = area.width < 80;
    let hero = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(if narrow {
            [Constraint::Fill(1), Constraint::Length(0)]
        } else {
            [Constraint::Fill(3), Constraint::Fill(2)]
        })
        .spacing(1)
        .split(rows[0]);
    let stats = if narrow {
        // 2×2 inside the same three-row band: one row of tiles, a gap, another
        // row. Four tiles across 55 cells leave ~13 each, the width at which
        // labels start touching values (design §9.4).
        let bands = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(rows[1]);
        let columns = |band: Rect| {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Fill(1), Constraint::Fill(1)])
                .spacing(1)
                .split(band)
        };
        let top = columns(bands[0]);
        let bottom = columns(bands[2]);
        [top[0], top[1], bottom[0], bottom[1]]
    } else {
        let row = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Fill(1); 4])
            .spacing(1)
            .split(rows[1]);
        [row[0], row[1], row[2], row[3]]
    };
    OverviewLayout {
        instance: hero[0],
        account: hero[1],
        stats,
        activity: rows[2],
    }
}

impl App {
    pub(in crate::tui::app) fn draw_nightlight_overview(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &Palette,
    ) {
        let layout = layout(area);
        // The card decides by the account card's own geometry, not by the
        // instance card's width: at 120 columns the hero splits 3:2, so the
        // instance card is ~55 cells wide while the account card is still
        // drawn. Judging on `area.width` folded the name into a card that had
        // a full account card right next to it.
        self.overview_instance_card(frame, layout.instance, layout.account.is_empty(), pal);
        if !layout.account.is_empty() {
            self.overview_account_card(frame, layout.account, pal);
        }
        self.overview_stats(frame, layout.stats, pal);
        self.overview_activity(frame, layout.activity, pal);
    }

    fn overview_instance_card(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        fold_account: bool,
        pal: &Palette,
    ) {
        if area.is_empty() {
            return;
        }
        card(
            frame,
            area,
            &self.i18n.text("overview.current_instance"),
            pal,
        );
        // The whole card is the launch target.
        self.hitmap.add(
            area,
            Action::Page {
                page: Page::Overview,
                act: PageAct::Launch,
            },
        );
        let inner = card_inner(area);
        if inner.is_empty() {
            return;
        }
        let instance = self.current_instance();
        let account = self.current_account();
        let (name, mut detail) = match instance {
            Some(instance) => (
                instance.name.clone(),
                format!("{} · {}", instance.version, loader_text(self, instance)),
            ),
            None => (
                self.i18n.text("overview.no_instance"),
                self.i18n.text("overview.open_instances"),
            ),
        };
        // Only when the account card is not on screen does the name fold in
        // here; otherwise the same account would be drawn twice.
        if fold_account {
            if let Some(account) = &account {
                detail.push_str(" · ");
                detail.push_str(&account.username);
            }
        }
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                truncate(&name, usize::from(inner.width).saturating_sub(2)),
                Style::default()
                    .fg(if instance.is_some() {
                        pal.text
                    } else {
                        pal.muted
                    })
                    .bg(pal.surface)
                    .add_modifier(Modifier::BOLD),
            ))),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
        if inner.height > 1 {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    truncate(&detail, usize::from(inner.width)),
                    Style::default().fg(pal.muted).bg(pal.surface),
                ))),
                Rect::new(inner.x, inner.y + 1, inner.width, 1),
            );
        }
        if instance.is_some() {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "▶",
                    Style::default()
                        .fg(pal.accent)
                        .bg(pal.surface)
                        .add_modifier(Modifier::BOLD),
                )))
                .alignment(Alignment::Right),
                Rect::new(inner.x, inner.y, inner.width, 1),
            );
        }

        // The big numbers: playtime on the left, launches on the right. A short
        // hero cannot fit the eight-pixel glyphs, and dropping them would leave
        // the narrow overview missing two of the card's four facts, so they fall
        // back to one text line. Hours alone turned every new instance into `0h`
        // even after a real session; `compact_play_time` reports minutes instead.
        let playtime = instance
            .map(|instance| compact_play_time(instance.play_time_seconds))
            .unwrap_or_else(|| "—".to_owned());
        let launches = instance
            .map(|instance| instance.launch_count.to_string())
            .unwrap_or_else(|| "—".to_owned());
        if inner.height >= 6 {
            let numbers = Rect::new(inner.x, inner.y + 3, inner.width, inner.height - 3);
            let halves = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Fill(1), Constraint::Fill(1)])
                .split(numbers);
            for (area, value, label) in [
                (
                    halves[0],
                    playtime.clone(),
                    self.i18n.text("overview.played"),
                ),
                (
                    halves[1],
                    launches.clone(),
                    self.i18n.text("nightlight.launches"),
                ),
            ] {
                let rows = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Min(0), Constraint::Length(1)])
                    .split(area);
                // `Quadrant` draws two pixels per cell row, so an 8-pixel-tall
                // glyph needs four rows. Anything shorter silently loses the
                // bottom of every digit, which reads as "the number is cut".
                if rows[0].height >= BIG_NUMBER_ROWS {
                    frame.render_widget(widgets::big_number(&value, pal), rows[0]);
                } else {
                    frame.render_widget(
                        Paragraph::new(Line::from(Span::styled(
                            value,
                            Style::default()
                                .fg(pal.accent)
                                .bg(pal.surface)
                                .add_modifier(Modifier::BOLD),
                        ))),
                        rows[0],
                    );
                }
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        label,
                        Style::default().fg(pal.muted).bg(pal.surface),
                    ))),
                    rows[1],
                );
            }
        } else if instance.is_some() && inner.height >= 3 {
            // Short hero (80x24 and below): one text line keeps both numbers.
            let line = Line::from(vec![
                Span::styled(
                    playtime,
                    Style::default()
                        .fg(pal.accent2)
                        .bg(pal.surface)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" {}", self.i18n.text("overview.played")),
                    Style::default().fg(pal.muted).bg(pal.surface),
                ),
                Span::styled("  ·  ", Style::default().fg(pal.faint).bg(pal.surface)),
                Span::styled(
                    launches,
                    Style::default()
                        .fg(pal.accent2)
                        .bg(pal.surface)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" {}", self.i18n.text("nightlight.launches")),
                    Style::default().fg(pal.muted).bg(pal.surface),
                ),
            ]);
            frame.render_widget(
                Paragraph::new(line).style(Style::default().bg(pal.surface)),
                Rect::new(inner.x, inner.y + 2, inner.width, 1),
            );
        }
    }

    fn overview_account_card(&self, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
        if area.is_empty() {
            return;
        }
        card(
            frame,
            area,
            &self.i18n.text("overview.current_account"),
            pal,
        );
        self.hitmap.add(area, Action::Nav(Page::Accounts));
        let inner = card_inner(area);
        if inner.is_empty() {
            return;
        }
        let account = self.current_account();
        let (name, kind, session) = match &account {
            Some(account) => (
                account.username.clone(),
                self.i18n.text(
                    if account.kind == crate::core::account::models::AccountKind::Microsoft {
                        "account.kind.microsoft"
                    } else {
                        "account.kind.offline"
                    },
                ),
                self.account_expiry_label(account),
            ),
            None => (
                self.i18n.text("overview.no_account"),
                self.i18n.text("overview.open_accounts"),
                String::new(),
            ),
        };
        let mut lines = vec![Line::from(vec![
            Span::styled(
                "● ",
                Style::default()
                    .fg(if account.is_some() { pal.ok } else { pal.err })
                    .bg(pal.surface),
            ),
            Span::styled(
                truncate(&name, usize::from(inner.width).saturating_sub(3)),
                Style::default()
                    .fg(if account.is_some() {
                        pal.text
                    } else {
                        pal.muted
                    })
                    .bg(pal.surface)
                    .add_modifier(Modifier::BOLD),
            ),
        ])];
        if inner.height > 1 {
            lines.push(Line::from(Span::styled(
                truncate(&kind, usize::from(inner.width)),
                Style::default().fg(pal.muted).bg(pal.surface),
            )));
        }
        if inner.height > 2 && !session.is_empty() {
            lines.push(Line::from(Span::styled(
                truncate(&session, usize::from(inner.width)),
                Style::default().fg(pal.faint).bg(pal.surface),
            )));
        }
        frame.render_widget(
            Paragraph::new(lines),
            Rect::new(inner.x, inner.y, inner.width, inner.height),
        );
    }

    fn overview_stats(&self, frame: &mut Frame<'_>, areas: [Rect; 4], pal: &Palette) {
        let running = self
            .task_snapshots
            .iter()
            .filter(|snapshot| snapshot.state.is_active())
            .count();
        let java_ready = !self.java.is_empty();
        let entries = [
            (
                Metric::Instances,
                self.instances.items.len().to_string(),
                self.i18n.text("page.instances.short"),
                pal.accent2,
            ),
            (
                Metric::Tasks,
                running.to_string(),
                self.i18n.text("page.tasks.short"),
                pal.accent2,
            ),
            (
                Metric::Accounts,
                self.accounts.accounts.len().to_string(),
                self.i18n.text("page.accounts.short"),
                pal.accent2,
            ),
            (
                Metric::Java,
                if java_ready { "✓" } else { "—" }.to_owned(),
                // The label is the noun and the value says whether it is ready.
                // "Java not set —" reads as two answers to one question.
                "Java".to_owned(),
                if java_ready { pal.ok } else { pal.err },
            ),
        ];
        for (area, (metric, value, label, color)) in areas.into_iter().zip(entries) {
            if area.is_empty() {
                continue;
            }
            frame.render_widget(
                Block::default().style(Style::default().bg(pal.surface)),
                area,
            );
            // One row down and two columns in: the tile is a block of text, not
            // a corner-anchored label.
            frame.render_widget(
                stat(&value, &label, color, pal),
                Rect::new(
                    area.x.saturating_add(2),
                    area.y.saturating_add(area.height.saturating_sub(1) / 2),
                    area.width.saturating_sub(3),
                    1,
                ),
            );
            self.hitmap.add(
                area,
                Action::Page {
                    page: Page::Overview,
                    act: PageAct::Metric(metric),
                },
            );
        }
    }

    fn overview_activity(&self, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
        if area.is_empty() {
            return;
        }
        card(frame, area, &self.i18n.text("overview.active_tasks"), pal);
        self.hitmap.add(
            area,
            Action::Page {
                page: Page::Overview,
                act: PageAct::Activity,
            },
        );
        let inner = card_inner(area);
        if inner.is_empty() {
            return;
        }
        let active = self
            .task_snapshots
            .iter()
            .find(|snapshot| snapshot.state.is_active());
        let recent = self.launcher_logs.matching_lines("");
        let mut lines = Vec::new();
        let mut used = 0u16;
        if let Some(task) = active {
            lines.push(Line::from(Span::styled(
                truncate(&task.label, usize::from(inner.width)),
                Style::default().fg(pal.text).bg(pal.surface),
            )));
            used += 1;
            if used < inner.height {
                let ratio = match task.total {
                    Some(total) if total > 0 => task.completed as f64 / total as f64,
                    _ => 0.0,
                };
                frame.render_widget(
                    Paragraph::new(widgets::bar(ratio, inner.width, pal.accent, pal.accent2)),
                    Rect::new(inner.x, inner.y + used, inner.width, 1),
                );
                used += 1;
            }
            if used < inner.height {
                lines.push(Line::default());
                used += 1;
            }
        }
        if !recent.is_empty() && used + 1 < inner.height {
            lines.push(Line::from(Span::styled(
                self.i18n.text("nightlight.recent"),
                Style::default().fg(pal.muted).bg(pal.surface),
            )));
            used += 1;
            let budget = usize::from(inner.height - used);
            let tail = recent.len().saturating_sub(budget);
            for line in recent.iter().skip(tail) {
                lines.push(Line::from(Span::styled(
                    truncate(line, usize::from(inner.width)),
                    Style::default().fg(pal.faint).bg(pal.surface),
                )));
            }
        }
        if lines.is_empty() {
            lines.push(Line::from(Span::styled(
                self.i18n.text("overview.no_active_tasks"),
                Style::default().fg(pal.muted).bg(pal.surface),
            )));
        }
        frame.render_widget(
            Paragraph::new(lines),
            Rect::new(inner.x, inner.y, inner.width, inner.height),
        );
    }
}

fn loader_text(app: &App, instance: &InstanceSummary) -> String {
    app.instance_loader_text(instance)
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::layout;

    fn assert_no_overlap(rects: &[Rect]) {
        for (index, left) in rects.iter().enumerate() {
            for right in rects.iter().skip(index + 1) {
                assert!(
                    left.intersection(*right).is_empty(),
                    "{left:?} overlaps {right:?}"
                );
            }
        }
    }

    #[test]
    fn the_overview_grid_fills_the_page() {
        let area = Rect::new(0, 0, 93, 38);
        let grid = layout(area);
        assert!(grid.instance.width > 0 && grid.account.width > 0);
        assert_eq!(grid.instance.y, 1);
        // The activity card runs to the last row of the inner area.
        assert_eq!(grid.activity.bottom(), area.bottom() - 1);
        let mut all = vec![grid.instance, grid.account, grid.activity];
        all.extend(grid.stats);
        assert_no_overlap(&all);
        assert!(grid.instance.right() < grid.account.x);
        assert!(
            grid.stats
                .windows(2)
                .all(|pair| pair[0].right() < pair[1].x)
        );
    }

    #[test]
    fn a_narrow_page_drops_the_account_card() {
        let grid = layout(Rect::new(0, 0, 71, 22));
        assert!(grid.account.is_empty());
        assert!(grid.instance.width > 60);
        assert_eq!(grid.activity.bottom(), 21);
    }

    #[test]
    fn narrow_metrics_stack_two_by_two() {
        let grid = layout(Rect::new(0, 0, 64, 18));
        // Two columns, two rows: the first pair shares a row, the second pair
        // sits under it in the same columns.
        assert_eq!(grid.stats[0].y, grid.stats[1].y);
        assert!(grid.stats[2].y > grid.stats[1].y);
        assert_eq!(grid.stats[0].x, grid.stats[2].x);
        assert_eq!(grid.stats[1].x, grid.stats[3].x);
        assert!(grid.stats[0].right() < grid.stats[1].x);
        assert_no_overlap(&grid.stats);
    }

    #[test]
    fn wide_metrics_stay_on_one_row() {
        let grid = layout(Rect::new(0, 0, 93, 38));
        assert!(grid.stats.iter().all(|rect| rect.y == grid.stats[0].y));
        assert!(
            grid.stats
                .windows(2)
                .all(|pair| pair[0].right() < pair[1].x)
        );
    }
}
