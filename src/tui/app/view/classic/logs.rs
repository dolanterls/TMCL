//! Logs page.

use super::*;

impl App {
    pub(in crate::tui::app) fn logs_hints(&self) -> Vec<crate::tui::chrome::Hint> {
        let mut hints = vec![
            self.hint("/", "action.search"),
            self.hint("f", "action.follow"),
            self.hint("g", "action.bottom"),
            self.hint("c", "action.clear_view"),
        ];
        if self.diagnosis.is_some() {
            hints.push(self.hint("F3", "action.diagnose"));
        }
        hints
    }

    pub(super) fn draw_logs_page(&self, frame: &mut Frame<'_>, area: Rect) {
        self.hitmap
            .scroll(area, crate::tui::action::ScrollTarget::LogLines);
        if area.is_empty() {
            return;
        }
        let current = self
            .instances
            .current_id
            .and_then(|id| self.instances.items.iter().find(|item| item.id == id))
            .map_or_else(|| self.i18n.text("common.none"), |item| item.name.clone());
        let follow = if self.logs.follow() {
            format!("● {}", self.i18n.text("logs.live"))
        } else {
            format!("○ {}", self.i18n.text("logs.paused"))
        };
        // The filter field uses the shared renderer, so its block blinks and
        // tracks the insertion point like every other input.
        let search_spans: Vec<Span<'static>> =
            self.log_filter.as_deref().map_or_else(Vec::new, |filter| {
                let focused = matches!(
                    self.search.as_ref(),
                    Some(state) if state.target == SearchTarget::Logs
                );
                let cursor = if focused {
                    self.search
                        .as_ref()
                        .map_or(filter.chars().count(), |state| state.cursor)
                } else {
                    filter.chars().count()
                };
                let label = format!("  {}: ", self.i18n.text("common.search"));
                let used = ratatui::text::Line::from(vec![Span::raw(follow.clone())]).width();
                let budget = usize::from(area.width)
                    .saturating_sub(used + ratatui::text::Line::from(label.as_str()).width() + 2);
                let style = crate::tui::text::FieldStyle {
                    text: self.theme.fg,
                    dim: self.theme.dim,
                    bg: self.theme.bg,
                    cursor_fg: ratatui::style::Color::Black,
                    cursor_bg: self.theme.accent,
                };
                let mut spans = vec![Span::styled(label, self.theme.dim())];
                spans.extend(
                    crate::tui::text::render(
                        filter,
                        cursor,
                        u16::try_from(budget).unwrap_or(u16::MAX),
                        focused,
                        crate::tui::text::blink_on(self.anim_time()),
                        style,
                        "",
                    )
                    .spans,
                );
                spans
            });
        let block =
            crate::tui::chrome::panel(&self.i18n.text(Page::Logs.title_key()), false, self.theme)
                .title_top(
                    Line::from(Span::styled(
                        format!("{}: {current}", self.i18n.text("logs.instance")),
                        self.theme.dim(),
                    ))
                    .alignment(Alignment::Left),
                )
                .title_bottom({
                    let mut bottom = Vec::new();
                    if let Some(badge) = self.diagnosis_badge() {
                        bottom.push(Span::styled(
                            format!("{badge}  "),
                            Style::default().fg(self.theme.error),
                        ));
                    }
                    bottom.push(Span::styled(
                        follow,
                        Style::default().fg(if self.logs.follow() {
                            self.theme.success
                        } else {
                            self.theme.key
                        }),
                    ));
                    bottom.extend(search_spans.clone());
                    Line::from(bottom).alignment(Alignment::Left)
                });
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.is_empty() {
            return;
        }

        let text_area = Rect {
            width: inner.width.saturating_sub(1),
            ..inner
        };
        // One page of lines, already styled: the buffer measures wrapping once
        // per line, so the page does not walk 4000 lines to draw 36.
        let query = self.log_filter.as_deref().unwrap_or("");
        let window = self
            .logs
            .window(query, text_area.width, text_area.height, false);
        let lines = window
            .lines
            .iter()
            .map(|line| {
                let style = if line.starts_with("[stderr]") {
                    Style::default().fg(self.theme.key)
                } else if line.starts_with("[stdout]") {
                    Style::default().fg(self.theme.label)
                } else {
                    self.theme.base()
                };
                highlight_line(line.clone(), query, style, self.theme)
            })
            .collect::<Vec<_>>();
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .scroll((u16::try_from(window.skip_rows).unwrap_or(u16::MAX), 0)),
            text_area,
        );

        if window.total_rows > 0 {
            let mut scrollbar_state = ScrollbarState::new(window.total_rows)
                .position(window.from_row)
                .viewport_content_length(text_area.height as usize);
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(Some("↑"))
                    .end_symbol(Some("↓")),
                inner,
                &mut scrollbar_state,
            );
        }
    }

    /// Rows between the top of the buffer and the last page, for the scroll
    /// keys. Measured from the same widths the painters use, so the last row
    /// is reachable in every preset.
    pub(in crate::tui::app) fn log_max_scroll_offset(&self) -> usize {
        let main = self
            .layout_chrome(Rect::new(0, 0, self.terminal_width, self.terminal_height))
            .main;
        // Every preset owns its head geometry: `nightlight` draws its own card
        // (page margin, title padding, header), `creeper` drops a header row and
        // a gutter, the sheets carry their own head. Ask each painter for the
        // numbers it fills; the bordered-panel geometry clips the last rows.
        let (width, height, clip) = match self.preset() {
            UiPreset::Paper => {
                let body = self.paper_log_area(main);
                (body.width, body.height, false)
            }
            UiPreset::Shelf => {
                let body = self.shelf_log_area(main);
                (body.width.saturating_sub(1), body.height, false)
            }
            UiPreset::Nightlight => {
                let body = App::nightlight_log_body(main);
                (body.width, body.height, false)
            }
            UiPreset::Creeper => {
                // The stream card drops a header row and a gutter on each side
                // before the first line.
                let inner = main.inner(ratatui::layout::Margin {
                    horizontal: 1,
                    vertical: 1,
                });
                (
                    inner.width.saturating_sub(2),
                    inner.height.saturating_sub(1),
                    false,
                )
            }
            UiPreset::Classic => {
                let inner = crate::tui::chrome::panel(
                    &self.i18n.text(Page::Logs.title_key()),
                    false,
                    self.theme,
                )
                .inner(main);
                (inner.width.saturating_sub(1), inner.height, false)
            }
        };
        self.logs
            .total_rows(self.log_filter.as_deref().unwrap_or(""), width, clip)
            .saturating_sub(usize::from(height))
    }

    pub(in crate::tui::app) fn toggle_log_follow(&mut self) {
        let max = self.log_max_scroll_offset();
        self.logs.toggle_follow(max);
        self.status = Status::info(if self.logs.follow() {
            self.i18n.text("status.log_follow_enabled")
        } else {
            self.i18n.text("status.log_follow_paused")
        });
    }

    pub(in crate::tui::app) fn follow_log_bottom(&mut self) {
        let max = self.log_max_scroll_offset();
        self.logs.set_follow(true, max);
        self.status = Status::info(self.i18n.text("status.log_bottom"));
    }

    pub(in crate::tui::app) fn open_log_directory(&mut self) {
        match crate::platform::windows::open_path(&self.paths.launcher_logs_dir) {
            Ok(()) => self.status = Status::success(self.i18n.text("status.log_directory_opened")),
            Err(error) => self.status = Status::error(error.to_string()),
        }
    }

    pub(in crate::tui::app) fn begin_log_export(&mut self, kind: LogExportKind) {
        let (directory, name) = match kind {
            LogExportKind::Launcher => (
                self.paths.launcher_logs_dir.clone(),
                "tmcl-launcher.log".to_owned(),
            ),
            LogExportKind::Game => (self.paths.game_logs_dir.clone(), "tmcl-game.log".to_owned()),
            // The bundle names itself and writes several files; it has its own
            // entry point so this one keeps meaning "copy one buffer out".
            LogExportKind::Bundle => {
                self.begin_diagnosis_export();
                return;
            }
        };
        self.pending_log_export = Some(kind);
        self.save_file_browse_request = Some((directory, name));
        self.status = Status::info(self.i18n.text("status.save_log_target"));
    }
}

fn highlight_line(line: String, query: &str, base: Style, theme: Theme) -> Line<'static> {
    if query.is_empty() {
        return Line::styled(line, base);
    }
    let mut spans = Vec::new();
    let mut rest = line.as_str();
    while let Some(position) = rest.find(query) {
        if position > 0 {
            spans.push(Span::styled(rest[..position].to_owned(), base));
        }
        spans.push(Span::styled(query.to_owned(), theme.accent_style()));
        rest = &rest[position + query.len()..];
    }
    if !rest.is_empty() {
        spans.push(Span::styled(rest.to_owned(), base));
    }
    Line::from(spans)
}
