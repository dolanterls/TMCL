//! `classic` instance list: name, version, loader and a status pill.
//!
//! Filtering, selection and the create/import actions live in
//! `app::state::instances`.
use super::*;
use crate::tui::hit::RowKind;

impl App {
    pub(super) fn draw_instances_page(&self, frame: &mut Frame<'_>, area: Rect) {
        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(area);
        let tabs = crate::tui::widgets::tabs::TabBar::new(self.instance_filter_labels());
        frame.render_widget(
            tabs.widget()
                .select(self.instance_filter)
                .highlight_style(self.theme.accent_style())
                .style(Style::default().fg(self.theme.label)),
            sections[0],
        );
        for (index, rect) in tabs.rects(sections[0]).iter().enumerate() {
            self.hitmap
                .add(*rect, Action::filter(Page::Instances, index));
        }

        let visible = self.visible_instance_indices();
        if visible.is_empty() {
            crate::tui::widgets::empty::render(
                frame,
                sections[1],
                &self.i18n.text("page.instances"),
                &self.i18n.text("instances.empty_page"),
                self.theme,
            );
            return;
        }
        let selected = visible
            .iter()
            .position(|index| *index == self.instances.focused)
            .unwrap_or(0);

        if self.is_narrow() {
            let items = visible
                .iter()
                .filter_map(|index| self.instances.items.get(*index))
                .map(|item| {
                    let current = if self.instances.current_id == Some(item.id) {
                        format!(" · {}", self.i18n.text("instances.current_short"))
                    } else {
                        String::new()
                    };
                    format!(
                        "{} · {} · {}{current}",
                        item.name,
                        item.version,
                        localize_instance_status(&item.status, &self.i18n)
                    )
                })
                .collect::<Vec<_>>();
            let layout = self.render_compact_list(
                frame,
                sections[1],
                &self.i18n.text("page.instances"),
                &items,
                selected,
                RowWindow::Instances,
            );
            self.hitmap.rows(layout, RowKind::Instances);
            return;
        }

        let split = self.instance_detail_areas(sections[1]);
        let table_area = split.map_or(sections[1], |(table, _)| table);
        let rows = visible
            .iter()
            .filter_map(|index| self.instances.items.get(*index))
            .map(|item| {
                let is_current = self.instances.current_id == Some(item.id);
                let status = if is_current {
                    crate::tui::widgets::status::badge_with_theme(
                        StatusKind::Current,
                        self.i18n.text("common.current"),
                        self.theme,
                    )
                } else {
                    crate::tui::widgets::status::badge_with_theme(
                        instance_status_kind(&item.status),
                        localize_instance_status(&item.status, &self.i18n),
                        self.theme,
                    )
                };
                let loader = self.instance_loader_text(item);
                vec![
                    item.name.clone(),
                    item.version.clone(),
                    loader,
                    format_play_time(item.play_time_seconds),
                    status.to_string(),
                ]
            })
            .collect::<Vec<_>>();
        let layout = self.row_windows.resolve(
            RowWindow::Instances,
            RowLayout::table(table_area, rows.len(), selected),
        );
        crate::tui::widgets::table::render_layout(
            layout,
            frame,
            table_area,
            self.i18n.text("page.instances"),
            vec![
                self.i18n.text("overview.name"),
                self.i18n.text("overview.version"),
                self.i18n.text("overview.loader"),
                self.i18n.text("overview.played"),
                self.i18n.text("overview.status"),
            ],
            rows,
            Some(selected),
            &[
                Constraint::Min(16),
                Constraint::Length(8),
                Constraint::Length(11),
                Constraint::Length(9),
                Constraint::Length(10),
            ],
            true,
            self.theme,
        );
        self.hitmap.rows(layout, RowKind::Instances);
        if let Some((_, pane)) = split {
            self.hitmap
                .scroll(pane, crate::tui::action::ScrollTarget::Pane);
            self.draw_detail_pane(frame, pane);
        }
    }
}
