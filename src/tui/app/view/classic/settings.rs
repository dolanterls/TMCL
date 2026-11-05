//! `classic` settings pages: the table, the search view, Java and game
//! directories.
//!
//! Every action behind these rows lives in `app::state::settings`.
use super::*;
use crate::tui::hit::RowKind;

impl App {
    /// Classic shell entry point: category tabs above the settings surface.
    pub(super) fn draw_settings_page(&self, frame: &mut Frame<'_>, area: Rect) {
        let categories = SettingsCategory::ALL
            .iter()
            .map(|category| self.i18n.text(category.title_key()))
            .collect::<Vec<_>>();
        if self.is_narrow() {
            self.draw_settings_surface(frame, area);
            return;
        }
        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(area);
        let tabs = crate::tui::widgets::tabs::TabBar::new(categories);
        frame.render_widget(
            tabs.widget()
                .select(self.settings.category.index())
                .highlight_style(self.theme.accent_style())
                .style(Style::default().fg(self.theme.label)),
            sections[0],
        );
        for (index, rect) in tabs.rects(sections[0]).iter().enumerate() {
            self.hitmap.add(*rect, Action::category(index));
        }
        self.draw_settings_surface(frame, sections[1]);
    }

    fn draw_settings_surface(&self, frame: &mut Frame<'_>, area: Rect) {
        // `/` swaps the page for the search view, the way the other two presets
        // do it: the field has to be on screen, or the key reads as a no-op.
        if self.settings_search_active() {
            self.draw_settings_search(frame, area);
            return;
        }
        match self.settings.category {
            SettingsCategory::Java => self.draw_java_page(frame, area),
            SettingsCategory::GameDirectories => self.draw_game_directories_page(frame, area),
            _ => {
                let rows = self.settings_rows();
                let title = self.i18n.text(self.settings.category.title_key());
                let layout = crate::tui::widgets::settings::render(
                    frame,
                    area,
                    &title,
                    [
                        self.i18n.text("table.setting").as_str(),
                        self.i18n.text("table.value").as_str(),
                    ],
                    &rows,
                    self.settings.selected,
                    true,
                    self.theme,
                );
                self.hitmap.rows(layout, RowKind::Settings);
                // The `‹ ›` ends of an adjustable value are real controls, as
                // they are in the other two presets. Registered after the row
                // boxes so the two glyph cells win over "select this row".
                let value =
                    crate::tui::widgets::settings::value_column(area, &title, true, self.theme);
                for (index, rect) in layout.rows() {
                    let Some(row) = rows.get(index) else {
                        continue;
                    };
                    let Some((open, close)) =
                        crate::tui::widgets::settings::step_targets(value, row, rect.y)
                    else {
                        continue;
                    };
                    self.hitmap.add(open, Action::step(index, -1));
                    self.hitmap.add(close, Action::step(index, 1));
                }
            }
        }
    }

    /// The settings search view: one field, then the flat list of matches
    /// across every category. `Enter` jumps to the highlighted row.
    fn draw_settings_search(&self, frame: &mut Frame<'_>, area: Rect) {
        use crate::tui::action::Action;
        use crate::tui::input::SearchTarget;

        let hits = self.settings_search_results();
        let selected = self.settings.selected.min(hits.len().saturating_sub(1));
        let title = self.i18n.text("settings.search_title");
        let mut block = crate::tui::chrome::panel(&title, true, self.theme);
        if !hits.is_empty() {
            block = block.title_bottom(
                Line::from(vec![
                    ratatui::text::Span::styled("|", self.theme.dim()),
                    ratatui::text::Span::styled(
                        format!("{}/{}", selected.saturating_add(1), hits.len()),
                        self.theme.accent_style(),
                    ),
                    ratatui::text::Span::styled("|", self.theme.dim()),
                ])
                .right_aligned(),
            );
        }
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.is_empty() {
            return;
        }

        let query = self.settings_search.as_deref().unwrap_or_default();
        let focused = matches!(
            self.search.as_ref(),
            Some(state) if state.target == SearchTarget::Settings
        );
        let cursor = self
            .search
            .as_ref()
            .filter(|state| state.target == SearchTarget::Settings)
            .map_or_else(|| query.chars().count(), |state| state.cursor);
        let count_label = self.i18n.format(
            "settings.search_results",
            &[("count", &hits.len().to_string())],
        );
        let right_width = u16::try_from(crate::tui::palette::width(&count_label))
            .unwrap_or(u16::MAX)
            .min(inner.width / 2);
        let field_width = inner.width.saturating_sub(right_width.saturating_add(1));
        if field_width > 4 {
            let field = Rect::new(inner.x, inner.y, field_width, 1);
            self.hitmap
                .add(field, Action::search(SearchTarget::Settings));
            let style = crate::tui::text::FieldStyle {
                text: self.theme.fg,
                dim: self.theme.dim,
                bg: self.theme.bg,
                cursor_fg: ratatui::style::Color::Black,
                cursor_bg: self.theme.accent,
            };
            let mut spans = vec![ratatui::text::Span::styled(
                " ⌕ ",
                if focused {
                    self.theme.accent_style()
                } else {
                    self.theme.dim()
                },
            )];
            spans.extend(
                crate::tui::text::render(
                    query,
                    cursor,
                    field_width.saturating_sub(3),
                    focused,
                    crate::tui::text::blink_on(self.anim_time()),
                    style,
                    &self.i18n.text("settings.search_placeholder"),
                )
                .spans,
            );
            frame.render_widget(
                Paragraph::new(Line::from(spans)).style(self.theme.base()),
                field,
            );
        }
        if right_width > 0 {
            frame.render_widget(
                Paragraph::new(Line::from(ratatui::text::Span::styled(
                    count_label,
                    self.theme.dim(),
                )))
                .alignment(ratatui::layout::Alignment::Right)
                .style(self.theme.base()),
                Rect::new(
                    inner.right().saturating_sub(right_width),
                    inner.y,
                    right_width,
                    1,
                ),
            );
        }

        let list = Rect::new(
            inner.x,
            inner.y.saturating_add(2),
            inner.width,
            inner.height.saturating_sub(2),
        );
        if list.is_empty() {
            return;
        }
        self.hitmap.scroll(
            list,
            crate::tui::action::ScrollTarget::Rows(RowKind::Settings),
        );
        if hits.is_empty() {
            frame.render_widget(
                Paragraph::new(Line::from(ratatui::text::Span::styled(
                    self.i18n.text("settings.empty"),
                    self.theme.dim(),
                )))
                .style(self.theme.base()),
                list,
            );
            return;
        }
        let visible = usize::from(list.height).max(1);
        let offset = selected.saturating_sub(visible.saturating_sub(1));
        for (visual, hit) in hits.iter().skip(offset).take(visible).enumerate() {
            let index = offset + visual;
            let y = list.y.saturating_add(visual as u16);
            let active = index == selected;
            let row = Rect::new(list.x, y, list.width, 1);
            let label = format!(
                "{} · {}",
                self.i18n.text(hit.category.title_key()),
                hit.row.title
            );
            let value_width = crate::tui::palette::width(&hit.row.value)
                .saturating_add(2)
                .min(usize::from(list.width) / 2);
            let label_width = usize::from(list.width).saturating_sub(value_width + 1);
            let style = if active {
                self.theme.selection_style()
            } else {
                self.theme.base()
            };
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    ratatui::text::Span::styled(
                        crate::tui::palette::fit(&label, label_width),
                        style,
                    ),
                    ratatui::text::Span::styled(format!(" {:<value_width$}", hit.row.value), style),
                ]))
                .style(style),
                row,
            );
            self.hitmap.add(row, Action::row(Page::Settings, index));
        }
    }

    pub(super) fn draw_java_page(&self, frame: &mut Frame<'_>, area: Rect) {
        if self.java.is_empty() {
            crate::tui::widgets::empty::render(
                frame,
                area,
                &self.i18n.text("java.runtime"),
                &self.i18n.text("java.empty"),
                self.theme,
            );
            return;
        }
        if self.is_narrow() {
            let items = self
                .java
                .iter()
                .map(|java| {
                    let selected =
                        if self.config.java.selected_path.as_ref() == Some(&java.executable) {
                            format!(" · {}", self.i18n.text("java.selected_short"))
                        } else {
                            String::new()
                        };
                    format!(
                        "Java {} · {} · {}{selected}",
                        java.major,
                        java.version,
                        java_supported_minecraft(java.major)
                    )
                })
                .collect::<Vec<_>>();
            let layout = self.render_compact_list(
                frame,
                area,
                &self.i18n.text("java.runtime"),
                &items,
                self.java_selected,
                RowWindow::Java,
            );
            self.hitmap.rows(layout, RowKind::Java);
            return;
        }
        let rows: Vec<Vec<String>> = self
            .java
            .iter()
            .map(|java| {
                let selected = self.config.java.selected_path.as_ref() == Some(&java.executable);
                vec![
                    crate::tui::widgets::status::badge_with_theme(
                        if selected {
                            StatusKind::Current
                        } else if java.is_jdk {
                            StatusKind::Ready
                        } else {
                            StatusKind::Warning
                        },
                        if selected {
                            self.i18n.text("java.selected")
                        } else {
                            self.i18n.text("java.available")
                        },
                        self.theme,
                    )
                    .to_string(),
                    format!("Java {}", java.major),
                    java.version.clone(),
                    java.vendor
                        .clone()
                        .unwrap_or_else(|| self.i18n.text("java.unknown_vendor")),
                    java_supported_minecraft(java.major),
                    display_path(&java.executable),
                ]
            })
            .collect();
        let selected = self.java_selected.min(self.java.len() - 1);
        let layout = self.row_windows.resolve(
            RowWindow::Java,
            RowLayout::table(area, rows.len(), selected),
        );
        crate::tui::widgets::table::render_layout(
            layout,
            frame,
            area,
            self.i18n.text("java.runtime"),
            vec![
                self.i18n.text("overview.status"),
                self.i18n.text("java.major"),
                self.i18n.text("java.version"),
                self.i18n.text("java.vendor"),
                self.i18n.text("java.minecraft_versions"),
                self.i18n.text("java.path"),
            ],
            rows,
            Some(selected),
            &[
                Constraint::Length(10),
                Constraint::Length(8),
                Constraint::Length(12),
                Constraint::Length(10),
                Constraint::Length(12),
                Constraint::Min(1),
            ],
            true,
            self.theme,
        );
        self.hitmap.rows(layout, RowKind::Java);
    }

    pub(super) fn draw_game_directories_page(&self, frame: &mut Frame<'_>, area: Rect) {
        if self.is_narrow() {
            let mut items = vec![
                self.i18n.text("settings.add_game_directory"),
                self.i18n.text("settings.import_game_directory"),
            ];
            items.extend(self.config.game_directories.iter().map(|path| {
                let count = self.game_directory_instance_count(path);
                format!(
                    "{} · {}",
                    display_path(path),
                    self.i18n.format(
                        "settings.directory_instances",
                        &[("count", &count.to_string())]
                    )
                )
            }));
            let layout = self.render_compact_list(
                frame,
                area,
                &self.i18n.text("settings.category.game_directories"),
                &items,
                self.settings.selected,
                RowWindow::Directories,
            );
            self.hitmap.rows(layout, RowKind::Directories);
            return;
        }
        let mut rows = vec![
            vec![
                format!("＋ {}", self.i18n.text("settings.add_game_directory")),
                String::new(),
                String::new(),
            ],
            vec![
                format!("⇣ {}", self.i18n.text("settings.import_game_directory")),
                String::new(),
                String::new(),
            ],
        ];
        for path in &self.config.game_directories {
            let count = self.game_directory_instance_count(path);
            let current = self.config.game_directories.first().is_some_and(|first| {
                normalize_instance_path(first) == normalize_instance_path(path)
            });
            rows.push(vec![
                display_path(path),
                self.i18n.format(
                    "settings.directory_instances",
                    &[("count", &count.to_string())],
                ),
                if current {
                    self.i18n.text("settings.current_directory")
                } else {
                    String::new()
                },
            ]);
        }
        let selected = self
            .settings
            .selected
            .min(self.settings_row_count().saturating_sub(1));
        let layout = self.row_windows.resolve(
            RowWindow::Directories,
            RowLayout::table(area, rows.len(), selected),
        );
        crate::tui::widgets::table::render_layout(
            layout,
            frame,
            area,
            self.i18n.text("settings.category.game_directories"),
            vec![
                self.i18n.text("settings.directory"),
                self.i18n.text("settings.instances"),
                self.i18n.text("settings.state"),
            ],
            rows,
            Some(selected),
            &[
                Constraint::Min(20),
                Constraint::Length(12),
                Constraint::Length(10),
            ],
            true,
            self.theme,
        );
        self.hitmap.rows(layout, RowKind::Directories);
    }
}
