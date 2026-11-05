//! Overview page: wordmark, current setup and the instance list.

use super::*;
use crate::tui::hit::RowKind;

impl App {
    pub(in crate::tui::app) fn overview_hints(&self) -> Vec<crate::tui::chrome::Hint> {
        vec![
            self.hint("Enter", "action.launch"),
            self.hint("n", "action.new"),
            self.hint("i", "action.import"),
        ]
    }

    pub(super) fn draw_overview_page(&self, frame: &mut Frame<'_>, area: Rect) {
        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(6),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(10),
                Constraint::Length(1),
                Constraint::Min(6),
            ])
            .split(area);

        crate::tui::widgets::logo::render(frame, sections[0], self.theme);

        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Minecraft ".to_owned(), self.theme.accent_style()),
                Span::styled(
                    self.i18n.text("overview.tagline"),
                    Style::default().add_modifier(Modifier::ITALIC),
                ),
            ]))
            .alignment(Alignment::Center),
            sections[1],
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled("─".repeat(33), self.theme.dim())))
                .alignment(Alignment::Center),
            sections[2],
        );

        self.draw_setup_panel(frame, sections[4]);
        self.draw_overview_instances(frame, sections[6]);
    }

    fn draw_setup_panel(&self, frame: &mut Frame<'_>, area: Rect) {
        let current = self
            .instances
            .current_id
            .and_then(|id| self.instances.items.iter().find(|item| item.id == id));
        let instance =
            current.map_or_else(|| self.i18n.text("common.none"), |item| item.name.clone());
        let version = current.map_or_else(
            || self.i18n.text("common.none"),
            |item| item.version.clone(),
        );
        let loader = current.map_or_else(
            || self.i18n.text("common.none"),
            |item| {
                item.loader_version.as_ref().map_or_else(
                    || item.loader.clone(),
                    |loader_version| format!("{} {loader_version}", item.loader),
                )
            },
        );
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
                || self.i18n.text("common.none"),
                |java| format!("Java {}", java.major),
            );
        let directory = self
            .config
            .game_directories
            .first()
            .map_or_else(|| self.i18n.text("common.none"), |path| display_path(path));

        let fields = [
            (self.i18n.text("shell.instance"), instance),
            (self.i18n.text("overview.version"), version),
            (self.i18n.text("overview.loader"), loader),
            (self.i18n.text("shell.account"), account),
            (self.i18n.text("shell.java"), java),
            (self.i18n.text("shell.game"), directory),
        ];
        // Pad labels by display width instead of character count: a CJK label
        // is twice as wide as its character count, and `{label:<10}` would leave
        // the colon of every such row in a different column.
        let label_width = fields
            .iter()
            .map(|(label, _)| Line::from(label.as_str()).width())
            .max()
            .unwrap_or(0)
            .max(10);
        let lines = fields
            .iter()
            .map(|(label, value)| {
                let padding = " ".repeat(label_width - Line::from(label.as_str()).width());
                Line::from(vec![
                    Span::styled(format!("  {label}"), self.theme.label_style()),
                    Span::raw(padding),
                    Span::styled(": ", self.theme.dim()),
                    Span::styled(truncate_text(value, 36), self.theme.accent_style()),
                ])
            })
            .collect::<Vec<_>>();

        let width = 58.min(area.width);
        let height = 8.min(area.height);
        let rect = Rect::new(
            area.x + area.width.saturating_sub(width) / 2,
            area.y,
            width,
            height,
        );
        frame.render_widget(
            Paragraph::new(lines).block(crate::tui::chrome::panel(
                &self.i18n.text("overview.setup"),
                false,
                self.theme,
            )),
            rect,
        );
    }

    fn draw_overview_instances(&self, frame: &mut Frame<'_>, area: Rect) {
        if self.instances.items.is_empty() {
            frame.render_widget(
                Paragraph::new(Span::styled(
                    self.i18n.text("overview.no_instances"),
                    self.theme.dim(),
                ))
                .block(crate::tui::chrome::panel(
                    &self.i18n.text("page.instances"),
                    false,
                    self.theme,
                )),
                area,
            );
            return;
        }

        let rows = self
            .instances
            .items
            .iter()
            .map(|item| {
                vec![
                    item.name.clone(),
                    item.version.clone(),
                    item.loader.clone(),
                    format_play_time(item.play_time_seconds),
                    localize_instance_status(&item.status, &self.i18n),
                ]
            })
            .collect::<Vec<_>>();
        let selected = (!self.instances.items.is_empty())
            .then(|| self.instances.focused.min(self.instances.items.len() - 1));
        let layout = self.row_windows.resolve(
            RowWindow::OverviewInstances,
            RowLayout::table(area, rows.len(), selected.unwrap_or(0)),
        );
        crate::tui::widgets::table::render_layout(
            layout,
            frame,
            area,
            self.i18n.text("page.instances"),
            vec![
                self.i18n.text("overview.name"),
                self.i18n.text("overview.version"),
                self.i18n.text("common.loaders"),
                self.i18n.text("overview.played"),
                self.i18n.text("overview.status"),
            ],
            rows,
            selected,
            &[
                Constraint::Min(18),
                Constraint::Length(9),
                Constraint::Length(12),
                Constraint::Length(12),
                Constraint::Length(10),
            ],
            false,
            self.theme,
        );
        self.hitmap.rows(layout, RowKind::OverviewInstances);
    }
}
