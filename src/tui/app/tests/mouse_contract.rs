//! P6 contract: no mouse target exists without a documented keyboard path.
//!
//! The parity table is hand-written on purpose: a new `Action` has to be added to
//! it, which is the moment someone decides what key reaches the same place. The
//! coverage test walks a drawn matrix and proves that every registered target
//! appears in the table; shapes no frame ever registers must be allowlisted with
//! a reason.

use ratatui::{Terminal, backend::TestBackend};

use crate::core::config::models::UiPreset;
use crate::tui::action::{Action, ModalAct, OverlayAct, PageAct};
use crate::tui::app::{App, Page};
use crate::tui::modal::{ConfirmAction, FormAction, Modal};
use crate::tui::pages::settings::SettingsCategory;

/// Parameter-free identity of an `Action`, so the parity table is about intents
/// rather than about row indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    Nav,
    Row,
    Activate,
    Filter,
    VersionFilter,
    Category,
    Step,
    Launch,
    Activity,
    Metric,
    Search,
    OverlayClose,
    OverlayNoop,
    ModalNoop,
    Focus,
    Toggle,
    Option,
    Browse,
    Submit,
    Cancel,
    Confirm,
    Reject,
    ModalFilter,
    ModalSearch,
    FilterCategories,
    CheckItem,
    FocusCheck,
    ApplyChecks,
    ResetChecks,
    ScrollOptions,
    CloseOptions,
    RetryOptions,
}

fn shape(action: Action) -> Shape {
    match action {
        Action::Nav(_) => Shape::Nav,
        Action::Overlay(OverlayAct::Close) => Shape::OverlayClose,
        Action::Overlay(OverlayAct::Noop) => Shape::OverlayNoop,
        Action::Modal(ModalAct::Noop) => Shape::ModalNoop,
        Action::Modal(ModalAct::Focus(_)) => Shape::Focus,
        Action::Modal(ModalAct::Toggle(_)) => Shape::Toggle,
        Action::Modal(ModalAct::Option(_)) => Shape::Option,
        Action::Modal(ModalAct::Browse(_)) => Shape::Browse,
        Action::Modal(ModalAct::BrowseFile(_)) => Shape::Browse,
        Action::Modal(ModalAct::Submit) => Shape::Submit,
        Action::Modal(ModalAct::Cancel) => Shape::Cancel,
        Action::Modal(ModalAct::Confirm) => Shape::Confirm,
        Action::Modal(ModalAct::Reject) => Shape::Reject,
        Action::Modal(ModalAct::ToggleVersionFilter(_)) => Shape::ModalFilter,
        Action::Modal(ModalAct::EditSearch) => Shape::ModalSearch,
        Action::Modal(ModalAct::CheckItem(_)) => Shape::CheckItem,
        Action::Modal(ModalAct::FocusCheck(_)) => Shape::FocusCheck,
        Action::Modal(ModalAct::ApplyChecks) => Shape::ApplyChecks,
        Action::Modal(ModalAct::ResetChecks) => Shape::ResetChecks,
        Action::Modal(ModalAct::ScrollOptions(_)) => Shape::ScrollOptions,
        Action::Modal(ModalAct::CloseOptions) => Shape::CloseOptions,
        Action::Modal(ModalAct::RetryOptions) => Shape::RetryOptions,
        Action::Page { act, .. } => match act {
            PageAct::FilterCategories => Shape::FilterCategories,
            PageAct::Row(_) => Shape::Row,
            PageAct::Activate(_) => Shape::Activate,
            PageAct::Filter(_) => Shape::Filter,
            PageAct::VersionFilter(_) => Shape::VersionFilter,
            PageAct::Category(_) => Shape::Category,
            PageAct::Step { .. } => Shape::Step,
            PageAct::Launch => Shape::Launch,
            PageAct::Activity => Shape::Activity,
            PageAct::Metric(_) => Shape::Metric,
            PageAct::Search(_) => Shape::Search,
            // The workspace reuses the vocabulary of the settings page: a rail,
            // rows, a step control and a toggle. Only the page differs, so the
            // drawn shape is the same and the contract stays comparable.
            PageAct::WorkspaceCategory(_) | PageAct::WorkspaceCategoryBy(_) => Shape::Category,
            PageAct::WorkspaceRow(_) | PageAct::WorkspaceRun(_) => Shape::Row,
            PageAct::WorkspaceToggle(_) => Shape::Toggle,
            PageAct::WorkspaceStep { .. } => Shape::Step,
        },
    }
}

const ALL_SHAPES: &[Shape] = &[
    Shape::Nav,
    Shape::Row,
    Shape::Activate,
    Shape::Filter,
    Shape::VersionFilter,
    Shape::Category,
    Shape::Step,
    Shape::Launch,
    Shape::Activity,
    Shape::Metric,
    Shape::Search,
    Shape::FilterCategories,
    Shape::OverlayClose,
    Shape::OverlayNoop,
    Shape::ModalNoop,
    Shape::Focus,
    Shape::Toggle,
    Shape::Option,
    Shape::Browse,
    Shape::Submit,
    Shape::Cancel,
    Shape::Confirm,
    Shape::Reject,
    Shape::ModalFilter,
    Shape::ModalSearch,
    Shape::CheckItem,
    Shape::FocusCheck,
    Shape::ApplyChecks,
    Shape::ResetChecks,
    Shape::ScrollOptions,
    Shape::CloseOptions,
    Shape::RetryOptions,
];

/// How the keyboard reaches the same place, or why there is none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Binding {
    Key(&'static str),
    MouseOnly(&'static str),
}

const PARITY: &[(Shape, Binding)] = &[
    (Shape::Nav, Binding::Key("Tab / BackTab")),
    (Shape::Row, Binding::Key("Up / Down")),
    (Shape::Activate, Binding::Key("Enter")),
    (Shape::Filter, Binding::Key("[ / ]")),
    (Shape::VersionFilter, Binding::Key("1-4")),
    (Shape::Category, Binding::Key("[ / ]")),
    (Shape::Step, Binding::Key("Left / Right")),
    (Shape::Launch, Binding::Key("Enter")),
    (
        Shape::Activity,
        Binding::MouseOnly("a card that opens logs; no key is advertised for it"),
    ),
    (
        Shape::Metric,
        Binding::MouseOnly("overview tiles are pointer shortcuts to a page"),
    ),
    (Shape::Search, Binding::Key("/")),
    (Shape::FilterCategories, Binding::Key("f")),
    (Shape::OverlayClose, Binding::Key("Esc")),
    (
        Shape::OverlayNoop,
        Binding::MouseOnly("the overlay body only swallows clicks"),
    ),
    (
        Shape::ModalNoop,
        Binding::MouseOnly("the dialog body only swallows clicks"),
    ),
    (Shape::Focus, Binding::Key("Tab / Shift-Tab")),
    (Shape::Toggle, Binding::Key("Enter / Space")),
    (Shape::Option, Binding::Key("Up / Down then Enter")),
    (Shape::Browse, Binding::Key("F2")),
    (Shape::Submit, Binding::Key("Enter / F5")),
    (Shape::Cancel, Binding::Key("Esc")),
    (Shape::Confirm, Binding::Key("Enter")),
    (Shape::Reject, Binding::Key("Esc")),
    (Shape::ModalFilter, Binding::Key("1-4")),
    (Shape::ModalSearch, Binding::Key("/")),
    (Shape::CheckItem, Binding::Key("Space")),
    (Shape::FocusCheck, Binding::Key("Up / Down")),
    (Shape::ApplyChecks, Binding::Key("Enter")),
    (Shape::ResetChecks, Binding::Key("R")),
    (
        Shape::ScrollOptions,
        Binding::MouseOnly("a wheel target that moves the open option list"),
    ),
    (Shape::CloseOptions, Binding::Key("Esc")),
    (Shape::RetryOptions, Binding::Key("r")),
];

/// Shapes that are deliberately never registered, with the reason.
const ALLOWLIST: &[(Shape, &str)] = &[
    (
        Shape::ScrollOptions,
        "never an Action: the wheel calls apply_scroll directly",
    ),
    (
        Shape::FocusCheck,
        "checklist cursor movement has no hit area: the wheel is registered as \
     ScrollOptions and the rows themselves as CheckItem",
    ),
    (
        Shape::VersionFilter,
        "version chips are gone in both presets: kinds are picked in the filter \
     dialog, and `1`-`4` stay as a keyboard shortcut",
    ),
    (
        Shape::RetryOptions,
        "only drawn when the option list failed to load, which no snapshot covers",
    ),
];

fn draw(app: &App, width: u16, height: u16) {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal.draw(|frame| app.draw(frame)).expect("draw app");
}

fn collect(app: &App, out: &mut Vec<Shape>) {
    for hitbox in app.hitmap.hitboxes() {
        let shape = shape(hitbox.action);
        if !out.contains(&shape) {
            out.push(shape);
        }
    }
}

fn instances() -> Vec<crate::tui::pages::instances::InstanceSummary> {
    (0..3)
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
        .collect()
}

fn versions() -> Vec<crate::core::version::models::VersionSummary> {
    (0..3)
        .map(|index| crate::core::version::models::VersionSummary {
            id: format!("1.21.{index}"),
            kind: "release".into(),
            url: String::new(),
            time: String::new(),
            release_time: String::new(),
            sha1: None,
            compliance_level: None,
        })
        .collect()
}

fn page_app(preset: UiPreset, page: Page, width: u16, height: u16) -> App {
    let mut app = App::new();
    app.config.state.ui.preset = preset;
    app.page = page;
    app.terminal_width = width;
    app.terminal_height = height;
    app.instances.apply(instances());
    app.versions = versions();
    app
}

fn overlay_apps(preset: UiPreset) -> Vec<App> {
    let mut apps = Vec::new();

    let mut help = page_app(preset, Page::Instances, 120, 40);
    help.help_open = true;
    apps.push(help);

    let mut detail = page_app(preset, Page::Instances, 120, 40);
    detail.open_details();
    apps.push(detail);

    let mut form = page_app(preset, Page::Instances, 120, 40);
    form.modal = Some(
        Modal::form(
            "Edit",
            &["Directory", "Name"],
            &[String::new(), "world".into()],
            FormAction::SettingValue(crate::tui::pages::settings::SettingId::WindowWidth),
        )
        .with_directory_picker(0)
        .with_selection_picker(0, vec!["a".into(), "b".into()]),
    );
    apps.push(form);

    let mut picker = page_app(preset, Page::Instances, 120, 40);
    let mut modal = Modal::form(
        "Edit",
        &["Version"],
        &["a".into()],
        FormAction::SettingValue(crate::tui::pages::settings::SettingId::WindowWidth),
    )
    .with_selection_picker(0, vec!["a".into(), "b".into()]);
    modal.apply(ModalAct::Toggle(0));
    picker.modal = Some(modal);
    apps.push(picker);

    let mut version = page_app(preset, Page::Instances, 120, 40);
    let mut modal = Modal::form(
        "Pick a version",
        &["Version"],
        &["1.21.0".into()],
        FormAction::SettingValue(crate::tui::pages::settings::SettingId::WindowWidth),
    )
    .with_version_picker(0, versions());
    modal.apply(ModalAct::Toggle(0));
    version.modal = Some(modal);
    apps.push(version);

    let mut checklist = page_app(preset, Page::Instances, 120, 40);
    checklist.modal = Some(Modal::checklist(
        "Filter",
        vec![
            crate::tui::modal::ChecklistItem::new("adventure", "Adventure"),
            crate::tui::modal::ChecklistItem::new("decoration", "Decoration"),
        ],
        &["adventure".to_string()],
        false,
    ));
    apps.push(checklist);

    let mut confirm = page_app(preset, Page::Instances, 120, 40);
    confirm.modal = Some(Modal::confirm(
        "Remove?",
        ConfirmAction::RemoveInstance(uuid::Uuid::nil()),
    ));
    apps.push(confirm);

    let mut prompt = page_app(preset, Page::Instances, 120, 40);
    prompt.modal = Some(Modal::prompt(
        "Rename",
        "Name",
        "Enter to apply",
        "world",
        FormAction::SettingValue(crate::tui::pages::settings::SettingId::WindowWidth),
    ));
    apps.push(prompt);

    apps
}

fn registered_shapes() -> Vec<Shape> {
    let mut shapes = Vec::new();
    for preset in UiPreset::ALL {
        for page in Page::ALL {
            for (width, height) in [(120, 40), (80, 24)] {
                let app = page_app(preset, page, width, height);
                draw(&app, width, height);
                collect(&app, &mut shapes);
            }
        }
        for app in overlay_apps(preset) {
            draw(&app, 120, 40);
            collect(&app, &mut shapes);
        }
    }
    shapes
}

fn binding(shape: Shape) -> Option<Binding> {
    PARITY
        .iter()
        .find(|(candidate, _)| *candidate == shape)
        .map(|(_, binding)| *binding)
}

#[test]
fn parity_table_has_exactly_one_entry_per_shape() {
    for shape in ALL_SHAPES {
        let entries = PARITY.iter().filter(|(s, _)| s == shape).count();
        assert_eq!(entries, 1, "{shape:?} must appear once in PARITY");
    }
    for (shape, _) in PARITY {
        assert!(ALL_SHAPES.contains(shape), "{shape:?} is not in ALL_SHAPES");
    }
}

#[test]
fn every_registered_target_has_a_documented_binding() {
    let registered = registered_shapes();
    for shape in &registered {
        match binding(*shape) {
            Some(Binding::Key(keys)) => assert!(!keys.is_empty(), "{shape:?}"),
            Some(Binding::MouseOnly(reason)) => assert!(!reason.is_empty(), "{shape:?}"),
            None => panic!("{shape:?} is clickable but missing from PARITY"),
        }
    }
}

#[test]
fn unregistered_shapes_are_allowlisted_with_a_reason() {
    let registered = registered_shapes();
    for shape in ALL_SHAPES {
        if registered.contains(shape) {
            continue;
        }
        let entry = ALLOWLIST.iter().find(|(s, _)| s == shape);
        let reason = entry.map(|(_, reason)| *reason).unwrap_or("");
        assert!(
            !reason.is_empty(),
            "{shape:?} is never registered and needs an allowlist entry with a reason"
        );
    }
    // The allowlist must not rot: an entry that starts being registered again
    // has to be removed.
    for (shape, _) in ALLOWLIST {
        assert!(
            !registered.contains(shape),
            "{shape:?} is registered now; drop its allowlist entry"
        );
    }
}

#[test]
fn every_action_page_matches_the_row_kind_page() {
    // `RowKind` owns the page of its rows; a mismatch would route a click into
    // another page's selection state.
    for kind in [
        crate::tui::hit::RowKind::OverviewInstances,
        crate::tui::hit::RowKind::Instances,
        crate::tui::hit::RowKind::Accounts,
        crate::tui::hit::RowKind::Tasks,
        crate::tui::hit::RowKind::Versions,
        crate::tui::hit::RowKind::ResourceProjects,
        crate::tui::hit::RowKind::ResourceFiles,
        crate::tui::hit::RowKind::Settings,
        crate::tui::hit::RowKind::Java,
        crate::tui::hit::RowKind::Directories,
    ] {
        match kind.action(0) {
            Action::Page { page, .. } => assert_eq!(page, kind.page(), "{kind:?}"),
            other => panic!("unexpected action {other:?}"),
        }
    }
}

#[test]
fn clicking_every_drawn_target_does_not_panic() {
    use crate::tui::event::AppEvent;
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    let clicked = std::cell::Cell::new(0usize);
    let click = |app: &mut App, hitbox: &crate::tui::hit::Hitbox| {
        let rect = hitbox.rect;
        let centre = (rect.x + rect.width / 2, rect.y + rect.height / 2);
        app.handle(AppEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: centre.0,
            row: centre.1,
            modifiers: KeyModifiers::NONE,
        }));
        clicked.set(clicked.get() + 1);
    };

    for preset in UiPreset::ALL {
        for page in Page::ALL {
            for (width, height) in [(120, 40), (80, 24)] {
                let mut app = page_app(preset, page, width, height);
                draw(&app, width, height);
                let boxes = app.hitmap.hitboxes();
                for hitbox in &boxes {
                    click(&mut app, hitbox);
                    // Repaint so the next click sees a live frame even if this
                    // one changed the page or opened an overlay.
                    draw(&app, width, height);
                }
            }
        }
        for mut app in overlay_apps(preset) {
            draw(&app, 120, 40);
            let boxes = app.hitmap.hitboxes();
            for hitbox in &boxes {
                click(&mut app, hitbox);
                draw(&app, 120, 40);
            }
        }
    }
    assert!(
        clicked.get() > 300,
        "the matrix only clicked {} targets",
        clicked.get()
    );
}

// ---------------------------------------------------------------------------
// Reverse parity: every `Command` is either reachable by a mouse target or is
// explicitly keyboard-only. The exhaustive `match` below is the guard: adding a
// variant stops compiling until someone decides which side it belongs to.
// ---------------------------------------------------------------------------

use crate::tui::command::Command;

/// Shape a registered target reaches for this command, or `None` when no target
/// exists and the command is listed in `KEYBOARD_ONLY_COMMANDS`.
fn mouse_reach(cmd: Command) -> Option<Shape> {
    match cmd {
        Command::NextTab | Command::PrevTab => Some(Shape::Nav),
        Command::FilterCategories => Some(Shape::FilterCategories),
        Command::PrevRow
        | Command::NextRow
        | Command::PrevPage
        | Command::NextPage
        | Command::Top
        | Command::Bottom
        | Command::SetCurrent => Some(Shape::Row),
        Command::Activate | Command::ActivateSetting => Some(Shape::Activate),
        Command::Launch => Some(Shape::Launch),
        Command::PrevBlock | Command::NextBlock => Some(Shape::Filter),
        Command::ToggleVersionCategory(_) => Some(Shape::VersionFilter),
        Command::AdjustSetting(_) => Some(Shape::Step),
        Command::Search => Some(Shape::Search),
        Command::Back => Some(Shape::OverlayClose),
        // No drawn target: application shortcuts, page actions that only exist
        // as keys, task/log controls and the text-editing commands.
        Command::Quit
        | Command::Help
        | Command::Diagnose
        | Command::Create
        | Command::Import
        | Command::ImportModpack
        | Command::Rename
        | Command::Delete
        | Command::NewAccount
        | Command::MicrosoftLogin
        | Command::Source
        | Command::Sort
        | Command::Target
        | Command::Refresh
        | Command::RescanWorkspace
        | Command::Pause
        | Command::Resume
        | Command::Retry
        | Command::ToggleFollow
        | Command::FollowBottom
        | Command::Clear
        | Command::OpenFolder
        | Command::ExportScript
        | Command::OpenDatapacks
        | Command::CheckModUpdates
        | Command::UpdateModpack
        | Command::ResetOverrides
        | Command::ReorderContent(_)
        | Command::BackupWorld
        | Command::RestoreWorld
        | Command::CloneWorld
        | Command::SettingAction(_)
        | Command::Remedy(_)
        | Command::ExportDiagnosis
        | Command::LocateInLog
        | Command::Input(_)
        | Command::Backspace
        | Command::CursorLeft
        | Command::CursorRight
        | Command::CursorHome
        | Command::CursorEnd
        | Command::DeleteForward
        | Command::Submit
        | Command::Cancel
        | Command::Nothing => None,
    }
}

const KEYBOARD_ONLY_COMMANDS: &[(Command, &str)] = &[
    (Command::Quit, "application shortcut"),
    (
        Command::Help,
        "opens an overlay; only closing it has a target",
    ),
    (
        Command::Diagnose,
        "opens the diagnosis card; only closing it has a target",
    ),
    (
        Command::Remedy('M'),
        "the card's key caps are printed, not drawn as buttons",
    ),
    (
        Command::ExportDiagnosis,
        "the card's key caps are printed, not drawn as buttons",
    ),
    (
        Command::LocateInLog,
        "the card's key caps are printed, not drawn as buttons",
    ),
    (Command::Create, "page action without a drawn button"),
    (Command::Import, "page action without a drawn button"),
    (Command::ImportModpack, "page action without a drawn button"),
    (Command::Rename, "page action without a drawn button"),
    (
        Command::Delete,
        "destructive action, keyboard only by design",
    ),
    (Command::NewAccount, "page action without a drawn button"),
    (
        Command::MicrosoftLogin,
        "page action without a drawn button",
    ),
    (Command::Source, "download filter cycled by key only"),
    (Command::Sort, "download filter cycled by key only"),
    (Command::Target, "download target picker opened by key only"),
    (Command::Refresh, "page action without a drawn button"),
    (
        Command::RescanWorkspace,
        "workspace rescan; the file list is not a button",
    ),
    (Command::Pause, "task control; no drawn button"),
    (Command::Resume, "task control; no drawn button"),
    (Command::Retry, "task control; no drawn button"),
    (
        Command::ToggleFollow,
        "log follow toggle; §4.2 keeps it keyboard only",
    ),
    (Command::FollowBottom, "log follow action; no drawn button"),
    (Command::Clear, "log clear action; no drawn button"),
    (
        Command::ResetOverrides,
        "workspace shortcut; the reset has no drawn button",
    ),
    (
        Command::ReorderContent(1),
        "resource-pack reorder; J/K have no drawn button",
    ),
    (Command::BackupWorld, "world backup; b has no drawn button"),
    (Command::RestoreWorld, "world restore; r opens a picker"),
    (Command::CloneWorld, "world clone; c has no drawn button"),
    (
        Command::OpenFolder,
        "workspace shortcut; the file manager has no drawn button",
    ),
    (
        Command::ExportScript,
        "workspace shortcut; writing a script has no drawn button",
    ),
    (
        Command::OpenDatapacks,
        "world sub-view; p has no drawn button",
    ),
    (
        Command::CheckModUpdates,
        "network check; u has no drawn button",
    ),
    (Command::UpdateModpack, "pack update; U has no drawn button"),
    (
        Command::SettingAction('r'),
        "hidden settings shortcut letters",
    ),
    (Command::Input('a'), "text editing"),
    (Command::Backspace, "text editing"),
    (Command::CursorLeft, "text editing"),
    (Command::CursorRight, "text editing"),
    (Command::CursorHome, "text editing"),
    (Command::CursorEnd, "text editing"),
    (Command::DeleteForward, "text editing"),
    (Command::Submit, "text editing (search submit)"),
    (Command::Cancel, "text editing (search cancel)"),
    (Command::Nothing, "not a real command"),
];

#[test]
fn every_keyboard_only_command_is_explicitly_listed() {
    for (command, reason) in KEYBOARD_ONLY_COMMANDS {
        assert!(
            mouse_reach(*command).is_none(),
            "{command:?} has a mouse target and must not be keyboard-only"
        );
        assert!(!reason.is_empty(), "{command:?} needs a reason");
    }
    // Every command that maps to a shape must be registered or allowlisted, and
    // that shape must carry a documented binding.
    for (command, shape) in [
        (Command::NextTab, Shape::Nav),
        (Command::NextRow, Shape::Row),
        (Command::Activate, Shape::Activate),
        (Command::Launch, Shape::Launch),
        (Command::NextBlock, Shape::Filter),
        (Command::ToggleVersionCategory(0), Shape::VersionFilter),
        (Command::AdjustSetting(1), Shape::Step),
        (Command::Search, Shape::Search),
        (Command::Back, Shape::OverlayClose),
        (Command::ActivateSetting, Shape::Activate),
    ] {
        assert_eq!(mouse_reach(command), Some(shape), "{command:?}");
        assert!(binding(shape).is_some(), "{shape:?} needs a PARITY entry");
    }
}

#[test]
fn the_inline_settings_search_field_is_clickable() {
    let mut app = page_app(UiPreset::Nightlight, Page::Settings, 120, 40);
    app.settings.select_category(SettingsCategory::Appearance);
    app.settings_search = Some(String::new());
    draw(&app, 120, 40);
    let field = app
        .hitmap
        .hitboxes()
        .iter()
        .find(|hitbox| hitbox.action == Action::search(crate::tui::input::SearchTarget::Settings))
        .map(|hitbox| hitbox.rect)
        .expect("the settings search bar");
    app.handle(crate::tui::event::AppEvent::Mouse(
        crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: field.x + 1,
            row: field.y,
            modifiers: crossterm::event::KeyModifiers::NONE,
        },
    ));
    assert!(
        app.search
            .as_ref()
            .is_some_and(|state| state.target == crate::tui::input::SearchTarget::Settings),
        "clicking the bar focuses the inline search again"
    );
}
