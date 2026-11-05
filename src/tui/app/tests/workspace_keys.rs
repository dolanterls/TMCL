//! Key-advertising contract for the instance workspace.
//!
//! The note strip and the footer both *claim* which keys work. That claim was
//! maintained by hand and drifted: `e` and `L` were advertised (design §6.3.1 /
//! §6.4) while nothing bound them in `Mode::Workspace`, and `r` was once reused
//! for "refresh" although it already restores a world. These tests make the
//! claim checkable:
//!
//! 1. every key the strip or the footer prints must map to a command;
//! 2. the keys with a fixed meaning must keep it, so reusing a command for a
//!    second job fails here instead of in a screenshot.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::core::config::models::UiPreset;
use crate::core::content::{ContentEntry, ContentKind, ContentMeta};
use crate::tui::app::{App, Page};
use crate::tui::command::{Command, Mode, from_key};
use crate::tui::pages::workspace::{LaunchField, WorkspaceCategory};

const WIDTH: u16 = 120;
const HEIGHT: u16 = 40;

struct TempRoot(std::path::PathBuf);

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn entry(name: &str, path: &str) -> ContentEntry {
    ContentEntry {
        name: name.into(),
        version: "1.0".into(),
        path: std::path::PathBuf::from(path),
        size: 1,
        enabled: true,
        order: Some(0),
        badge: None,
        warning: None,
        meta: ContentMeta::Unknown,
    }
}

/// A workspace whose categories are all non-empty and whose shader loader is
/// present, so the strip advertises its full set of keys.
fn app_with_content(preset: UiPreset) -> (App, TempRoot) {
    let root =
        TempRoot(std::env::temp_dir().join(format!("tmcl-ws-keys-{}", uuid::Uuid::new_v4())));
    std::fs::create_dir_all(&root.0).expect("create instance dir");
    let mut app = App::new();
    app.config.state.ui.preset = preset;
    app.page = Page::Instances;
    app.terminal_width = WIDTH;
    app.terminal_height = HEIGHT;
    app.instances
        .apply(vec![crate::tui::pages::instances::InstanceSummary {
            id: uuid::Uuid::new_v4(),
            name: "demo".into(),
            version: "1.20.1".into(),
            loader: "Forge".into(),
            loader_version: Some("47.4.22".into()),
            status: "ready".into(),
            path: root.0.clone(),
            launch_count: 0,
            play_time_seconds: 0,
        }]);
    app.instances.focused = 0;
    app.open_workspace();
    if let Some(workspace) = app.workspace.as_mut() {
        workspace
            .content
            .set(ContentKind::Mod, vec![entry("Alpha", "mods/alpha.jar")]);
        workspace.content.set(
            ContentKind::ResourcePack,
            vec![entry("Pack", "resourcepacks/pack.zip")],
        );
        workspace.content.set(
            ContentKind::Shader,
            vec![entry("Shader", "shaderpacks/shader.zip")],
        );
        workspace
            .content
            .set(ContentKind::World, vec![entry("World", "saves/world")]);
        workspace.shader_loader = Some(crate::core::content::shader::ShaderLoader::Iris);
        workspace.loading = false;
    }
    (app, root)
}

/// Arrow glyphs in the hint tables are their own key codes, not characters.
fn arrow(character: char) -> Option<KeyCode> {
    match character {
        '←' => Some(KeyCode::Left),
        '→' => Some(KeyCode::Right),
        '↑' => Some(KeyCode::Up),
        '↓' => Some(KeyCode::Down),
        _ => None,
    }
}

/// `"←/→"`, `"J/K"` and `"↑↓"` are several keys; the rest are single tokens.
fn keys_of(token: &str) -> Vec<KeyEvent> {
    // The search key *is* "/", so it must not be treated as a separator.
    if token == "/" {
        return vec![KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE)];
    }
    if !token.is_empty() && token.chars().all(|character| arrow(character).is_some()) {
        return token
            .chars()
            .filter_map(arrow)
            .map(|code| KeyEvent::new(code, KeyModifiers::NONE))
            .collect();
    }
    token
        .split('/')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let code = match part {
                "Enter" => KeyCode::Enter,
                "Space" => KeyCode::Char(' '),
                "Esc" => KeyCode::Esc,
                "F5" => KeyCode::F(5),
                other => {
                    let character = other.chars().next().expect("non-empty key token");
                    arrow(character).unwrap_or(KeyCode::Char(character))
                }
            };
            KeyEvent::new(code, KeyModifiers::NONE)
        })
        .collect()
}

/// `"Space Toggle · d Delete"` → `[("Space", "Toggle"), ("d", "Delete")]`.
fn advertised(keys: &str) -> Vec<(String, String)> {
    keys.split(" · ")
        .filter(|part| !part.trim().is_empty())
        .filter_map(|part| {
            let (key, label) = part.trim().split_once(' ')?;
            Some((key.to_owned(), label.to_owned()))
        })
        .collect()
}

#[test]
fn every_advertised_workspace_key_is_bound() {
    let (mut app, _root) = app_with_content(UiPreset::Creeper);
    for category in WorkspaceCategory::ALL {
        app.workspace.as_mut().expect("workspace").category = category;
        let workspace = app.workspace.as_ref().expect("workspace");
        let line = app.workspace_action_line(workspace);
        let mut claims = advertised(&line);
        assert!(
            !claims.is_empty(),
            "every category must advertise something: {category:?}"
        );
        // The footer makes the same claim, so it is held to the same rule.
        claims.extend(app.hints().into_iter().map(|hint| (hint.key, hint.action)));
        for (token, label) in claims {
            for key in keys_of(&token) {
                let command = from_key(Mode::Workspace, Page::Instances, key);
                assert_ne!(
                    command,
                    Command::Nothing,
                    "{category:?} advertises \"{token} {label}\" but {key:?} maps to nothing"
                );
            }
        }
    }
}

#[test]
fn advertised_keys_keep_their_documented_meaning() {
    let (mut app, _root) = app_with_content(UiPreset::Classic);
    let expected: &[(&str, Command)] = &[
        ("L", Command::Launch),
        ("e", Command::Rename),
        ("o", Command::OpenFolder),
        ("i", Command::Import),
        ("d", Command::Delete),
        ("/", Command::Search),
        ("s", Command::Sort),
        ("Enter", Command::Activate),
        ("Space", Command::ToggleFollow),
        ("F5", Command::RescanWorkspace),
        ("b", Command::BackupWorld),
        ("r", Command::RestoreWorld),
        ("c", Command::CloneWorld),
        ("J", Command::ReorderContent(1)),
        ("K", Command::ReorderContent(-1)),
        ("←", Command::AdjustSetting(-1)),
        ("→", Command::AdjustSetting(1)),
    ];

    for category in WorkspaceCategory::ALL {
        app.workspace.as_mut().expect("workspace").category = category;
        let workspace = app.workspace.as_ref().expect("workspace");
        let line = app.workspace_action_line(workspace);
        for (token, label) in advertised(&line) {
            for key in keys_of(&token) {
                let Some((_, command)) = expected
                    .iter()
                    .find(|(name, _)| keys_of(name).contains(&key))
                else {
                    continue;
                };
                assert_eq!(
                    &from_key(Mode::Workspace, Page::Instances, key),
                    command,
                    "{category:?} advertises \"{token} {label}\" but {key:?} now means something else"
                );
            }
        }
    }
}

#[test]
fn the_refresh_key_is_not_shared_with_rename() {
    // `Command::Refresh` opens the rename form on the instances page. Binding it
    // to the workspace refresh key silently turned the first manage row into a
    // rescan, so the two meanings stay separate.
    let f5 = KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE);
    assert_eq!(
        from_key(Mode::Workspace, Page::Instances, f5),
        Command::RescanWorkspace
    );
    let r = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE);
    assert_eq!(
        from_key(Mode::Workspace, Page::Instances, r),
        Command::RestoreWorld,
        "r restores a world in this mode; it must not be reused for refresh"
    );
    let e = KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE);
    assert_eq!(
        from_key(Mode::Workspace, Page::Instances, e),
        Command::Rename
    );
}

#[test]
fn the_launch_key_reaches_the_workspace_instance() {
    // Driven through the real key, so the binding is what gets tested rather
    // than the helper behind it.
    let (mut app, _root) = app_with_content(UiPreset::Creeper);
    let mut other = app.instances.items[0].clone();
    other.id = uuid::Uuid::new_v4();
    other.name = "other".into();
    app.instances.items.push(other);
    app.instances.focused = 1;

    app.handle_key(KeyEvent::new(KeyCode::Char('L'), KeyModifiers::NONE));

    assert_eq!(
        app.instances.focused, 0,
        "L must route the launch to the instance the workspace shows"
    );
}

#[test]
fn the_launch_category_labels_every_field() {
    // The renderer and the actions share this label table; a field without a
    // label would render an empty first column.
    let (app, _root) = app_with_content(UiPreset::Creeper);
    for field in LaunchField::ALL {
        assert!(
            !app.i18n.text(field.label_key()).is_empty(),
            "{field:?} has no label"
        );
    }
}

/// `/` no longer opens a hidden mode: the instance pages keep a search field in
/// the card header, and the key only puts the cursor in it.
#[test]
fn slash_focuses_the_workspace_search_field() {
    use crate::tui::input::SearchTarget;

    let (mut app, _root) = app_with_content(UiPreset::Nightlight);
    app.workspace.as_mut().expect("workspace").category = WorkspaceCategory::Mods;
    assert_eq!(app.workspace_content_entries().len(), 1);

    app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
    assert_eq!(
        app.mode(),
        Mode::Search(SearchTarget::Workspace),
        "the field has to take the keyboard, or typing goes to the row cursor"
    );

    for character in "zzz".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
    }
    assert_eq!(app.workspace.as_ref().expect("workspace").search, "zzz");
    assert!(app.workspace_content_entries().is_empty());

    // Enter keeps the query and hands the keyboard back to the list.
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(app.mode(), Mode::Workspace);
    assert_eq!(app.workspace.as_ref().expect("workspace").search, "zzz");

    // Esc is the cancel: it drops the query.
    app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(app.mode(), Mode::Workspace);
    assert!(app.workspace.as_ref().expect("workspace").search.is_empty());
    assert_eq!(app.workspace_content_entries().len(), 1);
}

/// Re-focusing the field must not wipe what it already holds.
#[test]
fn focusing_the_workspace_field_keeps_the_query() {
    let (mut app, _root) = app_with_content(UiPreset::Classic);
    app.workspace.as_mut().expect("workspace").category = WorkspaceCategory::Mods;
    app.workspace.as_mut().expect("workspace").search = "alp".into();

    app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
    assert_eq!(
        app.workspace.as_ref().expect("workspace").search,
        "alp",
        "clicking into a field to refine it must not clear it"
    );
    // The cursor lands at the end, so typing appends.
    app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
    assert_eq!(app.workspace.as_ref().expect("workspace").search, "alph");
}
