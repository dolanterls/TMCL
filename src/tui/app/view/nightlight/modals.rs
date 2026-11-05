//! Detail, help and dialog drawing for the `nightlight` preset.
//! Design inspiration: nightlightd (GPL-3.0-or-later).
//! This file is an independent implementation; no source code is copied.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};

use crate::tui::app::view::nightlight::tone::Palette;
use crate::tui::app::view::nightlight::widgets::{card, card_inner};
use crate::tui::app::view::{HELP_KEY_COLUMN, HelpLine, help_scrollbar};
use crate::tui::modal::Modal;
use crate::tui::palette::truncate;

use super::App;

/// Detail popup: the shared detail lines in the nightlight card.
pub(in crate::tui::app) fn detail(app: &App, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
    let rect = crate::tui::chrome::popup_rect(area);
    frame.render_widget(Clear, rect);
    card(frame, rect, &app.i18n.text("common.details"), pal);
    let inner = card_inner(rect);
    if inner.is_empty() {
        return;
    }
    let lines = app.detail_lines();
    // One maximum for the painter and the wheel: it counts wrapped rows at the
    // card's own width, which is one column narrower than the shared popup.
    let max_scroll = app.detail_max_scroll();
    let scroll = app.detail_scroll.min(max_scroll);
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0))
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(pal.muted).bg(pal.surface)),
        inner,
    );
}

/// Keyboard help: current page hints first, then the globals that are not
/// already on that page.
pub(in crate::tui::app) fn help(app: &App, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
    let help = app.help_lines();
    let layout = app.help_layout(area, &help);
    frame.render_widget(Clear, layout.rect);
    card(frame, layout.rect, &app.i18n.text("help.keyboard"), pal);
    if layout.body.is_empty() {
        return;
    }
    let lines: Vec<Line<'static>> = help
        .into_iter()
        .map(|line| match line {
            HelpLine::Header(title) => Line::from(Span::styled(
                title,
                Style::default()
                    .fg(pal.accent)
                    .bg(pal.surface)
                    .add_modifier(Modifier::BOLD),
            )),
            HelpLine::Row { key, action } => {
                let room = usize::from(layout.body.width).saturating_sub(HELP_KEY_COLUMN);
                let mut spans = vec![Span::styled(
                    crate::tui::palette::pad(&key, HELP_KEY_COLUMN),
                    Style::default().fg(pal.accent).bg(pal.surface),
                )];
                spans.push(Span::styled(
                    truncate(&action, room),
                    Style::default().fg(pal.muted).bg(pal.surface),
                ));
                Line::from(spans)
            }
            HelpLine::Blank => Line::from(""),
        })
        .collect();
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((u16::try_from(layout.scroll).unwrap_or(u16::MAX), 0))
            .style(Style::default().bg(pal.surface)),
        layout.body,
    );
    help_scrollbar(
        frame,
        &layout,
        Style::default().fg(pal.faint).bg(pal.surface),
    );
}

/// Dialog body. Clearing first keeps the dimmed page from bleeding through the
/// card's rounded corners.
pub(in crate::tui::app) fn render(
    app: &App,
    frame: &mut Frame<'_>,
    modal: &Modal,
    area: Rect,
    pal: &Palette,
) {
    let rect = modal.area(area);
    frame.render_widget(Clear, rect);
    modal.draw_nightlight(frame, &app.i18n, rect, app.anim_time(), pal, &app.hitmap);
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::core::config::models::UiPreset;
    use crate::tui::app::App;

    fn nightlight_app() -> App {
        let mut app = App::new();
        app.config.state.ui.preset = UiPreset::Nightlight;
        app
    }

    #[test]
    fn every_dialog_kind_draws_without_panicking() {
        use crate::tui::modal::{ConfirmAction, FormAction, Modal};

        let app = nightlight_app();
        let dialogs = vec![
            Modal::confirm("Delete it?", ConfirmAction::RemoveAccount),
            Modal::form(
                "Create",
                &["Name", "Version"],
                &[String::new(), String::new()],
                FormAction::CreateInstance,
            ),
            Modal::prompt(
                "Search",
                "Query",
                "Type to filter",
                "",
                FormAction::SearchLogs,
            ),
        ];
        for modal in dialogs {
            let backend = TestBackend::new(120, 40);
            let mut terminal = Terminal::new(backend).expect("terminal");
            let pal = app.nightlight_palette();
            terminal
                .draw(|frame| render(&app, frame, &modal, frame.area(), &pal))
                .expect("draw dialog");
            let buffer = terminal.backend().buffer();
            let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
            assert!(
                text.chars().any(|character| !character.is_whitespace()),
                "dialog drew an empty frame"
            );
        }
    }
    /// A dialog whose buttons are painted but never registered can only be used
    /// from the keyboard. Every confirm dialog must expose both buttons as
    /// clickable, and a checklist must expose its rows.
    #[test]
    fn dialogs_register_their_buttons() {
        use crate::tui::action::{Action, ModalAct};
        use crate::tui::modal::ConfirmAction;

        let app = nightlight_app();
        let pal = app.nightlight_palette();
        let mut shapes = Vec::new();
        let dialogs = vec![
            Modal::confirm("Remove it?", ConfirmAction::RemoveAccount),
            Modal::checklist(
                "Filter",
                vec![
                    crate::tui::modal::ChecklistItem::new("a", "Alpha"),
                    crate::tui::modal::ChecklistItem::new("b", "Beta"),
                ],
                &["a".to_owned()],
                false,
            ),
        ];
        for modal in dialogs {
            // The map ignores regions until a frame tells it how big the screen
            // is; `begin` is what a real draw would call first.
            app.hitmap.begin((120, 40), 1);
            let backend = TestBackend::new(120, 40);
            let mut terminal = Terminal::new(backend).expect("terminal");
            terminal
                .draw(|frame| render(&app, frame, &modal, frame.area(), &pal))
                .expect("draw dialog");
            for hitbox in app.hitmap.hitboxes() {
                if let Action::Modal(act) = hitbox.action {
                    shapes.push(act);
                }
            }
        }
        assert!(
            shapes.iter().any(|act| matches!(act, ModalAct::Confirm)),
            "the confirm button is not clickable"
        );
        assert!(
            shapes.iter().any(|act| matches!(act, ModalAct::Reject)),
            "the cancel button is not clickable"
        );
        assert!(
            shapes
                .iter()
                .any(|act| matches!(act, ModalAct::CheckItem(_))),
            "checklist rows are not clickable"
        );
        assert!(
            shapes
                .iter()
                .any(|act| matches!(act, ModalAct::ApplyChecks)),
            "the checklist apply button is not clickable"
        );
    }
}
