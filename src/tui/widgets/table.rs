//! Table widget with binsider-style decoration.
//!
//! Table decoration follows binsider `src/tui/ui.rs` (commit 7f0633d1,
//! MIT OR Apache-2.0): bold header, `|n/m|` counter, a right-side
//! scrollbar with up/down arrows, and green row selection.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table, TableState},
};

use crate::tui::theme::Theme;
use crate::tui::widgets::rows::RowLayout;

#[allow(clippy::too_many_arguments)]
pub fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    headers: &[&str],
    rows: Vec<Vec<String>>,
    selected: Option<usize>,
    widths: &[Constraint],
    focused: bool,
    theme: Theme,
) -> RowLayout {
    render_owned(
        frame,
        area,
        title.to_owned(),
        headers.iter().map(|header| (*header).to_owned()).collect(),
        rows,
        selected,
        widths,
        focused,
        theme,
    )
}

/// Draws the table and returns the row geometry it used, so the caller can
/// register exactly those rows as click targets.
#[allow(clippy::too_many_arguments)]
pub fn render_owned(
    frame: &mut Frame<'_>,
    area: Rect,
    title: String,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    selected: Option<usize>,
    widths: &[Constraint],
    focused: bool,
    theme: Theme,
) -> RowLayout {
    let layout = RowLayout::table(area, rows.len(), selected.unwrap_or(0));
    render_layout(
        layout, frame, area, title, headers, rows, selected, widths, focused, theme,
    );
    layout
}

/// Draws the table into `layout`'s window.
///
/// The caller built that layout against the same `area` and registered its rows,
/// so the table cannot pick a window the mouse does not answer to, and the list
/// keeps its position while the selection moves inside it.
#[allow(clippy::too_many_arguments)]
pub fn render_layout(
    layout: RowLayout,
    frame: &mut Frame<'_>,
    area: Rect,
    title: String,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    selected: Option<usize>,
    widths: &[Constraint],
    focused: bool,
    theme: Theme,
) {
    render_impl(
        frame,
        area,
        title,
        headers,
        rows,
        selected,
        widths,
        focused,
        theme,
        layout.offset(),
    );
}

#[allow(clippy::too_many_arguments)]
fn render_impl(
    frame: &mut Frame<'_>,
    area: Rect,
    title: String,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    selected: Option<usize>,
    widths: &[Constraint],
    focused: bool,
    theme: Theme,
    offset: usize,
) {
    let inner_width = area.width.saturating_sub(2);
    let columns = Layout::horizontal(widths.to_vec())
        .spacing(1)
        .split(Rect::new(0, 0, inner_width, 1));
    let limits = columns
        .iter()
        .map(|column| usize::from(column.width))
        .collect::<Vec<_>>();
    let header = Row::new(headers.iter().enumerate().map(|(index, header)| {
        Cell::from(fit_cell(
            header,
            limits.get(index).copied().unwrap_or(usize::MAX),
        ))
    }))
    .style(Style::default().add_modifier(Modifier::BOLD));
    let total = rows.len();
    let counter = selected.filter(|_| total > 0).map(|index| {
        Line::from(vec![
            Span::styled("|", theme.dim()),
            Span::styled(
                format!("{}/{}", index.saturating_add(1), total),
                theme.accent_style(),
            ),
            Span::styled("|", theme.dim()),
        ])
    });
    let rows = rows
        .into_iter()
        .map(|row| {
            Row::new(row.into_iter().enumerate().map(|(index, cell)| {
                Cell::from(fit_cell(
                    &cell,
                    limits.get(index).copied().unwrap_or(usize::MAX),
                ))
            }))
        })
        .collect::<Vec<_>>();
    let mut state = TableState::default().with_offset(offset);
    state.select(selected);
    let block = crate::tui::chrome::panel(&title, focused, theme)
        .title_bottom(counter.unwrap_or_default().right_aligned());
    frame.render_stateful_widget(
        Table::new(rows, widths.to_vec())
            .header(header)
            .block(block)
            .style(theme.base())
            .row_highlight_style(theme.selection_style())
            .column_spacing(1),
        area,
        &mut state,
    );
    render_scrollbar(frame, area, total, selected);
}

fn render_scrollbar(frame: &mut Frame<'_>, area: Rect, total: usize, selected: Option<usize>) {
    let visible = usize::from(area.height.saturating_sub(3)).max(1);
    if total <= visible {
        return;
    }
    let mut state = ScrollbarState::new(total)
        .position(selected.unwrap_or(0))
        .viewport_content_length(visible);
    frame.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓")),
        area,
        &mut state,
    );
}

fn fit_cell(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if Line::from(value).width() <= width {
        return value.to_owned();
    }
    let budget = width.saturating_sub(1);
    let mut fitted = String::new();
    let mut used = 0;
    for character in value.chars() {
        let character_width = Line::from(character.to_string()).width();
        if used + character_width > budget {
            break;
        }
        fitted.push(character);
        used += character_width;
    }
    fitted.push('…');
    fitted
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, layout::Constraint};

    use super::render;
    use crate::tui::theme::Theme;

    #[test]
    fn table_renders_headers_and_rows_at_supported_sizes() {
        for (width, height) in [(120, 40), (100, 30), (80, 24)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).expect("test terminal");
            terminal
                .draw(|frame| {
                    let _ = render(
                        frame,
                        frame.area(),
                        "Instances",
                        &["Status", "Name"],
                        vec![vec!["● Ready".into(), "Fabric".into()]],
                        Some(0),
                        &[Constraint::Length(10), Constraint::Min(1)],
                        true,
                        Theme::default(),
                    );
                })
                .expect("draw table");
            let rendered = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert!(rendered.contains("Instances"));
            assert!(rendered.contains("Fabric"));
        }
    }
}

#[cfg(test)]
mod color_tests {
    use ratatui::{Terminal, backend::TestBackend, layout::Constraint};

    use super::render;
    use crate::tui::theme::Theme;

    #[test]
    fn table_uses_green_selection_and_accent_panel_title() {
        let theme = Theme::default();
        let backend = TestBackend::new(60, 12);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                render(
                    frame,
                    frame.area(),
                    "Instances",
                    &["Status", "Name"],
                    vec![
                        vec!["● Ready".into(), "alpha".into()],
                        vec!["○ Ready".into(), "beta".into()],
                    ],
                    Some(0),
                    &[Constraint::Length(10), Constraint::Min(1)],
                    true,
                    theme,
                );
            })
            .expect("draw table");
        let buffer = terminal.backend().buffer();
        let mut selected_green = false;
        let mut title_accent = false;
        for y in 0..12 {
            for x in 0..60 {
                let cell = &buffer[(x, y)];
                if cell.fg == theme.selection {
                    selected_green = true;
                }
                if cell.fg == theme.accent {
                    title_accent = true;
                }
            }
        }
        assert!(selected_green, "selected row should be green");
        assert!(title_accent, "panel title should be accent");
    }
}

#[cfg(test)]
mod scroll_tests {
    use ratatui::{Terminal, backend::TestBackend, layout::Constraint};

    use super::render;
    use crate::tui::theme::Theme;

    #[test]
    fn table_renders_a_scrollbar_when_rows_overflow() {
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                render(
                    frame,
                    frame.area(),
                    "Rows",
                    &["Value"],
                    (0..20).map(|index| vec![format!("row {index}")]).collect(),
                    Some(0),
                    &[Constraint::Min(1)],
                    true,
                    Theme::default(),
                );
            })
            .expect("draw table");
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
