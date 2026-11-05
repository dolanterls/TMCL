use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Block, Borders, List, ListItem, ListState},
};

use ratatui::style::Style;

use crate::tui::theme::Theme;
use crate::tui::widgets::rows::RowLayout;

pub fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    items: &[String],
    selected: usize,
    theme: Theme,
) -> RowLayout {
    render_focused(frame, area, title, items, selected, false, theme)
}

/// Draws the list and returns the row geometry it used, so the caller can
/// register exactly those rows as click targets.
pub fn render_focused(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    items: &[String],
    selected: usize,
    focused: bool,
    theme: Theme,
) -> RowLayout {
    let layout = RowLayout::list(area, items.len(), selected);
    render_layout(layout, frame, area, title, items, selected, focused, theme);
    layout
}

/// Draws the list into `layout`'s window.
///
/// The caller built that layout against the same `area` and registered its rows,
/// so the list cannot pick a window the mouse does not answer to.
#[allow(clippy::too_many_arguments)]
pub fn render_layout(
    layout: RowLayout,
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    items: &[String],
    selected: usize,
    focused: bool,
    theme: Theme,
) {
    let rows = items.iter().map(|item| ListItem::new(item.as_str()));
    let mut state = ListState::default().with_offset(layout.offset());
    state.select((!items.is_empty()).then_some(selected.min(items.len() - 1)));
    frame.render_stateful_widget(
        List::new(rows)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(if focused {
                        Style::default().fg(theme.accent)
                    } else {
                        Style::default().fg(theme.dim)
                    })
                    .title(title),
            )
            .style(theme.base())
            .highlight_style(theme.selection_style()),
        area,
        &mut state,
    );
}

/// [`render_focused`] with an explicit window offset.
///
/// Kept for callers that already resolved the window themselves; the returned
/// layout carries that offset instead of re-deriving one, so hit testing matches
/// the rows that were actually drawn.
#[allow(clippy::too_many_arguments)]
pub fn render_focused_with_offset(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    items: &[String],
    selected: usize,
    focused: bool,
    theme: Theme,
    offset: usize,
) -> RowLayout {
    let layout = RowLayout::list(area, items.len(), selected).moved_to(offset);
    render_layout(layout, frame, area, title, items, selected, focused, theme);
    layout
}
