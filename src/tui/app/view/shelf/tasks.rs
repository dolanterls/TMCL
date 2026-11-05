//! `shelf` tasks: groups of two-row entries under a filter line.
//!
//! A task is a name with its state on the right, then a block bar under it.
//! Failed jobs get a third row for the reason — the one line nobody should have
//! to open a dialog to read.

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::tone::Palette;
use super::widgets;
use super::*;
use crate::core::task::TaskState;
use crate::tui::action::{Action, ScrollTarget};
use crate::tui::app::view::TaskGroup;
use crate::tui::hit::RowKind;
use crate::tui::pages::tasks::TaskFilter;
use crate::tui::palette;
use crate::tui::widgets::rows::RowWindow;
use crate::tui::widgets::status::StatusKind;

/// Rows one entry takes when it has nothing more to say.
const ENTRY_ROWS: u16 = 2;
/// Columns between the selection bar and the entry's text.
const TEXT_INSET: u16 = 2;

enum TaskLine {
    Group(TaskGroup),
    Task(usize),
}

impl TaskLine {
    fn rows(&self, rows: &[crate::tui::app::state::tasks::TaskRow]) -> u16 {
        match self {
            Self::Group(_) => 1,
            Self::Task(index) => rows.get(*index).map_or(ENTRY_ROWS, |row| {
                ENTRY_ROWS + u16::from(row.failure.is_some())
            }),
        }
    }
}

impl App {
    pub(in crate::tui::app) fn draw_shelf_tasks(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &Palette,
    ) {
        if area.width < 24 || area.height < 4 {
            return;
        }
        self.shelf_task_filters(frame, Rect::new(area.x, area.y, area.width, 1), pal);
        widgets::rule(
            frame,
            Rect::new(area.x, area.y.saturating_add(1), area.width, 1),
            pal,
        );
        let body = Rect::new(
            area.x,
            area.y.saturating_add(2),
            area.width,
            area.height.saturating_sub(2),
        );
        if body.height == 0 {
            return;
        }
        self.hitmap.scroll(body, ScrollTarget::Rows(RowKind::Tasks));

        let rows = self.task_rows();
        if rows.is_empty() {
            let text = self.i18n.text("task.no_filter_results");
            frame.render_widget(
                Paragraph::new(Span::styled(
                    palette::truncate(&text, usize::from(body.width)),
                    Style::default().fg(pal.faint).bg(pal.bg),
                )),
                Rect::new(body.x, body.y, body.width, 1),
            );
            return;
        }

        let lines = Self::shelf_task_lines(&rows);
        let selected = self.task_selected.min(rows.len().saturating_sub(1));
        let selected_line = lines
            .iter()
            .position(|line| matches!(line, TaskLine::Task(index) if *index == selected))
            .unwrap_or(0);
        let selected_top = lines
            .iter()
            .take(selected_line)
            .map(|line| line.rows(&rows))
            .sum::<u16>();
        let total_rows = lines.iter().map(|line| line.rows(&rows)).sum::<u16>();
        let offset = self.row_windows.resolve_offset(
            RowWindow::Tasks,
            usize::from(total_rows),
            usize::from(selected_top),
            usize::from(body.height),
        );
        // The window is counted in rows, but an entry is two or three rows tall:
        // an offset landing inside one would open the page on a nameless
        // progress bar. Round it back to the entry's first row.
        let offset =
            u16::try_from(Self::shelf_task_line_start(&lines, &rows, offset)).unwrap_or(u16::MAX);

        let mut y = i32::from(body.y) - i32::from(offset);
        for line in &lines {
            let height = i32::from(line.rows(&rows));
            let top = y;
            y += height;
            if y <= i32::from(body.y) || top >= i32::from(body.bottom()) {
                continue;
            }
            match line {
                TaskLine::Group(group) => {
                    if top >= i32::from(body.y) {
                        self.shelf_task_group(
                            frame,
                            Rect::new(body.x, top as u16, body.width, 1),
                            pal,
                            *group,
                        );
                    }
                }
                TaskLine::Task(index) => {
                    let Some(row) = rows.get(*index) else {
                        continue;
                    };
                    // A clipped entry has no name row and no hit box: start the
                    // window on a whole one instead of half-painting it.
                    if top < i32::from(body.y) {
                        continue;
                    }
                    let bottom = (top + height).min(i32::from(body.bottom()));
                    if bottom <= top {
                        continue;
                    }
                    // The hit box is clipped with the paint: an entry cut by
                    // the bottom edge must not cover the footer's key row.
                    let rect = Rect::new(body.x, top as u16, body.width, (bottom - top) as u16);
                    self.shelf_task_entry(frame, rect, pal, row, *index == selected);
                    self.hitmap.add(rect, Action::row(Page::Tasks, *index));
                }
            }
        }
    }

    /// Filter names with their counts, the current one underlined. The chips
    /// are the pointer's way to change the filter; `[`/`]` keep working.
    fn shelf_task_filters(&self, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
        let current = TaskFilter::ALL
            .iter()
            .position(|filter| *filter == self.task_filter)
            .unwrap_or(0);
        let labels = TaskFilter::ALL
            .iter()
            .map(|filter| {
                format!(
                    "{} {}",
                    self.i18n.text(filter.label_key()),
                    self.task_filter_count(*filter)
                )
            })
            .collect::<Vec<_>>();
        for (index, rect) in widgets::fit_labels(area, &labels, current, area.right()) {
            let style = if index == current {
                Style::default()
                    .fg(pal.accent_light)
                    .bg(pal.bg)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::default().fg(pal.dim).bg(pal.bg)
            };
            let Some(label) = labels.get(index) else {
                continue;
            };
            frame.render_widget(
                Paragraph::new(Span::styled(
                    palette::truncate(label, usize::from(rect.width)),
                    style,
                )),
                rect,
            );
            self.hitmap.add(rect, Action::filter(Page::Tasks, index));
        }
    }

    fn shelf_task_lines(rows: &[crate::tui::app::state::tasks::TaskRow]) -> Vec<TaskLine> {
        let mut lines = Vec::with_capacity(rows.len());
        let mut current = None;
        for (index, row) in rows.iter().enumerate() {
            let group = TaskGroup::of(row.state);
            if current != Some(group) {
                lines.push(TaskLine::Group(group));
                current = Some(group);
            }
            lines.push(TaskLine::Task(index));
        }
        lines
    }

    /// The first row of the entry that contains `offset`.
    fn shelf_task_line_start(
        lines: &[TaskLine],
        rows: &[crate::tui::app::state::tasks::TaskRow],
        offset: usize,
    ) -> usize {
        let mut start = 0usize;
        for line in lines {
            let next = start.saturating_add(usize::from(line.rows(rows)));
            if next > offset {
                return start;
            }
            start = next;
        }
        start
    }

    fn shelf_task_group(&self, frame: &mut Frame<'_>, area: Rect, pal: &Palette, group: TaskGroup) {
        let label = self.i18n.text(group.label_key());
        let width = palette::width(&label) as u16;
        frame.render_widget(
            Paragraph::new(Span::styled(
                label,
                Style::default()
                    .fg(pal.dim)
                    .bg(pal.bg)
                    .add_modifier(Modifier::BOLD),
            )),
            Rect::new(area.x, area.y, width.min(area.width), 1),
        );
        let rule_x = area.x.saturating_add(width).saturating_add(2);
        if rule_x < area.right() {
            widgets::rule(
                frame,
                Rect::new(rule_x, area.y, area.right().saturating_sub(rule_x), 1),
                pal,
            );
        }
    }

    fn shelf_task_entry(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &Palette,
        row: &crate::tui::app::state::tasks::TaskRow,
        active: bool,
    ) {
        if area.height == 0 {
            return;
        }
        let background = if active { pal.surface } else { pal.bg };
        if active {
            widgets::selection_band(frame, area, pal);
        }

        let status = localize_task_state(row.state, &self.i18n);
        let status_width = palette::width(&status) as u16;
        let name_x = area.x.saturating_add(TEXT_INSET);
        let name_width = area.width.saturating_sub(TEXT_INSET + status_width + 2);
        let name = if row.steps > 1 {
            format!("{}  ({}/{})", row.label, row.step, row.steps)
        } else {
            row.label.clone()
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                palette::truncate(&name, usize::from(name_width)),
                Style::default()
                    .fg(if active { pal.text } else { pal.dim })
                    .bg(background)
                    .add_modifier(if active {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            )),
            Rect::new(name_x, area.y, name_width, 1),
        );
        frame.render_widget(
            Paragraph::new(Span::styled(
                status,
                Style::default()
                    .fg(self.shelf_task_status_color(row.state, pal))
                    .bg(background),
            ))
            .alignment(Alignment::Right),
            Rect::new(area.x, area.y, area.width, 1),
        );

        let progress_y = area.y.saturating_add(1);
        if progress_y < area.bottom() {
            let line = Rect::new(name_x, progress_y, area.right().saturating_sub(name_x), 1);
            self.shelf_task_progress(frame, row, line, background, pal);
        }

        if let Some(failure) = &row.failure {
            let failure_y = area.y.saturating_add(2);
            if failure_y < area.bottom() {
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        format!(
                            "✕ {}",
                            palette::truncate(failure, usize::from(area.width.saturating_sub(4)))
                        ),
                        Style::default().fg(pal.error).bg(background),
                    )),
                    Rect::new(name_x, failure_y, area.width.saturating_sub(TEXT_INSET), 1),
                );
            }
        }
    }

    /// The bar closes the entry under its name.
    ///
    /// It is pinned to the right edge, under the state label, rather than run
    /// out from the name's left margin: the two lines then read as two columns,
    /// and a list of short titles stops looking like a ragged set of blocks.
    /// The byte count (`transfer_label`) keeps the left side, where a full
    /// `1.3/1.3 GB` still has room.
    fn shelf_task_progress(
        &self,
        frame: &mut Frame<'_>,
        row: &crate::tui::app::state::tasks::TaskRow,
        area: Rect,
        background: ratatui::style::Color,
        pal: &Palette,
    ) {
        let ratio = task_ratio(row.state, row.completed, row.total);
        let bar_width = if area.width >= 60 { 12 } else { 6 };
        let (filled, empty) = match ratio {
            Some(ratio) => {
                let filled = (ratio * f64::from(bar_width)).round() as u16;
                let filled = filled.min(bar_width);
                (filled, bar_width.saturating_sub(filled))
            }
            None => (0, bar_width),
        };
        let percent = format!("  {}", percent_label(ratio));
        let percent_width = u16::try_from(palette::width(&percent)).unwrap_or(u16::MAX);
        let block_width = bar_width.saturating_add(percent_width).min(area.width);
        let block_x = area.right().saturating_sub(block_width);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    "█".repeat(usize::from(filled)),
                    Style::default().fg(pal.accent).bg(background),
                ),
                Span::styled(
                    "░".repeat(usize::from(empty)),
                    Style::default().fg(pal.faint).bg(background),
                ),
                Span::styled(percent, Style::default().fg(pal.dim).bg(background)),
            ])),
            Rect::new(block_x, area.y, block_width, 1),
        );
        if let Some(transfer) = row.transfer_label() {
            let gap = block_x.saturating_sub(area.x).saturating_sub(2);
            if gap > 0 {
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        palette::truncate(&transfer, usize::from(gap)),
                        Style::default().fg(pal.faint).bg(background),
                    )),
                    Rect::new(area.x, area.y, gap, 1),
                );
            }
        }
    }

    fn shelf_task_status_color(&self, state: TaskState, pal: &Palette) -> ratatui::style::Color {
        match task_status_kind(state) {
            StatusKind::Running | StatusKind::Completed | StatusKind::Ready => pal.ok,
            StatusKind::Warning | StatusKind::Queued => pal.warn,
            StatusKind::Failed => pal.error,
            _ => pal.dim,
        }
    }
}
