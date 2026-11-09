//! The `paper` dialog: a slip laid on the page.
//!
//! No frame: a sheet one step brighter than the page, a title in the accent
//! colour, a hairline under it and another along the bottom edge. The key row
//! sits at the foot of the slip, so the dialog reads as part of the page rather
//! than as a window over it.
//!
//! Geometry lives in [`Modal::paper_area`] / [`Modal::paper_body`] and is used
//! by the painter **and** by the hit map. Classic's `Block::inner` cannot be
//! reused: the slip has no border to inset from, and a hit registered one row
//! off a printed field is worse than no hit at all.

use super::*;
use crate::tui::app::view::paper::PaperPalette;
use crate::tui::palette;
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::Block;

/// Text on the slip. Every paper surface paints its own background, otherwise
/// the scrim's dimmed page shows through between glyphs.
fn ink(pal: &PaperPalette, color: ratatui::style::Color) -> Style {
    Style::default().fg(color).bg(pal.sheet)
}

/// One row of `key label` pairs in the slip's own colours.
///
/// Returns the line and the column of every key cap, so the caller can register
/// a click target exactly where the cap is printed.
fn paper_key_row(
    i18n: &I18n,
    pal: &PaperPalette,
    keys: &[(&str, &str)],
    danger_first: bool,
) -> (Line<'static>, Vec<(u16, u16)>) {
    let mut spans = Vec::new();
    let mut caps = Vec::new();
    let mut x: u16 = 0;
    for (index, (key, label)) in keys.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("   ", ink(pal, pal.sheet)));
            x = x.saturating_add(3);
        }
        let cap = format!(" {key} ");
        let width = u16::try_from(palette::width(&cap)).unwrap_or(u16::MAX);
        caps.push((x, width));
        let mut style = ink(pal, pal.accent).add_modifier(Modifier::BOLD);
        if index == 0 && danger_first {
            // The destructive key carries an underline as well as the accent:
            // colour alone is not a signal for a reader who cannot see it.
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

/// Fill a rectangle with the slip colour.
/// Pushes the action row to the foot of the slip.
///
/// Without it the keys float wherever the content ends and the slip looks
/// half-filled; a printed form puts its signature line on the bottom rule.
fn pad_to_bottom(lines: &mut Vec<Line<'static>>, body: Rect, reserved: usize) {
    let target = usize::from(body.height).saturating_sub(reserved);
    while lines.len() < target {
        lines.push(Line::default());
    }
}

fn paint_slip(frame: &mut Frame<'_>, rect: Rect, pal: &PaperPalette) {
    frame.render_widget(
        Block::default().style(Style::default().fg(pal.ink).bg(pal.sheet)),
        rect,
    );
}

fn hairline(frame: &mut Frame<'_>, rect: Rect, pal: &PaperPalette, color: ratatui::style::Color) {
    crate::tui::app::view::paper::kit::rule_on(frame, rect, color, pal.sheet);
}

impl Modal {
    /// The slip, three rows taller than classic's framed box.
    ///
    /// `area()` sizes the box for a border top and bottom; the slip draws
    /// neither, so those two rows go to the content, plus one for the blank row
    /// that keeps the title off the top edge.
    pub(crate) fn paper_area(&self, terminal_area: Rect) -> Rect {
        let base = self.area(terminal_area);
        let height = base.height.saturating_add(3).min(terminal_area.height);
        let y = terminal_area
            .y
            .saturating_add(terminal_area.height.saturating_sub(height) / 2);
        Rect::new(base.x, y, base.width, height)
    }

    /// Rows between the title rule and the bottom rule, inset two columns.
    ///
    /// The slip reads: blank, title, rule, body, …, rule.
    pub(crate) fn paper_body(slip: Rect) -> Rect {
        Rect::new(
            slip.x.saturating_add(2),
            slip.y.saturating_add(3),
            slip.width.saturating_sub(4),
            slip.height.saturating_sub(4),
        )
    }

    /// Paints the slip and its running head, and returns the body.
    fn paper_shell(&self, frame: &mut Frame<'_>, pal: &PaperPalette, title: &str) -> Rect {
        let slip = self.paper_area(frame.area());
        frame.render_widget(Clear, slip);
        paint_slip(frame, slip, pal);
        if slip.width < 6 || slip.height < 5 {
            return Self::paper_body(slip);
        }
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                palette::truncate(title, usize::from(slip.width.saturating_sub(4))),
                ink(pal, pal.accent).add_modifier(Modifier::BOLD),
            ))),
            Rect::new(
                slip.x.saturating_add(2),
                slip.y.saturating_add(1),
                slip.width.saturating_sub(4),
                1,
            ),
        );
        hairline(
            frame,
            Rect::new(slip.x, slip.y.saturating_add(2), slip.width, 1),
            pal,
            pal.rule,
        );
        hairline(
            frame,
            Rect::new(slip.x, slip.bottom().saturating_sub(1), slip.width, 1),
            pal,
            pal.rule,
        );
        Self::paper_body(slip)
    }

    pub(crate) fn draw_paper(
        &self,
        frame: &mut Frame<'_>,
        pal: &PaperPalette,
        i18n: &I18n,
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
                    self.paper_picker(
                        frame,
                        pal,
                        i18n,
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
                let body = self.paper_shell(frame, pal, &i18n.text_or_raw(title));
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
                            ink(pal, if selected { pal.accent } else { pal.sheet }),
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
                        let mut style = ink(pal, if selected { pal.ink } else { pal.soft });
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
                            text: pal.ink,
                            dim: pal.dim,
                            bg: pal.sheet,
                            cursor_fg: pal.sheet,
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
                        // Only the value carries the rule; underlining the
                        // label too would make every row look editable.
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
                let (row, caps) = paper_key_row(i18n, pal, &keys, false);
                let key_line = lines.len();
                lines.push(row);
                super::register_caps(hits, body, key_line, &caps, &acts);
                frame.render_widget(Paragraph::new(lines), body);
            }
            Self::Confirm { prompt, .. } => {
                let body = self.paper_shell(frame, pal, &i18n.text("modal.confirm"));
                if body.is_empty() {
                    return;
                }
                // Every row above the key line is available: `Modal::area`
                // sized the card for the wrapped prompt, and a two-row cap
                // clipped anything longer. The key row keeps its own row at the
                // foot, so a long prompt can never push it out.
                let prompt_rows = body.height.saturating_sub(1).max(1);
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        prompt.as_str(),
                        ink(pal, pal.ink).add_modifier(Modifier::BOLD),
                    )))
                    .wrap(Wrap { trim: false }),
                    Rect::new(body.x, body.y, body.width, prompt_rows),
                );
                let key_line = usize::from(body.height).saturating_sub(1);
                let (row, caps) = paper_key_row(
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
                let body = self.paper_shell(frame, pal, &i18n.text_or_raw(title));
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
                    text: pal.ink,
                    dim: pal.dim,
                    bg: pal.sheet,
                    cursor_fg: pal.sheet,
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
                if body.height > 2 {
                    frame.render_widget(
                        Paragraph::new(Line::from(Span::styled(
                            palette::truncate(help, usize::from(body.width)),
                            ink(pal, pal.faint),
                        ))),
                        Rect::new(body.x, body.y.saturating_add(2), body.width, 1),
                    );
                }
                if body.height > 4 {
                    let (row, caps) = paper_key_row(
                        i18n,
                        pal,
                        &[("Enter", "hint.submit"), ("Esc", "hint.cancel")],
                        false,
                    );
                    frame.render_widget(
                        Paragraph::new(row),
                        Rect::new(body.x, body.y.saturating_add(4), body.width, 1),
                    );
                    super::register_caps(
                        hits,
                        body,
                        4,
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
                let body = self.paper_shell(frame, pal, &i18n.text_or_raw(title));
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
                        ink(pal, pal.ink).add_modifier(Modifier::BOLD)
                    } else {
                        ink(pal, pal.soft)
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
                let (row, caps) = paper_key_row(
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
                frame.render_widget(Paragraph::new(lines), body);
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

    /// The option list, drawn inside the same slip as the form it belongs to.
    #[allow(clippy::too_many_arguments)]
    fn paper_picker(
        &self,
        frame: &mut Frame<'_>,
        pal: &PaperPalette,
        i18n: &I18n,
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
        let body = self.paper_shell(frame, pal, &title);
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
                    text: pal.ink,
                    dim: pal.dim,
                    bg: pal.sheet,
                    cursor_fg: pal.sheet,
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
                    ink(pal, pal.soft),
                ));
            }
            lines.push(Line::from(spans));
        }

        // Rows the option window can hold. The slip is a fixed sixteen-row box
        // (`Modal::area`) whatever the preset, and paper's picker spends four of
        // its fifteen body rows on the counter, the version filters and the search
        // line. Promising `PICKER_VISIBLE_ROWS` options anyway would push the last
        // of them, and the selection bar with them, past the slip's bottom rule.
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
                ink(pal, if error { pal.accent } else { pal.faint }),
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
            if selected {
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled("▌", ink(pal, pal.accent)))),
                    Rect::new(
                        body.x.saturating_sub(1),
                        body.y
                            .saturating_add(u16::try_from(line).unwrap_or(u16::MAX)),
                        1,
                        1,
                    ),
                );
            }
            option_rows.push((line, index));
            lines.push(Line::from(Span::styled(
                palette::truncate(
                    super::selection_display(option),
                    usize::from(body.width.saturating_sub(2)),
                ),
                if selected {
                    ink(pal, pal.ink).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                } else {
                    ink(pal, pal.soft)
                },
            )));
        }
        if end < options.len() {
            lines.push(Line::from(Span::styled(
                i18n.text("modal.more_down"),
                ink(pal, pal.faint),
            )));
        }
        pad_to_bottom(&mut lines, body, 1);
        let picker_help = version_picker.filter(|picker| picker.field == field);
        let (keys, acts) = super::picker_key_row(&super::picker_key_hints(
            picker_help,
            selection_states.get(&field),
            selection_selected,
            options.is_empty(),
        ));
        let key_line = lines.len();
        let (row, caps) = paper_key_row(i18n, pal, &keys, false);
        lines.push(row);
        frame.render_widget(Paragraph::new(lines), body);
        super::register_caps(hits, body, key_line, &caps, &acts);
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
