//! Workspace mouse acceptance: the page is reachable with the pointer alone.
//!
//! Every case clicks a hitbox the draw pass registered, so a region that stops
//! being drawn (or moves) fails here instead of silently working in one preset
//! only.

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::{Terminal, backend::TestBackend};

use crate::core::config::models::UiPreset;
use crate::tui::action::Action;
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

fn buffer_of(app: &App) -> ratatui::buffer::Buffer {
    let backend = TestBackend::new(WIDTH, HEIGHT);
    let mut terminal = Terminal::new(backend).expect("create test terminal");
    terminal.draw(|frame| app.draw(frame)).expect("draw app");
    terminal.backend().buffer().clone()
}

fn click(app: &mut App, column: u16, row: u16) {
    app.handle(AppEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }));
}

fn hover(app: &mut App, column: u16, row: u16) {
    app.handle(AppEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Moved,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }));
}

fn rect_of(app: &App, action: Action) -> Rect {
    app.hitmap
        .hitboxes()
        .iter()
        .rev()
        .find(|hitbox| hitbox.action == action)
        .map(|hitbox| hitbox.rect)
        .unwrap_or_else(|| panic!("no drawn hitbox for {action:?}"))
}

fn click_action(app: &mut App, action: Action) {
    let rect = rect_of(app, action);
    click(app, rect.x, rect.y);
}

fn instance() -> crate::tui::pages::instances::InstanceSummary {
    crate::tui::pages::instances::InstanceSummary {
        id: uuid::Uuid::new_v4(),
        name: "BattleArmory TACZ".into(),
        version: "1.20.1".into(),
        loader: "Forge".into(),
        loader_version: Some("47.4.22".into()),
        status: "ready".into(),
        path: std::path::PathBuf::from("C:/games/versions/BattleArmory TACZ"),
        launch_count: 3,
        play_time_seconds: 120,
    }
}

/// Every workspace test owns a throwaway instance directory: the page writes
/// its overrides to disk on every edit, and a test must never leave a file
/// behind on the machine running it.
struct TempRoot(std::path::PathBuf);

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn workspace_app(preset: UiPreset) -> (App, TempRoot) {
    let root =
        TempRoot(std::env::temp_dir().join(format!("tmcl-ws-mouse-{}", uuid::Uuid::new_v4())));
    std::fs::create_dir_all(&root.0).expect("create instance dir");
    let mut app = App::new();
    app.config.state.ui.preset = preset;
    app.page = Page::Instances;
    let mut summary = instance();
    summary.path = root.0.clone();
    app.instances.apply(vec![summary]);
    app.instances.focused = 0;
    app.open_workspace();
    if let Some(workspace) = app.workspace.as_mut() {
        workspace.draft.loader = crate::core::instance::models::InstanceLoader::Forge {
            version: "47.4.22".into(),
        };
    }
    (app, root)
}

#[test]
fn clicking_the_rail_switches_category_in_every_preset() {
    for preset in UiPreset::ALL {
        let (mut app, _root) = workspace_app(preset);
        draw(&app);
        click_action(&mut app, Action::workspace_category(7));
        assert_eq!(
            app.workspace.as_ref().expect("workspace").category,
            crate::tui::pages::workspace::WorkspaceCategory::Manage,
            "{preset:?}: the rail must be clickable"
        );
        // The body was redrawn for the new category, not left on the old one.
        // `nightlight` renders the manage list as a menu, where a click runs the
        // command, so its rows register `WorkspaceRun` instead of `WorkspaceRow`.
        // Either shape proves the body followed the click.
        draw(&app);
        let row_zero = if preset == UiPreset::Nightlight {
            Action::Page {
                page: Page::Instances,
                act: crate::tui::action::PageAct::WorkspaceRun(0),
            }
        } else {
            Action::workspace_row(0)
        };
        assert!(
            rect_of(&app, row_zero).height == 1,
            "{preset:?}: manage rows must be drawn after the click"
        );
    }
}

#[test]
fn clicking_a_row_selects_it_and_a_double_click_opens_details() {
    let (mut app, _root) = workspace_app(UiPreset::Classic);
    if let Some(workspace) = app.workspace.as_mut() {
        workspace.category = crate::tui::pages::workspace::WorkspaceCategory::Mods;
        workspace.content.set(
            crate::core::content::ContentKind::Mod,
            vec![
                mod_entry("alpha", "mods/alpha.jar"),
                mod_entry("beta", "mods/beta.jar"),
            ],
        );
    }
    draw(&app);
    click_action(&mut app, Action::workspace_row(1));
    assert_eq!(app.workspace.as_ref().expect("workspace").selected, 1);
    assert!(!app.detail_open, "one click only selects");

    click_action(&mut app, Action::workspace_row(1));
    assert!(app.detail_open, "the second click opens the detail popup");
}

/// Every drawn workspace row and the rectangle it occupies.
fn workspace_rows(app: &App) -> Vec<(usize, Rect)> {
    app.hitmap
        .hitboxes()
        .iter()
        .filter_map(|hitbox| match hitbox.action {
            Action::Page {
                page: Page::Instances,
                act: crate::tui::action::PageAct::WorkspaceRow(index),
            } => Some((index, hitbox.rect)),
            _ => None,
        })
        .collect()
}

/// The workspace list keeps its window under a click, like the instance list.
///
/// Its rows are drawn by hand (`offset`, then one row per entry), so a window
/// taken from the selection alone re-anchors on any click above the last visible
/// row and the row jumps to the bottom edge in all three presets.
#[test]
fn clicking_a_drawn_content_row_keeps_the_workspace_list_still() {
    for preset in UiPreset::ALL {
        let (mut app, _root) = workspace_app(preset);
        if let Some(workspace) = app.workspace.as_mut() {
            workspace.category = crate::tui::pages::workspace::WorkspaceCategory::Mods;
            workspace.content.set(
                crate::core::content::ContentKind::Mod,
                (0..40)
                    .map(|index| {
                        mod_entry(
                            &format!("mod-{index:02}"),
                            &format!("mods/mod-{index:02}.jar"),
                        )
                    })
                    .collect(),
            );
            // The last mod, so the window is scrolled to the end and every row
            // above it is a candidate for the jump.
            workspace.selected = 39;
        }
        draw(&app);
        let before = workspace_rows(&app);
        assert!(
            before.len() > 2 && before[0].0 > 0,
            "{preset:?}: need a scrolled window to click in, got {before:?}"
        );

        let (index, rect) = before[before.len() / 2];
        click(&mut app, rect.x + 1, rect.y);
        assert_eq!(
            app.workspace.as_ref().expect("workspace").selected,
            index,
            "{preset:?}: the click must select the row it landed on"
        );

        draw(&app);
        assert_eq!(
            workspace_rows(&app),
            before,
            "{preset:?}: the list moved under a click on a row that was already drawn"
        );
    }
}

#[tokio::test]
async fn clicking_the_mark_toggles_a_mod_without_selecting_it() {
    let (mut app, _root) = workspace_app(UiPreset::Classic);
    let root = std::env::temp_dir().join(format!("tmcl-ws-click-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("mods")).expect("create mods dir");
    let jar = root.join("mods/alpha.jar");
    std::fs::write(&jar, b"jar").expect("write jar");
    if let Some(workspace) = app.workspace.as_mut() {
        workspace.root = root.clone();
        workspace.category = crate::tui::pages::workspace::WorkspaceCategory::Mods;
        workspace.content.set(
            crate::core::content::ContentKind::Mod,
            vec![mod_entry("alpha", &jar.display().to_string())],
        );
    }
    draw(&app);
    click_action(&mut app, Action::workspace_toggle(0));
    assert!(
        !jar.exists(),
        "the enabled file must have been renamed to .disabled"
    );
    std::fs::remove_dir_all(root).expect("clean instance");
}

#[test]
fn clicking_the_step_arrows_changes_the_value() {
    let (mut app, _root) = workspace_app(UiPreset::Classic);
    if let Some(workspace) = app.workspace.as_mut() {
        workspace.category = crate::tui::pages::workspace::WorkspaceCategory::Launch;
        workspace.selected = crate::tui::pages::workspace::LaunchField::MemoryMax.index();
        // Manual mode keeps the memory rows enabled; switching the mode itself
        // is a component of this test's precondition, not its subject.
        workspace.draft.overrides.memory_mode =
            Some(crate::core::config::models::MemoryMode::Manual);
    }
    draw(&app);
    // Following the global value: activating the row freezes it as a custom
    // one. A single click only moves the cursor, exactly like the settings page.
    app.apply_action(
        Action::workspace_row(crate::tui::pages::workspace::LaunchField::MemoryMax.index()),
        crate::tui::action::Click::Double,
    );
    assert_eq!(
        app.workspace
            .as_ref()
            .expect("workspace")
            .draft
            .overrides
            .max_memory_mb,
        Some(app.config.launch.max_memory_mb),
        "activating the row switches follow/custom"
    );
    draw(&app);
    let before = app
        .workspace
        .as_ref()
        .expect("workspace")
        .draft
        .overrides
        .max_memory_mb;
    click_action(&mut app, Action::workspace_step(2, 1));
    let after = app
        .workspace
        .as_ref()
        .expect("workspace")
        .draft
        .overrides
        .max_memory_mb;
    assert_ne!(before, after, "the right arrow must step the value");
}

#[test]
fn workspace_rows_span_the_panel_and_hover_covers_the_whole_row() {
    for preset in UiPreset::ALL {
        let (mut app, _root) = workspace_app(preset);
        app.config.launch.memory_mode = crate::core::config::models::MemoryMode::Manual;
        if let Some(workspace) = app.workspace.as_mut() {
            workspace.category = crate::tui::pages::workspace::WorkspaceCategory::Launch;
        }
        draw(&app);
        // Toggle rows show no arrows, and no launch row has a value-wide
        // toggle target: a single click on a value only selects the row.
        assert!(
            app.hitmap
                .hitboxes()
                .iter()
                .all(|hit| hit.action != Action::workspace_step(0, -1)),
            "{preset:?}: the memory-mode toggle must not advertise arrows"
        );
        assert!(
            !app.hitmap.hitboxes().iter().any(|hit| matches!(
                hit.action,
                Action::Page {
                    act: crate::tui::action::PageAct::WorkspaceToggle(_),
                    ..
                }
            )),
            "{preset:?}: launch values must not toggle on a single click"
        );
        let row = rect_of(&app, Action::workspace_row(1));
        let left = rect_of(&app, Action::workspace_step(1, -1));
        let right = rect_of(&app, Action::workspace_step(1, 1));
        assert!(row.width > 40, "{preset:?}: row is too narrow: {row:?}");
        for arrow in [left, right] {
            assert!(
                row.x <= arrow.x && arrow.right() <= row.right(),
                "{preset:?}: arrow {arrow:?} escapes row {row:?}"
            );
        }
        // The value area belongs to the row: no value-only hover target.
        hover(&mut app, row.right().saturating_sub(2), row.y);
        assert_eq!(
            app.hover,
            Some(Action::workspace_row(1)),
            "{preset:?}: hovering the value must highlight the row"
        );
        // Both arrows are real targets, and hovering one highlights only it.
        hover(&mut app, left.x, left.y);
        assert_eq!(app.hover, Some(Action::workspace_step(1, -1)));
        hover(&mut app, right.x + 1, right.y);
        assert_eq!(app.hover, Some(Action::workspace_step(1, 1)));
    }
}

#[test]
fn classic_workspace_uses_top_tabs() {
    let (classic, _classic_root) = workspace_app(UiPreset::Classic);
    draw(&classic);
    let first = rect_of(&classic, Action::workspace_category(0));
    let second = rect_of(&classic, Action::workspace_category(1));
    assert_eq!(first.height, 1);
    assert_eq!(first.y, second.y, "classic tabs share one row");
    assert!(second.x > first.x, "classic tabs run left to right");
    assert!(
        first.y < rect_of(&classic, Action::workspace_row(0)).y,
        "classic tabs sit above the body, not beside it"
    );
}

#[test]
fn classic_workspace_selection_uses_the_table_style() {
    let (app, _root) = workspace_app(UiPreset::Classic);
    draw(&app);
    let selected = rect_of(&app, Action::workspace_row(0));
    let buffer = buffer_of(&app);
    let cell = &buffer[(selected.x + 1, selected.y)];
    // Classic tables mark a selection with green bold text, never a band.
    assert_eq!(cell.fg, crate::tui::theme::Theme::default().selection);
    assert!(cell.modifier.contains(ratatui::style::Modifier::BOLD));
    assert_ne!(cell.bg, ratatui::style::Color::Rgb(19, 24, 36));
}

#[test]
fn folded_rail_arrows_step_the_category() {
    let (mut app, _root) = workspace_app(UiPreset::Classic);
    draw_sized(&app, 80, 24);
    assert_eq!(
        app.workspace.as_ref().expect("workspace").category,
        crate::tui::pages::workspace::WorkspaceCategory::Overview
    );
    let next = rect_of(&app, Action::workspace_category_by(1));
    click(&mut app, next.x, next.y);
    assert_eq!(
        app.workspace.as_ref().expect("workspace").category,
        crate::tui::pages::workspace::WorkspaceCategory::Launch
    );
    draw_sized(&app, 80, 24);
    let previous = rect_of(&app, Action::workspace_category_by(-1));
    click(&mut app, previous.x, previous.y);
    assert_eq!(
        app.workspace.as_ref().expect("workspace").category,
        crate::tui::pages::workspace::WorkspaceCategory::Overview
    );
}

#[test]
fn scrolling_the_pointer_over_the_body_moves_the_cursor() {
    let (mut app, _root) = workspace_app(UiPreset::Classic);
    if let Some(workspace) = app.workspace.as_mut() {
        workspace.category = crate::tui::pages::workspace::WorkspaceCategory::Manage;
    }
    draw(&app);
    let body = rect_of(&app, Action::workspace_row(0));
    app.handle(AppEvent::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: body.x,
        row: body.y,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(app.workspace.as_ref().expect("workspace").selected, 1);
}

fn mod_entry(name: &str, path: &str) -> crate::core::content::ContentEntry {
    use crate::core::content::{ContentEntry, ContentMeta};
    ContentEntry {
        name: name.into(),
        version: "1.0.0".into(),
        path: std::path::PathBuf::from(path),
        size: 1024,
        enabled: true,
        order: None,
        badge: None,
        warning: None,
        meta: ContentMeta::Mod {
            id: name.into(),
            name: name.into(),
            version: "1.0.0".into(),
            authors: Vec::new(),
            description: String::new(),
            depends: Vec::new(),
        },
    }
}

#[test]
fn classic_instances_draw_the_workspace_not_the_list() {
    let (mut app, _root) = workspace_app(UiPreset::Classic);
    draw(&app);
    // The workspace owns the body: its rail and rows are registered, and the
    // instance table behind it is not.
    assert!(
        !app.hitmap
            .hitboxes()
            .iter()
            .any(|hitbox| hitbox.action == Action::row(Page::Instances, 0)),
        "the instance table must not answer clicks from under the workspace"
    );
    rect_of(&app, Action::workspace_category(0));
    rect_of(&app, Action::workspace_row(0));

    // Enter on a manage row reached the state, so the screen the footer
    // advertises is the screen that is drawn.
    click_action(&mut app, Action::workspace_category(7));
    draw(&app);
    rect_of(&app, Action::workspace_row(3));
}

#[test]
fn leaving_the_instances_page_closes_the_workspace() {
    let (mut app, _root) = workspace_app(UiPreset::Classic);
    draw(&app);
    app.set_page(Page::Tasks);
    assert!(
        !app.workspace_is_open(),
        "another page must not inherit the workspace key map"
    );
}

/// The pointer has to reach the filter field too: it is drawn as a card, and a
/// card that cannot be clicked reads as decoration.
#[test]
fn clicking_the_search_field_focuses_it_in_every_preset() {
    use crate::tui::input::SearchTarget;
    use crate::tui::pages::workspace::WorkspaceCategory;

    for preset in UiPreset::ALL {
        let (mut app, _root) = workspace_app(preset);
        app.workspace.as_mut().expect("workspace").category = WorkspaceCategory::Mods;
        draw(&app);
        let action = Action::search(SearchTarget::Workspace);
        click_action(&mut app, action);
        assert_eq!(
            app.mode(),
            crate::tui::command::Mode::Search(SearchTarget::Workspace),
            "{preset:?}: the field must take the keyboard"
        );
    }

    // The overview category has no field, and nothing there claims the key.
    let (mut app, _root) = workspace_app(UiPreset::Classic);
    app.workspace.as_mut().expect("workspace").category = WorkspaceCategory::Launch;
    draw(&app);
    assert!(
        app.hitmap
            .hitboxes()
            .iter()
            .all(|hitbox| hitbox.action != Action::search(SearchTarget::Workspace)),
        "no field, no hitbox"
    );
}
