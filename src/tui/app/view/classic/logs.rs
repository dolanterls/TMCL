//! Logs page.

use super::*;

impl App {
    pub(in crate::tui::app) fn logs_hints(&self) -> Vec<crate::tui::chrome::Hint> {
        vec![
            self.hint("/", "action.search"),
            self.hint("f", "action.follow"),
            self.hint("g", "action.bottom"),
            self.hint("c", "action.clear_view"),
        ]
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
                    let mut bottom = vec![Span::styled(
                        follow,
                        Style::default().fg(if self.logs.follow() {
                            self.theme.success
                        } else {
                            self.theme.key
                        }),
                    )];
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
        let lines = self
            .logs
            .matching_lines(self.log_filter.as_deref().unwrap_or(""))
            .into_iter()
            .map(|line| {
                let style = if line.starts_with("[stderr]") {
                    Style::default().fg(self.theme.key)
                } else if line.starts_with("[stdout]") {
                    Style::default().fg(self.theme.label)
                } else {
                    self.theme.base()
                };
                highlight_line(
                    line,
                    self.log_filter.as_deref().unwrap_or(""),
                    style,
                    self.theme,
                )
            })
            .collect::<Vec<_>>();
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        let total_rows = paragraph.line_count(text_area.width);
        let max_offset = total_rows.saturating_sub(text_area.height as usize);
        let bottom_offset = self.logs.scroll_offset().min(max_offset);
        let scroll_from_top = max_offset.saturating_sub(bottom_offset);
        frame.render_widget(
            paragraph.scroll((u16::try_from(scroll_from_top).unwrap_or(u16::MAX), 0)),
            text_area,
        );

        if total_rows > 0 {
            let mut scrollbar_state = ScrollbarState::new(total_rows)
                .position(scroll_from_top)
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

    pub(in crate::tui::app) fn log_max_scroll_offset(&self) -> usize {
        let main = self
            .layout_chrome(Rect::new(0, 0, self.terminal_width, self.terminal_height))
            .main;
        // The nightlight page draws its own card: two rows of page margin, two
        // rows of card padding and a two-row header sit between `main` and the
        // first log line. Reusing the bordered-panel geometry clipped the last
        // rows; ask the painter for the rectangle it actually fills.
        // `paper` prints one log entry per row (the level letter plus a body
        // truncated to the body width), so its scroll maximum is a line count.
        // Ask the painter for its rectangle rather than re-deriving the head.
        if matches!(self.preset(), UiPreset::Paper | UiPreset::Shelf) {
            // Both print one entry per row, truncated rather than wrapped, so
            // the maximum is a line count — and both own their head geometry.
            let body = if self.preset() == UiPreset::Paper {
                self.paper_log_area(main)
            } else {
                self.shelf_log_area(main)
            };
            return self
                .logs
                .matching_lines(self.log_filter.as_deref().unwrap_or(""))
                .len()
                .saturating_sub(usize::from(body.height));
        }
        let (width, height) = if self.preset() == UiPreset::Nightlight {
            let body = App::nightlight_log_body(main);
            (body.width, body.height)
        } else {
            let inner = crate::tui::chrome::panel(
                &self.i18n.text(Page::Logs.title_key()),
                false,
                self.theme,
            )
            .inner(main);
            (inner.width.saturating_sub(1), inner.height)
        };
        let lines = self
            .logs
            .matching_lines(self.log_filter.as_deref().unwrap_or(""))
            .into_iter()
            .map(Line::from)
            .collect::<Vec<_>>();
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .line_count(width)
            .saturating_sub(usize::from(height))
    }

    pub(in crate::tui::app) fn toggle_log_follow(&mut self) {
        self.logs.toggle_follow();
        self.status = Status::info(if self.logs.follow() {
            self.i18n.text("status.log_follow_enabled")
        } else {
            self.i18n.text("status.log_follow_paused")
        });
    }

    pub(in crate::tui::app) fn follow_log_bottom(&mut self) {
        self.logs.set_follow(true);
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
            LogExportKind::CrashReport(ref path) => (
                path.parent()
                    .unwrap_or(&self.paths.game_logs_dir)
                    .to_path_buf(),
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("crash-report.txt")
                    .to_owned(),
            ),
        };
        self.pending_log_export = Some(kind);
        self.save_file_browse_request = Some((directory, name));
        self.status = Status::info(self.i18n.text("status.save_log_target"));
    }

    pub(in crate::tui::app) fn begin_crash_report_export(&mut self) {
        let Some(path) = self.last_crash_report.clone() else {
            self.status = Status::warning(self.i18n.text("status.no_crash_report"));
            return;
        };
        if !path.is_file() {
            self.status = Status::warning(self.i18n.text("status.no_crash_report"));
            return;
        }
        self.begin_log_export(LogExportKind::CrashReport(path));
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
