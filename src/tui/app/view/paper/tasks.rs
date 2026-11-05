//! `paper` tasks: jobs set as short entries under group headings.
//!
//! A task is two lines — name and status, then progress — because a printed
//! catalogue never fits a progress bar and a label on one line. Groups are
//! separated by a heading with a hairline running to the page edge.

use super::*;
use crate::core::task::TaskState;
use crate::tui::hit::RowKind;
use crate::tui::palette;
use crate::tui::widgets::rows::RowWindow;

/// Margin before the name, matching the instance list.
const MARGIN: u16 = 3;
/// Rows a task occupies: name, progress.
const TASK_ROWS: u16 = 2;

enum TaskLine {
    Group(TaskGroup),
    Task(usize),
}

impl TaskLine {
    fn rows(&self) -> u16 {
        match self {
            Self::Group(_) => 1,
            Self::Task(_) => TASK_ROWS,
        }
    }
}

impl App {
    pub(in crate::tui::app) fn draw_paper_tasks(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &PaperPalette,
    ) {
        if area.width < 24 || area.height < 4 {
            return;
        }
        let rows = self.task_rows();
        let count = self
            .i18n
            .format("task.count", &[("count", &rows.len().to_string())]);
        let mut title = self.i18n.text("page.tasks");
        if self.task_filter != crate::tui::pages::tasks::TaskFilter::All {
            title = format!("{title} · {}", self.i18n.text(self.task_filter.label_key()));
        }
        let body = kit::section(frame, area, pal, &title, Some(&count));
        self.hitmap
            .scroll(body, crate::tui::action::ScrollTarget::Rows(RowKind::Tasks));
        if rows.is_empty() {
            self.paper_message_at(frame, body, pal, "task.no_filter_results");
            return;
        }

        let lines = Self::task_lines(&rows);
        let selected = self.task_selected.min(rows.len().saturating_sub(1));
        let selected_line = lines
            .iter()
            .position(|line| matches!(line, TaskLine::Task(index) if *index == selected))
            .unwrap_or(0);
        let selected_top = lines
            .iter()
            .take(selected_line)
            .map(TaskLine::rows)
            .sum::<u16>();
        let total_rows = lines.iter().map(TaskLine::rows).sum::<u16>();
        let offset = self.row_windows.resolve_offset(
            RowWindow::Tasks,
            usize::from(total_rows),
            usize::from(selected_top),
            usize::from(body.height),
        );
        // The window is counted in lines, but an entry is two lines tall: an
        // offset that lands inside one used to open the page on a nameless
        // progress bar. Round it back to the entry's first line.
        let offset = u16::try_from(Self::task_line_start(&lines, offset)).unwrap_or(u16::MAX);

        // Rows are drawn in a signed coordinate so a partially scrolled entry
        // can be clipped at either edge instead of jumping a whole line.
        let mut y = i32::from(body.y) - i32::from(offset);
        for line in &lines {
            let height = i32::from(line.rows());
            let top = y;
            y += height;
            if y <= i32::from(body.y) {
                continue;
            }
            if top >= i32::from(body.bottom()) {
                break;
            }
            match line {
                TaskLine::Group(group) => {
                    if top >= i32::from(body.y) {
                        self.paper_task_group(
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
                    // An entry clipped at its top would print its progress bar
                    // with no name, and it has no hitbox either, so it is not
                    // even clickable. Start the window on a whole entry.
                    if top < i32::from(body.y) {
                        continue;
                    }
                    let bottom = (top + height).min(i32::from(body.bottom()));
                    if bottom <= top {
                        continue;
                    }
                    let rect = Rect::new(body.x, top as u16, body.width, (bottom - top) as u16);
                    self.paper_task_entry(frame, rect, pal, row, *index == selected);
                    // The whole two-row entry is the target, not just its name.
                    self.hitmap.add(
                        Rect::new(body.x, top as u16, body.width, TASK_ROWS),
                        Action::row(Page::Tasks, *index),
                    );
                }
            }
        }
    }

    /// The first line of the entry that contains `offset`.
    fn task_line_start(lines: &[TaskLine], offset: usize) -> usize {
        let mut start = 0usize;
        for line in lines {
            let next = start.saturating_add(usize::from(line.rows()));
            if next > offset {
                return start;
            }
            start = next;
        }
        start
    }

    fn task_lines(rows: &[crate::tui::app::state::tasks::TaskRow]) -> Vec<TaskLine> {
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

    fn paper_task_group(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &PaperPalette,
        group: TaskGroup,
    ) {
        let label = self.i18n.text(group.label_key());
        let width = palette::width(&label) as u16;
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                label,
                ink(pal, pal.dim).add_modifier(Modifier::BOLD),
            ))),
            Rect::new(area.x, area.y, width.min(area.width), 1),
        );
        let rule_x = area.x.saturating_add(width).saturating_add(2);
        if rule_x < area.right() {
            kit::rule(
                frame,
                Rect::new(rule_x, area.y, area.right().saturating_sub(rule_x), 1),
                pal,
                pal.rule_soft,
            );
        }
    }

    /// One entry: name over progress. `full` is false when only the lower half
    /// survived the top clip, in which case the name row is skipped.
    fn paper_task_entry(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &PaperPalette,
        row: &crate::tui::app::state::tasks::TaskRow,
        active: bool,
    ) {
        // The caller only hands over entries that start inside the body, so the
        // name row is always drawn; a bottom-clipped entry simply loses its
        // progress row, which is the half that can be inferred.
        if area.height > 0 {
            if active {
                kit::selection(frame, area, pal);
            }
            let status = localize_task_state(row.state, &self.i18n);
            let status_width = palette::width(&status) as u16;
            let name_x = area.x.saturating_add(MARGIN);
            let name_width = area.width.saturating_sub(MARGIN + status_width + 2);
            let name = if row.steps > 1 {
                format!("{}  ({}/{})", row.label, row.step, row.steps)
            } else {
                row.label.clone()
            };
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    palette::truncate(&name, usize::from(name_width)),
                    if active {
                        ink_marked(pal)
                    } else {
                        ink(pal, pal.ink)
                    },
                ))),
                Rect::new(name_x, area.y, name_width, 1),
            );
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    status,
                    ink(pal, self.paper_task_status_color(row.state, pal)),
                )))
                .alignment(Alignment::Right),
                Rect::new(area.x, area.y, area.width, 1),
            );
        }

        let progress_y = area.y.saturating_add(1);
        if progress_y >= area.bottom() {
            return;
        }
        let ratio = task_ratio(row.state, row.completed, row.total);
        let bar_width = if area.width >= 60 { 12 } else { 6 };
        let (filled, empty) = match ratio {
            Some(ratio) => {
                let filled = (ratio * f64::from(bar_width)).round() as u16;
                (
                    filled.min(bar_width),
                    bar_width.saturating_sub(filled.min(bar_width)),
                )
            }
            None => (0, bar_width),
        };
        let mut spans = vec![
            Span::styled("━".repeat(usize::from(filled)), ink(pal, pal.soft)),
            Span::styled("╌".repeat(usize::from(empty)), ink(pal, pal.rule)),
            Span::styled(format!("  {}", percent_label(ratio)), ink(pal, pal.faint)),
        ];
        if let Some(transfer) = row.transfer_label() {
            spans.push(Span::styled(format!("  {transfer}"), ink(pal, pal.faint)));
        }
        // The bar closes the entry on the right, under its status word: left
        // aligned it read as a second column of the list rather than as the
        // progress of the line above.
        frame.render_widget(
            Paragraph::new(Line::from(spans)).alignment(Alignment::Right),
            Rect::new(
                area.x.saturating_add(MARGIN),
                progress_y,
                area.width.saturating_sub(MARGIN),
                1,
            ),
        );
    }

    fn paper_task_status_color(&self, state: TaskState, pal: &PaperPalette) -> Color {
        match state {
            TaskState::Running | TaskState::Succeeded => pal.ok,
            TaskState::Paused => pal.warn,
            TaskState::Queued => pal.dim,
            TaskState::Failed => pal.accent,
            TaskState::Cancelled => pal.faint,
        }
    }

    /// A one-line note at `area`'s top-left, for empty and error states.
    pub(in crate::tui::app) fn paper_message_at(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &PaperPalette,
        key: &str,
    ) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                palette::truncate(&self.i18n.text(key), usize::from(area.width)),
                ink(pal, pal.faint),
            ))),
            Rect::new(area.x, area.y, area.width, 1),
        );
    }
}
