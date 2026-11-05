//! Behavior tests named after the plan checkpoints.

use super::*;
use crate::tui::app::state::data::normalize_instance_path;
use std::path::Path;

#[test]
fn overview_renders_setup_panel_and_instance_table() {
    let app = App::new();
    let rendered = render_app(&app, 100, 30);
    assert!(rendered.contains("Current setup"), "{rendered}");
    assert!(rendered.contains("Instances"), "{rendered}");
}

#[test]
fn overview_enter_without_instance_switches_to_instances() {
    let mut app = App::new();
    app.page = Page::Overview;
    app.handle(key(KeyCode::Enter));
    assert_eq!(app.page, Page::Instances);
}

#[test]
fn instances_enter_opens_the_workspace() {
    let mut app = App::new();
    app.page = Page::Instances;
    app.instances.apply(vec![InstanceSummary {
        id: uuid::Uuid::new_v4(),
        name: "demo".into(),
        version: "1.20.1".into(),
        loader: "Vanilla".into(),
        loader_version: None,
        status: "ready".into(),
        path: std::path::PathBuf::from("C:/games/versions/demo"),
        launch_count: 0,
        play_time_seconds: 0,
    }]);
    app.handle(key(KeyCode::Enter));
    assert!(app.workspace_is_open());
    assert!(!app.detail_open);
}

#[test]
fn instances_filter_changes_visible_rows() {
    let mut app = App::new();
    app.instances.apply(vec![
        InstanceSummary {
            id: Uuid::new_v4(),
            name: "vanilla".into(),
            version: "1.21.1".into(),
            loader: "Vanilla".into(),
            loader_version: None,
            status: "ready".into(),
            path: PathBuf::from("vanilla"),
            launch_count: 0,
            play_time_seconds: 0,
        },
        InstanceSummary {
            id: Uuid::new_v4(),
            name: "fabric".into(),
            version: "1.21.1".into(),
            loader: "Fabric".into(),
            loader_version: None,
            status: "ready".into(),
            path: PathBuf::from("fabric"),
            launch_count: 0,
            play_time_seconds: 0,
        },
    ]);
    app.set_instance_filter(2);
    assert_eq!(app.visible_instance_indices(), vec![1]);
    app.set_instance_filter(1);
    assert_eq!(app.visible_instance_indices(), vec![0]);
}

#[test]
fn accounts_page_lists_device_code_when_login_pending() {
    let mut app = App::new();
    app.page = Page::Accounts;
    app.accounts.accounts = vec![Account {
        id: Uuid::new_v4(),
        kind: AccountKind::Offline,
        username: "player".into(),
        uuid: Uuid::new_v4(),
        skin: None,
        secret_ref: None,
        access_expires_at: None,
    }];
    app.accounts.device_code = Some(DeviceCodeView {
        user_code: "ABCD-1234".into(),
        verification_uri: "https://example.test".into(),
        remaining_seconds: 60,
        status: "pending".into(),
    });
    let rendered = render_app(&app, 120, 40);
    assert!(rendered.contains("ABCD-1234"), "{rendered}");
}

#[test]
fn logs_follow_scrolls_to_bottom_and_indicator_is_rendered() {
    let mut app = App::new();
    app.page = Page::Logs;
    app.terminal_width = 120;
    app.terminal_height = 40;
    for index in 0..40 {
        app.logs.append(format!("line {index}"));
    }
    app.logs.set_follow(true);
    let rendered = render_app(&app, 120, 40);
    assert!(rendered.contains("Live game log"), "{rendered}");
    assert!(rendered.contains("line 39"), "{rendered}");
}

#[test]
fn settings_edit_opens_form_with_current_value() {
    let mut app = App::new();
    app.page = Page::Settings;
    app.open_setting_editor(crate::tui::pages::settings::SettingId::WindowWidth);
    match &app.modal {
        Some(Modal::Prompt { value, .. }) => {
            assert_eq!(value, &app.config.launch.width.to_string());
        }
        other => panic!("expected a prompt modal, got {other:?}"),
    }
}

#[test]
fn download_search_query_appears_in_the_panel_title() {
    let mut app = App::new();
    app.page = Page::DownloadCenter;
    app.versions = vec![crate::core::version::models::VersionSummary {
        id: "1.21.1".into(),
        kind: "release".into(),
        url: String::new(),
        time: String::new(),
        release_time: "2024-08-08".into(),
        sha1: None,
        compliance_level: None,
    }];
    app.handle(key(KeyCode::Char('/')));
    for character in "1.21".chars() {
        app.handle(key(KeyCode::Char(character)));
    }
    app.handle(key(KeyCode::Enter));
    let rendered = render_app(&app, 120, 40);
    assert!(rendered.contains("Search: 1.21"), "{rendered}");
}

#[test]
fn download_search_form_renders_a_filled_input_row() {
    let mut app = App::new();
    app.page = Page::DownloadCenter;
    app.handle(key(KeyCode::Char('/')));
    let rendered = render_app(&app, 120, 40);
    assert!(rendered.contains("Search game versions"), "{rendered}");
    assert!(rendered.contains("Query: "), "{rendered}");

    let (width, height) = (120u16, 40u16);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("create test terminal");
    terminal.draw(|frame| app.draw(frame)).expect("draw app");
    let buffer = terminal.backend().buffer();
    let rect = crate::tui::chrome::prompt_rect(Rect::new(0, 0, width, height));
    let field = &buffer[(rect.x + 2, rect.y + 1)];
    assert_eq!(
        field.bg, app.theme.field_bg,
        "the dialog input must sit on the field fill"
    );
    assert_ne!(
        field.bg,
        ratatui::style::Color::Reset,
        "the field fill must be visible"
    );
}

#[test]
fn download_search_form_starts_from_the_current_query() {
    let mut app = App::new();
    app.page = Page::DownloadCenter;
    app.download_center.version_filter.set_search("1.21");
    app.handle(key(KeyCode::Char('/')));
    assert_eq!(
        app.modal.as_ref().and_then(Modal::prompt_value),
        Some("1.21")
    );
    // The caret starts after the existing text, so typing appends.
    app.handle(key(KeyCode::Char('1')));
    app.handle(key(KeyCode::Enter));
    assert_eq!(app.download_center.version_filter.search(), "1.211");
}

#[test]
fn download_enter_on_version_opens_the_create_form() {
    let mut app = App::new();
    app.page = Page::DownloadCenter;
    app.versions = vec![crate::core::version::models::VersionSummary {
        id: "1.21.1".into(),
        kind: "release".into(),
        url: String::new(),
        time: String::new(),
        release_time: "2024-08-08".into(),
        sha1: None,
        compliance_level: None,
    }];
    app.version_selected = 0;
    app.handle(key(KeyCode::Enter));
    // The list already shows type and release date, so Enter goes straight to
    // the creation form instead of a details popup repeating them.
    assert!(!app.detail_open, "no details popup on the way");
    match &app.modal {
        Some(Modal::Form { values, .. }) => assert_eq!(values[2], "1.21.1"),
        other => panic!("expected the create form, got {other:?}"),
    }

    // Opening the form from the details popup closes the popup instead of
    // stacking both dialogs.
    let mut app = App::new();
    app.page = Page::DownloadCenter;
    app.versions = vec![crate::core::version::models::VersionSummary {
        id: "1.21.1".into(),
        kind: "release".into(),
        url: String::new(),
        time: String::new(),
        release_time: "2024-08-08".into(),
        sha1: None,
        compliance_level: None,
    }];
    app.version_selected = 0;
    app.detail_open = true;
    app.handle(key(KeyCode::Enter));
    assert!(
        !app.detail_open,
        "details popup must close when the form opens"
    );
    assert!(matches!(app.modal, Some(Modal::Form { .. })));
}

#[test]
fn form_popup_keeps_fields_inside_80x24() {
    let mut app = App::new();
    app.open_game_directory_form();
    let rendered = render_app(&app, 80, 24);
    assert!(rendered.contains("directory"), "{rendered}");
}

#[test]
fn search_mode_accepts_parentheses_and_spaces() {
    let mut app = App::new();
    app.page = Page::Logs;
    app.handle(key(KeyCode::Char('/')));
    for character in "a (b) c".chars() {
        app.handle(key(KeyCode::Char(character)));
    }
    assert_eq!(app.log_filter.as_deref(), Some("a (b) c"));
}

#[test]
fn form_tab_cycles_fields() {
    let mut app = App::new();
    app.open_create_form();
    app.handle(key(KeyCode::Tab));
    match &app.modal {
        Some(Modal::Form { focused, .. }) => assert_eq!(*focused, 1),
        other => panic!("expected a form modal, got {other:?}"),
    }
}

#[test]
fn help_overlay_lists_current_page_hints() {
    let mut app = App::new();
    app.page = Page::Instances;
    app.handle(key(KeyCode::Char('?')));
    let rendered = render_app(&app, 120, 40);
    assert!(rendered.contains("Details"), "{rendered}");
    assert!(rendered.contains("Keyboard help"), "{rendered}");
}

#[test]
fn task_key_state_tracks_the_selected_task() {
    use crate::tui::command::TaskKeyState;

    let snapshot = |state: TaskState| TaskSnapshot {
        job: None,
        created_seq: 0,
        id: Uuid::new_v4(),
        label: "task".into(),
        operation_type: TaskOperationType::JavaRuntime,
        state,
        retryable: true,
        attempt: 1,
        completed: 0,
        total: Some(1),
        failure: None,
    };
    let mut app = App::new();
    app.page = Page::Tasks;
    app.task_snapshots = vec![snapshot(TaskState::Running)];
    app.task_selected = 0;
    assert_eq!(app.task_key_state(), TaskKeyState::Pause);

    app.task_snapshots = vec![snapshot(TaskState::Paused)];
    app.task_selected = 0;
    assert_eq!(app.task_key_state(), TaskKeyState::Resume);

    app.task_snapshots = vec![snapshot(TaskState::Failed)];
    app.task_selected = 0;
    assert_eq!(app.task_key_state(), TaskKeyState::Retry);
}

#[test]
fn tasks_filter_and_popup_show_error_text() {
    let mut app = App::new();
    app.page = Page::Tasks;
    app.task_snapshots = vec![TaskSnapshot {
        job: None,
        created_seq: 0,
        id: Uuid::new_v4(),
        label: "failed task".into(),
        operation_type: TaskOperationType::JavaRuntime,
        state: TaskState::Failed,
        retryable: true,
        attempt: 1,
        completed: 1,
        total: Some(2),
        failure: Some("boom".into()),
    }];
    app.task_selected = 0;
    app.handle(key(KeyCode::Enter));
    let rendered = render_app(&app, 120, 40);
    assert!(rendered.contains("boom"), "{rendered}");
}

#[test]
fn settings_rows_are_data_driven() {
    use crate::tui::pages::settings::SettingsCategory;

    let mut app = App::new();
    app.settings.category = SettingsCategory::General;
    let rows = app.settings_rows();
    assert_eq!(rows.len(), app.setting_ids().len());
    let before = rows[0].value.clone();
    app.config.launcher.visibility =
        crate::core::config::models::LauncherVisibility::ExitAfterLaunch;
    let after = app.settings_rows()[0].value.clone();
    assert_ne!(before, after);
}

#[test]
fn accent_cycle_includes_orange() {
    use crate::core::config::models::AccentColor;
    use crate::tui::pages::settings::SettingId;

    let mut app = App::new();
    app.config.state.ui.accent_color = AccentColor::Magenta;
    app.cycle_setting(SettingId::AccentColor, 1);
    assert_eq!(app.config.state.ui.accent_color, AccentColor::Orange);
}

#[test]
fn search_cursor_moves_and_inserts_at_position() {
    let mut app = App::new();
    app.page = Page::Logs;
    app.handle(key(KeyCode::Char('/')));
    app.handle(key(KeyCode::Char('a')));
    app.handle(key(KeyCode::Char('b')));
    app.handle(key(KeyCode::Left));
    app.handle(key(KeyCode::Char('X')));
    assert_eq!(app.log_filter.as_deref(), Some("aXb"));
    app.handle(key(KeyCode::Backspace));
    assert_eq!(app.log_filter.as_deref(), Some("ab"));
}

#[tokio::test]
async fn download_enter_on_resource_keeps_pane_not_popup() {
    let mut app = App::new();
    app.page = Page::DownloadCenter;
    app.download_center.category =
        crate::tui::pages::download_center::DownloadCategory::Resource(ResourceKind::Mod);
    app.resource_projects = vec![ResourceProject {
        source: ResourceSource::Modrinth,
        project_id: "project".into(),
        title: "Project".into(),
        author: "author".into(),
        description: "description".into(),
        icon_url: None,
        categories: Vec::new(),
        game_versions: Vec::new(),
        loaders: Vec::new(),
        downloads: None,
        updated_at: None,
    }];
    app.download_center.selected = 0;
    app.handle(key(KeyCode::Enter));
    assert!(!app.detail_open);
}

#[test]
fn download_resource_shows_detail_pane() {
    let mut app = App::new();
    app.page = Page::DownloadCenter;
    app.download_center.category =
        crate::tui::pages::download_center::DownloadCategory::Resource(ResourceKind::Mod);
    app.resource_projects = vec![ResourceProject {
        source: ResourceSource::Modrinth,
        project_id: "project".into(),
        title: "Project".into(),
        author: "author".into(),
        description: "description".into(),
        icon_url: None,
        categories: Vec::new(),
        game_versions: Vec::new(),
        loaders: Vec::new(),
        downloads: None,
        updated_at: None,
    }];
    app.download_center.selected = 0;
    let rendered = render_app(&app, 120, 40);
    assert!(rendered.contains("Details"), "{rendered}");
    assert!(rendered.contains("Project"), "{rendered}");
}

#[test]
fn list_navigation_keys_reach_the_edges() {
    let instances = |count: usize| {
        (0..count)
            .map(|index| InstanceSummary {
                id: Uuid::new_v4(),
                name: format!("instance {index}"),
                version: "1.21.1".into(),
                loader: "Vanilla".into(),
                loader_version: None,
                status: "ready".into(),
                path: PathBuf::from(format!("instance-{index}")),
                launch_count: 0,
                play_time_seconds: 0,
            })
            .collect::<Vec<_>>()
    };

    let mut app = App::new();
    app.page = Page::Instances;
    app.instances.apply(instances(15));
    app.handle(key(KeyCode::End));
    assert_eq!(app.instances.focused, 14);
    app.handle(key(KeyCode::Home));
    assert_eq!(app.instances.focused, 0);
    app.handle(key(KeyCode::PageDown));
    assert!(app.instances.focused > 0);

    let mut app = App::new();
    app.page = Page::Tasks;
    app.task_snapshots = (0..15)
        .map(|index| TaskSnapshot {
            job: None,
            created_seq: 0,
            id: Uuid::new_v4(),
            label: format!("task {index}"),
            operation_type: TaskOperationType::JavaRuntime,
            state: TaskState::Running,
            retryable: false,
            attempt: 1,
            completed: 0,
            total: Some(1),
            failure: None,
        })
        .collect();
    app.handle(key(KeyCode::End));
    assert_eq!(app.task_selected, 14);
    app.handle(key(KeyCode::Home));
    assert_eq!(app.task_selected, 0);

    let mut app = App::new();
    app.page = Page::Accounts;
    app.accounts.accounts = (0..15)
        .map(|index| Account {
            id: Uuid::new_v4(),
            kind: AccountKind::Offline,
            username: format!("user {index}"),
            uuid: Uuid::new_v4(),
            skin: None,
            secret_ref: None,
            access_expires_at: None,
        })
        .collect();
    app.handle(key(KeyCode::End));
    assert_eq!(app.accounts.selected, 14);
    app.handle(key(KeyCode::Home));
    assert_eq!(app.accounts.selected, 0);

    let mut app = App::new();
    app.page = Page::Settings;
    app.handle(key(KeyCode::End));
    let rows = app.settings_rows().len();
    assert_eq!(app.settings.selected, rows - 1);
    app.handle(key(KeyCode::Home));
    assert_eq!(app.settings.selected, 0);

    let mut app = App::new();
    app.page = Page::Logs;
    app.terminal_width = 120;
    app.terminal_height = 40;
    for index in 0..40 {
        app.logs.append(format!("line {index}"));
    }
    app.handle(key(KeyCode::PageUp));
    assert!(app.logs.scroll_offset() > 0);
    app.handle(key(KeyCode::End));
    assert_eq!(app.logs.scroll_offset(), 0);
}

#[test]
fn instances_page_shows_a_detail_pane() {
    let mut app = App::new();
    app.page = Page::Instances;
    app.instances.apply(vec![InstanceSummary {
        id: Uuid::new_v4(),
        name: "pane instance".into(),
        version: "1.21.1".into(),
        loader: "Fabric".into(),
        loader_version: None,
        status: "ready".into(),
        path: PathBuf::from("pane-instance"),
        launch_count: 0,
        play_time_seconds: 0,
    }]);
    let rendered = render_app(&app, 120, 40);
    assert!(rendered.contains("Details"), "{rendered}");
    assert!(rendered.contains("Java"), "{rendered}");
}

#[test]
fn overview_instance_table_scrolls_with_navigation() {
    let mut app = App::new();
    app.page = Page::Overview;
    app.instances.apply(
        (0..15)
            .map(|index| InstanceSummary {
                id: Uuid::new_v4(),
                name: format!("overview {index}"),
                version: "1.21.1".into(),
                loader: "Vanilla".into(),
                loader_version: None,
                status: "ready".into(),
                path: PathBuf::from(format!("overview-{index}")),
                launch_count: 0,
                play_time_seconds: 0,
            })
            .collect(),
    );
    app.handle(key(KeyCode::End));
    assert_eq!(app.instances.focused, 14);
    let rendered = render_app(&app, 120, 40);
    assert!(rendered.contains("overview 14"), "{rendered}");
}

#[test]
fn modpack_enter_opens_the_setup_form_before_queueing_a_task() {
    let mut app = App::new();
    app.page = Page::DownloadCenter;
    app.download_center.category =
        crate::tui::pages::download_center::DownloadCategory::Resource(ResourceKind::Modpack);
    app.resource_files = vec![crate::core::resource::model::ResourceFile {
        source: ResourceSource::Modrinth,
        project_id: "project".into(),
        file_id: "file".into(),
        name: "Example Pack.mrpack".into(),
        download_url: "https://example.invalid/pack.mrpack".into(),
        sha1: None,
        size: None,
        game_versions: vec!["1.20.1".into()],
        loaders: vec!["fabric".into()],
        channel: crate::core::resource::model::ReleaseChannel::Release,
        dependencies: Vec::new(),
    }];
    app.download_center.selected = 0;
    app.resource_detail = Some("project".into());
    let tasks_before = app.task_engine.snapshots().len();
    app.handle(key(KeyCode::Enter));
    let Some(Modal::Form { labels, values, .. }) = app.modal.as_ref() else {
        panic!("modpack setup form must open");
    };
    assert_eq!(labels.len(), 3);
    assert!(!labels.iter().any(|label| {
        label.contains("Minecraft")
            || label.contains("Loader")
            || label.contains("Isolated")
            || label.contains("Existing")
    }));
    assert!(values[0].contains("modpacks"), "{}", values[0]);
    // No project page was open, so there is no artwork to carry over.
    assert!(
        app.pending_modpack_file
            .as_ref()
            .is_some_and(|pending| pending.icon_url.is_none())
    );
    assert_eq!(app.task_engine.snapshots().len(), tasks_before);
}

/// The project's artwork travels with the file it belongs to: the install form
/// only holds a `ResourceFile`, and the icon lives on the project page.
#[test]
fn the_modpack_form_binds_the_project_artwork_to_the_file() {
    use crate::core::resource::model::{ReleaseChannel, ResourceProject, ResourceSource};

    let mut app = App::new();
    app.page = Page::DownloadCenter;
    app.download_center.category =
        crate::tui::pages::download_center::DownloadCategory::Resource(ResourceKind::Modpack);
    app.resource_files = vec![crate::core::resource::model::ResourceFile {
        source: ResourceSource::Modrinth,
        project_id: "project".into(),
        file_id: "file".into(),
        name: "Example Pack.mrpack".into(),
        download_url: "https://example.invalid/pack.mrpack".into(),
        sha1: None,
        size: None,
        game_versions: vec!["1.20.1".into()],
        loaders: vec!["fabric".into()],
        channel: ReleaseChannel::Release,
        dependencies: Vec::new(),
    }];
    app.download_center.selected = 0;
    app.resource_detail = Some("project".into());
    app.resource_detail_project = Some(ResourceProject {
        source: ResourceSource::Modrinth,
        project_id: "project".into(),
        title: "Example Pack".into(),
        author: "someone".into(),
        description: String::new(),
        icon_url: Some("https://cdn.example/art.png".into()),
        categories: Vec::new(),
        game_versions: vec!["1.20.1".into()],
        loaders: vec!["fabric".into()],
        downloads: None,
        updated_at: None,
    });

    app.handle(key(KeyCode::Enter));

    let pending = app
        .pending_modpack_file
        .as_ref()
        .expect("the form must hold the file");
    assert_eq!(
        pending.icon_url.as_deref(),
        Some("https://cdn.example/art.png"),
        "the artwork must be bound to the file it was picked with"
    );
    assert_eq!(pending.file.name, "Example Pack.mrpack");
}

#[test]
fn excluded_paths_use_one_normalized_key() {
    let mut app = App::new();
    let path = PathBuf::from(r"\\?\D:\game\pcl\.minecraft\versions\Example");
    app.config.excluded_instances.push(path.clone());
    app.remove_exclusion(Path::new(r"D:\game\pcl\.minecraft\versions\Example"));
    assert!(app.config.excluded_instances.is_empty());
    assert_eq!(
        normalize_instance_path(&path),
        normalize_instance_path(Path::new(r"d:\game\pcl\.minecraft\versions\example"))
    );
}

fn missing_files_instance(root: &std::path::Path) -> InstanceSummary {
    let instance_dir = root.join("game/versions/pack");
    std::fs::create_dir_all(&instance_dir).expect("instance dir");
    std::fs::write(instance_dir.join("pack.jar"), b"jar").expect("client jar");
    std::fs::write(
        instance_dir.join("pack.json"),
        br#"{"id":"pack","type":"release","mainClass":"net.minecraft.client.main.Main","arguments":{"game":[],"jvm":[]},"libraries":[{"name":"org.example:library:1.0","downloads":{"artifact":{"path":"org/example/library/1.0/library-1.0.jar","url":"https://example.invalid/library.jar","sha1":"def456"}}}]}"#,
    )
    .expect("client json");
    InstanceSummary {
        id: Uuid::new_v4(),
        name: "pack".into(),
        version: "1.20.1".into(),
        loader: "Vanilla".into(),
        loader_version: None,
        status: "ready".into(),
        path: instance_dir,
        launch_count: 0,
        play_time_seconds: 0,
    }
}

#[test]
fn unreadable_metadata_is_reported_as_metadata_invalid() {
    let root = std::env::temp_dir().join(format!("tmcl-broken-{}", Uuid::new_v4()));
    let instance_directory = root.join("game/versions/broken");
    std::fs::create_dir_all(&instance_directory).expect("instance dir");
    std::fs::write(instance_directory.join("broken.jar"), b"jar").expect("client jar");
    std::fs::write(instance_directory.join("broken.json"), b"{}").expect("broken json");
    let instance = InstanceSummary {
        id: Uuid::new_v4(),
        name: "broken".into(),
        version: "1.20.1".into(),
        loader: "Vanilla".into(),
        loader_version: None,
        status: "ready".into(),
        path: instance_directory,
        launch_count: 0,
        play_time_seconds: 0,
    };
    let mut app = App::new();
    app.instances.apply(vec![instance.clone()]);
    app.instances.set_current(Some(instance.id));

    app.launch_selected_instance();

    let expected = app.i18n.format(
        "status.launch_blocked",
        &[
            ("reason", &app.i18n.text("failure.metadata_invalid")),
            ("action", &app.i18n.text("failure.action_verify")),
        ],
    );
    assert_eq!(app.status.as_str(), expected.as_str());
    assert!(app.pending_launch.is_none(), "no repair can fix metadata");
    std::fs::remove_dir_all(root).expect("clean");
}

#[test]
fn launch_gate_names_the_missing_library() {
    let root = std::env::temp_dir().join(format!("tmcl-gate-{}", Uuid::new_v4()));
    let instance = missing_files_instance(&root);
    let mut app = App::new();
    app.instances.apply(vec![instance.clone()]);
    app.instances.set_current(Some(instance.id));

    let missing = app.launch_blockers(&instance).expect("blockers");

    assert_eq!(missing.len(), 1, "{missing:?}");
    assert!(missing[0].path.ends_with("library-1.0.jar"));
    assert_eq!(
        missing[0].role,
        crate::core::launch::plan::FileRole::Library
    );
    std::fs::remove_dir_all(root).expect("clean");
}

#[test]
fn complete_instance_has_no_launch_blockers() {
    let root = std::env::temp_dir().join(format!("tmcl-gate-{}", Uuid::new_v4()));
    let instance = missing_files_instance(&root);
    let library = root.join("game/libraries/org/example/library/1.0/library-1.0.jar");
    std::fs::create_dir_all(library.parent().expect("library dir")).expect("dirs");
    std::fs::write(&library, b"jar").expect("library");
    let mut app = App::new();
    app.instances.apply(vec![instance.clone()]);
    app.instances.set_current(Some(instance.id));

    assert!(app.launch_blockers(&instance).expect("blockers").is_empty());
    std::fs::remove_dir_all(root).expect("clean");
}

#[test]
fn repair_failure_detail_names_the_file_and_is_logged() {
    let root = std::env::temp_dir().join(format!("tmcl-repair-log-{}", Uuid::new_v4()));
    let (sender, _) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::with_state(
        crate::core::config::DataPaths::from_appdata(&root),
        crate::core::config::Config::default(),
        sender,
    );
    let instance_id = Uuid::new_v4();
    app.pending_launch = Some(instance_id);

    app.finish_repair_with_detail(
        instance_id,
        false,
        Some("library C:/game/libraries/x.jar failed: timeout"),
    );

    let expected = app.i18n.format(
        "status.repair_failed_detail",
        &[("error", "library C:/game/libraries/x.jar failed: timeout")],
    );
    assert_eq!(app.status.as_str(), expected.as_str());
    assert!(app.pending_launch.is_none());
    let log = std::fs::read_to_string(app.paths.launcher_logs_dir.join("launcher.log"))
        .expect("launcher log");
    assert!(log.contains("C:/game/libraries/x.jar"), "{log}");
    std::fs::remove_dir_all(root).expect("clean");
}

#[test]
fn offline_repair_failure_uses_the_offline_message() {
    let root = std::env::temp_dir().join(format!("tmcl-repair-offline-{}", Uuid::new_v4()));
    let (sender, _) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::with_state(
        crate::core::config::DataPaths::from_appdata(&root),
        crate::core::config::Config::default(),
        sender,
    );

    app.finish_repair_with_detail(
        Uuid::new_v4(),
        false,
        Some("offline: cannot download C:/game/x.jar; connect once"),
    );

    let expected = app.i18n.format(
        "status.repair_offline",
        &[("error", "cannot download C:/game/x.jar; connect once")],
    );
    assert_eq!(app.status.as_str(), expected.as_str());
    assert!(
        app.status.as_str().contains("C:/game/x.jar"),
        "the offline message still names the file"
    );
    std::fs::remove_dir_all(root).expect("clean");
}

#[test]
fn repair_failure_clears_the_pending_launch() {
    let mut app = App::new();
    let instance_id = Uuid::new_v4();
    app.pending_launch = Some(instance_id);

    app.finish_repair(instance_id, false);

    assert!(app.pending_launch.is_none());
    assert_eq!(
        app.status.kind(),
        crate::tui::widgets::status::StatusKind::Failed
    );
    assert_eq!(app.status.as_str(), app.i18n.text("status.repair_failed"));
}

#[test]
fn repair_success_resumes_the_blocked_launch() {
    let root = std::env::temp_dir().join(format!("tmcl-resume-{}", Uuid::new_v4()));
    let instance = missing_files_instance(&root);
    // The repair the launcher just ran has put the library in place.
    let library = root.join("game/libraries/org/example/library/1.0/library-1.0.jar");
    std::fs::create_dir_all(library.parent().expect("library dir")).expect("dirs");
    std::fs::write(&library, b"jar").expect("library");
    let mut app = App::new();
    app.instances.apply(vec![instance.clone()]);
    app.instances.set_current(Some(instance.id));
    app.pending_launch = Some(instance.id);

    app.finish_repair(instance.id, true);

    assert!(app.pending_launch.is_none());
    // The launch was attempted; without an account it stops at the account gate.
    assert_eq!(
        app.status.as_str(),
        app.i18n.text("status.account_for_launch")
    );
    std::fs::remove_dir_all(root).expect("clean");
}

#[test]
fn a_launch_after_repair_does_not_start_another_repair() {
    let root = std::env::temp_dir().join(format!("tmcl-retry-{}", Uuid::new_v4()));
    let instance = missing_files_instance(&root);
    let mut app = App::new();
    app.instances.apply(vec![instance.clone()]);
    app.instances.set_current(Some(instance.id));
    app.launch_after_repair = true;

    app.launch_selected_instance();

    assert!(
        app.pending_launch.is_none(),
        "no second repair may be queued"
    );
    assert_eq!(
        app.status.as_str(),
        app.i18n.text("status.repair_incomplete")
    );
    assert!(!app.launch_after_repair, "the retry flag is consumed");
    std::fs::remove_dir_all(root).expect("clean");
}

#[test]
fn auto_repair_setting_defaults_on_and_toggles_off() {
    // A private data root keeps the shared test store clean: this test writes
    // config and would otherwise leak into every later `App::new()`.
    let root = std::env::temp_dir().join(format!("tmcl-settings-{}", Uuid::new_v4()));
    let (sender, _) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::with_state(
        crate::core::config::DataPaths::from_appdata(&root),
        crate::core::config::Config::default(),
        sender,
    );
    assert!(
        app.config.launch.auto_repair,
        "default keeps the old behaviour"
    );

    app.toggle_setting(crate::tui::pages::settings::SettingId::AutoRepair);

    assert!(!app.config.launch.auto_repair);
    let saved: crate::core::config::models::Config =
        crate::core::config::storage::read_json(&app.paths.config_file).expect("config on disk");
    assert!(!saved.launch.auto_repair, "the toggle is persisted");
    std::fs::remove_dir_all(root).expect("clean");
}

#[test]
fn disabled_auto_repair_asks_before_downloading() {
    let root = std::env::temp_dir().join(format!("tmcl-confirm-{}", Uuid::new_v4()));
    let instance = missing_files_instance(&root);
    let mut app = App::new();
    app.config.launch.auto_repair = false;
    app.instances.apply(vec![instance.clone()]);
    app.instances.set_current(Some(instance.id));

    app.launch_selected_instance();

    assert!(
        app.pending_launch.is_none(),
        "no task starts without consent"
    );
    match &app.modal {
        Some(crate::tui::modal::Modal::Confirm { action, .. }) => {
            assert_eq!(
                *action,
                crate::tui::modal::ConfirmAction::RepairInstance(instance.id)
            );
        }
        other => panic!("expected a repair confirmation, got {other:?}"),
    }
    std::fs::remove_dir_all(root).expect("clean");
}

#[tokio::test]
async fn launch_with_missing_files_starts_repair_and_remembers_intent() {
    let root = std::env::temp_dir().join(format!("tmcl-auto-repair-{}", Uuid::new_v4()));
    let instance = missing_files_instance(&root);
    // Submitting a task writes into the task store, so keep it off the shared root.
    let data_root = std::env::temp_dir().join(format!("tmcl-data-{}", Uuid::new_v4()));
    let (sender, _) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::with_state(
        crate::core::config::DataPaths::from_appdata(&data_root),
        crate::core::config::Config::default(),
        sender,
    );
    assert!(
        app.config.launch.auto_repair,
        "the automatic branch is the default"
    );
    app.instances.apply(vec![instance.clone()]);
    app.instances.set_current(Some(instance.id));

    app.launch_selected_instance();

    assert_eq!(app.pending_launch, Some(instance.id));
    assert!(
        app.modal.is_none(),
        "auto repair must not ask for confirmation"
    );
    let expected = app
        .i18n
        .format("status.repair_started", &[("name", "pack"), ("count", "1")]);
    assert_eq!(app.status.as_str(), expected.as_str());
    drop(app);
    std::fs::remove_dir_all(root).expect("clean instance");
    std::fs::remove_dir_all(data_root).expect("clean data");
}

#[tokio::test]
async fn confirming_the_repair_prompt_starts_the_task() {
    let root = std::env::temp_dir().join(format!("tmcl-confirm-{}", Uuid::new_v4()));
    let instance = missing_files_instance(&root);
    // Isolated paths: submitting a task writes a record into the task store.
    let data_root = std::env::temp_dir().join(format!("tmcl-data-{}", Uuid::new_v4()));
    let (sender, _) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::with_state(
        crate::core::config::DataPaths::from_appdata(&data_root),
        crate::core::config::Config::default(),
        sender,
    );
    app.config.launch.auto_repair = false;
    app.instances.apply(vec![instance.clone()]);
    app.instances.set_current(Some(instance.id));

    app.confirm_action(crate::tui::modal::ConfirmAction::RepairInstance(
        instance.id,
    ))
    .expect("confirm repair");

    assert_eq!(app.pending_launch, Some(instance.id));
    drop(app);
    std::fs::remove_dir_all(root).expect("clean instance");
    std::fs::remove_dir_all(data_root).expect("clean data");
}

#[tokio::test]
async fn importing_a_local_modpack_copies_the_archive_and_keeps_the_original() {
    use std::io::Write as _;

    let base = std::env::temp_dir().join(format!("tmcl-local-modpack-{}", uuid::Uuid::new_v4()));
    let game_root = base.join("game");
    let cache = base.join("cache");
    std::fs::create_dir_all(&game_root).expect("create game root");
    std::fs::create_dir_all(&cache).expect("create cache");

    let source = base.join("My Pack.zip");
    {
        let file = std::fs::File::create(&source).expect("create archive");
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("manifest.json", options).expect("manifest");
        zip.write_all(
            br#"{"minecraft":{"version":"1.20.1","modLoaders":[{"id":"forge-47.4.22","primary":true}]}}"#,
        )
        .expect("write manifest");
        zip.start_file("overrides/mods/demo.jar", options)
            .expect("override file");
        zip.write_all(b"jar").expect("write jar");
        // An exported instance carries its metadata; importing must not copy it.
        zip.start_file("tmcl.json", options).expect("metadata");
        zip.write_all(b"{}").expect("write metadata");
        zip.finish().expect("finish archive");
    }

    let setup = crate::tui::app::PendingModpack {
        root: game_root.clone(),
        name: "My Pack".into(),
        icon_url: None,
    };
    let staging = cache.join("modpacks/copy.zip");
    let prepared = crate::tui::app::downloads_resources::prepare_local_modpack(
        setup,
        source.clone(),
        staging.clone(),
        crate::core::download::DownloadSourceSnapshot::default(),
    )
    .await
    .expect("prepare the local modpack");

    assert!(source.is_file(), "the player's archive must survive");
    assert!(
        !staging.exists(),
        "the cache copy is removed after extraction"
    );
    assert!(
        prepared.instance.path.join("mods/demo.jar").is_file(),
        "overrides are extracted into the new instance"
    );
    assert!(
        prepared.instance.path.join("tmcl.json").is_file(),
        "the instance gets fresh metadata"
    );
    let metadata = std::fs::read_to_string(prepared.instance.path.join("tmcl.json"))
        .expect("read instance metadata");
    assert!(
        metadata.contains("\"My Pack\""),
        "the archived snapshot must not replace the new identity: {metadata}"
    );
    assert_eq!(prepared.instance.minecraft_version, "1.20.1");
    assert!(
        prepared.files.is_empty() && prepared.unresolved.is_empty(),
        "a manifest without a files array queues nothing"
    );
    std::fs::remove_dir_all(base).expect("clean test directory");
}

#[tokio::test]
async fn a_missing_modpack_archive_never_creates_an_instance() {
    let base = std::env::temp_dir().join(format!("tmcl-local-missing-{}", uuid::Uuid::new_v4()));
    let game_root = base.join("game");
    std::fs::create_dir_all(&game_root).expect("create game root");
    let setup = crate::tui::app::PendingModpack {
        root: game_root.clone(),
        name: "missing".into(),
        icon_url: None,
    };
    let error = crate::tui::app::downloads_resources::prepare_local_modpack(
        setup,
        base.join("nope.zip"),
        base.join("cache/copy.zip"),
        crate::core::download::DownloadSourceSnapshot::default(),
    )
    .await
    .expect_err("a missing archive must fail");
    assert!(error.to_string().contains("nope.zip"), "{error}");
    assert!(
        !game_root.join("versions/missing").exists(),
        "nothing may be created when the archive is missing"
    );
    std::fs::remove_dir_all(base).expect("clean test directory");
}

#[test]
fn import_modpack_is_on_shift_i_and_opens_a_file_field() {
    use crate::tui::modal::ModalResult;

    let mut app = App::new();
    app.page = Page::Instances;
    app.handle(key(KeyCode::Char('I')));
    let Some(Modal::Form { action, values, .. }) = app.modal.as_ref() else {
        panic!("the import form must open");
    };
    assert_eq!(*action, FormAction::ImportLocalModpack);
    assert_eq!(values.len(), 3, "archive, game directory, instance name");
    // The archive field is the one wired to a file picker.
    let result = app
        .modal
        .as_mut()
        .expect("modal")
        .handle_key(crossterm::event::KeyEvent::new(
            KeyCode::F(2),
            crossterm::event::KeyModifiers::NONE,
        ));
    assert!(
        matches!(result, ModalResult::BrowseFile(_)),
        "F2 on the archive field must open a file picker: {result:?}"
    );
}

#[test]
fn submitting_the_import_form_without_a_file_reports_the_path() {
    let mut app = App::new();
    app.page = Page::Instances;
    let missing = std::env::temp_dir().join("tmcl-does-not-exist.zip");
    let error = app
        .submit_form(
            FormAction::ImportLocalModpack,
            vec![
                missing.display().to_string(),
                std::env::temp_dir().display().to_string(),
                "demo".into(),
            ],
        )
        .expect_err("a missing archive must be refused");
    assert!(error.to_string().contains("does-not-exist.zip"), "{error}");
    assert!(app.pending_modpack.is_none(), "no install may be staged");
}

#[test]
fn cancelling_the_import_form_keeps_the_players_archive() {
    let base = std::env::temp_dir().join(format!("tmcl-local-cancel-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&base).expect("create base");
    let source = base.join("pack.zip");
    std::fs::write(&source, b"zip").expect("write archive");

    let mut app = App::new();
    app.page = Page::Instances;
    app.open_local_modpack_form();
    if let Some(Modal::Form { values, .. }) = app.modal.as_mut() {
        values[0] = source.display().to_string();
    }
    app.settle_modal_result(crate::tui::modal::ModalResult::Cancel);
    assert!(app.modal.is_none(), "the dialog closes");
    assert!(
        source.is_file(),
        "cancelling must never delete the archive the player picked"
    );
    std::fs::remove_dir_all(base).expect("clean test directory");
}

#[test]
fn browsing_the_archive_fills_the_name_only_when_it_is_empty() {
    let mut app = App::new();
    app.page = Page::Instances;
    app.open_local_modpack_form();
    app.set_browsed_modpack_file(std::path::PathBuf::from(r"D:/packs/Example Pack.mrpack"));
    let Some(Modal::Form { values, .. }) = app.modal.as_ref() else {
        panic!("the import form must stay open");
    };
    assert_eq!(values[0], r"D:/packs/Example Pack.mrpack");
    assert_eq!(values[2], "Example Pack", "the name follows the archive");

    // A name the player typed is theirs; browsing must not overwrite it.
    app.set_browsed_modpack_file(std::path::PathBuf::from(r"D:/packs/Other.zip"));
    let Some(Modal::Form { values, .. }) = app.modal.as_ref() else {
        panic!("the import form must stay open");
    };
    assert_eq!(values[0], r"D:/packs/Other.zip");
    assert_eq!(values[2], "Example Pack", "a typed name is kept");
}

/// The strip used to drop the key line whenever the selection had a
/// description and the panel was narrower than 100 columns — which is every
/// terminal while the category rail is open.
#[test]
fn a_description_does_not_swallow_the_workspace_key_line() {
    use crate::core::content::{ContentEntry, ContentKind, ContentMeta};
    use crate::tui::pages::workspace::WorkspaceCategory;

    let root = std::env::temp_dir().join(format!("tmcl-note-strip-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("mods")).expect("create mods");
    let mut app = App::new();
    app.page = Page::Instances;
    app.config.state.ui.preset = crate::core::config::models::UiPreset::Creeper;
    app.motion = false;
    app.instances.apply(vec![InstanceSummary {
        id: uuid::Uuid::new_v4(),
        name: "demo".into(),
        version: "1.20.1".into(),
        loader: "Forge".into(),
        loader_version: None,
        status: "ready".into(),
        path: root.clone(),
        launch_count: 0,
        play_time_seconds: 0,
    }]);
    app.instances.focused = 0;
    app.open_workspace();
    if let Some(workspace) = app.workspace.as_mut() {
        workspace.category = WorkspaceCategory::Mods;
        workspace.loading = false;
    }
    app.apply_workspace_scan(
        ContentKind::Mod,
        vec![ContentEntry {
            name: "Alpha".into(),
            version: "1.0".into(),
            path: root.join("mods/alpha.jar"),
            size: 1024,
            enabled: true,
            order: None,
            badge: None,
            warning: None,
            meta: ContentMeta::Mod {
                id: "alpha".into(),
                name: "Alpha".into(),
                version: "1.0".into(),
                authors: Vec::new(),
                description: "Keeps the frame time honest".into(),
                depends: Vec::new(),
            },
        }],
        None,
        None,
    );
    let rendered = render_app(&app, 100, 30);
    assert!(
        rendered.contains("Keeps the frame time honest"),
        "the description must be shown: {rendered}"
    );
    assert!(
        rendered.contains("Toggle") || rendered.contains("Details"),
        "the key line must survive a description: {rendered}"
    );
    std::fs::remove_dir_all(root).expect("clean instance");
}

/// The classic dialog printed the raw i18n keys (`hint.browse`) instead of the
/// translated labels; only the creeper renderer went through `i18n`.
#[test]
fn classic_form_key_hints_are_translated() {
    let mut app = App::new();
    app.page = Page::Instances;
    app.config.state.ui.preset = crate::core::config::models::UiPreset::Classic;
    app.motion = false;
    app.open_local_modpack_form();
    let rendered = render_app(&app, 100, 30);
    assert!(
        !rendered.contains("hint."),
        "no raw i18n key may reach the screen: {rendered}"
    );
    assert!(
        rendered.contains("Browse") || rendered.contains("浏览"),
        "the browse hint must be translated: {rendered}"
    );
}

#[test]
fn overview_launch_target_is_the_current_instance_not_the_browsed_row() {
    let mut app = App::new();
    app.page = Page::Overview;
    let current = InstanceSummary {
        id: Uuid::new_v4(),
        name: "current".into(),
        version: "1.20.1".into(),
        loader: "Fabric".into(),
        loader_version: Some("0.15.11".into()),
        status: "ready".into(),
        path: PathBuf::from("current"),
        launch_count: 0,
        play_time_seconds: 0,
    };
    let browsed = InstanceSummary {
        id: Uuid::new_v4(),
        name: "browsed".into(),
        version: "1.20.1".into(),
        loader: "Forge".into(),
        loader_version: Some("47.4.22".into()),
        status: "ready".into(),
        path: PathBuf::from("browsed"),
        launch_count: 0,
        play_time_seconds: 0,
    };
    app.instances.apply(vec![current.clone(), browsed.clone()]);
    app.instances.set_current(Some(current.id));
    // The player clicked through the list without marking anything current.
    app.instances.focused = 1;

    assert_eq!(
        app.overview_launch_target().map(|item| item.name.as_str()),
        Some("current"),
        "the overview must launch the instance its card names"
    );

    // With nothing marked current the highlighted row is the only hint left.
    app.instances.set_current(None);
    assert_eq!(
        app.overview_launch_target().map(|item| item.name.as_str()),
        Some("browsed")
    );
}

#[test]
fn the_clock_and_accent_rows_are_disabled_under_a_fixed_nightlight_theme() {
    use crate::core::config::models::{NightlightTheme, UiPreset};
    use crate::tui::pages::settings::SettingId;

    let mut app = App::new();
    app.config.state.ui.preset = UiPreset::Nightlight;
    app.config.state.ui.nightlight_theme = NightlightTheme::Ember;

    let warming = app.setting_row_for(SettingId::LiveAccent).expect("row");
    assert!(
        !warming.enabled,
        "ember pins its accent; a live switch here promises a change that cannot happen"
    );
    assert!(!warming.disabled_reason.is_empty(), "{warming:?}");

    let accent = app.setting_row_for(SettingId::AccentColor).expect("row");
    assert!(!accent.enabled, "the fixed theme supplies the accent");

    // `live` is the theme that follows the clock, so both rows apply again.
    app.config.state.ui.nightlight_theme = NightlightTheme::Live;
    assert!(
        app.setting_row_for(SettingId::LiveAccent)
            .expect("row")
            .enabled
    );
    assert!(
        app.setting_row_for(SettingId::AccentColor)
            .expect("row")
            .enabled
    );

    // Classic and creeper read the accent colour from their own theme, so the
    // nightlight theme must not take the row away from them.
    app.config.state.ui.preset = UiPreset::Classic;
    app.config.state.ui.nightlight_theme = NightlightTheme::Ember;
    assert!(
        app.setting_row_for(SettingId::AccentColor)
            .expect("row")
            .enabled
    );
    assert!(
        !app.setting_row_for(SettingId::LiveAccent)
            .expect("row")
            .enabled
    );
}

/// Every reload rescans Java in the background. Announcing that result buried
/// the message the action produced: switching a game directory ended up saying
/// "found N runtimes" instead of which directory became current.
#[test]
fn a_background_java_rescan_keeps_the_message_the_action_produced() {
    let mut app = App::new();
    let switched = app
        .i18n
        .format("status.directory_active", &[("path", "D:/games")]);
    app.status = crate::tui::app::Status::info(switched.clone());

    // What `reload()` does: start a refresh, get the result later.
    app.start_java_scan();
    app.apply_java_scan(Vec::new());
    assert_eq!(
        app.status.as_str(),
        switched,
        "a background rescan must not steal the status line"
    );

    // The scan the player asked for still answers.
    app.scan_java();
    app.apply_java_scan(Vec::new());
    assert_eq!(
        app.status.as_str(),
        app.i18n.format("status.java_found", &[("count", "0")])
    );
}

/// `reload()` runs after every save and every finished task, and each run used
/// to spawn one `java -version` per install. Nothing about the answer changes
/// between those events, so it is reused until the inputs or the clock move.
#[test]
fn a_reload_with_unchanged_inputs_skips_the_java_rescan() {
    fn key_of(app: &App) -> (Vec<std::path::PathBuf>, Option<std::path::PathBuf>) {
        (
            app.config.game_directories.clone(),
            app.config.java.selected_path.clone(),
        )
    }

    let mut app = App::new();
    app.java_scan_in_flight = false;
    app.java_scan_key = Some(key_of(&app));
    app.java_scanned_at = Some(std::time::Instant::now());
    assert!(!app.java_scan_due(), "same inputs, a fresh answer to reuse");

    app.java_scanned_at = Some(std::time::Instant::now() - std::time::Duration::from_secs(600));
    assert!(app.java_scan_due(), "a stale answer has to be refreshed");

    app.java_scanned_at = Some(std::time::Instant::now());
    app.config
        .game_directories
        .push(std::path::PathBuf::from("D:/another-root"));
    assert!(
        app.java_scan_due(),
        "a new game directory can bring new runtimes"
    );

    app.java_scan_key = Some(key_of(&app));
    app.java_scan_in_flight = true;
    assert!(!app.java_scan_due(), "one scan at a time");

    // The key the player presses always runs, throttle or not.
    app.java_scan_in_flight = false;
    app.scan_java();
    assert_eq!(
        app.status.as_str(),
        app.i18n.text("status.java_scanning"),
        "an explicit rescan has to answer"
    );
}

/// A cover written after the list was drawn has to reach the screen now, not
/// after the cache's one-second interval: a finished install says so.
#[test]
fn refresh_cover_reads_the_new_icon_on_the_next_frame() {
    use std::io::BufWriter;

    let base = std::env::temp_dir().join(format!("tmcl-refresh-cover-{}", uuid::Uuid::new_v4()));
    let instance = base.join("game/versions/pack");
    std::fs::create_dir_all(&instance).expect("instance dir");

    let app = App::new();
    assert!(
        app.covers.borrow_mut().ensure(&instance).is_none(),
        "an empty instance has no cover"
    );

    let file = std::fs::File::create(instance.join("icon.png")).expect("icon");
    let mut encoder = png::Encoder::new(BufWriter::new(file), 1, 1);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(&[9, 8, 7, 255]).expect("png data");
    writer.finish().expect("finish png");

    // The cached answer is still the old one until the app is told.
    assert!(app.covers.borrow_mut().ensure(&instance).is_none());

    app.refresh_cover(&instance);
    let cover = app
        .covers
        .borrow_mut()
        .ensure(&instance)
        .expect("the install wrote a cover");
    assert_eq!(cover.pixel(0, 0), [9, 8, 7, 255]);
    std::fs::remove_dir_all(base).expect("clean");
}
