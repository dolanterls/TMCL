//! Tasks page: filter chips and three-row task cards with a block progress bar.
//! Design inspiration: nightlightd (GPL-3.0-or-later).
//! This file is an independent implementation; no source code is copied.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::core::task::TaskState;
use crate::tui::app::view::nightlight::tone::Palette;
use crate::tui::app::view::nightlight::widgets::{self, card, card_inner, pill, right_cell};
use crate::tui::hit::RowKind;
use crate::tui::pages::tasks::TaskFilter;
use crate::tui::palette::truncate;
use crate::tui::widgets::rows::{RowLayout, RowWindow};

use crate::tui::app::state::data_format::{format_progress_bytes, percent_label, task_ratio};

use super::{App, Page};

const CARD_ROWS: u16 = 3;
const CARD_GAP: u16 = 1;
/// Width of the bar. Pinned, so the bars of a list line up as a column.
const BAR_CELLS: u16 = 16;
/// Room the byte count needs before the bar is allowed to take its full width.
const SIZES_CELLS: u16 = 10;
/// `100%` / ` 42%` / `   —`.
const PERCENT_CELLS: u16 = 4;

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
    pub(in crate::tui::app) fn draw_nightlight_tasks(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &Palette,
    ) {
        let [pills, body] = layout(area);
        let labels: Vec<String> = TaskFilter::ALL
            .iter()
            .map(|filter| {
                format!(
                    "{} {}",
                    self.i18n.text(filter.label_key()),
                    filter.indices(&self.task_snapshots).len()
                )
            })
            .collect();
        let active = TaskFilter::ALL
            .iter()
            .position(|filter| *filter == self.task_filter)
            .unwrap_or(0);
        self.draw_pill_row(frame, pills, &labels, active, Page::Tasks, pal);

        // One row per job: the download and the two install steps of a modpack
        // share a bar instead of showing up as three unrelated cards.
        let visible = self.task_rows();
        self.hitmap
            .scroll(body, crate::tui::action::ScrollTarget::Rows(RowKind::Tasks));
        if visible.is_empty() {
            card(frame, body, &self.i18n.text("page.tasks"), pal);
            let inner = card_inner(body);
            if !inner.is_empty() {
                frame.render_widget(
                    Paragraph::new(self.i18n.text("task.no_filter_results"))
                        .style(Style::default().fg(pal.muted).bg(pal.surface)),
                    inner,
                );
            }
            return;
        }
        let selected = self.task_selected.min(visible.len().saturating_sub(1));
        card(frame, body, &self.i18n.text("page.tasks"), pal);
        let inner = card_inner(body);
        if inner.height < 2 {
            return;
        }
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" {}  ·  {}/{}", visible.len(), selected + 1, visible.len()),
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
            RowWindow::Tasks,
            RowLayout::cards_bare(rows, visible.len(), selected, CARD_ROWS, CARD_GAP),
        );
        self.hitmap.rows(layout, RowKind::Tasks);
        let stride = CARD_ROWS + CARD_GAP;
        let start = layout.offset();
        for (visual, position) in visible
            .iter()
            .skip(start)
            .take(layout.visible_rows())
            .enumerate()
        {
            let task = position;
            let y = rows.y.saturating_add(visual as u16 * stride);
            let height = CARD_ROWS.min(rows.bottom().saturating_sub(y));
            if height == 0 {
                break;
            }
            self.task_card(
                frame,
                Rect::new(rows.x, y, rows.width, height),
                task,
                start + visual == selected,
                pal,
            );
        }
    }

    fn task_card(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        task: &crate::tui::app::state::tasks::TaskRow,
        active: bool,
        pal: &Palette,
    ) {
        let background = if active { pal.selection } else { pal.surface };
        frame.render_widget(
            Block::default().style(Style::default().bg(background)),
            area,
        );
        let (label, color) = state_label(task.state, &self.i18n, pal);
        let badge = pill(&label, color, pal);
        let step = if task.steps > 1 {
            format!("  {}/{}", task.step, task.steps)
        } else {
            String::new()
        };
        let used = 4 + widgets::width(&label) + widgets::width(&step) + 2;
        let name_width = usize::from(area.width).saturating_sub(used).max(6);
        let mut head = vec![
            Span::styled(
                if active { "▎ " } else { "  " },
                Style::default().fg(pal.accent).bg(background),
            ),
            Span::styled(
                truncate(&task.label, name_width),
                Style::default()
                    .fg(pal.text)
                    .bg(background)
                    .add_modifier(Modifier::BOLD),
            ),
        ];
        head.push(Span::styled(
            step,
            Style::default().fg(pal.faint).bg(background),
        ));
        push_right(&mut head, badge, usize::from(area.width), background);
        frame.render_widget(
            Paragraph::new(Line::from(head)).style(Style::default().bg(background)),
            Rect::new(area.x, area.y, area.width, 1),
        );
        if area.height >= 2 {
            let ratio = task_ratio(task.state, task.completed, task.total);
            let sizes = format_progress_bytes(task.completed, task.total);
            // Fixed column edges: sizes end where the bar begins, and the bar
            // and the percentage keep the same cells on every row. Sizing the
            // bar from the byte count made each row end somewhere else.
            let gutter = 3;
            let percent_width = PERCENT_CELLS;
            let bar_width = BAR_CELLS.min(
                area.width
                    .saturating_sub(gutter + SIZES_CELLS + 1 + percent_width),
            );
            // Two gaps: one before the bar, one before the percentage.
            let sizes_width = area
                .width
                .saturating_sub(gutter + 1 + bar_width + 1 + percent_width);
            let mut line = vec![
                Span::styled(
                    if active { "▎  " } else { "   " },
                    Style::default().fg(pal.accent).bg(background),
                ),
                Span::styled(
                    right_cell(&sizes, usize::from(sizes_width)),
                    Style::default().fg(pal.accent2).bg(background),
                ),
                Span::styled(" ", Style::default().bg(background)),
            ];
            match ratio {
                Some(ratio) => line.extend(
                    widgets::tracked_bar(ratio, bar_width, pal.accent, pal.accent2, pal).spans,
                ),
                None => line.extend(widgets::bar_unknown(bar_width, pal).spans),
            }
            line.push(Span::styled(" ", Style::default().bg(background)));
            line.push(Span::styled(
                percent_label(ratio),
                Style::default().fg(pal.muted).bg(background),
            ));
            frame.render_widget(
                Paragraph::new(Line::from(line)).style(Style::default().bg(background)),
                Rect::new(area.x, area.y + 1, area.width, 1),
            );
        }
        if area.height >= 3 {
            // Failure text when there is one, otherwise the operation name —
            // still useful while the task is running or queued.
            let detail = task
                .transfer_label()
                .or_else(|| task.failure.clone())
                .unwrap_or_else(|| self.task_operation_label(task.operation_type));
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        if active { "▎  " } else { "   " },
                        Style::default().fg(pal.accent).bg(background),
                    ),
                    Span::styled(
                        truncate(&detail, usize::from(area.width).saturating_sub(4)),
                        Style::default().fg(pal.faint).bg(background),
                    ),
                ]))
                .style(Style::default().bg(background)),
                Rect::new(area.x, area.y + 2, area.width, 1),
            );
        }
    }
}

fn state_label(
    state: TaskState,
    i18n: &crate::i18n::I18n,
    pal: &Palette,
) -> (String, ratatui::style::Color) {
    let (key, color) = match state {
        TaskState::Queued => ("task.queued", pal.muted),
        TaskState::Running => ("task.running", pal.accent),
        TaskState::Paused => ("task.paused", pal.accent2),
        TaskState::Succeeded => ("task.completed", pal.ok),
        TaskState::Failed => ("task.failed", pal.err),
        TaskState::Cancelled => ("task.cancelled", pal.muted),
    };
    (i18n.text(key), color)
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

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::layout;

    #[test]
    fn the_tasks_page_reserves_a_chip_row_above_the_cards() {
        let [pills, body] = layout(Rect::new(0, 0, 93, 38));
        assert_eq!(pills.height, 1);
        assert_eq!(body.y, 3);
        assert_eq!(body.bottom(), 37);
    }
}
