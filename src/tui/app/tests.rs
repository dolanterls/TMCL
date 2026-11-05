//! App tests moved out of app.rs.
mod mouse_contract;
mod mouse_hit;
mod plan_checks;
mod workspace_keys;
mod workspace_mouse;

mod render_checks;
mod structure;

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseEvent, MouseEventKind,
};
use ratatui::{
    Terminal,
    backend::TestBackend,
    layout::{Constraint, Direction, Layout, Rect},
    text::Line,
};
use uuid::Uuid;

use super::state::data::{game_directory_id, load_instances};
use super::{
    App, CREATE_LOADER_FIELD, CREATE_LOADER_VERSION_FIELD, CREATE_VERSION_FIELD, Page,
    resource_search_context,
};
use crate::core::account::models::{Account, AccountKind};
use crate::core::config::models::Language;
use crate::core::config::{Config, DataPaths};
use crate::core::download::DownloadSourceMode;
use crate::core::instance::path::normalize_game_directory;
use crate::core::java::JavaInstallation;
use crate::core::resource::model::{LoaderFilter, ResourceKind, ResourceProject, ResourceSource};
use crate::core::task::{TaskOperationType, TaskSnapshot, TaskState};
use crate::core::version::{
    loader_meta::{LoaderKind as MetadataLoaderKind, parse_loader_metadata},
    models::VersionSummary,
};
use crate::tui::app::state::status::Status;
use crate::tui::event::AppEvent;
use crate::tui::modal::{FormAction, Modal};
use crate::tui::pages::accounts::DeviceCodeView;
use crate::tui::pages::instances::InstanceSummary;
use crate::tui::pages::settings::{SettingId, SettingsCategory};

/// Builds a launch command for an instance summary.
///
/// The TUI no longer owns command assembly; this wraps the shared session API
/// so the tests below still express "what command would this instance launch".
fn build_for(
    paths: &DataPaths,
    config: &Config,
    installations: &[JavaInstallation],
    instance: &InstanceSummary,
    overrides: &crate::core::instance::overrides::LaunchOverrides,
    account: &Account,
    token: &str,
) -> crate::core::error::Result<crate::core::launch::command::CommandSpec> {
    let game_directory = crate::core::instance::path::game_directory_of(&instance.path)?;
    let mut instance_config = crate::core::instance::models::InstanceConfig::new(
        &instance.path,
        uuid::Uuid::new_v4(),
        &instance.name,
        &instance.version,
    );
    instance_config.overrides = overrides.clone();
    let resolved = crate::core::instance::resolve::ResolvedInstance {
        config: instance_config,
        game_directory,
        source: crate::core::instance::resolve::RefSource::Path,
    };
    let client = crate::core::launch::session::load_client(&resolved, None)?;
    let (spec, _) = crate::core::launch::session::build_command(
        crate::core::launch::session::CommandRequest {
            instance_directory: &resolved.config.path,
            instance_name: &resolved.config.name,
            minecraft_version: &resolved.config.minecraft_version,
            game_directory: &resolved.game_directory,
            config,
            overrides: &resolved.config.overrides,
            java_installations: installations,
            java_override: None,
            account,
            token,
            client: &client,
            logs_dir: &paths.game_logs_dir,
            min_memory_mb: None,
            max_memory_mb: None,
            width: None,
            height: None,
            fullscreen: None,
        },
    )?;
    Ok(spec)
}

fn key(code: KeyCode) -> AppEvent {
    AppEvent::Key(KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

#[tokio::test]
async fn tui_drains_queued_progress_events_before_redrawing() {
    let task_id = uuid::Uuid::new_v4();
    let (event_sender, mut events) = tokio::sync::mpsc::unbounded_channel();
    for completed in 1..=100 {
        event_sender
            .send(AppEvent::TasksUpdated(vec![TaskSnapshot {
                job: None,
                created_seq: 0,
                id: task_id,
                label: "Install test".into(),
                operation_type: TaskOperationType::JavaRuntime,
                state: TaskState::Running,
                retryable: false,
                attempt: 1,
                completed,
                total: Some(100),
                failure: None,
            }]))
            .expect("queue progress");
    }
    drop(event_sender);

    let mut app = App::new();
    app.handle(events.recv().await.expect("receive task update"));
    super::drain_pending_events(&mut app, &mut events);

    assert_eq!(app.task_snapshots[0].completed, 100);
}
#[test]
fn task_selection_follows_id_when_snapshot_order_changes() {
    let first_id = uuid::Uuid::new_v4();
    let second_id = uuid::Uuid::new_v4();
    let snapshot = |id: uuid::Uuid, label: &str| TaskSnapshot {
        id,
        job: None,
        created_seq: 0,
        label: label.into(),
        operation_type: TaskOperationType::JavaRuntime,
        state: TaskState::Running,
        retryable: false,
        attempt: 1,
        completed: 0,
        total: None,
        failure: None,
    };
    let first = snapshot(first_id, "first");
    let second = snapshot(second_id, "second");

    let mut app = App::new();
    app.apply_task_snapshots(vec![first, second.clone()]);
    app.set_task_selection(1);
    app.apply_task_snapshots(vec![second, snapshot(first_id, "first")]);

    assert_eq!(app.task_selected_id, Some(second_id));
    assert_eq!(app.task_selected, 0);
    assert_eq!(app.selected_task().map(|task| task.id), Some(second_id));
}
#[test]
fn failed_config_write_does_not_change_in_memory_settings() {
    let blocked_parent =
        std::env::temp_dir().join(format!("tmcl-config-blocked-{}", Uuid::new_v4()));
    std::fs::write(&blocked_parent, b"not a directory").expect("create blocking file");

    let mut app = App::new();
    app.paths.config_file = blocked_parent.join("config.json");
    let before = app.config.clone();
    assert!(
        app.save_config_change(|config| config.launch.fullscreen = !config.launch.fullscreen)
            .is_err()
    );
    assert_eq!(app.config, before);

    std::fs::remove_file(blocked_parent).expect("clean blocking file");
}
#[test]
fn failed_java_selection_write_keeps_the_previous_selection() {
    let blocked_parent =
        std::env::temp_dir().join(format!("tmcl-java-config-blocked-{}", Uuid::new_v4()));
    std::fs::write(&blocked_parent, b"not a directory").expect("create blocking file");

    let mut app = App::new();
    app.paths.config_file = blocked_parent.join("config.json");
    app.java.push(JavaInstallation {
        executable: PathBuf::from("C:/Java/jdk-21/bin/java.exe"),
        version: "21.0.1".into(),
        major: 21,
        vendor: None,
        is_jdk: true,
        lts: true,
    });
    app.java_selected = 0;
    let before = app.config.java.selected_path.clone();
    app.select_current_java();
    assert_eq!(app.config.java.selected_path, before);

    std::fs::remove_file(blocked_parent).expect("clean blocking file");
}

#[test]
fn settings_hints_follow_the_current_category() {
    let mut app = App::new();
    app.page = Page::Settings;
    app.settings
        .select_category(SettingsCategory::GameDirectories);
    let directory_keys = app
        .hints()
        .into_iter()
        .map(|hint| hint.key)
        .collect::<Vec<_>>();
    assert_eq!(directory_keys, ["Enter", "s", "x", "[ ]", "?", "Tab", "q"]);

    app.settings.select_category(SettingsCategory::Java);
    let java_keys = app
        .hints()
        .into_iter()
        .map(|hint| hint.key)
        .collect::<Vec<_>>();
    assert_eq!(
        java_keys,
        ["Enter", "a", "i", "x", "r", "[ ]", "?", "Tab", "q"]
    );
}

#[test]
fn every_page_hints_tab_and_quit() {
    for page in Page::ALL {
        let mut app = App::new();
        app.page = page;
        let keys = app
            .hints()
            .into_iter()
            .map(|hint| hint.key)
            .collect::<Vec<_>>();
        assert!(
            keys.iter().any(|key| key == "q"),
            "missing quit for {page:?}"
        );
        assert!(
            keys.iter().any(|key| key == "Tab"),
            "missing tab for {page:?}"
        );
    }
}

#[test]
fn hint_keys_are_dispatchable() {
    use crate::tui::command::{Command, Mode, from_key};

    for page in Page::ALL {
        let mut app = App::new();
        app.page = page;
        for hint in app.hints() {
            let code = match hint.key.as_str() {
                "Enter" => KeyCode::Enter,
                "Tab" => KeyCode::Tab,
                "Esc" => KeyCode::Esc,
                "q" => KeyCode::Char('q'),
                value if value.chars().count() == 1 => {
                    KeyCode::Char(value.chars().next().expect("single char"))
                }
                _ => continue,
            };
            assert_ne!(
                from_key(
                    Mode::Normal,
                    page,
                    KeyEvent {
                        code,
                        modifiers: KeyModifiers::NONE,
                        kind: KeyEventKind::Press,
                        state: KeyEventState::NONE,
                    },
                ),
                Command::Nothing,
                "hint {:?} for {page:?} is not dispatchable",
                hint.key
            );
        }
    }
}

#[test]
fn draining_a_large_event_queue_yields_for_a_redraw() {
    let (event_sender, mut events) = tokio::sync::mpsc::unbounded_channel();
    for index in 0..300 {
        event_sender
            .send(AppEvent::Status(format!("status-{index}")))
            .expect("queue status event");
    }

    let mut app = App::new();
    app.handle(events.try_recv().expect("receive first event"));
    super::drain_pending_events(&mut app, &mut events);

    assert!(
        events.try_recv().is_ok(),
        "the event drain must leave a large burst for the next draw cycle"
    );
}

#[test]
fn quit_keys_end_the_state_machine() {
    let mut app = App::new();
    app.handle(key(KeyCode::Char('q')));
    assert!(app.should_quit());

    let mut app = App::new();
    app.handle(key(KeyCode::Esc));
    assert!(app.should_quit());
}

#[test]
fn log_mouse_wheel_moves_the_viewport_in_the_wheel_direction() {
    let mut app = App::new();
    app.page = Page::Logs;
    app.terminal_width = 80;
    app.terminal_height = 24;
    let main = app.layout_chrome(Rect::new(0, 0, 80, 24)).main;
    for index in 0..20 {
        app.logs.append(format!("line {index}"));
    }
    let _ = render_app(&app, 80, 24);

    app.handle(AppEvent::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: main.x + 1,
        row: main.y + 1,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(app.logs.scroll_offset(), 3);

    app.handle(AppEvent::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: main.x + 1,
        row: main.y + 1,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(app.logs.scroll_offset(), 0);
}

#[test]
fn mouse_wheel_outside_the_active_main_view_does_not_scroll_logs() {
    let mut app = App::new();
    app.page = Page::Instances;
    app.terminal_width = 120;
    app.terminal_height = 40;
    for index in 0..20 {
        app.logs.append(format!("line {index}"));
    }
    let regions = app.layout_chrome(Rect::new(0, 0, 120, 40));
    let _ = render_app(&app, 120, 40);
    app.handle(AppEvent::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: regions.tabs.x + 1,
        row: regions.tabs.y,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(app.logs.scroll_offset(), 0);
}

/// The dialog prints its own keys now; the footer used to repeat them, which
/// put the same three hints on screen twice.
#[test]
fn a_form_dialog_owns_its_key_hints() {
    let mut app = App::new();
    app.page = Page::Settings;
    app.open_java_runtime_form();
    assert!(matches!(app.mode(), crate::tui::command::Mode::Form(_)));
    assert!(
        app.hints().is_empty(),
        "the footer must not repeat what the dialog already lists"
    );
    let rendered = render_app(&app, 120, 40);
    assert!(rendered.contains("Submit"), "{rendered}");
    assert!(rendered.contains("Cancel"), "{rendered}");

    // Several fields: the dialog carries the `Tab` cap the footer used to show.
    app.modal = Some(crate::tui::modal::Modal::form(
        "Create",
        &["Name".to_string(), "Directory".to_string()],
        &["demo".to_string(), "C:/games".to_string()],
        crate::tui::modal::FormAction::CreateInstance,
    ));
    let rendered = render_app(&app, 120, 40);
    assert!(rendered.contains("Next field"), "{rendered}");
}

#[test]
fn logs_slash_edits_inline_search() {
    let mut app = App::new();
    app.page = Page::Logs;
    app.handle(key(KeyCode::Char('/')));
    assert!(matches!(app.mode(), crate::tui::command::Mode::Search(_)));
    app.handle(key(KeyCode::Char('e')));
    app.handle(key(KeyCode::Char('r')));
    assert_eq!(app.log_filter.as_deref(), Some("er"));
    app.handle(key(KeyCode::Enter));
    assert_eq!(app.mode(), crate::tui::command::Mode::Normal);
    app.handle(key(KeyCode::Char('/')));
    app.handle(key(KeyCode::Esc));
    assert_eq!(app.log_filter, None);
    assert_eq!(app.mode(), crate::tui::command::Mode::Normal);
}

#[tokio::test]
async fn resource_slash_opens_the_search_form() {
    let mut app = App::new();
    app.page = Page::DownloadCenter;
    app.download_center.category =
        crate::tui::pages::download_center::DownloadCategory::Resource(ResourceKind::Mod);
    app.handle(key(KeyCode::Char('/')));
    assert!(
        matches!(app.mode(), crate::tui::command::Mode::Form(_)),
        "slash opens a dialog instead of an inline prompt"
    );
    assert!(app.modal.is_some());
    app.handle(key(KeyCode::Char('o')));
    app.handle(key(KeyCode::Char('p')));
    // Nothing is applied until the form is submitted.
    assert_eq!(app.download_center.search, "");
    app.handle(key(KeyCode::Enter));
    assert_eq!(app.download_center.search, "op");
    assert!(app.modal.is_none());
    assert_eq!(app.mode(), crate::tui::command::Mode::Normal);
}

#[test]
fn version_filter_search_uses_the_form() {
    let mut app = App::new();
    app.page = Page::DownloadCenter;
    app.download_center.version_filter.set_search("1.20");
    app.handle(key(KeyCode::Char('/')));
    assert!(matches!(app.mode(), crate::tui::command::Mode::Form(_)));
    // The form starts from the current query, so the caret sits at its end.
    assert_eq!(
        app.modal.as_ref().and_then(Modal::prompt_value),
        Some("1.20")
    );
    app.handle(key(KeyCode::Char('1')));
    assert_eq!(app.download_center.version_filter.search(), "1.20");
    app.handle(key(KeyCode::Enter));
    assert_eq!(app.download_center.version_filter.search(), "1.201");
    assert_eq!(app.mode(), crate::tui::command::Mode::Normal);
}

#[test]
fn download_search_form_cancels_without_touching_the_filter() {
    let mut app = App::new();
    app.page = Page::DownloadCenter;
    app.download_center.version_filter.set_search("1.20");
    app.handle(key(KeyCode::Char('/')));
    for character in "abc".chars() {
        app.handle(key(KeyCode::Char(character)));
    }
    app.handle(key(KeyCode::Esc));
    assert!(app.modal.is_none());
    assert_eq!(app.download_center.version_filter.search(), "1.20");
}

#[test]
fn too_small_terminal_renders_the_shell_and_quit_hint() {
    let mut app = App::new();
    app.terminal_width = 79;
    app.terminal_height = 23;
    let rendered = render_app(&app, 79, 23);
    assert!(rendered.contains("tmcl"));
    assert!(rendered.contains("Quit"));
}

#[test]
fn clicking_third_tab_opens_tasks() {
    let mut app = App::new();
    app.terminal_width = 120;
    app.terminal_height = 40;
    let regions = app.layout_chrome(Rect::new(0, 0, 120, 40));
    let labels = app.tab_labels(app.layout_chrome(Rect::new(0, 0, 120, 40)).mode);
    let rects = crate::tui::chrome::tab_rects(regions.tabs, &labels);
    let tasks = rects[3];
    // Tabs are clickable through the frame's hit map, so a frame must exist.
    let _ = render_app(&app, 120, 40);
    app.handle(AppEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column: tasks.x,
        row: tasks.y,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(app.page, Page::Tasks);
}

#[test]
fn overview_enter_launches_the_current_instance_instead_of_navigating_away() {
    let mut app = App::new();
    let id = Uuid::new_v4();
    app.instances.apply(vec![InstanceSummary {
        id,
        name: "current".into(),
        version: "1.21.1".into(),
        loader: "Vanilla".into(),
        loader_version: None,
        status: "ready".into(),
        path: PathBuf::from("current"),
        launch_count: 0,
        play_time_seconds: 0,
    }]);
    app.instances.set_current(Some(id));
    app.activate_selection();
    assert_eq!(app.page, Page::Overview);
    assert!(app.status.contains("account"));
}

#[test]
fn settings_forms_cancel_without_mutating_configuration() {
    let mut app = App::new();
    app.page = Page::Settings;
    let width = app.config.launch.width;
    app.open_setting_editor(SettingId::WindowWidth);
    assert!(matches!(app.modal, Some(Modal::Prompt { .. })));
    app.handle(key(KeyCode::Esc));
    assert!(app.modal.is_none());
    assert_eq!(app.config.launch.width, width);
}

#[test]
fn game_settings_use_single_value_editors() {
    let mut app = App::new();

    app.open_setting_editor(SettingId::MemoryMinimum);
    assert!(matches!(
        app.modal.as_ref(),
        Some(Modal::Prompt {
            action: FormAction::SettingValue(SettingId::MemoryMinimum),
            ..
        })
    ));

    app.open_setting_editor(SettingId::WindowWidth);
    assert!(matches!(
        app.modal.as_ref(),
        Some(Modal::Prompt {
            action: FormAction::SettingValue(SettingId::WindowWidth),
            ..
        })
    ));

    app.open_setting_editor(SettingId::JvmArguments);
    assert!(matches!(
        app.modal.as_ref(),
        Some(Modal::Prompt {
            action: FormAction::SettingValue(SettingId::JvmArguments),
            ..
        })
    ));
}

#[test]
fn game_setting_editors_commit_only_their_domain() {
    let root = std::env::temp_dir().join(format!("tmcl-settings-editors-{}", Uuid::new_v4()));
    let paths = DataPaths::from_appdata(root.join("AppData"));
    paths.ensure_dirs().expect("create data paths");
    let (sender, _) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::with_state(paths, Config::default(), sender);

    app.submit_form(
        FormAction::SettingValue(SettingId::MemoryMinimum),
        vec!["2048".into()],
    )
    .expect("save minimum memory");
    app.submit_form(
        FormAction::SettingValue(SettingId::MemoryMaximum),
        vec!["4096".into()],
    )
    .expect("save maximum memory");
    assert_eq!(app.config.launch.min_memory_mb, 2048);
    assert_eq!(app.config.launch.max_memory_mb, 4096);
    assert_eq!(app.config.launch.width, 1280);
    assert!(!app.config.launch.fullscreen);
    assert!(app.config.launch.jvm_args.is_empty());

    app.submit_form(
        FormAction::SettingValue(SettingId::WindowWidth),
        vec!["1920".into()],
    )
    .expect("save window width");
    app.submit_form(
        FormAction::SettingValue(SettingId::WindowHeight),
        vec!["1080".into()],
    )
    .expect("save window height");
    app.cycle_setting(SettingId::WindowMode, 1);
    assert_eq!(app.config.launch.width, 1920);
    assert_eq!(app.config.launch.height, 1080);
    assert!(app.config.launch.fullscreen);
    assert_eq!(app.config.launch.min_memory_mb, 2048);

    app.submit_form(
        FormAction::SettingValue(SettingId::JvmArguments),
        vec!["-Dfoo=bar -Xmx4G".into()],
    )
    .expect("save JVM arguments");
    assert_eq!(app.config.launch.jvm_args, ["-Dfoo=bar", "-Xmx4G"]);
    assert_eq!(app.config.launch.game_args, Vec::<String>::new());
    assert_eq!(app.config.launch.width, 1920);

    app.submit_form(
        FormAction::SettingValue(SettingId::GameArguments),
        vec!["--demo --fullscreen".into()],
    )
    .expect("save game arguments");
    assert_eq!(app.config.launch.game_args, ["--demo", "--fullscreen"]);
    assert_eq!(app.config.launch.jvm_args, ["-Dfoo=bar", "-Xmx4G"]);

    std::fs::remove_dir_all(root).expect("clean settings test files");
}

#[test]
fn settings_show_advanced_options_without_toggle() {
    let mut app = App::new();
    app.settings.category = SettingsCategory::Game;

    assert_eq!(
        app.setting_ids(),
        vec![
            SettingId::JavaSelection,
            SettingId::MemoryMode,
            SettingId::MemoryMinimum,
            SettingId::MemoryMaximum,
            SettingId::WindowMode,
            SettingId::WindowWidth,
            SettingId::WindowHeight,
            SettingId::AutoRepair,
            SettingId::JvmArguments,
            SettingId::GameArguments,
        ]
    );

    app.settings.category = SettingsCategory::Downloads;
    assert_eq!(
        app.setting_ids(),
        vec![
            SettingId::VersionSource,
            SettingId::FileSource,
            SettingId::AutoDownloadThreads,
            SettingId::DownloadThreads,
            SettingId::CacheDirectory,
            SettingId::CacheLimit,
            SettingId::CacheSize,
            SettingId::RetryCount,
            SettingId::Timeout,
        ]
    );

    app.settings.category = SettingsCategory::Logs;
    assert_eq!(
        app.setting_ids(),
        vec![
            SettingId::LauncherLogs,
            SettingId::GameDebug,
            SettingId::LogRetention,
            SettingId::GameLogs,
            SettingId::LauncherLogDirectory,
            SettingId::ExportLauncherLogs,
            SettingId::ExportGameLogs,
            SettingId::ExportCrashReport,
        ]
    );
}

#[test]
fn argument_editors_round_trip_quoted_values() {
    let arguments = vec![
        "-Dmessage=hello world".to_owned(),
        "--name".to_owned(),
        "A \"quoted\" value".to_owned(),
        r"C:\Games\Minecraft".to_owned(),
        String::new(),
    ];

    assert_eq!(super::split_args(&super::join_args(&arguments)), arguments);
}

#[test]
fn chinese_render_translates_shell_and_preserves_loader_names() {
    let mut app = App::new();
    app.config.state.ui.language = Language::SimplifiedChinese;
    app.i18n = crate::i18n::I18n::new(Language::SimplifiedChinese);
    app.status = Status::info(app.i18n.text("status.ready"));
    app.page = Page::Instances;
    app.instances.apply(vec![
        InstanceSummary {
            id: Uuid::new_v4(),
            name: "fabric-instance".into(),
            version: "1.20.1".into(),
            loader: "fabric".into(),
            loader_version: Some("0.16.5".into()),
            status: "ready".into(),
            path: PathBuf::from("fabric-instance"),
            launch_count: 0,
            play_time_seconds: 0,
        },
        InstanceSummary {
            id: Uuid::new_v4(),
            name: "forge-instance".into(),
            version: "1.20.1".into(),
            loader: "forge".into(),
            loader_version: Some("47.3.0".into()),
            status: "ready".into(),
            path: PathBuf::from("forge-instance"),
            launch_count: 0,
            play_time_seconds: 0,
        },
        InstanceSummary {
            id: Uuid::new_v4(),
            name: "neoforge-instance".into(),
            version: "1.21.1".into(),
            loader: "neoforge".into(),
            loader_version: Some("21.1.1".into()),
            status: "ready".into(),
            path: PathBuf::from("neoforge-instance"),
            launch_count: 0,
            play_time_seconds: 0,
        },
        InstanceSummary {
            id: Uuid::new_v4(),
            name: "vanilla-instance".into(),
            version: "1.21.1".into(),
            loader: "vanilla".into(),
            loader_version: None,
            status: "ready".into(),
            path: PathBuf::from("vanilla-instance"),
            launch_count: 0,
            play_time_seconds: 0,
        },
    ]);

    let rendered = render_app(&app, 120, 40);
    assert!(rendered.contains('实'));
    assert!(rendered.contains('例'));
    assert!(rendered.contains("fabric"));
    assert!(rendered.contains("forge"));
    assert!(rendered.contains("neoforge"));
    assert!(rendered.contains("vanilla"));

    app.page = Page::Settings;
    app.settings.select_category(SettingsCategory::Appearance);
    let rendered = render_app(&app, 120, 40);
    assert!(rendered.contains('语'));
    assert!(rendered.contains('简'));
}

#[test]
fn language_cycles_at_runtime() {
    let mut app = App::new();
    app.page = Page::Settings;
    app.settings.select_category(SettingsCategory::Appearance);

    app.cycle_setting(SettingId::Language, 1);
    assert_eq!(app.config.state.ui.language, Language::SimplifiedChinese);
    assert_eq!(app.i18n.language(), Language::SimplifiedChinese);

    app.cycle_setting(SettingId::Language, 1);
    assert_eq!(app.config.state.ui.language, Language::English);
    assert_eq!(app.i18n.language(), Language::English);
}

#[tokio::test]
async fn mouse_can_activate_page_tabs() {
    let mut app = App::new();
    app.page = Page::Tasks;
    app.terminal_width = 120;
    app.terminal_height = 40;
    let task_tabs = App::task_page_sections(app.layout_chrome(Rect::new(0, 0, 120, 40)).main)[0];
    click_page(&mut app, task_tabs.x + 12, task_tabs.y);
    assert_eq!(
        app.task_filter,
        crate::tui::pages::tasks::TaskFilter::Running
    );

    app.page = Page::DownloadCenter;
    let download_tabs = app
        .download_page_sections(app.layout_chrome(Rect::new(0, 0, 120, 40)).main)
        .expect("download sections")[1];
    click_page(&mut app, download_tabs.x + 18, download_tabs.y);
    assert_eq!(
        app.download_center.category,
        crate::tui::pages::download_center::DownloadCategory::Resource(
            crate::core::resource::model::ResourceKind::Mod
        )
    );
}

#[tokio::test]
async fn translated_download_tabs_hit_the_rendered_category() {
    let mut app = App::new();
    app.page = Page::DownloadCenter;
    app.i18n = crate::i18n::I18n::new(Language::SimplifiedChinese);
    app.terminal_width = 120;
    app.terminal_height = 40;
    let tabs = app
        .download_page_sections(app.layout_chrome(Rect::new(0, 0, 120, 40)).main)
        .expect("download sections")[1];
    let labels = ["游戏版本", "模组", "整合包", "资源包", "光影", "世界"];
    let mut mod_start = usize::from(tabs.x + 1);
    mod_start += Line::from(labels[0]).width() + 3;
    click_page(&mut app, (mod_start + 1) as u16, tabs.y);
    assert_eq!(
        app.download_center.category,
        crate::tui::pages::download_center::DownloadCategory::Resource(
            crate::core::resource::model::ResourceKind::Mod
        )
    );

    let resource_pack_start =
        mod_start + Line::from(labels[1]).width() + 3 + Line::from(labels[2]).width() + 3;
    click_page(&mut app, (resource_pack_start + 1) as u16, tabs.y);
    assert_eq!(
        app.download_center.category,
        crate::tui::pages::download_center::DownloadCategory::Resource(
            crate::core::resource::model::ResourceKind::ResourcePack
        )
    );
}

#[test]
fn narrow_layout_keeps_a_single_column() {
    let app = App::new();
    let regions = app.layout_chrome(Rect::new(0, 0, 95, 29));
    assert_eq!(regions.main, regions.content);
}

fn click_at(app: &mut App, column: u16, row: u16) {
    app.handle(AppEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }));
}

/// Draws the current page and then clicks: hit regions belong to a frame, so a
/// click without a preceding draw has nothing to resolve against.
fn click_page(app: &mut App, column: u16, row: u16) {
    let _ = render_app(app, app.terminal_width, app.terminal_height);
    click_at(app, column, row);
}

fn left_click(app: &mut App, row: u16) {
    let area = Rect::new(0, 0, app.terminal_width, app.terminal_height);
    let main = app.layout_chrome(area).main;
    // Hit regions come from the frame, so draw one before clicking.
    let _ = render_app(app, app.terminal_width, app.terminal_height);
    click_at(app, main.x + 1, row);
}

#[test]
fn clicking_first_row_selects_first_item() {
    let mut app = App::new();
    app.terminal_width = 120;
    app.terminal_height = 40;
    let first_row = app.layout_chrome(Rect::new(0, 0, 120, 40)).main.y + 2;

    app.page = Page::Instances;
    app.instances.apply(vec![
        InstanceSummary {
            id: Uuid::new_v4(),
            name: "first".into(),
            version: "1.20.1".into(),
            loader: "Vanilla".into(),
            loader_version: None,
            status: "ready".into(),
            path: PathBuf::from("first"),
            launch_count: 0,
            play_time_seconds: 0,
        },
        InstanceSummary {
            id: Uuid::new_v4(),
            name: "second".into(),
            version: "1.21.1".into(),
            loader: "Vanilla".into(),
            loader_version: None,
            status: "ready".into(),
            path: PathBuf::from("second"),
            launch_count: 0,
            play_time_seconds: 0,
        },
    ]);
    app.instances.focused = 1;
    left_click(&mut app, first_row + 1);
    assert_eq!(app.instances.focused, 0, "instances first row");

    app.page = Page::Accounts;
    app.accounts.accounts = vec![
        Account {
            id: Uuid::new_v4(),
            kind: AccountKind::Offline,
            username: "first".into(),
            uuid: Uuid::new_v4(),
            skin: None,
            secret_ref: None,
            access_expires_at: None,
        },
        Account {
            id: Uuid::new_v4(),
            kind: AccountKind::Offline,
            username: "second".into(),
            uuid: Uuid::new_v4(),
            skin: None,
            secret_ref: None,
            access_expires_at: None,
        },
    ];
    app.accounts.selected = 1;
    left_click(&mut app, first_row);
    assert_eq!(app.accounts.selected, 0, "accounts first row");

    app.page = Page::Settings;
    app.settings.select_category(SettingsCategory::Java);
    app.java = vec![
        JavaInstallation {
            executable: PathBuf::from("java-first"),
            version: "21.0.1".into(),
            major: 21,
            vendor: None,
            is_jdk: true,
            lts: true,
        },
        JavaInstallation {
            executable: PathBuf::from("java-second"),
            version: "17.0.1".into(),
            major: 17,
            vendor: None,
            is_jdk: true,
            lts: true,
        },
    ];
    app.java_selected = 1;
    let java_main = app
        .layout_chrome(Rect::new(0, 0, app.terminal_width, app.terminal_height))
        .main;
    click_page(&mut app, java_main.x + 25, java_main.y + 3);
    assert_eq!(app.java_selected, 0, "java first row");

    app.page = Page::Settings;
    app.config.game_directories = vec![PathBuf::from("games")];
    app.settings
        .select_category(SettingsCategory::GameDirectories);
    let settings_main = app
        .layout_chrome(Rect::new(0, 0, app.terminal_width, app.terminal_height))
        .main;
    click_page(&mut app, settings_main.x + 25, settings_main.y + 3);
    assert_eq!(app.settings.selected, 0, "settings first row");

    app.page = Page::Tasks;
    let first_task_id = Uuid::new_v4();
    let second_task_id = Uuid::new_v4();
    let task = |id: Uuid, label: &str| TaskSnapshot {
        id,
        job: None,
        created_seq: 0,
        label: label.into(),
        operation_type: TaskOperationType::JavaRuntime,
        state: TaskState::Queued,
        retryable: false,
        attempt: 1,
        completed: 0,
        total: Some(1),
        failure: None,
    };
    app.task_snapshots = vec![task(first_task_id, "first"), task(second_task_id, "second")];
    app.task_selected = 1;
    app.task_selected_id = Some(second_task_id);
    let first_task_row = first_row + 1;
    left_click(&mut app, first_task_row);
    assert_eq!(app.task_selected, 0, "tasks first row");
    assert_eq!(app.task_selected_id, Some(first_task_id));
}

#[test]
fn mouse_click_on_first_download_row_selects_first_version() {
    let mut app = App::new();
    app.page = Page::DownloadCenter;
    app.terminal_width = 120;
    app.terminal_height = 40;
    app.versions = vec![
        VersionSummary {
            id: "first".into(),
            kind: "release".into(),
            url: String::new(),
            time: String::new(),
            release_time: String::new(),
            sha1: None,
            compliance_level: None,
        },
        VersionSummary {
            id: "second".into(),
            kind: "release".into(),
            url: String::new(),
            time: String::new(),
            release_time: String::new(),
            sha1: None,
            compliance_level: None,
        },
    ];
    app.version_selected = 1;

    let main = app.layout_chrome(Rect::new(0, 0, 120, 40)).main;
    let list_area = app
        .current_download_page_sections(main)
        .expect("download sections")[4];
    let first_row = list_area.y + 2;
    left_click(&mut app, first_row);

    assert_eq!(app.version_selected, 0);
}

#[test]
fn download_game_versions_support_category_filters_and_search() {
    let version = |id: &str, kind: &str, release_time: &str| VersionSummary {
        id: id.into(),
        kind: kind.into(),
        url: String::new(),
        time: release_time.into(),
        release_time: release_time.into(),
        sha1: None,
        compliance_level: None,
    };
    let mut app = App::new();
    app.page = Page::DownloadCenter;
    app.terminal_width = 120;
    app.terminal_height = 40;
    app.versions = vec![
        version("1.21.1", "release", "2024-09-01T00:00:00Z"),
        version("25w15a", "snapshot", "2025-04-08T00:00:00Z"),
        version("25w13a", "snapshot", "2025-03-24T00:00:00Z"),
    ];

    let rendered = render_app(&app, 120, 40);
    assert!(rendered.contains("1.21.1"));
    assert!(!rendered.contains("25w15a"));

    app.handle(key(KeyCode::Char('2')));
    let rendered = render_app(&app, 120, 40);
    assert!(rendered.contains("1.21.1"));
    assert!(rendered.contains("25w15a"));

    app.handle(key(KeyCode::Char('/')));
    for character in "25W15A".chars() {
        app.handle(key(KeyCode::Char(character)));
    }
    app.handle(key(KeyCode::Enter));
    let rendered = render_app(&app, 120, 40);
    assert!(rendered.contains("25w15a"));
    assert!(!rendered.contains("25w13a"));
    assert!(!rendered.contains("1.21.1"));
    assert_eq!(app.version_selected, 0);
}

fn creeper_download_app(category: crate::tui::pages::download_center::DownloadCategory) -> App {
    let mut app = App::new();
    app.page = Page::DownloadCenter;
    app.config.state.ui.preset = crate::core::config::models::UiPreset::Creeper;
    app.i18n = crate::i18n::I18n::new(Language::SimplifiedChinese);
    // Freeze the animation clock so the caret and pulse are deterministic.
    app.motion = false;
    app.terminal_width = 120;
    app.terminal_height = 40;
    app.download_center.category = category;
    app
}

/// `TestBackend` pads every wide glyph with a blank cell, so CJK assertions
/// compare against a space-free copy of the frame.
fn compact_frame(rendered: &str) -> String {
    rendered.replace(' ', "")
}

#[test]
fn creeper_download_search_bar_is_always_on_screen_and_escape_cancels() {
    let version = |id: &str| VersionSummary {
        id: id.into(),
        kind: "release".into(),
        url: String::new(),
        time: "2024-09-01T00:00:00Z".into(),
        release_time: "2024-09-01T00:00:00Z".into(),
        sha1: None,
        compliance_level: None,
    };
    let mut app =
        creeper_download_app(crate::tui::pages::download_center::DownloadCategory::GameVersions);
    app.versions = vec![version("1.21.1"), version("1.20.1")];

    // The field is part of the page: visible before any key press, no dialog.
    let rendered = render_app(&app, 120, 40);
    assert!(compact_frame(&rendered).contains("输入搜索"), "{rendered}");
    assert!(app.modal.is_none(), "search must not open a modal");
    assert!(app.search.is_none());

    // `/` only moves focus; typed text filters the list live.
    app.handle(key(KeyCode::Char('/')));
    assert!(matches!(app.mode(), crate::tui::command::Mode::Search(_)));
    assert!(compact_frame(&render_app(&app, 120, 40)).contains("Enter搜索"));
    for character in "1.20".chars() {
        app.handle(key(KeyCode::Char(character)));
    }
    assert_eq!(app.download_center.version_filter.search(), "1.20");
    let rendered = render_app(&app, 120, 40);
    assert!(rendered.contains("1.20.1"), "{rendered}");
    assert!(!rendered.contains("1.21.1"), "{rendered}");

    // Esc restores the query the list was showing and leaves the field.
    app.handle(key(KeyCode::Esc));
    assert!(app.search.is_none());
    assert!(app.download_center.version_filter.search().is_empty());
    let rendered = render_app(&app, 120, 40);
    assert!(rendered.contains("1.21.1"), "{rendered}");
}

#[test]
fn creeper_resource_search_bar_edits_the_active_query_inline() {
    let mut app = creeper_download_app(
        crate::tui::pages::download_center::DownloadCategory::Resource(ResourceKind::Mod),
    );
    app.download_center.search = "sodium".into();
    app.download_center.total = 120;

    let rendered = render_app(&app, 120, 40);
    assert!(rendered.contains("sodium"), "{rendered}");
    assert!(compact_frame(&rendered).contains("共120个"), "{rendered}");

    app.handle(key(KeyCode::Char('/')));
    assert!(matches!(
        app.mode(),
        crate::tui::command::Mode::Search(crate::tui::input::SearchTarget::Resources)
    ));
    for character in " extra".chars() {
        app.handle(key(KeyCode::Char(character)));
    }
    assert_eq!(app.download_center.search, "sodium extra");

    app.handle(key(KeyCode::Esc));
    assert!(app.search.is_none());
    assert_eq!(app.download_center.search, "sodium");
}

#[test]
fn creeper_resource_empty_state_keeps_the_results_card() {
    let app = creeper_download_app(
        crate::tui::pages::download_center::DownloadCategory::Resource(ResourceKind::Mod),
    );

    let rendered = render_app(&app, 120, 40);
    let text = compact_frame(&rendered);
    assert!(text.contains("结果·模组"), "{rendered}");
    assert!(text.contains("没有结果"), "{rendered}");
}

#[test]
fn creeper_resource_rows_defer_author_and_project_id_to_the_detail_card() {
    let mut app = creeper_download_app(
        crate::tui::pages::download_center::DownloadCategory::Resource(ResourceKind::Mod),
    );
    app.resource_projects = vec![ResourceProject {
        source: ResourceSource::Modrinth,
        project_id: "AANobbMI".into(),
        title: "Sodium".into(),
        author: "jellysquid3".into(),
        description: "A modern rendering engine".into(),
        icon_url: None,
        categories: Vec::new(),
        game_versions: vec!["1.21.1".into()],
        loaders: vec!["neoforge".into()],
        downloads: Some(1234),
        updated_at: None,
    }];

    // A regular 120x40 terminal is wide enough for the card beside the list.
    let main = app.layout_chrome(Rect::new(0, 0, 120, 40)).main;
    let list_area = app
        .current_download_page_sections(main)
        .expect("download sections")[4];
    assert!(
        app.download_detail_areas(list_area).is_some(),
        "detail card must fit next to a title-only list"
    );

    let rendered = render_app(&app, 120, 40);
    assert!(rendered.contains("Sodium"), "{rendered}");
    // Each value comes from the detail card only: the list row is title-only.
    assert_eq!(
        rendered.matches("jellysquid3").count(),
        1,
        "author must not be duplicated into the list: {rendered}"
    );
    assert_eq!(
        rendered.matches("AANobbMI").count(),
        1,
        "project id must not be duplicated into the list: {rendered}"
    );
}

#[test]
fn creeper_task_pills_hit_the_chip_that_is_drawn() {
    let mut app = App::new();
    app.page = Page::Tasks;
    app.config.state.ui.preset = crate::core::config::models::UiPreset::Creeper;
    app.i18n = crate::i18n::I18n::new(Language::SimplifiedChinese);
    app.motion = false;
    app.terminal_width = 120;
    app.terminal_height = 40;

    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal.draw(|frame| app.draw(frame)).expect("draw app");
    let buffer = terminal.backend().buffer();
    let area = App::task_page_sections(app.layout_chrome(Rect::new(0, 0, 120, 40)).main)[0];

    // Non-blank glyphs on the pill row, with the column they occupy.
    let glyphs: Vec<(u16, char)> = (area.x..area.right())
        .filter_map(|x| {
            let symbol = buffer[(x, area.y)].symbol();
            symbol
                .chars()
                .next()
                .filter(|ch| !ch.is_whitespace())
                .map(|ch| (x, ch))
        })
        .collect();

    let labels = app.creeper_task_filter_labels();
    for (index, filter) in crate::tui::pages::tasks::TaskFilter::ALL.iter().enumerate() {
        let label = &labels[index];
        // Spaces are drawn as blank cells and dropped by the glyph scan.
        let chars: Vec<char> = label.chars().filter(|ch| !ch.is_whitespace()).collect();
        let start = glyphs
            .windows(chars.len())
            .position(|window| window.iter().map(|(_, ch)| *ch).eq(chars.iter().copied()))
            .map(|position| glyphs[position].0)
            .unwrap_or_else(|| panic!("pill {label:?} was not drawn"));
        let end = glyphs
            .iter()
            .skip_while(|(column, _)| *column < start)
            .nth(chars.len().saturating_sub(1))
            .map(|(column, _)| *column)
            .expect("pill end column");

        // Both edges of the label must select this filter.
        for column in [start, end] {
            app.task_filter = crate::tui::pages::tasks::TaskFilter::ALL[0];
            app.handle(AppEvent::Mouse(MouseEvent {
                kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column,
                row: area.y,
                modifiers: KeyModifiers::NONE,
            }));
            assert_eq!(
                app.task_filter, *filter,
                "clicking column {column} of {label:?} selected the wrong filter"
            );
        }
    }
}

#[test]
fn creeper_form_field_rows_are_clickable_where_they_are_drawn() {
    let mut app = App::new();
    app.config.state.ui.preset = crate::core::config::models::UiPreset::Creeper;
    app.i18n = crate::i18n::I18n::new(Language::SimplifiedChinese);
    app.motion = false;
    app.terminal_width = 120;
    app.terminal_height = 40;
    app.open_create_form_with_version("1.21.1".into());
    // Layout is cached while drawing; the click below reads it.
    render_app(&app, 120, 40);

    let area = app
        .modal
        .as_ref()
        .expect("create form")
        .area(Rect::new(0, 0, 120, 40));
    app.handle(AppEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column: area.x + 5,
        // Creeper dialogs open with a spacer row, so field 2 is at +3.
        row: area.y + 3,
        modifiers: KeyModifiers::NONE,
    }));
    match &app.modal {
        Some(Modal::Form { focused, .. }) => assert_eq!(*focused, 1),
        other => panic!("expected the form, got {other:?}"),
    }
}

#[test]
fn creeper_detail_pane_wheel_stops_at_the_last_line() {
    let mut app = creeper_download_app(
        crate::tui::pages::download_center::DownloadCategory::Resource(ResourceKind::Mod),
    );
    app.terminal_width = 120;
    app.terminal_height = 40;
    app.resource_projects = vec![ResourceProject {
        source: ResourceSource::Modrinth,
        project_id: "long".into(),
        title: "Long card".into(),
        author: "author".into(),
        description: "A very long description that has to scroll. ".repeat(200),
        icon_url: None,
        categories: Vec::new(),
        game_versions: Vec::new(),
        loaders: Vec::new(),
        downloads: Some(1),
        updated_at: None,
    }];
    let main = app.layout_chrome(Rect::new(0, 0, 120, 40)).main;
    let list_area = app
        .current_download_page_sections(main)
        .expect("download sections")[4];
    let (_, pane) = app
        .download_detail_areas(list_area)
        .expect("detail pane next to a title index");

    // The pane reports its range while drawing.
    render_app(&app, 120, 40);
    let max = app.pane_max_scroll.get();
    assert!(max > 4, "expected a scrollable card, got {max}");

    let wheel = |app: &mut App, kind: MouseEventKind| {
        app.handle(AppEvent::Mouse(MouseEvent {
            kind,
            column: pane.x + 2,
            row: pane.y + 2,
            modifiers: KeyModifiers::NONE,
        }));
    };
    for _ in 0..400 {
        wheel(&mut app, MouseEventKind::ScrollDown);
    }
    // Overscrolling must not stack up: one notch back moves immediately.
    assert_eq!(app.pane_scroll, max);
    wheel(&mut app, MouseEventKind::ScrollUp);
    assert_eq!(app.pane_scroll, max.saturating_sub(2));
}

#[test]
fn creeper_detail_pane_uses_the_page_accent_and_a_live_scrollbar() {
    use crate::tui::palette;

    let mut app = creeper_download_app(
        crate::tui::pages::download_center::DownloadCategory::Resource(ResourceKind::Mod),
    );
    app.terminal_width = 120;
    app.terminal_height = 40;
    app.resource_projects = vec![ResourceProject {
        source: ResourceSource::Modrinth,
        project_id: "long".into(),
        title: "Long card".into(),
        author: "author".into(),
        description: "A very long description that has to scroll. ".repeat(200),
        icon_url: None,
        categories: Vec::new(),
        game_versions: Vec::new(),
        loaders: Vec::new(),
        downloads: Some(1),
        updated_at: None,
    }];

    let main = app.layout_chrome(Rect::new(0, 0, 120, 40)).main;
    let list_area = app
        .current_download_page_sections(main)
        .expect("download sections")[4];
    let (_, pane) = app
        .download_detail_areas(list_area)
        .expect("detail pane next to a title index");

    let render = |app: &App| {
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal.draw(|frame| app.draw(frame)).expect("draw app");
        terminal.backend().buffer().clone()
    };

    let buffer = render(&app);
    // The card frame uses the page accent, not the neutral border.
    assert_eq!(
        buffer[(pane.x, pane.y)].fg,
        palette::mix(
            crate::tui::palette::CYAN,
            crate::tui::palette::BORDER_HI,
            0.35
        ),
        "detail card border must match the page accent"
    );
    // Body text sits on the pane background: no dark inset rectangles.
    let painted = ((pane.y + 1)..pane.bottom().saturating_sub(1))
        .flat_map(|row| ((pane.x + 1)..pane.right().saturating_sub(2)).map(move |x| (x, row)))
        .find_map(|(x, row)| {
            let cell = &buffer[(x, row)];
            (!cell.symbol().trim().is_empty()).then_some(cell.bg)
        });
    assert_eq!(
        painted,
        Some(palette::SURFACE_ALT),
        "text must be painted on the card background"
    );

    let thumb_rows = |buffer: &ratatui::buffer::Buffer| {
        ((pane.y + 1)..pane.bottom().saturating_sub(1))
            .filter(|row| buffer[(pane.right() - 2, *row)].symbol() == "█")
            .collect::<Vec<u16>>()
    };
    let top = thumb_rows(&buffer);
    assert!(!top.is_empty(), "scrollbar thumb missing");

    app.pane_scroll = usize::MAX; // clamped to the last line
    let scrolled = thumb_rows(&render(&app));
    assert!(
        !scrolled.is_empty(),
        "scrollbar thumb missing after scrolling"
    );
    assert!(
        scrolled[0] > top[0],
        "thumb must move down when the pane scrolls (top {:?} vs scrolled {:?})",
        top,
        scrolled
    );
    assert!(
        *scrolled.last().expect("thumb rows") >= pane.bottom().saturating_sub(3),
        "thumb must reach the bottom of the track: {:?}",
        scrolled
    );
}

#[test]
fn creeper_detail_card_merges_description_tags_downloads_and_loaders() {
    let mut app = creeper_download_app(
        crate::tui::pages::download_center::DownloadCategory::Resource(ResourceKind::Modpack),
    );
    app.terminal_width = 132;
    app.terminal_height = 40;
    let description = format!(
        "A modpack that changes progression, adds dungeons and keeps going so the \
         detail card has to wrap it instead of cutting it short. {}",
        "Nested structures, quests and a lot more text that ends with TAILMARK."
    );
    app.resource_projects = vec![ResourceProject {
        source: ResourceSource::Modrinth,
        project_id: "pack-id".into(),
        title: "Fabulously Optimized".into(),
        author: "robotkoer".into(),
        description,
        icon_url: None,
        categories: vec!["forge".into(), "combat".into(), "multiplayer".into()],
        game_versions: vec!["1.20.1".into()],
        loaders: Vec::new(),
        downloads: Some(19_550_012),
        updated_at: None,
    }];

    let rendered = render_app(&app, 132, 40);
    let text = compact_frame(&rendered);
    // The whole description is on screen: wrapping instead of an ellipsis.
    assert!(text.contains("TAILMARK"), "{rendered}");
    // Downloads, tags and the loader-appearing tag appear exactly once.
    assert_eq!(text.matches("19,550,012").count(), 1, "{rendered}");
    assert_eq!(text.matches("multiplayer").count(), 0, "{rendered}");
    assert_eq!(text.matches("forge").count(), 1, "{rendered}");
    // The forge tag was merged into 加载器 and removed from 标签.
    assert!(text.contains("加载器:forge"), "{rendered}");
    assert!(text.contains("标签:战斗,多人"), "{rendered}");
}

#[test]
fn detail_focus_scrolls_long_download_details_without_changing_main_selection() {
    let mut app = App::new();
    app.page = Page::DownloadCenter;
    app.download_center.category =
        crate::tui::pages::download_center::DownloadCategory::Resource(ResourceKind::Mod);
    app.terminal_width = 120;
    app.terminal_height = 40;
    app.resource_projects = vec![ResourceProject {
        source: ResourceSource::Modrinth,
        project_id: "long-detail".into(),
        title: "Long detail".into(),
        author: "author".into(),
        description: "detail text ".repeat(300),
        icon_url: None,
        categories: Vec::new(),
        game_versions: Vec::new(),
        loaders: Vec::new(),
        downloads: None,
        updated_at: None,
    }];
    app.detail_open = true;

    app.handle(key(KeyCode::Down));
    assert!(app.detail_scroll > 0);
    assert_eq!(app.download_center.selected, 0);

    app.handle(key(KeyCode::Home));
    assert_eq!(app.detail_scroll, 0);
    app.handle(key(KeyCode::PageDown));
    assert!(app.detail_scroll > 0);
}

#[test]
fn download_resource_mouse_rows_follow_the_rendered_list_and_table() {
    let projects = (0..32)
        .map(|index| ResourceProject {
            source: ResourceSource::Modrinth,
            project_id: format!("project-{index:03}"),
            title: format!("project-{index:03}"),
            author: "author".into(),
            description: String::new(),
            icon_url: None,
            categories: Vec::new(),
            game_versions: Vec::new(),
            loaders: Vec::new(),
            downloads: None,
            updated_at: None,
        })
        .collect::<Vec<_>>();

    for (width, height) in [(116, 27), (120, 40)] {
        for category in crate::tui::pages::download_center::DownloadCategory::ALL {
            if matches!(
                category,
                crate::tui::pages::download_center::DownloadCategory::GameVersions
            ) {
                continue;
            }

            let mut app = App::new();
            app.page = Page::DownloadCenter;
            app.i18n = crate::i18n::I18n::new(Language::SimplifiedChinese);
            app.terminal_width = width;
            app.terminal_height = height;
            app.download_center.category = category;
            app.resource_projects = projects.clone();
            let list_area = app
                .download_page_sections(app.layout_chrome(Rect::new(0, 0, width, height)).main)
                .expect("download sections")[4];

            for initial_selection in [0, 10, projects.len() - 1] {
                app.download_center.selected = initial_selection;
                // One render per state: every click below restores the same
                // selection first, so the row map cannot drift mid-loop.
                let backend = TestBackend::new(width, height);
                let mut terminal = Terminal::new(backend).expect("test terminal");
                terminal
                    .draw(|frame| app.draw(frame))
                    .expect("draw download page");
                let buffer = terminal.backend().buffer();
                // Only scan the result list: the detail card repeats the
                // selected project's title and id on other rows.
                let table_right = app
                    .download_detail_areas(list_area)
                    .map_or(list_area.right(), |(table, _)| table.right());
                let visible = (list_area.y..list_area.bottom())
                    .filter_map(|row| {
                        let text = (list_area.x..table_right)
                            .filter_map(|column| buffer.cell((column, row)))
                            .map(|cell| cell.symbol())
                            .collect::<String>();
                        projects
                            .iter()
                            .position(|project| text.contains(&project.project_id))
                            .map(|index| (index, row))
                    })
                    .collect::<Vec<_>>();

                assert!(!visible.is_empty(), "no rows rendered for {category:?}");
                for (expected, row) in visible {
                    app.download_center.selected = initial_selection;
                    app.handle(AppEvent::Mouse(MouseEvent {
                        kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
                        column: list_area.x + 1,
                        row,
                        modifiers: KeyModifiers::NONE,
                    }));
                    assert_eq!(
                        app.download_center.selected, expected,
                        "{width}x{height} {category:?} click at row {row}"
                    );
                }
            }
        }
    }
}

#[test]
fn clicking_the_second_download_row_keeps_the_first_row_visible() {
    let projects = (0..20)
        .map(|index| ResourceProject {
            source: ResourceSource::Modrinth,
            project_id: format!("visible-{index:03}"),
            title: format!("visible-{index:03}"),
            author: "author".into(),
            description: String::new(),
            icon_url: None,
            categories: Vec::new(),
            game_versions: Vec::new(),
            loaders: Vec::new(),
            downloads: None,
            updated_at: None,
        })
        .collect::<Vec<_>>();
    for (width, height) in [(116, 27), (120, 40)] {
        let mut app = App::new();
        app.page = Page::DownloadCenter;
        app.terminal_width = width;
        app.terminal_height = height;
        app.download_center.category =
            crate::tui::pages::download_center::DownloadCategory::Resource(ResourceKind::Mod);
        app.resource_projects = projects.clone();

        let list_area = app
            .download_page_sections(app.layout_chrome(Rect::new(0, 0, width, height)).main)
            .expect("download sections")[4];
        let layout = app.list_layout(list_area, projects.len(), 0);
        let first_row = layout.first_row();
        let second_row = first_row + layout.card_height() + layout.gap();
        let _ = render_app(&app, width, height);
        click_at(&mut app, list_area.x + 1, second_row);
        assert_eq!(app.download_center.selected, 1);

        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| app.draw(frame))
            .expect("draw download page");
        let first_row_text = (list_area.x..list_area.x + list_area.width)
            .filter_map(|column| terminal.backend().buffer().cell((column, first_row)))
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(
            first_row_text.contains("visible-000"),
            "{width}x{height} click on the second row unexpectedly scrolled the first row away: {first_row_text:?}"
        );
    }
}

#[test]
fn log_page_keeps_latest_wrapped_line_visible_and_renders_scrollbar() {
    let mut app = App::new();
    app.page = Page::Logs;
    for index in 0..40 {
        app.logs.append(format!(
            "long log line {index} with details that wrap several times in a narrow terminal"
        ));
    }
    app.logs.append("LATEST");

    let rendered = render_app(&app, 80, 24);

    assert!(rendered.contains("LATEST"));
    assert!(rendered.contains('║'));
}

#[cfg(windows)]
#[test]
fn java_and_settings_pages_hide_windows_verbatim_path_prefixes() {
    let verbatim_java = PathBuf::from(r"\\?\C:\Java\jdk-21\bin\java.exe");
    let mut app = App::new();
    app.page = Page::Settings;
    app.settings.select_category(SettingsCategory::Java);
    app.java.push(JavaInstallation {
        executable: verbatim_java,
        version: "21.0.1".into(),
        major: 21,
        vendor: None,
        is_jdk: true,
        lts: true,
    });

    let java_page = render_app(&app, 200, 30);
    assert!(java_page.contains(r"C:\Java\jdk-21\bin\java.exe"));
    assert!(!java_page.contains(r"\\?\"));

    app.page = Page::Settings;
    app.config.game_directories = vec![PathBuf::from(r"\\?\C:\Games\.minecraft")];
    app.settings
        .select_category(SettingsCategory::GameDirectories);
    let settings_page = render_app(&app, 120, 24);
    assert!(settings_page.contains(r"C:\Games\.minecraft"));
    assert!(!settings_page.contains(r"\\?\"));
}

fn render_app(app: &App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("create test terminal");
    terminal.draw(|frame| app.draw(frame)).expect("draw app");
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn status_kind_is_explicit_not_derived_from_text() {
    let mut app = App::new();
    app.status = Status::error("all good");
    assert_eq!(
        app.status.kind(),
        crate::tui::widgets::status::StatusKind::Failed
    );
    app.status = Status::success("failed to do nothing");
    assert_eq!(
        app.status.kind(),
        crate::tui::widgets::status::StatusKind::Completed
    );
    let rendered = render_app(&app, 120, 40);
    assert!(rendered.contains("failed to do nothing"));
}

#[test]
fn detail_hints_change_with_the_page() {
    let mut app = App::new();
    app.page = Page::Instances;
    app.open_details();
    let keys = app
        .hints()
        .into_iter()
        .map(|hint| hint.key)
        .collect::<Vec<_>>();
    assert!(keys.contains(&"Esc".to_owned()));
    assert!(!keys.contains(&"Enter".to_owned()));

    app.detail_open = false;
    app.page = Page::DownloadCenter;
    app.download_center.category =
        crate::tui::pages::download_center::DownloadCategory::Resource(ResourceKind::Mod);
    app.open_details();
    let keys = app
        .hints()
        .into_iter()
        .map(|hint| hint.key)
        .collect::<Vec<_>>();
    assert!(keys.contains(&"Enter".to_owned()), "{keys:?}");
}

#[test]
fn task_filters_cover_queued_and_done() {
    use crate::tui::pages::tasks::TaskFilter;
    assert_eq!(TaskFilter::ALL.len(), 5);
    assert_eq!(TaskFilter::Queued.label(), "Queued");
    assert_eq!(TaskFilter::Done.label(), "Done");
}

#[test]
fn settings_arrows_adjust_values_and_brackets_switch_category() {
    let mut app = App::new();
    app.page = Page::Settings;

    // `←/→` edits the selected value instead of moving the rail.
    let before = app.config.launcher.visibility;
    app.handle(key(KeyCode::Right));
    assert_ne!(app.config.launcher.visibility, before);
    app.handle(key(KeyCode::Left));
    assert_eq!(app.config.launcher.visibility, before);

    // Categories move with `[`/`]`.
    let before = app.settings.category;
    app.handle(key(KeyCode::Char(']')));
    assert_ne!(app.settings.category, before);
    app.handle(key(KeyCode::Char('[')));
    assert_eq!(app.settings.category, before);
}

#[test]
fn clicking_outside_the_detail_popup_closes_it() {
    let mut app = App::new();
    app.page = Page::Instances;
    app.open_details();
    app.terminal_width = 120;
    app.terminal_height = 40;
    let _ = render_app(&app, 120, 40);
    let rect = crate::tui::chrome::popup_rect(Rect::new(0, 0, 120, 40));

    // Inside the card the click is swallowed, so the user can select text in
    // the pane without dismissing it.
    app.handle(AppEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column: rect.x + 2,
        row: rect.y + 2,
        modifiers: KeyModifiers::NONE,
    }));
    assert!(app.detail_open);

    app.handle(AppEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column: 1,
        row: 39,
        modifiers: KeyModifiers::NONE,
    }));
    assert!(!app.detail_open);
}

#[test]
fn modpack_search_ignores_the_selected_instance_context() {
    let target = Some(("1.20.1".to_owned(), LoaderFilter::Fabric));

    assert_eq!(
        resource_search_context(ResourceKind::Modpack, target.clone()),
        (None, None)
    );
    assert_eq!(
        resource_search_context(ResourceKind::Mod, target),
        (Some("1.20.1".to_owned()), Some(LoaderFilter::Fabric))
    );
}

#[test]
fn selected_resource_projects_populate_the_detail_panel_for_every_kind() {
    for kind in [
        ResourceKind::Mod,
        ResourceKind::Modpack,
        ResourceKind::ResourcePack,
        ResourceKind::Shader,
        ResourceKind::World,
    ] {
        let mut app = App::new();
        app.page = Page::DownloadCenter;
        app.download_center.category =
            crate::tui::pages::download_center::DownloadCategory::Resource(kind);
        app.resource_projects = vec![ResourceProject {
            source: ResourceSource::Modrinth,
            project_id: format!("project-{kind:?}"),
            title: format!("selected-{kind:?}"),
            author: "author".into(),
            description: "project description".into(),
            icon_url: None,
            categories: Vec::new(),
            game_versions: vec!["1.21.1".into()],
            loaders: vec!["fabric".into()],
            downloads: Some(42),
            updated_at: Some("2026-09-27".into()),
        }];
        app.versions = vec![VersionSummary {
            id: "stale-game-version".into(),
            kind: "snapshot".into(),
            url: String::new(),
            time: String::new(),
            release_time: String::new(),
            sha1: None,
            compliance_level: None,
        }];

        let lines = app.detail_lines();
        assert_eq!(
            lines.first().map(ToString::to_string),
            Some(format!("selected-{kind:?}"))
        );
        assert!(
            !lines
                .iter()
                .any(|line| line.to_string().contains("stale-game-version"))
        );
    }
}

#[test]
fn layout_chrome_keep_shell_and_modal_within_terminal_bounds() {
    for (width, height) in [(120, 40), (100, 30), (95, 29), (80, 24)] {
        let mut app = App::new();
        app.terminal_width = width;
        app.terminal_height = height;
        let terminal = Rect::new(0, 0, width, height);
        let regions = app.layout_chrome(terminal);

        assert_eq!(regions.header.height, 3);
        assert_eq!(regions.body.y, regions.header.bottom());
        assert_eq!(regions.body.bottom(), height - 1);
        assert_eq!(regions.footer.bottom() + 1, regions.body.bottom());
        assert!(regions.main.x + regions.main.width <= width);
        assert!(regions.main.y >= regions.content.y);
    }

    let mut app = App::new();
    app.terminal_width = 120;
    app.terminal_height = 40;
    app.open_offline_account_form();
    let terminal = Rect::new(0, 0, 120, 40);
    let modal = app.layout_chrome(terminal).modal.expect("modal region");
    assert!(modal.x >= terminal.x);
    assert!(modal.y >= terminal.y);
    assert!(modal.x + modal.width <= terminal.x + terminal.width);
    assert!(modal.y + modal.height <= terminal.y + terminal.height);
}

#[test]
fn offline_account_form_only_asks_for_username() {
    let mut app = App::new();
    app.open_offline_account_form();

    let Some(Modal::Form { labels, values, .. }) = app.modal.as_ref() else {
        panic!("offline-account form should be open");
    };
    assert_eq!(labels, &["Username"]);
    assert_eq!(values.len(), 1);
}

#[test]
fn cached_minecraft_versions_initialize_picker_filters() {
    let mut app = App::new();
    app.versions = vec![
        VersionSummary {
            id: "1.21.1".into(),
            kind: "release".into(),
            url: String::new(),
            time: "2024-09-01T00:00:00Z".into(),
            release_time: "2024-09-01T00:00:00Z".into(),
            sha1: None,
            compliance_level: None,
        },
        VersionSummary {
            id: "25w14a".into(),
            kind: "snapshot".into(),
            url: String::new(),
            time: "2025-03-26T00:00:00Z".into(),
            release_time: "2025-03-26T00:00:00Z".into(),
            sha1: None,
            compliance_level: None,
        },
    ];
    app.open_create_form_with_version("1.21.1".into());
    app.modal
        .as_mut()
        .expect("create form")
        .open_selection_picker(CREATE_VERSION_FIELD);

    let rendered = render_app(&app, 100, 30);
    assert!(rendered.contains("Release (1)"));
    assert!(rendered.contains("Snapshot (1)"));
    assert!(rendered.contains("1.21.1"));
    assert!(!rendered.contains("25w14a"));
}

#[test]
fn loaded_minecraft_versions_fill_the_open_create_picker() {
    let mut app = App::new();
    app.open_create_form_with_version(String::new());
    let modal = app.modal.as_mut().expect("create form");
    modal.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    modal.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(
        modal.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        crate::tui::modal::ModalResult::OpenSelectionPicker { field: 2 }
    ));
    assert!(modal.set_selection_loading(2));

    app.handle(AppEvent::VersionsLoaded {
        versions: vec![
            VersionSummary {
                id: "1.21.1".into(),
                kind: "release".into(),
                url: "https://example.test/release.json".into(),
                time: "2024-09-01T00:00:00Z".into(),
                release_time: "2024-09-01T00:00:00Z".into(),
                sha1: None,
                compliance_level: None,
            },
            VersionSummary {
                id: "25w14a".into(),
                kind: "snapshot".into(),
                url: "https://example.test/snapshot.json".into(),
                time: "2025-03-26T00:00:00Z".into(),
                release_time: "2025-03-26T00:00:00Z".into(),
                sha1: None,
                compliance_level: None,
            },
        ],
        from_cache: false,
    });

    let Some(Modal::Form {
        selection_picker_open,
        selection_options,
        ..
    }) = app.modal.as_ref()
    else {
        panic!("create form should remain open");
    };
    assert_eq!(*selection_picker_open, Some(2));
    assert_eq!(selection_options[&2], ["1.21.1", "25w14a"]);
    let rendered = render_app(&app, 100, 30);
    assert!(rendered.contains("Snapshot (1)"));
    assert!(!rendered.contains("25w14a"));

    app.modal
        .as_mut()
        .expect("create form")
        .handle_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE));
    assert!(render_app(&app, 100, 30).contains("25w14a"));
}

#[test]
fn loader_metadata_events_ignore_stale_form_selections() {
    let mut app = App::new();
    app.open_create_form_with_version("1.21.1".into());
    let modal = app.modal.as_mut().expect("create form");
    modal.set_field_value(3, "fabric");
    modal.open_selection_picker(4);
    modal.set_selection_loading(4);
    let versions = parse_loader_metadata(
        MetadataLoaderKind::Fabric,
        "1.21.1",
        include_bytes!("../../../fixtures/loader_metadata/fabric.json"),
    )
    .expect("parse Fabric fixture");

    app.handle(AppEvent::LoaderVersionsLoaded {
        minecraft_version: "1.20.1".into(),
        kind: MetadataLoaderKind::Fabric,
        result: Ok(versions.clone()),
    });
    let Some(Modal::Form {
        selection_options, ..
    }) = app.modal.as_ref()
    else {
        panic!("create form should remain open");
    };
    assert!(selection_options[&4].is_empty());

    app.handle(AppEvent::LoaderVersionsLoaded {
        minecraft_version: "1.21.1".into(),
        kind: MetadataLoaderKind::Forge,
        result: Ok(versions.clone()),
    });
    let Some(Modal::Form {
        selection_options, ..
    }) = app.modal.as_ref()
    else {
        panic!("create form should remain open");
    };
    assert!(selection_options[&4].is_empty());

    app.handle(AppEvent::LoaderVersionsLoaded {
        minecraft_version: "1.21.1".into(),
        kind: MetadataLoaderKind::Fabric,
        result: Ok(versions),
    });
    let Some(Modal::Form {
        selection_picker_open,
        selection_options,
        ..
    }) = app.modal.as_ref()
    else {
        panic!("create form should remain open");
    };
    assert_eq!(*selection_picker_open, Some(4));
    assert_eq!(selection_options[&4], ["0.16.5", "0.16.4"]);
}

#[test]
fn loader_metadata_failure_stays_retryable_in_the_open_picker() {
    let mut app = App::new();
    app.open_create_form_with_version("1.21.1".into());
    let modal = app.modal.as_mut().expect("create form");
    modal.set_field_value(3, "neoforge");
    modal.open_selection_picker(CREATE_LOADER_VERSION_FIELD);
    modal.set_selection_loading(CREATE_LOADER_VERSION_FIELD);

    app.handle(AppEvent::LoaderVersionsLoaded {
        minecraft_version: "1.21.1".into(),
        kind: MetadataLoaderKind::NeoForge,
        result: Err("all metadata sources failed".into()),
    });

    let rendered = render_app(&app, 100, 30);
    assert!(rendered.contains("all metadata sources failed"));
    assert!(rendered.contains("r: retry"));
    let modal = app.modal.as_mut().expect("create form");
    assert!(matches!(
        modal.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE)),
        crate::tui::modal::ModalResult::RetrySelectionPicker {
            field: CREATE_LOADER_VERSION_FIELD
        }
    ));
}

#[test]
fn choosing_a_loader_prepares_a_loading_version_picker() {
    let mut app = App::new();
    app.open_create_form_with_version("1.21.1".into());
    let modal = app.modal.as_mut().expect("create form");
    for _ in 0..3 {
        modal.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    }
    modal.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    modal.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert!(matches!(
        modal.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        crate::tui::modal::ModalResult::SelectionChanged { field: 3, value }
            if value == "fabric"
    ));

    let request = app.create_selection_changed(CREATE_LOADER_FIELD);

    assert_eq!(request, Some(("1.21.1".into(), MetadataLoaderKind::Fabric)));
    let Some(Modal::Form {
        values,
        selection_options,
        selection_picker_open,
        ..
    }) = app.modal.as_ref()
    else {
        panic!("create form should remain open");
    };
    assert_eq!(values[4], "");
    assert!(selection_options[&4].is_empty());
    assert_eq!(*selection_picker_open, Some(4));
    assert!(render_app(&app, 100, 30).contains("Loading options"));
}

#[test]
fn changing_minecraft_version_refetches_loader_versions_and_clears_old_choice() {
    let mut app = App::new();
    app.open_create_form_with_version("1.20.1".into());
    let modal = app.modal.as_mut().expect("create form");
    modal.set_field_value(3, "forge");
    modal.set_field_value(4, "old-loader");
    modal.set_selection_options(4, vec!["old-loader".into()]);
    modal.set_field_value(2, "1.21.1");

    let request = app.create_selection_changed(CREATE_VERSION_FIELD);

    assert_eq!(request, Some(("1.21.1".into(), MetadataLoaderKind::Forge)));
    let Some(Modal::Form {
        values,
        selection_options,
        selection_picker_open,
        ..
    }) = app.modal.as_ref()
    else {
        panic!("create form should remain open");
    };
    assert!(values[4].is_empty());
    assert!(selection_options[&4].is_empty());
    assert_eq!(*selection_picker_open, Some(4));
}

#[test]
fn vanilla_selection_clears_loader_version_without_requesting_metadata() {
    let mut app = App::new();
    app.open_create_form_with_version("1.21.1".into());
    let modal = app.modal.as_mut().expect("create form");
    modal.set_field_value(4, "stale-version");
    modal.set_selection_options(4, vec!["stale-version".into()]);

    let request = app.create_selection_changed(CREATE_LOADER_FIELD);

    assert_eq!(request, None);
    let Some(Modal::Form {
        values,
        selection_options,
        selection_states,
        ..
    }) = app.modal.as_ref()
    else {
        panic!("create form should remain open");
    };
    assert!(values[4].is_empty());
    assert!(selection_options[&4].is_empty());
    assert!(matches!(
        selection_states.get(&4),
        Some(crate::tui::modal::SelectionState::Unavailable(message))
            if message.contains("Vanilla")
    ));
}

#[test]
fn minecraft_version_failure_is_shown_in_the_open_picker() {
    let mut app = App::new();
    app.open_create_form_with_version(String::new());
    let modal = app.modal.as_mut().expect("create form");
    modal.open_selection_picker(2);
    modal.set_selection_loading(2);

    app.handle(AppEvent::VersionsLoadFailed {
        for_picker: true,
        error: "network unavailable".into(),
    });

    let rendered = render_app(&app, 100, 30);
    assert!(rendered.contains("network unavailable"));
    assert!(rendered.contains("r: retry"));
}

#[test]
fn form_stays_open_when_submit_validation_fails() {
    let mut app = App::new();
    app.modal = Some(Modal::form(
        "Add game directory",
        &["Directory path"],
        &[String::new()],
        FormAction::AddGameDirectory,
    ));

    app.handle(key(KeyCode::Enter));

    assert!(app.modal.is_some());
    assert!(app.status.contains("directory"));
}

#[test]
fn settings_mouse_uses_the_clicked_column_and_shared_row_geometry() {
    let root = std::env::temp_dir().join(format!("tmcl-settings-nav-{}", uuid::Uuid::new_v4()));
    let paths = DataPaths::from_appdata(root.join("AppData"));
    paths.ensure_dirs().expect("create data paths");
    let game_directory = root.join("Games");
    std::fs::create_dir_all(&game_directory).expect("create game directory");
    let (sender, _) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::with_state(
        paths,
        Config {
            game_directories: vec![game_directory.clone()],
            ..Config::default()
        },
        sender,
    );
    app.page = Page::Settings;

    let main = app.layout_chrome(Rect::new(0, 0, 120, 40)).main;
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(main);
    // Clicks resolve against the frame's hit map, so draw one first.
    let _ = render_app(&app, 120, 40);
    let categories = SettingsCategory::ALL
        .iter()
        .map(|category| app.i18n.text(category.title_key()))
        .collect::<Vec<_>>();
    let tab = crate::tui::widgets::tabs::TabBar::new(categories).rects(sections[0])[0];
    click_at(&mut app, tab.x + 1, tab.y);
    assert_eq!(app.settings.category, SettingsCategory::General);
    assert_eq!(app.settings.category.index(), 0);

    let count = app.settings_rows().len();
    let layout = crate::tui::widgets::settings::layout(sections[1], count, 0);
    let rows = layout.rows().collect::<Vec<_>>();
    click_at(&mut app, rows[0].1.x + 2, rows[0].1.y);
    assert_eq!(app.settings.selected, 0);
    click_at(&mut app, rows[1].1.x + 2, rows[1].1.y);
    assert_eq!(app.settings.selected, 1);
    assert_eq!(app.selected(), 1);

    std::fs::remove_dir_all(root).expect("clean settings test files");
}

#[test]
fn loading_duplicate_registered_roots_displays_each_instance_once() {
    let root =
        std::env::temp_dir().join(format!("tmcl-duplicate-instances-{}", uuid::Uuid::new_v4()));
    let instance = root.join("versions/survival");
    std::fs::create_dir_all(&instance).expect("create instance");
    std::fs::write(instance.join("survival.jar"), b"jar").expect("write client jar");
    std::fs::write(instance.join("survival.json"), r#"{"id":"1.21.1"}"#)
        .expect("write client json");
    let config = Config {
        game_directories: vec![root.clone(), root.clone()],
        ..Config::default()
    };

    let instances = load_instances(&config, &mut Default::default()).expect("load instances");

    assert_eq!(instances.len(), 1);
    std::fs::remove_dir_all(root).expect("clean instance test files");
}

#[test]
fn loading_duplicate_instance_ids_from_distinct_roots_displays_one_entry() {
    let root = std::env::temp_dir().join(format!(
        "tmcl-duplicate-instance-ids-{}",
        uuid::Uuid::new_v4()
    ));
    let first_root = root.join("first");
    let second_root = root.join("second");
    let shared_id = uuid::Uuid::new_v4();
    for game_root in [&first_root, &second_root] {
        let instance_path = game_root.join("versions/survival");
        std::fs::create_dir_all(&instance_path).expect("create instance");
        std::fs::write(instance_path.join("survival.jar"), b"jar").expect("write client jar");
        std::fs::write(instance_path.join("survival.json"), r#"{"id":"1.21.1"}"#)
            .expect("write client json");
        let mut metadata = crate::core::instance::models::InstanceConfig::new(
            &instance_path,
            game_directory_id(game_root),
            "survival",
            "1.21.1",
        );
        metadata.id = shared_id;
        std::fs::write(
            instance_path.join("tmcl.json"),
            serde_json::to_vec(&metadata).expect("serialize instance metadata"),
        )
        .expect("write instance metadata");
    }
    let config = Config {
        game_directories: vec![first_root, second_root],
        ..Config::default()
    };

    let instances = load_instances(&config, &mut Default::default()).expect("load instances");

    assert_eq!(instances.len(), 1);
    std::fs::remove_dir_all(root).expect("clean duplicate instance test files");
}

#[test]
fn a_refresh_only_reads_the_instances_that_changed() {
    let root = std::env::temp_dir().join(format!("tmcl-scan-cache-{}", uuid::Uuid::new_v4()));
    let config = Config {
        game_directories: vec![root.clone()],
        ..Config::default()
    };
    for name in ["alpha", "beta"] {
        let instance = root.join("versions").join(name);
        std::fs::create_dir_all(&instance).expect("create instance");
        std::fs::write(instance.join(format!("{name}.jar")), b"jar").expect("jar");
        std::fs::write(instance.join(format!("{name}.json")), r#"{"id":"1.21.1"}"#).expect("json");
    }

    let mut cache = crate::tui::app::state::data::InstanceScanCache::default();
    let first = load_instances(&config, &mut cache).expect("first scan");
    assert_eq!(first.len(), 2);
    assert_eq!(cache.scans(), 2);

    // Nothing changed: both rows come from the cache, no metadata is re-read.
    let second = load_instances(&config, &mut cache).expect("second scan");
    assert_eq!(second.len(), 2);
    assert_eq!(cache.scans(), 2);

    // A loader change in one manifest invalidates exactly that row.
    let beta = root.join("versions/beta");
    let mut metadata: crate::core::instance::models::InstanceConfig =
        crate::core::config::storage::read_json(&beta.join("tmcl.json")).expect("metadata");
    metadata.loader = crate::core::instance::models::InstanceLoader::Fabric {
        version: "0.16.0".into(),
    };
    crate::core::config::storage::write_json_atomic(&beta.join("tmcl.json"), &metadata)
        .expect("write metadata");
    let third = load_instances(&config, &mut cache).expect("third scan");
    assert_eq!(cache.scans(), 3, "only the changed manifest is read again");
    assert_eq!(
        third
            .iter()
            .find(|instance| instance.name == "beta")
            .map(|instance| instance.loader.as_str()),
        Some("Fabric")
    );

    // Removing an instance drops its cache entry as well.
    std::fs::remove_dir_all(&beta).expect("remove instance");
    let fourth = load_instances(&config, &mut cache).expect("fourth scan");
    assert_eq!(fourth.len(), 1);
    std::fs::remove_dir_all(root).expect("clean scan cache files");
}

#[test]
fn operational_settings_validate_and_persist_sources_and_log_retention() {
    let root = std::env::temp_dir().join(format!(
        "tmcl-operational-settings-{}",
        uuid::Uuid::new_v4()
    ));
    let paths = DataPaths::from_appdata(root.join("AppData"));
    paths.ensure_dirs().expect("create data paths");
    let (sender, _) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::with_state(paths.clone(), Config::default(), sender);

    app.settings.select_category(SettingsCategory::Downloads);
    app.cycle_setting(SettingId::VersionSource, 1);
    app.cycle_setting(SettingId::VersionSource, 1);
    app.cycle_setting(SettingId::FileSource, 1);
    app.toggle_setting(SettingId::AutoDownloadThreads);
    app.submit_form(
        FormAction::SettingValue(SettingId::DownloadThreads),
        vec!["6".into()],
    )
    .expect("save download threads");
    app.submit_form(
        FormAction::SettingValue(SettingId::LogRetention),
        vec!["45".into()],
    )
    .expect("save log retention");

    assert_eq!(
        app.config.downloads.version_list_source,
        DownloadSourceMode::Mirror
    );
    assert_eq!(
        app.config.downloads.file_download_source,
        DownloadSourceMode::Official
    );
    assert!(!app.config.downloads.auto_download_threads);
    assert_eq!(app.config.downloads.download_threads, 6);
    assert_eq!(app.config.logs.retention_days, 45);
    let saved: Config =
        crate::core::config::storage::read_json(&paths.config_file).expect("read saved config");
    assert_eq!(saved, app.config);

    let unchanged = app.config.clone();
    assert!(
        app.submit_form(
            FormAction::SettingValue(SettingId::DownloadThreads),
            vec!["999".into()],
        )
        .is_err()
    );
    assert_eq!(app.config, unchanged);
    std::fs::remove_dir_all(root).expect("clean test files");
}

#[tokio::test]
async fn microsoft_instance_launch_attempts_refresh_instead_of_rejecting_account() {
    let root = std::env::temp_dir().join(format!(
        "tmcl-microsoft-launch-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos()
    ));
    let paths = DataPaths::from_appdata(root.join("AppData"));
    paths.ensure_dirs().expect("create data paths");
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::with_state(paths.clone(), Config::default(), sender);
    let account_id = uuid::Uuid::new_v4();
    app.accounts.accounts.push(Account {
        id: account_id,
        kind: AccountKind::Microsoft,
        username: "Player".into(),
        uuid: uuid::Uuid::new_v4(),
        skin: None,
        secret_ref: Some(account_id.to_string()),
        access_expires_at: None,
    });
    app.instances.items.push(InstanceSummary {
        id: uuid::Uuid::new_v4(),
        name: "world".into(),
        version: "1.21.1".into(),
        loader: "Vanilla".into(),
        loader_version: None,
        status: "ready".into(),
        path: root.join("game/versions/world"),
        launch_count: 0,
        play_time_seconds: 0,
    });

    app.launch_selected_instance();

    assert!(!app.status.contains("token refresh support"));
    // Launch stages arrive as `Status` events before the outcome; skip them and
    // wait for the failure the test is about.
    let event = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let event = receiver.recv().await.expect("status event");
            if matches!(event, AppEvent::Status(_)) {
                continue;
            }
            break event;
        }
    })
    .await
    .expect("launch task should report a result");
    assert!(matches!(
        event,
        AppEvent::LaunchAborted { error } if error.contains("Microsoft launch failed")
    ));
    std::fs::remove_dir_all(root).expect("clean test files");
}

#[test]
fn launch_uses_compatible_java_21_when_configured_java_17_is_too_old() {
    let root = std::env::temp_dir().join(format!(
        "tmcl-java-selection-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos()
    ));
    let paths = DataPaths::from_appdata(root.join("AppData"));
    paths.ensure_dirs().expect("create data paths");
    let instance_path = root.join("game/versions/neoforge");
    std::fs::create_dir_all(&instance_path).expect("create instance directory");
    std::fs::write(
        instance_path.join("neoforge.json"),
        br#"{"id":"neoforge","type":"release","mainClass":"net.minecraft.Main","javaVersion":{"majorVersion":21},"arguments":{"game":[],"jvm":[]}}"#,
    )
    .expect("write client JSON");
    // Launching checks the files the command points at, so the instance has to
    // look installed.
    std::fs::write(instance_path.join("neoforge.jar"), b"jar").expect("write client jar");
    let java17 = JavaInstallation {
        executable: root.join("Java/jdk-17/bin/java.exe"),
        version: "17.0.12".into(),
        major: 17,
        vendor: None,
        is_jdk: true,
        lts: true,
    };
    let java21 = JavaInstallation {
        executable: root.join("Java/jdk-21/bin/java.exe"),
        version: "21.0.10".into(),
        major: 21,
        vendor: None,
        is_jdk: true,
        lts: true,
    };
    for java in [&java17, &java21] {
        std::fs::create_dir_all(java.executable.parent().expect("java dir"))
            .expect("create java directory");
        std::fs::write(&java.executable, b"java").expect("write java");
    }
    let mut config = Config::default();
    config.java.selected_path = Some(java17.executable.clone());
    let account =
        crate::core::account::offline::create_account("Alex", None).expect("offline account");
    let instance = crate::tui::pages::instances::InstanceSummary {
        id: uuid::Uuid::new_v4(),
        name: "neoforge".into(),
        version: "1.21.1-NeoForge_21.1.247".into(),
        loader: "NeoForge".into(),
        loader_version: Some("21.1.247".into()),
        status: "ready".into(),
        path: instance_path,
        launch_count: 0,
        play_time_seconds: 0,
    };

    let spec = build_for(
        &paths,
        &config,
        &[java17, java21.clone()],
        &instance,
        &crate::core::instance::overrides::LaunchOverrides::default(),
        &account,
        "0",
    )
    .expect("compatible launch command");

    assert_eq!(spec.program, java21.executable);
    std::fs::remove_dir_all(root).expect("clean test files");
}

#[tokio::test]
async fn tab_switches_pages_in_display_order() {
    let mut app = App::new();
    app.terminal_width = 120;
    app.terminal_height = 40;
    assert_eq!(app.page, Page::Overview);
    app.handle(key(KeyCode::Tab));
    assert_eq!(app.page, Page::Instances);
    app.handle(key(KeyCode::Tab));
    assert_eq!(app.page, Page::DownloadCenter);
    app.handle(key(KeyCode::BackTab));
    assert_eq!(app.page, Page::Instances);
}

#[test]
fn question_mark_opens_and_closes_help_overlay() {
    let mut app = App::new();
    app.terminal_width = 120;
    app.terminal_height = 40;
    app.handle(key(KeyCode::Char('?')));
    assert!(app.help_open);
    assert_eq!(app.mode(), crate::tui::command::Mode::Help);
    let rendered = render_app(&app, 120, 40);
    assert!(rendered.contains("Esc"));

    app.handle(key(KeyCode::Esc));
    assert!(!app.help_open);
    assert_eq!(app.page, Page::Overview);
    assert!(!app.quit);
}

#[test]
fn escape_quits_only_when_no_layer_is_open() {
    let mut app = App::new();
    app.terminal_width = 120;
    app.terminal_height = 40;
    app.handle(key(KeyCode::Esc));
    assert!(app.quit);
}

#[test]
fn importing_game_directory_discovers_instances_and_versions() {
    let temporary = std::env::temp_dir().join(format!(
        "tmcl-import-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos()
    ));
    let game_directory = temporary.join("Portable");
    let data_paths = DataPaths::from_appdata(temporary.join("AppData"));
    data_paths.ensure_dirs().expect("create data paths");
    let (sender, _) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::with_state(data_paths, Config::default(), sender);

    for (name, json) in [
        ("vanilla", r#"{"id":"1.21.1"}"#),
        (
            "forge",
            r#"{"id":"1.20.1-forge-47.2.0","inheritsFrom":"1.20.1"}"#,
        ),
    ] {
        let instance_directory = game_directory.join("versions").join(name);
        std::fs::create_dir_all(&instance_directory).expect("create instance directory");
        std::fs::write(instance_directory.join(format!("{name}.jar")), b"jar")
            .expect("write client jar");
        std::fs::write(instance_directory.join(format!("{name}.json")), json)
            .expect("write client json");
    }

    app.submit_form(
        FormAction::ImportInstance,
        vec![game_directory.to_string_lossy().into_owned()],
    )
    .expect("import game directory");

    assert_eq!(app.instances.items.len(), 2);
    assert!(
        app.instances
            .items
            .iter()
            .any(|instance| { instance.name == "vanilla" && instance.version == "1.21.1" })
    );
    assert!(
        app.instances
            .items
            .iter()
            .any(|instance| { instance.name == "forge" && instance.version == "1.20.1" })
    );
    assert_eq!(
        app.config.game_directories[0],
        normalize_game_directory(&game_directory).unwrap()
    );

    let legacy_parent = temporary.join("Legacy");
    let legacy_game_directory = legacy_parent.join(".minecraft");
    let legacy_instance = legacy_game_directory.join("versions").join("legacy");
    std::fs::create_dir_all(&legacy_instance).expect("create legacy instance directory");
    std::fs::write(legacy_instance.join("legacy.jar"), b"jar").expect("write legacy jar");
    std::fs::write(legacy_instance.join("legacy.json"), r#"{"id":"1.16.5"}"#)
        .expect("write legacy client json");
    app.submit_form(
        FormAction::ImportInstance,
        vec![legacy_parent.to_string_lossy().into_owned()],
    )
    .expect("import detected .minecraft child directory");
    assert!(
        app.instances
            .items
            .iter()
            .any(|instance| { instance.name == "legacy" && instance.version == "1.16.5" })
    );
    assert_eq!(
        app.config.game_directories[0],
        normalize_game_directory(&legacy_game_directory).unwrap()
    );

    std::fs::remove_dir_all(temporary).expect("remove test tree");
}

#[test]
fn help_opens_with_shifted_question_mark() {
    let mut app = App::new();
    app.handle(AppEvent::Key(KeyEvent {
        code: KeyCode::Char('?'),
        modifiers: KeyModifiers::SHIFT,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }));
    assert!(app.help_open);
}

#[test]
fn resource_search_normalizes_imported_versions() {
    let context = resource_search_context(
        ResourceKind::Mod,
        Some(("1.12.2cocricot&miniaturia".into(), LoaderFilter::Forge)),
    );
    assert_eq!(context, (Some("1.12.2".into()), Some(LoaderFilter::Forge)));

    let empty = resource_search_context(
        ResourceKind::Mod,
        Some((String::new(), LoaderFilter::Forge)),
    );
    assert_eq!(empty, (None, Some(LoaderFilter::Forge)));

    let vanilla = resource_search_context(
        ResourceKind::Mod,
        Some(("1.21.1".into(), LoaderFilter::Vanilla)),
    );
    assert_eq!(vanilla, (Some("1.21.1".into()), None));
}

#[test]
fn creeper_footer_keeps_hints_above_the_status_band() {
    use crate::tui::palette::SURFACE;

    let mut app = creeper_download_app(
        crate::tui::pages::download_center::DownloadCategory::Resource(ResourceKind::Mod),
    );
    app.status = Status::info("已加载 921 个 Minecraft 版本".to_string());

    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.draw(frame)).expect("draw");
    let buffer = terminal.backend().buffer();
    let row_text = |row: u16| {
        (0..120)
            .map(|column| buffer[(column, row)].symbol())
            .collect::<String>()
    };

    // Two bands: key caps on top, the log/status line on the bottom row.
    let hints = compact_frame(&row_text(38));
    let status = compact_frame(&row_text(39));
    assert!(hints.contains("Enter打开/安装"), "{hints}");
    assert!(status.contains("已加载921个Minecraft版本"), "{status}");
    assert!(status.contains("↻0s"), "{status}");
    assert!(
        !status.contains("Enter"),
        "hints leaked into the status band"
    );
    assert!(
        !hints.contains("已加载"),
        "status leaked into the hints band"
    );
    // Both bands share the panel fill: the hints row must not turn into a
    // near-black strip under the cards.
    assert_eq!(buffer[(115, 38)].bg, SURFACE);
    assert_eq!(buffer[(115, 39)].bg, SURFACE);

    // A narrow terminal grows the footer downwards instead of hiding hints.
    app.terminal_width = 80;
    app.terminal_height = 24;
    let regions = app.layout_chrome(Rect::new(0, 0, 80, 24));
    assert_eq!(regions.footer.height, 3);
    assert_eq!(regions.footer.bottom(), 24);
}

#[test]
fn creeper_dialogs_dim_the_page_behind_them() {
    use crate::tui::palette::SURFACE;
    use ratatui::style::Color;

    let mut app = creeper_download_app(
        crate::tui::pages::download_center::DownloadCategory::Resource(ResourceKind::Mod),
    );
    let render = |app: &App| {
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|frame| app.draw(frame)).expect("draw");
        terminal.backend().buffer().clone()
    };

    let plain = render(&app);
    assert_eq!(plain[(2, 39)].bg, SURFACE);

    app.modal = Some(Modal::confirm(
        "确认移除？",
        crate::tui::modal::ConfirmAction::ClearDownloadCache,
    ));
    let dimmed = render(&app);
    // Footer strip is still visible behind the dialog, just darker.
    assert_eq!(dimmed[(2, 39)].bg, Color::Rgb(9, 11, 16));
    assert_ne!(dimmed[(2, 38)].fg, plain[(2, 38)].fg);
    // The dialog card itself is drawn after the scrim at full contrast.
    let area = app
        .modal
        .as_ref()
        .expect("modal")
        .area(Rect::new(0, 0, 120, 40));
    assert_eq!(dimmed[(area.x + 2, area.y + 1)].bg, SURFACE);
}

#[test]
fn creeper_settings_adjust_inline_and_keep_one_editor_per_row() {
    let mut app = creeper_download_app(
        crate::tui::pages::download_center::DownloadCategory::Resource(ResourceKind::Mod),
    );
    app.page = Page::Settings;
    app.settings.select_category(SettingsCategory::General);
    app.settings.selected = 0;

    let rendered = render_app(&app, 120, 40);
    assert!(rendered.contains("‹"), "{rendered}");
    assert!(rendered.contains("›"), "{rendered}");

    let before = app.config.launcher.visibility;
    app.handle(key(KeyCode::Right));
    assert_ne!(app.config.launcher.visibility, before);
    app.handle(key(KeyCode::Left));
    assert_eq!(app.config.launcher.visibility, before);

    // Space toggles the next row in place, without opening a dialog.
    app.settings.selected = 1;
    let before = app.config.launcher.auto_check_updates;
    app.handle(key(KeyCode::Char(' ')));
    assert_ne!(app.config.launcher.auto_check_updates, before);
    assert!(app.modal.is_none());

    // A text row opens exactly one field. (Index 4: visibility, auto-updates,
    // check-now, add-to-path sit above the server address.)
    app.settings.selected = 4;
    app.handle(key(KeyCode::Enter));
    assert!(matches!(
        app.modal.as_ref(),
        Some(Modal::Prompt {
            action: FormAction::SettingValue(SettingId::ServerAddress),
            ..
        })
    ));
}

#[test]
fn creeper_settings_search_jumps_across_categories() {
    let mut app = creeper_download_app(
        crate::tui::pages::download_center::DownloadCategory::Resource(ResourceKind::Mod),
    );
    app.page = Page::Settings;
    app.handle(key(KeyCode::Char('/')));
    assert!(matches!(
        app.mode(),
        crate::tui::command::Mode::Search(crate::tui::input::SearchTarget::Settings)
    ));
    for character in "内存".chars() {
        app.handle(key(KeyCode::Char(character)));
    }
    let rendered = compact_frame(&render_app(&app, 120, 40));
    assert!(rendered.contains("搜索设置"), "{rendered}");
    assert!(rendered.contains("个结果"), "{rendered}");

    // Results are a real list: Down moves the highlight before Enter jumps.
    app.handle(key(KeyCode::Down));
    app.handle(key(KeyCode::Enter));
    assert!(app.modal.is_none());
    assert_eq!(app.settings.category, SettingsCategory::Game);
    assert_eq!(
        app.setting_ids()[app.settings.selected],
        SettingId::MemoryMinimum
    );
    assert_eq!(app.mode(), crate::tui::command::Mode::Normal);
}

#[test]
fn creeper_java_category_opens_the_runtime_list_directly() {
    let mut app = creeper_download_app(
        crate::tui::pages::download_center::DownloadCategory::Resource(ResourceKind::Mod),
    );
    app.page = Page::Settings;
    app.java = vec![JavaInstallation {
        executable: PathBuf::from("java-21"),
        version: "21.0.1".into(),
        major: 21,
        vendor: None,
        is_jdk: true,
        lts: true,
    }];
    app.java_selected = 0;
    app.settings.select_category(SettingsCategory::Java);

    let rendered = render_app(&app, 120, 40);
    assert!(rendered.contains("Java 21"), "{rendered}");
    app.handle(key(KeyCode::Enter));
    assert_eq!(
        app.config.java.selected_path.as_deref(),
        Some(std::path::Path::new("java-21"))
    );
}

#[test]
fn instance_override_does_not_leak_between_instances() {
    let root = std::env::temp_dir().join(format!(
        "tmcl-override-leak-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos()
    ));
    let paths = DataPaths::from_appdata(root.join("AppData"));
    paths.ensure_dirs().expect("create data paths");
    let java = JavaInstallation {
        executable: root.join("Java/jdk-21/bin/java.exe"),
        version: "21.0.10".into(),
        major: 21,
        vendor: None,
        is_jdk: true,
        lts: true,
    };
    std::fs::create_dir_all(java.executable.parent().expect("java dir"))
        .expect("create java directory");
    std::fs::write(&java.executable, b"java").expect("write java");

    let summarize = |name: &str| {
        let path = root.join("game/versions").join(name);
        std::fs::create_dir_all(&path).expect("create instance directory");
        std::fs::write(
            path.join(format!("{name}.json")),
            br#"{"id":"demo","type":"release","mainClass":"net.minecraft.Main","javaVersion":{"majorVersion":21},"arguments":{"game":[],"jvm":[]}}"#,
        )
        .expect("write client JSON");
        std::fs::write(path.join(format!("{name}.jar")), b"jar").expect("write client jar");
        crate::tui::pages::instances::InstanceSummary {
            id: uuid::Uuid::new_v4(),
            name: name.into(),
            version: "1.20.1".into(),
            loader: "Vanilla".into(),
            loader_version: None,
            status: "ready".into(),
            path,
            launch_count: 0,
            play_time_seconds: 0,
        }
    };
    let alpha = summarize("alpha");
    let beta = summarize("beta");

    let mut config = Config::default();
    config.launch.memory_mode = crate::core::config::models::MemoryMode::Manual;
    config.launch.min_memory_mb = 1024;
    config.launch.max_memory_mb = 4096;
    let account =
        crate::core::account::offline::create_account("Alex", None).expect("offline account");

    let alpha_overrides = crate::core::instance::overrides::LaunchOverrides {
        min_memory_mb: Some(2048),
        max_memory_mb: Some(8192),
        ..Default::default()
    };
    let alpha_spec = build_for(
        &paths,
        &config,
        std::slice::from_ref(&java),
        &alpha,
        &alpha_overrides,
        &account,
        "0",
    )
    .expect("alpha launch command");
    let beta_spec = build_for(
        &paths,
        &config,
        std::slice::from_ref(&java),
        &beta,
        &crate::core::instance::overrides::LaunchOverrides::default(),
        &account,
        "0",
    )
    .expect("beta launch command");

    assert!(
        alpha_spec.args.iter().any(|arg| arg == "-Xmx8192M"),
        "{:?}",
        alpha_spec.args
    );
    assert!(
        beta_spec.args.iter().any(|arg| arg == "-Xmx4096M"),
        "beta must keep following the global memory: {:?}",
        beta_spec.args
    );
    std::fs::remove_dir_all(root).expect("clean test files");
}

#[test]
fn creeper_inline_search_edits_at_the_cursor() {
    let mut app = creeper_download_app(
        crate::tui::pages::download_center::DownloadCategory::Resource(ResourceKind::Mod),
    );
    app.download_center.search = "sodium".into();
    app.handle(key(KeyCode::Char('/')));

    // Two steps left, then insert: the character lands between `i` and `u`.
    app.handle(key(KeyCode::Left));
    app.handle(key(KeyCode::Left));
    app.handle(key(KeyCode::Char('X')));
    assert_eq!(app.download_center.search, "sodiXum");

    // Home + Delete removes the first character, not the last.
    app.handle(key(KeyCode::Home));
    app.handle(key(KeyCode::Delete));
    assert_eq!(app.download_center.search, "odiXum");
    assert_eq!(app.search.expect("still editing").cursor, 0);
}

/// Adding a PATH entry changes something outside the launcher, so it asks
/// first — and the dialog is where the exact directory is spelled out.
#[test]
fn add_to_path_opens_a_confirmation_instead_of_writing_at_once() {
    let mut app = creeper_download_app(
        crate::tui::pages::download_center::DownloadCategory::Resource(ResourceKind::Mod),
    );
    app.page = Page::Settings;
    app.settings.select_category(SettingsCategory::General);
    let index = app
        .setting_ids()
        .iter()
        .position(|id| *id == SettingId::AddToPath)
        .expect("the row exists");
    app.settings.selected = index;

    app.handle(key(KeyCode::Enter));

    match app.modal.as_ref() {
        Some(Modal::Confirm { action, prompt }) => {
            assert_eq!(*action, crate::tui::modal::ConfirmAction::AddToPath);
            assert!(!prompt.is_empty(), "the dialog has to say what will happen");
        }
        other => panic!("expected a confirmation, got {other:?}"),
    }
}
