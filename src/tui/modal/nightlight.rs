//! `nightlight` modal rendering: a raised, borderless card with an accent
//! title. State and key handling stay in [`Modal`]; only the look changes.
//! Design inspiration: nightlightd (GPL-3.0-or-later).
//! This file is an independent implementation; no source code is copied.

use std::collections::HashMap;

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::core::version::models::VersionCategory;
use crate::i18n::I18n;
use crate::tui::action::{Action, ModalAct, ScrollTarget};
use crate::tui::app::view::nightlight::tone::Palette;
use crate::tui::app::view::nightlight::widgets::{dialog_card, width};
use crate::tui::hit::Hitmap;
use crate::tui::palette::truncate;

use super::{
    Modal, SelectionState, VersionPickerState, selection_display, selection_picker_options,
};

impl Modal {
    pub fn draw_nightlight(
        &self,
        frame: &mut Frame<'_>,
        i18n: &I18n,
        area: Rect,
        t: f64,
        pal: &Palette,
        hits: &Hitmap,
    ) {
        match self {
            Self::Form {
                title,
                labels,
                values,
                cursors,
                focused,
                directory_field,
                file_field,
                selection_options,
                selection_states,
                selection_picker_open,
                selection_selected,
                selection_scroll,
                version_picker,
                ..
            } => {
                if let Some(field) = *selection_picker_open {
                    let options = selection_picker_options(
                        field,
                        selection_options,
                        version_picker.as_deref(),
                    );
                    picker(
                        frame,
                        area,
                        title,
                        field,
                        &options,
                        *selection_selected,
                        *selection_scroll,
                        selection_states,
                        version_picker
                            .as_deref()
                            .filter(|picker| picker.field == field),
                        t,
                        i18n,
                        pal,
                        hits,
                    );
                } else {
                    form(
                        frame,
                        area,
                        title,
                        labels,
                        values,
                        cursors,
                        *focused,
                        *directory_field,
                        *file_field,
                        selection_options,
                        version_picker.as_deref(),
                        t,
                        i18n,
                        pal,
                        hits,
                    );
                }
            }
            Self::Prompt {
                title,
                label,
                help,
                value,
                cursor,
                ..
            } => prompt(
                frame, area, title, label, help, value, *cursor, t, i18n, pal, hits,
            ),
            Self::Confirm { prompt, .. } => confirm(frame, area, i18n, prompt, pal, hits),
            Self::Checklist {
                title,
                items,
                checked,
                focused,
                scroll,
                note,
                ..
            } => checklist(
                frame,
                area,
                title,
                items,
                checked,
                *focused,
                *scroll,
                note.as_deref(),
                i18n,
                pal,
                hits,
            ),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn form(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    labels: &[String],
    values: &[String],
    cursors: &[usize],
    focused: usize,
    directory_field: Option<usize>,
    file_field: Option<usize>,
    selection_options: &HashMap<usize, Vec<String>>,
    version_picker: Option<&VersionPickerState>,
    t: f64,
    i18n: &I18n,
    pal: &Palette,
    hits: &Hitmap,
) {
    let inner = dialog_card(frame, area, title, pal);
    if inner.is_empty() {
        return;
    }
    let label_width = labels.iter().map(|label| width(label)).max().unwrap_or(0);
    let mut y = inner.y;
    for (index, label) in labels.iter().enumerate() {
        if y >= inner.bottom() {
            break;
        }
        let active = index == focused;
        let background = if active { pal.selection } else { pal.surface };
        frame.render_widget(
            Block::default().style(Style::default().bg(background)),
            Rect::new(inner.x, y, inner.width, 1),
        );
        let mut spans = vec![
            Span::styled(
                if active { "▎ " } else { "  " },
                Style::default().fg(pal.accent).bg(background),
            ),
            Span::styled(
                format!("{label:<label_width$}  ", label = label),
                Style::default().fg(pal.muted).bg(background),
            ),
        ];
        let value = values.get(index).map(String::as_str).unwrap_or("");
        let budget = inner
            .width
            .saturating_sub(4)
            .saturating_sub(u16::try_from(label_width).unwrap_or(0));
        let choice = selection_options.get(&index);
        if let Some(options) = choice {
            // A choice field stores a value, not text: paint the label the
            // picker shows rather than the raw `display|value` string, and do
            // not put a text cursor on a field that cannot be typed into.
            let display = options
                .iter()
                .find(|option| *option == value)
                .map_or(value, |option| selection_display(option));
            spans.push(Span::styled(
                truncate(display, usize::from(budget)),
                Style::default()
                    .fg(if active { pal.text } else { pal.muted })
                    .bg(background),
            ));
        } else {
            let cursor = cursors.get(index).copied().unwrap_or(value.chars().count());
            let style = crate::tui::text::FieldStyle {
                text: pal.text,
                dim: pal.muted,
                bg: background,
                cursor_fg: pal.bg,
                cursor_bg: pal.accent,
            };
            spans.extend(
                crate::tui::text::render(
                    value,
                    cursor,
                    budget,
                    active,
                    crate::tui::text::blink_on(t),
                    style,
                    "",
                )
                .spans,
            );
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(background)),
            Rect::new(inner.x, y, inner.width, 1),
        );
        // A row is a button: clicking focuses the field, and a field with
        // options opens its picker the same way Enter does. Without this the
        // dialog was keyboard-only.
        let has_options =
            choice.is_some() || version_picker.is_some_and(|picker| picker.field == index);
        let act = if has_options {
            ModalAct::Toggle(index)
        } else {
            ModalAct::Focus(index)
        };
        hits.add(Rect::new(inner.x, y, inner.width, 1), Action::Modal(act));
        y = y.saturating_add(1);
    }
    // A picker field is the one thing in this form that `Enter` cannot reach:
    // `Enter` walks to the next field, so the browse key has to be printed or
    // the field is keyboard-only in a way nothing on screen admits.
    let mut hints: Vec<(&str, String)> = Vec::new();
    let mut acts: Vec<Option<ModalAct>> = Vec::new();
    if directory_field == Some(focused) {
        hints.push(("F2", i18n.text("hint.browse")));
        acts.push(Some(ModalAct::Browse(focused)));
    } else if file_field == Some(focused) {
        hints.push(("F2", i18n.text("hint.browse")));
        acts.push(Some(ModalAct::BrowseFile(focused)));
    }
    // What `Enter` does here depends on the field: a choice field opens its
    // list, a text field advances, and only the last one saves.
    let has_options = selection_options.contains_key(&focused)
        || version_picker.is_some_and(|picker| picker.field == focused);
    if has_options {
        hints.push(("Enter", i18n.text("action.select")));
        acts.push(Some(ModalAct::Toggle(focused)));
    } else if focused + 1 < labels.len() {
        hints.push(("Enter", i18n.text("hint.next_field")));
        acts.push(Some(ModalAct::Focus(focused + 1)));
    } else {
        hints.push(("Enter", i18n.text("action.save")));
        acts.push(Some(ModalAct::Submit));
    }
    hints.push(("Esc", i18n.text("hint.cancel")));
    acts.push(Some(ModalAct::Cancel));
    hints_row(
        frame,
        Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
        &hints,
        &acts,
        pal,
        hits,
    );
}

#[allow(clippy::too_many_arguments)]
fn prompt(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    label: &str,
    help: &str,
    value: &str,
    cursor: usize,
    t: f64,
    i18n: &I18n,
    pal: &Palette,
    hits: &Hitmap,
) {
    let inner = dialog_card(frame, area, title, pal);
    if inner.is_empty() {
        return;
    }
    let style = crate::tui::text::FieldStyle {
        text: pal.text,
        dim: pal.muted,
        bg: pal.selection,
        cursor_fg: pal.bg,
        cursor_bg: pal.accent,
    };
    let mut spans = vec![Span::styled(
        format!(" {label} "),
        Style::default().fg(pal.muted).bg(pal.selection),
    )];
    spans.extend(
        crate::tui::text::render(
            value,
            cursor,
            inner.width.saturating_sub(width(label) as u16 + 2),
            true,
            crate::tui::text::blink_on(t),
            style,
            "",
        )
        .spans,
    );
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(pal.selection)),
        Rect::new(inner.x, inner.y + 1, inner.width, 1),
    );
    if !help.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                truncate(help, usize::from(inner.width)),
                Style::default().fg(pal.faint).bg(pal.surface),
            ))),
            Rect::new(inner.x, inner.y + 2, inner.width, 1),
        );
    }
    hints_row(
        frame,
        Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
        &[
            ("Enter", i18n.text("action.save")),
            ("Esc", i18n.text("hint.cancel")),
        ],
        &[Some(ModalAct::Submit), Some(ModalAct::Cancel)],
        pal,
        hits,
    );
}

fn confirm(
    frame: &mut Frame<'_>,
    area: Rect,
    i18n: &I18n,
    prompt: &str,
    pal: &Palette,
    hits: &Hitmap,
) {
    let inner = dialog_card(frame, area, &i18n.text("modal.confirm"), pal);
    if inner.is_empty() {
        return;
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            // A prompt longer than the card used to be cut mid-word with no
            // ellipsis, which is the worst case for a destructive dialog: the
            // object being deleted was exactly the part that vanished.
            truncate(&format!("! {prompt}"), usize::from(inner.width)),
            Style::default().fg(pal.err).bg(pal.surface),
        ))),
        Rect::new(inner.x, inner.y + 1, inner.width, 1),
    );
    hints_row(
        frame,
        Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
        &[
            ("Enter", i18n.text("hint.confirm")),
            ("Esc", i18n.text("hint.cancel")),
        ],
        &[Some(ModalAct::Confirm), Some(ModalAct::Reject)],
        pal,
        hits,
    );
}

#[allow(clippy::too_many_arguments)]
fn checklist(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    items: &[super::ChecklistItem],
    checked: &[bool],
    focused: usize,
    scroll: usize,
    note: Option<&str>,
    i18n: &I18n,
    pal: &Palette,
    hits: &Hitmap,
) {
    let inner = dialog_card(frame, area, title, pal);
    if inner.is_empty() {
        return;
    }
    if items.is_empty() {
        frame.render_widget(
            Paragraph::new(note.unwrap_or_default())
                .style(Style::default().fg(pal.muted).bg(pal.surface)),
            inner,
        );
        return;
    }
    let rows = inner.height.saturating_sub(1);
    for (visual, index) in (scroll..items.len()).take(usize::from(rows)).enumerate() {
        let item = &items[index];
        let y = inner.y.saturating_add(visual as u16);
        let active = index == focused;
        let background = if active { pal.selection } else { pal.surface };
        let mark = if checked.get(index).copied().unwrap_or(false) {
            "●"
        } else {
            "○"
        };
        let mut spans = vec![
            Span::styled(
                if active { "▎ " } else { "  " },
                Style::default().fg(pal.accent).bg(background),
            ),
            Span::styled(
                format!("{mark} "),
                Style::default()
                    .fg(if checked.get(index).copied().unwrap_or(false) {
                        pal.ok
                    } else {
                        pal.faint
                    })
                    .bg(background),
            ),
            Span::styled(
                truncate(&item.label, usize::from(inner.width).saturating_sub(6)),
                Style::default()
                    .fg(if active { pal.text } else { pal.muted })
                    .bg(background),
            ),
        ];
        // `R` (retry) and the count live on the last row; keep the list clean.
        if index == items.len().saturating_sub(1) {
            let count = format!("{}/{}", focused + 1, items.len());
            if usize::from(inner.width) > 6 + width(&count) {
                spans.push(Span::styled(
                    " ".repeat(usize::from(inner.width) - 6 - width(&count)),
                    Style::default().bg(background),
                ));
                spans.push(Span::styled(
                    count,
                    Style::default().fg(pal.faint).bg(background),
                ));
            }
        }
        let rect = Rect::new(inner.x, y, inner.width, 1);
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(background)),
            rect,
        );
        // A row is a checkbox: clicking it toggles that entry, exactly like
        // Space does on the focused row.
        hits.add(rect, Action::Modal(ModalAct::CheckItem(index)));
    }
    hints_row(
        frame,
        Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
        &[
            ("Space", i18n.text("hint.toggle")),
            ("Enter", i18n.text("hint.apply")),
            // Same casing as the other two presets for the same key.
            ("R", i18n.text("hint.reset")),
            ("Esc", i18n.text("hint.cancel")),
        ],
        &[
            Some(ModalAct::CheckItem(focused)),
            Some(ModalAct::ApplyChecks),
            Some(ModalAct::ResetChecks),
            Some(ModalAct::Cancel),
        ],
        pal,
        hits,
    );
}

/// The four version-category chips, one row under the search header.
///
/// `nightlight` used to leave this row out and put the search field there
/// instead, which made keys `1`-`4` (shared by all three presets) change a
/// filter that nothing displayed. Returns whether the row was drawn, so the
/// list can start below it.
///
/// The row degrades rather than disappearing: counts are dropped when the
/// dialog is too narrow for them, and only then the labels. All four chips stay
/// visible and clickable at any width, because a hidden control is worse than
/// a terse one.
fn draw_filter_chips(
    frame: &mut Frame<'_>,
    inner: Rect,
    picker: &VersionPickerState,
    i18n: &I18n,
    pal: &Palette,
    hits: &Hitmap,
) -> bool {
    if inner.height < 4 {
        return false;
    }
    let counts = picker.counts();
    let labels: Vec<(String, String, String)> = VersionCategory::ALL
        .iter()
        .map(|category| {
            (
                format!("{}", category.index() + 1),
                i18n.text(category.label_key()),
                format!("{}", counts[category.index()]),
            )
        })
        .collect();

    // Widest first: `n label count`, then `n label`, then `n`.
    let style: Option<u8> = [2u8, 1, 0].into_iter().find(|style| {
        let total = chip_row_width(&labels, *style);
        total <= usize::from(inner.width.saturating_sub(3))
    });
    let Some(style) = style else {
        return false;
    };

    let y = inner.y.saturating_add(1);
    let mut x = inner.x.saturating_add(2);
    for (index, (number, label, count)) in labels.iter().enumerate() {
        let enabled = picker.filter.is_enabled(VersionCategory::ALL[index]);
        let background = if enabled { pal.accent } else { pal.surface };
        let foreground = if enabled { pal.bg } else { pal.muted };
        let mut spans = vec![Span::styled(
            format!(" {number} "),
            Style::default().fg(foreground).bg(background),
        )];
        if style >= 1 {
            spans.push(Span::styled(
                format!("{label} "),
                Style::default().fg(foreground).bg(background),
            ));
        }
        if style >= 2 {
            spans.push(Span::styled(
                format!("{count} "),
                Style::default()
                    .fg(if enabled { pal.bg } else { pal.faint })
                    .bg(background),
            ));
        }
        let chip_width = u16::try_from(one_chip_width(&labels[index], style)).unwrap_or(u16::MAX);
        let rect = Rect::new(x, y, chip_width, 1);
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(background)),
            rect,
        );
        hits.add(rect, Action::Modal(ModalAct::ToggleVersionFilter(index)));
        x = x.saturating_add(chip_width).saturating_add(1);
    }
    true
}

fn one_chip_width(label: &(String, String, String), style: u8) -> usize {
    let mut total = width(&format!(" {} ", label.0));
    if style >= 1 {
        total += width(&format!("{} ", label.1));
    }
    if style >= 2 {
        total += width(&format!("{} ", label.2));
    }
    total
}

fn chip_row_width(labels: &[(String, String, String)], style: u8) -> usize {
    let chips: usize = labels
        .iter()
        .map(|label| one_chip_width(label, style))
        .sum();
    chips + labels.len().saturating_sub(1)
}

#[allow(clippy::too_many_arguments)]
fn picker(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    field: usize,
    options: &[String],
    selected: usize,
    scroll: usize,
    selection_states: &HashMap<usize, SelectionState>,
    version_picker: Option<&VersionPickerState>,
    t: f64,
    i18n: &I18n,
    pal: &Palette,
    hits: &Hitmap,
) {
    let inner = dialog_card(frame, area, title, pal);
    if inner.is_empty() {
        return;
    }
    // The header doubles as the filter field: typing `/` puts the query here,
    // so it has to show the query (and the cursor) instead of the list size.
    let query = version_picker.map_or("", |picker| picker.filter.search());
    let editing = version_picker.is_some_and(|picker| picker.editing_search);
    let cursor = version_picker.map_or(0, |picker| picker.search_cursor);
    let mut header = vec![Span::styled(
        " ⌕ ",
        Style::default()
            .fg(if editing { pal.accent } else { pal.muted })
            .bg(pal.surface),
    )];
    if query.is_empty() && !editing {
        header.push(Span::styled(
            format!("{} · {}", options.len(), title),
            Style::default().fg(pal.faint).bg(pal.surface),
        ));
    } else {
        let style = crate::tui::text::FieldStyle {
            text: pal.text,
            dim: pal.muted,
            bg: pal.surface,
            cursor_fg: pal.bg,
            cursor_bg: pal.accent,
        };
        header.extend(
            crate::tui::text::render(
                query,
                cursor,
                inner.width.saturating_sub(3),
                editing,
                crate::tui::text::blink_on(t),
                style,
                "",
            )
            .spans,
        );
    }
    let header_rect = Rect::new(inner.x, inner.y, inner.width, 1);
    frame.render_widget(
        Paragraph::new(Line::from(header)).style(Style::default().bg(pal.surface)),
        header_rect,
    );
    // The header doubles as the search field, so it has to take a click: `/`
    // was the only way in, and a field you can see but not click is a dead
    // control.
    hits.add(header_rect, Action::Modal(ModalAct::EditSearch));
    // The chip row the other two presets draw. Without it, `1`-`4` toggled a
    // filter that nothing on screen showed — the same "state exists, nothing
    // renders it" bug the classic settings search had.
    let chips = version_picker
        .is_some_and(|picker| draw_filter_chips(frame, inner, picker, i18n, pal, hits));
    let (list_top, rows) = if chips {
        (inner.y.saturating_add(2), inner.height.saturating_sub(3))
    } else {
        (inner.y.saturating_add(1), inner.height.saturating_sub(2))
    };
    // An empty list has three different reasons, and the other two presets say
    // which one it is. Leaving the body blank made a failed version fetch look
    // like a filter that matched nothing.
    if options.is_empty() && rows > 0 {
        let loaded = version_picker.is_some_and(|picker| !picker.versions.is_empty());
        let message = match selection_states.get(&field) {
            Some(SelectionState::Loading) => i18n.text("modal.loading_options"),
            Some(SelectionState::Error(error)) => {
                i18n.format("modal.load_failed", &[("error", error)])
            }
            Some(SelectionState::Unavailable(message)) => message.clone(),
            None if loaded => i18n.text("modal.no_versions_match"),
            None => i18n.text("modal.no_options"),
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                truncate(&message, usize::from(inner.width)),
                Style::default().fg(pal.muted).bg(pal.surface),
            )),
            Rect::new(inner.x, list_top, inner.width, 1),
        );
    }
    let mut first_row: Option<u16> = None;
    let mut last_row: Option<u16> = None;
    for (visual, index) in (scroll..options.len()).take(usize::from(rows)).enumerate() {
        let y = list_top.saturating_add(visual as u16);
        let active = index == selected;
        let background = if active { pal.selection } else { pal.surface };
        let mut spans = vec![
            Span::styled(
                if active { "▎ " } else { "  " },
                Style::default().fg(pal.accent).bg(background),
            ),
            Span::styled(
                truncate(
                    selection_display(&options[index]),
                    usize::from(inner.width).saturating_sub(4),
                ),
                Style::default()
                    .fg(if active { pal.text } else { pal.muted })
                    .bg(background),
            ),
        ];
        if index == options.len().saturating_sub(1) {
            let count = format!("{}/{}", selected + 1, options.len());
            let pad = usize::from(inner.width).saturating_sub(2 + width(&count));
            spans.push(Span::styled(
                " ".repeat(pad),
                Style::default().bg(background),
            ));
            spans.push(Span::styled(
                count,
                Style::default().fg(pal.faint).bg(background),
            ));
        }
        let rect = Rect::new(inner.x, y, inner.width, 1);
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(background)),
            rect,
        );
        // A row is an option: the click has to reach the same `Option(i)` the
        // shared picker registers, and the wheel has to reach the same list.
        hits.add(rect, Action::Modal(ModalAct::Option(index)));
        first_row.get_or_insert(y);
        last_row = Some(y);
    }
    if let (Some(top), Some(bottom)) = (first_row, last_row) {
        hits.scroll(
            Rect::new(inner.x, top, inner.width, bottom.saturating_sub(top) + 1),
            ScrollTarget::ModalOptions,
        );
    }
    // Every cap here has to do what the key does. `Enter` picks the highlighted
    // option and `Esc` only closes the list — the dialog's own save and cancel
    // are the form's keys, not the list's.
    let mut hints: Vec<(&str, String)> = Vec::new();
    let mut acts: Vec<Option<ModalAct>> = Vec::new();
    let failed = matches!(selection_states.get(&field), Some(SelectionState::Error(_)));
    if editing {
        // Typing a query: `Enter` ends the query and `Esc` clears it.
        hints.push(("Enter", i18n.text("hint.done")));
        acts.push(None);
        hints.push(("Esc", i18n.text("hint.clear")));
        acts.push(None);
    } else {
        if version_picker.is_some() {
            hints.push(("/", i18n.text("action.search")));
            acts.push(Some(ModalAct::EditSearch));
        }
        if failed || (options.is_empty() && !selection_states.contains_key(&field)) {
            hints.push(("r", i18n.text("action.retry")));
            acts.push(Some(ModalAct::RetryOptions));
        }
        hints.push(("Enter", i18n.text("hint.confirm")));
        acts.push(Some(ModalAct::Option(selected)));
        hints.push(("Esc", i18n.text("hint.cancel")));
        acts.push(Some(ModalAct::CloseOptions));
    }
    hints_row(
        frame,
        Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
        &hints,
        &acts,
        pal,
        hits,
    );
}

/// Draws the key hints and registers each one as a click target. A dialog whose
/// buttons are painted but not registered is a dialog you can only use from the
/// keyboard, which is exactly what the shared hit map exists to prevent.
fn hints_row(
    frame: &mut Frame<'_>,
    area: Rect,
    hints: &[(&str, String)],
    acts: &[Option<ModalAct>],
    pal: &Palette,
    hits: &Hitmap,
) {
    if area.is_empty() {
        return;
    }
    let mut spans = Vec::new();
    let mut column: u16 = 0;
    for (index, (key, label)) in hints.iter().enumerate() {
        let chunk = format!(" {label}   ");
        let chunk_width = u16::try_from(width(key) + width(&chunk)).unwrap_or(0);
        if let Some(Some(act)) = acts.get(index) {
            hits.add(
                Rect::new(
                    area.x.saturating_add(column),
                    area.y,
                    chunk_width.min(area.width.saturating_sub(column)),
                    1,
                ),
                Action::Modal(*act),
            );
        }
        spans.push(Span::styled(
            (*key).to_owned(),
            Style::default()
                .fg(pal.accent)
                .bg(pal.surface)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            chunk,
            Style::default().fg(pal.muted).bg(pal.surface),
        ));
        column = column.saturating_add(chunk_width);
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans))
            .alignment(Alignment::Left)
            .style(Style::default().bg(pal.surface)),
        area,
    );
}
