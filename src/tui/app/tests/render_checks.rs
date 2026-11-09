//! Render snapshots and the size matrix, split out of `tests.rs`.

use std::path::PathBuf;

use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use uuid::Uuid;

use super::{App, InstanceSummary, Page, render_app};
use crate::core::config::models::Language;
use crate::i18n::I18n;

fn render_text(app: &App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("create test terminal");
    terminal.draw(|frame| app.draw(frame)).expect("draw app");
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..height {
        let mut line = String::new();
        for x in 0..width {
            line.push_str(buffer[(x, y)].symbol());
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// The frame itself, when a test needs the cells rather than the text.
fn render_buffer(app: &App, width: u16, height: u16) -> ratatui::buffer::Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("create test terminal");
    terminal.draw(|frame| app.draw(frame)).expect("draw app");
    terminal.backend().buffer().clone()
}

/// Read a snapshot for comparison.
///
/// The renderer emits `\n`; a checkout with `core.autocrlf` turns the fixture
/// into CRLF. That is not a rendering change, so it must not fail the test.
fn read_snapshot(path: &std::path::Path) -> String {
    std::fs::read_to_string(path)
        .map(|text| text.replace("\r\n", "\n"))
        .unwrap_or_else(|error| {
            panic!(
                "missing snapshot {}: {error}; run UPDATE_FIXTURES=1",
                path.display()
            )
        })
}

#[test]
fn tui_snapshots_match_fixtures() {
    let cases = [
        ("overview", Page::Overview),
        ("instances", Page::Instances),
        ("download", Page::DownloadCenter),
        ("tasks", Page::Tasks),
    ];
    for (name, page) in cases {
        let mut app = App::new();
        app.page = page;
        app.terminal_width = 100;
        app.terminal_height = 30;
        let actual = render_text(&app, 100, 30);
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/tui");
        let path = dir.join(format!("{name}-100x30.txt"));
        if std::env::var("UPDATE_FIXTURES").is_ok() {
            std::fs::create_dir_all(&dir).expect("create fixture dir");
            std::fs::write(&path, &actual).expect("write fixture");
            continue;
        }
        let expected = read_snapshot(&path);
        assert_eq!(actual, expected, "snapshot mismatch for {name}");
    }
}

#[test]
fn size_matrix_keeps_header_tabs_and_quit_hint_for_every_page() {
    for (width, height) in [(120, 40), (100, 30), (80, 24)] {
        for page in Page::ALL {
            let mut app = App::new();
            app.page = page;
            app.terminal_width = width;
            app.terminal_height = height;
            let rendered = render_text(&app, width, height);
            assert!(rendered.contains("tmcl"), "{page:?} at {width}x{height}");
            assert!(rendered.contains("Quit"), "{page:?} at {width}x{height}");
            let mode = app.layout_chrome(Rect::new(0, 0, width, height)).mode;
            let labels = app.tab_labels(mode);
            let index = Page::ALL
                .iter()
                .position(|candidate| *candidate == page)
                .expect("page index");
            assert!(
                rendered.contains(&labels[index]),
                "{page:?} tab label {:?} missing at {width}x{height}",
                labels[index]
            );
        }
    }
}

#[test]
fn all_pages_render_at_full_compact_narrow_and_minimum_sizes() {
    for (width, height) in [(120, 40), (100, 30), (95, 29), (80, 24)] {
        for page in Page::ALL {
            let mut app = App::new();
            app.page = page;
            app.terminal_width = width;
            app.terminal_height = height;
            if page == Page::Logs {
                app.logs.append("[stdout] render matrix");
            }
            let rendered = render_app(&app, width, height);
            assert!(rendered.contains("tmcl"), "{page:?} at {width}x{height}");
        }
    }
}

#[test]
fn overview_setup_fields_share_one_separator_column() {
    for language in [Language::English, Language::SimplifiedChinese] {
        let mut app = App::new();
        app.i18n = I18n::new(language);
        app.config.game_directories = vec![PathBuf::from(r"D:\games\.minecraft")];
        app.instances.apply(vec![InstanceSummary {
            id: Uuid::new_v4(),
            name: "1.20.1".into(),
            version: "1.20.1".into(),
            loader: "Vanilla".into(),
            loader_version: None,
            status: "ready".into(),
            path: PathBuf::from("1.20.1"),
            launch_count: 0,
            play_time_seconds: 0,
        }]);
        let labels = [
            "shell.instance",
            "overview.version",
            "overview.loader",
            "shell.account",
            "shell.java",
            "shell.game",
        ]
        .map(|key| app.i18n.text(key));
        if language == Language::SimplifiedChinese {
            assert!(
                labels.iter().any(|label| !label.is_ascii()),
                "{language:?} labels must cover wide characters"
            );
        }

        let (width, height) = (120u16, 40u16);
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|frame| app.draw(frame)).expect("draw app");
        let buffer = terminal.backend().buffer();
        let mut columns = Vec::new();
        for y in 0..height {
            // One buffer cell per terminal column, so an index is a column number.
            let cells = (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<Vec<_>>();
            let Some(separator) = cells.iter().position(|symbol| *symbol == ":") else {
                continue;
            };
            // A wide glyph leaves a blank continuation cell behind it, so drop
            // blanks before asking whether this row opens with a setup label.
            let prefix = cells[..separator]
                .concat()
                .split_whitespace()
                .collect::<String>();
            if labels
                .iter()
                .any(|label| prefix.contains(&label.split_whitespace().collect::<String>()))
            {
                columns.push(separator);
            }
        }

        assert_eq!(
            columns.len(),
            labels.len(),
            "{language:?} must render every setup field, got columns {columns:?}"
        );
        assert!(
            columns.iter().all(|column| *column == columns[0]),
            "{language:?} setup separators drift out of column: {columns:?}"
        );
    }
}

#[test]
fn shelf_draws_its_own_shell_instead_of_the_classic_frame() {
    use crate::core::config::models::UiPreset;

    for (width, height) in [(120u16, 40u16), (80, 24)] {
        let mut app = nightlight_app_at(Page::Instances, width, height);
        app.config.state.ui.preset = UiPreset::Shelf;
        let text = render_text(&app, width, height);
        assert!(
            text.contains(env!("CARGO_PKG_VERSION")),
            "{width}x{height}: shelf lost its wordmark\n{text}"
        );
        let first = text.lines().next().unwrap_or("").trim_start();
        assert!(
            !first.starts_with('╭') && !first.starts_with('│'),
            "{width}x{height}: the classic frame leaked into shelf\n{text}"
        );
        let title = app.i18n.text(Page::Instances.title_key());
        assert!(
            text.contains(&title),
            "{width}x{height}: shelf lost the page title\n{text}"
        );
    }
}

/// The facts column keeps a loader version whole: at 120×40 the old 2×3 grid
/// cut `NeoForge 21.1.247` to `NeoForge 21.1.2…`, and at 100×30 to `NeoForge 2…`.
#[test]
fn shelf_instance_facts_keep_the_loader_version_whole() {
    use crate::core::config::models::UiPreset;

    for (width, height) in [(120u16, 40u16), (100, 30), (160, 50)] {
        let mut app = nightlight_app_at(Page::Instances, width, height);
        app.config.state.ui.preset = UiPreset::Shelf;
        app.instances.apply(vec![InstanceSummary {
            id: Uuid::new_v4(),
            name: "Loader Test".into(),
            version: "1.21.4".into(),
            loader: "NeoForge".into(),
            loader_version: Some("21.1.247".into()),
            status: "ready".into(),
            path: PathBuf::from("D:/games/.minecraft/versions/Loader"),
            launch_count: 1,
            play_time_seconds: 60,
        }]);

        let text = render_text(&app, width, height);
        assert_eq!(
            text.matches("NeoForge 21.1.247").count(),
            2,
            "the list and the facts column must both carry the whole version at {width}x{height}:\n{text}"
        );
        assert!(
            !text.contains("NeoForge 21.1.2…"),
            "the facts column truncated the loader at {width}x{height}:\n{text}"
        );
    }
}

/// The current game directory is marked in its own column. Painted over the
/// path, the marker turns `D:/games/.minecraft` into `Currentmes/…`.
#[test]
fn shelf_settings_prints_the_current_directory_without_overwriting_it() {
    use crate::tui::pages::settings::SettingsCategory;

    let mut app = shelf_data_app(Page::Settings);
    app.settings
        .select_category(SettingsCategory::GameDirectories);
    app.config.game_directories = vec![PathBuf::from("D:/games/.minecraft")];

    let text = render_text(&app, 120, 40);
    assert!(
        text.contains(&app.i18n.text("settings.current_directory")),
        "the current directory lost its marker:\n{text}"
    );
    assert!(
        text.contains("games/.minecraft"),
        "the marker covered the path:\n{text}"
    );
    assert!(
        !text.contains("Currentmes"),
        "the marker and the path share a cell:\n{text}"
    );
}

/// A terminal under 64×18 has no key row, so the one key that works has to be
/// printed under the message.
#[test]
fn shelf_too_small_still_prints_the_quit_key() {
    let mut app = shelf_app(Page::Overview);
    app.terminal_width = 63;
    app.terminal_height = 17;

    let text = render_text(&app, 63, 17);
    assert!(
        text.contains(&app.i18n.text("common.quit")),
        "the tiny layout dropped its way out:\n{text}"
    );
}

/// `PageDown` on the version list moves by the rows painted. A narrow layout that
/// reserves a border the painter does not draw pages past the content.
#[test]
fn shelf_version_paging_matches_the_rows_it_paints() {
    use crate::core::version::models::VersionSummary;
    use crate::tui::pages::download_center::DownloadCategory;

    let mut app = shelf_data_app(Page::DownloadCenter);
    app.terminal_width = 80;
    app.terminal_height = 24;
    app.download_center.category = DownloadCategory::GameVersions;
    app.versions = (0..20)
        .map(|index| VersionSummary {
            id: format!("1.20.{index}"),
            kind: "release".into(),
            url: String::new(),
            time: String::new(),
            release_time: "2026-01-14T12:00:00+00:00".into(),
            sha1: None,
            compliance_level: None,
        })
        .collect();

    let visible = app.version_visible_rows();
    let text = render_text(&app, 80, 24);
    let painted = text.lines().filter(|line| line.contains("1.20.")).count();
    assert_eq!(
        visible, painted,
        "a page step of {visible} rows over {painted} painted rows:\n{text}"
    );
}

#[test]
fn shelf_instances_pair_the_cover_with_the_facts() {
    use crate::core::config::models::UiPreset;

    let root = std::env::temp_dir().join(format!("tmcl-shelf-instance-{}", Uuid::new_v4()));
    write_cover(&root.join("saves/Castle/icon.png"));

    let mut app = nightlight_app_at(Page::Instances, 120, 40);
    app.config.state.ui.preset = UiPreset::Shelf;
    app.instances.apply(vec![InstanceSummary {
        id: Uuid::new_v4(),
        name: "Shelf Test".into(),
        version: "1.21.4".into(),
        loader: "NeoForge".into(),
        loader_version: Some("21.1".into()),
        status: "ready".into(),
        path: root.clone(),
        launch_count: 3,
        play_time_seconds: 7_200,
    }]);

    let wide = render_text(&app, 120, 40);
    assert!(wide.contains("Shelf Test"), "{wide}");
    assert!(wide.contains("NeoForge 21.1"), "{wide}");
    assert!(
        wide.matches('▀').count() > 34 * 10,
        "the 34x34 cover was not painted:\n{wide}"
    );

    app.terminal_width = 80;
    app.terminal_height = 24;
    let narrow = render_text(&app, 80, 24);
    assert!(narrow.contains("Shelf Test"), "{narrow}");
    assert!(
        !narrow.contains('▀'),
        "a narrow shelf page must not reserve a cover column:\n{narrow}"
    );

    std::fs::remove_dir_all(root).expect("clean");
}

/// The world panel under the facts: a count, the newest three, their age.
#[test]
fn shelf_instance_world_panel_lists_the_newest_worlds() {
    use crate::core::config::models::UiPreset;

    let root = std::env::temp_dir().join(format!("tmcl-shelf-worlds-{}", Uuid::new_v4()));
    write_world(&root, "Castlekeep", 3_600);
    write_world(&root, "Skyblockia", 3 * 86_400);
    write_world(&root, "Oldrealm", 10 * 86_400);
    // A fourth world must not push the panel past three rows.
    write_world(&root, "Fourthsave", 30 * 86_400);

    let mut app = nightlight_app_at(Page::Instances, 120, 40);
    app.config.state.ui.preset = UiPreset::Shelf;
    app.instances.apply(vec![InstanceSummary {
        id: Uuid::new_v4(),
        name: "Worlds Test".into(),
        version: "1.21.4".into(),
        loader: "NeoForge".into(),
        loader_version: Some("21.1".into()),
        status: "ready".into(),
        path: root.clone(),
        launch_count: 3,
        play_time_seconds: 7_200,
    }]);

    let text = render_text(&app, 120, 40);
    let label = app.i18n.text("workspace.category.worlds");
    // The instance is called "Worlds Test" and its name bar also sits in the
    // panel, so the header is the line whose tail after the label is only the
    // right-aligned count.
    let header = text
        .lines()
        .filter_map(|line| line.rfind(&label).map(|at| (line, at)))
        .find(|(line, at)| {
            line[*at + label.len()..]
                .trim_end_matches('│')
                .trim()
                .chars()
                .all(|letter| letter.is_ascii_digit())
        })
        .map(|(line, _)| line)
        .expect("the panel header is on screen");
    assert!(
        header.trim_end_matches('│').trim_end().ends_with('4'),
        "the count must close the header row: {header:?}"
    );
    for name in ["Castlekeep", "Skyblockia", "Oldrealm"] {
        assert!(text.contains(name), "{name} is missing:\n{text}");
    }
    assert!(
        !text.contains("Fourthsave"),
        "the panel shows three worlds, not the whole list:\n{text}"
    );
    assert!(
        text.contains("1h"),
        "the newest world needs its age:\n{text}"
    );
    assert!(
        text.contains("3d"),
        "the second world needs its age:\n{text}"
    );

    std::fs::remove_dir_all(root).expect("clean");
}

/// A world directory with a `level.dat` stamped `age_seconds` in the past.
fn write_world(root: &std::path::Path, name: &str, age_seconds: u64) {
    let dir = root.join("saves").join(name);
    std::fs::create_dir_all(&dir).expect("create world dir");
    let level = dir.join("level.dat");
    std::fs::write(&level, b"level").expect("write level.dat");
    let stamp = std::time::SystemTime::now() - std::time::Duration::from_secs(age_seconds);
    std::fs::File::options()
        .write(true)
        .open(&level)
        .expect("open level.dat")
        .set_modified(stamp)
        .expect("stamp level.dat");
}

fn write_cover(path: &std::path::Path) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create cover dir");
    }
    let file = std::fs::File::create(path).expect("create cover png");
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), 4, 4);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("cover header");
    writer
        .write_image_data(&[255, 0, 0, 255].repeat(16))
        .expect("cover data");
    writer.finish().expect("cover finish");
}

fn shelf_app(page: Page) -> App {
    use crate::core::config::models::UiPreset;

    let mut app = App::new();
    app.page = page;
    app.config.state.ui.preset = UiPreset::Shelf;
    app.terminal_width = 120;
    app.terminal_height = 40;
    let current = Uuid::from_u128(0x5E1F_0001);
    app.instances.apply(vec![
        InstanceSummary {
            id: current,
            name: "Create Above and Beyond".into(),
            version: "1.21.4".into(),
            loader: "NeoForge".into(),
            loader_version: Some("21.1.247".into()),
            status: "ready".into(),
            path: PathBuf::from("C:/games/.minecraft/versions/CAB"),
            launch_count: 12,
            play_time_seconds: 7_200,
        },
        InstanceSummary {
            id: Uuid::from_u128(0x5E1F_0002),
            name: "Better MC".into(),
            version: "1.20.1".into(),
            loader: "Forge".into(),
            loader_version: Some("47.3.0".into()),
            status: "ready".into(),
            path: PathBuf::from("C:/games/.minecraft/versions/BMC"),
            launch_count: 3,
            play_time_seconds: 1_800,
        },
        InstanceSummary {
            id: Uuid::from_u128(0x5E1F_0003),
            name: "All the Mods 10".into(),
            version: "1.21.1".into(),
            loader: "NeoForge".into(),
            loader_version: Some("21.1.72".into()),
            status: "installing".into(),
            path: PathBuf::from("C:/games/.minecraft/versions/ATM10"),
            launch_count: 0,
            play_time_seconds: 0,
        },
    ]);
    app.instances.current_id = Some(current);
    app
}

fn shelf_data_app(page: Page) -> App {
    use crate::core::resource::model::{ResourceProject, ResourceSource};
    use crate::core::task::{TaskOperationType, TaskSnapshot, TaskState};
    use crate::core::version::models::VersionSummary;

    let mut app = shelf_app(page);
    app.versions = (0..5)
        .map(|index| VersionSummary {
            id: format!("1.21.{}", 5 - index),
            kind: "release".into(),
            url: String::new(),
            time: String::new(),
            release_time: format!("2026-0{}-14T12:00:00+00:00", index + 1),
            sha1: None,
            compliance_level: None,
        })
        .collect();
    app.resource_projects = ["Farming Tales", "Create: Above and Beyond", "Sodium"]
        .iter()
        .enumerate()
        .map(|(index, title)| ResourceProject {
            source: ResourceSource::Modrinth,
            project_id: format!("shelf-project-{index}"),
            title: (*title).to_owned(),
            author: "shelf".into(),
            description: "A record long enough to need the column beside the list.".into(),
            icon_url: None,
            categories: Vec::new(),
            game_versions: vec!["1.21.4".into()],
            loaders: vec!["neoforge".into()],
            downloads: Some(1_200 + index as u64),
            updated_at: None,
        })
        .collect();
    use crate::core::account::models::{Account, AccountKind};
    let now = crate::tui::app::state::data::unix_seconds();
    app.accounts.accounts = vec![
        Account {
            id: Uuid::from_u128(0x5E1F_4001),
            kind: AccountKind::Offline,
            username: "Steve".into(),
            uuid: Uuid::from_u128(0x5E1F_5001),
            skin: None,
            secret_ref: None,
            access_expires_at: None,
        },
        Account {
            id: Uuid::from_u128(0x5E1F_4002),
            kind: AccountKind::Microsoft,
            username: "Alex".into(),
            uuid: Uuid::from_u128(0x5E1F_5002),
            skin: None,
            secret_ref: Some("alex".into()),
            access_expires_at: Some(now + 3_600),
        },
        Account {
            id: Uuid::from_u128(0x5E1F_4003),
            kind: AccountKind::Microsoft,
            username: "Herobrine".into(),
            uuid: Uuid::from_u128(0x5E1F_5003),
            skin: None,
            secret_ref: Some("hero".into()),
            access_expires_at: Some(now - 60),
        },
    ];
    app.accounts.current = Some(Uuid::from_u128(0x5E1F_4001));
    for line in [
        "[stdout] Starting Minecraft 1.21.4",
        "[main/INFO] Loading 214 mods",
        "[main/INFO] Preloading 391 textures",
        "[main/WARN] mixin conflict: create vs sodium",
        "[main/INFO] Sound engine started",
        "[stderr] java.lang.NoClassDefFoundError: create/Flywheel",
    ] {
        app.logs.append(line);
    }
    app.task_snapshots = vec![
        TaskSnapshot {
            job: None,
            created_seq: 0,
            id: Uuid::from_u128(0x5E1F_1001),
            label: "Installing NeoForge 21.1.247".into(),
            operation_type: TaskOperationType::InstanceInstall,
            state: TaskState::Running,
            retryable: false,
            attempt: 0,
            completed: 62,
            total: Some(100),
            failure: None,
        },
        TaskSnapshot {
            job: None,
            created_seq: 1,
            id: Uuid::from_u128(0x5E1F_1002),
            label: "Downloading farmingtales-1.4.3.jar".into(),
            operation_type: TaskOperationType::ResourceInstall,
            state: TaskState::Queued,
            retryable: false,
            attempt: 0,
            completed: 0,
            total: Some(4_096),
            failure: None,
        },
        TaskSnapshot {
            job: None,
            created_seq: 2,
            id: Uuid::from_u128(0x5E1F_1003),
            label: "Downloading library-2.0.jar".into(),
            operation_type: TaskOperationType::ResourceInstall,
            state: TaskState::Succeeded,
            retryable: false,
            attempt: 0,
            completed: 4_096,
            total: Some(4_096),
            failure: None,
        },
        TaskSnapshot {
            job: None,
            created_seq: 3,
            id: Uuid::from_u128(0x5E1F_1004),
            label: "Installing Fabric loader".into(),
            operation_type: TaskOperationType::InstanceInstall,
            state: TaskState::Failed,
            retryable: true,
            attempt: 1,
            completed: 0,
            total: None,
            failure: Some("connection reset by peer".into()),
        },
    ];
    app
}

/// Six instances with distinct names. Tests that check how a *list* paints
/// need rows: an empty state exercises almost none of the row painters.
fn seed_instances(app: &mut App) {
    app.instances.apply(
        (0..6)
            .map(|index| InstanceSummary {
                id: Uuid::new_v4(),
                name: format!("Instance {index}"),
                version: "1.21.4".to_owned(),
                loader: "Forge".to_owned(),
                loader_version: Some("47.4.10".to_owned()),
                status: "ready".to_owned(),
                path: PathBuf::from(format!("D:/games/.minecraft/versions/Instance {index}")),
                launch_count: index,
                play_time_seconds: 60,
            })
            .collect(),
    );
}

#[test]
fn shelf_renders_every_page_at_every_size() {
    use crate::tui::pages::download_center::DownloadCategory;

    for (width, height) in [(64u16, 18u16), (80, 24), (100, 30), (120, 40), (160, 50)] {
        for page in Page::ALL {
            let mut app = shelf_app(page);
            app.terminal_width = width;
            app.terminal_height = height;
            let text = render_text(&app, width, height);
            assert!(
                text.contains(env!("CARGO_PKG_VERSION")),
                "{page:?} lost the wordmark at {width}x{height}:\n{text}"
            );
            let regions = app.layout_chrome(Rect::new(0, 0, width, height));
            assert!(
                regions.body.bottom() <= regions.footer.y,
                "{page:?} body overlaps the footer at {width}x{height}"
            );
            // Every cell carries the page colour: a cell left at the default
            // background is a hole the terminal's own colour shows through.
            let buffer = render_buffer(&app, width, height);
            let palette = app.shelf_palette();
            for (index, cell) in buffer.content.iter().enumerate() {
                assert_ne!(
                    cell.bg,
                    ratatui::style::Color::Reset,
                    "{page:?} at {width}x{height} left cell {index} unpainted"
                );
            }
            for (x, y) in [
                (0, 0),
                (width - 1, 0),
                (0, height - 1),
                (width - 1, height - 1),
            ] {
                assert_eq!(
                    buffer[(x, y)].bg,
                    palette.bg,
                    "{page:?} at {width}x{height}: corner ({x}, {y}) is not the page colour"
                );
            }
            if page == Page::Instances && width == 80 {
                // The narrow layout has no cover column at all, so no half
                // block may appear; the wordmark is plain text at this size.
                assert!(
                    !text.contains('▀'),
                    "80 columns must not reserve a cover column:\n{text}"
                );
            }
            // The classic shell wraps the screen in a border; shelf draws panels
            // of its own but never a frame around the page, so a `│` or `╭` in
            // column 0 means classic's frame was drawn here.
            let first = text.lines().next().unwrap_or("").trim_start();
            assert!(
                !first.starts_with('╭') && !first.starts_with('│'),
                "{page:?} drew the classic frame at {width}x{height}:\n{text}"
            );
            if page == Page::Accounts {
                assert!(
                    text.contains(&app.i18n.text("action.offline_account")),
                    "the empty accounts page lost its way out at {width}x{height}:\n{text}"
                );
            }
            if page == Page::Logs {
                assert!(
                    text.contains(&app.i18n.text("common.search")),
                    "the logs search row vanished at {width}x{height}:\n{text}"
                );
            }
            if page == Page::Settings {
                let category = app.i18n.text(app.settings.category.title_key());
                assert!(
                    text.contains(&category),
                    "the current settings category is off the row at {width}x{height}:\n{text}"
                );
            }
            if page == Page::DownloadCenter {
                let current = app.i18n.text(
                    DownloadCategory::ALL
                        .iter()
                        .position(|category| *category == app.download_center.category)
                        .map(|index| DownloadCategory::ALL[index].label_key())
                        .unwrap_or("download.category_game_versions"),
                );
                assert!(
                    text.contains(&current),
                    "the current category is off the row at {width}x{height}:\n{text}"
                );
                assert!(
                    text.contains(&app.i18n.text("common.search")),
                    "the search row vanished at {width}x{height}:\n{text}"
                );
            }
            if page == Page::Tasks {
                let filter = app.i18n.text(app.task_filter.label_key());
                assert!(
                    text.contains(&filter),
                    "the task filter row vanished at {width}x{height}:\n{text}"
                );
            }
        }
    }

    let mut tiny = shelf_app(Page::Overview);
    tiny.terminal_width = 63;
    tiny.terminal_height = 17;
    let text = render_text(&tiny, 63, 17);
    let message = tiny.i18n.text("shelf.too_small");
    let first = message.lines().next().unwrap_or("");
    assert!(text.contains(first), "{text}");
}

/// The first `[dotted.key]` in `text`, the shape a missing translation falls
/// back to. `[ ]` (a key cap) and `[x]` are not keys and are ignored.
fn raw_key_in(text: &str) -> Option<String> {
    let mut rest = text;
    while let Some(start) = rest.find('[') {
        let after = &rest[start + 1..];
        let Some(end) = after.find(']') else { break };
        let candidate = &after[..end];
        if candidate.contains('.')
            && candidate.chars().all(|letter| {
                letter.is_ascii_lowercase() || letter.is_ascii_digit() || "._-".contains(letter)
            })
        {
            return Some(candidate.to_owned());
        }
        rest = &after[end..];
    }
    None
}

/// A key that does not exist falls back to `[the.key]`, and the page tests
/// cannot catch it: they take their expected text from the same catalog, so a
/// missing key would match its own fallback. Ask the frame directly.
#[test]
fn shelf_prints_no_raw_translation_key() {
    // The guard has to be able to fail, or it proves nothing.
    assert_eq!(
        raw_key_in("hit [shelf.tagline] here"),
        Some("shelf.tagline".to_owned())
    );
    assert_eq!(raw_key_in("[ ] Category"), None);

    for language in [Language::English, Language::SimplifiedChinese] {
        for page in Page::ALL {
            let mut app = shelf_data_app(page);
            app.i18n = I18n::new(language);
            app.terminal_width = 120;
            app.terminal_height = 40;
            let text = render_text(&app, 120, 40);
            if let Some(key) = raw_key_in(&text) {
                panic!("{page:?} printed the raw key {key} in {language:?}:\n{text}");
            }
        }
    }
}

#[test]
fn shelf_selection_is_a_filled_accent_card() {
    use crate::tui::action::Action;
    use ratatui::style::Color;

    let app = shelf_app(Page::Instances);
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).expect("create test terminal");
    terminal.draw(|frame| app.draw(frame)).expect("draw shelf");
    let buffer = terminal.backend().buffer();
    let rect = |index: usize| {
        app.hitmap
            .hitboxes()
            .iter()
            .find(|hitbox| hitbox.action == Action::row(Page::Instances, index))
            .map(|hitbox| hitbox.rect)
            .unwrap_or_else(|| panic!("no card {index}"))
    };
    let selected = rect(0);
    assert_eq!(
        buffer[(selected.x + 2, selected.y)].bg,
        Color::Rgb(0xA8, 0x96, 0xFF),
        "the selected card is not filled with the accent"
    );
    let other = rect(1);
    assert_eq!(
        buffer[(other.x + 2, other.y)].bg,
        Color::Rgb(0x12, 0x10, 0x1C),
        "an unselected card must keep the page colour"
    );
}

#[test]
fn shelf_snapshots_match_fixtures() {
    let cases = [
        ("overview", Page::Overview),
        ("instances", Page::Instances),
        ("download", Page::DownloadCenter),
        ("tasks", Page::Tasks),
        ("accounts", Page::Accounts),
        ("logs", Page::Logs),
        ("settings", Page::Settings),
    ];
    let sizes = [(120u16, 40u16), (80, 24)];
    for (name, page) in cases {
        for (width, height) in sizes {
            let mut app = shelf_data_app(page);
            app.terminal_width = width;
            app.terminal_height = height;
            let actual = render_text(&app, width, height);
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/tui");
            let path = dir.join(format!("shelf-{name}-{width}x{height}.txt"));
            if std::env::var("UPDATE_FIXTURES").is_ok() {
                std::fs::create_dir_all(&dir).expect("create fixture dir");
                std::fs::write(&path, &actual).expect("write fixture");
                continue;
            }
            let expected = read_snapshot(&path);
            assert_eq!(actual, expected, "snapshot mismatch for shelf-{name}");
        }
    }
}

/// A shelf workspace opened over the same synthetic instance the nightlight
/// one uses, so nothing is read from disk.
fn shelf_workspace_app(category: crate::tui::pages::workspace::WorkspaceCategory) -> App {
    use crate::core::config::models::UiPreset;

    let mut app = nightlight_workspace_app();
    app.config.state.ui.preset = UiPreset::Shelf;
    app.terminal_width = 120;
    app.terminal_height = 40;
    if let Some(workspace) = app.workspace.as_mut() {
        workspace.category = category;
    }
    app
}

#[test]
fn shelf_workspace_snapshots() {
    use crate::tui::pages::workspace::WorkspaceCategory;

    let cases = [
        ("overview", WorkspaceCategory::Overview),
        ("launch", WorkspaceCategory::Launch),
        ("mods", WorkspaceCategory::Mods),
        ("worlds", WorkspaceCategory::Worlds),
    ];
    let sizes = [(120u16, 40u16), (80, 24)];
    for (name, category) in cases {
        for (width, height) in sizes {
            let mut app = shelf_workspace_app(category);
            app.terminal_width = width;
            app.terminal_height = height;
            let actual = render_text(&app, width, height);
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/tui");
            let path = dir.join(format!("shelf-workspace-{name}-{width}x{height}.txt"));
            if std::env::var("UPDATE_FIXTURES").is_ok() {
                std::fs::create_dir_all(&dir).expect("create fixture dir");
                std::fs::write(&path, &actual).expect("write fixture");
                continue;
            }
            let expected = read_snapshot(&path);
            assert_eq!(
                actual, expected,
                "snapshot mismatch for shelf workspace {name} at {width}x{height}"
            );
        }
    }
}

#[tokio::test]
async fn all_presets_share_state_for_random_keys() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    // Deterministic LCG so a failure is reproducible without a rand dependency.
    let mut seed = 0x5eed_1234_u64;
    let mut next = move || {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (seed >> 33) as u32
    };
    let codes = [
        KeyCode::Tab,
        KeyCode::BackTab,
        KeyCode::Enter,
        KeyCode::Esc,
        KeyCode::Up,
        KeyCode::Down,
        KeyCode::Left,
        KeyCode::Right,
        KeyCode::Home,
        KeyCode::End,
        KeyCode::PageUp,
        KeyCode::PageDown,
        KeyCode::Backspace,
        KeyCode::Char('l'),
        KeyCode::Char('n'),
        KeyCode::Char('i'),
        KeyCode::Char('d'),
        KeyCode::Char('a'),
        KeyCode::Char('p'),
        KeyCode::Char('h'),
        KeyCode::Char(' '),
    ];
    let modifiers = [
        KeyModifiers::NONE,
        KeyModifiers::NONE,
        KeyModifiers::NONE,
        KeyModifiers::SHIFT,
        KeyModifiers::CONTROL,
        KeyModifiers::ALT,
    ];

    // Every preset, from the same state: a key that means one thing in one
    // look and another elsewhere is the bug this walks for.
    let presets = crate::core::config::models::UiPreset::ALL;
    let mut apps = presets
        .iter()
        .map(|preset| {
            let mut app = nightlight_app_at(Page::Overview, 120, 40);
            app.config.state.ui.preset = *preset;
            (*preset, app)
        })
        .collect::<Vec<_>>();
    for step in 0..200 {
        let key = KeyEvent::new(
            codes[next() as usize % codes.len()],
            modifiers[next() as usize % modifiers.len()],
        );
        for (_, app) in apps.iter_mut() {
            app.handle_key(key);
        }
        let (base_preset, base) = &apps[0];
        for (preset, app) in apps.iter().skip(1) {
            assert_eq!(
                base.page, app.page,
                "page drifted at step {step} ({base_preset:?} vs {preset:?}): {key:?}"
            );
            assert_eq!(
                base.mode(),
                app.mode(),
                "mode drifted at step {step} ({base_preset:?} vs {preset:?}): {key:?}"
            );
            assert_eq!(
                base.selected(),
                app.selected(),
                "selection drifted at step {step} ({base_preset:?} vs {preset:?}): {key:?}"
            );
            assert_eq!(
                base.status.as_str(),
                app.status.as_str(),
                "status drifted at step {step} ({base_preset:?} vs {preset:?}): {key:?}"
            );
        }
    }
}

#[test]
fn nightlight_settings_cycle_and_persist() {
    use crate::core::config::models::{NightlightTheme, UiPreset};
    use crate::tui::pages::settings::{SettingId, SettingsCategory};

    let mut app = App::new();
    app.paths = crate::core::config::DataPaths::from_appdata(
        std::env::temp_dir().join(format!("tmcl-nightlight-{}", Uuid::new_v4())),
    );
    app.paths.ensure_dirs().expect("create data dirs");
    app.config.state.ui.preset = UiPreset::Nightlight;
    app.settings.select_category(SettingsCategory::Appearance);

    let ids = app.setting_ids();
    assert_eq!(ids.len(), 8, "appearance keeps eight rows");
    let theme_index = ids
        .iter()
        .position(|id| *id == SettingId::NightlightTheme)
        .expect("appearance exposes the nightlight theme row");
    let accent_index = ids
        .iter()
        .position(|id| *id == SettingId::LiveAccent)
        .expect("appearance exposes the live accent row");

    assert_eq!(
        app.settings_rows()[theme_index].value,
        app.i18n.text("theme.live")
    );
    app.settings.selected = theme_index;
    app.activate_setting(SettingId::NightlightTheme);
    assert_eq!(app.config.state.ui.nightlight_theme, NightlightTheme::Tokyo);
    assert_eq!(
        app.settings_rows()[theme_index].value,
        app.i18n.text("theme.tokyo")
    );
    let stored = crate::core::config::load_or_default(&app.paths).expect("reload config");
    assert_eq!(stored.state.ui.nightlight_theme, NightlightTheme::Tokyo);

    // `←` walks backwards, and the toggle writes through the same path.
    app.settings.selected = accent_index;
    assert!(app.config.state.ui.live_accent);
    app.toggle_setting(SettingId::LiveAccent);
    assert!(!app.config.state.ui.live_accent);
    let stored = crate::core::config::load_or_default(&app.paths).expect("reload config");
    assert!(!stored.state.ui.live_accent);
}

#[test]
fn appearance_rows_are_scoped_to_the_presets_that_read_them() {
    use crate::core::config::models::{PaperTheme, UiPreset};
    use crate::tui::pages::settings::{SettingId, SettingsCategory};

    let mut app = App::new();
    app.paths = crate::core::config::DataPaths::from_appdata(
        std::env::temp_dir().join(format!("tmcl-preset-scope-{}", Uuid::new_v4())),
    );
    app.paths.ensure_dirs().expect("create data dirs");
    app.settings.select_category(SettingsCategory::Appearance);

    // Only the skins that read a value offer its row. Classic paints with
    // `Theme::from_ui`, which reads the mode, the accent and the contrast flag,
    // so all three stay. Nightlight owns its own theme and the live accent;
    // paper owns the stock and inherits only the day / night mode.
    let expected = |preset: UiPreset| match preset {
        UiPreset::Classic => vec![
            SettingId::UiPreset,
            SettingId::ThemeMode,
            SettingId::HighContrast,
            SettingId::AccentColor,
            SettingId::Language,
            SettingId::Mouse,
        ],
        UiPreset::Nightlight => vec![
            SettingId::UiPreset,
            SettingId::ThemeMode,
            SettingId::HighContrast,
            SettingId::AccentColor,
            SettingId::NightlightTheme,
            SettingId::LiveAccent,
            SettingId::Language,
            SettingId::Mouse,
        ],
        UiPreset::Paper => vec![
            SettingId::UiPreset,
            SettingId::ThemeMode,
            SettingId::PaperTheme,
            SettingId::Language,
            SettingId::Mouse,
        ],
        // Shelf paints its own two faces, so day / night stays; the accent and
        // the contrast flag remain out of reach. Its binding is a second axis,
        // so the scheme row is here and paper's stock is not.
        UiPreset::Shelf => vec![
            SettingId::UiPreset,
            SettingId::ThemeMode,
            SettingId::ShelfTheme,
            SettingId::Language,
            SettingId::Mouse,
        ],
    };
    for preset in UiPreset::ALL {
        app.config.state.ui.preset = preset;
        let ids = app.setting_ids();
        assert_eq!(ids, expected(preset), "{preset:?}");
        // The list that is drawn and the list the cursor indexes are the same.
        assert_eq!(app.settings_rows().len(), ids.len(), "{preset:?}");
        assert_eq!(app.settings_row_count(), ids.len(), "{preset:?}");
    }

    // Search indexes the same rows: a query for a hidden row finds nothing.
    let accent = app.i18n.text("settings.accent");
    app.settings_search = Some(accent.clone());
    for (preset, hits) in [
        (UiPreset::Paper, 0),
        (UiPreset::Classic, 1),
        (UiPreset::Nightlight, 1),
    ] {
        app.config.state.ui.preset = preset;
        assert_eq!(
            app.settings_search_results().len(),
            hits,
            "{preset:?}: the accent row is searchable {hits} time(s)"
        );
    }
    app.settings_search = None;

    // Paper's stock still works and is written to disk.
    app.config.state.ui.preset = UiPreset::Paper;
    let index = app
        .setting_ids()
        .iter()
        .position(|id| *id == SettingId::PaperTheme)
        .expect("paper exposes the stock row");
    assert_eq!(
        app.settings_rows()[index].value,
        app.i18n.text("paper.theme.ivory")
    );
    app.settings.selected = index;
    app.activate_setting(SettingId::PaperTheme);
    assert_eq!(app.config.state.ui.paper_theme, PaperTheme::Ash);
    let stored = crate::core::config::load_or_default(&app.paths).expect("reload config");
    assert_eq!(stored.state.ui.paper_theme, PaperTheme::Ash);
}

/// The three shared rows are not decoration on `classic`.
///
/// `classic` paints every frame with `Theme::from_ui`, so the accent, the
/// contrast flag and the day / night mode each change what is on screen. That
/// is the whole reason the rows are scoped where they are, so it is asserted
/// rather than assumed: moving one of these rows must fail here first.
#[test]
fn the_shared_appearance_rows_repaint_classic() {
    use crate::core::config::models::UiPreset;
    use crate::tui::pages::settings::{SettingId, SettingsCategory};

    use crate::tui::app::state::status::Status;

    type Cell = (String, ratatui::style::Color, ratatui::style::Color);
    /// Every cell of the frame, status line included. The status line is
    /// cleared first: saving a setting writes `设置已保存` there, and comparing
    /// that would make the test pass without a single cell of the skin changing.
    fn frame(app: &mut App) -> Vec<Cell> {
        app.status = Status::info("");
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|frame| app.draw(frame)).expect("draw");
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| (cell.symbol().to_owned(), cell.fg, cell.bg))
            .collect()
    }

    // Classic reads the accent, the contrast flag and the mode: those belong to
    // the preset whose `Theme` carries them.
    let mut app = App::new();
    app.paths = crate::core::config::DataPaths::from_appdata(
        std::env::temp_dir().join(format!("tmcl-repaint-{}", Uuid::new_v4())),
    );
    app.paths.ensure_dirs().expect("create data dirs");
    app.config.state.ui.preset = UiPreset::Classic;
    app.page = Page::Settings;
    // A category without appearance values on screen: the change has to show up
    // in the skin, not in a `< value >` cell this test wrote.
    app.settings.select_category(SettingsCategory::General);
    app.terminal_width = 120;
    app.terminal_height = 40;

    let base = frame(&mut app);
    app.cycle_setting(SettingId::AccentColor, 1);
    assert!(
        frame(&mut app) != base,
        "accent_color must repaint the frame"
    );
    app.toggle_setting(SettingId::HighContrast);
    assert!(
        frame(&mut app) != base,
        "high_contrast must repaint the frame"
    );
    app.cycle_setting(SettingId::ThemeMode, 1);
    assert!(frame(&mut app) != base, "theme_mode must repaint the frame");
}

/// Switching preset can shorten the appearance list under the cursor: the
/// selection has to follow it instead of pointing past the end.
#[test]
fn switching_preset_keeps_the_cursor_inside_the_list() {
    use crate::core::config::models::UiPreset;
    use crate::tui::pages::settings::{SettingId, SettingsCategory};

    let mut app = App::new();
    app.config.state.ui.preset = UiPreset::Nightlight;
    app.settings.select_category(SettingsCategory::Appearance);
    assert_eq!(app.settings_row_count(), 8);
    app.settings.selected = 7;

    // Nightlight -> paper: five rows, so index 7 no longer exists.
    app.activate_setting(SettingId::UiPreset);
    assert_eq!(app.preset(), UiPreset::Paper);
    assert_eq!(app.settings_row_count(), 5);
    assert!(
        app.settings.selected < app.settings_row_count(),
        "the cursor stayed at {} on a list of {}",
        app.settings.selected,
        app.settings_row_count()
    );
}

#[test]
fn preset_setting_cycles_through_all_presets() {
    use crate::core::config::models::UiPreset;
    use crate::tui::pages::settings::{SettingId, SettingsCategory};

    let mut app = App::new();
    // Isolate the config file: the default test paths are shared by every
    // App::new() in the suite and parallel writes would race.
    app.paths = crate::core::config::DataPaths::from_appdata(
        std::env::temp_dir().join(format!("tmcl-preset-{}", Uuid::new_v4())),
    );
    app.paths.ensure_dirs().expect("create data dirs");
    assert_eq!(app.preset(), UiPreset::Classic);
    app.settings.select_category(SettingsCategory::Appearance);
    let ids = app.setting_ids();
    let index = ids
        .iter()
        .position(|id| *id == SettingId::UiPreset)
        .expect("appearance exposes the preset row");
    assert_eq!(
        app.settings_rows()[index].value,
        app.i18n.text("settings.preset.classic")
    );

    app.settings.selected = index;
    for (expected, label) in [
        (UiPreset::Nightlight, "settings.preset.nightlight"),
        (UiPreset::Paper, "settings.preset.paper"),
        (UiPreset::Shelf, "settings.preset.shelf"),
        (UiPreset::Classic, "settings.preset.classic"),
    ] {
        app.activate_setting(SettingId::UiPreset);
        assert_eq!(app.preset(), expected);
        assert_eq!(app.settings_rows()[index].value, app.i18n.text(label));

        // The choice is written to disk, not just held in memory.
        let stored = crate::core::config::load_or_default(&app.paths).expect("reload config");
        assert_eq!(stored.state.ui.preset, expected);
    }
}

#[test]
fn toast_stack_keeps_at_most_three_messages() {
    let mut app = nightlight_app_at(Page::Overview, 120, 40);
    app.motion = true;
    for text in ["ready", "downloading", "installing", "finished"] {
        app.status = crate::tui::app::state::status::Status::success(text);
        let _ = render_app(&app, 120, 40);
    }
    let stack = app.status_toasts.borrow();
    assert!(stack.len() <= 3, "toast stack grew to {}", stack.len());
    assert_eq!(stack.back().expect("newest toast").0, "finished");

    // Repeating the same status must not enqueue duplicates.
    let before = stack.len();
    drop(stack);
    app.status = crate::tui::app::state::status::Status::success("finished");
    let _ = render_app(&app, 120, 40);
    assert_eq!(app.status_toasts.borrow().len(), before);
}

#[test]
fn toast_does_not_return_after_it_expires() {
    let mut app = nightlight_app_at(Page::Overview, 120, 40);
    app.motion = true;
    app.status = crate::tui::app::state::status::Status::success("ZZTOAST 921 versions loaded");

    // Footer status + one toast.
    let frame = render_text(&app, 120, 40);
    assert_eq!(
        frame.matches("ZZTOAST").count(),
        2,
        "toast missing:\n{frame}"
    );

    // Rewind the toast so it is older than its TTL.
    {
        let mut log = app.status_toasts.borrow_mut();
        let mut entry = log.pop_back().expect("queued toast");
        entry.2 = entry
            .2
            .checked_sub(std::time::Duration::from_secs(30))
            .expect("instant in range");
        log.push_back(entry);
    }

    let frame = render_text(&app, 120, 40);
    assert_eq!(
        frame.matches("ZZTOAST").count(),
        1,
        "expired toast must not be redrawn:\n{frame}"
    );
    // The bug was that the next frame re-enqueued the unchanged status, so the
    // toast came back forever. It must stay gone.
    let frame = render_text(&app, 120, 40);
    assert_eq!(
        frame.matches("ZZTOAST").count(),
        1,
        "expired toast came back:\n{frame}"
    );
}

#[test]
fn routine_info_status_stays_in_the_footer() {
    // Opening the download page sets an info status. It must not pop a toast,
    // and it must not reappear every time the previous one expires.
    let mut app = nightlight_app_at(Page::DownloadCenter, 120, 40);
    app.motion = true;
    app.status = crate::tui::app::state::status::Status::info("ZZINFO 921 versions loaded");

    let frame = render_text(&app, 120, 40);
    assert_eq!(
        frame.matches("ZZINFO").count(),
        1,
        "info status must not float as a toast:\n{frame}"
    );
    assert!(app.status_toasts.borrow().is_empty());
}

// ── overview interaction ──────────────────────────────────────────

#[test]
fn a_failed_launch_leaves_a_verdict_instead_of_crash_lines() {
    use crate::core::diagnose::RuleId;
    use crate::tui::event::AppEvent;

    let root = std::env::temp_dir().join(format!("tmcl-crash-ui-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("dir");
    let log = root.join("launch.log");
    std::fs::write(
        &log,
        "[stderr] [19:02:11] [main/INFO]: Backend library: LWJGL version 3.3.3\n\
         [stderr] java.lang.OutOfMemoryError: Java heap space\n",
    )
    .expect("log");
    let mut app = App::new();
    app.handle(AppEvent::LaunchFinished {
        instance_id: Uuid::new_v4(),
        instance_directory: root.join("inst"),
        play_time_seconds: 2,
        exit_code: Some(1),
        log_path: log,
        crash_report: None,
        launch_command: String::new(),
    });

    let diagnosis = app.diagnosis.as_ref().expect("verdict");
    let primary = diagnosis.primary.as_ref().expect("primary");
    assert_eq!(primary.rule, RuleId::OutOfMemory);
    assert!(app.diagnosis_open, "the card opens on a crash by default");

    let reason = app.i18n.text("diagnose.reason.out_of_memory");
    assert_eq!(
        app.status.as_str(),
        app.i18n
            .format("status.crash_detected", &[("reason", &reason)])
    );
    // The verdict lives in the card now; nothing is appended to the scrollback.
    let text = app.logs.matching_lines("").join("\n");
    assert!(
        !text.contains("[crash]"),
        "old crash lines came back: {text}"
    );
    std::fs::remove_dir_all(root).expect("clean");
}

fn workspace_app(category: crate::tui::pages::workspace::WorkspaceCategory) -> App {
    let mut app = App::new();
    app.page = Page::Instances;
    app.terminal_width = 120;
    app.terminal_height = 40;
    app.instances
        .apply(vec![crate::tui::pages::instances::InstanceSummary {
            id: uuid::Uuid::new_v4(),
            name: "BattleArmory TACZ".into(),
            version: "1.20.1".into(),
            loader: "Forge".into(),
            loader_version: Some("47.4.22".into()),
            status: "ready".into(),
            path: std::path::PathBuf::from("C:/games/versions/BattleArmory TACZ"),
            launch_count: 7,
            play_time_seconds: 7_320,
        }]);
    app.open_workspace();
    if let Some(workspace) = app.workspace.as_mut() {
        workspace.draft.loader = crate::core::instance::models::InstanceLoader::Forge {
            version: "47.4.22".into(),
        };
        workspace.draft.stats.launch_count = 7;
        workspace.draft.stats.play_time_seconds = 7_320;
        workspace.category = category;
    }
    app
}

/// The workspace's 加载器 row goes through the same mapping.
///
/// The row is built from `InstanceLoader`, so `"Vanilla"` must not be spelled out
/// here; that is the one place the list's translation never reached.
#[test]
fn the_workspace_prints_the_vanilla_loader_in_chinese() {
    use crate::core::config::models::Language;
    use crate::core::instance::models::InstanceLoader;
    use crate::tui::pages::workspace::WorkspaceCategory;

    let mut app = workspace_app(WorkspaceCategory::Overview);
    app.i18n = crate::i18n::I18n::new(Language::SimplifiedChinese);
    if let Some(workspace) = app.workspace.as_mut() {
        workspace.draft.loader = InstanceLoader::Vanilla;
    }

    let text = render_text(&app, 120, 40);
    assert!(
        !text.contains("Vanilla"),
        "the workspace still prints the raw loader name:\n{text}"
    );
    let compact = text
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    assert!(
        compact.contains("加载器原版"),
        "the row must read 原版:\n{text}"
    );
}

#[test]
fn workspace_renders_overview_and_categories() {
    let app = workspace_app(crate::tui::pages::workspace::WorkspaceCategory::Overview);
    let rendered = render_text(&app, 120, 40);
    assert!(rendered.contains("BattleArmory TACZ"), "{rendered}");
    assert!(rendered.contains("1.20.1"), "{rendered}");
    assert!(rendered.contains("Forge 47.4.22"), "{rendered}");
    assert!(rendered.contains("All following global"), "{rendered}");
    // The rail lists every category.
    for label in [
        "Overview",
        "Game settings",
        "Install",
        "Mods",
        "Resource packs",
        "Shaders",
        "Worlds",
        "Manage",
    ] {
        assert!(rendered.contains(label), "{label} missing: {rendered}");
    }
}

#[test]
fn workspace_launch_category_shows_follow_and_custom_marks() {
    let mut app = workspace_app(crate::tui::pages::workspace::WorkspaceCategory::Launch);
    if let Some(workspace) = app.workspace.as_mut() {
        workspace.draft.overrides.max_memory_mb = Some(6144);
    }
    let rendered = render_text(&app, 120, 40);
    assert!(rendered.contains("Follow global"), "{rendered}");
    assert!(rendered.contains("Custom 6144 MB"), "{rendered}");
}

#[test]
fn workspace_content_category_shows_empty_state() {
    let app = workspace_app(crate::tui::pages::workspace::WorkspaceCategory::Mods);
    let rendered = render_text(&app, 120, 40);
    assert!(
        rendered.contains("No mods in this instance yet"),
        "{rendered}"
    );
}

#[test]
fn workspace_narrow_layout_hides_the_rail_labels() {
    let mut app = workspace_app(crate::tui::pages::workspace::WorkspaceCategory::Overview);
    app.terminal_width = 80;
    app.terminal_height = 24;
    let rendered = render_text(&app, 80, 24);
    // Narrow: one category line instead of a full rail.
    assert!(rendered.contains("‹ Overview ›"), "{rendered}");
    assert!(rendered.contains("BattleArmory TACZ"), "{rendered}");
}

#[test]
fn classic_workspace_snapshots() {
    use crate::tui::pages::workspace::WorkspaceCategory;
    let cases = [
        ("overview", WorkspaceCategory::Overview),
        ("launch", WorkspaceCategory::Launch),
        ("install", WorkspaceCategory::Install),
        ("mods", WorkspaceCategory::Mods),
        ("packs", WorkspaceCategory::ResourcePacks),
        ("shaders", WorkspaceCategory::Shaders),
        ("worlds", WorkspaceCategory::Worlds),
        ("manage", WorkspaceCategory::Manage),
    ];
    let sizes = [(120u16, 40u16), (80, 24)];
    for (name, category) in cases {
        for (width, height) in sizes {
            let mut app = workspace_app(category);
            app.config.state.ui.preset = crate::core::config::models::UiPreset::Classic;
            app.terminal_width = width;
            app.terminal_height = height;
            let actual = render_text(&app, width, height);
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/tui");
            let path = dir.join(format!("workspace-classic-{name}-{width}x{height}.txt"));
            if std::env::var("UPDATE_FIXTURES").is_ok() {
                std::fs::create_dir_all(&dir).expect("create fixture dir");
                std::fs::write(&path, &actual).expect("write fixture");
                continue;
            }
            let expected = read_snapshot(&path);
            assert_eq!(
                actual, expected,
                "snapshot mismatch for classic workspace {name} at {width}x{height}"
            );
        }
    }
}

fn workspace_content_app() -> App {
    use crate::core::content::{ContentEntry, ContentKind, ContentMeta};
    let mut app = workspace_app(crate::tui::pages::workspace::WorkspaceCategory::Mods);
    let entry =
        |name: &str, version: &str, size: u64, enabled: bool, warning: Option<&str>| ContentEntry {
            name: name.into(),
            version: version.into(),
            path: std::path::PathBuf::from(format!("mods/{name}.jar")),
            size,
            enabled,
            order: None,
            badge: None,
            warning: warning.map(str::to_owned),
            meta: ContentMeta::Mod {
                id: name.to_lowercase(),
                name: name.into(),
                version: version.into(),
                authors: Vec::new(),
                description: String::new(),
                depends: Vec::new(),
            },
        };
    if let Some(workspace) = app.workspace.as_mut() {
        workspace.content.set(
            ContentKind::Mod,
            vec![
                entry("OPack2Reload", "1.0.1", 2_400_000, true, None),
                entry("AI-Improvements", "0.5.2", 182_000, true, None),
                entry("LambDynamicLights", "4.4.0", 1_100_000, false, None),
                entry(
                    "TacticalEquip",
                    "0.4.3",
                    900_000,
                    true,
                    Some("OPack2Reload.jar"),
                ),
            ],
        );
        workspace.scan_stamps.insert(ContentKind::Mod, None);
    }
    app
}

#[test]
fn workspace_content_list_shows_marks_and_warnings() {
    let app = workspace_content_app();
    let rendered = render_text(&app, 120, 40);
    assert!(rendered.contains("[x]"), "{rendered}");
    assert!(rendered.contains("[ ]"), "{rendered}");
    assert!(rendered.contains("OPack2Reload"), "{rendered}");
    assert!(rendered.contains("2.3 MB"), "{rendered}");
    assert!(rendered.contains("duplicate of"), "{rendered}");
}

#[test]
fn workspace_shader_category_warns_when_no_loader_is_installed() {
    let mut app = workspace_app(crate::tui::pages::workspace::WorkspaceCategory::Shaders);
    if let Some(workspace) = app.workspace.as_mut() {
        workspace.shader_loader = None;
        workspace.content.set(
            crate::core::content::ContentKind::Shader,
            vec![crate::core::content::ContentEntry {
                name: "BSL".into(),
                version: String::new(),
                path: std::path::PathBuf::from("shaderpacks/BSL.zip"),
                size: 1024,
                enabled: false,
                order: None,
                badge: None,
                warning: None,
                meta: crate::core::content::ContentMeta::Shader {
                    description: String::new(),
                },
            }],
        );
    }
    let rendered = render_text(&app, 120, 40);
    // The files stay visible; the reason they do nothing sits above them.
    assert!(rendered.contains("No Iris or OptiFine"), "{rendered}");
    assert!(rendered.contains("BSL"), "{rendered}");

    let empty = workspace_app(crate::tui::pages::workspace::WorkspaceCategory::Shaders);
    let rendered = render_text(&empty, 120, 40);
    assert!(
        rendered.contains("Shaders need Iris or OptiFine"),
        "{rendered}"
    );
}

#[test]
fn workspace_world_rows_show_time_and_hardcore() {
    use crate::core::content::{ContentEntry, ContentKind, ContentMeta};
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_millis() as i64;
    let world = |name: &str, last_played: Option<i64>, hardcore: bool, parsed: bool| ContentEntry {
        name: name.into(),
        version: "1.20.1".into(),
        path: std::path::PathBuf::from(format!("saves/{name}")),
        size: 4096,
        enabled: true,
        order: None,
        badge: None,
        warning: None,
        meta: ContentMeta::World {
            level_name: name.into(),
            version: Some("1.20.1".into()),
            last_played,
            game_mode: Some("survival".into()),
            hardcore,
            parsed,
        },
    };
    let mut app = workspace_app(crate::tui::pages::workspace::WorkspaceCategory::Worlds);
    if let Some(workspace) = app.workspace.as_mut() {
        workspace.content.set(
            ContentKind::World,
            vec![
                world("Castle", Some(now - 2 * 3_600_000), true, true),
                world("Broken", None, false, false),
            ],
        );
    }
    let rendered = render_text(&app, 120, 40);
    assert!(rendered.contains("Hardcore"), "{rendered}");
    assert!(rendered.contains("2h ago"), "{rendered}");
    assert!(rendered.contains("level.dat unreadable"), "{rendered}");
}

#[test]
fn workspace_install_rows_reflect_the_client_json() {
    let root = std::env::temp_dir().join(format!("tmcl-install-render-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create instance dir");
    std::fs::write(
        root.join("demo.json"),
        r#"{"mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher",
            "libraries":[{"name":"net.minecraftforge:forge:1.20.1-47.4.22"}]}"#,
    )
    .expect("write client json");
    let mut app = workspace_app(crate::tui::pages::workspace::WorkspaceCategory::Install);
    if let Some(workspace) = app.workspace.as_mut() {
        workspace.root = root.clone();
        workspace.draft.name = "demo".into();
    }
    let rendered = render_text(&app, 120, 40);
    assert!(
        rendered.contains("1.20.1-47.4.22 · Installed"),
        "{rendered}"
    );
    assert!(rendered.contains("Not compatible with Forge"), "{rendered}");
    // Only OptiFine is read-only now; Quilt became installable.
    assert!(rendered.contains("download manually"), "{rendered}");
    std::fs::remove_dir_all(root).expect("clean instance");
}

// ── nightlight preset ─────────────────────────────────────────────

/// Nightlight app with a pinned clock: snapshots and the static-frame check
/// must not depend on the wall clock or on motion.
fn nightlight_app_at(page: Page, width: u16, height: u16) -> App {
    let mut app = App::new();
    app.page = page;
    app.config.state.ui.preset = crate::core::config::models::UiPreset::Nightlight;
    app.motion = false;
    app.terminal_width = width;
    app.terminal_height = height;
    app.nightlight_minute.set(Some(16 * 60));
    app
}

/// A workspace opened over a synthetic instance path, so the snapshot text is
/// stable across machines (nothing is read from disk).
fn nightlight_workspace_app() -> App {
    let mut app = nightlight_app_at(Page::Instances, 120, 40);
    let summary = InstanceSummary {
        id: Uuid::new_v4(),
        name: "TMCL-Test".to_owned(),
        version: "1.21.4".to_owned(),
        loader: "Fabric".to_owned(),
        loader_version: Some("0.16.9".to_owned()),
        status: "ready".to_owned(),
        path: PathBuf::from(r"D:/games/.minecraft/versions/TMCL-Test"),
        launch_count: 34,
        play_time_seconds: 12 * 3600,
    };
    app.instances.apply(vec![summary.clone()]);
    app.instances.set_current(Some(summary.id));
    app.workspace = Some(crate::tui::pages::workspace::Workspace::open(&summary));
    // `tmcl.json` does not exist for the synthetic path, so the draft falls
    // back to Vanilla; keep the snapshot coherent with the summary.
    if let Some(workspace) = app.workspace.as_mut() {
        workspace.draft.loader = crate::core::instance::models::InstanceLoader::Fabric {
            version: "0.16.9".to_owned(),
        };
    }
    app
}

fn nightlight_seed_overview(app: &mut App) {
    let summary = InstanceSummary {
        id: Uuid::new_v4(),
        name: "TMCL-Test".to_owned(),
        version: "1.21.4".to_owned(),
        loader: "Fabric".to_owned(),
        loader_version: Some("0.16.9".to_owned()),
        status: "ready".to_owned(),
        path: PathBuf::from(r"D:/games/.minecraft/versions/TMCL-Test"),
        launch_count: 34,
        play_time_seconds: 12 * 3600,
    };
    app.instances.apply(vec![summary.clone()]);
    app.instances.set_current(Some(summary.id));
    app.logs.append("[stdout] ready");
    app.launcher_logs.append("instance verified");
}

#[test]
fn nightlight_snapshots_match_fixtures() {
    use crate::core::config::models::ThemeMode;

    let cases = [
        ("overview", Page::Overview),
        ("instances", Page::Instances),
        ("download", Page::DownloadCenter),
        ("tasks", Page::Tasks),
        ("accounts", Page::Accounts),
        ("logs", Page::Logs),
        ("settings", Page::Settings),
    ];
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/tui");
    let write = |name: &str, actual: &str| {
        let path = dir.join(format!("nightlight-{name}.txt"));
        if std::env::var("UPDATE_FIXTURES").is_ok() {
            std::fs::create_dir_all(&dir).expect("create fixture dir");
            std::fs::write(&path, actual).expect("write fixture");
        } else {
            assert_eq!(
                read_snapshot(&path),
                actual,
                "snapshot mismatch for nightlight {name}"
            );
        }
    };

    for (name, page) in cases {
        for (width, height) in [(120u16, 40u16), (80, 24)] {
            let mut app = nightlight_app_at(page, width, height);
            if page == Page::Overview {
                nightlight_seed_overview(&mut app);
            }
            if page == Page::Logs {
                app.logs.append("[stdout] nightlight snapshot");
            }
            let actual = render_text(&app, width, height);
            write(&format!("{name}-{width}x{height}"), &actual);
        }
    }
    let workspace = nightlight_workspace_app();
    write("workspace-120x40", &render_text(&workspace, 120, 40));
    write(
        "workspace-80x24",
        &render_text(&nightlight_workspace_app(), 80, 24),
    );

    // Two light-mode references: the paper ladder is a separate design, not a
    // tint of the dark one.
    for (name, page) in [("overview", Page::Overview), ("settings", Page::Settings)] {
        let mut app = nightlight_app_at(page, 120, 40);
        super::set_theme_mode(&mut app, ThemeMode::Light);
        if page == Page::Overview {
            nightlight_seed_overview(&mut app);
        }
        write(&format!("{name}-120x40-light"), &render_text(&app, 120, 40));
    }
}

#[test]
fn nightlight_renders_every_page_at_every_size_in_both_modes() {
    use crate::core::config::models::{NightlightTheme, ThemeMode};
    use crate::tui::app::view::nightlight::tone;

    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        // The dark ladder is the reference colour: no cell of the light frame may
        // match a dark-ladder value.
        let dark = tone::palette(NightlightTheme::Live, ThemeMode::Dark, None, false);
        for width in [64u16, 80, 100, 120, 160] {
            for height in [18u16, 24, 30, 40, 50] {
                for page in Page::ALL {
                    let mut app = nightlight_app_at(page, width, height);
                    app.config.state.ui.theme_mode = mode;
                    if page == Page::Overview {
                        nightlight_seed_overview(&mut app);
                    }
                    let backend = TestBackend::new(width, height);
                    let mut terminal = Terminal::new(backend).expect("terminal");
                    terminal
                        .draw(|frame| app.draw(frame))
                        .unwrap_or_else(|error| {
                            panic!("{page:?} at {width}x{height} {mode:?}: {error}")
                        });
                    let buffer = terminal.backend().buffer();
                    assert_eq!(
                        (buffer.area.width, buffer.area.height),
                        (width, height),
                        "{page:?} changed the frame size"
                    );
                    if mode == ThemeMode::Light {
                        for cell in buffer.content.iter() {
                            assert_ne!(
                                cell.bg, dark.bg,
                                "{page:?} at {width}x{height} leaked the dark page colour"
                            );
                            // A hard-coded dark surface is just as wrong as a
                            // dark page: the light ladder never uses these.
                            assert!(
                                cell.bg != dark.surface
                                    && cell.bg != dark.raised
                                    && cell.bg != dark.selection,
                                "{page:?} at {width}x{height} painted a dark surface in light mode"
                            );
                        }
                    }
                    if width >= 64 && height >= 18 {
                        // Brand band: block wordmark on a wide column, the
                        // one-line ` moon TMCL ` on the six-column rail.
                        let column = app.regions.get().header.width;
                        let brand_rows = crate::tui::app::view::nightlight::sidebar::brand_rows(
                            Rect::new(0, 0, column, height),
                        );
                        let mut band = String::new();
                        for y in 1..=brand_rows {
                            for x in 0..column {
                                band.push_str(buffer[(x, y)].symbol());
                            }
                        }
                        assert!(
                            !band.trim().is_empty(),
                            "{page:?} at {width}x{height} lost the brand"
                        );
                        if column < 20 {
                            assert!(
                                band.starts_with("  ☾"),
                                "{page:?} at {width}x{height} lost the rail brand: {band:?}"
                            );
                        }
                    }
                    // The bottom of the content column must land on something,
                    // never on an empty band of page colour. Inside the page
                    // margin the last content row is `bottom - 2`; settings pins
                    // a one-line description there, every other page has to
                    // paint that row edge to edge.
                    let palette = app.nightlight_palette();
                    let regions = app.regions.get();
                    let last = regions.main.bottom().saturating_sub(2);
                    let left = regions.main.x.saturating_add(1);
                    let right = regions.main.right().saturating_sub(1);
                    if page == Page::Settings {
                        let described = (left..right).any(|x| buffer[(x, last)].symbol() != " ");
                        assert!(
                            described,
                            "{page:?} at {width}x{height} {mode:?} lost its description line"
                        );
                    } else {
                        let bare = (left..right)
                            .filter(|x| buffer[(*x, last)].bg == palette.bg)
                            .count();
                        assert_eq!(
                            bare, 0,
                            "{page:?} at {width}x{height} {mode:?} left {bare} page-coloured cells on the last content row"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn nightlight_geometry_keeps_the_sidebar_and_footer_in_place() {
    use crate::core::config::models::UiPreset;
    use crate::tui::app::view::nightlight::sidebar::{nav_index_at, nav_rects};

    for (width, height) in [(64u16, 18u16), (80, 24), (96, 30), (120, 40), (160, 50)] {
        let area = Rect::new(0, 0, width, height);
        let regions = crate::tui::shell::layout_chrome_for(area, UiPreset::Nightlight);
        assert!(
            regions.main.right() <= regions.body.right()
                && regions.main.bottom() <= regions.body.bottom(),
            "{width}x{height}: content escaped the body"
        );
        assert_eq!(
            regions.footer.x, 0,
            "{width}x{height}: footer must start at the left edge"
        );
        assert_eq!(
            regions.footer.width, width,
            "{width}x{height}: footer must span the frame"
        );
        assert_eq!(
            regions.footer.bottom(),
            height,
            "{width}x{height}: footer must sit on the last row"
        );
        assert_eq!(
            regions.main.y, 0,
            "{width}x{height}: content must start on the first row"
        );
        if regions.mode == crate::tui::shell::LayoutMode::TooSmall {
            continue;
        }
        assert!(
            regions.tabs.right() <= regions.main.x,
            "{width}x{height}: sidebar overlaps the content"
        );
        // Draw/hit geometry: every nav row maps back to its own index.
        let column = Rect::new(
            regions.header.x,
            regions.body.y,
            regions.header.width,
            regions.body.height,
        );
        let rects = nav_rects(column, 7);
        assert_eq!(rects.len(), 7, "{width}x{height}: lost nav rows");
        for (index, rect) in rects.iter().enumerate() {
            assert_eq!(
                nav_index_at(column, 7, rect.y, column.x),
                Some(index),
                "{width}x{height}: nav row {index} is not clickable"
            );
        }
    }
}

#[test]
fn nightlight_keeps_entities_out_of_the_sidebar() {
    let mut app = nightlight_app_at(Page::Overview, 120, 40);
    app.instances.apply(vec![InstanceSummary {
        id: Uuid::new_v4(),
        name: "TMCL-Test".to_owned(),
        version: "1.21.4".to_owned(),
        loader: "Fabric".to_owned(),
        loader_version: None,
        status: "ready".to_owned(),
        path: PathBuf::from(r"D:/games/.minecraft/versions/TMCL-Test"),
        launch_count: 1,
        play_time_seconds: 60,
    }]);
    if let Some(id) = app.instances.items.first().map(|item| item.id) {
        app.instances.set_current(Some(id));
    }
    let rendered = render_text(&app, 120, 40);
    // Every sidebar row is registered as a nav target inside the sidebar column.
    let nav_hits = app
        .hitmap
        .hitboxes()
        .iter()
        .filter(|hitbox| matches!(hitbox.action, crate::tui::action::Action::Nav(_)))
        .filter(|hitbox| hitbox.rect.x < 27)
        .count();
    assert_eq!(nav_hits, 7, "sidebar registered {nav_hits} clickable rows");
    for name in ["TMCL-Test"] {
        for line in rendered.lines() {
            let Some(column) = line.find(name) else {
                continue;
            };
            assert!(
                column >= 27,
                "entity {name:?} leaked into the sidebar column {column}: {line:?}"
            );
        }
    }
}

#[test]
fn nightlight_frames_are_static_within_a_minute() {
    use crate::core::config::models::UiPreset;

    // Every page, not just the overview: a task bar or a clock chip that
    // animates would only show up on the page that draws it.
    for page in Page::ALL {
        let mut app = nightlight_app_at(page, 120, 40);
        if page == Page::Overview {
            nightlight_seed_overview(&mut app);
        }
        app.motion = true;
        assert_eq!(app.preset(), UiPreset::Nightlight);
        assert_eq!(app.refresh_millis(), 250);
        let first = render_text(&app, 120, 40);
        let second = render_text(&app, 120, 40);
        assert_eq!(first, second, "{page:?} must not animate between frames");
    }
}

/// A hit box must cover exactly the pixels the card was painted on. The two are
/// computed by different code paths, and when they disagree the row under the
/// pointer lights up instead of the row under the cursor.
#[test]
fn nightlight_row_hit_boxes_cover_exactly_the_painted_card() {
    use crate::tui::action::{Action, PageAct};

    let mut app = nightlight_app_at(Page::Instances, 120, 40);
    let items = (0..3)
        .map(|index| InstanceSummary {
            id: Uuid::new_v4(),
            name: format!("TMCL-{index}"),
            version: "1.21.4".to_owned(),
            loader: "Fabric".to_owned(),
            loader_version: None,
            status: "ready".to_owned(),
            path: PathBuf::from(r"D:/games/.minecraft/versions/TMCL"),
            launch_count: index as u64,
            play_time_seconds: 60,
        })
        .collect::<Vec<_>>();
    app.instances.apply(items);
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.draw(frame)).expect("draw");
    let buffer = terminal.backend().buffer();
    let pal = app.nightlight_palette();

    let mut checked = 0;
    for hitbox in app.hitmap.hitboxes() {
        if !matches!(
            hitbox.action,
            Action::Page {
                page: Page::Instances,
                act: PageAct::Row(_)
            }
        ) {
            continue;
        }
        for row in hitbox.rect.y..hitbox.rect.bottom() {
            let cell = &buffer[(hitbox.rect.x, row)];
            assert!(
                cell.bg == pal.surface || cell.bg == pal.selection,
                "hit box row {} starts on a gap, not on the card: bg={:?}",
                row,
                cell.bg
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 3, "every instance card must be clickable");
}

/// Every scrolling page must answer a wheel notch over its body. Registering
/// row hit boxes without a scroll target leaves the list deaf to the wheel,
/// which is invisible in a snapshot.
#[test]
fn nightlight_scrolling_pages_answer_the_wheel() {
    use crate::tui::hit::Layer;

    for page in [
        Page::Instances,
        Page::DownloadCenter,
        Page::Tasks,
        Page::Accounts,
        Page::Logs,
        Page::Settings,
    ] {
        let mut app = nightlight_app_at(page, 120, 40);
        if page == Page::DownloadCenter {
            // A version list exists without any network call.
            app.download_center.category =
                crate::tui::pages::download_center::DownloadCategory::GameVersions;
        }
        let _ = render_text(&app, 120, 40);
        let regions = app.regions.get();
        let column = regions.main.x + regions.main.width / 2;
        let row = regions.main.y + regions.main.height / 2;
        assert!(
            app.hitmap.scroll_at(column, row, Layer::Base).is_some(),
            "{page:?} swallows the wheel at its centre"
        );
    }
}

/// The workspace rows come from a shared model, so every row that model
/// produces must reach the screen. Hand-writing a shorter list on one side is
/// exactly how the nightlight workspace lost launch counts, play time, size,
/// content summary and launch settings while every snapshot stayed green.
#[test]
fn nightlight_workspace_renders_every_shared_row() {
    use crate::tui::pages::workspace::WorkspaceCategory;

    let mut app = nightlight_workspace_app();
    for category in WorkspaceCategory::ALL {
        if let Some(workspace) = app.workspace.as_mut() {
            workspace.category = category;
        }
        let expected = app.workspace_display_rows();
        if expected.is_empty() {
            continue;
        }
        let text = render_text(&app, 120, 40);
        for (label, _) in expected {
            assert!(
                text.contains(&label),
                "{category:?} dropped the row {label:?}"
            );
        }
    }
}

/// `Java runtimes` and `Game directories` are tables, not setting rows. Falling
/// through to the generic row list renders an empty card, which reads as "the
/// feature is missing".
#[test]
fn nightlight_settings_renders_the_two_table_categories() {
    use crate::tui::pages::settings::SettingsCategory;

    let mut app = nightlight_app_at(Page::Settings, 120, 40);
    app.settings.select_category(SettingsCategory::Java);
    let java = render_text(&app, 120, 40);
    assert!(
        java.contains(&app.i18n.text("java.runtime")),
        "the java card has no title"
    );
    // The empty message is a sentence and may be clipped at the card edge, so
    // compare on its head rather than on the whole string.
    let empty = app.i18n.text("java.empty");
    let head = empty.chars().take(8).collect::<String>();
    assert!(
        java.contains(&head) || !app.java.is_empty(),
        "the java card renders neither runtimes nor the empty state"
    );

    app.settings
        .select_category(SettingsCategory::GameDirectories);
    let dirs = render_text(&app, 120, 40);
    assert!(
        dirs.contains(&app.i18n.text("settings.add_game_directory")),
        "the directories card lost its add action"
    );
}

fn mod_entry(name: &str) -> crate::core::content::ContentEntry {
    use crate::core::content::{ContentEntry, ContentMeta};
    ContentEntry {
        name: name.into(),
        version: "1.2.3".into(),
        path: PathBuf::from(format!("{name}.jar")),
        size: 1024 * 1024,
        enabled: true,
        order: None,
        badge: None,
        warning: None,
        meta: ContentMeta::Mod {
            id: name.into(),
            name: name.into(),
            version: "1.2.3".into(),
            authors: Vec::new(),
            description: String::new(),
            depends: Vec::new(),
        },
    }
}

/// A content list carries more than a name: version, size and the enable mark,
/// which is also a click target of its own.
#[test]
fn nightlight_workspace_content_rows_carry_the_metadata() {
    use crate::core::content::ContentKind;
    use crate::tui::action::Action;
    use crate::tui::pages::workspace::WorkspaceCategory;

    let mut app = nightlight_workspace_app();
    {
        let workspace = app.workspace.as_mut().expect("workspace");
        workspace.category = WorkspaceCategory::Mods;
        workspace
            .content
            .set(ContentKind::Mod, vec![mod_entry("alpha")]);
    }
    let text = render_text(&app, 120, 40);
    assert!(text.contains("alpha"), "the row lost its name");
    assert!(text.contains("1.2.3"), "the row lost its version");
    assert!(text.contains("1.0 MB"), "the row lost its size");
    assert!(
        app.hitmap.hitboxes().iter().any(|hitbox| matches!(
            hitbox.action,
            Action::Page {
                page: Page::Instances,
                act: crate::tui::action::PageAct::WorkspaceToggle(0),
            }
        )),
        "the enable mark is not clickable"
    );
}

/// A shader list without a loader says why above itself; the files stay
/// visible, because hiding them reads as a failed scan.
#[test]
fn nightlight_shader_list_warns_about_the_missing_loader() {
    use crate::core::content::ContentKind;
    use crate::tui::pages::workspace::WorkspaceCategory;

    let mut app = nightlight_workspace_app();
    {
        let workspace = app.workspace.as_mut().expect("workspace");
        workspace.category = WorkspaceCategory::Shaders;
        workspace
            .content
            .set(ContentKind::Shader, vec![mod_entry("soft-shadows")]);
    }
    let banner = app.i18n.text("workspace.shaders.no_loader_banner");
    let head = banner.chars().take(8).collect::<String>();
    let text = render_text(&app, 120, 40);
    assert!(text.contains(&head), "the missing-loader banner is gone");
    assert!(
        text.contains("soft-shadows"),
        "the banner must not replace the file list"
    );
}

/// The second level of the resource flow: a project's files, each with its
/// compatibility. Rendering only the project index means Enter looks like it
/// jumped straight to an install dialog.
#[test]
fn nightlight_resource_files_page_shows_compatibility() {
    use crate::core::resource::model::{ReleaseChannel, ResourceFile, ResourceSource};
    use crate::tui::pages::download_center::DownloadCategory;

    let mut app = nightlight_app_at(Page::DownloadCenter, 120, 40);
    app.download_center.category =
        DownloadCategory::Resource(crate::core::resource::model::ResourceKind::Mod);
    app.resource_detail = Some("alpha".to_owned());
    app.resource_files = vec![ResourceFile {
        source: ResourceSource::Modrinth,
        project_id: "alpha".to_owned(),
        file_id: "v1".to_owned(),
        name: "alpha-1.0.jar".to_owned(),
        download_url: "https://example.invalid/alpha.jar".to_owned(),
        sha1: None,
        size: Some(1024),
        game_versions: vec!["1.20.1".to_owned()],
        loaders: vec!["fabric".to_owned()],
        channel: ReleaseChannel::Release,
        dependencies: Vec::new(),
    }];
    let text = render_text(&app, 120, 40);
    assert!(
        text.contains(&app.i18n.text("download.compatibility")),
        "no compatibility column"
    );
    assert!(
        text.contains(&app.i18n.text("download.dependencies")),
        "no dependency column"
    );
    assert!(text.contains("alpha-1.0.jar"), "the file row is missing");
    assert!(
        app.hitmap.hitboxes().iter().any(|hitbox| matches!(
            hitbox.action,
            crate::tui::action::Action::Page {
                page: Page::DownloadCenter,
                act: crate::tui::action::PageAct::Row(0),
            }
        )),
        "the file row is not clickable"
    );
}

/// A category without a target instance must not pretend it has one. A modpack
/// builds an instance of its own and a world is saved to a path the player
/// picks, so a "target", a compatibility badge against it and a dependency
/// count all describe an install that never happens.
#[test]
fn downloads_without_a_target_show_neither_target_nor_compatibility() {
    use crate::core::config::models::UiPreset;
    use crate::core::resource::model::{
        ReleaseChannel, ResourceFile, ResourceKind, ResourceSource,
    };
    use crate::tui::pages::download_center::DownloadCategory;

    let file = ResourceFile {
        source: ResourceSource::Modrinth,
        project_id: "pack".to_owned(),
        file_id: "v1".to_owned(),
        name: "pack-1.0.mrpack".to_owned(),
        download_url: "https://example.invalid/pack.mrpack".to_owned(),
        sha1: None,
        size: Some(4096),
        game_versions: vec!["1.20.1".to_owned()],
        loaders: vec!["fabric".to_owned()],
        channel: ReleaseChannel::Release,
        dependencies: Vec::new(),
    };

    for (kind, summary_key) in [
        (ResourceKind::Modpack, "download.creates_instance"),
        (ResourceKind::World, "download.saves_to_path"),
    ] {
        for preset in [UiPreset::Classic, UiPreset::Nightlight] {
            let mut app = App::new();
            app.page = Page::DownloadCenter;
            app.config.state.ui.preset = preset;
            app.motion = false;
            app.terminal_width = 120;
            app.terminal_height = 40;
            app.nightlight_minute.set(Some(16 * 60));
            app.download_center.category = DownloadCategory::Resource(kind);
            app.download_center.total = 1;
            app.resource_detail = Some("pack".to_owned());
            app.resource_files = vec![file.clone()];

            let text = render_text(&app, 120, 40);
            for key in ["download.compatibility", "download.dependencies"] {
                let caption = app.i18n.text(key);
                assert!(
                    !text.contains(&caption),
                    "{preset:?} still shows {caption} for {kind:?}:\n{text}"
                );
            }
            assert!(
                !text.contains(&app.i18n.text("action.target")),
                "{preset:?} still names a target instance for {kind:?}:\n{text}"
            );
            assert!(
                text.contains("pack-1.0.mrpack"),
                "{preset:?} lost the file row for {kind:?}:\n{text}"
            );
            // `classic` puts the note in the page caption; `nightlight`
            // keeps it in the one-line summary above the results.
            let summary = app.i18n.text(summary_key);
            let info = app.i18n.text(app.download_info_key());
            let tail = info
                .rsplit('\u{00b7}')
                .next()
                .unwrap_or_default()
                .trim()
                .to_owned();
            assert!(
                text.contains(&summary) || text.contains(&tail),
                "{preset:?} does not say where a {kind:?} download goes:\n{text}"
            );

            // The narrow layouts fold the table into compact rows; drawing them
            // must not rely on the columns this page removed.
            let _ = render_text(&app, 80, 24);
        }
    }
}

/// The `Manage` list is a menu, not a selection: one click runs the row. The
/// destructive entries still route through their confirmation.
#[test]
fn nightlight_manage_rows_run_on_a_single_click() {
    use crate::tui::action::{Action, Click, PageAct};
    use crate::tui::pages::workspace::{Workspace, WorkspaceCategory};

    let mut app = nightlight_app_at(Page::Instances, 120, 40);
    let summary = InstanceSummary {
        id: Uuid::new_v4(),
        name: "TMCL-Test".to_owned(),
        version: "1.21.4".to_owned(),
        loader: "Fabric".to_owned(),
        loader_version: None,
        status: "ready".to_owned(),
        path: PathBuf::from(r"D:/games/.minecraft/versions/TMCL-Test"),
        launch_count: 0,
        play_time_seconds: 0,
    };
    app.instances.apply(vec![summary.clone()]);
    app.instances.set_current(Some(summary.id));
    let mut workspace = Workspace::open(&summary);
    workspace.category = WorkspaceCategory::Manage;
    app.workspace = Some(workspace);

    let _ = render_text(&app, 120, 40);
    let runs = app
        .hitmap
        .hitboxes()
        .into_iter()
        .filter(|hitbox| {
            matches!(
                hitbox.action,
                Action::Page {
                    act: PageAct::WorkspaceRun(_),
                    ..
                }
            )
        })
        .count();
    assert_eq!(runs, 5, "the manage menu registered {runs} clickable rows");

    // Row 4 is "delete instance": a click must open its confirmation, not
    // remove anything.
    app.apply_action(
        Action::Page {
            page: Page::Instances,
            act: PageAct::WorkspaceRun(4),
        },
        Click::Single,
    );
    assert!(
        app.modal.is_some(),
        "deleting an instance without a confirmation"
    );
}

/// The second level is the file table. No preset reserves a column beside it:
/// the table already carries compatibility, channel and dependency count, and
/// a card next to it repeated the selected row for a third of the width.
#[test]
fn the_file_list_is_never_split_by_a_detail_column() {
    use crate::core::config::models::UiPreset;
    use crate::core::resource::model::{
        ReleaseChannel, ResourceFile, ResourceKind, ResourceSource,
    };
    use crate::tui::pages::download_center::DownloadCategory;

    for preset in UiPreset::ALL {
        for width in [120u16, 160] {
            let mut app = nightlight_app_at(Page::DownloadCenter, width, 40);
            app.config.state.ui.preset = preset;
            app.download_center.category = DownloadCategory::Resource(ResourceKind::Mod);
            app.resource_detail = Some("farming-tales".to_owned());
            app.resource_files = vec![ResourceFile {
                source: ResourceSource::Modrinth,
                project_id: "farming-tales".to_owned(),
                file_id: "v1".to_owned(),
                name: "farmingtales-1.4.3.jar".to_owned(),
                download_url: "https://example.invalid/a.jar".to_owned(),
                sha1: None,
                size: Some(1024),
                game_versions: vec!["1.20.1".to_owned()],
                loaders: vec!["forge".to_owned()],
                channel: ReleaseChannel::Release,
                dependencies: Vec::new(),
            }];
            let main = app.layout_chrome(Rect::new(0, 0, width, 40)).main;
            // Each preset measures its own list body. Asking one preset's section
            // helper for another's rectangle compares two different rows.
            let list = app.download_list_area(main);
            assert!(
                app.download_detail_areas(list).is_none(),
                "{preset:?} splits the file list at {width} columns"
            );
            let text = render_text(&app, width, 40);
            assert!(
                text.contains("farmingtales-1.4.3.jar"),
                "{preset:?} lost the file row"
            );
            assert!(
                text.contains(&app.i18n.text("download.dependencies")),
                "{preset:?} lost the dependency column"
            );
        }
    }
}

/// A scan in flight must not blank the list: the entries are already cached, and
/// replacing them with "scanning" makes every refresh look like a freeze.
#[test]
fn nightlight_keeps_the_content_visible_while_rescanning() {
    use crate::core::content::ContentKind;
    use crate::tui::pages::workspace::WorkspaceCategory;

    let mut app = nightlight_workspace_app();
    {
        let workspace = app.workspace.as_mut().expect("workspace");
        workspace.category = WorkspaceCategory::Mods;
        workspace
            .content
            .set(ContentKind::Mod, vec![mod_entry("alpha")]);
        workspace.loading = true;
    }
    let text = render_text(&app, 120, 40);
    assert!(text.contains("alpha"), "the cached row disappeared");
    assert!(
        !text.contains(&app.i18n.text("workspace.scanning")),
        "the list was replaced by the scanning message"
    );

    // With nothing cached the message is still the right answer.
    let mut empty = nightlight_workspace_app();
    {
        let workspace = empty.workspace.as_mut().expect("workspace");
        workspace.category = WorkspaceCategory::Mods;
        workspace.loading = true;
    }
    let text = render_text(&empty, 120, 40);
    assert!(
        text.contains(&empty.i18n.text("workspace.scanning")),
        "an empty list must say that it is scanning"
    );
}

/// The description column answers the wheel: the clamp lives on `App`, and a pane
/// that never sets it receives no scroll input.
#[test]
fn nightlight_detail_pane_answers_the_wheel() {
    use crate::core::resource::model::{ResourceProject, ResourceSource};
    use crate::tui::action::ScrollTarget;
    use crate::tui::hit::Layer;
    use crate::tui::pages::download_center::DownloadCategory;

    let mut app = nightlight_app_at(Page::DownloadCenter, 120, 40);
    app.download_center.category =
        DownloadCategory::Resource(crate::core::resource::model::ResourceKind::Mod);
    app.resource_projects = vec![ResourceProject {
        source: ResourceSource::Modrinth,
        project_id: "NNAgCjsB".to_owned(),
        title: "Entity Culling".to_owned(),
        description: "A description long enough to need the room.".to_owned(),
        author: "tr7zw".to_owned(),
        downloads: Some(1),
        categories: vec!["optimization".to_owned()],
        game_versions: vec!["1.20.1".to_owned()],
        loaders: vec!["fabric".to_owned()],
        icon_url: None,
        updated_at: None,
    }];
    let _ = render_text(&app, 120, 40);
    let regions = app.regions.get();
    let column = regions.main.right().saturating_sub(10);
    let row = regions.main.y + regions.main.height / 2;
    assert_eq!(
        app.hitmap.scroll_at(column, row, Layer::Base),
        Some(ScrollTarget::Pane),
        "the description column does not take the wheel"
    );
}

/// The launch settings are adjustable in place: the arrows are drawn and
/// registered from one calculation, so a row that shows `‹ ›` is a row that
/// responds to them.
#[test]
fn nightlight_launch_rows_show_and_register_arrows() {
    use crate::tui::action::{Action, PageAct};
    use crate::tui::pages::workspace::WorkspaceCategory;

    let mut app = nightlight_workspace_app();
    if let Some(workspace) = app.workspace.as_mut() {
        workspace.category = WorkspaceCategory::Launch;
    }
    let text = render_text(&app, 120, 40);
    assert!(
        text.contains('\u{2039}') && text.contains('\u{203a}'),
        "no adjustable row drew its arrows"
    );
    let steps = app
        .hitmap
        .hitboxes()
        .into_iter()
        .filter(|hitbox| {
            matches!(
                hitbox.action,
                Action::Page {
                    act: PageAct::WorkspaceStep { .. },
                    ..
                }
            )
        })
        .count();
    assert!(steps >= 2, "the arrows are drawn but not clickable");
}

/// The hero row must not say the same account twice: at 120 columns the
/// instance card is ~55 cells wide, but the account card is still on screen.
#[test]
fn the_overview_instance_card_keeps_the_account_out() {
    use crate::core::account::models::{Account, AccountKind};

    let mut app = nightlight_app_at(Page::Overview, 120, 40);
    nightlight_seed_overview(&mut app);
    let account = Account {
        id: Uuid::new_v4(),
        kind: AccountKind::Offline,
        username: "Steve".to_owned(),
        uuid: Uuid::new_v4(),
        skin: None,
        secret_ref: None,
        access_expires_at: None,
    };
    app.accounts.current = Some(account.id);
    app.accounts.accounts = vec![account];

    let text = render_text(&app, 120, 40);
    assert_eq!(
        text.matches("Steve").count(),
        1,
        "the account must appear once, in its own card:\n{text}"
    );
}

/// An empty index is `0/0`, never `1/0`: the position cannot be past the end.
#[test]
fn an_empty_resource_index_counts_from_zero() {
    use crate::tui::pages::download_center::DownloadCategory;

    let mut app = nightlight_app_at(Page::DownloadCenter, 120, 40);
    app.download_center.category =
        DownloadCategory::Resource(crate::core::resource::model::ResourceKind::Mod);
    app.resource_projects.clear();
    app.resource_detail = None;
    let text = render_text(&app, 120, 40);
    assert!(
        !text.contains("1/0"),
        "an empty list cannot sit at position 1:\n{text}"
    );
    assert!(text.contains("0/0"), "the counter must read 0/0:\n{text}");
    assert!(
        text.contains(&app.i18n.text("download.no_results")),
        "an empty index must explain itself:\n{text}"
    );
}

/// The version table had no empty state at all: a filter that matches nothing
/// left the card showing a header over blank rows.
#[test]
fn an_empty_version_table_says_so() {
    use crate::tui::pages::download_center::DownloadCategory;

    let mut app = nightlight_app_at(Page::DownloadCenter, 120, 40);
    app.download_center.category = DownloadCategory::GameVersions;
    app.versions.clear();
    let text = render_text(&app, 120, 40);
    assert!(
        text.contains(&app.i18n.text("download.no_results")),
        "an empty version table needs a message:\n{text}"
    );
}

/// The current account carries an extra `●` marker; the row still has to end
/// with the whole badge instead of pushing two cells past the card edge.
#[test]
fn the_current_account_keeps_its_whole_badge() {
    use crate::core::account::models::{Account, AccountKind};

    let mut app = nightlight_app_at(Page::Accounts, 120, 40);
    let account = Account {
        id: Uuid::new_v4(),
        kind: AccountKind::Offline,
        username: "Steve".to_owned(),
        uuid: Uuid::new_v4(),
        skin: None,
        secret_ref: None,
        access_expires_at: None,
    };
    app.accounts.current = Some(account.id);
    app.accounts.accounts = vec![account];

    let text = render_text(&app, 120, 40);
    let row = text
        .lines()
        .find(|line| line.contains("Steve"))
        .expect("the account row");
    assert!(
        row.trim_end()
            .ends_with(&app.i18n.text("account.kind.offline")),
        "the badge is clipped: {row:?}"
    );
}

/// `Space` marks the current instance, and the footer has to say so.
///
/// It sat last in the list with the highest drop priority, so at 120 columns
/// every preset but `paper` had already dropped it: the action existed, `?`
/// described it, and nothing on the page hinted it was there.
#[test]
fn the_instances_footer_offers_space_in_every_preset() {
    use crate::core::config::models::UiPreset;

    for preset in UiPreset::ALL {
        let mut app = shelf_data_app(Page::Instances);
        app.config.state.ui.preset = preset;
        app.terminal_width = 120;
        app.terminal_height = 40;

        let text = render_text(&app, 120, 40);
        assert!(
            text.contains("Space") && text.contains("Set current"),
            "{preset:?}: the footer never mentions Space:\n{text}"
        );
    }
}

/// No footer may drop the way out.
///
/// `paper` built its hint line by stopping at the page edge, so a narrow window
/// simply ended the sentence: `? Help`, `Tab Next page` and `q Quit` were all
/// missing, with nothing on screen to say the keys existed. Every footer now
/// drops by priority instead, and `q` is never the one dropped.
#[test]
fn no_footer_drops_help_tab_or_quit() {
    use crate::core::config::models::UiPreset;

    for width in [120u16, 100, 80] {
        for preset in UiPreset::ALL {
            let mut app = shelf_data_app(Page::Instances);
            app.config.state.ui.preset = preset;
            app.terminal_width = width;
            app.terminal_height = 40;

            let text = render_text(&app, width, 40);
            for label in ["Help", "Next page", "Quit"] {
                assert!(
                    text.contains(label),
                    "{preset:?} at {width}: the footer has no {label}:\n{text}"
                );
            }
        }
    }
}

/// No glyph falls below the contrast floor on either painted page.
///
/// `深色` and `浅色` used to paint nothing and only swap the inks; on the
/// wrong window that put dark text on a dark page. Both lay down their own
/// page now, and this reads every painted cell back off the frame to prove the
/// inks land on it. 4.5:1 is the WCAG floor for body text.
#[test]
fn no_classic_glyph_falls_below_the_contrast_floor() {
    use crate::core::config::models::{ThemeMode, UiPreset};
    use crate::tui::palette::relative_luminance;
    use ratatui::style::Color;

    fn ratio(a: Color, b: Color) -> f64 {
        let (a, b) = (relative_luminance(a), relative_luminance(b));
        let (light, dark) = if a > b { (a, b) } else { (b, a) };
        (light + 0.05) / (dark + 0.05)
    }

    let pages = [
        Page::Overview,
        Page::Instances,
        Page::Accounts,
        Page::DownloadCenter,
        Page::Tasks,
        Page::Logs,
        Page::Settings,
    ];
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        for page in pages {
            let mut app = shelf_data_app(page);
            app.config.state.ui.preset = UiPreset::Classic;
            app.config.state.ui.theme_mode = mode;
            app.theme = crate::tui::theme::Theme::from_ui(&app.config.state.ui);
            app.terminal_width = 120;
            app.terminal_height = 35;

            let buffer = render_buffer(&app, 120, 35);
            for y in 0..35u16 {
                for x in 0..120u16 {
                    let cell = &buffer[(x, y)];
                    if cell.symbol().trim().is_empty() {
                        continue;
                    }
                    let against = ratio(cell.fg, cell.bg);
                    assert!(
                        against >= 4.5,
                        "{mode:?}/{page:?}: {:?} at ({x},{y}) is {against:.2}:1 on {:?}",
                        cell.symbol(),
                        cell.bg
                    );
                }
            }
        }
    }
}

/// A dialog pushes the page back without inverting it.
///
/// The scrim scaled every channel by 0.55, which is how a dark page recedes:
/// the ink and the paper both lose brightness and stay in ratio. On cream the
/// same arithmetic lands on mid grey with near black text on top — 2.6:1, and
/// the window looks like the lights went out. A paper page keeps its colour and
/// only loses its ink, and the dialog drawn over it is crisp.
#[test]
fn a_dialog_pushes_the_page_back_without_inverting_it() {
    use crate::core::config::models::{ThemeMode, UiPreset};
    use crate::tui::modal::{ConfirmAction, Modal};
    use crate::tui::palette::relative_luminance;
    use ratatui::style::Color;

    fn ratio(a: Color, b: Color) -> f64 {
        let (a, b) = (relative_luminance(a), relative_luminance(b));
        let (light, dark) = if a > b { (a, b) } else { (b, a) };
        (light + 0.05) / (dark + 0.05)
    }

    let area = Rect::new(0, 0, 120, 35);
    let prompt = "Clear the download cache?";
    let card = Modal::confirm(prompt, ConfirmAction::ClearDownloadCache).area(area);

    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let mut app = shelf_data_app(Page::Instances);
        app.config.state.ui.preset = UiPreset::Classic;
        app.config.state.ui.theme_mode = mode;
        app.theme = crate::tui::theme::Theme::from_ui(&app.config.state.ui);
        app.terminal_width = 120;
        app.terminal_height = 35;
        app.modal = Some(Modal::confirm(prompt, ConfirmAction::ClearDownloadCache));

        let buffer = render_buffer(&app, 120, 35);
        let mut behind = 0;
        for y in 0..35u16 {
            for x in 0..120u16 {
                let cell = &buffer[(x, y)];
                let inside = card.contains(ratatui::layout::Position::new(x, y));
                if inside {
                    if cell.symbol().trim().is_empty() {
                        continue;
                    }
                    let against = ratio(cell.fg, cell.bg);
                    assert!(
                        against >= 4.5,
                        "{mode:?} dialog: {:?} at ({x},{y}) is {against:.2}:1",
                        cell.symbol()
                    );
                    continue;
                }
                // Behind the scrim the page keeps its own side of the scale:
                // light stays light, dark stays dark. Never a mid-grey slab.
                let page = relative_luminance(cell.bg);
                match mode {
                    ThemeMode::Light => assert!(
                        page > 0.5,
                        "the light page went dark behind the dialog at ({x},{y}): {:?}",
                        cell.bg
                    ),
                    _ => assert!(
                        page < 0.05,
                        "the dark page went pale behind the dialog at ({x},{y}): {:?}",
                        cell.bg
                    ),
                }
                behind += 1;
            }
        }
        assert!(
            behind > 1000,
            "{mode:?}: the page was never drawn: {behind}"
        );
    }
}

/// The two sheet presets wrap a long log line instead of cutting it.
///
/// One entry per row with the body truncated at the page edge turns every real
/// Forge line into "…ModLauncher running: args [--username, 233, --version, F…".
/// Half a mod path or half a stack frame is not a log.
#[test]
fn the_sheet_logs_wrap_a_long_line_instead_of_cutting_it() {
    use crate::core::config::models::UiPreset;

    let tail = "END-OF-A-VERY-LONG-MOD-PATH";
    let head = "[stdout] [15:39:08] [main/INFO] [cp.mo.mo.Launcher/MODLAUNCHER]: ";
    for preset in [UiPreset::Paper, UiPreset::Shelf] {
        let mut app = App::new();
        app.config.state.ui.preset = preset;
        app.page = Page::Logs;
        app.terminal_width = 120;
        app.terminal_height = 40;
        app.logs
            .append(format!("{head}{}{tail}", "padding ".repeat(20)));

        let text = render_text(&app, 120, 40);
        assert!(
            text.contains(tail),
            "{preset:?}: the line was cut instead of wrapped:\n{text}"
        );
        assert!(
            text.contains("MODLAUNCHER"),
            "{preset:?}: the line is not on the page at all:\n{text}"
        );
    }
}

/// A one-row scroll moves the page by exactly one row, including into the middle
/// of a wrapped line.
///
/// The viewport hands the painter a slice plus "skip this many rows of the first
/// line". Ignoring the skip, or letting the row arithmetic drift from the
/// wrapper, makes the page jump a whole line and skip content.
#[test]
fn a_one_row_scroll_moves_the_page_by_exactly_one_row() {
    let mut app = App::new();
    app.page = Page::Logs;
    app.terminal_width = 120;
    app.terminal_height = 40;
    for index in 0..40 {
        // Long enough to wrap three times at this width.
        app.logs
            .append(format!("line {index:02} {}", "body ".repeat(40)));
    }
    let main = app.layout_chrome(Rect::new(0, 0, 120, 40)).main;
    let inner = crate::tui::chrome::panel(&app.i18n.text(Page::Logs.title_key()), false, app.theme)
        .inner(main);

    let before = log_body_rows(&app, inner);
    let max = app.log_max_scroll_offset();
    assert!(max > 3, "the buffer must overflow the page: {max}");
    app.logs.scroll(-1, max);
    let after = log_body_rows(&app, inner);

    assert!(before[0] != after[0], "the page did not move");
    // Scrolling up one row shows one older row: everything after the first row
    // of the new frame is what the old frame drew from its second row on.
    assert_eq!(
        &after[1..],
        &before[..before.len() - 1],
        "the page must shift by one row, not by one line"
    );
}

fn log_body_rows(app: &crate::tui::app::App, inner: Rect) -> Vec<String> {
    let buffer = render_buffer(app, 120, 40);
    let width = inner.width.saturating_sub(1);
    (inner.top()..inner.bottom())
        .map(|y| {
            (inner.left()..inner.left() + width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

/// The wheel clamp must describe the same rectangle the painter fills. The
/// shared clamp is four rows taller than the nightlight body, which made the
/// top of the buffer unreachable.
#[test]
fn the_log_page_reaches_both_ends_of_the_buffer() {
    let mut app = nightlight_app_at(Page::Logs, 120, 40);
    for index in 0..200 {
        app.logs.append(format!("line-{index:03}"));
    }
    let max = app.log_max_scroll_offset();
    app.logs.set_follow(false, max);
    app.logs.scroll(-(max as isize), max);
    let top = render_text(&app, 120, 40);
    assert!(
        top.contains("line-000"),
        "the first line must be reachable:\n{top}"
    );

    app.logs.set_follow(true, app.log_max_scroll_offset());
    let bottom = render_text(&app, 120, 40);
    assert!(
        bottom.contains("line-199"),
        "the last line must be visible when following:\n{bottom}"
    );
}

/// A status message must not reflow the page. A footer that grows to two rows for
/// a long message shrinks every card by a row.
#[test]
fn a_long_status_never_changes_the_page_height() {
    let mut app = nightlight_app_at(Page::Overview, 120, 40);
    let short = app
        .layout_chrome(ratatui::layout::Rect::new(0, 0, 120, 40))
        .main
        .height;
    app.status = crate::tui::app::Status::warning(
        "Failed to export the launcher log bundle: access denied while writing \
         C:/Users/demo/AppData/Roaming/TMCL/logs/launcher.log",
    );
    let long = app
        .layout_chrome(ratatui::layout::Rect::new(0, 0, 120, 40))
        .main
        .height;
    assert_eq!(short, long, "a status message resized the page");

    let text = render_text(&app, 120, 40);
    assert!(
        text.contains("Next page"),
        "the key hints must survive a long message:\n{text}"
    );
    assert!(
        text.contains('…'),
        "the message itself must be truncated, not clipped:\n{text}"
    );
}

/// The logs card says whose log it is, like the other two presets do.
#[test]
fn the_logs_card_names_the_instance() {
    let mut app = nightlight_app_at(Page::Logs, 120, 40);
    nightlight_seed_overview(&mut app);
    let text = render_text(&app, 120, 40);
    let title = text
        .lines()
        .find(|line| line.contains('●') || line.contains("Logs"))
        .unwrap_or_default();
    assert!(
        text.contains("Logs · TMCL-Test"),
        "the card title must name the instance, got {title:?}:\n{text}"
    );
}

/// 80x24 keeps playtime and launches. The big glyphs need six rows, so a short
/// hero falls back to one text line instead of dropping them.
#[test]
fn the_narrow_overview_keeps_both_numbers() {
    let mut app = nightlight_app_at(Page::Overview, 80, 24);
    nightlight_seed_overview(&mut app);
    let text = render_text(&app, 80, 24);
    assert!(text.contains("12h"), "playtime missing:\n{text}");
    assert!(text.contains("34"), "launches missing:\n{text}");
}

/// At 80 columns the chip row must not cut off the last categories; cycling onto
/// `Worlds` would otherwise produce no visible change.
#[test]
fn the_active_filter_chip_is_always_visible() {
    use crate::core::resource::model::ResourceKind;
    use crate::tui::pages::download_center::DownloadCategory;

    let mut app = nightlight_app_at(Page::DownloadCenter, 80, 24);
    app.download_center.category = DownloadCategory::Resource(ResourceKind::World);
    let text = render_text(&app, 80, 24);
    let row = text
        .lines()
        .find(|line| line.contains("Worlds"))
        .expect("the active category must be painted");
    assert!(
        row.contains('…'),
        "hidden categories need an ellipsis: {row:?}"
    );
}

/// A confirm prompt longer than the card wraps instead of being cut. An
/// ellipsis was the old answer; on a destructive dialog the truncated tail is
/// exactly the part that names what is about to change.
#[test]
fn a_long_confirm_prompt_wraps_instead_of_being_sliced() {
    use crate::tui::modal::{ConfirmAction, Modal};

    let prompt = "要从 TMCL 移除 TMCL-Test-A-Very-Long-Instance-Name-For-Clipping 吗？游戏文件和存档会保留。";
    let mut app = nightlight_app_at(Page::Instances, 120, 40);
    app.modal = Some(Modal::confirm(prompt, ConfirmAction::RemoveAccount));

    // Newlines are the wrap itself, so they come out with the spaces.
    let flat = render_text(&app, 120, 40).replace([' ', '\n'], "");
    assert!(
        flat.contains("游戏文件和存档会保留"),
        "the tail of the prompt is still cut:\n{flat}"
    );
}

/// In a Chinese UI `Vanilla` must read `原版`, in the filter strip and in every
/// list row. Brand loader names stay as they are.
#[test]
fn the_vanilla_loader_name_is_translated() {
    use crate::core::config::models::Language;

    let mut app = nightlight_app_at(Page::Instances, 120, 35);
    app.i18n = crate::i18n::I18n::new(Language::SimplifiedChinese);

    let labels = app.instance_filter_labels();
    assert_eq!(labels[0], app.i18n.text("common.all"));
    assert_eq!(labels[1], "原版", "the Vanilla filter chip");
    assert_eq!(&labels[2..], ["Fabric", "Forge", "NeoForge", "Quilt"]);

    // One mapping feeds the chip, the list row and the detail pane.
    assert_eq!(app.loader_label("Vanilla"), "原版");
    assert_eq!(app.loader_label("vanilla"), "原版");
    assert_eq!(app.loader_label("NeoForge"), "NeoForge");

    // The tab widget pads CJK labels with a space to keep them legible, so the
    // buffer reads `原 版`; compare without whitespace.
    let text = render_text(&app, 120, 35);
    let compact: String = text
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    assert!(
        compact.contains("原版"),
        "the strip must print the translation:\n{text}"
    );
}

/// No screen prints `Vanilla` in a Chinese UI.
///
/// The current-instance block, the overview table and the workspace's loader row
/// each built "loader, plus the version" by hand, so `Vanilla` kept coming back.
/// Every preset's two instance-facing pages are rendered here, so a new call site
/// cannot reintroduce it.
#[test]
fn no_screen_prints_the_vanilla_loader_untranslated() {
    use crate::core::config::models::{Language, UiPreset};
    use crate::core::instance::models::InstanceLoader;

    let mut app = shelf_data_app(Page::Instances);
    app.i18n = crate::i18n::I18n::new(Language::SimplifiedChinese);
    let id = Uuid::from_u128(0x5E1F_00FF);
    app.instances.apply(vec![InstanceSummary {
        id,
        name: "纯净生存".into(),
        version: "1.21.4".into(),
        loader: "Vanilla".into(),
        loader_version: None,
        status: "ready".into(),
        path: PathBuf::from("C:/games/.minecraft/versions/vanilla"),
        launch_count: 2,
        play_time_seconds: 600,
    }]);
    app.instances.current_id = Some(id);

    // The two helpers every screen goes through now.
    assert_eq!(app.instance_loader_text(&app.instances.items[0]), "原版");
    assert_eq!(app.draft_loader_text(&InstanceLoader::Vanilla), "原版");
    assert_eq!(
        app.draft_loader_text(&InstanceLoader::Fabric {
            version: "0.16.5".into()
        }),
        "Fabric 0.16.5",
        "brand names keep their spelling and carry the version"
    );

    for page in [Page::Instances, Page::Overview] {
        app.page = page;
        for preset in UiPreset::ALL {
            app.config.state.ui.preset = preset;
            let text = render_text(&app, 120, 40);
            assert!(
                !text.contains("Vanilla"),
                "{preset:?}/{page:?} still prints the raw loader name:\n{text}"
            );
        }
    }
}

/// Task rows print sizes, not byte counts, and the bar, the percentage and the
/// size share one set of column edges. A bar sized from the byte count ends every
/// row somewhere else and clips the percentage before its `%`.
#[test]
fn task_rows_align_their_sizes_bars_and_percents() {
    use crate::core::task::{TaskOperationType, TaskSnapshot, TaskState};

    let mut app = nightlight_app_at(Page::Tasks, 120, 35);
    let snapshot = |label: &str, state, completed, total: Option<u64>| TaskSnapshot {
        job: None,
        created_seq: 0,
        id: Uuid::new_v4(),
        label: label.to_owned(),
        operation_type: TaskOperationType::InstanceInstall,
        state,
        retryable: false,
        attempt: 0,
        completed,
        total,
        failure: None,
    };
    app.apply_task_snapshots(vec![
        snapshot("big", TaskState::Succeeded, 139_191_552, Some(139_191_552)),
        snapshot("half", TaskState::Running, 173_859, Some(347_718)),
        snapshot("unknown", TaskState::Running, 765_673_220, None),
    ]);
    let text = render_text(&app, 120, 35);
    assert!(
        !text.contains("139191552") && !text.contains("765673220"),
        "raw byte counts must not reach the frame:\n{text}"
    );
    assert!(text.contains("132.7/132.7 MB"), "sizes in MB:\n{text}");
    assert!(
        text.contains("730.2 MB"),
        "an unknown total still shows a size:\n{text}"
    );
    assert!(
        text.contains("169.8/339.6 KB"),
        "KB below a megabyte:\n{text}"
    );

    // One bar per task row; the sidebar wordmark also draws block glyphs, so
    // only the rows carrying a unit are considered.
    let rows: Vec<&str> = text
        .lines()
        .filter(|line| [" MB", " KB", " GB"].iter().any(|unit| line.contains(unit)))
        .collect();
    assert_eq!(rows.len(), 3, "three task rows:\n{text}");
    // Count cells, not bytes: `▎` and `█` are three bytes each in UTF-8.
    fn bar_column(line: &str) -> usize {
        line.chars()
            .take_while(|character| *character != '█' && *character != '░')
            .count()
    }

    let first_bar = bar_column(rows[0]);
    for row in &rows {
        assert_eq!(
            bar_column(row),
            first_bar,
            "bars must start on the same column"
        );
        let tail = row.trim_end();
        assert!(
            tail.ends_with('%') || tail.ends_with('—'),
            "the percentage is never clipped, and an unknown one says so: {tail:?}"
        );
    }
    assert!(
        rows.iter().any(|row| row.trim_end().ends_with('—')),
        "unknown total:\n{text}"
    );
}

#[test]
fn transfer_labels_fit_a_task_row() {
    use crate::tui::app::state::data_format::{format_eta, format_speed, format_transfer};

    assert_eq!(format_speed(0), "0 B/s");
    assert_eq!(format_speed(1024), "1.0 KB/s");
    assert_eq!(format_speed(12_897_485), "12.3 MB/s");
    assert_eq!(format_eta(134, 1).as_deref(), Some("2m 14s"));
    assert_eq!(format_eta(45, 1).as_deref(), Some("45s"));
    assert_eq!(format_eta(3_600, 1).as_deref(), Some("1h 00m"));
    assert_eq!(
        format_transfer(100, Some(200), 50).as_deref(),
        Some("50 B/s · 2s")
    );
    // Without a total, only the rate is known.
    assert_eq!(format_transfer(100, None, 50).as_deref(), Some("50 B/s"));
    // A reached total would read `0s`; the rate alone is the honest label.
    assert_eq!(
        format_transfer(200, Some(200), 50).as_deref(),
        Some("50 B/s")
    );
    assert_eq!(format_transfer(100, Some(200), 0), None);
}

#[test]
fn the_speed_tracker_ignores_short_windows_and_expires() {
    use std::time::{Duration, Instant};

    use crate::tui::app::state::tasks::SpeedTracker;

    let mut tracker = SpeedTracker::default();
    let id = uuid::Uuid::new_v4();
    let base = Instant::now();
    tracker.observe(id, 0, base);
    // 100 ms is below the sampling window; the pair is not used yet.
    tracker.observe(id, 500_000, base + Duration::from_millis(100));
    assert_eq!(tracker.speed(id, base + Duration::from_millis(100)), None);
    tracker.observe(id, 1_000_000, base + Duration::from_secs(1));
    assert_eq!(
        tracker.speed(id, base + Duration::from_secs(1)),
        Some(1_000_000)
    );
    // A sample older than three seconds is stale, not a speed from the past.
    assert_eq!(tracker.speed(id, base + Duration::from_secs(5)), None);
    // A retry restarts the counter instead of producing a negative delta.
    tracker.observe(id, 10, base + Duration::from_secs(6));
    assert_eq!(tracker.speed(id, base + Duration::from_secs(6)), None);
}

/// The building blocks of those rows, checked directly.
#[test]
fn byte_sizes_and_percentages_share_one_format() {
    use crate::tui::app::state::data_format::{format_progress_bytes, format_size, percent_label};

    assert_eq!(format_size(0), "0 B");
    assert_eq!(format_size(512), "512 B");
    assert_eq!(format_size(2048), "2.0 KB");
    assert_eq!(format_size(5 * 1024 * 1024), "5.0 MB");
    assert_eq!(format_size(3 * 1024 * 1024 * 1024), "3.0 GB");

    assert_eq!(format_progress_bytes(512, None), "512 B");
    assert_eq!(format_progress_bytes(1024, Some(2048)), "1.0/2.0 KB");
    assert_eq!(
        format_progress_bytes(139_191_552, Some(139_191_552)),
        "132.7/132.7 MB"
    );
    // A zero total means "not known yet", not "done".
    assert_eq!(format_progress_bytes(4096, Some(0)), "4.0 KB");

    assert_eq!(percent_label(None), "   —");
    assert_eq!(percent_label(Some(0.0)), "  0%");
    assert_eq!(percent_label(Some(0.5)), " 50%");
    assert_eq!(percent_label(Some(1.0)), "100%");
    assert_eq!(
        percent_label(Some(7.0)),
        "100%",
        "over-reporting is clamped"
    );
}

/// A dialog prints its keys once. A help sentence, the clickable key row and the
/// footer bar would otherwise name the same key three times.
#[test]
fn a_classic_dialog_prints_its_keys_once() {
    use crate::core::config::models::UiPreset;
    use crate::tui::modal::{ConfirmAction, FormAction, Modal};

    let mut app = nightlight_app_at(Page::Instances, 120, 40);
    app.config.state.ui.preset = UiPreset::Classic;

    app.modal = Some(Modal::confirm(
        "Remove the instance?",
        ConfirmAction::RemoveInstance(uuid::Uuid::nil()),
    ));
    // The dialog title is also `Confirm`, so the cancel label is the one that
    // tells a duplicated key row apart.
    let text = render_text(&app, 120, 40);
    let cancel = app.i18n.text("hint.cancel");
    assert_eq!(
        text.matches(&cancel).count(),
        1,
        "`{cancel}` must appear exactly once in a dialog:\n{text}"
    );

    app.modal = Some(Modal::form(
        "Create instance",
        &["Name".to_string()],
        &["demo".to_string()],
        FormAction::CreateInstance,
    ));
    let text = render_text(&app, 120, 40);
    let cancel = app.i18n.text("hint.cancel");
    assert_eq!(
        text.matches(&cancel).count(),
        1,
        "the form's cancel hint belongs to its key row alone:\n{text}"
    );
    assert!(
        !text.contains(&app.i18n.text("modal.text_form_help")),
        "the paraphrased help line is gone:\n{text}"
    );
}

/// The nightlight form must show `F2`: it lives in the key handler, so a form
/// that omits it leaves the picker unreachable. `Enter` is labelled 保存, but it
/// walks to the next field.
#[test]
fn a_nightlight_form_shows_the_browse_key_and_what_enter_really_does() {
    use crate::tui::action::ModalAct;
    use crate::tui::modal::{FormAction, Modal};

    let mut app = nightlight_app_at(Page::Instances, 120, 40);
    let labels = [
        app.i18n.text("form.modpack_archive"),
        app.i18n.text("form.game_directory"),
        app.i18n.text("form.new_instance_name"),
    ];
    app.modal = Some(
        Modal::form(
            app.i18n.text("form.import_modpack"),
            &labels,
            &[String::new(), r"D:\Games".to_owned(), String::new()],
            FormAction::ImportLocalModpack,
        )
        .with_file_picker(0)
        .with_directory_picker(1),
    );

    let browse = app.i18n.text("hint.browse");
    let text = render_text(&app, 120, 40);
    assert!(
        text.contains("F2") && text.contains(&browse),
        "the archive field has to advertise F2:\n{text}"
    );
    assert!(
        text.contains(&app.i18n.text("hint.next_field")),
        "Enter advances here, it does not save:\n{text}"
    );

    app.modal.as_mut().expect("modal").apply(ModalAct::Focus(1));
    let text = render_text(&app, 120, 40);
    assert!(
        text.contains("F2"),
        "the folder field is a picker too:\n{text}"
    );

    app.modal.as_mut().expect("modal").apply(ModalAct::Focus(2));
    let text = render_text(&app, 120, 40);
    assert!(
        !text.contains("F2"),
        "a plain text field must not advertise a picker:\n{text}"
    );
    assert!(
        text.contains(&app.i18n.text("action.save")),
        "the last field is where Enter saves:\n{text}"
    );
}

/// `/` on the classic settings page swaps the table for a search view with a
/// visible field; before this the key changed the footer and nothing else.
#[test]
fn the_classic_settings_page_searches_from_a_field() {
    use crate::core::config::models::UiPreset;

    let mut app = nightlight_app_at(Page::Settings, 120, 40);
    app.config.state.ui.preset = UiPreset::Classic;
    draw_key(&mut app, crossterm::event::KeyCode::Char('/'));
    let text = render_text(&app, 120, 40);
    assert!(text.contains('⌕'), "the field has to be on screen:\n{text}");
    assert!(
        text.contains(&app.i18n.text("settings.search_title")),
        "the panel says what it is:\n{text}"
    );

    for character in "mem".chars() {
        draw_key(&mut app, crossterm::event::KeyCode::Char(character));
    }
    let text = render_text(&app, 120, 40);
    assert!(text.contains("mem"), "the query is visible:\n{text}");
    // A narrowed list keeps the category prefix and drops the rest.
    assert!(text.contains(" · "), "matches list their category:\n{text}");

    // Esc closes the view and gives the table back.
    draw_key(&mut app, crossterm::event::KeyCode::Esc);
    let text = render_text(&app, 120, 40);
    assert!(!text.contains('⌕'), "the table is back:\n{text}");
}

/// Feeds one key through the app the way the event loop does.
fn draw_key(app: &mut App, code: crossterm::event::KeyCode) {
    app.handle_key(crossterm::event::KeyEvent::new(
        code,
        crossterm::event::KeyModifiers::NONE,
    ));
}

/// The three steps of a modpack install share a `job` id, and the task list
/// has to collapse them into one row: three separate lines for one download
/// read as unrelated work.
#[test]
fn a_modpack_job_collapses_into_one_row() {
    use crate::core::task::{TaskOperationType, TaskSnapshot, TaskState};

    let job = Uuid::new_v4();
    let step =
        |label: &str, op, state, completed, total: Option<u64>, job: Option<Uuid>| TaskSnapshot {
            id: Uuid::new_v4(),
            created_seq: 0,
            job,
            label: label.to_owned(),
            operation_type: op,
            state,
            retryable: false,
            attempt: 0,
            completed,
            total,
            failure: None,
        };
    let mut app = nightlight_app_at(Page::Tasks, 120, 30);
    app.apply_task_snapshots(vec![
        step(
            "Download modpack Zombie Invade 100 Days 2.3.mrpack",
            TaskOperationType::ModpackInstall,
            TaskState::Succeeded,
            29_975_184,
            Some(29_975_184),
            Some(job),
        ),
        step(
            "Install Zombie Invade 100 Days 2.3 (1.20.1, forge)",
            TaskOperationType::InstanceInstall,
            TaskState::Running,
            1_000,
            Some(765_673_220),
            Some(job),
        ),
        step(
            "Install 129 modpack files",
            TaskOperationType::ModpackFiles,
            TaskState::Queued,
            0,
            Some(139_191_552),
            Some(job),
        ),
        step(
            "Install Mojang Java 21",
            TaskOperationType::JavaRuntime,
            TaskState::Queued,
            0,
            Some(1),
            None,
        ),
    ]);

    let rows = app.task_rows();
    assert_eq!(rows.len(), 2, "one row per job, one for the lone task");
    let grouped = &rows[0];
    assert_eq!(grouped.steps, 3, "three steps behind the row");
    assert_eq!(grouped.step, 2, "the row reports the step that is moving");
    assert_eq!(
        grouped.label, "Install Zombie Invade 100 Days 2.3 (1.20.1, forge)",
        "the loader step names the instance, so it names the row"
    );
    assert_eq!(grouped.state, TaskState::Running);
    assert_eq!(grouped.completed, 29_975_184 + 1_000);
    assert_eq!(
        grouped.total,
        Some(29_975_184 + 765_673_220 + 139_191_552),
        "progress is summed, not taken from one step"
    );

    let text = render_text(&app, 120, 30);
    assert!(
        text.contains("2/3"),
        "the row says which step is running:\n{text}"
    );
    assert!(
        !text.contains("Install 129 modpack files"),
        "a step must not get a row of its own:\n{text}"
    );
}

/// A failure anywhere in the job colours the whole row.
#[test]
fn a_failed_step_marks_the_whole_job_failed() {
    use crate::core::task::{TaskOperationType, TaskSnapshot, TaskState};

    let job = Uuid::new_v4();
    let step = |op, state, failure: Option<&str>| TaskSnapshot {
        id: Uuid::new_v4(),
        created_seq: 0,
        job: Some(job),
        label: format!("{op:?}"),
        operation_type: op,
        state,
        retryable: true,
        attempt: 1,
        completed: 0,
        total: Some(10),
        failure: failure.map(str::to_owned),
    };
    let mut app = nightlight_app_at(Page::Tasks, 120, 30);
    app.apply_task_snapshots(vec![
        step(
            TaskOperationType::ModpackInstall,
            TaskState::Succeeded,
            None,
        ),
        step(
            TaskOperationType::InstanceInstall,
            TaskState::Failed,
            Some("checksum mismatch"),
        ),
    ]);

    let rows = app.task_rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].state, TaskState::Failed);
    assert_eq!(rows[0].failure.as_deref(), Some("checksum mismatch"));
    assert!(rows[0].retryable, "the row is retryable through its step");
}

/// `?` is advertised as a global key, so it has to open the help from every mode,
/// including an open detail pane and the instance workspace. Any mode left out
/// makes the help page look gone.
#[test]
fn the_help_page_opens_from_every_mode() {
    let press = |app: &mut App| {
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('?'),
            crossterm::event::KeyModifiers::NONE,
        ));
        app.help_open
    };

    // Normal.
    let mut app = nightlight_app_at(Page::Instances, 120, 40);
    assert!(press(&mut app), "normal mode");
    // ...and `?` again closes it.
    assert!(!press(&mut app), "a second `?` closes the panel");

    // Detail overlay.
    let mut app = nightlight_app_at(Page::Instances, 120, 40);
    app.detail_open = true;
    assert_eq!(
        app.mode(),
        crate::tui::command::Mode::Detail(crate::tui::command::DetailTarget::Instances)
    );
    assert!(press(&mut app), "detail mode");

    // Instance workspace, which is the biggest key map in the app.
    let mut app = workspace_app(crate::tui::pages::workspace::WorkspaceCategory::Mods);
    app.config.state.ui.preset = crate::core::config::models::UiPreset::Classic;
    assert_eq!(app.mode(), crate::tui::command::Mode::Workspace);
    assert!(press(&mut app), "workspace mode");
}

/// An option list that failed must not render as an empty box: that is the same
/// picture as "your filter matched nothing", with no way to learn that `r`
/// retries.
#[test]
fn a_failed_option_list_says_so_and_offers_the_retry_key() {
    use crate::tui::modal::{FormAction, Modal};

    let mut app = nightlight_app_at(Page::Instances, 120, 30);
    let modal = Modal::form(
        "Pick a loader version",
        &["Version"],
        &[String::new()],
        FormAction::CreateInstance,
    )
    .with_selection_picker(0, Vec::new());
    app.modal = Some(modal);
    // Opening an empty list asks for the options; the failure arrives after.
    app.modal
        .as_mut()
        .expect("modal")
        .apply(crate::tui::action::ModalAct::Toggle(0));
    app.modal
        .as_mut()
        .expect("modal")
        .set_selection_error(0, "network down");

    let text = render_text(&app, 120, 30);
    assert!(
        text.contains("network down"),
        "the reason has to show:\n{text}"
    );
    let retry = app.i18n.text("action.retry");
    assert!(
        text.contains(&retry),
        "`r {retry}` has to be offered:\n{text}"
    );
}

/// A modpack writes the version jar and json before it fetches a single mod, so
/// the disk-only summary read "ready" while the task was still at 0%. The badge
/// has to say the instance is not playable yet.
#[test]
fn an_instance_being_installed_does_not_read_ready() {
    use crate::core::config::models::UiPreset;

    assert_eq!(
        crate::tui::app::state::data::instance_status_kind("installing"),
        crate::tui::widgets::status::StatusKind::Running,
        "installing is progress, not a missing file"
    );

    for preset in UiPreset::ALL {
        let mut app = nightlight_app_at(Page::Instances, 120, 40);
        app.config.state.ui.preset = preset;
        app.instances.apply(vec![InstanceSummary {
            id: Uuid::new_v4(),
            name: "Pack".to_owned(),
            version: "1.20.1".to_owned(),
            loader: "Forge".to_owned(),
            loader_version: Some("47.4.10".to_owned()),
            status: crate::tui::app::state::data::INSTALLING.to_owned(),
            path: PathBuf::from("D:/games/.minecraft/versions/Pack"),
            launch_count: 0,
            play_time_seconds: 0,
        }]);
        // The footer status line also says "Ready"; keep it out of the scan so
        // the assertion talks about the badge.
        app.status = crate::tui::app::Status::info(String::new());

        let text = render_text(&app, 120, 40);
        let installing = app.i18n.text("status.installing");
        assert!(
            text.contains(&installing),
            "{preset:?} lost the installing badge:\n{text}"
        );
        assert!(
            !text.contains(&app.i18n.text("status.ready")),
            "{preset:?} still reads ready while the task runs:\n{text}"
        );
    }
}

/// The instance detail binds `L` but the footer never printed it, so the key
/// read as missing exactly where the launcher is most used.
#[test]
fn the_instance_detail_advertises_the_launch_key() {
    let mut app = nightlight_app_at(Page::Instances, 120, 40);
    let summary = InstanceSummary {
        id: Uuid::new_v4(),
        name: "Detail-Test".to_owned(),
        version: "1.21.4".to_owned(),
        loader: "Fabric".to_owned(),
        loader_version: Some("0.16.9".to_owned()),
        status: "ready".to_owned(),
        path: PathBuf::from(r"D:/games/.minecraft/versions/Detail-Test"),
        launch_count: 1,
        play_time_seconds: 60,
    };
    app.instances.apply(vec![summary.clone()]);
    app.instances.set_current(Some(summary.id));
    app.detail_open = true;

    let launch = app.i18n.text("action.launch");
    let text = render_text(&app, 120, 40);
    assert!(
        text.contains('L') && text.contains(&launch),
        "the detail has to offer `L {launch}`:\n{text}"
    );
    assert!(
        text.contains(&app.i18n.text("hint.close")),
        "and still keep the way out:\n{text}"
    );
}

/// The panel lists three sections in every preset: the shared content is built
/// once, so a preset cannot quietly lose one.
#[test]
fn the_help_page_lists_every_section_in_every_preset() {
    use crate::core::config::models::{Language, UiPreset};

    for preset in UiPreset::ALL {
        let mut app = nightlight_app_at(Page::Instances, 120, 40);
        app.config.state.ui.preset = preset;
        app.i18n = crate::i18n::I18n::new(Language::SimplifiedChinese);
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('?'),
            crossterm::event::KeyModifiers::NONE,
        ));

        // CJK labels are padded with spaces in the buffer, so compare without
        // whitespace the way the loader-name test does.
        let render = |app: &App| -> String {
            render_text(app, 120, 40)
                .chars()
                .filter(|character| !character.is_whitespace())
                .collect()
        };
        let text = render(&app);
        for key in ["help.section.general", "help.section.page"] {
            let title = app.i18n.text(key);
            let compact: String = title.chars().filter(|c| !c.is_whitespace()).collect();
            assert!(
                text.contains(&compact),
                "{preset:?}: the help page is missing `{title}`"
            );
        }

        // The mouse section sits below the fold; `End` brings it into view
        // instead of the card stretching to hold it.
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::End,
            crossterm::event::KeyModifiers::NONE,
        ));
        let text = render(&app);
        for key in ["help.section.diagnosis", "help.section.mouse"] {
            let title = app.i18n.text(key);
            let compact: String = title.chars().filter(|c| !c.is_whitespace()).collect();
            assert!(
                text.contains(&compact),
                "{preset:?}: `{title}` must be reachable by scrolling"
            );
        }
        assert!(
            text.contains("滚动当前列表"),
            "{preset:?}: the last mouse row must be reachable"
        );
    }
}

/// `?` stretched its card to every binding of the page: on the instance list
/// that put the mouse section on the bottom border, and on the workspace pages
/// it ran past the terminal. The card now stops at a fixed height and scrolls.
#[test]
fn the_help_card_scrolls_instead_of_growing_with_the_key_map() {
    use crate::core::config::models::{Language, UiPreset};

    for preset in UiPreset::ALL {
        let mut app = nightlight_app_at(Page::Instances, 120, 40);
        app.config.state.ui.preset = preset;
        app.i18n = I18n::new(Language::SimplifiedChinese);
        app.help_open = true;

        let lines = app.help_lines();
        let layout = app.help_layout(Rect::new(0, 0, 120, 40), &lines);
        assert!(
            layout.rect.height < u16::try_from(lines.len()).expect("rows") + 4,
            "{preset:?}: the card grew to fit every row"
        );
        // One row of breathing room above and below. Flush text on the border
        // is what this whole change started from.
        assert!(
            layout.body.y > layout.rect.y + 1,
            "{preset:?}: the first line touches the top border"
        );
        assert!(
            layout.body.bottom() <= layout.rect.bottom().saturating_sub(2),
            "{preset:?}: the last line touches the bottom border"
        );
        assert!(
            app.help_max_scroll() > 0,
            "{preset:?}: the reference is longer than the card, so it must scroll"
        );
    }
}

/// The reference is a dialog in every preset, `paper` included: same card
/// rectangle, same scroll window. Printed as a page, it also covers the running
/// head and the key band that close it, so the way out disappears with the page.
#[test]
fn every_preset_puts_the_help_in_the_same_card() {
    use crate::core::config::models::{Language, UiPreset};

    for (width, height) in [(120u16, 40u16), (80, 24)] {
        let mut reference = None;
        for preset in UiPreset::ALL {
            let mut app = nightlight_app_at(Page::Instances, width, height);
            app.config.state.ui.preset = preset;
            app.i18n = I18n::new(Language::SimplifiedChinese);
            app.help_open = true;
            let layout = app.help_layout(Rect::new(0, 0, width, height), &app.help_lines());
            match reference {
                None => reference = Some(layout.rect),
                Some(rect) => assert_eq!(
                    layout.rect, rect,
                    "{preset:?} does not use the shared card at {width}x{height}"
                ),
            }
            assert!(
                layout.rect.width < width,
                "{preset:?}: the card took the page at {width}x{height}"
            );
        }
    }
}

/// The card is measured from its own rows, not from the terminal. Short labels in
/// a fixed 76-column box leave two thirds of it empty.
#[test]
fn the_help_card_is_measured_from_its_longest_row() {
    let app = nightlight_app_at(Page::Instances, 120, 40);
    let lines = app.help_lines();
    let narrow = app.help_layout(Rect::new(0, 0, 120, 40), &lines).rect;
    let wide = app.help_layout(Rect::new(0, 0, 160, 50), &lines).rect;

    assert_eq!(
        narrow.width, wide.width,
        "the card follows the content, not the terminal"
    );
    assert!(
        narrow.width < 76,
        "a short key map must not take the whole popup width"
    );
}

#[test]
fn the_help_page_scrolls_from_the_keyboard() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = nightlight_app_at(Page::Instances, 120, 40);
    app.i18n = I18n::new(Language::SimplifiedChinese);
    app.help_open = true;
    let max = app.help_max_scroll();
    assert!(max > 0, "the fixture page must overflow the card");

    let press = |app: &mut App, code: KeyCode| {
        app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    };

    press(&mut app, KeyCode::Down);
    assert_eq!(app.help_scroll, 1);
    press(&mut app, KeyCode::Char('j'));
    assert_eq!(app.help_scroll, 2, "j scrolls like Down");

    press(&mut app, KeyCode::End);
    assert_eq!(app.help_scroll, max, "End reaches the last row");
    press(&mut app, KeyCode::Down);
    assert_eq!(app.help_scroll, max, "and the page stops there");

    press(&mut app, KeyCode::Home);
    assert_eq!(app.help_scroll, 0);
    press(&mut app, KeyCode::Up);
    assert_eq!(app.help_scroll, 0, "the top is the top");

    // A page step is the visible height, not a hard-coded 8 rows.
    let page = usize::try_from(app.help_page_rows()).expect("page rows");
    press(&mut app, KeyCode::PageDown);
    assert_eq!(app.help_scroll, page.min(max));
}

/// With the help card open the arrows must scroll it, not the workspace behind
/// it: `?` over an instance would look frozen while the hidden list moved.
#[test]
fn the_help_card_scrolls_over_the_instance_workspace() {
    use crate::core::config::models::Language;
    use crate::tui::pages::workspace::WorkspaceCategory;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = workspace_app(WorkspaceCategory::Mods);
    app.i18n = I18n::new(Language::SimplifiedChinese);

    let key = |app: &mut App, code| {
        app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    };
    key(&mut app, KeyCode::Char('?'));
    assert_eq!(app.mode(), crate::tui::command::Mode::Help);
    assert!(app.help_max_scroll() > 0, "the workspace key map is long");

    // The workspace is on screen behind the card; its cursor must not move.
    let (category, selected) = app
        .workspace
        .as_ref()
        .map(|workspace| (workspace.category, workspace.selected))
        .expect("workspace");
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Char('j'));
    assert_eq!(app.help_scroll, 2, "the keys scroll the card");
    let after = app.workspace.as_ref().expect("workspace");
    assert_eq!(after.category, category);
    assert_eq!(
        after.selected, selected,
        "the page behind the card stays put"
    );

    key(&mut app, KeyCode::End);
    assert_eq!(app.help_scroll, app.help_max_scroll());
    key(&mut app, KeyCode::Home);
    assert_eq!(app.help_scroll, 0);
}

/// The line under the account name says what the hours are: a bare "剩余 5
/// 小时" reads like a countdown the player has to act on, when all it describes
/// is when the cached credential runs out (a silent refresh follows).
#[test]
fn the_account_card_names_the_credential_lifetime() {
    use crate::core::account::models::{Account, AccountKind};

    let mut app = nightlight_app_at(Page::Overview, 120, 40);
    app.i18n = I18n::new(Language::SimplifiedChinese);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock")
        .as_secs();
    let account = Account {
        id: Uuid::new_v4(),
        kind: AccountKind::Microsoft,
        username: "Alex".to_owned(),
        uuid: Uuid::new_v4(),
        skin: None,
        secret_ref: None,
        // A minute of slack: the renderer takes its own `now`, and the label
        // floors to whole hours, so exactly five hours can read as four when a
        // second ticks over between the two clocks.
        access_expires_at: Some(now + 5 * 3600 + 60),
    };
    app.accounts.current = Some(account.id);
    app.accounts.accounts = vec![account];

    let text: String = render_text(&app, 120, 40)
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    assert!(
        text.contains("凭据有效期5小时"),
        "the card must name the credential, not just a countdown:\n{text}"
    );
    assert!(
        !text.contains("剩余"),
        "the old countdown wording is gone:\n{text}"
    );
}

#[test]
fn shelf_download_project_index_keeps_the_record_column() {
    use crate::core::resource::model::ResourceKind;
    use crate::tui::pages::download_center::DownloadCategory;

    let mut app = shelf_data_app(Page::DownloadCenter);
    app.download_center.category = DownloadCategory::Resource(ResourceKind::Mod);

    let wide = render_text(&app, 120, 40);
    assert!(
        wide.contains(&app.i18n.text("common.details")),
        "the project index lost its record column:\n{wide}"
    );
    for field in ["shelf-project-0", "1.21.4", "neoforge"] {
        assert!(
            wide.contains(field),
            "the record column dropped {field}:\n{wide}"
        );
    }

    // The record column belongs to the project index only: a narrow terminal
    // keeps the list, and the file table never splits.
    app.terminal_width = 80;
    app.terminal_height = 24;
    let narrow = render_text(&app, 80, 24);
    assert!(
        !narrow.contains(&app.i18n.text("common.details")),
        "a narrow shelf page must not keep the record column:\n{narrow}"
    );
    assert!(narrow.contains("Farming Tales"), "{narrow}");

    app.terminal_width = 120;
    app.terminal_height = 40;
    app.resource_detail = Some("shelf-project-0".into());
    app.resource_files = vec![crate::core::resource::model::ResourceFile {
        source: crate::core::resource::model::ResourceSource::Modrinth,
        project_id: "shelf-project-0".into(),
        file_id: "v1".into(),
        name: "farmingtales-1.4.3.jar".into(),
        download_url: "https://example.invalid/a.jar".into(),
        sha1: None,
        size: Some(1024),
        game_versions: vec!["1.21.4".into()],
        loaders: vec!["neoforge".into()],
        channel: crate::core::resource::model::ReleaseChannel::Release,
        dependencies: Vec::new(),
    }];
    let files = render_text(&app, 120, 40);
    assert!(files.contains("farmingtales-1.4.3.jar"), "{files}");
    assert!(
        !files.contains(&app.i18n.text("common.details")),
        "the file table must keep the full width:\n{files}"
    );
}

#[test]
fn shelf_download_lists_remember_their_own_window() {
    use crate::core::resource::model::{ResourceKind, ResourceProject, ResourceSource};
    use crate::tui::pages::download_center::DownloadCategory;
    use crate::tui::widgets::rows::RowWindow;

    let mut app = shelf_data_app(Page::DownloadCenter);
    app.download_center.category = DownloadCategory::Resource(ResourceKind::Mod);
    app.resource_projects = (0..30)
        .map(|index| ResourceProject {
            source: ResourceSource::Modrinth,
            project_id: format!("project-{index:02}"),
            title: format!("project-{index:02}"),
            author: "shelf".into(),
            description: String::new(),
            icon_url: None,
            categories: Vec::new(),
            game_versions: Vec::new(),
            loaders: Vec::new(),
            downloads: None,
            updated_at: None,
        })
        .collect();
    app.download_center.selected = 29;
    render_text(&app, 120, 40);
    let projects = app
        .row_windows
        .offset(RowWindow::ResourceProjects)
        .expect("the project list was drawn");
    assert!(projects > 0, "the window did not follow the selection");

    app.download_center.category = DownloadCategory::GameVersions;
    app.version_selected = 3;
    render_text(&app, 120, 40);
    let versions = app
        .row_windows
        .offset(RowWindow::Versions)
        .expect("the version list was drawn");
    assert_eq!(versions, 0, "five versions fit, so the window stays at 0");

    app.download_center.category = DownloadCategory::Resource(ResourceKind::Mod);
    render_text(&app, 120, 40);
    assert_eq!(
        app.row_windows.offset(RowWindow::ResourceProjects),
        Some(projects),
        "switching category threw the project window away"
    );
}

#[test]
fn shelf_task_bars_follow_the_progress() {
    use crate::core::task::{TaskOperationType, TaskSnapshot, TaskState};

    fn filled(state: TaskState, completed: u64, total: Option<u64>) -> usize {
        let mut app = shelf_data_app(Page::Tasks);
        app.task_snapshots = vec![TaskSnapshot {
            job: None,
            created_seq: 0,
            id: Uuid::from_u128(0x5E1F_2001),
            label: "shelf task".into(),
            operation_type: TaskOperationType::ResourceInstall,
            state,
            retryable: false,
            attempt: 0,
            completed,
            total,
            failure: None,
        }];
        let text = render_text(&app, 120, 40);
        let name_row = text
            .lines()
            .position(|line| line.contains("shelf task"))
            .expect("the task row is on screen");
        text.lines()
            .nth(name_row + 1)
            .map(|line| line.matches('█').count())
            .unwrap_or(0)
    }

    assert_eq!(filled(TaskState::Running, 0, Some(100)), 0);
    assert_eq!(filled(TaskState::Running, 50, Some(100)), 6);
    assert_eq!(filled(TaskState::Succeeded, 100, Some(100)), 12);
    assert_eq!(filled(TaskState::Running, 1, None), 0, "no total, no bar");
}

/// The bar is the right-hand column of the entry, under the state label, not a
/// block that starts at the name's left margin and stops wherever it likes.
#[test]
fn shelf_task_bars_share_one_right_edge() {
    use crate::core::task::{TaskOperationType, TaskSnapshot, TaskState};

    let mut app = shelf_data_app(Page::Tasks);
    let snapshot = |seq: u64, label: &str, state, completed, total: Option<u64>| TaskSnapshot {
        job: None,
        created_seq: seq,
        id: Uuid::new_v4(),
        label: label.to_owned(),
        operation_type: TaskOperationType::ResourceInstall,
        state,
        retryable: false,
        attempt: 0,
        completed,
        total,
        failure: None,
    };
    app.task_snapshots = vec![
        snapshot(0, "short", TaskState::Succeeded, 100, Some(100)),
        snapshot(
            1,
            "a much longer task name that runs most of the way",
            TaskState::Running,
            50,
            Some(100),
        ),
        snapshot(2, "mid", TaskState::Running, 7, None),
    ];
    let text = render_text(&app, 120, 40);
    let lines: Vec<&str> = text.lines().collect();
    // The progress row is the one under the name; the unknown-total row ends in
    // `—` and carries no `%`.
    let bars: Vec<&str> = ["short", "a much longer task name", "mid"]
        .iter()
        .map(|label| {
            let index = lines
                .iter()
                .position(|line| line.contains(label))
                .unwrap_or_else(|| panic!("missing row {label}:\n{text}"));
            lines[index + 1]
        })
        .collect();

    let start = |line: &str| {
        line.chars()
            .take_while(|character| *character != '█' && *character != '░')
            .count()
    };
    let ends: Vec<usize> = bars
        .iter()
        .map(|line| line.trim_end().chars().count())
        .collect();
    assert!(
        bars.iter()
            .map(|line| start(line))
            .all(|column| column == start(bars[0])),
        "the bars do not start on the same column: {:?}\n{text}",
        bars.iter().map(|line| start(line)).collect::<Vec<_>>()
    );
    assert!(
        ends.windows(2).all(|pair| pair[0] == pair[1]),
        "the percentages do not share a right edge: {ends:?}\n{text}"
    );
    assert!(
        start(bars[0]) > 40,
        "the bar still starts at the name's margin (column {})",
        start(bars[0])
    );
}

#[test]
fn shelf_logs_reach_the_bottom_and_say_when_empty() {
    let mut app = shelf_data_app(Page::Logs);
    for index in 0..60 {
        app.logs.append(format!("[main/INFO] line {index:02}"));
    }
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::End,
        crossterm::event::KeyModifiers::NONE,
    ));
    let text = render_text(&app, 120, 40);
    assert!(
        text.contains("line 59"),
        "the newest line must be on screen after `End`:\n{text}"
    );
    assert!(
        !text.contains("line 00"),
        "`End` must scroll the oldest lines away:\n{text}"
    );

    let empty = shelf_app(Page::Logs);
    let text = render_text(&empty, 120, 40);
    assert!(
        text.contains(&empty.i18n.text("logs.empty")),
        "an empty buffer must say so:\n{text}"
    );
}

#[test]
fn shelf_settings_switch_every_category() {
    use crate::tui::action::Action;
    use crate::tui::pages::settings::SettingsCategory;

    let mut app = shelf_data_app(Page::Settings);
    for (index, category) in SettingsCategory::ALL.iter().enumerate() {
        app.apply_action(Action::category(index), crate::tui::action::Click::Single);
        app.terminal_width = 120;
        app.terminal_height = 40;
        let text = render_text(&app, 120, 40);
        let title = app.i18n.text(category.title_key());
        assert!(
            text.contains(&title),
            "{category:?} lost its name on the category row:\n{text}"
        );
        assert!(
            app.settings.selected < app.settings_row_count().max(1),
            "{category:?} left the selection past the end of its list"
        );
    }

    // A longer list leaves a selection a shorter one cannot hold.
    app.apply_action(
        Action::category(SettingsCategory::General.index()),
        crate::tui::action::Click::Single,
    );
    app.settings.selected = app.settings_row_count().saturating_sub(1);
    let long = app.settings.selected;
    app.apply_action(
        Action::category(SettingsCategory::Java.index()),
        crate::tui::action::Click::Single,
    );
    assert!(
        app.settings.selected < app.settings_row_count().max(1),
        "switching to a shorter category kept selection {long}"
    );
}

#[test]
fn shelf_detail_and_help_open_on_their_own_slab() {
    let mut app = shelf_data_app(Page::Instances);
    app.open_details();
    let wide = render_text(&app, 120, 40);
    assert!(
        wide.contains(&app.i18n.text("common.details")),
        "the detail card lost its title:\n{wide}"
    );
    assert!(
        wide.contains(&app.i18n.text("action.close").to_uppercase()) || wide.contains("Esc"),
        "the detail card must keep its way out:\n{wide}"
    );

    app.detail_open = false;
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('?'),
        crossterm::event::KeyModifiers::NONE,
    ));
    let help = render_text(&app, 80, 24);
    assert!(
        help.contains(&app.i18n.text("help.keyboard")),
        "the help card lost its title:\n{help}"
    );
    // `Esc` is printed on the card; the footer behind it is covered, so the way
    // out has to be inside the overlay.
    assert!(help.contains("Esc"), "the help card lost `Esc`:\n{help}");
}

/// The wordmark is a mark, not a heading: four rows of block glyphs on every size
/// class that can spare them. A `Compact` 120×35 terminal, four rows short of
/// `Full`, would otherwise show a one-line `TMCL 0.3.0` over two thirds empty
/// page.
#[test]
fn shelf_draws_the_block_wordmark_on_every_size_that_can_spare_it() {
    use crate::core::config::models::UiPreset;

    for (width, height) in [(120u16, 35u16), (100, 30), (120, 40)] {
        let mut app = shelf_data_app(Page::Overview);
        app.config.state.ui.preset = UiPreset::Shelf;
        app.terminal_width = width;
        app.terminal_height = height;
        let text = render_text(&app, width, height);
        let head = text.lines().take(4).collect::<Vec<_>>().join("\n");
        assert!(
            head.contains('▀') || head.contains('█'),
            "{width}x{height} drew a text line where the mark belongs:\n{text}"
        );
        assert!(
            text.contains(env!("CARGO_PKG_VERSION")),
            "{width}x{height} kept the mark but dropped the version:\n{text}"
        );
    }
}

/// The ramp runs corner to corner: the top-left stroke is lighter than the
/// bottom-left one, and the bottom row still darkens from left to right. A fill
/// per row would pass the first check and fail the second.
#[test]
fn shelf_wordmark_ramps_from_light_to_accent() {
    use crate::core::config::models::UiPreset;
    use crate::tui::shell::{SHELF_WORDMARK_COLS, SHELF_WORDMARK_ROWS};

    let mut app = shelf_data_app(Page::Overview);
    app.config.state.ui.preset = UiPreset::Shelf;
    app.terminal_width = 120;
    app.terminal_height = 35;
    let buffer = render_buffer(&app, 120, 35);

    let brightness = |colour: ratatui::style::Color| {
        let ratatui::style::Color::Rgb(red, green, blue) = colour else {
            panic!("the wordmark ramp must be RGB, got {colour:?}");
        };
        u32::from(red) + u32::from(green) + u32::from(blue)
    };
    // The head starts at the page margin, two columns in.
    let margin = 2;
    let top_left = buffer[(margin, 0)].fg;
    let bottom_left = buffer[(margin, SHELF_WORDMARK_ROWS - 1)].fg;
    let bottom_right = buffer[(margin + SHELF_WORDMARK_COLS - 1, SHELF_WORDMARK_ROWS - 1)].fg;

    assert!(
        brightness(top_left) > brightness(bottom_left) + 60,
        "the mark does not carry a vertical ramp: {top_left:?} → {bottom_left:?}"
    );
    assert!(
        brightness(bottom_left) > brightness(bottom_right) + 30,
        "the mark carries a flat fill per row: {bottom_left:?} → {bottom_right:?}"
    );
}

/// The launch hint belongs beside the name bar, not under the cover: under it
/// the line read as a caption of the picture and cost the column a row.
#[test]
fn shelf_overview_puts_the_launch_hint_beside_the_name_bar() {
    let app = shelf_app(Page::Overview);
    let text = render_text(&app, 120, 40);
    let hint = app.i18n.text("overview.enter_launch");
    let line = text
        .lines()
        .find(|line| line.contains(&hint))
        .unwrap_or_else(|| panic!("the launch hint is gone:\n{text}"));
    assert!(
        line.contains("Create Above and Beyond"),
        "the hint is not on the name bar's row: {line:?}"
    );
    assert!(
        line.find(&hint).unwrap_or(0) > line.find("Create Above and Beyond").unwrap_or(0),
        "the hint sits before the name: {line:?}"
    );
}

/// The version lines up under the `M` of `Minecraft`, on the row of the count,
/// not under the count at the far right. The tagline is right-aligned, so that
/// left edge is a computed column, not the block's.
#[test]
fn shelf_version_lines_up_with_the_taglines_left_edge() {
    let app = shelf_app(Page::Overview);
    let text = render_text(&app, 120, 40);
    let tagline = app.i18n.text("shelf.tagline");
    let version = env!("CARGO_PKG_VERSION");

    // Byte offsets are useless here: the header's bottom row opens with block
    // glyphs, three bytes each. Measure the display column instead.
    let column = |line: &str, needle: &str| {
        line.find(needle)
            .map(|at| crate::tui::palette::width(&line[..at]))
    };
    let mut tagline_at = None;
    let mut version_at = None;
    for (index, line) in text.lines().enumerate() {
        if let Some(at) = column(line, &tagline) {
            tagline_at = Some((index, at));
        }
        if let Some(at) = column(line, version) {
            version_at = Some((index, at));
        }
    }
    let (tagline_row, tagline_column) = tagline_at.expect("the tagline is on screen");
    let (version_row, version_column) = version_at.expect("the version is on screen");
    assert_eq!(
        version_column, tagline_column,
        "the version does not start under `Minecraft`:\n{text}"
    );
    assert_eq!(
        version_row,
        tagline_row + 1,
        "the version is not on the count's row:\n{text}"
    );
}

/// The instance `Enter` will start says `当前` instead of its stored status,
/// in the list and in the facts beside it. shelf was the only preset that kept
/// printing `就绪` after the key was pressed, so nothing on the page said the
/// choice had taken.
#[test]
fn shelf_marks_the_current_instance_in_the_list_and_in_the_facts() {
    let app = shelf_app(Page::Instances);
    let text = render_text(&app, 120, 40);
    let current = app.i18n.text("common.current");
    let status = app.i18n.text("task.status");

    // The list row, third line: `Current · 2h` rather than `Ready · 2h`.
    assert!(
        text.contains(&format!(" {current} · ")),
        "the list row still prints the stored status:\n{text}"
    );
    // The facts column of the panel on the right.
    assert!(
        text.contains(&format!("{status}  {current}")),
        "the facts column still prints the stored status:\n{text}"
    );
    // And only for that instance: the others keep the status they have.
    assert!(
        text.contains(" Ready · "),
        "every row was marked current:\n{text}"
    );

    // The overview prints the same fact for the focused instance.
    let overview = render_text(&shelf_app(Page::Overview), 120, 40);
    assert!(
        overview.contains(&format!("{status}  {current}")),
        "the overview still prints the stored status:\n{overview}"
    );
}

/// `主题 = 浅色` swaps shelf's whole sheet, not one accent slot: the page, the
/// panels and the ink all move to the light palette, and the covers still sit
/// on a page of the same design.
#[test]
fn shelf_light_mode_paints_the_light_sheet() {
    use crate::core::config::models::ThemeMode;
    use crate::tui::app::view::shelf::tone;

    let mut app = shelf_app(Page::Instances);
    super::set_theme_mode(&mut app, ThemeMode::Light);
    let buffer = render_buffer(&app, 120, 40);
    let light = tone::palette(
        crate::core::config::models::ShelfTheme::Lavender,
        ThemeMode::Light,
    );

    assert_eq!(buffer[(0, 0)].bg, light.bg, "the page stayed dark");
    assert_eq!(
        buffer[(119, 39)].bg,
        light.bg,
        "the bottom-right corner stayed dark"
    );
    // The selected row is the accent band; its text is `on_accent`.
    let selected = buffer
        .content
        .iter()
        .find(|cell| cell.bg == light.accent)
        .expect("the instance list has a selected row");
    assert_eq!(selected.fg, light.on_accent);
}

/// `书架配色` is a real second axis, not a label: each binding repaints the
/// page, the accent and the cover blocks. A scheme that only swapped the title
/// colour would still pass the palette tests in `tone.rs`.
#[test]
fn shelf_schemes_repaint_the_page_and_the_covers() {
    use crate::core::config::models::{ShelfTheme, ThemeMode};
    use crate::tui::app::view::shelf::tone;

    let mut pages: Vec<ratatui::style::Color> = Vec::new();
    let mut app = shelf_data_app(Page::Instances);
    for theme in ShelfTheme::ALL {
        app.config.state.ui.shelf_theme = theme;
        super::set_theme_mode(&mut app, ThemeMode::Dark);
        let buffer = render_buffer(&app, 120, 40);
        let pal = tone::palette(theme, ThemeMode::Dark);

        assert_eq!(buffer[(0, 0)].bg, pal.bg, "{}: the page", theme.code());
        assert_eq!(
            buffer[(119, 39)].bg,
            pal.bg,
            "{}: the bottom-right corner",
            theme.code()
        );
        // The selected row is the accent band; its text is `on_accent`.
        let selected = buffer
            .content
            .iter()
            .find(|cell| cell.bg == pal.accent)
            .unwrap_or_else(|| panic!("{}: no accent band on the list", theme.code()));
        assert_eq!(selected.fg, pal.on_accent, "{}", theme.code());
        // Covers without a file are drawn from `surface` / `faint` / `bg`, so a
        // sheet that forgot its own faint keeps the previous blocks.
        assert!(
            buffer
                .content
                .iter()
                .any(|cell| cell.bg == pal.faint || cell.bg == pal.surface),
            "{}: nothing in the frame uses the sheet's own surface",
            theme.code()
        );

        assert!(
            !pages.contains(&pal.bg),
            "{} paints a page another scheme already owns",
            theme.code()
        );
        pages.push(pal.bg);
    }
    assert_eq!(pages.len(), ShelfTheme::ALL.len());

    // `跟随终端` is not tied to the shipped sheet: every binding hands the page
    // back, and keeps its own ink while doing it.
    super::set_theme_mode(&mut app, ThemeMode::Terminal);
    for theme in ShelfTheme::ALL {
        app.config.state.ui.shelf_theme = theme;
        let buffer = render_buffer(&app, 120, 40);
        assert_eq!(
            buffer[(0, 0)].bg,
            ratatui::style::Color::Reset,
            "{}: the page stayed opaque under `跟随终端`",
            theme.code()
        );
        let pal = tone::palette(theme, ThemeMode::Terminal);
        assert!(
            buffer.content.iter().any(|cell| cell.fg == pal.text),
            "{}: the terminal sheet lost its text ink",
            theme.code()
        );
        assert!(
            buffer.content.iter().any(|cell| cell.fg == pal.accent),
            "{}: the terminal sheet lost its accent",
            theme.code()
        );
    }
}

/// The binding row is offered to shelf alone, cycles in place and writes
/// through the same path as every other choice row.
#[test]
fn shelf_scheme_row_cycles_and_persists() {
    use crate::core::config::models::{ShelfTheme, UiPreset};
    use crate::tui::pages::settings::{SettingId, SettingsCategory};

    let mut app = App::new();
    app.paths = crate::core::config::DataPaths::from_appdata(
        std::env::temp_dir().join(format!("tmcl-shelf-scheme-{}", Uuid::new_v4())),
    );
    app.paths.ensure_dirs().expect("create data dirs");
    app.config.state.ui.preset = UiPreset::Shelf;
    app.settings.select_category(SettingsCategory::Appearance);

    let index = app
        .setting_ids()
        .iter()
        .position(|id| *id == SettingId::ShelfTheme)
        .expect("shelf exposes the binding row");
    // Directly under `主题`: the two axes are read together.
    assert_eq!(app.setting_ids()[index - 1], SettingId::ThemeMode);

    assert_eq!(
        app.settings_rows()[index].value,
        app.i18n.text("shelf.theme.lavender")
    );
    app.settings.selected = index;
    app.activate_setting(SettingId::ShelfTheme);
    assert_eq!(app.config.state.ui.shelf_theme, ShelfTheme::Moss);
    assert_eq!(
        app.settings_rows()[index].value,
        app.i18n.text("shelf.theme.moss")
    );
    let stored = crate::core::config::load_or_default(&app.paths).expect("reload config");
    assert_eq!(stored.state.ui.shelf_theme, ShelfTheme::Moss);

    // `←` walks back past the first entry to the last one.
    app.settings.selected = index;
    app.adjust_current_setting(-1);
    app.adjust_current_setting(-1);
    assert_eq!(app.config.state.ui.shelf_theme, ShelfTheme::Rose);

    // Other presets never see the row.
    app.config.state.ui.preset = UiPreset::Classic;
    assert!(
        !app.setting_ids().contains(&SettingId::ShelfTheme),
        "classic must not offer a shelf-only row"
    );
}

/// The credential line answers "can this account sign in again?", not "is the
/// cached access token still fresh?".
///
/// A Microsoft access token lasts about a day and the next launch trades the
/// stored refresh token for a new one, so a spent token is not a spent account.
/// The old label printed `已过期` for that state while every launch worked.
#[test]
fn an_expired_access_token_is_not_an_expired_account() {
    use crate::core::account::models::{Account, AccountKind};

    let mut app = shelf_data_app(Page::Accounts);
    let now = crate::tui::app::state::data::unix_seconds();
    let account = |kind, secret: Option<&str>, expires: Option<u64>| Account {
        id: Uuid::new_v4(),
        kind,
        username: "Alex".into(),
        uuid: Uuid::new_v4(),
        skin: None,
        secret_ref: secret.map(str::to_owned),
        access_expires_at: expires,
    };
    app.accounts.accounts = vec![
        // Spent token, refresh token on disk: the launcher renews it silently.
        account(AccountKind::Microsoft, Some("slot"), Some(now - 60)),
        // Nothing left to renew with: this is the only state that needs help.
        account(AccountKind::Microsoft, None, Some(now - 60)),
        account(AccountKind::Offline, None, None),
    ];
    app.accounts.current = Some(app.accounts.accounts[0].id);

    assert_eq!(
        app.account_expiry_label(&app.accounts.accounts[0]),
        app.i18n.text("account.session_renewable")
    );
    assert_eq!(
        app.account_expiry_label(&app.accounts.accounts[1]),
        app.i18n.text("account.session_reauth")
    );
    assert_eq!(
        app.account_expiry_label(&app.accounts.accounts[2]),
        app.i18n.text("account.session_local")
    );

    let text = render_text(&app, 120, 40);
    assert!(
        text.contains(&app.i18n.text("account.session_renewable")),
        "a renewable account must say so:\n{text}"
    );
    assert!(
        !text.contains(&app.i18n.text("account.expired")),
        "no account here is actually expired:\n{text}"
    );
}

/// A launch that retires a credential updates the row at once.
///
/// A rejected refresh token cannot be traded again, so the launch clears it on
/// disk (`Error::AccountExpired` → `retire_credential`). The account page kept
/// printing `自动续期` — a renewal that could never happen — until the next full
/// reload, and that page is exactly where the player goes after a failed
/// launch.
#[test]
fn an_aborted_launch_reloads_the_retired_account() {
    use crate::core::account::models::{Account, AccountKind};
    use crate::core::account::store::AccountStore;
    use crate::tui::event::AppEvent;

    let mut app = shelf_data_app(Page::Accounts);
    std::fs::create_dir_all(&app.paths.root).expect("data root");
    let account = Account {
        id: Uuid::new_v4(),
        kind: AccountKind::Microsoft,
        username: "Donalan".into(),
        uuid: Uuid::new_v4(),
        skin: None,
        secret_ref: Some("slot".into()),
        access_expires_at: Some(crate::tui::app::state::data::unix_seconds() - 60),
    };
    AccountStore::new(&app.paths.accounts_file)
        .save(std::slice::from_ref(&account))
        .expect("seed accounts");
    app.accounts.accounts = vec![account.clone()];
    assert_eq!(
        app.account_expiry_label(&account),
        app.i18n.text("account.session_renewable"),
        "the fixture has to start in the renewable state"
    );

    // What the launch writes once AAD refuses the token.
    let retired = Account {
        secret_ref: None,
        access_expires_at: None,
        ..account
    };
    AccountStore::new(&app.paths.accounts_file)
        .save(std::slice::from_ref(&retired))
        .expect("retire account");

    app.handle(AppEvent::LaunchAborted {
        error: "Microsoft 启动失败：account error: sign in again".into(),
    });

    let row = app.accounts.accounts.first().expect("the account survives");
    assert!(row.secret_ref.is_none(), "the dead reference must be gone");
    assert_eq!(
        app.account_expiry_label(row),
        app.i18n.text("account.session_reauth"),
        "the row must stop promising an automatic renewal"
    );
}

/// A live process is not `就绪`, and the footer must stop reporting the last
/// launch stage once the game is up.
#[test]
fn a_running_instance_is_not_listed_as_ready() {
    use crate::core::launch::process::GameEvent;
    use crate::tui::app::ActiveLaunch;
    use crate::tui::event::AppEvent;

    let mut app = shelf_data_app(Page::Instances);
    let id = app.instances.items[0].id;
    let name = app.instances.items[0].name.clone();
    app.active_launch = Some(ActiveLaunch::starting(id, name.clone()));
    app.sync_launch_status();

    // The facts card is where the status is spelled out; the list row only
    // carries the name, so the label is what to look for.
    let status_label = app.i18n.text("task.status");
    let status_line = |text: &str| -> String {
        text.lines()
            .find(|line| line.contains(&status_label))
            .unwrap_or_default()
            .to_owned()
    };
    let text = render_text(&app, 120, 40);
    assert!(
        status_line(&text).contains(&app.i18n.text("status.launching")),
        "the facts card still says 就绪 while the instance starts:\n{text}"
    );

    app.handle(AppEvent::Game(GameEvent::GameStarted { pid: 4242 }));
    let text = render_text(&app, 120, 40);
    assert!(
        status_line(&text).contains(&app.i18n.text("status.running")),
        "the card does not follow the process into `运行中`:\n{text}"
    );
    assert_eq!(
        app.status.as_str(),
        app.i18n.format("status.game_running", &[("name", &name)]),
        "the footer kept the last launch stage"
    );
    assert!(
        !text.contains(&app.i18n.text("status.launch_stage.verifying_files")),
        "the footer still says it is verifying files:\n{text}"
    );

    // The process ends: the row goes back to what the disk says.
    app.active_launch = None;
    app.sync_launch_status();
    let text = render_text(&app, 120, 40);
    let row = status_line(&text);
    assert!(
        !row.contains(&app.i18n.text("status.running")),
        "a finished launch left `运行中` behind:\n{text}"
    );
    assert!(
        !row.contains(&app.i18n.text("status.launching")),
        "a finished launch left `启动中` behind:\n{text}"
    );
}

/// `跟随终端` hands the page back to the terminal in every preset, not just
/// `classic`. Only the page moves to `Reset`: panels, bands and text keep
/// their colours, otherwise the presets would stop being themselves.
#[test]
fn following_the_terminal_hands_the_page_back_in_every_preset() {
    use crate::core::config::models::{ThemeMode, UiPreset};

    for preset in UiPreset::ALL {
        let mut app = App::new();
        app.config.state.ui.preset = preset;
        super::set_theme_mode(&mut app, ThemeMode::Terminal);
        app.terminal_width = 120;
        app.terminal_height = 40;
        let buffer = render_buffer(&app, 120, 40);
        assert_eq!(
            buffer[(0, 0)].bg,
            ratatui::style::Color::Reset,
            "{preset:?} still paints the page in `跟随终端`"
        );
        // The page alone is not enough: a preset whose body is cards edge to
        // edge would keep most of the screen opaque and look unchanged.
        let transparent = buffer
            .content
            .iter()
            .filter(|cell| cell.bg == ratatui::style::Color::Reset)
            .count();
        assert!(
            transparent * 2 > buffer.content.len(),
            "{preset:?} handed back only {transparent} of {} cells",
            buffer.content.len()
        );
    }
}

/// A row is the bulk of a list page: handing back only the page and the panel
/// tones left every row band opaque. The rows go; the pills and the key caps
/// stay, because a status pill carries a label that has to be read.
/// The contract itself, not a proportion: after a frame, **no cell may still
/// be painted with any colour the preset declared as a container fill**.
///
/// The percentage test below cannot see a single missing layer. Dropping
/// `nightlight`'s `selection` costs 6.9% of the frame and still clears an 88%
/// floor, while a selected row comes back solid.
#[test]
fn every_preset_hands_back_every_container_fill() {
    use crate::core::config::models::{ThemeMode, UiPreset};
    use crate::tui::app::view::sheet::Sheet;

    for preset in UiPreset::ALL {
        let mut app = shelf_data_app(Page::Instances);
        app.config.state.ui.preset = preset;
        super::set_theme_mode(&mut app, ThemeMode::Terminal);
        seed_instances(&mut app);
        let buffer = render_buffer(&app, 120, 40);

        let fills = match preset {
            UiPreset::Classic => [None; 4],
            UiPreset::Nightlight => app.nightlight_palette().fills(),
            UiPreset::Paper => app.paper_palette().fills(),
            UiPreset::Shelf => app.shelf_palette().fills(),
        };
        // The count is pinned rather than derived: a preset that forgets a layer
        // still passes the loop below, because the loop can only check what was
        // declared. Editing a palette's fills means editing this line too, which
        // is the point: it is a contract, not a tautology.
        let expected = match preset {
            UiPreset::Classic => 0,
            UiPreset::Nightlight => 4,
            UiPreset::Paper => 2,
            UiPreset::Shelf => 2,
        };
        let declared = fills.iter().flatten().count();
        assert_eq!(
            declared, expected,
            "{preset:?} declares {declared} container fills, the contract says {expected}"
        );

        for fill in fills.iter().flatten() {
            let painted = buffer
                .content
                .iter()
                .filter(|cell| cell.bg == *fill)
                .count();
            assert_eq!(
                painted, 0,
                "{preset:?} still paints {fill:?} in {painted} cells under 跟随终端"
            );
        }
    }
}

/// Every preset hands its list rows back, not just its page and panels. The rows
/// are what a list page is made of.
///
/// `shelf` keeps 8% on purpose: its selected card is a block of `ACCENT` and
/// its cover square is a picture, and both are content rather than container.
#[test]
fn every_preset_hands_back_its_list_rows() {
    use crate::core::config::models::{ThemeMode, UiPreset};

    for preset in UiPreset::ALL {
        let mut app = shelf_data_app(Page::Instances);
        app.config.state.ui.preset = preset;
        super::set_theme_mode(&mut app, ThemeMode::Terminal);
        seed_instances(&mut app);
        let buffer = render_buffer(&app, 120, 40);
        let reset = buffer
            .content
            .iter()
            .filter(|cell| cell.bg == ratatui::style::Color::Reset)
            .count();
        assert!(
            reset * 100 >= buffer.content.len() * 88,
            "{preset:?} handed back only {reset} of {} cells",
            buffer.content.len()
        );
    }
}

/// Lifting stops at the edge of a pill: text that was chosen against a solid
/// fill must keep the ink that fill was built for.
#[test]
fn lifting_dim_text_skips_glyphs_that_sit_on_a_pill() {
    use crate::core::config::models::{ThemeMode, UiPreset};

    let mut app = shelf_data_app(Page::Instances);
    app.config.state.ui.preset = UiPreset::Nightlight;
    super::set_theme_mode(&mut app, ThemeMode::Terminal);
    seed_instances(&mut app);

    // The other switch off, so the only difference between the two frames is
    // the lift itself.
    app.config.state.ui.lift_dim_text = false;
    let flat = render_buffer(&app, 120, 40);
    app.config.state.ui.lift_dim_text = true;
    let lifted = render_buffer(&app, 120, 40);

    let mut raised = 0;
    for (after, before) in lifted.content.iter().zip(flat.content.iter()) {
        if after.fg == before.fg {
            continue;
        }
        assert_eq!(
            before.bg,
            ratatui::style::Color::Reset,
            "a glyph on a solid fill was lifted: {:?} on {:?}",
            before.symbol(),
            before.bg
        );
        // A hair under 0.30 is the 8-bit quantisation, not a missed lift:
        // the target is hit in linear light and then rounded to a channel.
        assert!(
            crate::tui::palette::relative_luminance(after.fg) >= 0.295,
            "lifted to {after:?}, still under the floor"
        );
        raised += 1;
    }
    assert!(raised > 0, "nothing was lifted; the test proves nothing");
}

/// A dialog must not turn the acrylic off.
///
/// `跟随终端` hands the page's fills back to the terminal at the end of the frame;
/// `Color::Reset` is the only background that lets a window's transparency
/// through. Skipping that pass while an overlay is open turns the whole window
/// solid. The punch runs between the page and the overlays, so the page keeps its
/// acrylic and the dialog keeps its own background.
#[test]
fn a_dialog_keeps_the_page_transparent_under_follow_terminal() {
    use crate::core::config::models::{ThemeMode, UiPreset};
    use crate::tui::modal::{ConfirmAction, Modal};

    let area = Rect::new(0, 0, 120, 40);
    let prompt = "Clear the download cache?";
    let card = Modal::confirm(prompt, ConfirmAction::ClearDownloadCache).area(area);

    for preset in [UiPreset::Nightlight, UiPreset::Paper, UiPreset::Shelf] {
        let mut app = App::new();
        app.config.state.ui.preset = preset;
        super::set_theme_mode(&mut app, ThemeMode::Terminal);
        app.terminal_width = 120;
        app.terminal_height = 40;

        let before = see_through(&render_buffer(&app, 120, 40), area, card);
        app.modal = Some(Modal::confirm(prompt, ConfirmAction::ClearDownloadCache));
        let buffer = render_buffer(&app, 120, 40);
        let after = see_through(&buffer, area, card);

        assert!(
            before > 100,
            "{preset:?}: nothing was see-through to begin with: {before}"
        );
        assert!(
            after * 10 >= before * 9,
            "{preset:?}: the dialog turned the page solid: {after} of {before} cells kept the terminal"
        );

        // The dialog itself stays a slab: its text has to read over whatever is
        // behind the window.
        let solid = (card.top() + 1..card.bottom().saturating_sub(1))
            .flat_map(|y| (card.left() + 1..card.right().saturating_sub(1)).map(move |x| (x, y)))
            .filter(|(x, y)| buffer[(*x, *y)].bg != ratatui::style::Color::Reset)
            .count();
        assert!(
            solid > 20,
            "{preset:?}: the dialog slab went see-through: {solid}"
        );
    }
}

/// Cells whose background is the terminal's, still showing what is behind the
/// window. `card` is the dialog, which is allowed to be solid.
fn see_through(buffer: &ratatui::buffer::Buffer, area: Rect, card: Rect) -> usize {
    (area.top()..area.bottom())
        .flat_map(|y| (area.left()..area.right()).map(move |x| (x, y)))
        .filter(|(x, y)| !card.contains(ratatui::layout::Position::new(*x, *y)))
        .filter(|(x, y)| buffer[(*x, *y)].bg == ratatui::style::Color::Reset)
        .count()
}

/// A light terminal gets the light sheet and still gives its fills up, which is
/// the difference between `Terminal + light` and plain `Light`.
/// The record column prints in the same ink as the list beside it.
///
/// `styled_detail_lines` takes the ink the preset paints a list row with. Handing
/// every value to `Span::raw` leaves the terminal's default foreground, the one
/// colour a preset cannot see, so `shelf`'s light page came out mid grey on white.
/// The sheet's heading ink fixes the contrast but not the page: near-black text
/// beside a list printed in `dim`. This reads the ink back off the frame rather
/// than trusting the call site.
#[test]
fn the_shelf_record_column_prints_in_the_list_ink() {
    use crate::core::config::models::ThemeMode;

    let mut app = shelf_data_app(Page::DownloadCenter);
    app.download_center.category = crate::tui::pages::download_center::DownloadCategory::Resource(
        crate::core::resource::model::ResourceKind::Mod,
    );
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        app.config.state.ui.theme_mode = mode;
        let pal = app.shelf_palette();
        let area = Rect::new(0, 0, app.terminal_width, app.terminal_height);
        let main = app.layout_chrome(area).main;
        let body = crate::tui::app::view::shelf::download_body(main);
        let (_, pane) = app
            .download_detail_areas(body)
            .expect("120 columns keep the record column");

        let backend = TestBackend::new(app.terminal_width, app.terminal_height);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|frame| app.draw(frame)).expect("draw app");
        let buffer = terminal.backend().buffer();

        let (mut dim, mut heading, mut fallback) = (0usize, 0usize, 0usize);
        for y in pane.top()..pane.bottom() {
            for x in pane.left()..pane.right() {
                match buffer[(x, y)].fg {
                    ink if ink == pal.dim => dim += 1,
                    ink if ink == pal.text => heading += 1,
                    ratatui::style::Color::Reset => fallback += 1,
                    _ => {}
                }
            }
        }
        assert!(dim > 20, "{mode:?}: the record printed no values: {dim}");
        assert_eq!(
            heading, 0,
            "{mode:?}: the record is still painted in the heading ink"
        );
        assert_eq!(
            fallback, 0,
            "{mode:?}: a cell fell back to the terminal default"
        );
    }
}

/// `跟随终端` on a light terminal swaps the palette; the shared `Theme` the
/// record painter takes has to follow it, or the labels come from the dark
/// sheet while the page under them is the light one.
#[test]
fn the_shelf_theme_follows_the_sheet_being_painted() {
    use crate::core::config::models::ThemeMode;

    let mut app = shelf_app(Page::Instances);
    super::set_theme_mode(&mut app, ThemeMode::Terminal);
    app.set_terminal_background(Some(true));
    let light = app.shelf_palette();
    assert_eq!(app.shelf_theme().fg, light.text, "light terminal ink");
    assert_eq!(app.shelf_theme().dim, light.dim, "light terminal dim");

    app.set_terminal_background(Some(false));
    assert_eq!(
        app.shelf_theme().fg,
        app.shelf_palette().text,
        "dark terminal"
    );
}

/// The three readability switches only appear while they can change a frame:
/// a toggle that does nothing is worse than no toggle.
#[test]
fn the_terminal_readability_rows_appear_only_under_the_terminal_mode() {
    use crate::core::config::models::ThemeMode;
    use crate::tui::pages::settings::{SettingId, SettingsCategory};

    let mut app = shelf_app(Page::Settings);
    let ids = |app: &App| {
        app.settings_rows_for(SettingsCategory::Appearance)
            .into_iter()
            .map(|row| row.id)
            .collect::<Vec<_>>()
    };

    super::set_theme_mode(&mut app, ThemeMode::Dark);
    let dark = ids(&app);
    for id in [SettingId::LiftDimText, SettingId::DetectTerminal] {
        assert!(
            !dark.contains(&id),
            "{id:?} is offered on a dark sheet, where it cannot change anything"
        );
    }

    super::set_theme_mode(&mut app, ThemeMode::Terminal);
    let terminal = ids(&app);
    for id in [SettingId::LiftDimText, SettingId::DetectTerminal] {
        assert!(terminal.contains(&id), "{id:?} is missing under 跟随终端");
    }
}

/// Each switch is a real toggle, not a label: flipping it changes the value it
/// claims to control.
#[test]
fn the_terminal_readability_switches_toggle_and_persist() {
    use crate::core::config::models::ThemeMode;
    use crate::tui::pages::settings::SettingId;

    let mut app = shelf_app(Page::Settings);
    super::set_theme_mode(&mut app, ThemeMode::Terminal);

    for (id, read) in [
        (
            SettingId::LiftDimText,
            (|app: &App| app.config.state.ui.lift_dim_text) as fn(&App) -> bool,
        ),
        (SettingId::DetectTerminal, |app: &App| {
            app.config.state.ui.detect_terminal
        }),
    ] {
        let before = read(&app);
        app.toggle_setting(id);
        assert_ne!(read(&app), before, "{id:?} did not flip");
        app.toggle_setting(id);
        assert_eq!(read(&app), before, "{id:?} did not flip back");
    }
}

/// `classic` opts out by declaring no fills. Lifting its ink anyway would
/// rewrite the one preset that never asked for any of this.
#[test]
fn classic_ignores_the_lift_switch() {
    use crate::core::config::models::{ThemeMode, UiPreset};

    let mut app = shelf_app(Page::Instances);
    app.config.state.ui.preset = UiPreset::Classic;
    super::set_theme_mode(&mut app, ThemeMode::Terminal);

    app.config.state.ui.lift_dim_text = false;
    let plain = render_buffer(&app, 120, 40);

    app.config.state.ui.lift_dim_text = true;
    let tuned = render_buffer(&app, 120, 40);

    for (after, before) in tuned.content.iter().zip(plain.content.iter()) {
        assert_eq!(
            (after.fg, after.bg),
            (before.fg, before.bg),
            "classic changed at {:?}",
            before.symbol()
        );
    }
}

/// `原版` on screen, `vanilla` on the wire: the picker stores `display|value`
/// and the loader parser reads the half after the bar.
#[test]
fn the_create_form_names_the_loader_and_still_submits_its_code() {
    use crate::core::config::models::Language;

    let mut app = shelf_app(Page::Instances);
    app.i18n = crate::i18n::I18n::new(Language::SimplifiedChinese);
    app.open_create_form_with_version("1.21.4".into());

    let text = render_text(&app, 120, 40).replace(' ', "");
    assert!(
        text.contains("加载器:原版"),
        "the loader field still prints the raw code:\n{text}"
    );
    // The version field's own label leans on the same word.
    assert!(
        !text.contains("Vanilla") && !text.contains("vanilla"),
        "an untranslated `vanilla` is still on screen:\n{text}"
    );

    // The form hands the parser the code, not the label.
    app.open_selection_picker(3);
    let options = match app.modal.as_ref() {
        Some(crate::tui::modal::Modal::Form {
            selection_options, ..
        }) => selection_options[&3].clone(),
        _ => panic!("the loader field lost its picker"),
    };
    assert!(
        options.contains(&"原版|vanilla".to_owned()),
        "the picker offers {options:?}"
    );
    app.create_selection_changed(3);
    assert_eq!(
        app.current_create_loader(),
        Some(("1.21.4".to_owned(), None)),
        "the label did not parse back to vanilla"
    );
}

/// The target picker names the instance. A UUID in that row tells nobody which
/// instance they are picking.
#[tokio::test]
async fn the_download_target_picker_shows_names_and_submits_ids() {
    use crate::core::resource::model::ResourceKind;
    use crate::tui::pages::download_center::DownloadCategory;

    let mut app = shelf_data_app(Page::DownloadCenter);
    app.download_center.category = DownloadCategory::Resource(ResourceKind::Mod);
    app.open_download_target_picker();

    let options = match app.modal.as_ref() {
        Some(crate::tui::modal::Modal::Form {
            selection_options, ..
        }) => selection_options[&0].clone(),
        _ => panic!("no target picker"),
    };
    for (option, item) in options.iter().zip(&app.instances.items) {
        let (display, value) = option.rsplit_once('|').expect("display|id");
        assert_eq!(display, item.name, "the row is not the instance name");
        assert_eq!(value, item.id.to_string(), "the value is not the id");
    }

    let picked = options[1].clone();
    let expected = app.instances.items[1].id;
    app.submit_form(
        crate::tui::modal::FormAction::SelectResourceTarget,
        vec![picked],
    )
    .expect("select target");
    assert_eq!(app.download_target_id, Some(expected));
}

/// A modpack builds an instance of its own: there is no target to compare its
/// files against, so the compatibility column does not apply to it.
#[test]
fn the_modpack_file_table_drops_the_compatibility_column() {
    use crate::core::config::models::Language;
    use crate::core::resource::model::{
        ReleaseChannel, ResourceFile, ResourceKind, ResourceSource,
    };
    use crate::tui::pages::download_center::DownloadCategory;

    let file = |name: &str| ResourceFile {
        source: ResourceSource::Modrinth,
        project_id: "p".into(),
        file_id: format!("f-{name}"),
        name: name.to_owned(),
        download_url: String::new(),
        sha1: None,
        size: Some(2 * 1024 * 1024),
        game_versions: vec!["1.21.4".into()],
        loaders: vec!["fabric".into()],
        channel: ReleaseChannel::Release,
        dependencies: Vec::new(),
    };
    let mut app = shelf_data_app(Page::DownloadCenter);
    app.i18n = crate::i18n::I18n::new(Language::SimplifiedChinese);
    app.resource_detail = Some("p".into());
    app.resource_detail_project = app.resource_projects.first().cloned();
    app.resource_files = vec![file("alpha-1.0.jar")];

    let render = |app: &App| render_text(app, 120, 40).replace(' ', "");

    app.download_center.category = DownloadCategory::Resource(ResourceKind::Modpack);
    let modpack = render(&app);
    assert!(
        !modpack.contains("兼容性"),
        "the modpack list still prints a compatibility column:\n{modpack}"
    );
    assert!(
        modpack.contains("发布类型"),
        "the channel column lost its header:\n{modpack}"
    );

    app.download_center.category = DownloadCategory::Resource(ResourceKind::Mod);
    let module = render(&app);
    assert!(
        module.contains("兼容性"),
        "a mod list without a target has nothing to compare, but the column is\
         still the reason the table exists:\n{module}"
    );
    assert!(
        !module.contains("频道"),
        "`频道` reads as a TV channel; `发布类型` is what the column means:\n{module}"
    );
}

/// A confirm prompt wraps instead of being cut. Truncated to one line, a
/// destructive action hides exactly the path or name the reader is being asked
/// about.
#[test]
fn a_long_confirm_prompt_wraps_instead_of_being_cut() {
    use crate::core::config::models::{Language, UiPreset};

    for preset in UiPreset::ALL {
        let mut app = shelf_data_app(Page::Settings);
        app.config.state.ui.preset = preset;
        app.i18n = crate::i18n::I18n::new(Language::SimplifiedChinese);
        app.open_add_to_path_confirmation();

        // 108 columns is where the report came from: the dialog is 76 wide and
        // this sentence is far longer than that.
        let flat = render_text(&app, 108, 40).replace([' ', '\n'], "");
        assert!(
            flat.contains("新开的终端才会生效"),
            "{preset:?} lost the tail of the prompt:\n{flat}"
        );
    }
}

/// A verdict with one finding, enough to exercise every section of the card.
fn sample_diagnosis() -> crate::core::diagnose::Diagnosis {
    use crate::core::diagnose::finding::{EvidenceLine, Finding};
    use crate::core::diagnose::{Diagnosis, ExitKind, Facts, LaunchStage, RuleId, Source};

    Diagnosis {
        stage: LaunchStage::InGame,
        exit: ExitKind::Crashed,
        primary: Some(Finding {
            rule: RuleId::OutOfMemory,
            captures: Vec::new(),
            evidence: vec![EvidenceLine {
                line: 3,
                text: "[stdout] java.lang.OutOfMemoryError: Java heap space".to_owned(),
            }],
            score: 53,
        }),
        others: Vec::new(),
        keywords: Vec::new(),
        facts: Facts {
            instance_name: "survival".to_owned(),
            minecraft_version: "1.21.1".to_owned(),
            loader: "fabric 0.16.9".to_owned(),
            ..Facts::default()
        },
        sources: vec![Source {
            path: "game.log".to_owned(),
            present: true,
            bytes: 1024,
        }],
        unknown: false,
    }
}

/// The verdict lives in the body, not in the border.
///
/// A sentence is wider than a border title, so the copy on the border arrives cut
/// in half (`…IFakeIResourceManager$V1，通…`) while the whole of it sits ten rows
/// below.
#[test]
fn the_card_title_carries_the_name_and_not_the_verdict() {
    let mut app = App::new();
    app.terminal_width = 100;
    app.terminal_height = 30;
    app.diagnosis = Some(sample_diagnosis());
    app.diagnosis_open = true;

    let text = render_text(&app, 100, 30);
    let title = app.i18n.text("diagnose.title");
    let row = text
        .lines()
        .find(|row| row.contains(&title))
        .unwrap_or_else(|| panic!("no title row in:\n{text}"));
    let verdict = app.i18n.text("diagnose.reason.out_of_memory");
    assert!(
        !row.contains(&verdict),
        "the verdict is printed twice: {row:?}"
    );
}

/// The stage and the facts have to survive the trip through the border.
///
/// A title centred over right-aligned facts meets them in the middle on a
/// 72-column card, and the facts lose their head: `未进游戏 · 1.20.1 · Forge
/// 47.4.10` arrives as `戏 · 1.20.1 · …`.
#[test]
fn the_card_title_keeps_the_stage_and_the_facts_whole() {
    let mut app = App::new();
    app.diagnosis = Some(sample_diagnosis());
    app.diagnosis_open = true;

    let text = render_text(&app, 120, 40);
    let title = app.i18n.text("diagnose.title");
    let row = text
        .lines()
        .find(|row| row.contains(&title))
        .unwrap_or_else(|| panic!("no title row in:\n{text}"));
    let context = app.diagnosis_context();
    assert!(
        row.contains(&context),
        "the facts were clipped: {row:?}\nwanted: {context:?}"
    );
}

/// The card is shared by five presets; none of them may draw it off-screen or
/// print a raw key when a translation is missing.
#[test]
fn every_preset_draws_the_diagnosis_card_inside_the_screen() {
    use crate::core::config::models::UiPreset;

    for preset in [
        UiPreset::Classic,
        UiPreset::Nightlight,
        UiPreset::Paper,
        UiPreset::Shelf,
    ] {
        for (width, height) in [(120u16, 40u16), (80, 24)] {
            let mut app = App::new();
            app.page = Page::Logs;
            app.config.state.ui.preset = preset;
            app.motion = false;
            app.terminal_width = width;
            app.terminal_height = height;
            app.diagnosis = Some(sample_diagnosis());
            app.diagnosis_open = true;

            let text = render_text(&app, width, height);
            assert!(
                text.contains(&app.i18n.text("diagnose.section.conclusion")),
                "{preset:?} at {width}x{height} did not draw the card:/n{text}"
            );
            assert_eq!(
                raw_key_in(&text),
                None,
                "{preset:?} at {width}x{height} printed a raw key:\n{text}"
            );
            // The remedy row carries the cap that runs it. OutOfMemory's only
            // remedy is "raise the memory", whose cap is `M`.
            assert!(
                text.contains("[M]"),
                "{preset:?} at {width}x{height} did not print the remedy cap:\n{text}"
            );

            let rect = app.diagnosis_rect(Rect::new(0, 0, width, height));
            assert!(
                rect.right() <= width && rect.bottom() <= height,
                "{preset:?} at {width}x{height}: card {rect:?} escapes the screen"
            );
            assert!(rect.width >= 20 && rect.height >= 5);
        }
    }
}

/// `F3` is the only opener, and it must not react when there is nothing to show.
#[test]
fn f3_is_wired_and_the_card_swallows_q() {
    use crate::tui::command::{Command, Mode, from_key};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
    assert_eq!(
        from_key(Mode::Normal, Page::Logs, key(KeyCode::F(3))),
        Command::Diagnose
    );
    assert_eq!(
        from_key(Mode::Diagnosis, Page::Logs, key(KeyCode::Char('q'))),
        Command::Nothing,
        "q would quit the launcher, not just close the card"
    );
    assert_eq!(
        from_key(Mode::Diagnosis, Page::Logs, key(KeyCode::Esc)),
        Command::Back
    );
    assert_eq!(
        from_key(Mode::Diagnosis, Page::Logs, key(KeyCode::F(1))),
        Command::Help,
        "the card prints F1 as help, so F1 must open it"
    );

    let mut app = App::new();
    app.toggle_diagnosis();
    assert!(!app.diagnosis_open, "no verdict, nothing to open");

    app.diagnosis = Some(sample_diagnosis());
    app.toggle_diagnosis();
    assert!(app.diagnosis_open);
    app.toggle_diagnosis();
    assert!(!app.diagnosis_open);
}

/// The key row is chrome: it has to sit on the last body row whatever the
/// content height, and the text viewport must stop above it.
#[test]
fn the_key_row_sits_on_the_last_body_row() {
    let (width, height) = (120u16, 40u16);
    let mut app = App::new();
    app.page = Page::Logs;
    app.terminal_width = width;
    app.terminal_height = height;
    app.diagnosis = Some(sample_diagnosis());
    app.diagnosis_open = true;

    let area = Rect::new(0, 0, width, height);
    let layout = app.diagnosis_layout(area);
    assert_eq!(
        layout.text.bottom(),
        layout.body.bottom() - 1,
        "content must stop one row above the key row"
    );

    let text = render_text(&app, width, height);
    let rows = text.lines().collect::<Vec<_>>();
    let key_row = rows[usize::from(layout.body.bottom() - 1)];
    let keys = app.i18n.text("diagnose.keys");
    let prefix = keys.chars().take(12).collect::<String>();
    assert!(
        key_row.contains(&prefix),
        "no key row at row {}: {key_row:?}",
        layout.body.bottom() - 1
    );
    let above = rows[usize::from(layout.body.bottom() - 2)];
    assert!(
        !above.contains(&prefix),
        "the key row also leaked into the content"
    );
}

/// Where the card's ghosting does *not* come from.
///
/// A player reported fragments of the previous line left inside the card while
/// scrolling (a `toms` in the middle of the evidence, tails of long lines at
/// the right edge); resizing the window cleared them. This test rules out the
/// card itself: every frame is a complete buffer, and the diff emits every cell
/// whose content changed, so the artefacts are the terminal's to explain. It is
/// here so the next investigation starts from that fact instead of re-deriving
/// it.
#[test]
fn the_card_diff_repaints_every_changed_cell() {
    use crate::core::diagnose::Source;

    let mut app = App::new();
    app.config.state.ui.theme_mode = crate::core::config::models::ThemeMode::Terminal;
    app.terminal_width = 100;
    app.terminal_height = 30;
    let mut diagnosis = sample_diagnosis();
    // A tab, because the loader's own evidence line has one, plus enough
    // sources to make the card scroll.
    diagnosis.primary.as_mut().expect("primary").evidence = vec![
        crate::core::diagnose::finding::EvidenceLine {
            line: 3,
            text: "[stdout] \tMod ID: 'exposure', Requested by: 'exposure_camcorder'".to_owned(),
        },
        crate::core::diagnose::finding::EvidenceLine {
            line: 4,
            text: "[stdout] Expected range: '[1.9,)', Actual version: '[MISSING]'".to_owned(),
        },
    ];
    diagnosis.sources = (0..40)
        .map(|index| Source {
            path: format!("source-{index}.log"),
            present: true,
            bytes: 1024,
        })
        .collect();
    app.diagnosis = Some(diagnosis);
    app.diagnosis_open = true;

    // Every frame starts from a clean buffer, so two fresh renders are exactly
    // the two buffers `Terminal::draw` diffs against each other.
    let mut before = render_buffer(&app, 100, 30);
    let max_scroll = app.diagnosis_max_scroll();
    assert!(max_scroll > 4, "the fixture must scroll: {max_scroll}");
    for offset in 1..=max_scroll {
        app.scroll_diagnosis(1);
        assert_eq!(app.diagnosis_scroll, offset);
        let after = render_buffer(&app, 100, 30);
        let painted: std::collections::HashSet<(u16, u16)> = after
            .diff(&before)
            .iter()
            .map(|(x, y, _)| (*x, *y))
            .collect();
        let lost: Vec<String> = (0..30u16)
            .flat_map(|y| (0..100u16).map(move |x| (x, y)))
            .filter(|&(x, y)| after[(x, y)] != before[(x, y)] && !painted.contains(&(x, y)))
            .map(|(x, y)| {
                format!(
                    "({x},{y}) {:?} -> {:?}",
                    before[(x, y)].symbol(),
                    after[(x, y)].symbol()
                )
            })
            .collect();
        assert!(
            lost.is_empty(),
            "scroll {offset}: {} cells changed but were never repainted: {lost:?}",
            lost.len()
        );
        before = after;
    }

    assert_eq!(app.diagnosis_scroll, max_scroll);
}

/// No cell of the card may hold a control character.
///
/// `Buffer::set_stringn` filters them and `Paragraph` does not, so a tab is
/// stored as a cell symbol and printed as a tab stop. The rest of the row is
/// then drawn outside the card's border, in cells no later frame repaints.
#[test]
fn the_card_prints_no_control_characters() {
    use crate::core::diagnose::finding::EvidenceLine;

    let mut app = App::new();
    app.terminal_width = 131;
    app.terminal_height = 38;
    let mut diagnosis = sample_diagnosis();
    diagnosis.primary.as_mut().expect("primary").evidence = vec![
        EvidenceLine {
            line: 1,
            text: "[stdout] [14:54:39] [main/ERROR] [ne.mi.fm.lo.ModSorter/LOADING]: Missing or unsupported mandatory dependencies:".to_owned(),
        },
        EvidenceLine {
            line: 2,
            text: "[stdout] \tMod ID: 'exposure', Requested by: 'exposure_camcorder', Expected range: '[1.9,)', Actual version: '[MISSING]'"
                .to_owned(),
        },
    ];
    app.diagnosis = Some(diagnosis);
    app.diagnosis_open = true;

    let buffer = render_buffer(&app, 131, 38);
    let rect = app.diagnosis_rect(Rect::new(0, 0, 131, 38));
    let mut offenders = Vec::new();
    for y in rect.top()..rect.bottom() {
        for x in rect.left()..rect.right() {
            let symbol = buffer[(x, y)].symbol();
            if symbol.chars().any(char::is_control) {
                offenders.push(format!("({x},{y}) {symbol:?}"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "the card hands control characters to the terminal: {offenders:?}"
    );
}

/// Closing the card is not navigation: the page underneath stays put.
#[test]
fn escape_closes_the_card_without_leaving_the_page() {
    let mut app = App::new();
    app.page = Page::Logs;
    app.diagnosis = Some(sample_diagnosis());
    app.diagnosis_open = true;
    app.go_back();
    assert!(!app.diagnosis_open);
    assert_eq!(app.page, Page::Logs);
}

/// The arrows belong to the card while it is up: the list behind it must not
/// move, or the player scrolls two things at once.
#[test]
fn the_up_key_scrolls_the_card_not_the_list_behind_it() {
    use crate::core::diagnose::Source;
    use crate::tui::command::Command;

    let mut app = App::new();
    app.page = Page::Instances;
    app.terminal_width = 80;
    app.terminal_height = 24;
    app.instances.focused = 3;

    let mut diagnosis = sample_diagnosis();
    diagnosis.sources = (0..40)
        .map(|index| Source {
            path: format!("source-{index}.log"),
            present: true,
            bytes: 1024,
        })
        .collect();
    app.diagnosis = Some(diagnosis);
    app.diagnosis_open = true;
    app.diagnosis_scroll = app.diagnosis_max_scroll();
    assert!(
        app.diagnosis_scroll > 0,
        "the fixture must overflow the card"
    );

    app.run_command(Command::PrevRow);
    assert_eq!(app.diagnosis_scroll, app.diagnosis_max_scroll() - 1);
    assert_eq!(app.instances.focused, 3, "the instance list did not move");

    app.run_command(Command::NextRow);
    assert_eq!(app.diagnosis_scroll, app.diagnosis_max_scroll());
    assert_eq!(app.instances.focused, 3);
}

/// The log page shows `诊断：<cause>` when a verdict exists and nothing when it
/// does not. Three presets draw it in their log panel; `paper` and `shelf` have
/// no bottom bar there and leave the verdict to the status bar and `F3`, so for
/// them only the empty state is asserted.
#[test]
fn the_log_page_badge_appears_only_with_a_verdict() {
    use crate::core::config::models::UiPreset;

    for preset in [
        UiPreset::Classic,
        UiPreset::Nightlight,
        UiPreset::Paper,
        UiPreset::Shelf,
    ] {
        let mut app = App::new();
        app.page = Page::Logs;
        app.config.state.ui.preset = preset;
        app.motion = false;
        app.terminal_width = 120;
        app.terminal_height = 40;

        let empty = render_text(&app, 120, 40);
        assert!(
            !empty.contains(&app.i18n.text("diagnose.badge").replace("{reason}", "")),
            "{preset:?} printed a badge with no verdict"
        );

        app.diagnosis = Some(sample_diagnosis());
        let badge = app.diagnosis_badge().expect("badge");
        let with_verdict = render_text(&app, 120, 40);
        if matches!(preset, UiPreset::Paper | UiPreset::Shelf) {
            assert!(
                !with_verdict.contains(&badge),
                "{preset:?} has no log panel bar; the badge should not appear there:\n{with_verdict}"
            );
        } else {
            assert!(
                with_verdict.contains(&badge),
                "{preset:?} lost the log-page badge:\n{with_verdict}"
            );
        }
    }
}
