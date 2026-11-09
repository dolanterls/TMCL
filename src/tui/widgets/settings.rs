use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Cell, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table, TableState},
};

use crate::tui::theme::Theme;
use crate::tui::widgets::rows::RowLayout;

/// How a setting behaves when it is activated or nudged left/right.
///
/// This is the single source of truth for the row hint, the key handling and
/// the value styling, so a row cannot advertise one editor and open another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingKind {
    Toggle,
    Choice,
    Number { step: i64 },
    Text,
    Action,
}

impl SettingKind {
    pub fn hint_key(self) -> &'static str {
        match self {
            Self::Toggle => "hint.setting_toggle",
            Self::Choice => "hint.setting_choose",
            Self::Number { .. } => "hint.setting_step",
            Self::Text => "hint.setting_edit",
            Self::Action => "hint.setting_run",
        }
    }

    /// Whether `←/→` changes the value in place.
    pub fn is_adjustable(self) -> bool {
        matches!(self, Self::Toggle | Self::Choice | Self::Number { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingRow {
    pub id: crate::tui::pages::settings::SettingId,
    pub title: String,
    pub description: String,
    pub value: String,
    pub kind: SettingKind,
    pub hint: String,
    pub enabled: bool,
    pub disabled_reason: String,
    /// Only set for toggles, so the renderer can pick the ●/○ glyph without
    /// reparsing the localized "on/off" label.
    pub value_on: Option<bool>,
}

impl SettingRow {
    pub fn new(
        id: crate::tui::pages::settings::SettingId,
        title: impl Into<String>,
        description: impl Into<String>,
        value: impl Into<String>,
        kind: SettingKind,
        hint: impl Into<String>,
    ) -> Self {
        Self {
            id,
            title: title.into(),
            description: description.into(),
            value: value.into(),
            kind,
            hint: hint.into(),
            enabled: true,
            disabled_reason: String::new(),
            value_on: None,
        }
    }

    pub fn toggle_state(mut self, on: bool) -> Self {
        self.value_on = Some(on);
        self
    }

    pub fn disabled(mut self, reason: impl Into<String>) -> Self {
        self.enabled = false;
        self.disabled_reason = reason.into();
        self
    }

    pub fn cells(&self) -> Vec<String> {
        vec![self.title.clone(), self.value.clone()]
    }
}

/// The value column of the table drawn by [`render`], in screen coordinates.
///
/// The `‹ ›` step targets have to land on the glyphs that advertise them, so
/// the column is derived from the same block and the same constraints the
/// table uses instead of a second guess at the percentages.
pub fn value_column(area: Rect, title: &str, focused: bool, theme: Theme) -> Rect {
    let inner = crate::tui::chrome::panel(title, focused, theme).inner(area);
    let columns =
        ratatui::layout::Layout::horizontal([Constraint::Percentage(55), Constraint::Min(12)])
            .spacing(1)
            .split(Rect::new(inner.x, inner.y, inner.width, 1));
    columns[1]
}

/// The value cell as the table draws it: rows that answer to `←/→` get `‹ ›`
/// around the value, everything else is plain text.
///
/// Drawing and hit-testing both go through this, so the glyphs on screen and
/// the cells that accept a click cannot drift apart.
pub fn step_cell(row: &SettingRow) -> Option<String> {
    if !row.enabled || !matches!(row.kind, SettingKind::Choice | SettingKind::Number { .. }) {
        return None;
    }
    Some(format!("‹ {} ›", row.value))
}

/// The two glyph cells of one steppable row, in screen coordinates.
///
/// The table left-aligns the value cell, so the closing `›` sits right after the
/// text. A target at the far edge of the column would sit dozens of cells from
/// the glyph it advertises.
pub fn step_targets(value: Rect, row: &SettingRow, y: u16) -> Option<(Rect, Rect)> {
    let text = step_cell(row)?;
    if value.width < 3 {
        return None;
    }
    let width = u16::try_from(crate::tui::palette::width(&text)).unwrap_or(u16::MAX);
    let close = value
        .x
        .saturating_add(width)
        .saturating_sub(1)
        .min(value.right().saturating_sub(1));
    let open = Rect::new(value.x, y, 2.min(value.width), 1);
    let close = Rect::new(
        close.saturating_sub(1),
        y,
        2.min(value.right().saturating_sub(close) + 1),
        1,
    );
    Some((open, close))
}

pub fn layout(area: Rect, row_count: usize, selected: usize) -> RowLayout {
    RowLayout::table(area, row_count, selected)
}

#[allow(clippy::too_many_arguments)]
pub fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    headers: [&str; 2],
    rows: &[SettingRow],
    selected: usize,
    focused: bool,
    theme: Theme,
) -> RowLayout {
    let offset = layout(area, rows.len(), selected).offset();
    let header = Row::new(headers.into_iter().map(Cell::from))
        .style(Style::default().add_modifier(Modifier::BOLD));
    let body = rows.iter().map(|row| {
        let mut cells = row.cells();
        // `‹ ›` advertise the step controls, exactly like the nightlight rows:
        // a value that answers to `←/→` should say so, or the column reads as
        // plain text.
        if let Some(cell) = step_cell(row)
            && let Some(value) = cells.get_mut(1)
        {
            *value = cell;
        }
        Row::new(cells.into_iter().map(Cell::from))
    });
    let mut state = TableState::default().with_offset(offset);
    state.select((!rows.is_empty()).then_some(selected.min(rows.len() - 1)));
    frame.render_stateful_widget(
        Table::new(body, [Constraint::Percentage(55), Constraint::Min(12)])
            .header(header)
            .block({
                let mut block = crate::tui::chrome::panel(title, focused, theme);
                if !rows.is_empty() {
                    block = block.title_bottom(
                        Line::from(vec![
                            ratatui::text::Span::styled("|", theme.dim()),
                            ratatui::text::Span::styled(
                                format!("{}/{}", selected.saturating_add(1), rows.len()),
                                theme.accent_style(),
                            ),
                            ratatui::text::Span::styled("|", theme.dim()),
                        ])
                        .right_aligned(),
                    );
                }
                block
            })
            .style(theme.base())
            .row_highlight_style(theme.selection_style()),
        area,
        &mut state,
    );
    let visible = usize::from(area.height.saturating_sub(3)).max(1);
    if rows.len() > visible {
        let mut scrollbar = ScrollbarState::new(rows.len())
            .position(selected)
            .viewport_content_length(visible);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓")),
            area,
            &mut scrollbar,
        );
    }
    layout(area, rows.len(), selected)
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};

    use super::{RowLayout, SettingKind, SettingRow, layout, render};
    use crate::tui::pages::settings::SettingId;
    use crate::tui::theme::Theme;

    fn row(title: &str, value: &str) -> SettingRow {
        SettingRow::new(
            SettingId::AutoCheckUpdates,
            title,
            "",
            value,
            SettingKind::Toggle,
            "Space",
        )
    }

    #[test]
    fn settings_rows_report_the_layout_they_are_drawn_with() {
        let area = Rect::new(2, 4, 80, 8);
        let rows = vec![row("One", "on"), row("Two", "off")];
        let expected = layout(area, rows.len(), 0);
        assert_eq!(expected.index_at(6), Some(0));
        assert_eq!(expected.index_at(7), Some(1));
        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                let _ = render(
                    frame,
                    area,
                    "Settings",
                    ["Setting", "Value"],
                    &rows,
                    0,
                    true,
                    Theme::default(),
                );
            })
            .expect("render settings");
        let output = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(output.contains("Settings"));
        assert!(output.contains("One"));
    }

    #[test]
    fn disabled_row_preserves_the_reason() {
        let row = row("Priority", "High").disabled("Unsupported");
        assert!(!row.enabled);
        assert_eq!(row.disabled_reason, "Unsupported");
        assert_eq!(
            row.cells(),
            vec!["Priority".to_string(), "High".to_string()]
        );
    }

    #[test]
    fn narrow_settings_rows_use_list_geometry() {
        let area = Rect::new(2, 4, 80, 8);
        let list = RowLayout::list(area, 2, 1);
        assert_eq!(list.index_at(5), Some(0));
        assert_eq!(list.index_at(6), Some(1));
        assert_eq!(layout(area, 2, 0).index_at(5), None);
    }
}
#[cfg(test)]
mod scroll_tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::{SettingKind, SettingRow, render};
    use crate::tui::pages::settings::SettingId;
    use crate::tui::theme::Theme;

    #[test]
    fn settings_table_renders_a_scrollbar_when_rows_overflow() {
        let rows = (0..20)
            .map(|index| {
                SettingRow::new(
                    SettingId::AutoCheckUpdates,
                    format!("Setting {index}"),
                    "",
                    "on",
                    SettingKind::Toggle,
                    "Space",
                )
            })
            .collect::<Vec<_>>();
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                render(
                    frame,
                    frame.area(),
                    "Settings",
                    ["Setting", "Value"],
                    &rows,
                    0,
                    true,
                    Theme::default(),
                );
            })
            .expect("draw settings");
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("↑"), "{text}");
        assert!(text.contains("↓"), "{text}");
    }
}
