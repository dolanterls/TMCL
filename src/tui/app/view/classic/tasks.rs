//! `classic` task list: a status/name/progress/age table.
//!
//! Row grouping and the actions that act on a row live in `app::state::tasks`;
//! this file only draws what they compute.
use super::*;
use crate::tui::hit::RowKind;

impl App {
    pub(super) fn draw_tasks_page(&self, frame: &mut Frame<'_>, area: Rect) {
        let sections = Self::task_page_sections(area);
        let tabs = self.task_tabs();
        let selected_filter = TaskFilter::ALL
            .iter()
            .position(|filter| *filter == self.task_filter)
            .unwrap_or(0);
        frame.render_widget(
            tabs.widget()
                .select(selected_filter)
                .highlight_style(self.theme.accent_style())
                .style(Style::default().fg(self.theme.label)),
            sections[0],
        );
        for (index, rect) in tabs.rects(sections[0]).iter().enumerate() {
            self.hitmap.add(*rect, Action::filter(Page::Tasks, index));
        }

        let rows = self.task_rows();
        if rows.is_empty() {
            crate::tui::widgets::empty::render(
                frame,
                sections[1],
                &self.i18n.text("page.tasks"),
                &self.i18n.text("task.no_filter_results"),
                self.theme,
            );
            return;
        }
        if self.is_narrow() {
            let items = rows
                .iter()
                .map(|row| {
                    let progress = format!(
                        "{} {}",
                        percent_label(task_ratio(row.state, row.completed, row.total)),
                        format_progress_bytes(row.completed, row.total)
                    );
                    let label = if row.steps > 1 {
                        format!("{} ({}/{})", row.label, row.step, row.steps)
                    } else {
                        row.label.clone()
                    };
                    let transfer = row
                        .transfer_label()
                        .map_or_else(String::new, |value| format!(" · {value}"));
                    format!(
                        "{} · {label} · {progress}{transfer}",
                        localize_task_state(row.state, &self.i18n)
                    )
                })
                .collect::<Vec<_>>();
            let layout = self.render_compact_list(
                frame,
                sections[1],
                &self.i18n.text("page.tasks"),
                &items,
                self.task_selected,
                RowWindow::Tasks,
            );
            self.hitmap.rows(layout, RowKind::Tasks);
            return;
        }
        let table_rows: Vec<Vec<String>> = rows
            .iter()
            .map(|row| {
                // Percent first, then the size: the column reads as a column
                // even though the numbers change width as a download advances.
                let progress = format!(
                    "{} {}",
                    percent_label(task_ratio(row.state, row.completed, row.total)),
                    format_progress_bytes(row.completed, row.total)
                );
                // A job shows which step is running, not only its name.
                let label = if row.steps > 1 {
                    format!("{}  ({}/{})", row.label, row.step, row.steps)
                } else {
                    row.label.clone()
                };
                let updated = self
                    .task_snapshots
                    .get(row.head)
                    .and_then(|snapshot| self.task_engine.updated_at(snapshot.id))
                    .map_or_else(
                        || "-".to_owned(),
                        |updated| format_age(updated, unix_seconds()),
                    );
                vec![
                    crate::tui::widgets::status::badge_with_theme(
                        task_status_kind(row.state),
                        localize_task_state(row.state, &self.i18n),
                        self.theme,
                    )
                    .to_string(),
                    label,
                    progress,
                    row.transfer_label().unwrap_or_else(|| String::from("-")),
                    updated,
                ]
            })
            .collect();
        let selected = self.task_selected.min(rows.len() - 1);
        let layout = self.row_windows.resolve(
            RowWindow::Tasks,
            RowLayout::table(sections[1], table_rows.len(), selected),
        );
        crate::tui::widgets::table::render_layout(
            layout,
            frame,
            sections[1],
            self.i18n.text("page.tasks"),
            vec![
                self.i18n.text("task.status"),
                self.i18n.text("task.task"),
                self.i18n.text("task.progress"),
                self.i18n.text("task.speed"),
                self.i18n.text("task.updated"),
            ],
            table_rows,
            Some(selected),
            &[
                Constraint::Length(14),
                Constraint::Min(1),
                Constraint::Length(21),
                Constraint::Length(18),
                Constraint::Length(10),
            ],
            true,
            self.theme,
        );
        self.hitmap.rows(layout, RowKind::Tasks);
    }
}
