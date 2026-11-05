//! The `shelf` dialog: a slab of `SURFACE` laid on the page.
//!
//! Rounded border in `FAINT`, a title in the accent colour, the body on the
//! surface and the key row at the foot. The geometry is `Modal::area`, shared
//! with every other preset, so a click registered on a key cap lands on the cap
//! that was printed.

use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, BorderType, Clear};

use super::*;
use crate::tui::app::view::shelf::tone::Palette;
use crate::tui::palette;

/// Text on the slab. Every span paints its own background, otherwise the
/// scrim's dimmed page shows through between glyphs.
fn ink(pal: &Palette, color: ratatui::style::Color) -> Style {
    Style::default().fg(color).bg(pal.surface)
}

/// One row of `key label` pairs in the slab's colours, plus the column of every
/// cap so the caller can register a click target exactly where it is printed.
fn shelf_key_row(
    i18n: &I18n,
    pal: &Palette,
    keys: &[(&str, &str)],
    danger_first: bool,
) -> (Line<'static>, Vec<(u16, u16)>) {
    let mut spans = Vec::new();
    let mut caps = Vec::new();
    let mut x: u16 = 0;
    for (index, (key, label)) in keys.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("   ", ink(pal, pal.surface)));
            x = x.saturating_add(3);
        }
        let cap = format!(" {key} ");
        let width = u16::try_from(palette::width(&cap)).unwrap_or(u16::MAX);
        caps.push((x, width));
        let mut style = ink(pal, pal.accent).add_modifier(Modifier::BOLD);
        if index == 0 && danger_first {
            // Destructive keys carry an underline as well as the colour: colour
            // alone is not a signal for a reader who cannot see it.
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        spans.push(Span::styled(cap, style));
        x = x.saturating_add(width);
        let text = format!(" {}", i18n.text(label));
        x = x.saturating_add(u16::try_from(palette::width(&text)).unwrap_or(0));
        spans.push(Span::styled(text, ink(pal, pal.dim)));
    }
    (Line::from(spans), caps)
}

/// Pushes the action row to the foot of the slab, the way a form puts its
/// signature line on the bottom rule.
fn pad_to_bottom(lines: &mut Vec<Line<'static>>, body: Rect, reserved: usize) {
    let target = usize::from(body.height).saturating_sub(reserved);
    while lines.len() < target {
        lines.push(Line::default());
    }
}

impl Modal {
    /// Paints the slab and its title, and returns the body.
    fn shelf_shell(&self, frame: &mut Frame<'_>, i18n: &I18n, pal: &Palette, title: &str) -> Rect {
        let area = self.area(frame.area());
        frame.render_widget(Clear, area);
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(pal.faint).bg(pal.surface))
            .title(Line::from(Span::styled(
                format!(
                    " {} ",
                    palette::truncate(title, usize::from(area.width.saturating_sub(6)))
                ),
                Style::default()
                    .fg(pal.accent)
                    .bg(pal.surface)
                    .add_modifier(Modifier::BOLD),
            )))
            .style(Style::default().bg(pal.surface));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let _ = i18n;
        inner
    }

    pub(crate) fn draw_shelf(
        &self,
        frame: &mut Frame<'_>,
        i18n: &I18n,
        pal: &Palette,
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
                    self.shelf_picker(
                        frame,
                        i18n,
                        pal,
                        field,
                        labels,
                        selection_options,
                        selection_states,
                        *selection_selected,
                        *selection_scroll,
                        version_picker.as_deref(),
                        t,
                        hits,
                    );
                    return;
                }
                let body = self.shelf_shell(frame, i18n, pal, &i18n.text_or_raw(title));
                if body.is_empty() {
                    return;
                }
                let label_width = labels
                    .iter()
                    .map(|label| palette::width(&i18n.text_or_raw(label)))
                    .max()
                    .unwrap_or(0);
                let mut lines: Vec<Line<'static>> = Vec::new();
                for (index, (label, value)) in labels.iter().zip(values).enumerate() {
                    let selected = index == *focused;
                    let marker = if selected { "▌ " } else { "  " };
                    let budget = usize::from(body.width).saturating_sub(label_width + 4);
                    let mut spans = vec![
                        Span::styled(
                            marker,
                            ink(pal, if selected { pal.accent } else { pal.surface }),
                        ),
                        Span::styled(
                            format!("{}: ", palette::pad(&i18n.text_or_raw(label), label_width)),
                            ink(pal, pal.dim),
                        ),
                    ];
                    if selection_options.contains_key(&index) {
                        let display = selection_options
                            .get(&index)
                            .and_then(|options| options.iter().find(|option| *option == value))
                            .map_or(value.as_str(), |option| super::selection_display(option));
                        let mut style = ink(pal, if selected { pal.text } else { pal.dim });
                        style = style.add_modifier(Modifier::UNDERLINED);
                        if selected {
                            style = style.add_modifier(Modifier::BOLD);
                        }
                        spans.push(Span::styled(palette::truncate(display, budget), style));
                    } else {
                        let cursor = cursors
                            .get(index)
                            .copied()
                            .unwrap_or_else(|| value.chars().count());
                        let style = crate::tui::text::FieldStyle {
                            text: pal.text,
                            dim: pal.dim,
                            bg: pal.surface,
                            cursor_fg: pal.surface,
                            cursor_bg: pal.accent,
                        };
                        let mut rendered = crate::tui::text::render(
                            value,
                            cursor,
                            u16::try_from(budget).unwrap_or(u16::MAX),
                            selected,
                            crate::tui::text::blink_on(t),
                            style,
                            "",
                        )
                        .spans;
                        // Only the value carries the rule; underlining the label
                        // too would make every row look editable.
                        for span in &mut rendered {
                            let mut extra = Modifier::UNDERLINED;
                            if selected {
                                extra |= Modifier::BOLD;
                            }
                            span.style = span.style.add_modifier(extra);
                        }
                        spans.extend(rendered);
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
                            body.x,
                            body.y
                                .saturating_add(u16::try_from(index).unwrap_or(u16::MAX)),
                            body.width,
                            1,
                        ),
                        Action::Modal(act),
                    );
                }
                lines.push(Line::default());
                lines.push(Line::default());

                let mut keys: Vec<(&str, &str)> = Vec::new();
                let mut acts: Vec<Option<ModalAct>> = Vec::new();
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
                pad_to_bottom(&mut lines, body, 1);
                let (row, caps) = shelf_key_row(i18n, pal, &keys, false);
                let key_line = lines.len();
                lines.push(row);
                super::register_caps(hits, body, key_line, &caps, &acts);
                frame.render_widget(
                    Paragraph::new(lines).style(Style::default().bg(pal.surface)),
                    body,
                );
            }
            Self::Confirm { prompt, .. } => {
                let body = self.shelf_shell(frame, i18n, pal, &i18n.text("modal.confirm"));
                if body.is_empty() {
                    return;
                }
                // Two rows for the question, wrapped: the name of the thing
                // about to be deleted is often longer than the slab.
                let prompt_rows = body.height.min(2);
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        prompt.as_str(),
                        ink(pal, pal.text).add_modifier(Modifier::BOLD),
                    )))
                    .wrap(Wrap { trim: false }),
                    Rect::new(body.x, body.y, body.width, prompt_rows),
                );
                let key_line = usize::from(body.height).saturating_sub(1);
                let (row, caps) = shelf_key_row(
                    i18n,
                    pal,
                    &[("Enter", "hint.confirm"), ("Esc", "hint.cancel")],
                    true,
                );
                frame.render_widget(
                    Paragraph::new(row),
                    Rect::new(
                        body.x,
                        body.y
                            .saturating_add(u16::try_from(key_line).unwrap_or(u16::MAX)),
                        body.width,
                        1,
                    ),
                );
                super::register_caps(
                    hits,
                    body,
                    key_line,
                    &caps,
                    &[Some(ModalAct::Confirm), Some(ModalAct::Reject)],
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
                let body = self.shelf_shell(frame, i18n, pal, &i18n.text_or_raw(title));
                if body.is_empty() {
                    return;
                }
                let label_text = format!("{}: ", i18n.text_or_raw(label));
                let field_width = body
                    .width
                    .saturating_sub(u16::try_from(palette::width(&label_text)).unwrap_or(u16::MAX));
                let field = Rect::new(body.x, body.y, body.width, 1);
                hits.add(field, Action::Modal(ModalAct::Focus(0)));
                let style = crate::tui::text::FieldStyle {
                    text: pal.text,
                    dim: pal.dim,
                    bg: pal.surface,
                    cursor_fg: pal.surface,
                    cursor_bg: pal.accent,
                };
                let mut spans = vec![Span::styled(label_text, ink(pal, pal.dim))];
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
                    Paragraph::new(Line::from(spans))
                        .style(Style::default().add_modifier(Modifier::UNDERLINED)),
                    field,
                );
                // The key row goes on the slab's last row, always: a prompt is
                // six rows tall, so its body is four, and the old `> 4` guard
                // meant the one dialog that is nothing but a field and a key
                // had no key printed at all. The help line yields to the keys
                // when the body shrinks to three.
                let key_row = body.height.saturating_sub(1);
                if body.height > 2 && key_row > 2 {
                    frame.render_widget(
                        Paragraph::new(Line::from(Span::styled(
                            palette::truncate(help, usize::from(body.width)),
                            ink(pal, pal.faint),
                        ))),
                        Rect::new(body.x, body.y.saturating_add(2), body.width, 1),
                    );
                }
                if body.height >= 2 {
                    let (row, caps) = shelf_key_row(
                        i18n,
                        pal,
                        &[("Enter", "hint.submit"), ("Esc", "hint.cancel")],
                        false,
                    );
                    frame.render_widget(
                        Paragraph::new(row),
                        Rect::new(body.x, body.y.saturating_add(key_row), body.width, 1),
                    );
                    super::register_caps(
                        hits,
                        body,
                        usize::from(key_row),
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
                ..
            } => {
                let body = self.shelf_shell(frame, i18n, pal, &i18n.text_or_raw(title));
                if body.is_empty() {
                    return;
                }
                let start = (*scroll).min(items.len());
                let end = start
                    .saturating_add(super::CHECKLIST_VISIBLE_ROWS)
                    .min(items.len());
                let mut lines: Vec<Line<'static>> = Vec::new();
                if let Some(note) = note {
                    lines.push(Line::from(Span::styled(
                        palette::truncate(note, usize::from(body.width)),
                        ink(pal, pal.faint),
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
                    let selected = index == *focused;
                    if selected {
                        frame.render_widget(
                            Paragraph::new(Line::from(Span::styled("▌", ink(pal, pal.accent)))),
                            Rect::new(
                                body.x.saturating_sub(1),
                                body.y
                                    .saturating_add(u16::try_from(row).unwrap_or(u16::MAX)),
                                1,
                                1,
                            ),
                        );
                    }
                    let style = if selected {
                        ink(pal, pal.text).add_modifier(Modifier::BOLD)
                    } else {
                        ink(pal, pal.dim)
                    };
                    lines.push(Line::from(Span::styled(
                        format!(
                            "{marker}{}",
                            palette::truncate(
                                &i18n.text_or_raw(&item.label),
                                usize::from(body.width.saturating_sub(4))
                            )
                        ),
                        style,
                    )));
                    hits.add(
                        Rect::new(
                            body.x,
                            body.y
                                .saturating_add(u16::try_from(row).unwrap_or(u16::MAX)),
                            body.width,
                            1,
                        ),
                        Action::Modal(ModalAct::CheckItem(index)),
                    );
                }
                pad_to_bottom(&mut lines, body, 1);
                let (row, caps) = shelf_key_row(
                    i18n,
                    pal,
                    &[
                        ("Space", "hint.toggle"),
                        ("Enter", "hint.apply"),
                        ("R", "hint.reset"),
                        ("Esc", "hint.cancel"),
                    ],
                    false,
                );
                let key_line = lines.len();
                lines.push(row);
                frame.render_widget(
                    Paragraph::new(lines).style(Style::default().bg(pal.surface)),
                    body,
                );
                super::register_caps(
                    hits,
                    body,
                    key_line,
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

    /// The option list, drawn on the same slab as the form it belongs to.
    #[allow(clippy::too_many_arguments)]
    fn shelf_picker(
        &self,
        frame: &mut Frame<'_>,
        i18n: &I18n,
        pal: &Palette,
        field: usize,
        labels: &[String],
        selection_options: &HashMap<usize, Vec<String>>,
        selection_states: &HashMap<usize, SelectionState>,
        selection_selected: usize,
        selection_scroll: usize,
        version_picker: Option<&VersionPickerState>,
        t: f64,
        hits: &Hitmap,
    ) {
        let options = super::selection_picker_options(field, selection_options, version_picker);
        let option_label = labels
            .get(field)
            .map(|label| i18n.text_or_raw(label))
            .unwrap_or_else(|| i18n.text("modal.options"));
        let title = format!("{} {option_label}", i18n.text("modal.select"));
        let body = self.shelf_shell(frame, i18n, pal, &title);
        if body.is_empty() {
            return;
        }
        let mut lines: Vec<Line<'static>> = Vec::new();

        lines.push(Line::from(Span::styled(
            if options.is_empty() {
                option_label.clone()
            } else {
                format!(
                    "{option_label} ({}/{})",
                    selection_selected.saturating_add(1),
                    options.len()
                )
            },
            ink(pal, pal.faint),
        )));

        if let Some(picker) = version_picker.filter(|picker| picker.field == field) {
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
                let y = body
                    .y
                    .saturating_add(u16::try_from(line).unwrap_or(u16::MAX));
                let left_width = palette::width(&left);
                hits.add(
                    Rect::new(body.x, y, u16::try_from(left_width).unwrap_or(u16::MAX), 1),
                    Action::Modal(ModalAct::ToggleVersionFilter(first.index())),
                );
                let right_x = body
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
                lines.push(Line::from(Span::styled(
                    palette::truncate(&format!("{left}    {right}"), usize::from(body.width)),
                    ink(pal, pal.dim),
                )));
            }
            let search_line = lines.len();
            hits.add(
                Rect::new(
                    body.x,
                    body.y
                        .saturating_add(u16::try_from(search_line).unwrap_or(u16::MAX)),
                    body.width,
                    1,
                ),
                Action::Modal(ModalAct::EditSearch),
            );
            let label = format!("{} (/): ", i18n.text("common.search"));
            let budget = body
                .width
                .saturating_sub(u16::try_from(palette::width(&label)).unwrap_or(u16::MAX));
            let mut spans = vec![Span::styled(label, ink(pal, pal.dim))];
            if picker.editing_search {
                let style = crate::tui::text::FieldStyle {
                    text: pal.text,
                    dim: pal.dim,
                    bg: pal.surface,
                    cursor_fg: pal.surface,
                    cursor_bg: pal.accent,
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
                    ink(pal, pal.dim),
                ));
            }
            lines.push(Line::from(spans));
        }

        // Rows the window can hold: the slab is `Modal::area`, whose body loses
        // one row to the border top and bottom. Promising more options than fit
        // pushed the last of them — and the selection bar with them — past the
        // bottom border.
        let reserved = lines.len().saturating_add(3); // help line, ↑ more, ↓ more
        let visible = usize::from(body.height)
            .saturating_sub(reserved)
            .max(1)
            .min(options.len().max(1));
        let mut start = selection_scroll.min(options.len().saturating_sub(visible));
        if selection_selected < start {
            start = selection_selected;
        } else if selection_selected >= start.saturating_add(visible) {
            start = selection_selected.saturating_add(1).saturating_sub(visible);
        }
        let end = start.saturating_add(visible).min(options.len());

        if options.is_empty() {
            let has_loaded_versions = version_picker
                .is_some_and(|picker| picker.field == field && !picker.versions.is_empty());
            let (message, error) = match selection_states.get(&field) {
                Some(SelectionState::Loading) => (i18n.text("modal.loading_options"), false),
                Some(SelectionState::Error(error)) => {
                    (i18n.format("modal.load_failed", &[("error", error)]), true)
                }
                Some(SelectionState::Unavailable(message)) => (message.clone(), false),
                None if has_loaded_versions => (i18n.text("modal.no_versions_match"), false),
                None => (i18n.text("modal.no_options"), false),
            };
            lines.push(Line::from(Span::styled(
                palette::truncate(&message, usize::from(body.width)),
                ink(pal, if error { pal.error } else { pal.faint }),
            )));
        }
        if start > 0 {
            lines.push(Line::from(Span::styled(
                i18n.text("modal.more_up"),
                ink(pal, pal.faint),
            )));
        }
        let mut option_rows = Vec::new();
        for (index, option) in options.iter().enumerate().take(end).skip(start) {
            let line = lines.len();
            let selected = index == selection_selected;
            let marker = if selected { "▌ " } else { "  " };
            let style = if selected {
                ink(pal, pal.text).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                ink(pal, pal.dim)
            };
            option_rows.push((line, index));
            lines.push(Line::from(vec![
                Span::styled(marker, ink(pal, pal.accent)),
                Span::styled(
                    palette::truncate(
                        super::selection_display(option),
                        usize::from(body.width.saturating_sub(2)),
                    ),
                    style,
                ),
            ]));
        }
        if end < options.len() {
            lines.push(Line::from(Span::styled(
                i18n.text("modal.more_down"),
                ink(pal, pal.faint),
            )));
        }
        pad_to_bottom(&mut lines, body, 1);
        let picker_help = version_picker.filter(|picker| picker.field == field);
        lines.push(Line::from(Span::styled(
            match (picker_help, selection_states.get(&field)) {
                (Some(picker), Some(SelectionState::Error(_))) if picker.editing_search => {
                    i18n.text("modal.search_help")
                }
                (Some(_), Some(SelectionState::Error(_))) => i18n.text("modal.retry_filters_help"),
                (_, Some(SelectionState::Error(_))) => i18n.text("modal.retry_help"),
                (_, Some(SelectionState::Loading | SelectionState::Unavailable(_))) => {
                    i18n.text("modal.back_help")
                }
                (Some(picker), _) if picker.editing_search => i18n.text("modal.search_help"),
                (Some(_), _) => i18n.text("modal.filter_help"),
                (None, _) if options.is_empty() => i18n.text("modal.load_retry_help"),
                (None, _) => i18n.text("modal.choose_help"),
            },
            ink(pal, pal.faint),
        )));
        frame.render_widget(
            Paragraph::new(lines).style(Style::default().bg(pal.surface)),
            body,
        );
        for (line, index) in &option_rows {
            hits.add(
                Rect::new(
                    body.x,
                    body.y
                        .saturating_add(u16::try_from(*line).unwrap_or(u16::MAX)),
                    body.width,
                    1,
                ),
                Action::Modal(ModalAct::Option(*index)),
            );
        }
        if let (Some((first, _)), Some((last, _))) = (option_rows.first(), option_rows.last()) {
            let top = body.y.saturating_add(u16::try_from(*first).unwrap_or(0));
            let bottom = body.y.saturating_add(u16::try_from(*last).unwrap_or(0));
            hits.scroll(
                Rect::new(body.x, top, body.width, bottom.saturating_sub(top) + 1),
                ScrollTarget::ModalOptions,
            );
        }
    }
}
