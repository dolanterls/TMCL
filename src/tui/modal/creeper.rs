//! `creeper` preset rendering for dialogs.
//!
//! Behaviour (key handling, submission, picker scrolling) stays in the shared
//! `Modal` methods; this file only decides how the same fields look.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use super::*;
use crate::tui::app::view::creeper::tone::Palette;
use crate::tui::palette;
use crate::tui::widgets::kit as widgets;

impl Modal {
    pub fn draw_creeper(
        &self,
        frame: &mut Frame<'_>,
        i18n: &I18n,
        pal: &Palette,
        area: Rect,
        t: f64,
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
                    let start = (*selection_scroll).min(options.len());
                    let end = start.saturating_add(PICKER_VISIBLE_ROWS).min(options.len());
                    let option_label = labels
                        .get(field)
                        .map(|label| i18n.text_or_raw(label))
                        .unwrap_or_else(|| i18n.text("modal.options"));
                    let title = format!("{} {option_label}", i18n.text("modal.select"));
                    let block = widgets::card(&title, pal.lime, true, t, pal.surface);
                    let inner = block.inner(area);
                    frame.render_widget(block, area);
                    if inner.is_empty() {
                        return;
                    }
                    let mut lines = Vec::new();
                    if let Some(picker) = version_picker
                        .as_ref()
                        .filter(|picker| picker.field == field)
                    {
                        let counts = picker.counts();
                        for pair in [
                            (VersionCategory::Release, VersionCategory::Snapshot),
                            (VersionCategory::Legacy, VersionCategory::AprilFools),
                        ] {
                            let mut spans = Vec::new();
                            let line = lines.len();
                            let mut x = inner.x;
                            for category in [pair.0, pair.1] {
                                if !spans.is_empty() {
                                    spans.push(Span::styled(
                                        "   ",
                                        Style::default().bg(pal.surface),
                                    ));
                                    x = x.saturating_add(3);
                                }
                                let enabled = picker.filter.is_enabled(category);
                                let number = format!(" {} ", category.index() + 1);
                                let label = format!(" {} ", i18n.text(category.label_key()));
                                let count = format!("{} ", counts[category.index()]);
                                let width = palette::width(&number)
                                    + palette::width(&label)
                                    + palette::width(&count);
                                hits.add(
                                    Rect::new(
                                        x,
                                        inner.y.saturating_add(
                                            u16::try_from(line).unwrap_or(u16::MAX),
                                        ),
                                        u16::try_from(width).unwrap_or(u16::MAX),
                                        1,
                                    ),
                                    Action::Modal(ModalAct::ToggleVersionFilter(category.index())),
                                );
                                x = x.saturating_add(u16::try_from(width).unwrap_or(0));
                                spans.push(Span::styled(
                                    number,
                                    Style::default()
                                        .fg(pal.bg)
                                        .bg(palette::mix(
                                            pal.text_dim,
                                            if enabled { pal.green } else { pal.text_faint },
                                            0.55,
                                        ))
                                        .add_modifier(Modifier::BOLD),
                                ));
                                spans.push(Span::styled(
                                    label,
                                    Style::default()
                                        .fg(if enabled { pal.text } else { pal.text_faint })
                                        .bg(pal.surface),
                                ));
                                spans.push(Span::styled(
                                    count,
                                    Style::default()
                                        .fg(if enabled { pal.cyan } else { pal.text_faint })
                                        .bg(pal.surface),
                                ));
                            }
                            lines.push(Line::from(spans));
                        }
                        let search_line = lines.len();
                        hits.add(
                            Rect::new(
                                inner.x,
                                inner
                                    .y
                                    .saturating_add(u16::try_from(search_line).unwrap_or(u16::MAX)),
                                inner.width,
                                1,
                            ),
                            Action::Modal(ModalAct::EditSearch),
                        );
                        let search = if picker.editing_search {
                            let style = crate::tui::text::FieldStyle {
                                text: pal.text,
                                dim: pal.text_faint,
                                bg: pal.surface,
                                cursor_fg: pal.bg,
                                cursor_bg: pal.cyan,
                            };
                            crate::tui::text::render(
                                picker.filter.search(),
                                picker.search_cursor,
                                inner.width,
                                true,
                                crate::tui::text::blink_on(t),
                                style,
                                &i18n.text("modal.search_placeholder"),
                            )
                        } else {
                            Line::from(Span::styled(
                                format!(
                                    "{}: {}",
                                    i18n.text("common.search"),
                                    picker.filter.search()
                                ),
                                Style::default().fg(pal.text_dim).bg(pal.surface),
                            ))
                        };
                        lines.push(search);
                    }
                    if options.is_empty() {
                        let message = match selection_states.get(&field) {
                            Some(SelectionState::Loading) => i18n.text("modal.loading_options"),
                            Some(SelectionState::Error(error)) => {
                                i18n.format("modal.load_failed", &[("error", error)])
                            }
                            Some(SelectionState::Unavailable(message)) => message.clone(),
                            None => i18n.text("modal.no_options"),
                        };
                        lines.push(Line::from(Span::styled(
                            format!(" {message} "),
                            Style::default().fg(pal.gold).bg(pal.surface),
                        )));
                    }
                    if start > 0 {
                        lines.push(Line::from(Span::styled(
                            i18n.text("modal.more_up"),
                            Style::default().fg(pal.text_faint).bg(pal.surface),
                        )));
                    }
                    let mut option_rows = Vec::new();
                    for (index, option) in options.iter().enumerate().take(end).skip(start) {
                        option_rows.push((lines.len(), index));
                        let selected = index == *selection_selected;
                        let bg = if selected {
                            palette::mix(pal.surface, pal.lime, 0.18)
                        } else {
                            pal.surface
                        };
                        // Pad to the full inner width so the highlight covers
                        // the whole row, not just the text.
                        let text_width = usize::from(inner.width).saturating_sub(3);
                        lines.push(Line::from(vec![
                            Span::styled(
                                if selected { " ▌ " } else { "   " },
                                Style::default()
                                    .fg(palette::mix(pal.lime, pal.text, 0.2))
                                    .bg(bg)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                palette::fit(selection_display(option), text_width),
                                Style::default()
                                    .fg(if selected { pal.text } else { pal.text_dim })
                                    .bg(bg)
                                    .add_modifier(if selected {
                                        Modifier::BOLD
                                    } else {
                                        Modifier::empty()
                                    }),
                            ),
                        ]));
                    }
                    if end < options.len() {
                        lines.push(Line::from(Span::styled(
                            i18n.text("modal.more_down"),
                            Style::default().fg(pal.text_faint).bg(pal.surface),
                        )));
                    }
                    if options.len() > PICKER_VISIBLE_ROWS {
                        lines.push(Line::from(Span::styled(
                            format!(
                                " ↕ {}/{} ",
                                (*selection_selected).saturating_add(1).min(options.len()),
                                options.len()
                            ),
                            Style::default().fg(pal.text_faint).bg(pal.surface),
                        )));
                    }
                    lines.push(Line::from(""));
                    let help = match (version_picker.as_ref(), selection_states.get(&field)) {
                        (Some(picker), Some(SelectionState::Error(_))) if picker.editing_search => {
                            i18n.text("modal.search_help")
                        }
                        (Some(_), Some(SelectionState::Error(_))) => {
                            i18n.text("modal.retry_filters_help")
                        }
                        (_, Some(SelectionState::Error(_))) => i18n.text("modal.retry_help"),
                        (_, Some(SelectionState::Loading | SelectionState::Unavailable(_))) => {
                            i18n.text("modal.back_help")
                        }
                        (Some(picker), _) if picker.editing_search => {
                            i18n.text("modal.search_help")
                        }
                        (Some(_), _) => i18n.text("modal.filter_help"),
                        (None, _) if options.is_empty() => i18n.text("modal.load_retry_help"),
                        (None, _) => i18n.text("modal.choose_help"),
                    };
                    lines.push(Line::from(Span::styled(
                        format!(" {help} "),
                        Style::default().fg(pal.text_faint).bg(pal.surface),
                    )));
                    frame.render_widget(
                        Paragraph::new(lines).style(Style::default().bg(pal.surface)),
                        inner,
                    );
                    for (line, index) in &option_rows {
                        hits.add(
                            Rect::new(
                                inner.x,
                                inner
                                    .y
                                    .saturating_add(u16::try_from(*line).unwrap_or(u16::MAX)),
                                inner.width,
                                1,
                            ),
                            Action::Modal(ModalAct::Option(*index)),
                        );
                    }
                    if let (Some((first, _)), Some((last, _))) =
                        (option_rows.first(), option_rows.last())
                    {
                        let top = inner.y.saturating_add(u16::try_from(*first).unwrap_or(0));
                        let bottom = inner.y.saturating_add(u16::try_from(*last).unwrap_or(0));
                        hits.scroll(
                            Rect::new(inner.x, top, inner.width, bottom.saturating_sub(top) + 1),
                            ScrollTarget::ModalOptions,
                        );
                    }
                    return;
                }

                let block =
                    widgets::card(&i18n.text_or_raw(title), pal.green, true, t, pal.surface);
                let inner = block.inner(area);
                frame.render_widget(block, area);
                if inner.is_empty() {
                    return;
                }
                let label_width = labels
                    .iter()
                    .map(|label| palette::width(&i18n.text_or_raw(label)))
                    .max()
                    .unwrap_or(0)
                    .min(usize::from(inner.width).saturating_sub(12));
                let value_width = inner
                    .width
                    .saturating_sub(u16::try_from(3 + label_width + 2).unwrap_or(u16::MAX));
                let mut lines = vec![Line::from("")];
                for (index, (label, value)) in labels.iter().zip(values).enumerate() {
                    let selected = index == *focused;
                    let label_text = i18n.text_or_raw(label);
                    let bg = if selected {
                        palette::mix(pal.surface, pal.green, 0.12)
                    } else {
                        pal.surface
                    };
                    let mut spans = vec![
                        Span::styled(
                            if selected { " ▌ " } else { "   " },
                            Style::default()
                                .fg(palette::mix(pal.green, pal.text, 0.2))
                                .bg(bg)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            palette::pad(&label_text, label_width),
                            Style::default().fg(pal.text_dim).bg(bg),
                        ),
                        Span::styled("  ", Style::default().bg(bg)),
                    ];
                    if selection_options.contains_key(&index) {
                        // Chosen from a list, never typed into: no cursor.
                        let display = selection_options
                            .get(&index)
                            .and_then(|options| options.iter().find(|option| *option == value))
                            .map_or(value.as_str(), |option| selection_display(option));
                        spans.push(Span::styled(
                            palette::truncate(display, usize::from(value_width)),
                            Style::default()
                                .fg(if selected {
                                    palette::mix(pal.lime, pal.text, 0.15)
                                } else {
                                    pal.text
                                })
                                .bg(bg)
                                .add_modifier(if selected {
                                    Modifier::BOLD
                                } else {
                                    Modifier::empty()
                                }),
                        ));
                    } else {
                        let style = crate::tui::text::FieldStyle {
                            text: if selected {
                                palette::mix(pal.lime, pal.text, 0.15)
                            } else {
                                pal.text
                            },
                            dim: pal.text_faint,
                            bg,
                            cursor_fg: pal.bg,
                            cursor_bg: pal.green,
                        };
                        let cursor = cursors
                            .get(index)
                            .copied()
                            .unwrap_or_else(|| value.chars().count());
                        spans.extend(
                            crate::tui::text::render(
                                value,
                                cursor,
                                value_width,
                                selected,
                                crate::tui::text::blink_on(t),
                                style,
                                "",
                            )
                            .spans,
                        );
                    }
                    lines.push(Line::from(spans));
                }
                for index in 0..labels.len() {
                    let has_options = selection_options.contains_key(&index)
                        || version_picker
                            .as_ref()
                            .is_some_and(|picker| picker.field == index);
                    let act = if has_options {
                        ModalAct::Toggle(index)
                    } else {
                        ModalAct::Focus(index)
                    };
                    hits.add(
                        Rect::new(
                            inner.x,
                            inner
                                .y
                                .saturating_add(u16::try_from(1 + index).unwrap_or(u16::MAX)),
                            inner.width,
                            1,
                        ),
                        Action::Modal(act),
                    );
                }
                lines.push(Line::from(""));
                let next = if labels.is_empty() {
                    0
                } else {
                    (*focused + 1) % labels.len()
                };
                let mut keys: Vec<(&str, &str)> = vec![("Tab", "hint.next_field")];
                let mut acts: Vec<Option<ModalAct>> = vec![Some(ModalAct::Focus(next))];
                if directory_field == &Some(*focused) {
                    keys.push(("F2", "hint.browse"));
                    acts.push(Some(ModalAct::Browse(*focused)));
                } else if file_field == &Some(*focused) {
                    keys.push(("F2", "hint.browse"));
                    acts.push(Some(ModalAct::BrowseFile(*focused)));
                }
                // A choice field opens its list on `Enter`; only a text field
                // walks to the next row. Printing "next" on a list field would
                // promise the wrong thing.
                let has_options = selection_options.contains_key(focused)
                    || version_picker
                        .as_ref()
                        .is_some_and(|picker| picker.field == *focused);
                if labels.len() > 1 {
                    if has_options {
                        keys.push(("Enter", "action.select"));
                        acts.push(Some(ModalAct::Toggle(*focused)));
                    } else {
                        // Enter walks the fields; F5 submits from anywhere.
                        keys.push(("Enter", "hint.next_step"));
                        acts.push(Some(ModalAct::Focus(next)));
                    }
                    keys.push(("F5", "hint.submit"));
                    acts.push(Some(ModalAct::Submit));
                } else {
                    keys.push(("Enter", "hint.submit"));
                    acts.push(Some(ModalAct::Submit));
                }
                keys.push(("Esc", "hint.cancel"));
                acts.push(Some(ModalAct::Cancel));
                let (key_row_line, caps) = key_row(pal, i18n, &keys);
                let key_line = lines.len();
                lines.push(key_row_line);
                super::register_caps(hits, inner, key_line, &caps, &acts);
                render_dialog(pal, frame, inner, lines);
            }
            Self::Confirm { prompt, .. } => {
                let block =
                    widgets::card(&i18n.text("modal.confirm"), pal.red, true, t, pal.surface);
                let inner = block.inner(area);
                frame.render_widget(block, area);
                if inner.is_empty() {
                    return;
                }
                let mut lines = vec![
                    Line::from(""),
                    Line::from(vec![
                        Span::styled(
                            " ! ",
                            Style::default()
                                .fg(pal.bg)
                                .bg(pal.red)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(" ", Style::default().bg(pal.surface)),
                        Span::styled(
                            palette::truncate(prompt, usize::from(inner.width).saturating_sub(6)),
                            Style::default()
                                .fg(palette::mix(pal.red, pal.text, 0.25))
                                .bg(pal.surface)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]),
                    Line::from(""),
                ];
                let (key_row_line, caps) = key_row(
                    pal,
                    i18n,
                    &[("Enter", "hint.confirm"), ("Esc", "hint.cancel")],
                );
                let key_line = lines.len();
                lines.push(key_row_line);
                super::register_caps(
                    hits,
                    inner,
                    key_line,
                    &caps,
                    &[Some(ModalAct::Confirm), Some(ModalAct::Reject)],
                );
                render_dialog(pal, frame, inner, lines);
            }
            Self::Prompt {
                title,
                label,
                help,
                value,
                cursor,
                ..
            } => {
                let block = widgets::card(&i18n.text_or_raw(title), pal.cyan, true, t, pal.surface);
                let inner = block.inner(area);
                frame.render_widget(block, area);
                if inner.is_empty() {
                    return;
                }
                let field_bg = palette::mix(pal.surface, pal.cyan, 0.12);
                let label_text = format!("{}: ", i18n.text_or_raw(label));
                // Three cells for ` ▸ `, then the label, then the field fill.
                let field_width = inner.width.saturating_sub(
                    u16::try_from(3 + palette::width(&label_text)).unwrap_or(u16::MAX),
                );
                let style = crate::tui::text::FieldStyle {
                    text: pal.text,
                    dim: pal.text_faint,
                    bg: field_bg,
                    cursor_fg: pal.bg,
                    cursor_bg: pal.cyan,
                };
                let mut spans = vec![
                    Span::styled(
                        " ▸ ",
                        Style::default()
                            .fg(pal.cyan)
                            .bg(pal.surface)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        label_text,
                        Style::default().fg(pal.text_dim).bg(pal.surface),
                    ),
                    Span::styled(" ", Style::default().bg(field_bg)),
                ];
                spans.extend(
                    crate::tui::text::render(
                        value,
                        *cursor,
                        field_width.saturating_sub(2),
                        true,
                        crate::tui::text::blink_on(t),
                        style,
                        "",
                    )
                    .spans,
                );
                spans.push(Span::styled(" ", Style::default().bg(field_bg)));
                let field = Line::from(spans);
                let mut lines = vec![Line::from(""), field];
                hits.add(
                    Rect::new(inner.x, inner.y.saturating_add(1), inner.width, 1),
                    Action::Modal(ModalAct::Focus(0)),
                );
                let (key_row_line, caps) = key_row(
                    pal,
                    i18n,
                    &[("Enter", "hint.submit"), ("Esc", "hint.cancel")],
                );
                let key_line = lines.len();
                lines.push(key_row_line);
                super::register_caps(
                    hits,
                    inner,
                    key_line,
                    &caps,
                    &[Some(ModalAct::Submit), Some(ModalAct::Cancel)],
                );
                lines.push(Line::from(Span::styled(
                    format!(" {} ", help),
                    Style::default().fg(pal.text_faint).bg(pal.surface),
                )));
                render_dialog(pal, frame, inner, lines);
            }
            Self::Checklist {
                title,
                items,
                checked,
                single,
                focused,
                scroll,
                note,
                retryable: _,
            } => {
                let block = widgets::card(&i18n.text_or_raw(title), pal.lime, true, t, pal.surface);
                let inner = block.inner(area);
                frame.render_widget(block, area);
                if inner.is_empty() {
                    return;
                }
                let start = (*scroll).min(items.len());
                let end = start
                    .saturating_add(super::CHECKLIST_VISIBLE_ROWS)
                    .min(items.len());
                let mut lines = vec![Line::from("")];
                if let Some(note) = note {
                    lines.push(Line::from(Span::styled(
                        format!(" {note} "),
                        Style::default().fg(pal.text_faint).bg(pal.surface),
                    )));
                }
                for (index, item) in items.iter().enumerate().take(end).skip(start) {
                    let row = lines.len();
                    let marker = match (*single, checked[index]) {
                        (true, true) => "(•) ",
                        (true, false) => "( ) ",
                        (false, true) => "[x] ",
                        (false, false) => "[ ] ",
                    };
                    let row_bg = if index == *focused {
                        palette::mix(pal.surface, pal.lime, 0.18)
                    } else {
                        pal.surface
                    };
                    let marker_style = Style::default()
                        .fg(if checked[index] {
                            pal.green
                        } else {
                            pal.text_faint
                        })
                        .bg(row_bg)
                        .add_modifier(if checked[index] {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        });
                    let label_style = Style::default()
                        .fg(if index == *focused {
                            pal.text
                        } else {
                            pal.text_dim
                        })
                        .bg(row_bg);
                    lines.push(Line::from(vec![
                        Span::styled(marker, marker_style),
                        Span::styled(format!(" {} ", i18n.text_or_raw(&item.label)), label_style),
                    ]));
                    hits.add(
                        Rect::new(
                            inner.x,
                            inner
                                .y
                                .saturating_add(u16::try_from(row).unwrap_or(u16::MAX)),
                            inner.width,
                            1,
                        ),
                        Action::Modal(ModalAct::CheckItem(index)),
                    );
                }
                let (key_row_line, caps) = key_row(
                    pal,
                    i18n,
                    &[
                        ("Space", "hint.toggle"),
                        ("Enter", "hint.apply"),
                        ("R", "hint.reset"),
                        ("Esc", "hint.cancel"),
                    ],
                );
                let key_line = lines.len();
                lines.push(key_row_line);
                super::register_caps(
                    hits,
                    inner,
                    key_line,
                    &caps,
                    &[
                        Some(ModalAct::CheckItem(*focused)),
                        Some(ModalAct::ApplyChecks),
                        Some(ModalAct::ResetChecks),
                        Some(ModalAct::Cancel),
                    ],
                );
                render_dialog(pal, frame, inner, lines);
            }
        }
    }
}

fn key_row(pal: &Palette, i18n: &I18n, keys: &[(&str, &str)]) -> (Line<'static>, Vec<(u16, u16)>) {
    let mut spans = Vec::new();
    let mut caps = Vec::new();
    let mut x: u16 = 0;
    for (index, (key, action)) in keys.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("  ", Style::default().bg(pal.surface)));
            x = x.saturating_add(2);
        }
        let width = u16::try_from(palette::width(&format!(" {key} "))).unwrap_or(u16::MAX);
        caps.push((x, width));
        spans.push(widgets::key_cap(key));
        x = x.saturating_add(width);
        let label = format!(" {}", i18n.text_or_raw(action));
        x = x.saturating_add(u16::try_from(palette::width(&label)).unwrap_or(0));
        spans.push(Span::styled(
            label,
            Style::default().fg(pal.text_dim).bg(pal.surface),
        ));
    }
    (Line::from(spans), caps)
}

/// Draw dialog lines from the top of the card.
///
/// Centring looked tidy in theory, but a 16-row popup holding six rows of
/// fields left a wide empty band between the title and the first field.
/// Every dialog body already starts with a spacer line, so anchoring to the
/// top is enough breathing room.
fn render_dialog(pal: &Palette, frame: &mut Frame<'_>, inner: Rect, lines: Vec<Line<'static>>) {
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .style(Style::default().bg(pal.surface)),
        inner,
    );
}
