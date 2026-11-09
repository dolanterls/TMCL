//! Modal rendering.
//!
//! Every interactive row and key chip is registered in the frame's hit map at
//! the same coordinates it is drawn at, so a click resolves to what the user
//! sees. Text editing stays in `Modal::handle_key`; this file only decides what
//! is visible and which rectangles are live.

use super::*;
use crate::tui::palette;
use ratatui::style::Style;
use ratatui::text::Span;

/// One row of `[key] label` chips plus the rectangle of every key cap.
///
/// Labels arrive as i18n keys (`hint.submit`), not as display text: the caller
/// does not know the language, and printing the key was exactly the bug this
/// signature prevents.
pub(super) fn key_row(
    i18n: &I18n,
    theme: Theme,
    keys: &[(&str, &str)],
) -> (Line<'static>, Vec<(u16, u16)>) {
    let mut spans = Vec::new();
    let mut caps = Vec::new();
    let mut x: u16 = 0;
    for (index, (key, label)) in keys.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("  "));
            x = x.saturating_add(2);
        }
        let cap = format!(" {key} ");
        let width = u16::try_from(palette::width(&cap)).unwrap_or(u16::MAX);
        caps.push((x, width));
        spans.push(Span::styled(cap, theme.key_style()));
        x = x.saturating_add(width);
        let text = format!(" {}", i18n.text(label));
        x = x.saturating_add(u16::try_from(palette::width(&text)).unwrap_or(0));
        spans.push(Span::styled(text, theme.dim()));
    }
    (Line::from(spans), caps)
}

impl Modal {
    pub fn draw(&self, frame: &mut Frame<'_>, theme: Theme, i18n: &I18n, t: f64, hits: &Hitmap) {
        let area = self.area(frame.area());
        frame.render_widget(Clear, area);
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
                    let block = modal_widgets::panel(
                        format!("{} {option_label}", i18n.text("modal.select")),
                        theme,
                    );
                    let inner = block.inner(area);
                    frame.render_widget(block, area);
                    if inner.is_empty() {
                        return;
                    }
                    let mut lines = vec![Line::from(format!(
                        "{}{}",
                        option_label,
                        if options.is_empty() {
                            String::new()
                        } else {
                            format!(
                                " ({}/{})",
                                selection_selected.saturating_add(1),
                                options.len()
                            )
                        }
                    ))];
                    if let Some(picker) = version_picker
                        .as_ref()
                        .filter(|picker| picker.field == field)
                    {
                        let counts = picker.counts();
                        for (first, second) in [
                            (VersionCategory::Release, VersionCategory::Snapshot),
                            (VersionCategory::Legacy, VersionCategory::AprilFools),
                        ] {
                            let segment = |category: VersionCategory| {
                                format!(
                                    "{}:[{}] {} ({})",
                                    category.index() + 1,
                                    if picker.filter.is_enabled(category) {
                                        'x'
                                    } else {
                                        ' '
                                    },
                                    i18n.text(category.label_key()),
                                    counts[category.index()]
                                )
                            };
                            let left = segment(first);
                            let right = segment(second);
                            let line = lines.len();
                            let y = inner
                                .y
                                .saturating_add(u16::try_from(line).unwrap_or(u16::MAX));
                            let left_width = palette::width(&left);
                            hits.add(
                                Rect::new(
                                    inner.x,
                                    y,
                                    u16::try_from(left_width).unwrap_or(u16::MAX),
                                    1,
                                ),
                                Action::Modal(ModalAct::ToggleVersionFilter(first.index())),
                            );
                            let right_x = inner
                                .x
                                .saturating_add(u16::try_from(left_width + 4).unwrap_or(u16::MAX));
                            hits.add(
                                Rect::new(
                                    right_x,
                                    y,
                                    u16::try_from(palette::width(&right)).unwrap_or(u16::MAX),
                                    1,
                                ),
                                Action::Modal(ModalAct::ToggleVersionFilter(second.index())),
                            );
                            // Truncate so a narrow dialog never wraps the chip row
                            // and pushes the option list down.
                            lines.push(Line::from(palette::truncate(
                                &format!("{left}    {right}"),
                                usize::from(inner.width),
                            )));
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
                        let label = format!("{} (/): ", i18n.text("common.search"));
                        let budget = inner.width.saturating_sub(
                            u16::try_from(palette::width(&label)).unwrap_or(u16::MAX),
                        );
                        let mut spans = vec![Span::styled(label, theme.label_style())];
                        if picker.editing_search {
                            let style = crate::tui::text::FieldStyle {
                                text: theme.fg,
                                dim: theme.dim,
                                bg: theme.bg,
                                cursor_fg: ratatui::style::Color::Black,
                                cursor_bg: theme.accent,
                            };
                            spans.extend(
                                crate::tui::text::render(
                                    picker.filter.search(),
                                    picker.search_cursor,
                                    budget,
                                    true,
                                    crate::tui::text::blink_on(t),
                                    style,
                                    "",
                                )
                                .spans,
                            );
                        } else {
                            spans.push(Span::styled(
                                picker.filter.search().to_owned(),
                                theme.base(),
                            ));
                        }
                        lines.push(Line::from(spans));
                    }
                    if options.is_empty() {
                        let has_loaded_versions = version_picker.as_ref().is_some_and(|picker| {
                            picker.field == field && !picker.versions.is_empty()
                        });
                        let message = match selection_states.get(&field) {
                            Some(SelectionState::Loading) => i18n.text("modal.loading_options"),
                            Some(SelectionState::Error(error)) => {
                                i18n.format("modal.load_failed", &[("error", error)])
                            }
                            Some(SelectionState::Unavailable(message)) => message.clone(),
                            None if has_loaded_versions => i18n.text("modal.no_versions_match"),
                            None => i18n.text("modal.no_options"),
                        };
                        lines.push(Line::from(message));
                    }
                    if start > 0 {
                        lines.push(Line::from(i18n.text("modal.more_up")));
                    }
                    let mut option_rows = Vec::new();
                    for (index, option) in options.iter().enumerate().take(end).skip(start) {
                        let marker = if index == *selection_selected {
                            "> "
                        } else {
                            "  "
                        };
                        option_rows.push((lines.len(), index));
                        // Truncated rather than wrapped: a long option that
                        // wrapped would push every row below it down, and the
                        // key row at the foot would be printed on one row and
                        // hit-tested on another.
                        lines.push(Line::from(palette::truncate(
                            &format!("{marker}{}", selection_display(option)),
                            usize::from(inner.width),
                        )));
                    }
                    if end < options.len() {
                        lines.push(Line::from(i18n.text("modal.more_down")));
                    }
                    lines.push(Line::from(""));
                    let picker_help = version_picker
                        .as_ref()
                        .filter(|picker| picker.field == field)
                        .map(|picker| &**picker);
                    let (keys, acts) = picker_key_row(&picker_key_hints(
                        picker_help,
                        selection_states.get(&field),
                        *selection_selected,
                        options.is_empty(),
                    ));
                    let key_line = lines.len();
                    let (row, caps) = key_row(i18n, theme, &keys);
                    lines.push(row);
                    frame.render_widget(
                        Paragraph::new(lines)
                            .wrap(Wrap { trim: false })
                            .style(theme.base()),
                        inner,
                    );
                    super::register_caps(hits, inner, key_line, &caps, &acts);
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

                let block = modal_widgets::panel(i18n.text_or_raw(title), theme);
                let inner = block.inner(area);
                frame.render_widget(block, area);
                if inner.is_empty() {
                    return;
                }
                let label_width = labels
                    .iter()
                    .map(|label| palette::width(&i18n.text_or_raw(label)))
                    .max()
                    .unwrap_or(0);
                let mut lines = Vec::new();
                for (index, (label, value)) in labels.iter().zip(values).enumerate() {
                    let selected = index == *focused;
                    let marker = if selected { "> " } else { "  " };
                    let budget = usize::from(inner.width).saturating_sub(label_width + 4);
                    let mut spans = vec![
                        Span::styled(
                            marker,
                            if selected {
                                theme.accent_style()
                            } else {
                                theme.dim()
                            },
                        ),
                        Span::styled(
                            format!("{}: ", palette::pad(&i18n.text_or_raw(label), label_width)),
                            theme.label_style(),
                        ),
                    ];
                    if selection_options.contains_key(&index) {
                        let display = selection_options
                            .get(&index)
                            .and_then(|options| options.iter().find(|option| *option == value))
                            .map_or(value.as_str(), |option| selection_display(option));
                        spans.push(Span::styled(
                            palette::truncate(display, budget),
                            if selected {
                                theme.accent_style()
                            } else {
                                theme.base()
                            },
                        ));
                    } else {
                        // Same editor and renderer as every other field.
                        let style = crate::tui::text::FieldStyle {
                            text: theme.fg,
                            dim: theme.dim,
                            bg: theme.bg,
                            cursor_fg: ratatui::style::Color::Black,
                            cursor_bg: theme.accent,
                        };
                        let cursor = cursors
                            .get(index)
                            .copied()
                            .unwrap_or_else(|| value.chars().count());
                        spans.extend(
                            crate::tui::text::render(
                                value,
                                cursor,
                                u16::try_from(budget).unwrap_or(u16::MAX),
                                selected,
                                crate::tui::text::blink_on(t),
                                style,
                                "",
                            )
                            .spans,
                        );
                    }
                    lines.push(Line::from(spans));
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
                                .saturating_add(u16::try_from(index).unwrap_or(u16::MAX)),
                            inner.width,
                            1,
                        ),
                        Action::Modal(act),
                    );
                }
                lines.push(Line::from(""));
                // No help sentence here: it only listed the keys the clickable
                // row below already shows, and the footer repeated them a third
                // time.
                lines.push(Line::from(""));
                let mut keys: Vec<(&str, &str)> = Vec::new();
                let mut acts: Vec<Option<ModalAct>> = Vec::new();
                // The dialog owns its key row, so it has to carry `Tab`, and
                // clicking the cap moves focus like the key does.
                if values.len() > 1 {
                    let next = (*focused + 1) % values.len();
                    keys.push(("Tab", "hint.next_field"));
                    acts.push(Some(ModalAct::Focus(next)));
                }
                if directory_field == &Some(*focused) {
                    keys.push(("F2", "hint.browse"));
                    acts.push(Some(ModalAct::Browse(*focused)));
                } else if file_field == &Some(*focused) {
                    keys.push(("F2", "hint.browse"));
                    acts.push(Some(ModalAct::BrowseFile(*focused)));
                }
                keys.push(("F5", "hint.submit"));
                acts.push(Some(ModalAct::Submit));
                keys.push(("Esc", "hint.cancel"));
                acts.push(Some(ModalAct::Cancel));
                let (row, caps) = key_row(i18n, theme, &keys);
                let key_line = lines.len();
                lines.push(row);
                super::register_caps(hits, inner, key_line, &caps, &acts);
                frame.render_widget(
                    Paragraph::new(lines)
                        .wrap(Wrap { trim: false })
                        .style(theme.base()),
                    inner,
                );
            }
            Self::Confirm { prompt, .. } => {
                let block = modal_widgets::panel(i18n.text("modal.confirm"), theme);
                let inner = block.inner(area);
                frame.render_widget(block, area);
                if inner.is_empty() {
                    return;
                }
                // Wrapped, not truncated: a cut prompt hides the path or the
                // name it is asking about. The card height follows the wrap
                // (`Modal::area`), so the tail is never clipped either.
                let width = usize::from(inner.width).max(1);
                let mut lines = palette::wrap(prompt, width)
                    .into_iter()
                    .map(Line::from)
                    .collect::<Vec<_>>();
                lines.push(Line::from(""));
                let (row, caps) = key_row(
                    i18n,
                    theme,
                    &[("Enter", "hint.confirm"), ("Esc", "hint.cancel")],
                );
                let key_line = lines.len();
                lines.push(row);
                super::register_caps(
                    hits,
                    inner,
                    key_line,
                    &caps,
                    &[Some(ModalAct::Confirm), Some(ModalAct::Reject)],
                );
                frame.render_widget(
                    Paragraph::new(lines)
                        .wrap(Wrap { trim: false })
                        .style(modal_widgets::danger(theme)),
                    inner,
                );
            }
            Self::Prompt {
                title,
                label,
                help,
                value,
                cursor,
                ..
            } => {
                let block = modal_widgets::panel(i18n.text_or_raw(title), theme);
                let inner = block.inner(area);
                frame.render_widget(block, area);
                if inner.is_empty() {
                    return;
                }

                // The input row carries the field fill, so the dialog reads as a
                // form with a text box rather than as bare text.
                let field = Rect {
                    x: inner.x,
                    y: inner.y,
                    width: inner.width,
                    height: 1,
                };
                hits.add(field, Action::Modal(ModalAct::Focus(0)));
                let label_text = format!(" {}: ", i18n.text_or_raw(label));
                let field_width = field
                    .width
                    .saturating_sub(u16::try_from(palette::width(&label_text)).unwrap_or(u16::MAX));
                let style = crate::tui::text::FieldStyle {
                    text: theme.fg,
                    dim: theme.dim,
                    bg: theme.field_bg,
                    cursor_fg: ratatui::style::Color::Black,
                    cursor_bg: theme.accent,
                };
                let mut spans = vec![Span::styled(
                    label_text,
                    Style::default().fg(theme.label).bg(theme.field_bg),
                )];
                spans.extend(
                    crate::tui::text::render(
                        value,
                        *cursor,
                        field_width,
                        true,
                        crate::tui::text::blink_on(t),
                        style,
                        "",
                    )
                    .spans,
                );
                frame.render_widget(
                    Paragraph::new(Line::from(spans)).style(theme.field_style()),
                    field,
                );

                if inner.height > 2 {
                    frame.render_widget(
                        Paragraph::new(Line::from(Span::styled(help.as_str(), theme.dim()))),
                        Rect {
                            x: inner.x + 1,
                            y: inner.y + 2,
                            width: inner.width.saturating_sub(2),
                            height: 1,
                        },
                    );
                }
                if inner.height > 3 {
                    let (row, caps) = key_row(
                        i18n,
                        theme,
                        &[("Enter", "hint.submit"), ("Esc", "hint.cancel")],
                    );
                    frame.render_widget(
                        Paragraph::new(row),
                        Rect::new(inner.x, inner.y + 3, inner.width, 1),
                    );
                    super::register_caps(
                        hits,
                        inner,
                        3,
                        &caps,
                        &[Some(ModalAct::Submit), Some(ModalAct::Cancel)],
                    );
                }
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
                let block = modal_widgets::panel(i18n.text_or_raw(title), theme);
                let inner = block.inner(area);
                frame.render_widget(block, area);
                if inner.is_empty() {
                    return;
                }
                let start = (*scroll).min(items.len());
                let end = start
                    .saturating_add(super::CHECKLIST_VISIBLE_ROWS)
                    .min(items.len());
                let mut lines = Vec::new();
                if let Some(note) = note {
                    lines.push(Line::from(Span::styled(note.as_str(), theme.dim())));
                }
                for (index, item) in items.iter().enumerate().take(end).skip(start) {
                    let row = lines.len();
                    let marker = match (*single, checked[index]) {
                        (true, true) => "(•) ",
                        (true, false) => "( ) ",
                        (false, true) => "[x] ",
                        (false, false) => "[ ] ",
                    };
                    let style = if index == *focused {
                        theme.selection_style()
                    } else {
                        theme.base()
                    };
                    lines.push(Line::from(Span::styled(
                        format!("{marker}{}", i18n.text_or_raw(&item.label)),
                        style,
                    )));
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
                lines.push(Line::from(""));
                let (key_line, caps) = key_row(
                    i18n,
                    theme,
                    &[
                        ("Space", "hint.toggle"),
                        ("Enter", "hint.apply"),
                        ("R", "hint.reset"),
                        ("Esc", "hint.cancel"),
                    ],
                );
                let key_line_index = lines.len();
                lines.push(key_line);
                frame.render_widget(
                    Paragraph::new(lines)
                        .wrap(Wrap { trim: false })
                        .style(theme.base()),
                    inner,
                );
                super::register_caps(
                    hits,
                    inner,
                    key_line_index,
                    &caps,
                    &[
                        Some(ModalAct::CheckItem(*focused)),
                        Some(ModalAct::ApplyChecks),
                        Some(ModalAct::ResetChecks),
                        Some(ModalAct::Cancel),
                    ],
                );
            }
        }
    }
}
