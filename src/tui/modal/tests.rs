use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

use super::{ChecklistItem, ConfirmAction, FormAction, Modal, ModalAct, ModalResult};
use crate::core::version::models::VersionSummary;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

#[test]
fn directory_field_opens_picker_and_accepts_selected_path() {
    let initial = PathBuf::from(r"C:\Games\.minecraft");
    let selected = PathBuf::from(r"D:\Portable Minecraft");
    let mut modal = Modal::form(
        "Game directory",
        &["Directory"],
        &[initial.to_string_lossy().into_owned()],
        FormAction::AddGameDirectory,
    )
    .with_directory_picker(0);
    let key = KeyEvent {
        code: KeyCode::F(2),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };

    assert!(matches!(
        modal.handle_key(key),
        ModalResult::BrowseDirectory(path) if path == initial
    ));
    assert!(modal.set_browsed_directory(selected.clone()));
    let Modal::Form { values, .. } = modal else {
        panic!("form modal expected");
    };
    assert_eq!(values[0], selected.to_string_lossy());
}

/// A click on the picker's own caps has to do what the keys do there: `Enter`
/// takes the highlighted option, `Esc` closes the list. Routing both to the
/// dialog's submit and cancel would let the mouse do something else entirely.
#[test]
fn the_picker_caps_choose_and_close_instead_of_submitting_the_form() {
    let mut modal = Modal::form(
        "Pick a loader",
        &["Loader", "Name"],
        &[String::new(), "demo".into()],
        FormAction::CreateInstance,
    )
    .with_selection_picker(
        0,
        vec![
            "vanilla".to_owned(),
            "fabric".to_owned(),
            "quilt".to_owned(),
        ],
    );
    assert!(matches!(
        modal.apply(ModalAct::Toggle(0)),
        ModalResult::Continue
    ));

    // `Esc` on the list backs out of the list, not out of the dialog.
    assert!(matches!(
        modal.apply(ModalAct::CloseOptions),
        ModalResult::Continue
    ));
    let Modal::Form {
        selection_picker_open,
        values,
        ..
    } = &modal
    else {
        panic!("form expected");
    };
    assert!(selection_picker_open.is_none(), "the list has to close");
    assert_eq!(values[0], "", "closing a list must not choose anything");

    // `Enter` takes the highlighted row and leaves the dialog open.
    assert!(matches!(
        modal.apply(ModalAct::Toggle(0)),
        ModalResult::Continue
    ));
    let picked = modal.apply(ModalAct::Option(1));
    assert!(matches!(
        picked,
        ModalResult::SelectionChanged { field: 0, .. }
    ));
    let Modal::Form {
        selection_picker_open,
        focused,
        values,
        ..
    } = &modal
    else {
        panic!("form expected");
    };
    assert_eq!(values[0], "fabric");
    assert!(selection_picker_open.is_none());
    assert_eq!(*focused, 1, "choosing moves on to the next field");
}

#[test]
fn structural_actions_focus_fields_and_resolve_confirmations() {
    let mut form = Modal::form(
        "Settings",
        &["First", "Second"],
        &["one".into(), "two".into()],
        FormAction::SettingValue(crate::tui::pages::settings::SettingId::WindowWidth),
    );
    assert!(matches!(
        form.apply(crate::tui::action::ModalAct::Focus(1)),
        ModalResult::Continue
    ));
    let Modal::Form { focused, .. } = &form else {
        panic!("form expected");
    };
    assert_eq!(*focused, 1);

    let mut confirm = Modal::confirm("Remove?", ConfirmAction::RemoveInstance(uuid::Uuid::nil()));
    assert!(matches!(
        confirm.apply(crate::tui::action::ModalAct::Confirm),
        ModalResult::Confirm(ConfirmAction::RemoveInstance(_))
    ));
    assert!(matches!(
        confirm.apply(crate::tui::action::ModalAct::Reject),
        ModalResult::Reject
    ));
}
#[test]
fn version_field_enter_only_selects_and_f5_submits() {
    let mut modal = Modal::form(
        "Create instance",
        &["Name", "Minecraft version"],
        &["World".into(), "1.21.1".into()],
        FormAction::CreateInstance,
    )
    .with_selection_picker(1, vec!["1.21.1".into(), "1.20.1".into()]);

    assert!(matches!(
        modal.handle_key(key(KeyCode::Enter)),
        ModalResult::Continue
    ));
    assert!(matches!(
        modal.handle_key(key(KeyCode::Enter)),
        ModalResult::Continue
    ));
    assert!(matches!(
        modal.handle_key(key(KeyCode::Down)),
        ModalResult::Continue
    ));
    assert!(matches!(
        modal.handle_key(key(KeyCode::Enter)),
        ModalResult::SelectionChanged { field: 1, value } if value == "1.20.1"
    ));

    let Modal::Form { values, .. } = &modal else {
        panic!("form modal should remain open after choosing a version");
    };
    assert_eq!(values[1], "1.20.1");
    assert!(matches!(
        modal.handle_key(key(KeyCode::Enter)),
        ModalResult::Continue
    ));
    assert!(matches!(
        modal.handle_key(key(KeyCode::Esc)),
        ModalResult::Continue
    ));
    assert!(matches!(
        modal.handle_key(key(KeyCode::F(5))),
        ModalResult::Submit(FormAction::CreateInstance, _)
    ));
}

#[test]
fn empty_selection_opens_picker_immediately_and_requests_options() {
    let mut modal = Modal::form(
        "Create instance",
        &["Name", "Minecraft version"],
        &["World".into(), String::new()],
        FormAction::CreateInstance,
    )
    .with_selection_picker(1, Vec::new());
    modal.handle_key(key(KeyCode::Enter));

    assert!(matches!(
        modal.handle_key(key(KeyCode::Enter)),
        ModalResult::OpenSelectionPicker { field: 1 }
    ));
    let Modal::Form {
        selection_picker_open,
        ..
    } = &modal
    else {
        panic!("form modal should remain open");
    };
    assert_eq!(*selection_picker_open, Some(1));
}

#[test]
fn selection_picker_displays_loading_and_retryable_error_states() {
    let mut modal = Modal::form(
        "Create instance",
        &["Name", "Minecraft version"],
        &["World".into(), String::new()],
        FormAction::CreateInstance,
    )
    .with_selection_picker(1, Vec::new());
    modal.handle_key(key(KeyCode::Enter));
    modal.handle_key(key(KeyCode::Enter));

    assert!(modal.set_selection_loading(1));
    assert!(render_modal(&modal).contains("Loading"));

    assert!(modal.set_selection_error(1, "network unavailable"));
    let rendered = render_modal(&modal);
    assert!(rendered.contains("network unavailable"));
    // The retry cap is a key row now, not a sentence: cap + label.
    assert!(rendered.contains("Retry"), "{rendered}");
    assert!(matches!(
        modal.handle_key(key(KeyCode::Char('r'))),
        ModalResult::RetrySelectionPicker { field: 1 }
    ));
}

#[test]
fn late_selection_options_do_not_reopen_a_closed_picker() {
    let mut modal = Modal::form(
        "Create instance",
        &["Name", "Minecraft version"],
        &["World".into(), String::new()],
        FormAction::CreateInstance,
    )
    .with_selection_picker(1, Vec::new());
    modal.handle_key(key(KeyCode::Enter));
    modal.handle_key(key(KeyCode::Enter));
    modal.handle_key(key(KeyCode::Esc));

    assert!(modal.set_selection_options(1, vec!["1.21.1".into()]));

    let Modal::Form {
        selection_picker_open,
        values,
        ..
    } = &modal
    else {
        panic!("form modal should remain open");
    };
    assert_eq!(*selection_picker_open, None);
    assert!(values[1].is_empty());
}

#[test]
fn choosing_a_selection_reports_the_changed_field_and_value() {
    let mut modal = Modal::form(
        "Create instance",
        &["Name", "Minecraft version"],
        &["World".into(), "1.21.1".into()],
        FormAction::CreateInstance,
    )
    .with_selection_picker(1, vec!["1.21.1".into(), "1.20.1".into()]);
    modal.handle_key(key(KeyCode::Enter));
    modal.handle_key(key(KeyCode::Enter));
    modal.handle_key(key(KeyCode::Down));

    assert!(matches!(
        modal.handle_key(key(KeyCode::Enter)),
        ModalResult::SelectionChanged { field: 1, value } if value == "1.20.1"
    ));
}

fn render_modal(modal: &Modal) -> String {
    render_modal_at(modal, 100, 30)
}

fn render_modal_at(modal: &Modal, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("create test terminal");
    terminal
        .draw(|frame| {
            modal.draw(
                frame,
                crate::tui::theme::Theme::default(),
                &crate::i18n::I18n::new(crate::core::config::models::Language::English),
                0.0,
                &crate::tui::hit::Hitmap::default(),
            )
        })
        .expect("draw modal");
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

fn sample_versions() -> Vec<VersionSummary> {
    [
        ("1.21.1", "release", "2024-09-01T00:00:00Z"),
        ("25w15a", "snapshot", "2025-04-08T00:00:00Z"),
        ("25w13a", "snapshot", "2025-03-24T00:00:00Z"),
        ("b1.7.3", "old_beta", "2011-07-08T00:00:00Z"),
        ("19w14a", "snapshot", "2019-04-01T12:00:00Z"),
        ("25w14a", "snapshot", "2025-04-01T00:00:00Z"),
        ("a1.2.3", "old_alpha", "2010-08-04T00:00:00Z"),
    ]
    .into_iter()
    .map(|(id, kind, release_time)| VersionSummary {
        id: id.into(),
        kind: kind.into(),
        url: String::new(),
        time: release_time.into(),
        release_time: release_time.into(),
        sha1: None,
        compliance_level: None,
    })
    .collect()
}

fn version_picker() -> Modal {
    Modal::form(
        "Create instance",
        &["Minecraft version"],
        &[String::new()],
        FormAction::CreateInstance,
    )
    .with_version_picker(0, sample_versions())
}

#[test]
fn version_picker_defaults_to_releases_and_shows_full_category_counts() {
    let mut modal = version_picker();
    modal.open_selection_picker(0);

    let rendered = render_modal(&modal);
    assert!(rendered.contains("Release (1)"));
    assert!(rendered.contains("Snapshot (2)"));
    assert!(rendered.contains("Legacy (2)"));
    assert!(rendered.contains("April Fools (2)"));
    assert!(rendered.contains("1.21.1"));
    assert!(!rendered.contains("25w15a"));
    assert!(!rendered.contains("b1.7.3"));

    modal.handle_key(key(KeyCode::Down));
    assert!(matches!(
        modal.handle_key(key(KeyCode::Enter)),
        ModalResult::SelectionChanged { field: 0, value } if value == "1.21.1"
    ));
}

#[test]
fn version_category_toggles_combine_as_a_union_in_manifest_order() {
    let mut modal = version_picker();
    modal.open_selection_picker(0);
    modal.handle_key(key(KeyCode::Char('2')));
    modal.handle_key(key(KeyCode::Down));

    assert!(matches!(
        modal.handle_key(key(KeyCode::Enter)),
        ModalResult::SelectionChanged { field: 0, value } if value == "25w15a"
    ));
}

#[test]
fn version_picker_combines_categories_and_searches_ids_case_insensitively() {
    let mut modal = version_picker();
    modal.open_selection_picker(0);
    modal.handle_key(key(KeyCode::Char('2')));
    modal.handle_key(key(KeyCode::Char('/')));
    for character in "25W15AX".chars() {
        modal.handle_key(key(KeyCode::Char(character)));
    }
    modal.handle_key(key(KeyCode::Backspace));

    let rendered = render_modal(&modal);
    assert!(rendered.contains("Snapshot (2)"));
    assert!(rendered.contains("25w15a"));
    assert!(!rendered.contains("25w14a"));

    assert!(matches!(
        modal.handle_key(key(KeyCode::Enter)),
        ModalResult::Continue
    ));
    assert!(matches!(
        modal.handle_key(key(KeyCode::Enter)),
        ModalResult::SelectionChanged { field: 0, value } if value == "25w15a"
    ));
}

#[test]
fn version_picker_reports_no_matches_and_escape_clears_search_before_closing() {
    let mut modal = version_picker();
    modal.open_selection_picker(0);
    modal.handle_key(key(KeyCode::Char('/')));
    for character in "missing".chars() {
        modal.handle_key(key(KeyCode::Char(character)));
    }
    assert!(render_modal(&modal).contains("No versions match"));

    modal.handle_key(key(KeyCode::Esc));
    let rendered = render_modal(&modal);
    assert!(rendered.contains("1.21.1"));
    assert!(rendered.contains("Release (1)"));
    modal.handle_key(key(KeyCode::Esc));
    let Modal::Form {
        selection_picker_open,
        ..
    } = modal
    else {
        panic!("create form should remain open");
    };
    assert_eq!(selection_picker_open, None);
}

#[test]
fn version_picker_filter_toolbar_renders_at_narrow_and_wide_terminal_sizes() {
    for (width, height) in [(95, 30), (120, 40)] {
        let mut modal = version_picker();
        modal.open_selection_picker(0);
        let rendered = render_modal_at(&modal, width, height);

        assert!(rendered.contains("Release (1)"));
        assert!(rendered.contains("Snapshot (2)"));
        assert!(rendered.contains("Legacy (2)"));
        assert!(rendered.contains("April Fools (2)"));
        assert!(rendered.contains("Search (/):"));
        assert!(rendered.contains("1.21.1"));
    }
}

#[test]
fn page_navigation_keeps_selected_picker_option_visible() {
    let options = (0..40).map(|index| format!("1.21.{index}")).collect();
    let mut modal = Modal::form(
        "Create instance",
        &["Minecraft version"],
        &["1.21.0".into()],
        FormAction::CreateInstance,
    )
    .with_selection_picker(0, options);
    modal.handle_key(key(KeyCode::Enter));
    modal.handle_key(key(KeyCode::End));
    modal.handle_key(key(KeyCode::PageUp));

    let Modal::Form {
        selection_selected,
        selection_scroll,
        ..
    } = modal
    else {
        panic!("instance form should remain open");
    };
    assert_eq!(selection_selected, 27);
    assert_eq!(selection_scroll, 27);
}

#[test]
fn updating_options_does_not_turn_an_ordinary_field_into_a_picker() {
    let mut modal = Modal::form(
        "Settings",
        &["Minimum memory", "Maximum memory", "Window width"],
        &["1024".into(), "4096".into(), "1280".into()],
        FormAction::SettingValue(crate::tui::pages::settings::SettingId::MemoryMinimum),
    );

    assert!(!modal.set_selection_options(1, vec!["not a setting choice".into()]));
    modal.handle_key(key(KeyCode::Enter));
    modal.handle_key(key(KeyCode::Enter));

    let Modal::Form {
        focused,
        selection_picker_open,
        ..
    } = modal
    else {
        panic!("settings form should remain open");
    };
    assert_eq!(focused, 2);
    assert_eq!(selection_picker_open, None);
}

#[test]
fn enter_advances_from_version_selection_to_loader_and_loader_version_fields() {
    let mut modal = Modal::form(
        "Create instance",
        &[
            "Game directory",
            "Name",
            "Minecraft version",
            "Loader",
            "Loader version",
        ],
        &[
            "C:/Games".into(),
            "World".into(),
            "1.21.1".into(),
            "vanilla".into(),
            String::new(),
        ],
        FormAction::CreateInstance,
    )
    .with_selection_picker(2, vec!["1.21.1".into(), "1.20.1".into()]);

    modal.handle_key(key(KeyCode::Enter));
    modal.handle_key(key(KeyCode::Enter));
    modal.handle_key(key(KeyCode::Enter));
    modal.handle_key(key(KeyCode::Enter));

    let super::Modal::Form { focused, .. } = &modal else {
        panic!("instance form should remain open");
    };
    assert_eq!(
        *focused, 3,
        "choosing a game version should advance to Loader"
    );

    modal = modal
        .with_selection_picker(
            3,
            vec![
                "vanilla".into(),
                "fabric".into(),
                "forge".into(),
                "neoforge".into(),
            ],
        )
        .with_selection_picker(4, vec!["0.16.5".into()]);
    modal.handle_key(key(KeyCode::Enter));
    modal.handle_key(key(KeyCode::Down));
    assert!(matches!(
        modal.handle_key(key(KeyCode::Enter)),
        ModalResult::SelectionChanged { field: 3, value } if value == "fabric"
    ));
    modal.handle_key(key(KeyCode::Enter));
    modal.handle_key(key(KeyCode::Char('9')));
    modal.handle_key(key(KeyCode::Backspace));
    assert!(matches!(
        modal.handle_key(key(KeyCode::Enter)),
        ModalResult::SelectionChanged { field: 4, value } if value == "0.16.5"
    ));

    assert!(matches!(
        modal.handle_key(key(KeyCode::F(5))),
        ModalResult::Submit(FormAction::CreateInstance, values)
            if values[3] == "fabric" && values[4] == "0.16.5"
    ));
}
#[test]
fn version_filter_chips_and_the_search_line_have_actions() {
    let mut modal = Modal::form(
        "Pick a version",
        &["Version"],
        &["1.21.0".into()],
        FormAction::SettingValue(crate::tui::pages::settings::SettingId::WindowWidth),
    )
    .with_version_picker(0, sample_versions());
    assert!(matches!(
        modal.apply(crate::tui::action::ModalAct::Toggle(0)),
        ModalResult::Continue
    ));

    let enabled_before = match &modal {
        Modal::Form {
            version_picker: Some(picker),
            ..
        } => picker
            .filter
            .is_enabled(crate::core::version::models::VersionCategory::Snapshot),
        other => panic!("version picker expected, got {other:?}"),
    };
    modal.apply(crate::tui::action::ModalAct::ToggleVersionFilter(1));
    let enabled_after = match &modal {
        Modal::Form {
            version_picker: Some(picker),
            ..
        } => picker
            .filter
            .is_enabled(crate::core::version::models::VersionCategory::Snapshot),
        other => panic!("version picker expected, got {other:?}"),
    };
    assert_ne!(
        enabled_before, enabled_after,
        "the chip toggles its category"
    );

    modal.apply(crate::tui::action::ModalAct::EditSearch);
    match &modal {
        Modal::Form {
            version_picker: Some(picker),
            ..
        } => assert!(picker.editing_search, "the search line starts editing"),
        other => panic!("version picker expected, got {other:?}"),
    }
}

fn checklist_modal(single: bool) -> Modal {
    Modal::checklist(
        "Filter",
        vec![
            ChecklistItem::new("adventure", "Adventure"),
            ChecklistItem::new("decoration", "Decoration"),
            ChecklistItem::new("magic", "Magic"),
        ],
        &["decoration".to_string()],
        single,
    )
}

fn checked_flags(modal: &Modal) -> Vec<bool> {
    match modal {
        Modal::Checklist { checked, .. } => checked.clone(),
        other => panic!("checklist expected, got {other:?}"),
    }
}

fn focused_row(modal: &Modal) -> usize {
    match modal {
        Modal::Checklist { focused, .. } => *focused,
        other => panic!("checklist expected, got {other:?}"),
    }
}

#[test]
fn checklist_toggles_rows_and_reports_the_selection() {
    let mut modal = checklist_modal(false);
    assert_eq!(checked_flags(&modal), vec![false, true, false]);
    assert!(matches!(
        modal.apply(ModalAct::CheckItem(0)),
        ModalResult::Continue
    ));
    assert_eq!(checked_flags(&modal), vec![true, true, false]);
    modal.apply(ModalAct::CheckItem(0));
    assert_eq!(checked_flags(&modal), vec![false, true, false]);
    match modal.apply(ModalAct::ApplyChecks) {
        ModalResult::FilterApply(ids) => assert_eq!(ids, vec!["decoration".to_string()]),
        other => panic!("filter apply expected, got {other:?}"),
    }
}

#[test]
fn checklist_single_mode_keeps_exactly_one_row() {
    let mut modal = checklist_modal(true);
    modal.apply(ModalAct::CheckItem(0));
    assert_eq!(checked_flags(&modal), vec![true, false, false]);
    modal.apply(ModalAct::CheckItem(2));
    assert_eq!(checked_flags(&modal), vec![false, false, true]);
}

#[test]
fn checklist_reset_cursor_and_keys_share_the_same_actions() {
    let mut modal = checklist_modal(false);
    modal.apply(ModalAct::CheckItem(0));
    modal.apply(ModalAct::ResetChecks);
    assert_eq!(checked_flags(&modal), vec![false, false, false]);
    assert_eq!(focused_row(&modal), 0);

    modal.apply(ModalAct::FocusCheck(99));
    assert_eq!(focused_row(&modal), 2);
    modal.apply(ModalAct::FocusCheck(0));
    assert_eq!(focused_row(&modal), 0);

    // Keys go through the same `apply` path as clicks.
    modal.handle_key(key(KeyCode::Char(' ')));
    assert_eq!(checked_flags(&modal), vec![true, false, false]);
    modal.handle_key(key(KeyCode::Char('r')));
    assert_eq!(checked_flags(&modal), vec![false, false, false]);
    modal.handle_key(key(KeyCode::Down));
    assert_eq!(focused_row(&modal), 1);
    match modal.handle_key(key(KeyCode::Enter)) {
        ModalResult::FilterApply(ids) => assert!(ids.is_empty()),
        other => panic!("filter apply expected, got {other:?}"),
    }
    assert!(matches!(
        modal.handle_key(key(KeyCode::Esc)),
        ModalResult::Cancel
    ));
}

#[test]
fn checklist_renders_markers_in_both_presets() {
    use crate::core::config::models::Language;
    use crate::tui::hit::Hitmap;
    use crate::tui::theme::Theme;

    let mut modal = checklist_modal(false);
    modal.apply(ModalAct::CheckItem(0));
    let hits = Hitmap::default();
    for preset in crate::core::config::models::UiPreset::ALL {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| match preset {
                crate::core::config::models::UiPreset::Classic => modal.draw(
                    frame,
                    Theme::default(),
                    &crate::i18n::I18n::new(Language::English),
                    0.0,
                    &hits,
                ),
                crate::core::config::models::UiPreset::Paper => modal.draw_paper(
                    frame,
                    &crate::core::config::models::PaperTheme::Ivory
                        .palette(crate::core::config::models::ThemeMode::Light),
                    &crate::i18n::I18n::new(Language::English),
                    0.0,
                    &hits,
                ),
                crate::core::config::models::UiPreset::Shelf => modal.draw_shelf(
                    frame,
                    &crate::i18n::I18n::new(Language::English),
                    crate::tui::app::view::shelf::tone::palette(
                        crate::core::config::models::ShelfTheme::Lavender,
                        crate::core::config::models::ThemeMode::Dark,
                    ),
                    0.0,
                    &hits,
                ),
                // Nightlight borrows the creeper dialog until P6 ships its own.
                crate::core::config::models::UiPreset::Creeper
                | crate::core::config::models::UiPreset::Nightlight => {
                    let area = modal.area(frame.area());
                    modal.draw_creeper(
                        frame,
                        &crate::i18n::I18n::new(Language::English),
                        crate::tui::app::view::creeper::tone::palette(
                            crate::core::config::models::ThemeMode::Dark,
                        ),
                        area,
                        0.0,
                        &hits,
                    )
                }
            })
            .expect("draw checklist");
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("[x]"), "{preset:?}: {text}");
        assert!(text.contains("[ ]"), "{preset:?}: {text}");
        assert!(text.contains("Adventure"), "{preset:?}: {text}");
    }
}

#[test]
fn checklist_retry_row_asks_the_caller_to_reload() {
    let mut modal = checklist_modal(false).with_retry(true);
    assert!(matches!(
        modal.apply(ModalAct::ResetChecks),
        ModalResult::FilterRetry
    ));
    assert_eq!(checked_flags(&modal), vec![false, false, false]);
    let mut plain = checklist_modal(false);
    assert!(matches!(
        plain.apply(ModalAct::ResetChecks),
        ModalResult::Continue
    ));
}

// ── shared text field ─────────────────────────────────────────────

fn creeper_buffer(modal: &Modal, t: f64) -> ratatui::buffer::Buffer {
    let backend = TestBackend::new(64, 18);
    let mut terminal = Terminal::new(backend).expect("terminal");
    let hits = crate::tui::hit::Hitmap::default();
    terminal
        .draw(|frame| {
            let area = modal.area(frame.area());
            frame.render_widget(ratatui::widgets::Clear, frame.area());
            modal.draw_creeper(
                frame,
                &crate::i18n::I18n::new(crate::core::config::models::Language::English),
                crate::tui::app::view::creeper::tone::palette(
                    crate::core::config::models::ThemeMode::Dark,
                ),
                area,
                t,
                &hits,
            );
        })
        .expect("draw modal");
    terminal.backend().buffer().clone()
}

/// Cells painted with `bg`, as `(symbol, count)`.
fn cells_with_bg(
    buffer: &ratatui::buffer::Buffer,
    bg: ratatui::style::Color,
) -> Vec<(String, ratatui::style::Color)> {
    let mut found = Vec::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let cell = &buffer[(x, y)];
            if cell.bg == bg {
                found.push((cell.symbol().to_owned(), cell.fg));
            }
        }
    }
    found
}

#[test]
fn form_typing_lands_at_the_cursor_not_at_the_end() {
    let mut modal = Modal::form(
        "Create instance",
        &["Name"],
        &["abc".to_string()],
        FormAction::CreateInstance,
    );

    modal.handle_key(key(KeyCode::Home));
    modal.handle_key(key(KeyCode::Char('X')));

    let Modal::Form {
        values, cursors, ..
    } = &modal
    else {
        panic!("form");
    };
    assert_eq!(values[0], "Xabc");
    assert_eq!(cursors[0], 1);
}

#[test]
fn form_cursor_block_sits_on_the_insertion_point() {
    let mut modal = Modal::form(
        "Create instance",
        &["Name"],
        &["abc".to_string()],
        FormAction::CreateInstance,
    );
    modal.handle_key(key(KeyCode::Home));
    modal.handle_key(key(KeyCode::Char('X')));

    // Blink on: the character after the cursor is drawn as the block.
    let on = creeper_buffer(&modal, 0.0);
    let block = cells_with_bg(&on, crate::tui::palette::GREEN);
    assert!(
        block.iter().any(|(symbol, _)| symbol == "a"),
        "cursor block missing: {block:?}"
    );

    // Blink off: the same cell goes back to plain text.
    let off = creeper_buffer(&modal, 0.4);
    let block = cells_with_bg(&off, crate::tui::palette::GREEN);
    assert!(
        !block.iter().any(|(symbol, _)| symbol == "a"),
        "cursor did not blink: {block:?}"
    );
}

#[test]
fn prompt_cursor_blinks_with_the_shared_clock() {
    let modal = Modal::prompt(
        "JVM arguments",
        "Value",
        "space separated",
        "-Xmx2G",
        FormAction::SettingValue(crate::tui::pages::settings::SettingId::WindowWidth),
    );

    let on = creeper_buffer(&modal, 0.0);
    let off = creeper_buffer(&modal, 0.4);
    let cyan = crate::tui::palette::CYAN;
    assert!(
        !cells_with_bg(&on, cyan).is_empty(),
        "prompt cursor missing while lit"
    );
    assert!(
        cells_with_bg(&off, cyan).is_empty(),
        "prompt cursor did not blink"
    );
}

#[test]
fn form_option_fields_keep_no_typed_cursor() {
    let mut modal = Modal::form(
        "Create instance",
        &["Loader"],
        &["vanilla".to_string()],
        FormAction::CreateInstance,
    );
    modal = modal.with_selection_picker(0, vec!["vanilla".to_string(), "fabric".to_string()]);
    let buffer = creeper_buffer(&modal, 0.0);
    let block = cells_with_bg(&buffer, crate::tui::palette::GREEN);
    assert!(
        !block.iter().any(|(symbol, _)| symbol == "v"),
        "a picker field must not draw a text cursor: {block:?}"
    );
}

// ---------------------------------------------------------------------------
// paper dialog
// ---------------------------------------------------------------------------

fn paper_palette() -> crate::tui::app::view::paper::PaperPalette {
    crate::core::config::models::PaperTheme::Ivory
        .palette(crate::core::config::models::ThemeMode::Light)
}

/// Draws a modal the way the paper preset does and returns text plus buffer.
fn draw_paper_modal(
    modal: &Modal,
    width: u16,
    height: u16,
) -> (Vec<String>, ratatui::buffer::Buffer) {
    draw_paper_modal_with(modal, &paper_palette(), width, height)
}

fn draw_paper_modal_with(
    modal: &Modal,
    pal: &crate::tui::app::view::paper::PaperPalette,
    width: u16,
    height: u16,
) -> (Vec<String>, ratatui::buffer::Buffer) {
    use crate::core::config::models::Language;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    let hits = crate::tui::hit::Hitmap::default();
    terminal
        .draw(|frame| {
            modal.draw_paper(
                frame,
                pal,
                &crate::i18n::I18n::new(Language::English),
                0.0,
                &hits,
            )
        })
        .expect("draw paper modal");
    let buffer = terminal.backend().buffer().clone();
    let rows = (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect();
    (rows, buffer)
}

/// The cell a needle starts at, so a modifier can be read back.
fn cell_of(rows: &[String], needle: &str) -> (u16, u16) {
    let y = rows
        .iter()
        .position(|row| row.contains(needle))
        .unwrap_or_else(|| panic!("{needle:?} not drawn:\n{}", rows.join("\n")));
    let byte = rows[y].find(needle).expect("column");
    (
        crate::tui::palette::width(&rows[y][..byte]) as u16,
        y as u16,
    )
}

#[test]
fn the_paper_confirm_prints_its_title_and_both_keys() {
    let modal = Modal::confirm("Delete this instance?", ConfirmAction::RemoveAccount);
    let (rows, _) = draw_paper_modal(&modal, 100, 30);
    let text = rows.join("\n");
    assert!(text.contains("Confirm"), "the title is missing:\n{text}");
    assert!(text.contains("Delete this instance?"), "{text}");
    assert!(
        text.contains("Enter"),
        "the confirm key is missing:\n{text}"
    );
    assert!(text.contains("Esc"), "the cancel key is missing:\n{text}");
}

#[test]
fn the_paper_confirm_underlines_its_dangerous_key() {
    use ratatui::style::Modifier;
    let modal = Modal::confirm("Delete this instance?", ConfirmAction::RemoveAccount);
    let (rows, buffer) = draw_paper_modal(&modal, 100, 30);
    let (x, y) = cell_of(&rows, "Enter");
    assert!(
        buffer[(x, y)].modifier.contains(Modifier::UNDERLINED),
        "the destructive key is not underlined: {:?}",
        buffer[(x, y)]
    );
    let (x, y) = cell_of(&rows, "Esc");
    assert!(
        !buffer[(x, y)].modifier.contains(Modifier::UNDERLINED),
        "cancel should not carry the danger mark"
    );
}

#[test]
fn the_paper_form_underlines_the_value_of_the_focused_field() {
    use ratatui::style::Modifier;
    let mut modal = Modal::form(
        "Settings",
        &["First", "Second"],
        &["one".into(), "two".into()],
        FormAction::SettingValue(crate::tui::pages::settings::SettingId::WindowWidth),
    );
    let (rows, buffer) = draw_paper_modal(&modal, 100, 30);
    let (x, y) = cell_of(&rows, "one");
    assert!(
        buffer[(x, y)].modifier.contains(Modifier::UNDERLINED),
        "the focused value lost its rule: {:?}",
        buffer[(x, y)]
    );
    assert!(
        buffer[(x, y)].modifier.contains(Modifier::BOLD),
        "the focused value is not emphasised"
    );

    modal.apply(ModalAct::Focus(1));
    let (rows, buffer) = draw_paper_modal(&modal, 100, 30);
    let (x, y) = cell_of(&rows, "one");
    assert!(
        !buffer[(x, y)].modifier.contains(Modifier::BOLD),
        "focus did not move off the first field"
    );
    let (x, y) = cell_of(&rows, "two");
    assert!(
        buffer[(x, y)].modifier.contains(Modifier::UNDERLINED)
            && buffer[(x, y)].modifier.contains(Modifier::BOLD),
        "the second field is not the focused one"
    );
}

#[test]
fn the_paper_picker_prints_an_error_in_the_accent() {
    let mut modal = Modal::form(
        "Pick a loader",
        &["Loader", "Name"],
        &[String::new(), "demo".into()],
        FormAction::CreateInstance,
    )
    .with_selection_picker(0, Vec::new());
    // Opening the list puts the field into `Loading`; the failure lands after.
    modal.apply(ModalAct::Toggle(0));
    assert!(modal.set_selection_error(0, "the source did not answer"));
    let (rows, buffer) = draw_paper_modal(&modal, 100, 30);
    let (x, y) = cell_of(&rows, "the source did not answer");
    assert_eq!(
        buffer[(x, y)].fg,
        paper_palette().accent,
        "the error is not printed in the accent colour"
    );
}

#[test]
fn the_paper_picker_lists_every_option() {
    let mut modal = Modal::form(
        "Pick a loader",
        &["Loader", "Name"],
        &[String::new(), "demo".into()],
        FormAction::CreateInstance,
    )
    .with_selection_picker(0, vec!["vanilla".into(), "fabric".into()]);
    modal.apply(ModalAct::Toggle(0));
    let (rows, _) = draw_paper_modal(&modal, 100, 30);
    let text = rows.join("\n");
    assert!(text.contains("vanilla"), "{text}");
    assert!(text.contains("fabric"), "{text}");
    assert!(
        text.contains('▌'),
        "the chosen option has no margin bar:\n{text}"
    );
}

#[test]
fn the_paper_dialog_stays_readable_on_every_stock() {
    use crate::core::config::models::{PaperTheme, ThemeMode};
    for theme in PaperTheme::ALL {
        for mode in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::Terminal] {
            let pal = theme.palette(mode);
            let modal = Modal::confirm("Delete this instance?", ConfirmAction::RemoveAccount);
            let (rows, buffer) = draw_paper_modal_with(&modal, &pal, 100, 30);
            let text = rows.join("\n");
            for needle in ["Confirm", "Enter", "Esc"] {
                let (x, y) = cell_of(&rows, needle);
                let cell = &buffer[(x, y)];
                assert_ne!(
                    cell.fg, cell.bg,
                    "{theme:?}/{mode:?}: {needle} is printed in its own background\n{text}"
                );
                assert_eq!(
                    cell.bg, pal.sheet,
                    "{theme:?}/{mode:?}: {needle} left the slip"
                );
            }
            // The slip is a distinct surface, not the page painted twice.
            assert_ne!(pal.sheet, pal.bg, "{theme:?}/{mode:?}: sheet equals bg");
            assert_eq!(
                buffer[(50, 15)].bg,
                pal.sheet,
                "{theme:?}/{mode:?}: the slip body is not on sheet"
            );
        }
    }
}

#[test]
fn a_long_confirmation_wraps_instead_of_losing_its_tail() {
    let prompt = format!(
        "Remove the instance {} and everything in it? ENDMARK",
        "long name ".repeat(6)
    );
    let modal = Modal::confirm(prompt, ConfirmAction::RemoveAccount);
    let (rows, _) = draw_paper_modal(&modal, 100, 30);
    let text = rows.join("\n");
    assert!(
        text.contains("ENDMARK"),
        "the end of the question was cut off:\n{text}"
    );
    // The keys still sit on their own row, inside the slip.
    let enter = rows
        .iter()
        .position(|row| row.contains("Enter"))
        .expect("keys");
    let last_rule = rows
        .iter()
        .rposition(|row| row.contains('─'))
        .expect("bottom rule");
    assert!(enter < last_rule, "the keys fell out of the slip:\n{text}");
}

/// The slip is a fixed sixteen-row box, so the option window has to be measured
/// against its body. Promising `PICKER_VISIBLE_ROWS` options anyway pushed the
/// last rows — and the bar marking the chosen one — past the slip's bottom
/// rule, where the bar hung in the page below the form.
#[test]
fn the_paper_picker_keeps_its_selection_inside_the_slip() {
    let versions = (0..40)
        .map(|index| VersionSummary {
            id: format!("1.20.{index}"),
            kind: "release".into(),
            url: String::new(),
            time: "2026-01-01T00:00:00+00:00".into(),
            release_time: "2026-01-01T00:00:00+00:00".into(),
            sha1: None,
            compliance_level: None,
        })
        .collect::<Vec<_>>();
    let mut modal = Modal::form(
        "New instance",
        &["Name", "Version"],
        &["demo".to_owned(), String::new()],
        FormAction::CreateInstance,
    )
    .with_version_picker(1, versions);
    modal.apply(ModalAct::Toggle(1));
    // `End` drags the scroll window with the cursor: the last option has to be
    // drawn, and drawn inside the form.
    modal.handle_key(key(KeyCode::End));

    let (rows, _) = draw_paper_modal(&modal, 120, 35);
    let slip = modal.paper_area(ratatui::layout::Rect::new(0, 0, 120, 35));
    let body = Modal::paper_body(slip);
    let at = rows
        .iter()
        .position(|row| row.contains("1.20.39"))
        .unwrap_or_else(|| panic!("the selected version is not drawn:\n{}", rows.join("\n")));
    assert!(
        (body.y as usize..body.bottom() as usize).contains(&at),
        "the selected option printed outside the slip body: row {at}, body {body:?}"
    );
    assert!(
        rows[at].contains('▌'),
        "the selection bar is not on the selected row: {:?}",
        rows[at]
    );
    for row in &rows[body.bottom() as usize..] {
        assert!(
            !row.contains('▌'),
            "a selection bar hangs outside the form: {row:?}"
        );
    }
}

/// Draws `modal` with the shelf renderer and returns the frame as one string.
fn shelf_modal_text(modal: &Modal, width: u16, height: u16) -> String {
    use crate::core::config::models::Language;
    use crate::tui::hit::Hitmap;

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    let hits = Hitmap::default();
    terminal
        .draw(|frame| {
            modal.draw_shelf(
                frame,
                &crate::i18n::I18n::new(Language::English),
                crate::tui::app::view::shelf::tone::palette(
                    crate::core::config::models::ShelfTheme::Lavender,
                    crate::core::config::models::ThemeMode::Dark,
                ),
                0.0,
                &hits,
            )
        })
        .expect("draw shelf dialog");
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

/// All five dialog shapes on the shelf slab, wide and narrow: every one has to
/// paint its title and reach the frame without being cut off.
#[test]
fn the_shelf_dialog_draws_every_shape_at_both_sizes() {
    use crate::tui::pages::settings::SettingId;

    let mut picker = version_picker();
    picker.open_selection_picker(0);

    let cases = [
        (
            "form",
            Modal::form(
                "Edit",
                &["First", "Second"],
                &["one".to_string(), "two".to_string()],
                FormAction::SettingValue(SettingId::WindowWidth),
            ),
            "Edit",
        ),
        (
            "confirm",
            Modal::confirm("Remove?", ConfirmAction::RemoveInstance(uuid::Uuid::nil())),
            "Remove?",
        ),
        (
            "prompt",
            Modal::prompt(
                "Rename",
                "Name",
                "Enter saves",
                "instance",
                FormAction::SettingValue(SettingId::WindowWidth),
            ),
            "Rename",
        ),
        ("checklist", checklist_modal(false), "Adventure"),
        ("picker", picker, "1.21.1"),
    ];

    for (width, height) in [(120u16, 40u16), (80, 24)] {
        for (name, modal, expected) in &cases {
            let text = shelf_modal_text(modal, width, height);
            assert!(
                text.contains(expected),
                "{name} at {width}x{height} lost {expected:?}:\n{text}"
            );
            // The slab is drawn as a block; both border rows must be there, or
            // the dialog was taller than the frame.
            assert!(
                text.matches('╰').count() >= 1 && text.matches('╭').count() >= 1,
                "{name} at {width}x{height} has no closed slab:\n{text}"
            );
        }
    }
}

/// A prompt is six rows tall, so its body is four, and the key row has to be the
/// last of them. Gating it on a fifth row would leave the one dialog that is
/// nothing but a field and a help line with no keys at all.
#[test]
fn the_shelf_prompt_prints_its_keys_on_the_last_body_row() {
    use crate::tui::pages::settings::SettingId;

    let modal = Modal::prompt(
        "Rename",
        "Name",
        "Enter saves",
        "instance",
        FormAction::SettingValue(SettingId::WindowWidth),
    );
    for (width, height) in [(120u16, 40u16), (80, 24)] {
        let text = shelf_modal_text(&modal, width, height);
        for key in ["Enter", "Esc"] {
            assert!(
                text.contains(key),
                "the prompt dropped `{key}` at {width}x{height}:\n{text}"
            );
        }
        // The keys belong to the dialog, not to the page behind it.
        let slab = text
            .lines()
            .skip_while(|line| !line.contains('╭'))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            slab.contains("Enter") && slab.contains("Esc"),
            "the keys are outside the slab at {width}x{height}:\n{text}"
        );
    }
}
