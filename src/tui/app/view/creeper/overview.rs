//! Overview page: hero card, stat tiles and the activity feed.

use crate::tui::app::view::sheet::Sheet;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use super::tone::Palette;
use super::*;
use crate::tui::palette;
use crate::tui::widgets::kit as widgets;

impl App {
    pub(in crate::tui::app) fn draw_creeper_overview(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        t: f64,
    ) {
        if area.width < 20 || area.height < 6 {
            return;
        }
        let layout = overview_layout(area);
        // Tiles and cards are registered where they are laid out, so the hit
        // boxes are the drawn rectangles by construction.
        self.hitmap.add(layout.play, Action::LAUNCH);
        for (tile, metric) in layout.stats.iter().zip([
            crate::tui::action::Metric::Instances,
            crate::tui::action::Metric::Tasks,
            crate::tui::action::Metric::Accounts,
            crate::tui::action::Metric::Java,
        ]) {
            self.hitmap.add(*tile, Action::metric(metric));
        }
        self.hitmap.add(layout.activity, Action::ACTIVITY);
        self.hero(frame, layout.hero, layout.hero_left, layout.play, t);
        if layout.stats[0].width > 0 {
            self.stat_strip(frame, layout.stats, t);
        }
        if layout.activity.width > 0 {
            self.activity(frame, layout.activity, t);
        }
    }

    fn hero(&self, frame: &mut Frame<'_>, area: Rect, left: Rect, play: Rect, t: f64) {
        let pal = self.creeper_palette();
        let block = widgets::card(
            &self.i18n.text("shell.instance"),
            pal.lime,
            true,
            t,
            pal.surface,
        );
        frame.render_widget(block, area);
        if left.width == 0 {
            return;
        }

        let current = self
            .instances
            .current_id
            .and_then(|id| self.instances.items.iter().find(|item| item.id == id));
        let mut lines = vec![Line::from(theme_gradient(
            pal,
            "TERMINAL",
            pal.green,
            pal.cyan,
            t * 0.8,
        ))];
        if left.height >= 6 {
            lines.push(Line::from(theme_gradient(
                pal,
                "MINECRAFT LAUNCHER",
                pal.cyan,
                pal.purple,
                t * 0.55 + 1.0,
            )));
            lines.push(Line::from(""));
        }
        match current {
            Some(instance) => {
                lines.push(Line::from(vec![
                    Span::styled("◆ ", Style::default().fg(pal.green).bg(pal.surface)),
                    Span::styled(
                        palette::truncate(
                            &instance.name,
                            usize::from(left.width).saturating_sub(3),
                        ),
                        Style::default()
                            .fg(pal.text)
                            .bg(pal.surface)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]));
            }
            None => lines.push(Line::from(vec![
                Span::styled("◆ ", Style::default().fg(pal.gold).bg(pal.surface)),
                Span::styled(
                    self.i18n.text("overview.no_instance"),
                    Style::default()
                        .fg(pal.text)
                        .bg(pal.surface)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(
                        "  ·  n {}  ·  i {}",
                        self.i18n.text("action.new"),
                        self.i18n.text("action.import")
                    ),
                    Style::default().fg(pal.text_faint).bg(pal.surface),
                ),
            ])),
        }

        match self.current_account() {
            Some(account) => lines.push(Line::from(vec![
                Span::styled(
                    "◉ ",
                    Style::default()
                        .fg(palette::mix(
                            pal.green,
                            pal.text,
                            palette::pulse(t, 2.0) * 0.25,
                        ))
                        .bg(pal.surface),
                ),
                Span::styled(
                    account.username,
                    Style::default().fg(pal.text).bg(pal.surface),
                ),
                Span::styled(
                    format!(
                        "  ·  {}",
                        match account.kind {
                            crate::core::account::models::AccountKind::Microsoft => {
                                self.i18n.text("creeper.account.microsoft")
                            }
                            crate::core::account::models::AccountKind::Offline => {
                                self.i18n.text("creeper.account.offline")
                            }
                        }
                    ),
                    Style::default().fg(pal.text_faint).bg(pal.surface),
                ),
            ])),
            None => lines.push(Line::from(vec![
                Span::styled("○ ", Style::default().fg(pal.gold).bg(pal.surface)),
                Span::styled(
                    self.i18n.text("overview.no_account"),
                    Style::default().fg(pal.text_dim).bg(pal.surface),
                ),
            ])),
        }

        if left.height as usize > lines.len() {
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
            // The instance directory used to trail this row, but a deep path
            // never fits and only produced an ellipsis. Java is the one fact
            // worth keeping here.
            lines.push(Line::from(vec![
                Span::styled("▸ ", Style::default().fg(pal.cyan).bg(pal.surface)),
                Span::styled(java, Style::default().fg(pal.text_dim).bg(pal.surface)),
            ]));
        }
        if left.height as usize > lines.len()
            && let Some(instance) = current
        {
            let loader = instance.loader_version.as_ref().map_or_else(
                || self.loader_label(&instance.loader),
                |version| format!("{} {version}", self.loader_label(&instance.loader)),
            );
            lines.push(Line::from(vec![
                Span::styled("▸ ", Style::default().fg(pal.cyan).bg(pal.surface)),
                Span::styled(
                    palette::truncate(&loader, usize::from(left.width).saturating_sub(4)),
                    Style::default().fg(pal.text_dim).bg(pal.surface),
                ),
            ]));
        }
        frame.render_widget(
            Paragraph::new(lines).style(Style::default().bg(pal.surface)),
            left,
        );

        if play.width > 0 {
            self.play_button(frame, play, current.is_some(), t);
        }
    }

    fn play_button(&self, frame: &mut Frame<'_>, area: Rect, has_instance: bool, t: f64) {
        let pal = self.creeper_palette();
        let (accent, glyph, label, key) = if !has_instance {
            (pal.gold, "↓", self.i18n.text("action.new"), Some("n"))
        } else if self.current_account().is_none() {
            // Clicking opens the accounts page; there is no overview shortcut
            // for it, so the card shows no key cap rather than a wrong one.
            (pal.cyan, "◉", self.i18n.text("action.add_account"), None)
        } else {
            (
                pal.green,
                "▶",
                self.i18n.text("action.launch"),
                Some("Enter"),
            )
        };
        let button = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(palette::mix(
                accent,
                pal.text,
                0.1 + palette::pulse(t, 2.2) * 0.25,
            )))
            .style(Style::default().bg(pal.fill(accent, 0.1)));
        let inner = button.inner(area);
        frame.render_widget(button, area);
        if inner.height == 0 {
            return;
        }
        let mut lines = Vec::new();
        if inner.height >= 3 {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            glyph,
            Style::default()
                .fg(palette::brighten(accent, 0.1))
                .bg(pal.fill(accent, 0.1))
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::styled(
            palette::truncate(&label, usize::from(inner.width)),
            Style::default()
                .fg(pal.text)
                .bg(pal.fill(accent, 0.1))
                .add_modifier(Modifier::BOLD),
        )));
        if inner.height >= 4
            && let Some(key) = key
        {
            lines.push(Line::from(Span::styled(
                key.to_string(),
                Style::default()
                    .fg(palette::mix(accent, pal.text, 0.3))
                    .bg(pal.fill(accent, 0.1)),
            )));
        }
        frame.render_widget(
            Paragraph::new(lines)
                .alignment(ratatui::layout::Alignment::Center)
                .style(Style::default().bg(pal.fill(accent, 0.1))),
            inner,
        );
    }

    fn stat_strip(&self, frame: &mut Frame<'_>, tiles_area: [Rect; 4], t: f64) {
        let pal = self.creeper_palette();
        let running = self
            .task_snapshots
            .iter()
            .filter(|task| {
                matches!(
                    task.state,
                    crate::core::task::TaskState::Running | crate::core::task::TaskState::Paused
                )
            })
            .count();
        let tiles = [
            (
                self.instances.items.len().to_string(),
                self.i18n.text("page.instances"),
                pal.green,
            ),
            (
                running.to_string(),
                self.i18n.text("task.running"),
                pal.purple,
            ),
            (
                self.accounts.accounts.len().to_string(),
                self.i18n.text("page.accounts"),
                pal.pink,
            ),
            (self.java.len().to_string(), "Java".to_owned(), pal.gold),
        ];
        for (index, (value, caption, color)) in tiles.iter().enumerate() {
            widgets::stat_tile(frame, tiles_area[index], value, caption, *color, t);
        }
    }

    fn activity(&self, frame: &mut Frame<'_>, area: Rect, t: f64) {
        let pal = self.creeper_palette();
        let block = widgets::card("ACTIVITY", pal.cyan, false, t, pal.surface);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.height == 0 || inner.width < 10 {
            return;
        }
        let mut lines = Vec::new();

        // A running game is activity too: without this the card stayed blank
        // while the launcher was visibly starting Minecraft.
        if let Some(launch) = &self.active_launch {
            let (kind, accent, phase) = match launch.phase {
                LaunchPhase::Starting => (
                    StatusKind::Queued,
                    pal.gold,
                    self.i18n.text("overview.launching"),
                ),
                LaunchPhase::Running => (
                    StatusKind::Running,
                    pal.green,
                    self.i18n.text("task.running"),
                ),
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {} ", widgets::status_glyph(kind)),
                    Style::default()
                        .fg(palette::mix(
                            accent,
                            pal.text,
                            palette::pulse(t, 2.0) * 0.25,
                        ))
                        .bg(pal.surface)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    palette::truncate(
                        &launch.instance_name,
                        usize::from(inner.width.saturating_sub(24)),
                    ),
                    Style::default()
                        .fg(pal.text)
                        .bg(pal.surface)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(
                        "  {phase}  {}",
                        palette::format_elapsed(launch.started.elapsed().as_secs())
                    ),
                    Style::default().fg(pal.text_faint).bg(pal.surface),
                ),
            ]));
            if inner.height >= 2 {
                let width = usize::from(inner.width.saturating_sub(2)).min(32);
                lines.push(Line::from(widgets::progress_spans(
                    None,
                    width,
                    accent,
                    pal.lime,
                    t,
                    pal.surface,
                )));
            }
        }

        let remaining = usize::from(inner.height).saturating_sub(lines.len());
        if self.task_snapshots.is_empty() {
            if lines.is_empty() {
                lines.push(Line::from(Span::styled(
                    self.i18n.text("overview.no_active_tasks"),
                    Style::default().fg(pal.text_faint).bg(pal.surface),
                )));
            }
        } else {
            for task in self.task_snapshots.iter().rev().take(remaining) {
                let kind = task_kind(task.state);
                let ratio = task
                    .total
                    .filter(|total| *total > 0)
                    .map(|total| task.completed as f64 / total as f64);
                let mut spans = vec![
                    Span::styled(
                        format!(" {} ", widgets::status_glyph(kind)),
                        Style::default()
                            .fg(widgets::status_color(kind))
                            .bg(pal.surface)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        palette::truncate(&task.label, usize::from(inner.width.saturating_sub(22))),
                        Style::default().fg(pal.text).bg(pal.surface),
                    ),
                    Span::styled(
                        format!(
                            "  {}/{}",
                            task.completed,
                            task.total
                                .map_or_else(|| "?".into(), |total| total.to_string())
                        ),
                        Style::default().fg(pal.text_faint).bg(pal.surface),
                    ),
                ];
                if let Some(ratio) = ratio {
                    let bar: Vec<Span<'static>> = widgets::progress_spans(
                        Some(ratio),
                        6,
                        widgets::status_color(kind),
                        pal.lime,
                        t,
                        pal.surface,
                    );
                    spans.push(Span::styled(" ", Style::default().bg(pal.surface)));
                    spans.extend(bar);
                }
                lines.push(Line::from(spans));
            }
            if lines.len() < usize::from(inner.height) {
                let label = format!("— {} —", self.i18n.text("creeper.feed_end"));
                let pad = usize::from(inner.width).saturating_sub(palette::width(&label)) / 2;
                lines.push(Line::from(Span::styled(
                    format!("{}{label}", " ".repeat(pad)),
                    Style::default()
                        .fg(palette::dim(pal.text_faint, 0.25))
                        .bg(pal.surface),
                )));
            }
        }
        frame.render_widget(
            Paragraph::new(lines).style(Style::default().bg(pal.surface)),
            inner,
        );
    }
}

/// Geometry for the creeper overview, shared by drawing and click routing.
///
/// Keeping this in one place is what makes the cards clickable: the renderer
/// and [`App::overview_click`] both derive their rectangles here, so a layout
/// tweak cannot silently move a card away from its hit box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::tui::app) struct OverviewLayout {
    pub hero: Rect,
    pub hero_left: Rect,
    pub play: Rect,
    pub stats: [Rect; 4],
    pub activity: Rect,
}

pub(in crate::tui::app) fn overview_layout(area: Rect) -> OverviewLayout {
    let show_stats = area.height >= 16;
    let show_activity = area.height >= 22;
    let hero_height = if area.height >= 30 { 10 } else { 8 };
    let mut constraints = vec![Constraint::Length(hero_height)];
    if show_stats {
        constraints.push(Constraint::Length(1));
        constraints.push(Constraint::Length(3));
    }
    if show_activity {
        constraints.push(Constraint::Length(1));
        constraints.push(Constraint::Min(4));
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let hero = rows[0];
    let inner = Rect {
        x: hero.x.saturating_add(1),
        y: hero.y.saturating_add(1),
        width: hero.width.saturating_sub(2),
        height: hero.height.saturating_sub(2),
    };
    let (hero_left, play) = if inner.height < 3 || inner.width < 30 {
        (Rect::default(), Rect::default())
    } else {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(58),
                Constraint::Length(1),
                Constraint::Min(20),
            ])
            .split(inner);
        let right = columns[2];
        // A compact button centred in the right column reads better than one
        // stretched to the full hero height. Leaving one row above and below
        // keeps the optical spacing even.
        let play_width = right.width.min(26);
        let play_height = right.height.saturating_sub(2).clamp(5, 7).min(right.height);
        (
            columns[0],
            Rect::new(
                right.x + right.width.saturating_sub(play_width) / 2,
                right.y + right.height.saturating_sub(play_height) / 2,
                play_width,
                play_height,
            ),
        )
    };

    let mut stats = [Rect::default(); 4];
    if show_stats {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Ratio(1, 4),
                Constraint::Length(1),
                Constraint::Ratio(1, 4),
                Constraint::Length(1),
                Constraint::Ratio(1, 4),
                Constraint::Length(1),
                Constraint::Ratio(1, 4),
            ])
            .split(rows[2]);
        for (index, tile) in stats.iter_mut().enumerate() {
            *tile = columns[index * 2];
        }
    }

    let activity = if show_activity {
        rows[rows.len() - 1]
    } else {
        Rect::default()
    };

    OverviewLayout {
        hero,
        hero_left,
        play,
        stats,
        activity,
    }
}

/// Gradient heading with a trailing dim caption.
fn theme_gradient(
    pal: &Palette,
    text: &str,
    from: ratatui::style::Color,
    to: ratatui::style::Color,
    phase: f64,
) -> Vec<Span<'static>> {
    let spaced = text
        .chars()
        .map(|ch| ch.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    palette::gradient_spans(&spaced, from, to, pal.surface, phase, true)
}

pub(in crate::tui::app) fn task_kind(state: crate::core::task::TaskState) -> StatusKind {
    match state {
        crate::core::task::TaskState::Queued => StatusKind::Queued,
        crate::core::task::TaskState::Running => StatusKind::Running,
        crate::core::task::TaskState::Paused => StatusKind::Warning,
        crate::core::task::TaskState::Succeeded => StatusKind::Completed,
        crate::core::task::TaskState::Failed => StatusKind::Failed,
        crate::core::task::TaskState::Cancelled => StatusKind::Missing,
    }
}
