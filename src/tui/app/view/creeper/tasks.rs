//! Tasks page: filter pills and one card per task with its progress.

use crate::tui::app::view::sheet::Sheet;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use super::overview::task_kind;
use super::*;
use crate::tui::app::state::data_format::{format_progress_bytes, percent_label, task_ratio};
use crate::tui::hit::RowKind;
use crate::tui::palette;
use crate::tui::widgets::kit as widgets;

/// Width of the progress bar. Pinned so the bars of a list line up.
const BAR_CELLS: usize = 16;
/// `100%` / ` 42%` / `   —`.
const PERCENT_CELLS: usize = 4;

impl App {
    pub(in crate::tui::app) fn draw_creeper_tasks(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        t: f64,
    ) {
        let pal = self.creeper_palette();
        let sections = Self::task_page_sections(area);
        self.task_pills(frame, sections[0]);

        let visible = self.visible_task_indices();
        if visible.is_empty() {
            widgets::empty_card(
                frame,
                sections[1],
                &self.i18n.text("page.tasks"),
                &self.i18n.text("task.no_filter_results"),
                pal.purple,
                t,
            );
            return;
        }
        self.task_cards(frame, sections[1], t);
    }

    fn task_pills(&self, frame: &mut Frame<'_>, area: Rect) {
        let pal = self.creeper_palette();
        let labels = self.creeper_task_filter_labels();
        let selected = TaskFilter::ALL
            .iter()
            .position(|filter| *filter == self.task_filter);
        let spans = widgets::pill_row(&labels, selected, pal.purple, pal.surface);
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(pal.surface)),
            area,
        );
        for (index, rect) in widgets::pill_rects(&labels, area).iter().enumerate() {
            self.hitmap.add(*rect, Action::filter(Page::Tasks, index));
        }
    }

    fn task_cards(&self, frame: &mut Frame<'_>, area: Rect, t: f64) {
        let pal = self.creeper_palette();
        // One row per job: the steps of a modpack install share a bar.
        let visible = self.task_rows();
        let selected = self.task_selected.min(visible.len().saturating_sub(1));
        let block = widgets::with_counter(
            widgets::card(
                &self.i18n.text("page.tasks"),
                pal.purple,
                true,
                t,
                pal.surface,
            ),
            selected + 1,
            visible.len(),
        );
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.height < 2 || inner.width < 16 {
            return;
        }
        let stride = self.creeper_card_height();
        let layout = self.row_windows.resolve(
            RowWindow::Tasks,
            RowLayout::cards(area, visible.len(), selected, stride),
        );
        self.hitmap.rows(layout, RowKind::Tasks);
        let start = layout.offset();
        for (visual, raw) in visible
            .iter()
            .skip(start)
            .take(layout.visible_rows())
            .enumerate()
        {
            let task = raw;
            let y = inner
                .y
                .saturating_add(1)
                .saturating_add(visual as u16 * stride);
            let height = stride.min(inner.bottom().saturating_sub(y));
            if height == 0 {
                break;
            }
            let active = start + visual == selected;
            let kind = task_kind(task.state);
            let accent = widgets::status_color(kind);
            let bg = if active {
                pal.fill(pal.purple, 0.18)
            } else if visual % 2 == 1 {
                pal.fill(pal.surface_alt, 0.5)
            } else {
                pal.surface
            };
            frame.render_widget(
                Block::default().style(Style::default().bg(bg)),
                Rect::new(inner.x, y, inner.width, height),
            );
            let state_label = match task.state {
                crate::core::task::TaskState::Queued => self.i18n.text("task.queued"),
                crate::core::task::TaskState::Running => self.i18n.text("task.running"),
                crate::core::task::TaskState::Paused => self.i18n.text("task.paused"),
                crate::core::task::TaskState::Succeeded => self.i18n.text("task.completed"),
                crate::core::task::TaskState::Failed => self.i18n.text("task.failed"),
                crate::core::task::TaskState::Cancelled => self.i18n.text("task.cancelled"),
            };
            // The state pill is a column, not a suffix: it sits on the card's
            // right edge, so four tasks can be compared by state without the eye
            // following four different name lengths. `steps` goes with it,
            // because `3/3` belongs to the same reading and behind the name the
            // count and the pill would land on two different columns.
            let mut trailing = Vec::with_capacity(2);
            if task.steps > 1 {
                trailing.push(Span::styled(
                    format!("{}/{}  ", task.step, task.steps),
                    Style::default().fg(pal.text_faint).bg(bg),
                ));
            }
            trailing.push(widgets::status_pill(kind, state_label, bg));
            let trailing_width =
                u16::try_from(Line::from(trailing.clone()).width()).unwrap_or(u16::MAX);
            let trailing_width = trailing_width.min(inner.width);
            let trailing_x = inner.right().saturating_sub(trailing_width);
            // The name owns what is left; measure the pill instead of reserving
            // a guessed 24 cells, or a long status in another language eats the
            // name and a short one leaves a gutter.
            let name_width = trailing_x.saturating_sub(inner.x);
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        if active { "▌ " } else { "  " },
                        Style::default()
                            .fg(palette::mix(pal.purple, pal.text, 0.15))
                            .bg(bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        palette::truncate(&task.label, usize::from(name_width).saturating_sub(2)),
                        Style::default()
                            .fg(pal.text)
                            .bg(bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]))
                .style(Style::default().bg(bg)),
                Rect::new(inner.x, y, name_width, 1),
            );
            frame.render_widget(
                Paragraph::new(Line::from(trailing)).style(Style::default().bg(bg)),
                Rect::new(trailing_x, y, trailing_width, 1),
            );
            if height >= 2 {
                let ratio = task_ratio(task.state, task.completed, task.total);
                let sizes = format_progress_bytes(task.completed, task.total);
                let percent = percent_label(ratio);
                // Fixed columns, the way the crash-report cards do it: the byte
                // count ends where the bar begins, and the bar keeps the same
                // width on every row instead of growing with the card.
                let bar_width = BAR_CELLS.min(usize::from(inner.width).saturating_sub(28));
                let sizes_width =
                    usize::from(inner.width).saturating_sub(3 + bar_width + 1 + PERCENT_CELLS + 1);
                let sizes_text = palette::truncate(&sizes, sizes_width);
                let sizes_pad = sizes_width.saturating_sub(palette::width(&sizes_text));
                let mut spans = vec![
                    Span::styled("   ", Style::default().bg(bg)),
                    Span::styled(
                        format!("{}{sizes_text}", " ".repeat(sizes_pad)),
                        Style::default().fg(pal.text_dim).bg(bg),
                    ),
                    Span::styled(" ", Style::default().bg(bg)),
                ];
                spans.extend(widgets::progress_spans(
                    ratio, bar_width, accent, accent, t, bg,
                ));
                spans.push(Span::styled(" ", Style::default().bg(bg)));
                spans.push(Span::styled(
                    percent,
                    Style::default().fg(pal.text_faint).bg(bg),
                ));
                if task.attempt > 0 {
                    spans.push(Span::styled(
                        format!("  #{}", task.attempt),
                        Style::default().fg(pal.text_faint).bg(bg),
                    ));
                }
                frame.render_widget(
                    Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)),
                    Rect::new(inner.x, y + 1, inner.width, 1),
                );
            }
            if height >= 3 {
                let detail = task
                    .transfer_label()
                    .or_else(|| task.failure.clone())
                    .unwrap_or_else(|| {
                        if task.retryable {
                            self.i18n.text("task.retryable")
                        } else {
                            String::new()
                        }
                    });
                if !detail.is_empty() {
                    frame.render_widget(
                        Paragraph::new(Line::from(vec![
                            Span::styled("   ", Style::default().bg(bg)),
                            Span::styled(
                                palette::truncate(
                                    &detail,
                                    usize::from(inner.width).saturating_sub(4),
                                ),
                                Style::default()
                                    .fg(palette::mix(accent, pal.text_dim, 0.4))
                                    .bg(bg),
                            ),
                        ]))
                        .style(Style::default().bg(bg)),
                        Rect::new(inner.x, y + 2, inner.width, 1),
                    );
                }
            }
        }
    }
}
