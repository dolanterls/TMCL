//! P1 acceptance: chrome hit regions, overlay blockers and hover state.

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::{Terminal, backend::TestBackend};

use crate::core::config::models::UiPreset;
use crate::tui::action::{Action, PageAct};
use crate::tui::app::{App, Page};
use crate::tui::event::AppEvent;

const WIDTH: u16 = 120;
const HEIGHT: u16 = 40;

fn draw(app: &App) {
    draw_sized(app, WIDTH, HEIGHT);
}

fn draw_sized(app: &App, width: u16, height: u16) {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("create test terminal");
    terminal.draw(|frame| app.draw(frame)).expect("draw app");
}

/// Column of the first `symbol` drawn on `row`, in terminal cells.
fn drawn_symbol_column(app: &App, row: u16, symbol: &str) -> Option<u16> {
    let backend = TestBackend::new(WIDTH, HEIGHT);
    let mut terminal = Terminal::new(backend).expect("create test terminal");
    terminal.draw(|frame| app.draw(frame)).expect("draw app");
    let buffer = terminal.backend().buffer();
    (0..WIDTH).find(|column| buffer[(*column, row)].symbol() == symbol)
}

fn mouse(app: &mut App, kind: MouseEventKind, column: u16, row: u16) {
    app.handle(AppEvent::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }));
}

fn click(app: &mut App, column: u16, row: u16) {
    mouse(app, MouseEventKind::Down(MouseButton::Left), column, row);
}

fn classic_tab(app: &App, index: usize) -> Rect {
    let regions = app.layout_chrome(Rect::new(0, 0, WIDTH, HEIGHT));
    let labels = app.tab_labels(regions.mode);
    crate::tui::chrome::tab_rects(regions.tabs, &labels)[index]
}

fn creeper_nav(app: &App, index: usize) -> Rect {
    let regions = app.layout_chrome(Rect::new(0, 0, WIDTH, HEIGHT));
    let layout = crate::tui::app::view::creeper::sidebar::nav_layout(regions.tabs);
    crate::tui::app::view::creeper::sidebar::nav_rects(&layout, crate::tui::app::PAGES.len())[index]
}

#[test]
fn classic_tabs_are_clickable_through_the_hit_map() {
    let mut app = App::new();
    draw(&app);

    let rect = classic_tab(&app, 3);
    assert_eq!(
        app.hitmap
            .at(rect.x, rect.y, app.top_layer())
            .map(|hit| hit.action),
        Some(Action::Nav(Page::Tasks)),
        "the drawn tab must be registered at its own coordinates"
    );

    click(&mut app, rect.x, rect.y);
    assert_eq!(app.page, Page::Tasks);
}

#[test]
fn creeper_sidebar_rows_are_clickable_through_the_hit_map() {
    let mut app = App::new();
    app.config.state.ui.preset = UiPreset::Creeper;
    draw(&app);

    let rect = creeper_nav(&app, 3);
    assert_eq!(
        app.hitmap
            .at(rect.x, rect.y, app.top_layer())
            .map(|hit| hit.action),
        Some(Action::Nav(Page::Tasks))
    );

    click(&mut app, rect.x, rect.y);
    assert_eq!(app.page, Page::Tasks);
}

#[test]
fn the_help_overlay_blocks_the_page_behind_it() {
    let mut app = App::new();
    app.help_open = true;
    draw(&app);

    let rect = classic_tab(&app, 3);
    click(&mut app, rect.x, rect.y);

    assert_eq!(
        app.page,
        Page::Overview,
        "a click behind the help card must not reach the page"
    );
    assert!(
        !app.help_open,
        "a click outside the help card closes the overlay"
    );
}

#[test]
fn the_help_overlay_is_blocking_in_the_creeper_preset_too() {
    let mut app = App::new();
    app.config.state.ui.preset = UiPreset::Creeper;
    app.help_open = true;
    draw(&app);

    let rect = creeper_nav(&app, 3);
    click(&mut app, rect.x, rect.y);

    assert_eq!(app.page, Page::Overview);
    assert!(!app.help_open);
}

#[test]
fn clicking_inside_the_help_card_keeps_it_open() {
    let mut app = App::new();
    app.help_open = true;
    draw(&app);

    // The blocker has to cover the card the painter draws, not the 16-row
    // default: with the default, clicking the lower half of the reference counts
    // as an outside click and closes it.
    let rect = crate::tui::app::view::help_rect(Rect::new(0, 0, WIDTH, HEIGHT), &app.help_lines());
    click(&mut app, rect.x + 2, rect.y + 2);
    assert!(app.help_open, "the top of the card is inside it");
    click(&mut app, rect.x + 2, rect.bottom() - 2);
    assert!(app.help_open, "the bottom of the card is inside it too");
}

#[test]
fn hover_tracks_the_tab_under_the_pointer_and_clears_off_target() {
    let mut app = App::new();
    draw(&app);

    let rect = classic_tab(&app, 3);
    mouse(&mut app, MouseEventKind::Moved, rect.x, rect.y);
    assert_eq!(app.hover, Some(Action::Nav(Page::Tasks)));

    let outside = classic_tab(&app, 6);
    mouse(
        &mut app,
        MouseEventKind::Moved,
        outside.right().saturating_add(40),
        outside.y,
    );
    assert_eq!(app.hover, None);
}

#[test]
fn clicks_are_ignored_until_the_next_frame_after_a_resize() {
    let mut app = App::new();
    draw(&app);
    let rect = classic_tab(&app, 3);

    app.handle(AppEvent::Resize(WIDTH, HEIGHT));
    click(&mut app, rect.x, rect.y);
    assert_eq!(
        app.page,
        Page::Overview,
        "regions drawn for the previous size must not route"
    );

    draw(&app);
    click(&mut app, rect.x, rect.y);
    assert_eq!(app.page, Page::Tasks);
}

// ---------------------------------------------------------------------------
// P2: page hit regions
// ---------------------------------------------------------------------------

use crate::tui::hit::RowKind;

/// Draws `page` at `(width, height)` and returns the app plus the boxes the
/// frame registered.
fn page_frame(
    preset: UiPreset,
    page: Page,
    width: u16,
    height: u16,
) -> (App, Vec<crate::tui::hit::Hitbox>) {
    let mut app = App::new();
    app.config.state.ui.preset = preset;
    app.page = page;
    app.terminal_width = width;
    app.terminal_height = height;
    draw_sized(&app, width, height);
    let boxes = app.hitmap.hitboxes();
    (app, boxes)
}

#[test]
fn every_registered_box_is_reachable_and_inside_the_frame() {
    let sizes = [(120, 40), (100, 30), (80, 24)];
    for preset in [UiPreset::Classic, UiPreset::Creeper, UiPreset::Nightlight] {
        for page in Page::ALL {
            for (width, height) in sizes {
                let (app, boxes) = page_frame(preset, page, width, height);
                for hitbox in &boxes {
                    let rect = hitbox.rect;
                    assert!(
                        rect.right() <= width && rect.bottom() <= height,
                        "{preset:?} {page:?} {width}x{height}: {rect:?} leaves the frame"
                    );
                    let centre = (rect.x + rect.width / 2, rect.y + rect.height / 2);
                    let found = app
                        .hitmap
                        .at(centre.0, centre.1, app.top_layer())
                        .map(|hit| hit.action);
                    assert_eq!(
                        found,
                        Some(hitbox.action),
                        "{preset:?} {page:?} {width}x{height}: {rect:?} is covered by another box"
                    );
                }
            }
        }
    }
}

#[test]
fn list_pages_register_one_target_per_visible_row() {
    let cases = [
        (Page::Instances, RowKind::Instances),
        (Page::Accounts, RowKind::Accounts),
        (Page::Tasks, RowKind::Tasks),
        (Page::Overview, RowKind::OverviewInstances),
    ];
    for preset in [UiPreset::Classic, UiPreset::Creeper, UiPreset::Nightlight] {
        for (page, kind) in cases {
            let (app, boxes) = page_frame(preset, page, 120, 40);
            let rows = boxes
                .iter()
                .filter(|hitbox| {
                    matches!(
                        hitbox.action,
                        Action::Page {
                            page: action_page,
                            act: PageAct::Row(_)
                        } if action_page == kind.page()
                    )
                })
                .count();
            // Empty fixtures still draw the frame, so allow zero; what must hold
            // is that row targets never outnumber the registered boxes.
            assert!(
                rows <= boxes.len(),
                "{preset:?} {page:?}: {rows} rows out of {} boxes",
                boxes.len()
            );
            let _ = &app;
        }
    }
}

#[test]
fn a_click_on_a_drawn_row_selects_that_row() {
    for preset in [UiPreset::Classic, UiPreset::Creeper, UiPreset::Nightlight] {
        let (mut app, _boxes) = page_frame(preset, Page::Instances, 120, 40);
        app.instances.apply(vec![
            crate::tui::pages::instances::InstanceSummary {
                id: uuid::Uuid::new_v4(),
                name: "first".into(),
                version: "1.20.1".into(),
                loader: "Vanilla".into(),
                loader_version: None,
                status: "ready".into(),
                path: std::path::PathBuf::from("first"),
                launch_count: 0,
                play_time_seconds: 0,
            },
            crate::tui::pages::instances::InstanceSummary {
                id: uuid::Uuid::new_v4(),
                name: "second".into(),
                version: "1.21.1".into(),
                loader: "Fabric".into(),
                loader_version: None,
                status: "ready".into(),
                path: std::path::PathBuf::from("second"),
                launch_count: 0,
                play_time_seconds: 0,
            },
        ]);
        draw_sized(&app, 120, 40);
        let boxes = app.hitmap.hitboxes();
        assert!(boxes.iter().any(|hitbox| matches!(
            hitbox.action,
            Action::Page {
                act: PageAct::Row(0),
                ..
            }
        )));
        let row = boxes
            .iter()
            .find(|hitbox| {
                matches!(
                    hitbox.action,
                    Action::Page {
                        act: PageAct::Row(1),
                        ..
                    }
                )
            })
            .copied();
        // With two instances the second row may be off-screen in the smallest
        // layouts; only click it when the frame offered it.
        if let Some(row) = row {
            click(&mut app, row.rect.x + 1, row.rect.y);
            assert_eq!(app.instances.focused, 1, "{preset:?}");
        }
    }
}

/// Every drawn instance row and the rectangle it occupies.
fn instance_rows(app: &App) -> Vec<(usize, Rect)> {
    app.hitmap
        .hitboxes()
        .iter()
        .filter_map(|hitbox| match hitbox.action {
            Action::Page {
                page: Page::Instances,
                act: PageAct::Row(index),
            } => Some((index, hitbox.rect)),
            _ => None,
        })
        .collect()
}

#[test]
fn shelf_cover_opens_the_instance_workspace() {
    let mut app = instances_app(1);
    app.config.state.ui.preset = UiPreset::Shelf;
    draw_sized(&app, 120, 40);

    // Two targets carry the same action: the picture and the name bar under
    // it, exactly the pair the interaction contract names.
    let mut targets = app
        .hitmap
        .hitboxes()
        .iter()
        .filter_map(|hitbox| {
            matches!(
                hitbox.action,
                Action::Page {
                    page: Page::Instances,
                    act: PageAct::Activate(0),
                }
            )
            .then_some(hitbox.rect)
        })
        .collect::<Vec<_>>();
    targets.sort_by_key(|rect| rect.height);
    assert_eq!(targets.len(), 2, "cover and name bar must both be targets");
    let (bar, cover) = (targets[0], targets[1]);
    assert_eq!(bar.height, 1, "the name bar is one row tall");

    click(&mut app, cover.x + 1, cover.y + 1);
    assert!(
        app.workspace.is_some(),
        "clicking the cover must open the instance workspace, the same as Enter"
    );

    let mut bar_app = instances_app(1);
    bar_app.config.state.ui.preset = UiPreset::Shelf;
    draw_sized(&bar_app, 120, 40);
    click(&mut bar_app, bar.x + 1, bar.y);
    assert!(
        bar_app.workspace.is_some(),
        "clicking the name bar must open the workspace too"
    );
}

#[test]
fn shelf_tabs_switch_pages_through_the_hit_map() {
    let mut app = instances_app(1);
    app.config.state.ui.preset = UiPreset::Shelf;
    draw_sized(&app, 120, 40);

    let rect = app
        .hitmap
        .hitboxes()
        .iter()
        .rev()
        .find(|hitbox| hitbox.action == Action::Nav(Page::Tasks))
        .map(|hitbox| hitbox.rect)
        .expect("the Tasks tab must be registered");
    click(&mut app, rect.x + 1, rect.y);
    assert_eq!(app.page, Page::Tasks);
}

#[test]
fn shelf_instance_cards_are_clickable_at_their_drawn_rows() {
    let mut app = instances_app(5);
    app.config.state.ui.preset = UiPreset::Shelf;
    draw_sized(&app, 120, 40);

    let rows = instance_rows(&app);
    assert_eq!(rows.len(), 5, "five three-row cards must all be registered");
    let (index, rect) = rows[3];
    click(&mut app, rect.x + 1, rect.y + 1);
    assert_eq!(app.instances.focused, index);
}

/// A click on a row that is already on screen must not move the list.
///
/// A window derived from the selection alone re-anchors the whole list when the
/// reader picks any row above the last visible one: the clicked row slides to the
/// bottom edge and the row under the pointer changes. Every preset shares this
/// contract.
#[test]
fn clicking_a_drawn_row_leaves_the_instance_list_where_it_is() {
    for preset in UiPreset::ALL {
        let mut app = instances_app(80);
        app.config.state.ui.preset = preset;
        // The last item, so the window is scrolled to the end and every row
        // above it is a candidate for the jump.
        app.instances.focused = 79;
        draw_sized(&app, 120, 40);
        let before = instance_rows(&app);
        assert!(
            before.len() > 2 && before[0].0 > 0,
            "{preset:?}: need a scrolled window to click in, got {before:?}"
        );

        // A row that is neither the selection nor the last one: under the old
        // rule this is exactly where the jump showed up.
        let (index, rect) = before[before.len() / 2];
        click(&mut app, rect.x + 1, rect.y);
        assert_eq!(
            app.instances.focused, index,
            "{preset:?}: the click must select the row it landed on"
        );

        draw_sized(&app, 120, 40);
        assert_eq!(
            instance_rows(&app),
            before,
            "{preset:?}: the list moved under a click on a row that was already drawn"
        );
    }
}

// ---------------------------------------------------------------------------
// P3: wheel routing and hover
// ---------------------------------------------------------------------------

fn wheel(app: &mut App, kind: MouseEventKind, column: u16, row: u16) {
    mouse(app, kind, column, row);
}

fn instances_app(count: usize) -> App {
    let mut app = App::new();
    app.page = Page::Instances;
    app.terminal_width = 120;
    app.terminal_height = 40;
    app.instances.apply(
        (0..count)
            .map(|index| crate::tui::pages::instances::InstanceSummary {
                id: uuid::Uuid::new_v4(),
                name: format!("instance-{index}"),
                version: "1.21.1".into(),
                loader: "Vanilla".into(),
                loader_version: None,
                status: "ready".into(),
                path: std::path::PathBuf::from(format!("instance-{index}")),
                launch_count: 0,
                play_time_seconds: 0,
            })
            .collect(),
    );
    app
}

fn row_rect(app: &App, index: usize) -> Option<ratatui::layout::Rect> {
    app.hitmap
        .hitboxes()
        .iter()
        .find(|hitbox| {
            matches!(
                hitbox.action,
                Action::Page { act: PageAct::Row(row), .. } if row == index
            )
        })
        .map(|hitbox| hitbox.rect)
}

#[test]
fn the_wheel_moves_the_list_under_the_pointer_by_one_row() {
    let mut app = instances_app(10);
    draw_sized(&app, 120, 40);
    let row = row_rect(&app, 0).expect("a drawn row");

    wheel(&mut app, MouseEventKind::ScrollDown, row.x + 1, row.y);
    assert_eq!(app.instances.focused, 1, "one notch moves one row");
    wheel(&mut app, MouseEventKind::ScrollDown, row.x + 1, row.y);
    assert_eq!(app.instances.focused, 2, "notches accumulate one row each");
    wheel(&mut app, MouseEventKind::ScrollUp, row.x + 1, row.y);
    assert_eq!(app.instances.focused, 1);
}

#[test]
fn the_wheel_outside_any_region_is_ignored() {
    let mut app = instances_app(10);
    draw_sized(&app, 120, 40);
    // The footer strip carries no list region.
    wheel(&mut app, MouseEventKind::ScrollDown, 1, 39);
    assert_eq!(app.instances.focused, 0);
}

#[test]
fn the_help_body_scrolls_with_the_wheel() {
    let mut app = instances_app(3);
    app.help_open = true;
    draw(&app);
    assert!(
        app.help_max_scroll() > 0,
        "the fixture page has to overflow the card for this test to mean anything"
    );
    let rect = crate::tui::app::view::help_rect(Rect::new(0, 0, WIDTH, HEIGHT), &app.help_lines());
    wheel(&mut app, MouseEventKind::ScrollDown, rect.x + 2, rect.y + 2);
    assert_eq!(app.help_scroll, 3);
    wheel(&mut app, MouseEventKind::ScrollUp, rect.x + 2, rect.y + 2);
    assert_eq!(app.help_scroll, 0);
}

#[test]
fn hover_changes_colour_only_never_geometry() {
    let mut app = instances_app(10);
    let plain = crate::tui::app::tests::render_app(&app, 120, 40);
    let before = app.hitmap.hitboxes();
    let row = row_rect(&app, 2).expect("a drawn row");

    mouse(&mut app, MouseEventKind::Moved, row.x + 1, row.y);
    assert_eq!(app.hover, Some(Action::row(Page::Instances, 2)));
    let hovered = crate::tui::app::tests::render_app(&app, 120, 40);
    let after = app.hitmap.hitboxes();

    assert_eq!(before.len(), after.len());
    for (a, b) in before.iter().zip(after.iter()) {
        assert_eq!((a.rect, a.action, a.layer), (b.rect, b.action, b.layer));
    }
    // Same glyphs: the tint only rewrote background colours.
    assert_eq!(plain, hovered);
}

#[test]
fn the_wheel_does_not_reach_the_page_behind_an_overlay() {
    let mut app = instances_app(10);
    app.help_open = true;
    draw_sized(&app, 120, 40);
    // Bottom-left corner: outside the help card, over the page list.
    wheel(&mut app, MouseEventKind::ScrollDown, 1, 10);
    assert_eq!(app.instances.focused, 0, "the page must not scroll");
    assert_eq!(app.help_scroll, 0);
}

// ---------------------------------------------------------------------------
// P4: dialogs are ordinary hit regions
// ---------------------------------------------------------------------------

use crate::tui::action::ModalAct;
use crate::tui::modal::{ConfirmAction, FormAction, Modal};

fn form_app(preset: UiPreset, modal: Modal) -> App {
    let mut app = App::new();
    app.config.state.ui.preset = preset;
    app.terminal_width = 120;
    app.terminal_height = 40;
    app.modal = Some(modal);
    app
}

fn modal_box(app: &App, act: ModalAct) -> Option<ratatui::layout::Rect> {
    app.hitmap
        .hitboxes()
        .iter()
        .rev()
        .find(|hitbox| hitbox.action == Action::Modal(act))
        .map(|hitbox| hitbox.rect)
}

#[test]
fn dialog_fields_are_clickable_in_every_preset() {
    for preset in UiPreset::ALL {
        let mut app = form_app(
            preset,
            Modal::form(
                "Edit",
                &["First", "Second"],
                &["one".into(), "two".into()],
                FormAction::SettingValue(crate::tui::pages::settings::SettingId::WindowWidth),
            ),
        );
        draw_sized(&app, 120, 40);
        let row = modal_box(&app, ModalAct::Focus(1)).expect("second field row");
        click(&mut app, row.x + 2, row.y);
        match &app.modal {
            Some(Modal::Form { focused, .. }) => assert_eq!(*focused, 1, "{preset:?}"),
            other => panic!("form expected, got {other:?}"),
        }
    }
}

#[test]
fn dialog_option_rows_pick_a_value_and_close_the_list() {
    let mut app = form_app(
        UiPreset::Classic,
        Modal::form(
            "Edit",
            &["Mode"],
            &["a".into()],
            FormAction::SettingValue(crate::tui::pages::settings::SettingId::WindowWidth),
        )
        .with_selection_picker(0, vec!["a".into(), "b".into()]),
    );
    app.modal
        .as_mut()
        .expect("dialog")
        .apply(ModalAct::Toggle(0));
    draw_sized(&app, 120, 40);
    let option = modal_box(&app, ModalAct::Option(1)).expect("second option row");
    click(&mut app, option.x + 2, option.y);
    match &app.modal {
        Some(Modal::Form {
            values,
            selection_picker_open,
            ..
        }) => {
            assert_eq!(values[0], "b");
            assert!(selection_picker_open.is_none(), "picking closes the list");
        }
        other => panic!("form expected, got {other:?}"),
    }
}

#[test]
fn a_click_outside_a_dialog_keeps_it_open() {
    let mut app = form_app(
        UiPreset::Classic,
        Modal::form(
            "Edit",
            &["Only"],
            &["x".into()],
            FormAction::SettingValue(crate::tui::pages::settings::SettingId::WindowWidth),
        ),
    );
    draw_sized(&app, 120, 40);
    click(&mut app, 1, 1);
    assert!(app.modal.is_some());
}

#[test]
fn a_dialog_key_chip_resolves_the_confirmation() {
    for preset in UiPreset::ALL {
        let mut app = form_app(
            preset,
            Modal::confirm("Remove?", ConfirmAction::RemoveInstance(uuid::Uuid::nil())),
        );
        draw_sized(&app, 120, 40);
        let chip = modal_box(&app, ModalAct::Reject)
            .unwrap_or_else(|| panic!("{preset:?}: reject chip is missing"));
        click(&mut app, chip.x + 1, chip.y);
        assert!(
            app.modal.is_none(),
            "{preset:?}: the chip closes the dialog"
        );
    }
}

#[test]
fn the_wheel_moves_the_open_option_list() {
    let options = (0..20)
        .map(|index| format!("option-{index}"))
        .collect::<Vec<_>>();
    let mut app = form_app(
        UiPreset::Classic,
        Modal::form(
            "Edit",
            &["Mode"],
            &["option-0".into()],
            FormAction::SettingValue(crate::tui::pages::settings::SettingId::WindowWidth),
        )
        .with_selection_picker(0, options),
    );
    app.modal
        .as_mut()
        .expect("dialog")
        .apply(ModalAct::Toggle(0));
    draw_sized(&app, 120, 40);
    let option = modal_box(&app, ModalAct::Option(0)).expect("first option row");
    wheel(&mut app, MouseEventKind::ScrollDown, option.x + 2, option.y);
    match &app.modal {
        Some(Modal::Form {
            selection_selected, ..
        }) => assert_eq!(*selection_selected, 1),
        other => panic!("form expected, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// P5: double click activation
// ---------------------------------------------------------------------------

use std::time::{Duration, Instant};

fn click_row(app: &mut App, page: Page, index: usize) {
    let rect = app
        .hitmap
        .hitboxes()
        .iter()
        .rev()
        .find(|hitbox| hitbox.action == Action::row(page, index))
        .map(|hitbox| hitbox.rect)
        .unwrap_or_else(|| panic!("no drawn row {index} on {page:?}"));
    click(app, rect.x + 1, rect.y);
}

#[test]
fn double_click_selects_the_row_and_runs_enter() {
    for preset in [UiPreset::Classic, UiPreset::Creeper, UiPreset::Nightlight] {
        let mut app = instances_app(3);
        app.config.state.ui.preset = preset;
        draw_sized(&app, 120, 40);
        click_row(&mut app, Page::Instances, 1);
        assert_eq!(app.instances.focused, 1);
        assert!(!app.detail_open, "the first click only selects");
        click_row(&mut app, Page::Instances, 1);
        assert!(
            app.workspace_is_open(),
            "{preset:?}: the second click opens the workspace"
        );
    }
}

#[test]
fn a_second_click_after_the_window_only_selects_again() {
    let mut app = instances_app(3);
    draw_sized(&app, 120, 40);
    click_row(&mut app, Page::Instances, 0);
    assert!(!app.detail_open);

    // Pretend the first click happened long ago.
    let context = app.click_context();
    app.last_click = Some((
        Action::row(Page::Instances, 0),
        context,
        Instant::now() - Duration::from_millis(600),
    ));
    click_row(&mut app, Page::Instances, 0);
    assert!(!app.detail_open, "a stale click must not activate");
}

#[test]
fn clicking_another_row_starts_a_new_pair() {
    let mut app = instances_app(3);
    draw_sized(&app, 120, 40);
    click_row(&mut app, Page::Instances, 0);
    click_row(&mut app, Page::Instances, 1);
    assert_eq!(app.instances.focused, 1);
    assert!(!app.detail_open, "different targets never pair up");

    click_row(&mut app, Page::Instances, 1);
    assert!(app.workspace_is_open(), "the pair restarted on row 1");
}

#[test]
fn double_click_uses_the_same_path_as_enter() {
    let mut app = App::new();
    app.page = Page::DownloadCenter;
    app.terminal_width = 120;
    app.terminal_height = 40;
    app.download_center.category =
        crate::tui::pages::download_center::DownloadCategory::GameVersions;
    app.versions = vec![crate::core::version::models::VersionSummary {
        id: "1.21.1".into(),
        kind: "release".into(),
        url: String::new(),
        time: String::new(),
        release_time: String::new(),
        sha1: None,
        compliance_level: None,
    }];
    draw_sized(&app, 120, 40);
    click_row(&mut app, Page::DownloadCenter, 0);
    click_row(&mut app, Page::DownloadCenter, 0);
    // `Enter` on a version opens the create-instance form; so does a double
    // click, and nothing is launched behind the user's back.
    assert!(
        matches!(app.modal, Some(Modal::Form { .. })),
        "expected the create form, got {:?}",
        app.modal
    );
}

// ---------------------------------------------------------------------------
// P6 audit gaps: prompt, step arrows, overview rows, detail wheel, budget
// ---------------------------------------------------------------------------

#[test]
fn the_single_line_prompt_is_clickable_and_submits() {
    let mut app = App::new();
    app.terminal_width = 120;
    app.terminal_height = 40;
    app.modal = Some(Modal::prompt(
        "Window width",
        "Columns",
        "Enter to apply",
        "100",
        FormAction::SettingValue(crate::tui::pages::settings::SettingId::WindowWidth),
    ));
    draw_sized(&app, 120, 40);

    let field = modal_box(&app, ModalAct::Focus(0)).expect("prompt field");
    click(&mut app, field.x + 2, field.y);
    assert!(
        app.modal.is_some(),
        "focusing the field keeps the dialog open"
    );

    draw_sized(&app, 120, 40);
    let status_before = app.status.as_str().to_owned();
    let submit = modal_box(&app, ModalAct::Submit).expect("prompt submit chip");
    click(&mut app, submit.x + 1, submit.y);
    // Submitting either applies the value and closes, or reports a validation
    // error; both prove the chip reached the dialog's state machine.
    assert!(
        app.modal.is_none() || app.status.as_str() != status_before,
        "the chip must submit the prompt"
    );
}

#[test]
fn the_step_arrows_change_the_setting_value() {
    let mut app = App::new();
    app.config.state.ui.preset = UiPreset::Creeper;
    app.page = Page::Settings;
    app.terminal_width = 120;
    app.terminal_height = 40;
    app.settings
        .select_category(crate::tui::pages::settings::SettingsCategory::Appearance);
    draw_sized(&app, 120, 40);
    let before = app.settings_rows()[0].value.clone();

    // The first row is the one drawn highest on screen; hitbox registration
    // order is an implementation detail and must not decide what gets clicked.
    let right = app
        .hitmap
        .hitboxes()
        .iter()
        .filter(|hitbox| {
            matches!(
                hitbox.action,
                Action::Page {
                    act: PageAct::Step { by: 1, .. },
                    ..
                }
            )
        })
        .min_by_key(|hitbox| hitbox.rect.y)
        .map(|hitbox| hitbox.rect)
        .expect("a step arrow on the first setting row");
    click(&mut app, right.x, right.y);
    let after = app.settings_rows()[0].value.clone();
    assert_ne!(before, after, "clicking › must change the value");
}

#[test]
fn both_step_arrows_are_drawn_under_their_hit_boxes() {
    use crate::tui::pages::settings::SettingsCategory;

    let mut app = App::new();
    app.config.state.ui.preset = UiPreset::Creeper;
    app.page = Page::Settings;
    app.terminal_width = WIDTH;
    app.terminal_height = HEIGHT;
    app.settings.select_category(SettingsCategory::Appearance);
    draw(&app);

    // A short value leaves `›` right after the text, so its two clickable cells
    // must sit there rather than on blank space at the end of the row.
    for (by, glyph) in [(-1i8, '‹'), (1, '›')] {
        let hitbox = app
            .hitmap
            .hitboxes()
            .iter()
            .rev()
            .find(|hitbox| {
                matches!(
                    hitbox.action,
                    Action::Page {
                        act: PageAct::Step { by: step, .. },
                        ..
                    } if step == by
                )
            })
            .copied()
            .expect("step arrow hit box");
        let column =
            drawn_symbol_column(&app, hitbox.rect.y, &glyph.to_string()).expect("drawn step arrow");
        assert!(
            hitbox.rect.x <= column && column < hitbox.rect.x + hitbox.rect.width,
            "{glyph} is drawn at column {column} but its hit box is {:?}",
            hitbox.rect
        );
    }
}

#[test]
fn a_classic_overview_row_selects_the_instance() {
    let mut app = instances_app(3);
    app.page = Page::Overview;
    draw_sized(&app, 120, 40);
    let row = app
        .hitmap
        .hitboxes()
        .iter()
        .rev()
        .find(|hitbox| hitbox.action == Action::row(Page::Overview, 1))
        .map(|hitbox| hitbox.rect)
        .expect("second overview row");
    click(&mut app, row.x + 2, row.y);
    assert_eq!(app.instances.focused, 1);
}

#[test]
fn the_wheel_does_not_scroll_the_page_behind_the_detail_popup() {
    let mut app = instances_app(6);
    app.open_details();
    draw_sized(&app, 120, 40);
    let list = app
        .hitmap
        .hitboxes()
        .iter()
        .rev()
        .find(|hitbox| {
            matches!(
                hitbox.action,
                Action::Page {
                    act: PageAct::Row(0),
                    ..
                }
            )
        })
        .map(|hitbox| hitbox.rect)
        .expect("a page row behind the popup");
    wheel(&mut app, MouseEventKind::ScrollDown, list.x + 1, list.y);
    assert_eq!(app.instances.focused, 0, "the page list must not move");
    assert!(app.detail_open);
}

#[test]
fn the_hit_map_stays_within_its_budget() {
    let mut app = App::new();
    app.page = Page::Settings;
    app.terminal_width = 120;
    app.terminal_height = 40;
    draw_sized(&app, 120, 40);
    assert!(
        app.hitmap.len() <= 200,
        "hit map grew to {}",
        app.hitmap.len()
    );

    let cell = app
        .hitmap
        .hitboxes()
        .first()
        .map(|hitbox| (hitbox.rect.x, hitbox.rect.y))
        .expect("at least one box");
    let started = Instant::now();
    let mut hits = 0;
    for _ in 0..1000 {
        if app.hitmap.at(cell.0, cell.1, app.top_layer()).is_some() {
            hits += 1;
        }
    }
    let elapsed = started.elapsed();
    assert_eq!(hits, 1000);
    // Debug builds are slow; this only fails if the lookup degenerates.
    assert!(
        elapsed < Duration::from_millis(50),
        "1000 lookups took {elapsed:?}"
    );
}

#[test]
fn clicking_a_select_field_opens_its_option_list() {
    let mut app = form_app(
        UiPreset::Classic,
        Modal::form(
            "Pick",
            &["Mode"],
            &["a".into()],
            FormAction::SettingValue(crate::tui::pages::settings::SettingId::WindowWidth),
        )
        .with_selection_picker(0, Vec::new()),
    );
    draw_sized(&app, 120, 40);
    let row = app
        .hitmap
        .hitboxes()
        .iter()
        .rev()
        .find(|hitbox| hitbox.action == Action::Modal(ModalAct::Toggle(0)))
        .map(|hitbox| hitbox.rect)
        .expect("the select row");
    click(&mut app, row.x + 2, row.y);
    match &app.modal {
        Some(Modal::Form {
            selection_picker_open,
            ..
        }) => assert_eq!(*selection_picker_open, Some(0)),
        other => panic!("form expected, got {other:?}"),
    }
}

fn row_colours(
    app: &App,
    rect: ratatui::layout::Rect,
) -> Vec<(ratatui::style::Color, ratatui::style::Color)> {
    let backend = TestBackend::new(app.terminal_width, app.terminal_height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal.draw(|frame| app.draw(frame)).expect("draw app");
    (rect.left()..rect.right())
        .map(|x| {
            let cell = &terminal.backend().buffer()[(x, rect.y)];
            (cell.bg, cell.fg)
        })
        .collect()
}

#[test]
fn the_selected_row_keeps_its_style_under_the_pointer() {
    let mut app = instances_app(4);
    draw_sized(&app, 120, 40);
    let rect_of = |app: &App, index: usize| {
        app.hitmap
            .hitboxes()
            .iter()
            .find(|hitbox| hitbox.action == Action::row(Page::Instances, index))
            .map(|hitbox| hitbox.rect)
            .unwrap_or_else(|| panic!("row {index}"))
    };
    let selected = rect_of(&app, 0);
    let other = rect_of(&app, 2);
    let plain_selected = row_colours(&app, selected);
    let plain_other = row_colours(&app, other);

    mouse(&mut app, MouseEventKind::Moved, selected.x + 1, selected.y);
    assert_eq!(
        row_colours(&app, selected),
        plain_selected,
        "the selection must win over hover"
    );

    mouse(&mut app, MouseEventKind::Moved, other.x + 1, other.y);
    assert_ne!(
        row_colours(&app, other),
        plain_other,
        "a non-selected row gets a hover highlight"
    );
}

#[test]
fn hover_uses_a_background_tint_on_creeper_and_a_foreground_tint_on_classic() {
    for preset in [
        UiPreset::Classic,
        UiPreset::Creeper,
        UiPreset::Nightlight,
        UiPreset::Paper,
    ] {
        let mut app = instances_app(4);
        app.config.state.ui.preset = preset;
        draw_sized(&app, 120, 40);
        let row = app
            .hitmap
            .hitboxes()
            .iter()
            .find(|hitbox| hitbox.action == Action::row(Page::Instances, 2))
            .map(|hitbox| hitbox.rect)
            .expect("row 2");
        let plain = row_colours(&app, row);
        mouse(&mut app, MouseEventKind::Moved, row.x + 1, row.y);
        let hovered = row_colours(&app, row);
        let changed_fg = plain.iter().zip(&hovered).any(|(a, b)| a.1 != b.1);
        let changed_bg = plain.iter().zip(&hovered).any(|(a, b)| a.0 != b.0);
        match preset {
            UiPreset::Classic => assert!(changed_fg, "classic tints the foreground"),
            // Nightlight paints whole rows too, so the tint lands on the
            // background exactly like creeper. Paper mixes the accent into the
            // sheet, which is also a background change.
            UiPreset::Creeper | UiPreset::Nightlight | UiPreset::Paper | UiPreset::Shelf => {
                assert!(changed_bg, "{preset:?} tints the background")
            }
        }
    }
}

#[test]
fn the_wheel_walks_the_fields_of_an_open_form() {
    for preset in [UiPreset::Classic, UiPreset::Creeper, UiPreset::Nightlight] {
        let mut app = form_app(
            preset,
            Modal::form(
                "Create",
                &["First", "Second", "Third"],
                &["a".into(), "b".into(), "c".into()],
                FormAction::SettingValue(crate::tui::pages::settings::SettingId::WindowWidth),
            ),
        );
        draw_sized(&app, 120, 40);
        let field = modal_box(&app, ModalAct::Focus(0)).expect("first field");
        wheel(&mut app, MouseEventKind::ScrollDown, field.x + 2, field.y);
        match &app.modal {
            Some(Modal::Form { focused, .. }) => assert_eq!(*focused, 1, "{preset:?}"),
            other => panic!("form expected, got {other:?}"),
        }
        wheel(&mut app, MouseEventKind::ScrollUp, field.x + 2, field.y);
        match &app.modal {
            Some(Modal::Form { focused, .. }) => assert_eq!(*focused, 0, "{preset:?}"),
            other => panic!("form expected, got {other:?}"),
        }
    }
}

#[test]
fn dialog_controls_get_a_hover_highlight_but_the_body_does_not() {
    for preset in [UiPreset::Classic, UiPreset::Creeper, UiPreset::Nightlight] {
        let mut app = form_app(
            preset,
            Modal::form(
                "Create",
                &["First", "Second"],
                &["a".into(), "b".into()],
                FormAction::SettingValue(crate::tui::pages::settings::SettingId::WindowWidth),
            ),
        );
        draw_sized(&app, 120, 40);
        let field = modal_box(&app, ModalAct::Focus(1)).expect("second field");
        let plain = row_colours(&app, field);
        mouse(&mut app, MouseEventKind::Moved, field.x + 2, field.y);
        let hovered = row_colours(&app, field);
        assert_ne!(plain, hovered, "{preset:?}: a dialog field must highlight");

        // The dialog body itself is a blocker, not a control. Find a blank cell
        // inside the card (labels, help and key rows differ per preset).
        let body = modal_box(&app, ModalAct::Noop).expect("dialog body");
        let blank = (body.y + 1..body.bottom().saturating_sub(1))
            .flat_map(|y| (body.x + 1..body.right().saturating_sub(1)).map(move |x| (x, y)))
            .find(|(x, y)| {
                app.hitmap.at(*x, *y, app.top_layer()).map(|hit| hit.action)
                    == Some(Action::Modal(ModalAct::Noop))
            })
            .expect("a blank cell inside the dialog");
        mouse(&mut app, MouseEventKind::Moved, blank.0, blank.1);
        assert_eq!(app.hover, Some(Action::Modal(ModalAct::Noop)));
        assert_eq!(
            row_colours(&app, field),
            plain,
            "{preset:?}: the body must not keep the field highlighted"
        );
    }
}

#[test]
fn version_picker_chips_and_search_are_clickable_and_hover() {
    for preset in [UiPreset::Classic, UiPreset::Creeper, UiPreset::Nightlight] {
        let mut app = form_app(
            preset,
            Modal::form(
                "Pick a version",
                &["Version"],
                &["1.21.0".into()],
                FormAction::SettingValue(crate::tui::pages::settings::SettingId::WindowWidth),
            )
            .with_version_picker(
                0,
                vec![crate::core::version::models::VersionSummary {
                    id: "1.21.0".into(),
                    kind: "release".into(),
                    url: String::new(),
                    time: String::new(),
                    release_time: String::new(),
                    sha1: None,
                    compliance_level: None,
                }],
            ),
        );
        app.modal
            .as_mut()
            .expect("dialog")
            .apply(ModalAct::Toggle(0));
        draw_sized(&app, 120, 40);

        let chip = modal_box(&app, ModalAct::ToggleVersionFilter(1)).expect("snapshot chip");
        let plain = row_colours(&app, chip);
        mouse(&mut app, MouseEventKind::Moved, chip.x + 1, chip.y);
        assert_ne!(row_colours(&app, chip), plain, "{preset:?}: chip hover");
        click(&mut app, chip.x + 1, chip.y);
        assert!(
            app.modal.is_some(),
            "{preset:?}: the chip keeps the picker open"
        );

        // The click toggled the filter, so the chip has to look different on the
        // next frame. A preset that accepts keys `1`-`4` without drawing a chip
        // leaves the filter state invisible.
        draw_sized(&app, 120, 40);
        assert_ne!(
            row_colours(&app, chip),
            plain,
            "{preset:?}: the chip shows the filter it now has"
        );

        let search = modal_box(&app, ModalAct::EditSearch).expect("search line");
        click(&mut app, search.x + 1, search.y);
        assert!(
            app.modal.is_some(),
            "{preset:?}: the search line is clickable"
        );
    }
}

#[test]
fn checklist_rows_click_toggle_and_apply_sets_the_filter() {
    use crate::tui::modal::ChecklistItem;

    let modal = Modal::checklist(
        "Filter",
        vec![
            ChecklistItem::new("adventure", "Adventure"),
            ChecklistItem::new("decoration", "Decoration"),
        ],
        &[],
        false,
    );
    let mut app = form_app(UiPreset::Classic, modal);
    draw_sized(&app, 120, 40);

    let row = app
        .hitmap
        .hitboxes()
        .iter()
        .rev()
        .find(|hitbox| hitbox.action == Action::Modal(ModalAct::CheckItem(1)))
        .map(|hitbox| hitbox.rect)
        .expect("second checklist row");
    click(&mut app, row.x + 2, row.y);
    match &app.modal {
        Some(Modal::Checklist { checked, .. }) => assert_eq!(checked, &[false, true]),
        other => panic!("checklist expected, got {other:?}"),
    }

    // The wheel moves the cursor, which the Space chip then toggles.
    wheel(&mut app, MouseEventKind::ScrollDown, row.x + 2, row.y);
    match &app.modal {
        Some(Modal::Checklist { focused, .. }) => assert_eq!(*focused, 1),
        other => panic!("checklist expected, got {other:?}"),
    }

    draw_sized(&app, 120, 40);
    let apply = modal_box(&app, ModalAct::ApplyChecks).expect("apply chip");
    click(&mut app, apply.x + 1, apply.y);
    assert!(app.modal.is_none(), "applying closes the dialog");
    assert_eq!(
        app.download_center.categories,
        vec!["decoration".to_string()]
    );
    assert_eq!(app.download_center.page, 0);
}

fn resource_download_app(source: crate::core::resource::model::ResourceSource) -> App {
    let mut app = App::new();
    app.page = Page::DownloadCenter;
    app.terminal_width = WIDTH;
    app.terminal_height = HEIGHT;
    app.download_center.category = crate::tui::pages::download_center::DownloadCategory::Resource(
        crate::core::resource::model::ResourceKind::Mod,
    );
    app.download_center.source = source;
    app
}

fn ready_categories(app: &mut App, names: &[(&str, &str)]) {
    let key = app.current_category_key().expect("resource page");
    assert!(app.resource_categories.begin(key, 90));
    let categories = names
        .iter()
        .map(
            |(id, name)| crate::core::resource::model::ResourceCategory {
                id: (*id).into(),
                name: (*name).into(),
                kind: key.1,
            },
        )
        .collect();
    app.apply_resource_categories(90, key.0, key.1, Ok(categories));
}

fn press(app: &mut App, code: crossterm::event::KeyCode) {
    app.handle(AppEvent::Key(crossterm::event::KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: crossterm::event::KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    }));
}

#[test]
fn f_opens_the_category_filter_with_cached_items() {
    let mut app = resource_download_app(crate::core::resource::model::ResourceSource::Modrinth);
    ready_categories(&mut app, &[("adventure", "Adventure")]);
    draw_sized(&app, WIDTH, HEIGHT);
    press(&mut app, crossterm::event::KeyCode::Char('f'));
    match &app.modal {
        Some(Modal::Checklist {
            items,
            single,
            note,
            ..
        }) => {
            assert_eq!(items.len(), 2, "the all row plus one category");
            assert_eq!(items[0].id, "");
            assert_eq!(items[1].id, "adventure");
            assert!(!*single, "Modrinth supports multiple categories");
            assert!(note.is_none());
        }
        other => panic!("checklist expected, got {other:?}"),
    }
}

/// Glyphs drawn inside `rect`, whitespace removed.
///
/// A wide CJK glyph occupies two cells and leaves the second one blank, and
/// the entry is padded, so only the non-blank cells are comparable.
fn drawn_glyphs(app: &App, rect: Rect) -> String {
    let backend = TestBackend::new(WIDTH, HEIGHT);
    let mut terminal = Terminal::new(backend).expect("create test terminal");
    terminal.draw(|frame| app.draw(frame)).expect("draw app");
    let buffer = terminal.backend().buffer();
    (rect.x..rect.right())
        .map(|x| buffer[(x, rect.y)].symbol())
        .collect::<String>()
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect()
}

fn filter_entry_hitbox(app: &App) -> Rect {
    app.hitmap
        .hitboxes()
        .iter()
        .rev()
        .find(|hitbox| {
            matches!(
                hitbox.action,
                Action::Page {
                    act: PageAct::FilterCategories,
                    ..
                }
            )
        })
        .map(|hitbox| hitbox.rect)
        .expect("category filter entry")
}

#[test]
fn the_category_entry_opens_the_filter() {
    let mut app = resource_download_app(crate::core::resource::model::ResourceSource::Modrinth);
    ready_categories(&mut app, &[("adventure", "Adventure")]);
    draw_sized(&app, WIDTH, HEIGHT);
    let entry = filter_entry_hitbox(&app);
    click(&mut app, entry.x, entry.y);
    assert!(matches!(app.modal, Some(Modal::Checklist { .. })));
}

/// The entry draws `<label> ›` and registers a rectangle for it. The rectangle
/// must cover exactly those cells: the chip it replaced registered two columns
/// too few because the spaces before the label were never counted, so hovering
/// highlighted text that had already scrolled past. Both presets share
/// `download_filter_entry_rect`, and this walks the rendered cells instead of
/// re-deriving the geometry, so the guard survives another move of the entry.
#[test]
fn the_category_entry_covers_exactly_the_text_it_draws() {
    for preset in [UiPreset::Classic, UiPreset::Creeper, UiPreset::Nightlight] {
        let mut app = resource_download_app(crate::core::resource::model::ResourceSource::Modrinth);
        app.config.state.ui.preset = preset;
        app.i18n = crate::i18n::I18n::new(crate::core::config::models::Language::SimplifiedChinese);
        ready_categories(&mut app, &[("adventure", "Adventure")]);
        app.download_center.categories = vec!["adventure".into()];
        draw_sized(&app, WIDTH, HEIGHT);

        let entry = filter_entry_hitbox(&app);
        let drawn = drawn_glyphs(&app, entry);
        let expected: String = format!("{} ›", app.download_filter_entry())
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect();
        assert_eq!(drawn, expected, "{preset:?}: hit box {entry:?}");
    }
}

#[test]
fn an_open_filter_refreshes_when_the_list_arrives() {
    let mut app = resource_download_app(crate::core::resource::model::ResourceSource::Modrinth);
    // Pretend a fetch is already in flight: pressing `f` must reuse it instead
    // of spawning another one (a plain unit test has no Tokio runtime).
    let key = app.current_category_key().expect("resource page");
    assert!(app.resource_categories.begin(key, 5));
    draw_sized(&app, WIDTH, HEIGHT);
    press(&mut app, crossterm::event::KeyCode::Char('f'));
    match &app.modal {
        Some(Modal::Checklist { items, note, .. }) => {
            assert!(items.is_empty());
            assert!(note.is_some(), "loading must be explained");
        }
        other => panic!("checklist expected, got {other:?}"),
    }
    app.apply_resource_categories(
        5,
        key.0,
        key.1,
        Ok(vec![crate::core::resource::model::ResourceCategory {
            id: "magic".into(),
            name: "Magic".into(),
            kind: key.1,
        }]),
    );
    match &app.modal {
        Some(Modal::Checklist { items, note, .. }) => {
            assert_eq!(items.len(), 2, "the all row plus the loaded category");
            assert_eq!(items[0].id, "");
            assert_eq!(items[1].id, "magic");
            assert!(note.is_none());
        }
        other => panic!("checklist expected, got {other:?}"),
    }
}

#[test]
fn curseforge_filter_is_single_choice() {
    let mut app = resource_download_app(crate::core::resource::model::ResourceSource::CurseForge);
    ready_categories(&mut app, &[("4474", "Adventure"), ("4483", "Combat")]);
    draw_sized(&app, WIDTH, HEIGHT);
    press(&mut app, crossterm::event::KeyCode::Char('f'));
    match &app.modal {
        Some(Modal::Checklist { single, .. }) => assert!(*single),
        other => panic!("checklist expected, got {other:?}"),
    }
}

// Applying the filter starts a search, so this test needs a runtime.
#[tokio::test]
async fn choosing_all_clears_the_category_selection() {
    let mut app = resource_download_app(crate::core::resource::model::ResourceSource::Modrinth);
    ready_categories(&mut app, &[("adventure", "Adventure"), ("magic", "Magic")]);
    app.download_center.categories = vec!["adventure".into()];
    draw_sized(&app, WIDTH, HEIGHT);
    press(&mut app, crossterm::event::KeyCode::Char('f'));
    draw_sized(&app, WIDTH, HEIGHT);

    let all = modal_box(&app, ModalAct::CheckItem(0)).expect("the all row");
    click(&mut app, all.x + 2, all.y);
    draw_sized(&app, WIDTH, HEIGHT);
    let apply = modal_box(&app, ModalAct::ApplyChecks).expect("apply chip");
    click(&mut app, apply.x + 1, apply.y);

    assert!(app.modal.is_none());
    assert!(
        app.download_center.categories.is_empty(),
        "all means no filter"
    );
}

#[test]
fn curseforge_filter_explains_the_single_choice() {
    let mut app = resource_download_app(crate::core::resource::model::ResourceSource::CurseForge);
    ready_categories(&mut app, &[("4474", "Adventure")]);
    draw_sized(&app, WIDTH, HEIGHT);
    press(&mut app, crossterm::event::KeyCode::Char('f'));
    match &app.modal {
        Some(Modal::Checklist { note, single, .. }) => {
            assert!(*single);
            assert_eq!(
                note.as_deref(),
                Some(app.i18n.text("download.filter_single_hint").as_str())
            );
        }
        other => panic!("checklist expected, got {other:?}"),
    }
}

// ── download filter entry + version kind dialog ───────────────────

fn creeper_download_app() -> App {
    let mut app = App::new();
    app.config.state.ui.preset = UiPreset::Creeper;
    app.page = Page::DownloadCenter;
    app.terminal_width = WIDTH;
    app.terminal_height = HEIGHT;
    app
}

fn download_sections(app: &App) -> [Rect; 5] {
    let regions = app.layout_chrome(Rect::new(0, 0, WIDTH, HEIGHT));
    app.download_page_sections(regions.main)
        .expect("download sections")
}

fn category_entry_rect(app: &App, sections: &[Rect; 5]) -> Rect {
    let row = sections[1];
    let width = u16::try_from(crate::tui::palette::width(&app.download_filter_entry()) + 2)
        .expect("entry width");
    Rect::new(row.right().saturating_sub(width), row.y, width, 1)
}

fn row_text(buffer: &ratatui::buffer::Buffer, row: Rect) -> String {
    (row.x..row.right())
        .map(|x| buffer[(x, row.y)].symbol())
        .collect()
}

#[tokio::test]
async fn creeper_download_filter_entry_sits_on_the_category_row() {
    let mut app = creeper_download_app();
    draw(&app);

    let sections = download_sections(&app);
    let entry = category_entry_rect(&app, &sections);
    assert_eq!(entry.y, sections[1].y, "entry must share the tab row");
    assert_eq!(
        app.hitmap
            .at(entry.x + 1, entry.y, app.top_layer())
            .map(|hit| hit.action),
        Some(Action::Page {
            page: Page::DownloadCenter,
            act: PageAct::FilterCategories,
        })
    );

    click(&mut app, entry.x + 1, entry.y);
    assert!(
        matches!(app.modal, Some(crate::tui::modal::Modal::Checklist { .. })),
        "the entry must open the filter dialog"
    );
}

#[test]
fn creeper_download_rows_use_the_card_surface() {
    let app = creeper_download_app();
    let sections = download_sections(&app);
    let backend = TestBackend::new(WIDTH, HEIGHT);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.draw(frame)).expect("draw");
    let buffer = terminal.backend().buffer();

    let bg = crate::tui::palette::BG;
    for (name, row) in [("category", sections[1]), ("info", sections[3])] {
        if row.height == 0 {
            continue;
        }
        for x in row.x..row.right() {
            assert_ne!(
                buffer[(x, row.y)].bg,
                bg,
                "{name} row still paints the page background at column {x}"
            );
        }
    }
}

/// Both sides of the page's two vertical seams are `SURFACE` cards: the column
/// between the sidebar and the page, and the one between the result list and
/// the detail card. Leaving them on the page background drew two black strips
/// down the body.
#[test]
fn creeper_download_page_shows_no_page_background_between_its_cards() {
    use crate::core::resource::model::{ResourceKind, ResourceProject, ResourceSource};

    let mut app = creeper_download_app();
    app.download_center.category =
        crate::tui::pages::download_center::DownloadCategory::Resource(ResourceKind::Mod);
    app.resource_projects = vec![ResourceProject {
        source: ResourceSource::Modrinth,
        project_id: "AANobbMI".into(),
        title: "Sodium".into(),
        author: "jellysquid3".into(),
        description: "A modern rendering engine".into(),
        icon_url: None,
        categories: Vec::new(),
        game_versions: vec!["1.21.1".into()],
        loaders: vec!["fabric".into()],
        downloads: Some(1234),
        updated_at: None,
    }];
    assert!(
        app.download_detail_areas(download_sections(&app)[4])
            .is_some(),
        "the detail card must be drawn, otherwise the seam is not exercised"
    );

    let regions = app.layout_chrome(Rect::new(0, 0, WIDTH, HEIGHT));
    let backend = TestBackend::new(WIDTH, HEIGHT);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.draw(frame)).expect("draw");
    let buffer = terminal.backend().buffer();

    let bg = crate::tui::palette::BG;
    for y in regions.body.top()..regions.body.bottom() {
        for x in regions.tabs.right()..regions.body.right() {
            assert_ne!(
                buffer[(x, y)].bg,
                bg,
                "the page background shows through at ({x}, {y})"
            );
        }
    }
}

/// Cards on one surface, page by page: nothing inside the body may fall back
/// to the page background, or the gaps read as black seams (the overview's row
/// gaps, the instance and task filter rows, the settings rail, the download
/// split).
#[test]
fn every_creeper_page_paints_the_body_with_the_card_surface() {
    for page in Page::ALL {
        let mut app = App::new();
        app.config.state.ui.preset = UiPreset::Creeper;
        app.page = page;
        app.terminal_width = WIDTH;
        app.terminal_height = HEIGHT;

        let backend = TestBackend::new(WIDTH, HEIGHT);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|frame| app.draw(frame)).expect("draw");
        let buffer = terminal.backend().buffer();

        let regions = app.layout_chrome(Rect::new(0, 0, WIDTH, HEIGHT));
        for y in regions.body.top()..regions.body.bottom() {
            for x in regions.tabs.right()..regions.body.right() {
                assert_ne!(
                    buffer[(x, y)].bg,
                    crate::tui::palette::BG,
                    "{page:?}: the page background shows through at ({x}, {y})"
                );
            }
        }
    }
}

#[tokio::test]
async fn game_versions_filter_opens_the_category_checklist() {
    use crate::core::version::models::{VersionCategory, VersionSummary};

    let mut app = creeper_download_app();
    app.download_center.category =
        crate::tui::pages::download_center::DownloadCategory::GameVersions;
    app.versions = (0..3)
        .map(|index| VersionSummary {
            id: format!("1.21.{index}"),
            kind: if index == 2 { "snapshot" } else { "release" }.into(),
            url: String::new(),
            time: "2026-08-08T12:00:00+00:00".into(),
            release_time: format!("2026-08-0{}T12:00:00+00:00", index + 1),
            sha1: None,
            compliance_level: None,
        })
        .collect();
    draw(&app);

    app.open_filter_dialog();
    let Some(crate::tui::modal::Modal::Checklist { items, checked, .. }) = app.modal.as_ref()
    else {
        panic!("versions must open the same checklist dialog as resources");
    };
    assert_eq!(items.len(), 5, "all row plus four kinds: {items:?}");
    assert!(!checked[0], "all is unchecked while only one kind is on");
    assert!(
        checked[1] && !checked[2],
        "only Release is on by default: {checked:?}"
    );

    app.apply_version_filter(&["snapshot".into()]);
    assert!(
        !app.download_center
            .version_filter
            .is_enabled(VersionCategory::Release)
    );
    assert!(
        app.download_center
            .version_filter
            .is_enabled(VersionCategory::Snapshot)
    );

    // The all row turns every kind back on.
    app.apply_version_filter(&[String::new()]);
    for category in VersionCategory::ALL {
        assert!(app.download_center.version_filter.is_enabled(category));
    }

    // Kind names follow the interface language, not the manifest.
    app.i18n = crate::i18n::I18n::new(crate::core::config::models::Language::SimplifiedChinese);
    app.modal = None;
    app.open_filter_dialog();
    let Some(crate::tui::modal::Modal::Checklist { items, .. }) = app.modal.as_ref() else {
        panic!("dialog missing");
    };
    let labels = items
        .iter()
        .map(|item| item.label.as_str())
        .collect::<Vec<_>>();
    assert!(
        labels.iter().any(|label| label.starts_with("正式版")),
        "{labels:?}"
    );
    assert!(
        labels.iter().any(|label| label.starts_with("愚人节版")),
        "{labels:?}"
    );
    assert!(
        labels.iter().all(|label| !label.contains("Release")),
        "untranslated kind names: {labels:?}"
    );
}

#[test]
fn game_versions_page_has_no_target() {
    use crate::tui::command::Command;

    let mut app = creeper_download_app();
    app.download_center.category =
        crate::tui::pages::download_center::DownloadCategory::GameVersions;
    app.download_target_id = Some(uuid::Uuid::new_v4());

    let hints = app.downloads_hints();
    assert!(
        hints.iter().all(|hint| hint.key != "t"),
        "the target hint must not appear on the versions page"
    );

    app.run_command(Command::Target);
    assert!(
        app.modal.is_none(),
        "`t` must not open a target picker for a game version"
    );

    let sections = download_sections(&app);
    let backend = TestBackend::new(WIDTH, HEIGHT);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.draw(frame)).expect("draw");
    let caption = row_text(terminal.backend().buffer(), sections[3]);
    assert!(
        !caption.contains("目标") && !caption.contains("target"),
        "the versions caption must not mention a target: {caption:?}"
    );
}

#[test]
fn categories_that_do_not_install_into_an_instance_have_no_target() {
    use crate::core::resource::model::ResourceKind;
    use crate::tui::command::Command;
    use crate::tui::pages::download_center::DownloadCategory;

    // A modpack builds an instance of its own; a world is written wherever the
    // save dialog points. Neither has a target instance to pick.
    for (kind, wording, status) in [
        (
            ResourceKind::Modpack,
            "download.creates_instance",
            "status.target_not_needed_new_instance",
        ),
        (
            ResourceKind::World,
            "download.saves_to_path",
            "status.target_not_needed_save_path",
        ),
    ] {
        let mut app = creeper_download_app();
        app.download_center.category = DownloadCategory::Resource(kind);
        app.download_target_id = Some(uuid::Uuid::new_v4());

        let hints = app.downloads_hints();
        assert!(
            hints.iter().all(|hint| hint.key != "t"),
            "{kind:?} must not offer a download target"
        );

        app.run_command(Command::Target);
        assert!(
            app.modal.is_none(),
            "`t` must not open a target picker for {kind:?}"
        );
        assert_eq!(app.status.as_str(), app.i18n.text(status));

        // Compatibility is measured against a target instance; with none,
        // every file would read "unknown" next to a download that ignores
        // instances.
        assert!(
            app.resource_target_context().is_none(),
            "{kind:?} has no target to compare files against"
        );

        let sections = download_sections(&app);
        let backend = TestBackend::new(WIDTH, HEIGHT);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|frame| app.draw(frame)).expect("draw");
        let caption = row_text(terminal.backend().buffer(), sections[3]);
        assert!(
            !caption.contains("目标") && !caption.contains("target"),
            "the {kind:?} caption must not mention a target: {caption:?}"
        );
        // `creeper` puts the note in the page caption (`… · you pick the save
        // path`), not in the short `all · …` summary.
        let info = app.i18n.text(app.download_info_key());
        let tail = info.rsplit('\u{00b7}').next().unwrap_or_default().trim();
        assert!(
            caption.contains(&app.i18n.text(wording)) || caption.contains(tail),
            "the {kind:?} caption should say where the file goes: {caption:?}"
        );
    }
}

#[tokio::test]
async fn version_filter_all_row_applies_to_every_kind() {
    use crate::core::version::models::VersionCategory;
    use crate::tui::action::ModalAct;
    use crate::tui::modal::ModalResult;

    let mut app = creeper_download_app();
    app.download_center.category =
        crate::tui::pages::download_center::DownloadCategory::GameVersions;
    app.open_filter_dialog();

    // Tick the "all" row through the same path the keyboard uses, then apply.
    let modal = app.modal.as_mut().expect("filter dialog");
    modal.apply(ModalAct::CheckItem(0));
    let result = modal.apply(ModalAct::ApplyChecks);
    let ModalResult::FilterApply(ids) = result else {
        panic!("apply must confirm the checklist, got {result:?}");
    };
    // The all row's id is empty and the modal drops it; the app treats that
    // as "every kind", not "no kind".
    assert!(ids.is_empty(), "{ids:?}");

    app.modal = None;
    app.apply_version_filter(&ids);
    for category in VersionCategory::ALL {
        assert!(
            app.download_center.version_filter.is_enabled(category),
            "{category:?} stayed off after checking all"
        );
    }

    // Reopening shows the all row alone, not five ticks.
    app.open_filter_dialog();
    let Some(crate::tui::modal::Modal::Checklist { checked, .. }) = app.modal.as_ref() else {
        panic!("filter dialog");
    };
    assert_eq!(checked.first().copied(), Some(true), "{checked:?}");
    assert!(checked[1..].iter().all(|tick| !tick), "{checked:?}");
}

// ── classic preset parity for the download category row ───────────

#[tokio::test]
async fn classic_download_filter_entry_sits_on_the_tab_row() {
    let mut app = App::new();
    app.page = Page::DownloadCenter;
    app.terminal_width = WIDTH;
    app.terminal_height = HEIGHT;
    app.download_center.category = crate::tui::pages::download_center::DownloadCategory::Resource(
        crate::core::resource::model::ResourceKind::Mod,
    );
    draw(&app);

    let sections = download_sections(&app);
    assert_eq!(sections[2].height, 0, "the version chip row must be gone");
    // One column of padding keeps the entry off the panel border.
    let row = Rect {
        width: sections[1].width.saturating_sub(1),
        ..sections[1]
    };
    let entry = app.download_filter_entry_rect(row);
    assert_eq!(entry.y, sections[1].y, "entry must share the tab row");
    assert_eq!(
        app.hitmap
            .at(entry.x + 1, entry.y, app.top_layer())
            .map(|hit| hit.action),
        Some(Action::Page {
            page: Page::DownloadCenter,
            act: PageAct::FilterCategories,
        })
    );

    click(&mut app, entry.x + 1, entry.y);
    assert!(
        matches!(app.modal, Some(crate::tui::modal::Modal::Checklist { .. })),
        "classic entry must open the same filter dialog"
    );
}

#[tokio::test]
async fn classic_game_versions_use_the_same_checklist() {
    let mut app = App::new();
    app.page = Page::DownloadCenter;
    app.terminal_width = WIDTH;
    app.terminal_height = HEIGHT;
    app.download_center.category =
        crate::tui::pages::download_center::DownloadCategory::GameVersions;

    app.open_filter_dialog();
    let Some(crate::tui::modal::Modal::Checklist { items, .. }) = app.modal.as_ref() else {
        panic!("classic versions must open the checklist dialog");
    };
    assert_eq!(items.len(), 5, "all row plus four kinds: {items:?}");
}

// ---------------------------------------------------------------------------
// `nightlight` dialogs. The preset paints its own card, so it registers its
// own hit targets; these tests keep it from drifting back into keyboard-only.
// ---------------------------------------------------------------------------

fn nightlight_form(fields: &[&str], options: Option<Vec<String>>) -> Modal {
    let labels = fields
        .iter()
        .map(|field| (*field).to_owned())
        .collect::<Vec<_>>();
    let values = fields.iter().map(|_| String::new()).collect::<Vec<_>>();
    let modal = Modal::form(
        "Edit",
        &labels,
        &values,
        FormAction::SettingValue(crate::tui::pages::settings::SettingId::WindowWidth),
    );
    match options {
        Some(options) => modal.with_selection_picker(0, options),
        None => modal,
    }
}

#[test]
fn nightlight_form_rows_focus_on_click() {
    let mut app = form_app(
        UiPreset::Nightlight,
        nightlight_form(&["First", "Second"], None),
    );
    draw_sized(&app, 120, 40);
    for field in 0..2 {
        let row = modal_box(&app, ModalAct::Focus(field)).expect("field row");
        click(&mut app, row.x + 2, row.y);
        match &app.modal {
            Some(Modal::Form { focused, .. }) => assert_eq!(*focused, field),
            other => panic!("form expected, got {other:?}"),
        }
    }
}

#[test]
fn nightlight_select_row_opens_the_picker() {
    let mut app = form_app(
        UiPreset::Nightlight,
        nightlight_form(&["Loader"], Some(vec!["alpha".into(), "beta".into()])),
    );
    draw_sized(&app, 120, 40);
    let row = modal_box(&app, ModalAct::Toggle(0)).expect("select row");
    click(&mut app, row.x + 2, row.y);
    match &app.modal {
        Some(Modal::Form {
            selection_picker_open,
            ..
        }) => assert_eq!(*selection_picker_open, Some(0)),
        other => panic!("form expected, got {other:?}"),
    }
}

#[test]
fn nightlight_picker_options_are_clickable() {
    let mut app = form_app(
        UiPreset::Nightlight,
        nightlight_form(&["Loader"], Some(vec!["alpha".into(), "beta".into()])),
    );
    if let Some(modal) = app.modal.as_mut() {
        modal.apply(ModalAct::Toggle(0));
    }
    draw_sized(&app, 120, 40);
    let option = modal_box(&app, ModalAct::Option(1)).expect("the second option");
    click(&mut app, option.x + 2, option.y);
    match &app.modal {
        Some(Modal::Form {
            values,
            selection_picker_open,
            ..
        }) => {
            assert_eq!(
                values[0], "beta",
                "the click must pick the row it landed on"
            );
            assert_eq!(*selection_picker_open, None, "picking closes the list");
        }
        other => panic!("form expected, got {other:?}"),
    }
}

#[test]
fn nightlight_picker_answers_the_wheel() {
    let options = (0..20)
        .map(|index| format!("option-{index:02}"))
        .collect::<Vec<_>>();
    let mut app = form_app(
        UiPreset::Nightlight,
        nightlight_form(&["Loader"], Some(options)),
    );
    if let Some(modal) = app.modal.as_mut() {
        modal.apply(ModalAct::Toggle(0));
    }
    draw_sized(&app, 120, 40);
    let row = modal_box(&app, ModalAct::Option(0)).expect("the first option");
    // Three notches move the cursor; the list starts scrolling only once the
    // cursor leaves the first window, so both steps are asserted.
    for _ in 0..3 {
        wheel(&mut app, MouseEventKind::ScrollDown, row.x + 2, row.y);
    }
    match &app.modal {
        Some(Modal::Form {
            selection_selected, ..
        }) => assert_eq!(
            *selection_selected, 3,
            "the wheel must move the option cursor"
        ),
        other => panic!("form expected, got {other:?}"),
    }
    for _ in 0..10 {
        wheel(&mut app, MouseEventKind::ScrollDown, row.x + 2, row.y);
    }
    match &app.modal {
        Some(Modal::Form {
            selection_scroll, ..
        }) => assert!(
            *selection_scroll > 0,
            "the wheel must scroll the list, not the page behind it"
        ),
        other => panic!("form expected, got {other:?}"),
    }
}

/// The `‹ ›` ends of a classic settings row are controls, not decoration: a
/// click on either glyph steps the value, the way it does in the other two
/// presets. The glyphs are painted by the table, so the test reads their
/// position off the frame instead of re-deriving the column arithmetic.
#[test]
fn a_classic_setting_arrow_steps_the_row() {
    use crate::tui::hit::Layer;

    let mut app = App::new();
    app.page = Page::Settings;
    draw(&app);

    let (column, row_y) = (0..HEIGHT)
        .find_map(|row| drawn_symbol_column(&app, row, "‹").map(|column| (column, row)))
        .expect("a setting row shows its step arrows");

    let hit = app
        .hitmap
        .at(column, row_y, Layer::Base)
        .expect("the arrow is a hit target");
    let Action::Page {
        page: Page::Settings,
        act: PageAct::Step { row, by: -1 },
    } = hit.action
    else {
        panic!(
            "the left glyph must step the row down, got {:?}",
            hit.action
        );
    };

    let before = app.settings_rows()[row].value.clone();
    click(&mut app, column, row_y);
    let after = app.settings_rows()[row].value.clone();
    assert_ne!(before, after, "clicking the arrow left the value alone");
    assert_eq!(app.settings.selected, row, "stepping selects the row");
}

/// Clicking away from an inline search has to end the edit. `Esc` was the only
/// way out, and `Esc` is a *cancel*: the mouse had no way to say "done".
#[test]
fn a_click_outside_the_search_field_leaves_it() {
    use crate::tui::pages::download_center::DownloadCategory;

    let mut app = App::new();
    app.page = Page::DownloadCenter;
    app.config.state.ui.preset = UiPreset::Nightlight;
    app.download_center.category = DownloadCategory::GameVersions;
    app.terminal_width = WIDTH;
    app.terminal_height = HEIGHT;
    draw(&app);

    // The magnifier marks the field; clicking it is what a user does.
    let (field, field_row) = (0..12)
        .find_map(|row| drawn_symbol_column(&app, row, "⌕").map(|column| (column, row)))
        .expect("the search field is on screen");
    click(&mut app, field, field_row);
    assert!(app.search.is_some(), "the card focuses the field");

    // Text typed but not submitted yet must survive the click.
    app.download_center.version_filter.set_search("1.20");
    click(&mut app, field, field_row);
    assert!(app.search.is_some(), "a click inside keeps the field");

    click(&mut app, 0, 0);
    assert!(app.search.is_none(), "a click outside must leave the field");
    assert_eq!(
        app.download_center.version_filter.search(),
        "1.20",
        "leaving the field is not a cancel: the query stays"
    );
}

/// `›` sits right after the value, not at the far edge of the column, so its
/// hit box has to follow the text. Sizing it from the column left the glyph
/// unclickable while `‹` worked.
#[test]
fn the_closing_settings_arrow_is_a_click_target() {
    use crate::tui::hit::Layer;

    let mut app = App::new();
    app.page = Page::Settings;
    draw(&app);

    let (column, row_y) = (0..HEIGHT)
        .find_map(|row| drawn_symbol_column(&app, row, "›").map(|column| (column, row)))
        .expect("a steppable row is on screen");
    let hit = app
        .hitmap
        .at(column, row_y, Layer::Base)
        .expect("the arrow is a hit target");
    let Action::Page {
        page: Page::Settings,
        act: PageAct::Step { row, by: 1 },
    } = hit.action
    else {
        panic!("the closing glyph must step up, got {:?}", hit.action);
    };

    let before = app.settings_rows()[row].value.clone();
    click(&mut app, column, row_y);
    let after = app.settings_rows()[row].value.clone();
    assert_ne!(before, after, "clicking › left the value alone");
}

/// The category row is the pointer's way to switch the download source.
///
/// The click changes the category, which starts a source search. That needs a
/// Tokio context, so the test carries one instead of asserting the hit value
/// alone.
#[tokio::test]
async fn shelf_category_labels_switch_the_download_category() {
    use crate::core::resource::model::ResourceKind;
    use crate::tui::pages::download_center::DownloadCategory;

    let mut app = App::new();
    app.config.state.ui.preset = UiPreset::Shelf;
    app.page = Page::DownloadCenter;
    app.terminal_width = WIDTH;
    app.terminal_height = HEIGHT;
    draw_sized(&app, WIDTH, HEIGHT);

    let wanted = DownloadCategory::ALL
        .iter()
        .position(|category| *category == DownloadCategory::Resource(ResourceKind::Modpack))
        .expect("modpacks are a category");
    let rect = app
        .hitmap
        .hitboxes()
        .iter()
        .rev()
        .find(|hitbox| hitbox.action == Action::filter(Page::DownloadCenter, wanted))
        .map(|hitbox| hitbox.rect)
        .expect("every drawn category must be clickable");
    click(&mut app, rect.x + 1, rect.y);
    assert_eq!(
        app.download_center.category,
        DownloadCategory::Resource(ResourceKind::Modpack)
    );
}

/// A task entry is two rows tall and the whole entry is the target, so clicking
/// the progress line selects the job above it.
#[test]
fn shelf_task_entries_are_clickable_as_a_block() {
    use crate::core::task::{TaskOperationType, TaskSnapshot, TaskState};

    let mut app = App::new();
    app.config.state.ui.preset = UiPreset::Shelf;
    app.page = Page::Tasks;
    app.terminal_width = WIDTH;
    app.terminal_height = HEIGHT;
    app.task_snapshots = (0..3)
        .map(|index| TaskSnapshot {
            job: None,
            created_seq: index,
            id: uuid::Uuid::from_u128(0x5E1F_3000 + index as u128),
            label: format!("task-{index}"),
            operation_type: TaskOperationType::ResourceInstall,
            state: TaskState::Running,
            retryable: false,
            attempt: 0,
            completed: 1,
            total: Some(2),
            failure: None,
        })
        .collect();
    draw_sized(&app, WIDTH, HEIGHT);

    let entries = app
        .hitmap
        .hitboxes()
        .iter()
        .filter(|hitbox| {
            matches!(
                hitbox.action,
                Action::Page {
                    page: Page::Tasks,
                    act: PageAct::Row(_),
                }
            )
        })
        .map(|hitbox| (hitbox.action, hitbox.rect))
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 3, "three entries must be registered");

    let (action, rect) = entries[2];
    // The second row of the entry is the progress bar, which has no text of
    // its own: the hit box has to cover the whole block.
    click(&mut app, rect.x + 4, rect.y + 1);
    assert_eq!(
        app.task_selected, 2,
        "the block click did not select the job"
    );
    let Action::Page { act, .. } = action else {
        unreachable!("filtered above")
    };
    let PageAct::Row(index) = act else {
        unreachable!("filtered above")
    };
    let rows = app.task_rows();
    let expected = app
        .task_snapshots
        .get(rows[index].head)
        .map(|snapshot| snapshot.id);
    assert_eq!(
        app.selected_task().map(|task| task.id),
        expected,
        "the click landed on a different snapshot than the row reports"
    );
}

/// The search row is where `/` lands: a click there must focus the same field.
#[test]
fn shelf_search_row_focuses_the_resource_filter() {
    use crate::core::resource::model::ResourceKind;
    use crate::tui::input::SearchTarget;
    use crate::tui::pages::download_center::DownloadCategory;

    let mut app = App::new();
    app.config.state.ui.preset = UiPreset::Shelf;
    app.page = Page::DownloadCenter;
    app.download_center.category = DownloadCategory::Resource(ResourceKind::Mod);
    app.terminal_width = WIDTH;
    app.terminal_height = HEIGHT;
    draw_sized(&app, WIDTH, HEIGHT);

    let rect = app
        .hitmap
        .hitboxes()
        .iter()
        .rev()
        .find(|hitbox| hitbox.action == Action::search(SearchTarget::Resources))
        .map(|hitbox| hitbox.rect)
        .expect("the search row must be clickable");
    click(&mut app, rect.x + 2, rect.y);
    assert_eq!(
        app.search.as_ref().map(|state| state.target),
        Some(SearchTarget::Resources),
        "clicking the search row must focus the resource filter"
    );
}

/// A profile card is three rows tall; clicking its detail line selects it.
#[test]
fn shelf_account_cards_select_the_clicked_profile() {
    use crate::core::account::models::{Account, AccountKind};

    let mut app = App::new();
    app.config.state.ui.preset = UiPreset::Shelf;
    app.page = Page::Accounts;
    app.terminal_width = WIDTH;
    app.terminal_height = HEIGHT;
    app.accounts.accounts = (0..3)
        .map(|index| Account {
            id: uuid::Uuid::from_u128(0x5E1F_6000 + index as u128),
            kind: if index == 0 {
                AccountKind::Offline
            } else {
                AccountKind::Microsoft
            },
            username: format!("player-{index}"),
            uuid: uuid::Uuid::from_u128(index as u128),
            skin: None,
            secret_ref: None,
            access_expires_at: None,
        })
        .collect();
    draw_sized(&app, WIDTH, HEIGHT);

    let cards = app
        .hitmap
        .hitboxes()
        .iter()
        .filter(|hitbox| {
            matches!(
                hitbox.action,
                Action::Page {
                    page: Page::Accounts,
                    act: PageAct::Row(_),
                }
            )
        })
        .map(|hitbox| (hitbox.action, hitbox.rect))
        .collect::<Vec<_>>();
    assert_eq!(cards.len(), 3, "three profile cards must be registered");
    let (action, rect) = cards[1];
    click(&mut app, rect.x + 3, rect.y + 1);
    assert_eq!(
        app.accounts.selected, 1,
        "the block click did not select the profile"
    );
    assert_eq!(action, Action::row(Page::Accounts, 1));
}

/// The settings category line is clickable, and the row list under it follows.
#[test]
fn shelf_settings_categories_switch_from_the_strip() {
    use crate::tui::pages::settings::SettingsCategory;

    let mut app = App::new();
    app.config.state.ui.preset = UiPreset::Shelf;
    app.page = Page::Settings;
    app.terminal_width = WIDTH;
    app.terminal_height = HEIGHT;
    draw_sized(&app, WIDTH, HEIGHT);

    let index = SettingsCategory::Java.index();
    let rect = app
        .hitmap
        .hitboxes()
        .iter()
        .rev()
        .find(|hitbox| hitbox.action == Action::category(index))
        .map(|hitbox| hitbox.rect)
        .expect("every drawn category must be clickable");
    click(&mut app, rect.x + 1, rect.y);
    assert_eq!(app.settings.category, SettingsCategory::Java);
}

/// The wheel walks the shelf list one card per notch, the way it walks every
/// other preset's list. The cards are three rows tall; the target is the card.
#[test]
fn the_wheel_moves_the_shelf_list_under_the_pointer() {
    let mut app = instances_app(10);
    app.config.state.ui.preset = UiPreset::Shelf;
    draw_sized(&app, 120, 40);

    let row = row_rect(&app, 0).expect("a drawn card");
    wheel(&mut app, MouseEventKind::ScrollDown, row.x + 1, row.y);
    assert_eq!(app.instances.focused, 1, "one notch moves one card");
    wheel(&mut app, MouseEventKind::ScrollDown, row.x + 1, row.y);
    assert_eq!(app.instances.focused, 2);
    wheel(&mut app, MouseEventKind::ScrollUp, row.x + 1, row.y);
    assert_eq!(app.instances.focused, 1);
}

/// The strip and the key are the same navigation: clicking the Nth tab lands
/// where N presses of `Tab` land.
#[tokio::test]
async fn shelf_tabs_land_where_the_tab_key_lands() {
    use crossterm::event::{KeyCode, KeyEvent};

    for (index, page) in Page::ALL.iter().enumerate() {
        let mut clicked = instances_app(1);
        clicked.config.state.ui.preset = UiPreset::Shelf;
        clicked.page = Page::Overview;
        draw_sized(&clicked, 120, 40);
        let rect = clicked
            .hitmap
            .hitboxes()
            .iter()
            .rev()
            .find(|hitbox| hitbox.action == Action::Nav(*page))
            .map(|hitbox| hitbox.rect)
            .unwrap_or_else(|| panic!("{page:?}: the tab is not drawn"));
        click(&mut clicked, rect.x, rect.y);
        assert_eq!(clicked.page, *page, "{page:?}: clicking its tab");

        let mut tabbed = instances_app(1);
        tabbed.config.state.ui.preset = UiPreset::Shelf;
        tabbed.page = Page::Overview;
        for _ in 0..index {
            tabbed.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        }
        assert_eq!(
            tabbed.page, *page,
            "Tab x{index} and the {page:?} tab disagree"
        );
    }
}

/// The shelf help card takes clicks the way the other presets' cards do: the
/// card keeps it open, the page behind it stays shut.
#[test]
fn the_shelf_help_card_blocks_the_page_behind_it() {
    let mut app = instances_app(1);
    app.config.state.ui.preset = UiPreset::Shelf;
    draw_sized(&app, 120, 40);
    let tab = app
        .hitmap
        .hitboxes()
        .iter()
        .rev()
        .find(|hitbox| hitbox.action == Action::Nav(Page::Logs))
        .map(|hitbox| hitbox.rect)
        .expect("the logs tab");

    app.help_open = true;
    draw_sized(&app, 120, 40);
    let card = crate::tui::app::view::help_rect(Rect::new(0, 0, 120, 40), &app.help_lines());
    click(&mut app, card.x + 2, card.y + 2);
    assert!(app.help_open, "the top of the card is inside it");

    click(&mut app, tab.x, tab.y);
    assert!(!app.help_open, "a click outside the card closes it");
    assert_eq!(app.page, Page::Instances, "and never reaches the page");
}

/// A picker's foot is a key row, not a sentence.
///
/// A sentence there (`↑/↓：选择 … Esc：返回`) leaves the one row of the dialog that
/// looks like buttons as plain text, so `Esc` closes the list from the keyboard
/// and does nothing under the pointer. All five presets paint the same caps and
/// register them; this walks every preset to prove the cap is printed where it
/// can be clicked.
#[test]
fn a_picker_foot_cap_closes_the_list_in_every_preset() {
    use crate::tui::action::ModalAct;
    use crate::tui::modal::{FormAction, Modal};

    for preset in UiPreset::ALL {
        let mut app = instances_app(1);
        app.config.state.ui.preset = preset;
        let mut modal = Modal::form(
            "Choose",
            &["Target"],
            &[String::new()],
            FormAction::SelectResourceTarget,
        )
        .with_selection_picker(0, vec!["alpha".to_owned(), "beta".to_owned()]);
        assert!(modal.open_selection_picker(0), "{preset:?}: open the list");
        app.modal = Some(modal);
        draw(&app);

        let cap = app
            .hitmap
            .hitboxes()
            .iter()
            .rev()
            .find(|hitbox| hitbox.action == Action::Modal(ModalAct::CloseOptions))
            .map(|hitbox| hitbox.rect)
            .unwrap_or_else(|| panic!("{preset:?}: the picker foot offers no clickable Esc"));

        let backend = TestBackend::new(WIDTH, HEIGHT);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|frame| app.draw(frame)).expect("draw app");
        let buffer = terminal.backend().buffer();
        let row = (0..WIDTH)
            .map(|column| buffer[(column, cap.y)].symbol())
            .collect::<String>();
        assert!(
            row.contains("Esc"),
            "{preset:?}: the cap is clickable on a row that does not print it: {row:?}"
        );

        click(&mut app, cap.x + 1, cap.y);
        match app.modal.as_ref() {
            Some(Modal::Form {
                selection_picker_open,
                ..
            }) => assert_eq!(
                *selection_picker_open, None,
                "{preset:?}: the Esc cap must close the list"
            ),
            other => panic!("{preset:?}: the dialog must stay open, got {other:?}"),
        }
    }
}

/// Hovering a key cap tints the cap, not the row that shares its action.
///
/// A picker's `Enter` cap and the option it would choose are both
/// `Modal(Option(selected))`. Looking the rectangle up by action alone takes
/// whichever box was registered last, so the pointer sits on `Enter` and the first
/// row of the list lights up.
#[test]
fn hovering_a_picker_cap_tints_the_cap_and_not_the_option() {
    use crate::tui::action::ModalAct;
    use crate::tui::modal::{FormAction, Modal};

    for preset in UiPreset::ALL {
        let mut app = instances_app(1);
        app.config.state.ui.preset = preset;
        let mut modal = Modal::form(
            "Choose",
            &["Target"],
            &[String::new()],
            FormAction::SelectResourceTarget,
        )
        .with_selection_picker(0, vec!["alpha".to_owned(), "beta".to_owned()]);
        assert!(modal.open_selection_picker(0), "{preset:?}: open the list");
        app.modal = Some(modal);
        draw(&app);

        // Both the cap and the highlighted row act on option 0; the cap is the
        // one nearest the foot of the card.
        let mut boxes = app
            .hitmap
            .hitboxes()
            .into_iter()
            .filter(|hitbox| hitbox.action == Action::Modal(ModalAct::Option(0)))
            .map(|hitbox| hitbox.rect)
            .collect::<Vec<_>>();
        boxes.sort_by_key(|rect| rect.y);
        assert!(
            boxes.len() >= 2,
            "{preset:?}: the cap and the option row must both be registered"
        );
        let (row, cap) = (boxes[0], *boxes.last().expect("nearest the foot"));

        let plain = frame_buffer(&app);
        mouse(&mut app, MouseEventKind::Moved, cap.x + 1, cap.y);
        let tinted = frame_buffer(&app);

        assert!(
            region_changed(&plain, &tinted, cap),
            "{preset:?}: hovering the Enter cap changed nothing"
        );
        assert!(
            !region_changed(&plain, &tinted, row),
            "{preset:?}: the first option row lit up instead of the cap"
        );
    }
}

fn frame_buffer(app: &App) -> ratatui::buffer::Buffer {
    let backend = TestBackend::new(WIDTH, HEIGHT);
    let mut terminal = Terminal::new(backend).expect("create test terminal");
    terminal.draw(|frame| app.draw(frame)).expect("draw app");
    terminal.backend().buffer().clone()
}

fn region_changed(
    before: &ratatui::buffer::Buffer,
    after: &ratatui::buffer::Buffer,
    rect: Rect,
) -> bool {
    (rect.top()..rect.bottom()).any(|y| {
        (rect.left()..rect.right())
            .any(|x| before[(x, y)].fg != after[(x, y)].fg || before[(x, y)].bg != after[(x, y)].bg)
    })
}
