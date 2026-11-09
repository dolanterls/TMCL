//! `paper` preset: the launcher as a printed page.
//!
//! No cards and no filled rows: sections are separated by hairline rules and the
//! selection is a margin bar plus an underline, distinct from `classic`'s framed
//! tables and `shelf`'s rounded covers. Pages are drawn here; the instance
//! workspace lives in `view/workspace.rs` for classic, paper and shelf.

use super::*;
use crate::tui::action::Action;
use crate::tui::hit::Layer;
use crate::tui::palette;
use crate::tui::shell::ChromeRegions;

mod accounts;
mod detail;
mod downloads;
mod help;
mod instances;
pub(crate) mod kit;
mod logs;
mod overview;
mod settings;
mod tasks;
mod tone;

pub(crate) use tone::PaperPalette;

impl App {
    pub(in crate::tui::app) fn draw_paper(&self, frame: &mut Frame<'_>, area: Rect) {
        let pal = self.paper_palette();
        kit::sheet(frame, area, &pal);
        let regions = self.layout_chrome(area);
        self.regions.set(regions);
        self.regions_size.set((area.width, area.height));
        if regions.mode == LayoutMode::TooSmall {
            self.paper_too_small(frame, area, &pal);
            return;
        }

        self.paper_head(frame, &regions, &pal);
        self.paper_chapters(frame, &regions, &pal);
        match self.page {
            Page::Overview => self.draw_paper_overview(frame, regions.main, &pal),
            Page::Instances => {
                if self.workspace_is_open() {
                    self.draw_paper_workspace(frame, regions.main);
                } else {
                    self.draw_paper_instances(frame, regions.main, &pal);
                }
            }
            Page::DownloadCenter => self.draw_paper_downloads(frame, regions.main, &pal),
            Page::Tasks => self.draw_paper_tasks(frame, regions.main, &pal),
            Page::Accounts => self.draw_paper_accounts(frame, regions.main, &pal),
            Page::Logs => self.draw_paper_logs(frame, regions.main, &pal),
            Page::Settings => self.draw_paper_settings(frame, regions.main, &pal),
        }
        self.paper_footer(frame, &regions, &pal);
        repaint_sheet(frame, area, &pal);
    }

    /// Running head: the brand and where this page sits in the sequence.
    ///
    /// The chapter row already says which page is open (it is the underlined
    /// word), so repeating the title here would print it twice. The title page
    /// leaves the brand out as well: it carries the wordmark itself.
    fn paper_head(&self, frame: &mut Frame<'_>, regions: &ChromeRegions, pal: &PaperPalette) {
        let position = tab_index(self.page).unwrap_or(0) + 1;
        if self.page != Page::Overview {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "TMCL",
                    ink(pal, pal.ink).add_modifier(Modifier::BOLD),
                ))),
                regions.header,
            );
        }
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("{position} / {}", PAGES.len()),
                ink(pal, pal.faint),
            )))
            .alignment(Alignment::Right),
            regions.header,
        );
        kit::rule(
            frame,
            Rect::new(
                regions.header.x,
                regions.header.y.saturating_add(1),
                regions.header.width,
                1,
            ),
            pal,
            pal.rule,
        );
    }

    /// Chapter row: the page list, underlined where the reader is.
    fn paper_chapters(&self, frame: &mut Frame<'_>, regions: &ChromeRegions, pal: &PaperPalette) {
        let labels = self.tab_labels(regions.mode);
        let active = tab_index(self.page);
        let mut x = regions.tabs.x;
        for (index, label) in labels.iter().enumerate() {
            let width = palette::width(label) as u16;
            let rect = Rect::new(x, regions.tabs.y, width, 1);
            if rect.right() > regions.tabs.right() {
                break;
            }
            let style = if Some(index) == active {
                ink(pal, pal.accent).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                ink(pal, pal.dim)
            };
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(label.clone(), style))),
                rect,
            );
            if let Some(page) = PAGES.get(index) {
                self.hitmap.add(rect, Action::Nav(*page));
            }
            x = x.saturating_add(width).saturating_add(3);
        }
    }

    /// Three rows: the rule that closes the page, the keys, the status.
    fn paper_footer(&self, frame: &mut Frame<'_>, regions: &ChromeRegions, pal: &PaperPalette) {
        let footer = regions.footer;
        if footer.height == 0 {
            return;
        }
        kit::rule(
            frame,
            Rect::new(footer.x, footer.y, footer.width, 1),
            pal,
            pal.rule,
        );
        if footer.height >= 2 {
            frame.render_widget(
                Paragraph::new(self.paper_hint_line(footer.width, pal)),
                Rect::new(footer.x, footer.y.saturating_add(1), footer.width, 1),
            );
        }
        if footer.height >= 3 {
            frame.render_widget(
                Paragraph::new(self.paper_status_line(pal)),
                Rect::new(footer.x, footer.y.saturating_add(2), footer.width, 1),
            );
        }
    }

    /// The keys as a sentence — wrapped? no: dropped by priority, like every
    /// other footer.
    ///
    /// This line used to stop at the page edge, which reads as a sentence that
    /// simply ends: `?`, `Tab` and `q` were gone from a 120-column page and
    /// nothing said so.
    fn paper_hint_line(&self, width: u16, pal: &PaperPalette) -> Line<'static> {
        let hints = self.hints();
        let fitted = crate::tui::chrome::fitted_hints(&hints, |items| {
            self.paper_hint_spans(items, pal).width() <= usize::from(width)
        });
        self.paper_hint_spans(&fitted, pal)
    }

    fn paper_hint_spans(
        &self,
        hints: &[&crate::tui::chrome::Hint],
        pal: &PaperPalette,
    ) -> Line<'static> {
        let mut spans = Vec::new();
        for hint in hints {
            if !spans.is_empty() {
                spans.push(Span::styled("   ", ink(pal, pal.faint)));
            }
            spans.push(Span::styled(hint.key.clone(), ink(pal, pal.accent)));
            spans.push(Span::styled(
                format!(" {}", self.i18n.text_or_raw(&hint.action)),
                ink(pal, pal.dim),
            ));
        }
        Line::from(spans)
    }

    fn paper_status_line(&self, pal: &PaperPalette) -> Line<'static> {
        let color = match self.status.kind() {
            StatusKind::Warning => pal.warn,
            StatusKind::Failed => pal.accent,
            _ => pal.faint,
        };
        Line::from(Span::styled(
            self.status.as_str().to_owned(),
            ink(pal, color),
        ))
    }

    /// Overlays are still classic's: a dimmed page under a dark dialog reads as
    /// a sheet in shadow, and keeps every action reachable while the two pages
    /// are being judged.
    pub(in crate::tui::app) fn paper_overlays(&self, frame: &mut Frame<'_>, area: Rect) {
        let pal = self.paper_palette();
        if self.detail_open || self.diagnosis_open || self.help_open || self.modal.is_some() {
            crate::tui::widgets::scrim::dim(frame);
        }
        self.register_overlay_hits(area);
        if self.detail_open {
            self.draw_paper_detail(frame, &pal);
        }
        if self.diagnosis_open {
            self.draw_diagnosis_overlay_with(frame, self.diagnosis_theme());
        }
        if self.help_open {
            self.draw_paper_help(frame, &pal);
        }
        if let Some(modal) = &self.modal {
            self.hitmap.set_layer(Layer::Modal);
            modal.draw_paper(frame, &pal, &self.i18n, self.anim_time(), &self.hitmap);
            self.hitmap.set_layer(Layer::Base);
        }
        self.paint_hover(frame);
    }

    fn paper_too_small(&self, frame: &mut Frame<'_>, area: Rect, pal: &PaperPalette) {
        let row = Rect::new(
            area.x,
            area.y.saturating_add(area.height / 2),
            area.width,
            1,
        );
        frame.render_widget(
            Paragraph::new(self.i18n.text("layout.too_small"))
                .alignment(Alignment::Center)
                .style(ink(pal, pal.dim)),
            row,
        );
    }
}

/// Style for ordinary page text.
pub(in crate::tui::app) fn ink(pal: &PaperPalette, color: Color) -> Style {
    Style::default().fg(color).bg(pal.bg)
}

/// Style for text printed on a slip (dialogs, the detail sheet, the help card).
///
/// `ink` carries the page's background, so using it on a slip would punch a
/// stripe of page through the sheet.
pub(in crate::tui::app) fn sheet_ink(pal: &PaperPalette, color: Color) -> Style {
    Style::default().fg(color).bg(pal.sheet)
}

/// Style for the selected row's text: ink, bold, underlined.
pub(in crate::tui::app) fn ink_marked(pal: &PaperPalette) -> Style {
    Style::default()
        .fg(pal.ink)
        .bg(pal.bg)
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
}

/// Paints the page back behind every cell the text left transparent.
///
/// The text renderer resets the trailing cell of a wide character, so a CJK
/// glyph leaves one `Reset` cell behind it. On the dark presets that is
/// invisible; on a light page it is a white notch in the middle of a word, and
/// on a dark sheet it is a black one. Only `Reset` cells are touched, so hover
/// tints and overlays keep what they set.
pub(in crate::tui::app) fn repaint_sheet(frame: &mut Frame<'_>, area: Rect, pal: &PaperPalette) {
    let buffer = frame.buffer_mut();
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            if let Some(cell) = buffer.cell_mut((x, y))
                && cell.bg == Color::Reset
            {
                cell.bg = pal.bg;
            }
        }
    }
}

impl App {
    /// The right-hand scrollbar for a region that scrolls, drawn on `bg`.
    ///
    /// Track in `faint`, thumb in `accent`, no arrow caps. One implementation
    /// for the whole preset so the download column and the help card signal
    /// "more below" the same way. `bg` is the surface being scrolled, the page
    /// or a slip, so the track never shows a different paper through the text.
    pub(in crate::tui::app) fn paper_scrollbar(
        &self,
        frame: &mut Frame<'_>,
        body: Rect,
        total: usize,
        scroll: usize,
        pal: &PaperPalette,
        bg: Color,
    ) {
        margin_scrollbar(frame, body, total, scroll, pal.faint, pal.accent, bg);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::core::config::models::{PaperTheme, ThemeMode, UiPreset};
    use crate::tui::pages::instances::InstanceSummary;

    fn instance(
        name: &str,
        version: &str,
        loader: &str,
        status: &str,
        played: u64,
    ) -> InstanceSummary {
        InstanceSummary {
            id: uuid::Uuid::new_v4(),
            name: name.into(),
            version: version.into(),
            loader: loader.into(),
            loader_version: None,
            status: status.into(),
            path: std::path::PathBuf::from(format!("C:/games/versions/{name}")),
            launch_count: 1,
            play_time_seconds: played,
        }
    }

    fn paper_app(count: usize) -> App {
        let mut app = App::new();
        app.config.state.ui.preset = UiPreset::Paper;
        // The prototype's assertions describe the day sheet. `ThemeMode`
        // defaults to `Dark`, so without this the same frame would be the night
        // print and the assertions would quietly stop testing the light one.
        app.config.state.ui.theme_mode = ThemeMode::Light;
        app.page = Page::Instances;
        app.terminal_width = 120;
        app.terminal_height = 40;
        app.instances.apply(
            (0..count)
                .map(|index| {
                    instance(
                        &format!("instance-{index}"),
                        "1.20.1",
                        "Forge 47.4.22",
                        "ready",
                        3600,
                    )
                })
                .collect(),
        );
        app
    }

    /// Draws the frame and returns it as rows of text plus the raw buffer.
    fn screen(app: &mut App, width: u16, height: u16) -> (Vec<String>, ratatui::buffer::Buffer) {
        // Keep `App`'s idea of the terminal in step with the backend: several
        // layout decisions read it instead of the frame area.
        app.terminal_width = width;
        app.terminal_height = height;
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|frame| app.draw(frame)).expect("draw paper");
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

    #[test]
    fn the_instances_page_is_a_light_page_with_a_margin_bar() {
        let mut app = paper_app(19);
        // The prototype is aimed at the Chinese interface; widths are the risk.
        app.i18n = crate::i18n::I18n::new(crate::core::config::models::Language::SimplifiedChinese);
        app.instances.focused = 3;
        let pal = app.paper_palette();
        let (rows, buffer) = screen(&mut app, 120, 40);

        let text = rows.join("\n");
        assert!(text.contains("TMCL"), "{text}");
        assert!(text.contains("2 / 7"), "page position missing: {text}");
        assert!(text.contains("instance-3"), "{text}");
        // The selection sits in the margin, not in a filled row.
        let selected_row = rows
            .iter()
            .find(|row| row.contains('▌'))
            .expect("the selected row carries a margin bar");
        assert!(
            selected_row.contains("instance-3"),
            "margin bar is not next to the selection: {selected_row}"
        );
        // Every cell of the frame is painted, so nothing falls back to the
        // terminal's own background. A hole is invisible on the dark presets and
        // conspicuous on this one, so it is asserted rather than eyeballed. The
        // trailing cell of a wide glyph never reaches the buffer and is skipped.
        for y in 0..40u16 {
            for x in 0..120u16 {
                let continuation = x > 0 && Line::from(buffer[(x - 1, y)].symbol()).width() > 1;
                if continuation {
                    continue;
                }
                assert_eq!(buffer[(x, y)].bg, pal.bg, "unpainted cell at ({x}, {y})");
            }
        }
        // The two signature moves: ink on the selected row, accent on the
        // current chapter.
        let name_byte = selected_row.find("instance-3").expect("name column");
        let name_x = palette::width(&selected_row[..name_byte]) as u16;
        let selected_y = rows
            .iter()
            .position(|row| row.contains('▌'))
            .expect("selected row") as u16;
        let name_cell = &buffer[(name_x, selected_y)];
        assert_eq!(name_cell.fg, pal.ink);
        assert!(
            name_cell
                .modifier
                .contains(ratatui::style::Modifier::UNDERLINED),
            "the selected name is underlined"
        );
    }

    #[test]
    fn the_overview_page_prints_the_current_instance_and_figures() {
        let mut app = paper_app(4);
        app.page = Page::Overview;
        app.instances
            .set_current(app.instances.items.first().map(|item| item.id));
        let (rows, _) = screen(&mut app, 120, 40);

        let text = rows.join("\n");
        assert!(
            text.contains('█'),
            "the wordmark is set in block type:\n{text}"
        );
        assert!(text.contains("instance-0"), "{text}");
        assert!(
            text.contains("4 个实例") || text.contains("4 instances"),
            "{text}"
        );

        // The masthead line carries the date, not the game directory: the
        // reference sets `2026-11-02 13:37` next to the runtime.
        let facts = rows
            .iter()
            .find(|row| row.contains("instances") || row.contains("个实例"))
            .expect("the masthead line");
        assert!(
            facts.contains('-') && facts.contains(':'),
            "masthead date missing: {facts:?}"
        );

        // Figures print one over the other: number row, then labels, with
        // hairlines in the gutters.
        let labels = rows
            .iter()
            .position(|row| row.contains("Running tasks") || row.contains("运行中任务"))
            .expect("the figures carry their own labels");
        let numbers = &rows[labels - 1];
        assert!(
            numbers.contains('│'),
            "columns are separated by rules: {numbers:?}"
        );
        assert_eq!(numbers.split_whitespace().next(), Some("4"));
    }

    /// Every visible cell carries the sheet, except the trailing cell of a wide
    /// glyph (the renderer never diffs it).
    fn assert_sheet(buffer: &ratatui::buffer::Buffer, width: u16, height: u16, bg: Color) {
        for y in 0..height {
            for x in 0..width {
                let continuation = x > 0 && Line::from(buffer[(x - 1, y)].symbol()).width() > 1;
                if continuation {
                    continue;
                }
                assert_eq!(buffer[(x, y)].bg, bg, "unpainted cell at ({x}, {y})");
            }
        }
    }

    #[test]
    fn every_page_is_painted_with_the_paper() {
        use crate::core::config::models::{PaperTheme, ThemeMode};
        // Seven pages, three stocks, two day sheets. The counter is asserted at
        // the end: a loop that silently shrinks back to one axis is exactly how
        // this test passed as "42" while only checking seven.
        let mut seen = 0;
        for theme in PaperTheme::ALL {
            for mode in [ThemeMode::Light, ThemeMode::Dark] {
                for (index, page) in Page::ALL.into_iter().enumerate() {
                    let mut app = paper_app(3);
                    app.page = page;
                    app.config.state.ui.paper_theme = theme;
                    app.config.state.ui.theme_mode = mode;
                    let pal = app.paper_palette();
                    let (rows, buffer) = screen(&mut app, 120, 40);
                    assert_sheet(&buffer, 120, 40, pal.bg);
                    let position = format!("{} / 7", index + 1);
                    assert!(
                        rows.join("\n").contains(&position),
                        "{page:?} on {theme:?}/{mode:?}: the running head lost {position}"
                    );
                    // The key band is part of every page; a page that returns
                    // no hints would leave the reader without a way out.
                    let footer = app.layout_chrome(Rect::new(0, 0, 120, 40)).footer;
                    let band = &rows[usize::from(footer.y).saturating_add(1)];
                    let first = app
                        .hints()
                        .first()
                        .map(|hint| hint.key.clone())
                        .expect("every page offers at least one key");
                    assert!(
                        band.contains(&first),
                        "{page:?} on {theme:?}/{mode:?}: the key band lost {first}"
                    );
                    seen += 1;
                }
            }
        }
        assert_eq!(seen, 42, "the page matrix shrank");
    }

    #[test]
    fn the_layout_survives_narrow_and_short_terminals() {
        let mut seen = 0;
        for (width, height) in [(120, 40), (80, 24), (60, 16), (56, 14)] {
            for page in [Page::Overview, Page::Instances, Page::Settings] {
                let mut app = paper_app(6);
                app.page = page;
                let (rows, _) = screen(&mut app, width, height);
                let text = rows.join("\n");
                assert!(
                    text.contains("/ 7"),
                    "{width}x{height} {page:?} lost the running head:\n{text}"
                );
                seen += 1;
            }
        }
        assert_eq!(seen, 12, "the size matrix shrank");
    }

    #[test]
    fn the_workspace_is_drawn_on_the_paper() {
        let mut app = paper_app(4);
        let summary = app.instances.items[0].clone();
        app.instances.set_current(Some(summary.id));
        app.workspace = Some(crate::tui::pages::workspace::Workspace::open(&summary));
        assert!(app.workspace_is_open());

        let pal = app.paper_palette();
        let (rows, buffer) = screen(&mut app, 120, 40);
        assert_sheet(&buffer, 120, 40, pal.bg);
        let text = rows.join("\n");
        assert!(
            text.contains(&app.i18n.text("workspace.title")),
            "the workspace head is missing:\n{text}"
        );
        assert!(
            text.contains(&app.i18n.text("workspace.category.overview")),
            "the category rail is missing:\n{text}"
        );
        assert!(!text.contains("tmcl"), "classic's frame leaked:\n{text}");

        // The selection is a margin bar on the page itself, never a filled row:
        // the cell behind the bar keeps the sheet colour. Both the category rail
        // and the content rows carry one.
        let main = app.layout_chrome(Rect::new(0, 0, 120, 40)).main;
        let rail_row = &rows[usize::from(main.y).saturating_add(1)];
        assert!(
            rail_row.contains(&app.i18n.text("workspace.category.overview")),
            "the rail row moved: {rail_row:?}"
        );
        assert!(
            rail_row.contains('▌'),
            "the active category has no margin bar: {rail_row:?}"
        );
        assert!(
            rows.iter()
                .any(|row| row.contains('▌') && row.contains("Name")),
            "the content rows have no margin bar:\n{text}"
        );

        let y = rows.iter().position(|row| row.contains('▌')).expect("bar") as u16;
        let row = &rows[y as usize];
        let byte = row.find('▌').expect("bar column");
        let x = palette::width(&row[..byte]) as u16;
        assert_eq!(buffer[(x, y)].bg, pal.bg);
        assert_eq!(buffer[(x, y)].fg, pal.accent);
    }

    fn click(app: &mut App, column: u16, row: u16) {
        app.handle(crate::tui::event::AppEvent::Mouse(
            crossterm::event::MouseEvent {
                kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column,
                row,
                modifiers: crossterm::event::KeyModifiers::NONE,
            },
        ));
    }

    fn version(id: &str) -> crate::core::version::models::VersionSummary {
        crate::core::version::models::VersionSummary {
            id: id.to_owned(),
            kind: "release".into(),
            url: String::new(),
            time: "2026-01-01T00:00:00+00:00".into(),
            release_time: "2026-01-01T00:00:00+00:00".into(),
            sha1: None,
            compliance_level: None,
        }
    }

    fn project(title: &str) -> crate::core::resource::model::ResourceProject {
        use crate::core::resource::model::ResourceSource;
        crate::core::resource::model::ResourceProject {
            source: ResourceSource::Modrinth,
            project_id: format!("id-{title}"),
            title: title.to_owned(),
            author: "author".into(),
            description: "a short summary".into(),
            icon_url: None,
            categories: Vec::new(),
            game_versions: vec!["1.20.1".into()],
            loaders: vec!["fabric".into()],
            downloads: Some(1234),
            updated_at: None,
        }
    }

    fn file(name: &str) -> crate::core::resource::model::ResourceFile {
        use crate::core::resource::model::{ReleaseChannel, ResourceSource};
        crate::core::resource::model::ResourceFile {
            source: ResourceSource::Modrinth,
            project_id: "p".into(),
            file_id: format!("f-{name}"),
            name: name.to_owned(),
            download_url: String::new(),
            sha1: None,
            size: Some(2 * 1024 * 1024),
            game_versions: vec!["1.20.1".into()],
            loaders: vec!["fabric".into()],
            channel: ReleaseChannel::Release,
            dependencies: Vec::new(),
        }
    }

    fn snapshot(
        label: &str,
        state: crate::core::task::TaskState,
        completed: u64,
        total: Option<u64>,
    ) -> crate::core::task::TaskSnapshot {
        crate::core::task::TaskSnapshot {
            job: None,
            created_seq: 0,
            id: uuid::Uuid::new_v4(),
            label: label.to_owned(),
            operation_type: crate::core::task::TaskOperationType::InstanceInstall,
            state,
            retryable: false,
            attempt: 0,
            completed,
            total,
            failure: None,
        }
    }

    fn account(
        name: &str,
        kind: crate::core::account::models::AccountKind,
    ) -> crate::core::account::models::Account {
        crate::core::account::models::Account {
            id: uuid::Uuid::new_v4(),
            kind,
            username: name.to_owned(),
            uuid: uuid::Uuid::new_v4(),
            skin: None,
            secret_ref: None,
            access_expires_at: None,
        }
    }

    #[test]
    fn the_download_page_shows_its_categories_and_versions() {
        let mut app = paper_app(2);
        app.page = Page::DownloadCenter;
        app.versions = vec![version("1.21.4"), version("1.21.3"), version("1.20.1")];
        app.version_selected = 1;
        let pal = app.paper_palette();
        let (rows, buffer) = screen(&mut app, 120, 40);
        assert_sheet(&buffer, 120, 40, pal.bg);

        let text = rows.join("\n");
        assert!(text.contains("1.21.3"), "{text}");
        assert!(text.contains("2 / 3"), "position missing:\n{text}");
        assert!(
            rows.iter()
                .any(|row| row.contains('▌') && row.contains("1.21.3")),
            "the selected version carries the margin bar:\n{text}"
        );
    }

    #[test]
    fn clicking_a_version_keeps_the_list_still() {
        let mut app = paper_app(2);
        app.page = Page::DownloadCenter;
        app.versions = (0..12)
            .map(|index| version(&format!("1.{index}")))
            .collect();
        app.version_selected = 0;
        screen(&mut app, 120, 40);
        let row = app
            .hitmap
            .hitboxes()
            .iter()
            .find(|hitbox| hitbox.action == Action::row(Page::DownloadCenter, 1))
            .map(|hitbox| hitbox.rect)
            .expect("version row 1");
        click(&mut app, row.x + 4, row.y);
        assert_eq!(app.version_selected, 1);
        let _ = screen(&mut app, 120, 40);
        let after = app
            .hitmap
            .hitboxes()
            .iter()
            .find(|hitbox| hitbox.action == Action::row(Page::DownloadCenter, 1))
            .map(|hitbox| hitbox.rect)
            .expect("version row 1 after the click");
        assert_eq!(row, after, "the list moved under a click");
    }

    #[test]
    fn the_download_page_lists_projects_files_and_a_detail_column() {
        use crate::core::resource::model::ResourceKind;
        let mut app = paper_app(2);
        app.page = Page::DownloadCenter;
        app.download_center.category = DownloadCategory::Resource(ResourceKind::Mod);
        app.resource_projects = vec![project("Sodium"), project("Lithium")];
        app.download_center.selected = 0;
        let (list, pane) = download_split(&app, 160, 40);
        let (rows, _) = screen(&mut app, 160, 40);
        let list_text = region_text(&rows, list);
        assert!(list_text.contains("Sodium"), "{list_text}");
        // Title-only rows, like the other presets: the description is printed
        // once, in the column, not again beside the title.
        assert!(
            !list_text.contains("a short summary"),
            "the list repeats the description the column already prints:\n{list_text}"
        );
        assert!(
            region_text(&rows, pane).contains("a short summary"),
            "{:?}",
            region_text(&rows, pane)
        );

        app.resource_detail = Some("id-Sodium".into());
        app.resource_detail_project = Some(project("Sodium"));
        app.resource_files = vec![file("sodium-1.0.jar"), file("sodium-0.9.jar")];
        let (rows, _) = screen(&mut app, 160, 40);
        let text = rows.join("\n");
        assert!(text.contains("sodium-1.0.jar"), "{text}");
        // Level two is the file table alone. The table already carries channel,
        // dependency count and compatibility, so the list keeps its full width.
        assert!(
            app.download_detail_areas(download_body(&app, 160, 40))
                .is_none(),
            "the file list reserves a column for a card it does not need"
        );
        assert!(
            !text.contains(&app.i18n.text("download.install_file")),
            "the file pane is still printed:\n{text}"
        );
    }

    /// The two halves of the download body — list, column — split the way the
    /// painter splits them, so a test can talk about one of them instead of the
    /// whole frame.
    fn download_body(app: &App, width: u16, height: u16) -> Rect {
        let main = app.layout_chrome(Rect::new(0, 0, width, height)).main;
        Rect::new(
            main.x,
            main.y + downloads::HEAD_ROWS,
            main.width,
            main.height - downloads::HEAD_ROWS,
        )
    }

    fn download_split(app: &App, width: u16, height: u16) -> (Rect, Rect) {
        app.download_detail_areas(download_body(app, width, height))
            .expect("this width leaves room for the detail column")
    }

    fn download_pane(app: &App, width: u16, height: u16) -> Rect {
        download_split(app, width, height).1
    }

    fn region_text(rows: &[String], area: Rect) -> String {
        rows[area.y as usize..area.bottom() as usize]
            .iter()
            .map(|row| {
                row.chars()
                    .skip(area.x as usize)
                    .take(area.width as usize)
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Same fields as `classic` and `nightlight`: this column *is* the detail
    /// sheet, printed on the page instead of on a slip. A reduced card of its
    /// own silently dropped project id, versions, loaders and tags.
    #[test]
    fn the_download_column_prints_the_whole_record() {
        use crate::core::resource::model::ResourceKind;
        let mut app = paper_app(1);
        app.page = Page::DownloadCenter;
        app.download_center.category = DownloadCategory::Resource(ResourceKind::Mod);
        app.resource_projects = vec![project("Sodium")];
        let pane = download_pane(&app, 160, 40);
        let (rows, _) = screen(&mut app, 160, 40);
        let column = region_text(&rows, pane);

        for key in [
            "download.author",
            "download.project_id",
            "download.downloads",
            "detail.versions",
            "detail.loaders",
            "download.tags",
        ] {
            let text = app.i18n.text(key);
            let label = text.split([':', '：']).next().unwrap_or(&text).trim();
            assert!(
                column.contains(label),
                "`{key}` is missing from the column:\n{column}"
            );
        }
        // The values, not just the labels: a record that prints "Versions" with
        // nothing after it would pass the loop above.
        for value in ["author", "id-Sodium", "1,234", "1.20.1", "fabric"] {
            assert!(column.contains(value), "`{value}` is missing:\n{column}");
        }
        assert!(column.contains("a short summary"), "{column}");
    }

    /// The column says how much is left, the way the other presets' panes do:
    /// a rail in the page margin with an accent thumb, and no rail at all when
    /// the record already fits.
    #[test]
    fn a_long_record_gets_the_scrollbar_the_other_presets_have() {
        use crate::core::resource::model::ResourceKind;
        let mut app = paper_app(1);
        app.page = Page::DownloadCenter;
        app.download_center.category = DownloadCategory::Resource(ResourceKind::Mod);
        app.resource_projects = vec![project("Sodium")];
        let (_, pane) = download_split(&app, 120, 30);
        let rail_x = pane.right();
        let rail = |buffer: &ratatui::buffer::Buffer| {
            (pane.y..pane.bottom())
                .map(|y| buffer[(rail_x, y)].symbol().to_owned())
                .collect::<Vec<_>>()
        };

        let (_, buffer) = screen(&mut app, 120, 30);
        assert!(
            rail(&buffer).iter().all(|symbol| symbol == " "),
            "a record that fits must not grow a rail: {:?}",
            rail(&buffer)
        );

        app.resource_projects[0].description = "word ".repeat(600);
        let (_, buffer) = screen(&mut app, 120, 30);
        let at_top = rail(&buffer);
        let thumb = at_top
            .iter()
            .position(|symbol| symbol == "▌")
            .expect("an overflowing record needs a thumb");
        assert!(
            at_top.iter().any(|symbol| symbol == "│"),
            "the rail has no track: {at_top:?}"
        );

        app.pane_scroll = 20;
        let (_, buffer) = screen(&mut app, 120, 30);
        let moved = rail(&buffer)
            .iter()
            .position(|symbol| symbol == "▌")
            .expect("thumb after scrolling");
        assert!(
            moved > thumb,
            "the thumb did not follow the wheel: {thumb} -> {moved}"
        );
    }

    /// The column describes the row under the cursor without waiting for
    /// `Enter`. Opening a project costs a network round trip, and until it
    /// returns the right-hand third is blank, which reads as a broken page.
    #[test]
    fn the_download_column_follows_the_list_before_a_project_is_opened() {
        use crate::core::resource::model::ResourceKind;
        let mut app = paper_app(1);
        app.page = Page::DownloadCenter;
        app.download_center.category = DownloadCategory::Resource(ResourceKind::Mod);
        app.resource_projects = vec![project("Sodium"), project("Lithium")];
        app.download_center.selected = 0;
        let pane = download_pane(&app, 160, 40);

        let (rows, _) = screen(&mut app, 160, 40);
        let column = region_text(&rows, pane);
        assert!(
            column.contains("Sodium"),
            "the column stayed blank until the project was opened:\n{column}"
        );
        assert!(column.contains("a short summary"), "{column}");

        app.download_center.selected = 1;
        let (rows, _) = screen(&mut app, 160, 40);
        let column = region_text(&rows, pane);
        assert!(column.contains("Lithium"), "{column}");
        assert!(
            !column.contains("Sodium"),
            "the column kept the previous project:\n{column}"
        );
    }

    #[test]
    fn the_tasks_page_groups_entries_and_draws_progress() {
        use crate::core::task::TaskState;
        let mut app = paper_app(1);
        app.page = Page::Tasks;
        app.task_snapshots = vec![
            snapshot("downloading", TaskState::Running, 1, Some(2)),
            snapshot("queued job", TaskState::Queued, 0, Some(2)),
            snapshot("old job", TaskState::Succeeded, 2, Some(2)),
        ];
        app.task_selected = 0;
        let pal = app.paper_palette();
        let (rows, buffer) = screen(&mut app, 120, 40);
        assert_sheet(&buffer, 120, 40, pal.bg);

        let text = rows.join("\n");
        assert!(text.contains("downloading"), "{text}");
        assert!(text.contains("queued job"), "{text}");
        assert!(text.contains("old job"), "{text}");
        assert!(text.contains("50%"), "progress missing:\n{text}");
        assert!(text.contains('━'), "the bar is drawn:\n{text}");
        assert!(
            rows.iter()
                .any(|row| row.contains('▌') && row.contains("downloading")),
            "the selected task carries the margin bar:\n{text}"
        );

        // Running first, finished last: the order is the point of grouping.
        let position = |needle: &str| rows.iter().position(|row| row.contains(needle));
        let running = position("downloading").expect("running group");
        let queued = position("queued job").expect("queued group");
        let finished = position("old job").expect("finished group");
        assert!(
            running < queued && queued < finished,
            "the groups are out of order: {running} / {queued} / {finished}\n{text}"
        );
    }

    /// Speed and ETA sit to the left of the bar. On the right they pushed the
    /// bar leftwards, so a running entry's bar no longer aligned with the
    /// finished ones above and below it.
    #[test]
    fn a_transfer_label_sits_left_of_the_bar_and_leaves_it_on_its_column() {
        use crate::core::task::TaskState;
        use std::time::{Duration, Instant};

        let mut app = paper_app(1);
        app.page = Page::Tasks;
        let moving = snapshot("downloading", TaskState::Running, 84, Some(100));
        let quiet = snapshot("waiting", TaskState::Running, 40, Some(100));
        let done = snapshot("finished", TaskState::Succeeded, 100, Some(100));
        let id = moving.id;
        app.task_snapshots = vec![moving, quiet, done];
        app.task_selected = 0;

        // Two samples a second apart: the tracker reports a delta, not a total.
        let then = Instant::now() - Duration::from_secs(1);
        app.speed_tracker.observe(id, 1_000_000, then);
        app.speed_tracker.observe(id, 4_200_000, Instant::now());

        let (rows, _) = screen(&mut app, 120, 40);
        // Byte offsets would do here, but the label carries a two-byte `·`;
        // compare display columns like every other width assertion.
        let column = |row: &str| {
            let byte = row.find('━').or_else(|| row.find('╌'))?;
            Some(crate::tui::palette::width(&row[..byte]))
        };
        let bars = rows
            .iter()
            .filter_map(|row| column(row))
            .collect::<Vec<_>>();
        assert!(bars.len() >= 3, "expected three progress rows: {bars:?}");
        assert!(
            bars.iter().all(|at| *at == bars[0]),
            "the bar moved between rows: {bars:?}\n{}",
            rows.join("\n")
        );

        let moving = rows
            .iter()
            .find(|row| row.contains("MB/s"))
            .expect("the speed label is drawn");
        let speed = moving.find("MB/s").expect("speed");
        let bar = moving.find('━').expect("bar");
        assert!(
            speed < bar,
            "the speed label is to the right of the bar: {moving:?}"
        );
    }

    #[test]
    fn the_accounts_page_marks_the_current_profile() {
        use crate::core::account::models::AccountKind;
        let mut app = paper_app(1);
        app.page = Page::Accounts;
        let first = account("Steve", AccountKind::Microsoft);
        let second = account("Alex", AccountKind::Offline);
        app.accounts.current = Some(first.id);
        app.accounts.accounts = vec![first, second];
        app.accounts.selected = 0;
        let (rows, _) = screen(&mut app, 120, 40);
        let text = rows.join("\n");
        assert!(text.contains("Steve"), "{text}");
        assert!(text.contains("Alex"), "{text}");
        assert!(
            text.contains("current"),
            "the current marker is missing:\n{text}"
        );
        assert!(text.contains("Microsoft"), "{text}");
        assert!(text.contains('▌'), "{text}");
    }

    #[test]
    fn the_accounts_page_prints_the_device_code() {
        let mut app = paper_app(1);
        app.page = Page::Accounts;
        app.accounts.device_code = Some(crate::tui::pages::accounts::DeviceCodeView {
            user_code: "ABCD-1234".into(),
            verification_uri: "https://microsoft.com/link".to_owned(),
            remaining_seconds: 900,
            status: "waiting for the browser".to_owned(),
        });
        let (rows, _) = screen(&mut app, 120, 40);
        let text = rows.join("\n");
        assert!(text.contains("A B C D - 1 2 3 4"), "{text}");
        assert!(text.contains("microsoft.com/link"), "{text}");
    }

    #[test]
    fn the_logs_page_spells_levels_and_marks_errors() {
        let mut app = paper_app(1);
        app.page = Page::Logs;
        app.logs.append("[stdout] hello world");
        app.logs.append("[stderr] something broke");
        let (rows, _) = screen(&mut app, 120, 40);
        let text = rows.join("\n");
        assert!(
            text.contains(&app.i18n.text("logs.live")),
            "the head lost the follow state:\n{text}"
        );
        assert!(text.contains("I hello world"), "{text}");
        assert!(text.contains("E something broke"), "{text}");
        assert!(
            rows.iter()
                .any(|row| row.contains('▌') && row.contains("E something")),
            "an error carries the margin bar:\n{text}"
        );
    }

    #[test]
    fn the_settings_page_lists_categories_and_adjustable_values() {
        let mut app = paper_app(1);
        app.page = Page::Settings;
        app.settings
            .select_category(crate::tui::pages::settings::SettingsCategory::Appearance);
        app.settings.selected = 0;
        let (rows, _) = screen(&mut app, 120, 40);
        let text = rows.join("\n");
        assert!(text.contains("Appearance"), "{text}");
        assert!(
            text.contains('‹') && text.contains('›'),
            "steppers missing:\n{text}"
        );
        assert!(text.contains('▌'), "{text}");
    }

    #[test]
    fn the_settings_page_shows_java_and_game_directories() {
        use crate::core::java::models::JavaInstallation;
        use crate::tui::pages::settings::SettingsCategory;
        let mut app = paper_app(1);
        app.page = Page::Settings;
        app.java = vec![JavaInstallation {
            executable: std::path::PathBuf::from("C:/java/bin/java.exe"),
            version: "17.0.8".into(),
            major: 17,
            vendor: Some("Eclipse".into()),
            is_jdk: true,
            lts: true,
        }];
        app.java_selected = 0;
        app.settings.select_category(SettingsCategory::Java);
        let (rows, _) = screen(&mut app, 120, 40);
        let text = rows.join("\n");
        assert!(text.contains("Java 17.0.8"), "{text}");
        assert!(text.contains("java.exe"), "{text}");

        app.settings
            .select_category(SettingsCategory::GameDirectories);
        app.settings.selected = 2;
        app.config.game_directories = vec![std::path::PathBuf::from("C:/games")];
        let (rows, _) = screen(&mut app, 120, 40);
        let text = rows.join("\n");
        assert!(text.contains("C:/games"), "{text}");
        assert!(
            text.contains("1 instances") || text.contains("1 个实例"),
            "{text}"
        );
    }

    #[test]
    fn the_settings_search_lists_matches_across_categories() {
        let mut app = paper_app(1);
        app.page = Page::Settings;
        app.settings_search = Some("theme".into());
        app.settings.selected = 0;
        let (rows, _) = screen(&mut app, 120, 40);
        let text = rows.join("\n");
        assert!(text.contains("Theme"), "{text}");
        assert!(
            text.contains("Appearance"),
            "the category is named:\n{text}"
        );
    }

    #[test]
    fn a_narrow_instance_row_keeps_one_status_word() {
        for (width, height) in [(80u16, 24u16), (56, 14)] {
            let mut app = paper_app(6);
            app.page = Page::Instances;
            let (rows, _) = screen(&mut app, width, height);
            let status = localize_instance_status("ready", &app.i18n);
            let mut seen = 0;
            for row in rows
                .iter()
                .filter(|row| row.contains("instance-") && !row.contains('※'))
            {
                assert!(
                    !row.contains("1.20.1") && !row.contains("Forge"),
                    "{width}x{height}: a narrow row still prints the version and loader: {row:?}"
                );
                assert_eq!(
                    row.matches(&status).count(),
                    1,
                    "{width}x{height}: the status column wrapped into the name: {row:?}"
                );
                seen += 1;
            }
            assert!(seen > 0, "{width}x{height}: no instance rows drawn");
        }
    }

    #[test]
    fn settings_fold_the_rail_on_a_narrow_terminal() {
        use crate::tui::pages::settings::SettingsCategory;
        let mut app = paper_app(1);
        app.page = Page::Settings;
        app.settings.select_category(SettingsCategory::Appearance);
        let pal = app.paper_palette();
        let (rows, buffer) = screen(&mut app, 80, 24);
        assert_sheet(&buffer, 80, 24, pal.bg);
        let text = rows.join("\n");
        assert!(
            text.contains("Game directories"),
            "the folded category line lost an entry:\n{text}"
        );
        assert!(
            !text.contains('│'),
            "the rail column must be gone at 80 columns"
        );
        assert!(text.contains("Paper stock"), "{text}");
    }

    #[test]
    fn the_download_chapter_row_lists_all_six_categories() {
        use crate::tui::pages::download_center::DownloadCategory;
        let mut app = paper_app(1);
        app.page = Page::DownloadCenter;
        let (rows, buffer) = screen(&mut app, 120, 40);

        let current = app.i18n.text(DownloadCategory::GameVersions.label_key());
        let y = rows
            .iter()
            .position(|row| row.contains(&current))
            .expect("the chapter row") as u16;
        for category in DownloadCategory::ALL {
            let label = app.i18n.text(category.label_key());
            assert!(
                rows[y as usize].contains(&label),
                "{label} is missing from the chapter row: {:?}",
                rows[y as usize]
            );
        }
        let row = &rows[y as usize];
        let byte = row.find(&current).expect("label column");
        let x = palette::width(&row[..byte]) as u16;
        assert!(
            buffer[(x, y)]
                .modifier
                .contains(ratatui::style::Modifier::UNDERLINED),
            "the current category is not underlined: {:?}",
            buffer[(x, y)]
        );
    }

    #[test]
    fn clicking_a_project_or_a_file_keeps_its_list_still() {
        use crate::core::resource::model::ResourceKind;
        let mut app = paper_app(1);
        app.page = Page::DownloadCenter;
        app.download_center.category = DownloadCategory::Resource(ResourceKind::Mod);

        // Projects first, then the same list once a project is open and the
        // rows are files: both go through `RowLayout::list_bare`, so both can
        // drift under a click.
        for open_detail in [false, true] {
            if open_detail {
                app.resource_detail = Some("id-Project 0".into());
                app.resource_detail_project = Some(project("Project 0"));
                app.resource_files = (0..12)
                    .map(|index| file(&format!("file-{index}.jar")))
                    .collect();
            } else {
                app.resource_projects = (0..12)
                    .map(|index| project(&format!("Project {index}")))
                    .collect();
            }
            app.download_center.selected = 0;
            screen(&mut app, 120, 40);
            let before = app
                .hitmap
                .hitboxes()
                .iter()
                .find(|hitbox| hitbox.action == Action::row(Page::DownloadCenter, 1))
                .map(|hitbox| hitbox.rect)
                .expect("row 1");
            click(&mut app, before.x + 4, before.y);
            assert_eq!(
                app.download_center.selected, 1,
                "the click did not reach the list (detail open: {open_detail})"
            );
            screen(&mut app, 120, 40);
            let after = app
                .hitmap
                .hitboxes()
                .iter()
                .find(|hitbox| hitbox.action == Action::row(Page::DownloadCenter, 1))
                .map(|hitbox| hitbox.rect)
                .expect("row 1 after the click");
            assert_eq!(before, after, "the list moved under a click");
        }
    }

    #[test]
    fn the_download_detail_ends_with_the_install_legend() {
        use crate::core::resource::model::ResourceKind;
        let mut app = paper_app(1);
        app.page = Page::DownloadCenter;
        app.download_center.category = DownloadCategory::Resource(ResourceKind::Mod);
        app.resource_projects = vec![project("Sodium")];
        let pane = download_pane(&app, 160, 40);
        let (rows, _) = screen(&mut app, 160, 40);
        let column = region_text(&rows, pane);
        let hint = app.i18n.text("download.install_file");
        let last = column
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or_default();
        assert!(
            last.trim_start().starts_with('▸') && last.contains(&hint),
            "the record has to close with the install line ({hint:?}):\n{column}"
        );
    }

    #[test]
    fn an_empty_download_list_says_so() {
        use crate::core::resource::model::ResourceKind;
        let mut app = paper_app(1);
        app.page = Page::DownloadCenter;
        app.download_center.category = DownloadCategory::Resource(ResourceKind::Mod);
        app.resource_projects.clear();
        let (rows, _) = screen(&mut app, 120, 40);
        assert!(
            rows.join("\n")
                .contains(&app.i18n.text("download.no_results")),
            "the empty list printed nothing:\n{}",
            rows.join("\n")
        );
    }

    #[test]
    fn a_long_task_name_is_truncated_instead_of_wrapping() {
        use crate::core::task::TaskState;
        let mut app = paper_app(1);
        app.page = Page::Tasks;
        let long = "Installing a very long instance name ".repeat(4);
        app.task_snapshots = vec![snapshot(&long, TaskState::Running, 1, Some(2))];
        app.task_selected = 0;
        let (rows, _) = screen(&mut app, 120, 40);
        let text = rows.join("\n");
        assert!(
            !text.contains(&long),
            "the task name was not truncated:\n{text}"
        );
        assert!(
            rows.iter().any(|row| row.contains('…')),
            "the truncation has no ellipsis:\n{text}"
        );
    }

    #[test]
    fn clicking_an_account_keeps_the_list_still() {
        use crate::core::account::models::AccountKind;
        let mut app = paper_app(1);
        app.page = Page::Accounts;
        app.accounts.accounts = (0..12)
            .map(|index| account(&format!("Player {index}"), AccountKind::Offline))
            .collect();
        app.accounts.selected = 0;
        screen(&mut app, 120, 40);
        let before = app
            .hitmap
            .hitboxes()
            .iter()
            .find(|hitbox| hitbox.action == Action::row(Page::Accounts, 1))
            .map(|hitbox| hitbox.rect)
            .expect("account row 1");
        click(&mut app, before.x + 4, before.y);
        assert_eq!(app.accounts.selected, 1);
        screen(&mut app, 120, 40);
        let after = app
            .hitmap
            .hitboxes()
            .iter()
            .find(|hitbox| hitbox.action == Action::row(Page::Accounts, 1))
            .map(|hitbox| hitbox.rect)
            .expect("account row 1 after the click");
        assert_eq!(before, after, "the account list moved under a click");
    }

    #[test]
    fn the_logs_reach_both_ends_of_the_buffer() {
        let mut app = paper_app(1);
        app.page = Page::Logs;
        for index in 0..60 {
            app.logs.append(format!("[stdout] line {index}"));
        }
        // Following the tail is the default: the newest line is on the page.
        let (rows, _) = screen(&mut app, 120, 40);
        let text = rows.join("\n");
        assert!(text.contains("line 59"), "the tail is not shown:\n{text}");

        // Scrolling up must reach the very first line. A maximum shorter than
        // the painter's would strand the first rows forever.
        let max = app.log_max_scroll_offset();
        assert!(
            max > 0,
            "60 lines must overflow the page, else this proves nothing"
        );
        app.logs.scroll(-(max as isize) - 10, max);
        let (rows, _) = screen(&mut app, 120, 40);
        let text = rows.join("\n");
        assert!(text.contains("line 0 "), "the head is unreachable:\n{text}");
        assert!(
            !text.contains("line 59"),
            "the tail is still drawn:\n{text}"
        );
    }

    #[test]
    fn the_log_search_shows_its_cursor_when_focused() {
        use crate::tui::input::{SearchState, SearchTarget};
        let mut app = paper_app(1);
        app.page = Page::Logs;
        app.logs.append("[stdout] hello");
        let (rows, _) = screen(&mut app, 120, 40);
        assert!(
            !rows.join("\n").contains('▏'),
            "an unfocused field shows a cursor"
        );

        app.search = Some(SearchState::new(SearchTarget::Logs));
        let (rows, _) = screen(&mut app, 120, 40);
        assert!(
            rows.join("\n").contains('▏'),
            "the focused search field has no cursor"
        );
    }

    #[test]
    fn every_adjustable_value_ends_on_the_same_column() {
        use crate::tui::pages::settings::SettingsCategory;
        let mut app = paper_app(1);
        app.page = Page::Settings;
        app.settings.select_category(SettingsCategory::Appearance);
        let (rows, _) = screen(&mut app, 120, 40);
        let columns = rows
            .iter()
            .filter_map(|row| {
                let byte = row.rfind('›')?;
                Some(palette::width(&row[..byte]))
            })
            .collect::<Vec<_>>();
        assert!(
            columns.len() >= 3,
            "too few steppers to compare: {columns:?}"
        );
        assert!(
            columns.windows(2).all(|pair| pair[0] == pair[1]),
            "the value column is ragged: {columns:?}"
        );
    }

    #[test]
    fn the_java_and_directory_lists_keep_their_own_selection() {
        use crate::core::java::models::JavaInstallation;
        use crate::tui::pages::settings::SettingsCategory;
        let mut app = paper_app(1);
        app.page = Page::Settings;
        app.java = (0..4)
            .map(|index| JavaInstallation {
                executable: std::path::PathBuf::from(format!("C:/java/{index}/bin/java.exe")),
                version: format!("17.0.{index}"),
                major: 17,
                vendor: Some("Eclipse".into()),
                is_jdk: true,
                lts: true,
            })
            .collect();
        app.settings.select_category(SettingsCategory::Java);
        app.java_selected = 0;
        app.settings.selected = 0;
        screen(&mut app, 120, 40);
        let row = app
            .hitmap
            .hitboxes()
            .iter()
            .find(|hitbox| hitbox.action == Action::row(Page::Settings, 1))
            .map(|hitbox| hitbox.rect)
            .expect("java row 1");
        click(&mut app, row.x + 4, row.y);
        assert_eq!(
            app.java_selected, 1,
            "the java list did not follow the click"
        );
        assert_eq!(
            app.settings.selected, 0,
            "clicking java moved the settings rows too"
        );
    }

    #[test]
    fn the_settings_search_counter_matches_the_filtered_rows() {
        let mut app = paper_app(1);
        app.page = Page::Settings;
        app.settings_search = Some("theme".into());
        let hits = app.settings_search_results().len();
        let (rows, _) = screen(&mut app, 120, 40);
        let text = rows.join("\n");
        let counter = app
            .i18n
            .format("settings.search_results", &[("count", &hits.to_string())]);
        assert!(
            text.contains(&counter),
            "the counter does not match the {hits} filtered rows:\n{text}"
        );
        for hit in app.settings_search_results() {
            assert!(
                text.contains(&hit.row.title),
                "{} is counted but not drawn:\n{text}",
                hit.row.title
            );
        }
    }

    fn press(app: &mut App, code: crossterm::event::KeyCode) {
        app.handle(crate::tui::event::AppEvent::Key(
            crossterm::event::KeyEvent {
                code,
                modifiers: crossterm::event::KeyModifiers::NONE,
                kind: crossterm::event::KeyEventKind::Press,
                state: crossterm::event::KeyEventState::NONE,
            },
        ));
    }

    #[test]
    fn the_detail_slip_prints_the_selected_instance() {
        let mut app = paper_app(3);
        app.page = Page::Instances;
        app.instances.focused = 0;
        app.detail_open = true;
        let pal = app.paper_palette();
        let (rows, buffer) = screen(&mut app, 120, 30);
        let text = rows.join("\n");
        assert!(
            text.contains(&app.i18n.text("common.details")),
            "the slip has no title:\n{text}"
        );
        assert!(text.contains("instance-0"), "{text}");
        assert!(text.contains("Version"), "{text}");
        // The same card the dialogs use: centred, on the sheet colour, with a
        // row of air above the title.
        let rect = app.paper_detail_rect(Rect::new(0, 0, 120, 30));
        assert_eq!(
            buffer[(rect.x.saturating_add(2), rect.y.saturating_add(3))].bg,
            pal.sheet,
            "the slip is not on sheet"
        );
        assert_ne!(buffer[(4, 12)].bg, pal.sheet, "the page became a slip");
        // The head row of the card is air: the title sits one row below it,
        // exactly as `modal/paper.rs` prints a dialog.
        let head = Rect::new(rect.x, rect.y, rect.width, 1);
        assert!(
            region_text(&rows, head).trim().is_empty(),
            "the title touches the top edge: {:?}",
            region_text(&rows, head)
        );
        let title = Rect::new(rect.x, rect.y.saturating_add(1), rect.width, 1);
        assert!(
            region_text(&rows, title).contains(&app.i18n.text("common.details")),
            "the title is not on the dialog's head row: {:?}",
            region_text(&rows, title)
        );
    }

    #[test]
    fn a_narrow_terminal_keeps_the_detail_inside_the_screen() {
        let mut app = paper_app(3);
        app.page = Page::Instances;
        app.instances.focused = 0;
        app.detail_open = true;
        let pal = app.paper_palette();
        let (rows, buffer) = screen(&mut app, 80, 24);
        let rect = app.paper_detail_rect(Rect::new(0, 0, 80, 24));
        assert!(rect.width <= 76, "the card took the page: {rect:?}");
        assert!(rect.right() <= 80 && rect.bottom() <= 24, "{rect:?}");
        assert_eq!(
            buffer[(rect.x.saturating_add(2), rect.y.saturating_add(3))].bg,
            pal.sheet
        );
        assert!(
            rows.iter().any(|row| row.contains("instance-0")),
            "the record lost the name:\n{}",
            rows.join("\n")
        );
    }

    #[test]
    fn the_detail_scrolls_to_its_last_line() {
        let mut app = paper_app(3);
        app.page = Page::Instances;
        app.instances.focused = 0;
        // A long name wraps in the slip and pushes the last row past the page.
        app.instances.items[0].name = format!("instance-{}", "x".repeat(90));
        app.detail_open = true;
        let (rows, _) = screen(&mut app, 120, 14);
        let text = rows.join("\n");
        assert!(!text.contains("Path "), "the record already fits:\n{text}");

        let max = app.detail_max_scroll();
        assert!(max > 0, "the fixture does not overflow");
        app.detail_scroll = max;
        let (rows, _) = screen(&mut app, 120, 14);
        let text = rows.join("\n");
        assert!(
            text.contains("Path "),
            "the last line is unreachable:\n{text}"
        );
    }

    #[test]
    fn escape_closes_the_paper_detail() {
        let mut app = paper_app(2);
        app.page = Page::Instances;
        app.instances.focused = 0;
        app.detail_open = true;
        screen(&mut app, 120, 30);
        press(&mut app, crossterm::event::KeyCode::Esc);
        assert!(!app.detail_open, "Esc left the detail open");
    }

    /// The reference is a slip, like every other preset: a page-wide one also
    /// covered the running head and the key band that close it, so the keys
    /// that get you out disappeared with the page.
    #[test]
    fn the_help_is_a_slip_over_the_page() {
        let mut app = paper_app(1);
        app.page = Page::Instances;
        app.help_open = true;
        let pal = app.paper_palette();
        let area = Rect::new(0, 0, 120, 40);
        let card = app.help_layout(area, &app.help_lines()).rect;
        let footer = app.layout_chrome(area).footer;
        let (rows, buffer) = screen(&mut app, 120, 40);
        let text = rows.join("\n");
        assert!(
            text.contains(&app.i18n.text("help.keyboard")),
            "the reference has no title:\n{text}"
        );
        for section in ["help.section.general", "help.section.page"] {
            assert!(
                text.contains(&app.i18n.text(section)),
                "{section} is missing:\n{text}"
            );
        }
        // A card, not the page: it starts below the running head and stops
        // above the key band.
        assert!(card.y > 0, "the slip touches the running head: {card:?}");
        assert!(card.bottom() < 40, "the slip covers the key band: {card:?}");
        assert!(card.width < 120, "the slip took the page: {card:?}");
        assert_eq!(buffer[(card.x, card.y)].bg, pal.sheet, "{card:?}");
        // The head is the dialog's: a blank row, the title in the accent, a
        // hairline. A title in body ink on the card's top edge was neither.
        let head = Rect::new(card.x, card.y, card.width, 1);
        assert!(
            region_text(&rows, head).trim().is_empty(),
            "the title touches the top edge: {:?}",
            region_text(&rows, head)
        );
        let title = Rect::new(card.x, card.y.saturating_add(1), card.width, 1);
        assert!(
            region_text(&rows, title).contains(&app.i18n.text("help.keyboard")),
            "the title is not on the dialog's head row: {:?}",
            region_text(&rows, title)
        );
        assert_eq!(
            buffer[(card.x.saturating_add(2), card.y.saturating_add(1))].fg,
            pal.accent,
            "the title is not printed in the accent"
        );
        // Both survivors are still printed.
        assert!(rows[0].contains("TMCL"), "the running head is gone");
        // The footer opens with its own hairline; the keys sit on the next row.
        let keys = rows[footer.y as usize + 1].clone();
        assert!(
            keys.contains(&app.i18n.text("action.back")),
            "the key band lost the way out:\n{keys}"
        );
    }

    #[test]
    fn the_help_scroll_stops_at_its_own_end() {
        let mut app = paper_app(1);
        app.page = Page::Instances;
        app.help_open = true;
        screen(&mut app, 80, 20);
        let max = app.help_max_scroll();
        assert!(max > 0, "the reference fits, so this proves nothing");

        app.help_scroll = max.saturating_add(10);
        screen(&mut app, 80, 20);
        let layout = app.help_layout(Rect::new(0, 0, 80, 20), &app.help_lines());
        assert_eq!(layout.scroll, max, "the page scrolled past its maximum");
    }

    /// WCAG relative luminance, for comparing two surfaces.
    fn luminance(color: Color) -> f64 {
        let Color::Rgb(r, g, b) = color else {
            return 0.0;
        };
        fn channel(value: u8) -> f64 {
            let value = f64::from(value) / 255.0;
            if value <= 0.039_28 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
    }

    fn contrast(a: Color, b: Color) -> f64 {
        let (first, second) = (luminance(a), luminance(b));
        let (high, low) = if first > second {
            (first, second)
        } else {
            (second, first)
        };
        (high + 0.05) / (low + 0.05)
    }

    #[test]
    fn the_scrim_darkens_the_page_but_leaves_the_slip_alone() {
        use crate::core::config::models::{PaperTheme, ThemeMode};
        for theme in PaperTheme::ALL {
            for mode in [ThemeMode::Light, ThemeMode::Dark] {
                let mut app = paper_app(3);
                app.config.state.ui.paper_theme = theme;
                app.config.state.ui.theme_mode = mode;
                app.modal = Some(crate::tui::modal::Modal::confirm(
                    "Delete this instance?",
                    crate::tui::modal::ConfirmAction::RemoveAccount,
                ));
                let pal = app.paper_palette();
                let (_, buffer) = screen(&mut app, 120, 40);
                let slip = app
                    .modal
                    .as_ref()
                    .expect("modal")
                    .paper_area(Rect::new(0, 0, 120, 40));

                let page = buffer[(4, 30)].bg;
                assert!(
                    luminance(page) < luminance(pal.sheet),
                    "{theme:?}/{mode:?}: the page is not dimmed below the slip"
                );
                assert_eq!(
                    buffer[(slip.x.saturating_add(5), slip.y.saturating_add(3))].bg,
                    pal.sheet,
                    "{theme:?}/{mode:?}: the slip body is not on sheet"
                );
                // The accent has to carry the title and the key caps on the
                // slip, not just on the page.
                assert!(
                    contrast(pal.accent, pal.sheet) >= 3.0,
                    "{theme:?}/{mode:?}: accent on sheet is {:.2}:1",
                    contrast(pal.accent, pal.sheet)
                );
            }
        }
    }

    #[test]
    fn clicking_inside_the_detail_slip_keeps_it_open() {
        let mut app = paper_app(2);
        app.page = Page::Instances;
        app.instances.focused = 0;
        app.detail_open = true;
        screen(&mut app, 120, 30);
        let rect = app.paper_detail_rect(Rect::new(0, 0, 120, 30));

        click(&mut app, rect.x.saturating_add(3), rect.y.saturating_add(4));
        assert!(
            app.detail_open,
            "a click inside the record closed it — the blocker is using another rect"
        );
        click(&mut app, rect.x.saturating_sub(4), rect.y.saturating_add(4));
        assert!(!app.detail_open, "a click outside the record left it open");
    }

    #[test]
    fn clicking_inside_the_help_card_keeps_it_open() {
        let mut app = paper_app(2);
        app.page = Page::Instances;
        app.help_open = true;
        screen(&mut app, 120, 40);
        let card = app
            .help_layout(Rect::new(0, 0, 120, 40), &app.help_lines())
            .rect;

        click(&mut app, card.x + 2, card.y + 2);
        assert!(
            app.help_open,
            "a click inside the card closed it — the blocker is using another rect"
        );
        click(&mut app, card.x.saturating_sub(2), card.y + 2);
        assert!(
            !app.help_open,
            "a click on the page beside the card must close the reference"
        );
    }

    #[test]
    fn an_empty_accounts_page_names_the_way_out() {
        let mut app = paper_app(1);
        app.page = Page::Accounts;
        app.accounts.accounts.clear();
        app.accounts.current = None;
        let (rows, _) = screen(&mut app, 120, 40);
        let text = rows.join("\n");
        assert!(
            text.contains(&app.i18n.text("account.login_empty")),
            "the empty page says nothing:\n{text}"
        );
        // A blank page with no keys is a dead end: both ways to add an account
        // are printed under it.
        for key in ["action.offline_account", "action.microsoft_login"] {
            assert!(
                text.contains(&app.i18n.text(key)),
                "{key} is missing from the empty page:\n{text}"
            );
        }
    }

    #[test]
    fn every_settings_category_stays_inside_the_page() {
        use crate::tui::pages::settings::SettingsCategory;
        // The logs category prints a directory path; before the width clamp it
        // ran past the right edge and panicked in the frame buffer.
        for category in SettingsCategory::ALL {
            for (width, height) in [(120u16, 40u16), (80, 24), (56, 14)] {
                let mut app = paper_app(1);
                app.page = Page::Settings;
                app.settings.select_category(category);
                let pal = app.paper_palette();
                let (rows, buffer) = screen(&mut app, width, height);
                assert_sheet(&buffer, width, height, pal.bg);
                assert!(
                    rows.join("\n")
                        .contains(&app.i18n.text(category.title_key())),
                    "{category:?} at {width}x{height}: the section title is missing"
                );
            }
        }
    }

    #[test]
    fn every_version_row_carries_its_kind_and_date() {
        let mut app = paper_app(1);
        app.page = Page::DownloadCenter;
        app.versions = (0..4)
            .map(|index| version(&format!("1.21.{index}")))
            .collect();
        app.version_selected = 0;
        let (rows, _) = screen(&mut app, 120, 40);

        let kind = app.version_kind_label("release");
        let mut seen = 0;
        for row in rows.iter().filter(|row| row.contains("1.21.")) {
            assert!(
                row.contains(&kind) && row.contains("2026-01-01"),
                "a version row lost its columns: {row:?}"
            );
            seen += 1;
        }
        assert!(seen >= 3, "too few version rows drawn: {seen}");
    }

    /// The file row carries channel, dependency count and compatibility in
    /// place of the detail card. The head row above it names the columns;
    /// `兼容性` reads as a word, a bare count does not.
    #[test]
    fn every_file_row_carries_the_table_columns() {
        use crate::core::resource::model::{DependencyKind, ResourceDependency, ResourceKind};
        let mut app = paper_app(1);
        app.page = Page::DownloadCenter;
        app.download_center.category = DownloadCategory::Resource(ResourceKind::Mod);
        app.resource_detail = Some("id-Sodium".into());
        app.resource_detail_project = Some(project("Sodium"));
        app.resource_files = (0..4)
            .map(|index| file(&format!("sodium-{index}.jar")))
            .collect();
        app.resource_files[0].dependencies = (0..7)
            .map(|index| ResourceDependency {
                project_id: Some(format!("dep-{index}")),
                file_id: None,
                name: format!("Dependency {index}"),
                kind: DependencyKind::Required,
            })
            .collect();
        app.download_center.selected = 0;
        let (rows, _) = screen(&mut app, 120, 40);

        let head = rows
            .iter()
            .find(|row| row.contains(&app.i18n.text("download.compatibility")))
            .expect("the table head names the columns");
        for key in ["download.file", "download.channel", "download.dependencies"] {
            assert!(
                head.contains(&app.i18n.text(key)),
                "`{key}` is missing from the head: {head:?}"
            );
        }

        let mut seen = 0;
        for row in rows.iter().filter(|row| row.contains(".jar")) {
            assert!(
                row.contains("Stable") && row.contains("Unknown"),
                "a file row lost its channel or compatibility: {row:?}"
            );
            seen += 1;
        }
        assert!(seen >= 3, "too few file rows drawn: {seen}");
        // The dependency count is the only bare number in the row.
        let first = rows
            .iter()
            .find(|row| row.contains("sodium-0.jar"))
            .expect("the first file row");
        assert!(
            first.contains(" 7"),
            "the dependency count is missing: {first:?}"
        );
    }

    #[test]
    fn the_page_number_is_flush_with_the_right_margin() {
        let mut app = paper_app(2);
        app.page = Page::DownloadCenter;
        app.versions = (0..3)
            .map(|index| version(&format!("1.21.{index}")))
            .collect();
        app.version_selected = 0;
        let (rows, _) = screen(&mut app, 120, 40);

        let main = app.layout_chrome(Rect::new(0, 0, 120, 40)).main;
        let row = &rows[usize::from(main.y)];
        let byte = row.find("1 / 3").expect("the page number is missing");
        let x = palette::width(&row[..byte]) as u16;
        assert_eq!(
            x,
            main.right() - 5,
            "the page number does not sit on the margin: {row:?}"
        );
        assert!(
            !row.contains("Packs1"),
            "the number runs into the last category: {row:?}"
        );
    }

    #[test]
    fn a_narrow_download_row_keeps_the_chosen_category() {
        use crate::core::resource::model::ResourceKind;
        let mut app = paper_app(1);
        app.page = Page::DownloadCenter;
        // The last category: at 56 columns the row must still show the chosen
        // name rather than stopping at Resource Packs.
        app.download_center.category = DownloadCategory::Resource(ResourceKind::World);
        let (rows, _) = screen(&mut app, 56, 14);
        let main = app.layout_chrome(Rect::new(0, 0, 56, 14)).main;
        let row = &rows[usize::from(main.y)];
        assert!(
            row.contains(
                &app.i18n
                    .text(DownloadCategory::Resource(ResourceKind::World).label_key())
            ),
            "the chosen category is not on the row: {row:?}"
        );
    }

    #[test]
    fn the_folded_settings_strip_keeps_the_current_category() {
        use crate::tui::pages::settings::SettingsCategory;
        let mut app = paper_app(1);
        app.page = Page::Settings;
        app.settings.select_category(SettingsCategory::Logs);
        let (rows, _) = screen(&mut app, 56, 14);
        let main = app.layout_chrome(Rect::new(0, 0, 56, 14)).main;
        let strip = &rows[usize::from(main.y)];
        assert!(
            strip.contains(&app.i18n.text(SettingsCategory::Logs.title_key())),
            "the current category is hidden: {strip:?}"
        );
        assert!(
            !strip.trim_end().ends_with('·'),
            "the strip ends with a dangling separator: {strip:?}"
        );
    }

    #[test]
    fn a_long_value_does_not_shrink_its_label() {
        use crate::tui::pages::settings::SettingsCategory;
        let mut app = paper_app(1);
        app.page = Page::Settings;
        app.settings.select_category(SettingsCategory::Logs);
        let (rows, _) = screen(&mut app, 80, 24);
        let text = rows.join("\n");
        assert!(
            text.contains(&app.i18n.text("settings.launcher_log_directory")),
            "the label was squeezed by its value:\n{text}"
        );
    }

    #[test]
    fn an_empty_version_list_is_on_page_zero() {
        let mut app = paper_app(1);
        app.page = Page::DownloadCenter;
        app.versions.clear();
        app.version_selected = 0;
        let (rows, _) = screen(&mut app, 120, 40);
        let text = rows.join("\n");
        assert!(
            text.contains("0 / 0"),
            "an empty list claims page one:\n{text}"
        );
        assert!(!text.contains("1 / 0"), "{text}");
    }

    #[test]
    fn a_scrolled_task_list_never_prints_a_bar_without_its_name() {
        use crate::core::task::TaskState;
        let mut app = paper_app(1);
        app.page = Page::Tasks;
        app.task_snapshots = (0..6)
            .map(|index| snapshot(&format!("task{index}"), TaskState::Running, 1, Some(2)))
            .collect();
        // Last one selected in a short window: the offset lands inside an entry.
        app.task_selected = 5;
        let (rows, _) = screen(&mut app, 120, 14);

        let body = app.layout_chrome(Rect::new(0, 0, 120, 14)).main;
        let first = &rows[usize::from(body.y).saturating_add(3)];
        assert!(
            !first.contains('━'),
            "the list opens on a nameless progress bar: {first:?}"
        );
        assert!(
            first.contains("task"),
            "the first entry lost its name: {first:?}"
        );
    }

    #[test]
    fn every_java_row_carries_its_version_and_state() {
        use crate::core::java::models::JavaInstallation;
        use crate::tui::pages::settings::SettingsCategory;
        let mut app = paper_app(1);
        app.page = Page::Settings;
        app.java = (0..3)
            .map(|index| JavaInstallation {
                executable: std::path::PathBuf::from(format!("C:/java/{index}/bin/java.exe")),
                version: format!("17.0.{index}"),
                major: 17,
                vendor: Some("Eclipse".into()),
                is_jdk: true,
                lts: true,
            })
            .collect();
        app.java_selected = 0;
        app.settings.select_category(SettingsCategory::Java);
        let (rows, _) = screen(&mut app, 120, 40);

        let state = app.i18n.text("java.available");
        let mut seen = 0;
        for row in rows.iter().filter(|row| row.contains("java.exe")) {
            assert!(
                row.contains("Java 17.0."),
                "a java row lost its version column: {row:?}"
            );
            assert!(
                row.contains(&state),
                "the state word is cut to {:?}: {row:?}",
                &state[..state.len().min(8)]
            );
            seen += 1;
        }
        assert!(seen >= 3, "too few java rows drawn: {seen}");
    }

    #[test]
    fn every_directory_row_carries_its_instance_count() {
        use crate::tui::pages::settings::SettingsCategory;
        let mut app = paper_app(1);
        app.page = Page::Settings;
        app.config.game_directories = (0..3)
            .map(|index| std::path::PathBuf::from(format!("C:/games/{index}")))
            .collect();
        app.settings
            .select_category(SettingsCategory::GameDirectories);
        let (rows, _) = screen(&mut app, 120, 40);

        let count = app
            .i18n
            .format("settings.directory_instances", &[("count", "0")]);
        let mut seen = 0;
        for row in rows.iter().filter(|row| row.contains("C:/games/")) {
            assert!(
                row.contains(&count),
                "a directory row lost its instance count: {row:?}"
            );
            seen += 1;
        }
        assert!(seen >= 3, "too few directory rows drawn: {seen}");
    }

    #[test]
    fn the_compatibility_column_holds_every_status_word() {
        let app = paper_app(1);
        let width = app.compatibility_column_width();
        for key in [
            "download.compatible",
            "download.unknown_compatibility",
            "download.incompatible",
        ] {
            let label = app.i18n.text(key);
            assert!(
                palette::width(&label) as u16 <= width,
                "{key} ({label:?}) does not fit the {width}-cell column"
            );
        }
    }

    #[test]
    fn the_overview_legend_names_the_keys_the_page_binds() {
        let mut app = paper_app(2);
        app.page = Page::Overview;
        let (rows, _) = screen(&mut app, 120, 35);

        let launch = app.i18n.text("action.launch");
        let row = rows
            .iter()
            .find(|row| row.contains("Enter") && row.contains(&launch))
            .expect("the launch legend");
        let x = palette::width(&row[..row.find("Enter").expect("key column")]);
        assert!(x > 50, "the legend left the right column: {row:?}");

        // The second line names the page's other key. `L` belongs to the
        // Instances page and must not appear here.
        let new = app.i18n.text("action.new");
        assert!(
            rows.iter()
                .any(|row| row.contains('n') && row.contains(&new)),
            "the create key is missing from the legend:\n{}",
            rows.join("\n")
        );
    }

    #[test]
    fn the_current_instance_is_marked_in_the_status_column() {
        let mut app = paper_app(3);
        app.page = Page::Instances;
        let id = app.instances.items[1].id;
        app.instances.set_current(Some(id));
        let (rows, _) = screen(&mut app, 120, 30);

        let current = app.i18n.text("common.current");
        let chosen = rows
            .iter()
            .find(|row| row.contains("instance-1"))
            .expect("the current row");
        assert!(
            chosen.trim_end().ends_with(&current),
            "the current instance still prints its status: {chosen:?}"
        );
        let other = rows
            .iter()
            .find(|row| row.contains("instance-0"))
            .expect("a plain row");
        assert!(
            !other.contains(&current),
            "an unrelated row claims to be current: {other:?}"
        );
    }

    #[test]
    fn the_task_progress_ends_on_the_right_margin() {
        use crate::core::task::TaskState;
        let mut app = paper_app(1);
        app.page = Page::Tasks;
        app.task_snapshots = vec![snapshot(
            "Install Minecraft 1.21.4",
            TaskState::Running,
            620,
            Some(1000),
        )];
        app.task_selected = 0;
        let (rows, _) = screen(&mut app, 120, 24);

        let bar = rows
            .iter()
            .find(|row| row.contains('━'))
            .expect("the progress bar");
        assert!(
            bar.trim_end().ends_with('%'),
            "the bar does not close the row: {bar:?}"
        );
        let x = palette::width(&bar[..bar.find('━').expect("bar column")]);
        assert!(x > 60, "the bar is still in the name column: {bar:?}");
    }

    #[test]
    fn the_paper_dialog_keeps_a_blank_row_above_its_title() {
        let mut app = paper_app(1);
        app.modal = Some(crate::tui::modal::Modal::form(
            "Create instance",
            &["Name"],
            &["demo".into()],
            crate::tui::modal::FormAction::CreateInstance,
        ));
        let (rows, _) = screen(&mut app, 100, 24);
        let slip = app
            .modal
            .as_ref()
            .expect("modal")
            .paper_area(Rect::new(0, 0, 100, 24));

        let top = usize::from(slip.y);
        assert!(
            !rows[top].contains("Create instance"),
            "the title sits on the slip's edge: {:?}",
            rows[top]
        );
        assert!(
            rows[top + 1].contains("Create instance"),
            "the title is missing from its row: {:?}",
            rows[top + 1]
        );
        assert!(
            rows[top + 2].contains('─'),
            "the rule is not under the title: {:?}",
            rows[top + 2]
        );
    }

    #[test]
    fn the_paper_workspace_uses_the_same_search_line_as_the_pages() {
        let mut app = paper_app(2);
        let summary = app.instances.items[0].clone();
        app.instances.set_current(Some(summary.id));
        app.workspace = Some(crate::tui::pages::workspace::Workspace::open(&summary));
        app.workspace.as_mut().expect("workspace").category =
            crate::tui::pages::workspace::WorkspaceCategory::Mods;
        let (rows, _) = screen(&mut app, 120, 30);

        let label = app.i18n.text("common.search");
        let row = rows
            .iter()
            .find(|row| row.contains(&label))
            .expect("the search line");
        assert!(
            row.contains('…'),
            "the empty field lost its placeholder: {row:?}"
        );
        assert!(
            !row.contains('⌕'),
            "the workspace still uses its magnifier card: {row:?}"
        );
    }

    #[test]
    fn the_overview_wordmark_is_the_quadrant_face() {
        let mut app = paper_app(1);
        app.page = Page::Overview;
        let (rows, _) = screen(&mut app, 120, 40);
        let body = app.layout_chrome(Rect::new(0, 0, 120, 40)).main;

        let band = &rows[usize::from(body.y)..usize::from(body.y).saturating_add(4)];
        let glyphs = "▛▜▟▙▖▗▘▝▀▄▌▐█";
        for (index, row) in band.iter().enumerate() {
            assert!(
                row.chars().any(|character| glyphs.contains(character)),
                "wordmark row {index} is blank: {row:?}"
            );
        }
        assert!(
            !rows.iter().any(|row| row.contains("█████")),
            "the old five-cell block face is back"
        );
    }

    #[test]
    fn the_search_line_sits_on_its_rule() {
        let mut app = paper_app(1);
        let field = format!("{} …", app.i18n.text("common.search"));
        for page in [Page::DownloadCenter, Page::Logs] {
            app.page = page;
            let (rows, _) = screen(&mut app, 120, 24);
            // The empty field prints `search …`; the footer only prints the key,
            // so this cannot match the hint line.
            let y = rows
                .iter()
                .position(|row| row.contains(&field))
                .unwrap_or_else(|| panic!("{page:?}: no search line"));
            assert!(
                rows[y - 1].contains('─'),
                "{page:?}: a blank row sits above the search: {:?}",
                rows[y - 1]
            );
        }
    }

    #[test]
    fn the_workspace_search_box_is_one_row() {
        let mut app = paper_app(2);
        let summary = app.instances.items[0].clone();
        app.instances.set_current(Some(summary.id));
        app.workspace = Some(crate::tui::pages::workspace::Workspace::open(&summary));
        app.workspace.as_mut().expect("workspace").category =
            crate::tui::pages::workspace::WorkspaceCategory::Mods;
        screen(&mut app, 120, 30);

        let hits = app
            .hitmap
            .hitboxes()
            .into_iter()
            .filter(|hitbox| {
                matches!(
                    hitbox.action,
                    Action::Page {
                        act: crate::tui::action::PageAct::Search(_),
                        ..
                    }
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(hits.len(), 1, "the search field lost its hit box");
        assert_eq!(
            hits[0].rect.height, 1,
            "the search hit box spans {} rows; hover paints a slab again",
            hits[0].rect.height
        );
    }

    #[test]
    fn kraft_prints_on_its_own_stock() {
        let mut app = paper_app(3);
        app.page = Page::Overview;
        app.config.state.ui.paper_theme = PaperTheme::Kraft;
        let pal = app.paper_palette();
        let (_, buffer) = screen(&mut app, 120, 40);
        assert_eq!(pal.bg, Color::Rgb(0xE7, 0xDA, 0xC3));
        assert_eq!(buffer[(60, 20)].bg, pal.bg);
        // The accent is the kraft one, not the ivory one.
        assert_ne!(
            pal.accent,
            PaperTheme::Ivory.palette(ThemeMode::Light).accent
        );
    }

    #[test]
    fn the_night_print_keeps_the_page_and_the_margin_bar() {
        for theme in PaperTheme::ALL {
            for page in [Page::Overview, Page::Instances] {
                let mut app = paper_app(19);
                app.page = page;
                app.config.state.ui.paper_theme = theme;
                app.config.state.ui.theme_mode = ThemeMode::Dark;
                app.instances.focused = 3;
                let pal = app.paper_palette();
                assert_ne!(
                    pal.bg,
                    PaperTheme::Ivory.palette(ThemeMode::Light).bg,
                    "{theme:?}: the night print must not reuse the day sheet"
                );

                let (rows, buffer) = screen(&mut app, 120, 40);
                for y in 0..40u16 {
                    for x in 0..120u16 {
                        let continuation =
                            x > 0 && Line::from(buffer[(x - 1, y)].symbol()).width() > 1;
                        if continuation {
                            continue;
                        }
                        assert_eq!(
                            buffer[(x, y)].bg,
                            pal.bg,
                            "{theme:?}/{page:?}: unpainted cell at ({x}, {y})"
                        );
                    }
                }
                assert!(rows.join("\n").contains("/ 7"), "{theme:?}/{page:?}");

                if page == Page::Instances {
                    let selected = rows
                        .iter()
                        .find(|row| row.contains('▌'))
                        .expect("the night print keeps the margin bar");
                    assert!(selected.contains("instance-3"), "{selected}");
                    let selected_y = rows
                        .iter()
                        .position(|row| row.contains('▌'))
                        .expect("selected row") as u16;
                    let name_byte = selected.find("instance-3").expect("name column");
                    let name_x = palette::width(&selected[..name_byte]) as u16;
                    assert!(
                        buffer[(name_x, selected_y)]
                            .modifier
                            .contains(ratatui::style::Modifier::UNDERLINED),
                        "{theme:?}: the selected name is underlined"
                    );
                }
            }
        }
    }

    #[test]
    fn hovering_an_unselected_row_tints_it_on_every_sheet() {
        // Both day sheets and both night prints: the hover mix is a share of
        // the paper, so it has to show on all six.
        for theme in PaperTheme::ALL {
            for mode in [ThemeMode::Light, ThemeMode::Dark] {
                let mut app = paper_app(9);
                app.config.state.ui.paper_theme = theme;
                app.config.state.ui.theme_mode = mode;
                app.instances.focused = 0;
                let (_, before) = screen(&mut app, 120, 40);
                let row = app
                    .hitmap
                    .hitboxes()
                    .iter()
                    .find(|hitbox| hitbox.action == Action::row(Page::Instances, 3))
                    .map(|hitbox| hitbox.rect)
                    .expect("row 3");
                let sample = (row.x + 4, row.y);
                assert_eq!(before[sample].bg, app.paper_palette().bg);

                app.hover = Some(Action::row(Page::Instances, 3));
                let (_, after) = screen(&mut app, 120, 40);
                assert_ne!(
                    after[sample].bg, before[sample].bg,
                    "{theme:?}/{mode:?}: hover is invisible on this sheet"
                );
            }
        }
    }
}
