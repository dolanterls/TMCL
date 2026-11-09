//! `nightlight` preset: state shared by every page.
//!
//! The palette is a pure function of six inputs, so it is cached against all of
//! them at once. That keeps the 250 ms frame tick from re-deriving the ladder
//! four times a second while still reacting immediately to a settings change.
//!
//! Clean-room notice: see `crate::tui::app::view::nightlight`.

use ratatui::Frame;
use ratatui::style::{Color, Style};
use ratatui::widgets::Block;

use crate::core::config::models::{AccentColor, NightlightTheme, ThemeMode, UiPreset};
use crate::platform::windows::local_minutes_since_midnight;
use crate::tui::shell::{ChromeRegions, LayoutMode};

use super::{App, Page};

mod accounts;
mod downloads;
mod instances;
mod live;
mod logs;
mod modals;
mod overview;
mod settings;
mod shell;
pub(in crate::tui::app) mod sidebar;
mod tasks;
/// Palette derivation and the piece library. `tui::modal` draws its dialog with
/// them, so both are crate-visible.
pub(crate) mod tone;
pub(crate) mod widgets;
mod workspace;

use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use self::tone::Palette;
use self::widgets::width;
use crate::tui::action::Action;

/// Noon. Used when the platform cannot report local time, so `live` falls back
/// to its neutral daytime accent instead of guessing a timezone.
const FALLBACK_MINUTE: u32 = 12 * 60;

/// Cells the footer status keeps for itself on a one-row footer: a glyph, a
/// short word and the elapsed clock. Anything longer is truncated, never
/// wrapped: a failure message must not be able to change the page height.
pub(in crate::tui::app) const STATUS_MIN_WIDTH: u16 = 13;

/// Everything the palette depends on. Any difference means a recompute; an
/// extra field here is cheaper than a wrong colour on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::tui::app) struct PaletteKey {
    minute: Option<u16>,
    theme: NightlightTheme,
    preset: UiPreset,
    mode: ThemeMode,
    /// Whether the mode is `跟随终端`. Kept apart from `mode` because a light
    /// terminal paints the light ladder *and* still withdraws its fills, so
    /// `Terminal + light` and `Light` must not share a cache entry.
    terminal: bool,
    high_contrast: bool,
    /// The two remaining inputs of `nightlight_seed`. Without them, flipping
    /// the clock switch or the accent inside one minute kept the cached
    /// palette until the minute rolled over.
    live_accent: bool,
    accent: AccentColor,
}

/// Test-only derivation counter. It lives on `App` rather than in a global so
/// parallel tests cannot see each other's increments.
#[cfg(test)]
mod counter {
    use super::App;

    pub(crate) fn computes(app: &App) -> usize {
        app.nightlight_computes.get()
    }

    pub(crate) fn reset(app: &App) {
        app.nightlight_computes.set(0);
    }
}

/// Cache slot carried by [`App`]. `None` until the first frame asks for a
/// palette; the key comparison does the invalidation.
pub(in crate::tui::app) type PaletteCache = std::cell::Cell<Option<(PaletteKey, Palette)>>;

impl App {
    /// Full frame for the `nightlight` preset.
    ///
    /// Order matters: the page colour goes down first, then the sidebar and the
    /// footer belong to the shell, the page fills the content column, and the
    /// overlays sit above everything.
    pub(in crate::tui::app) fn draw_nightlight(&self, frame: &mut Frame<'_>) {
        let area = frame.area();
        let palette = self.nightlight_palette();
        frame.render_widget(
            Block::default().style(Style::default().bg(palette.bg)),
            area,
        );
        let regions = self.layout_chrome(area);
        self.regions.set(regions);
        self.regions_size.set((area.width, area.height));
        if regions.mode == LayoutMode::TooSmall {
            shell::too_small(self, frame, &regions, &palette);
            return;
        }

        shell::sidebar_panel(self, frame, &regions, &palette);
        self.draw_nightlight_page(frame, &regions, &palette);
        shell::hairline(frame, &regions, &palette);
        shell::footer(self, frame, &regions, &palette);
        shell::toast(self, frame, &regions, &palette);
    }

    /// Same single-scrim contract as the other presets: one dimming pass below
    /// every overlay, then the shared blocker registration. Runs after the page
    /// has handed its fills back to the terminal.
    pub(in crate::tui::app) fn nightlight_overlays(&self, frame: &mut Frame<'_>, area: Rect) {
        let palette = self.nightlight_palette();
        if self.detail_open || self.diagnosis_open || self.help_open || self.modal.is_some() {
            crate::tui::widgets::scrim::dim(frame);
        }
        self.register_overlay_hits(area);
        if self.detail_open {
            modals::detail(self, frame, area, &palette);
        }
        if self.diagnosis_open {
            self.draw_diagnosis_overlay_with(frame, self.diagnosis_theme());
        }
        if self.help_open {
            modals::help(self, frame, area, &palette);
        }
        if let Some(modal) = &self.modal {
            self.hitmap.set_layer(crate::tui::hit::Layer::Modal);
            modals::render(self, frame, modal, area, &palette);
            self.hitmap.set_layer(crate::tui::hit::Layer::Base);
        }
        self.paint_hover(frame);
    }

    fn draw_nightlight_page(&self, frame: &mut Frame<'_>, regions: &ChromeRegions, pal: &Palette) {
        match self.page {
            Page::Overview => self.draw_nightlight_overview(frame, regions.main, pal),
            Page::Instances => {
                if self.workspace_is_open() {
                    self.draw_nightlight_workspace(frame, regions.main, pal);
                } else {
                    self.draw_nightlight_instances(frame, regions.main, pal);
                }
            }
            Page::DownloadCenter => self.draw_nightlight_downloads(frame, regions.main, pal),
            Page::Tasks => self.draw_nightlight_tasks(frame, regions.main, pal),
            Page::Accounts => self.draw_nightlight_accounts(frame, regions.main, pal),
            Page::Logs => self.draw_nightlight_logs(frame, regions.main, pal),
            Page::Settings => self.draw_nightlight_settings(frame, regions.main, pal),
        }
    }

    /// One row of filter chips. The chip rectangles come from the same widths
    /// that draw them, so clicks and glyphs cannot drift.
    ///
    /// The window always keeps the active chip on screen: six categories at 80
    /// columns overflow, and cycling onto an off-screen chip would leave the row
    /// looking unchanged. Hidden leading chips become an ellipsis.
    pub(in crate::tui::app) fn draw_pill_row(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        labels: &[String],
        active: usize,
        page: Page,
        pal: &Palette,
    ) {
        if area.is_empty() || labels.is_empty() {
            return;
        }
        let texts = labels
            .iter()
            .map(|label| format!(" {label} "))
            .collect::<Vec<_>>();
        let widths = texts
            .iter()
            .map(|text| u16::try_from(width(text)).unwrap_or(u16::MAX))
            .collect::<Vec<_>>();

        // Smallest first index whose `first..=active` span still fits.
        let mut first = 0usize;
        while first < active {
            let mut needed = 2u16; // the leading "… "
            for (offset, cells) in widths[first..=active].iter().enumerate() {
                needed =
                    needed
                        .saturating_add(*cells)
                        .saturating_add(if offset > 0 { 2 } else { 0 });
            }
            if needed <= area.width {
                break;
            }
            first += 1;
        }

        // How many chips fit, holding two cells back for a trailing ellipsis
        // whenever the row cannot show every remaining label.
        let start_x = area.x.saturating_add(if first > 0 { 2 } else { 0 });
        let mut walk = start_x;
        let mut drawn = first;
        for (index, cells) in widths.iter().enumerate().skip(first) {
            let gap: u16 = if index > first { 2 } else { 0 };
            let reserve: u16 = if index + 1 < labels.len() { 2 } else { 0 };
            let needed = gap.saturating_add(*cells).saturating_add(reserve);
            if walk.saturating_add(needed) > area.right() {
                break;
            }
            walk = walk.saturating_add(gap).saturating_add(*cells);
            drawn = index + 1;
        }

        let mut spans = Vec::new();
        let mut rects = Vec::new();
        let mut x = area.x;
        if first > 0 {
            spans.push(Span::styled(
                "… ",
                Style::default().fg(pal.faint).bg(pal.bg),
            ));
            x = x.saturating_add(2);
        }
        for index in first..drawn {
            if index > first {
                spans.push(Span::styled("  ", Style::default().bg(pal.bg)));
                x = x.saturating_add(2);
            }
            let cells = widths[index];
            let (fg, bg, modifier) = if index == active {
                (pal.bg, pal.accent, Modifier::BOLD)
            } else {
                (pal.muted, pal.bg, Modifier::empty())
            };
            spans.push(Span::styled(
                texts[index].clone(),
                Style::default().fg(fg).bg(bg).add_modifier(modifier),
            ));
            rects.push((index, Rect::new(x, area.y, cells, 1)));
            x = x.saturating_add(cells);
        }
        // Trailing "…" says the row is cut short. Without it a category that did
        // not fit left no trace: Worlds disappeared at 80 columns while Game
        // Versions was selected.
        if drawn < labels.len() {
            spans.push(Span::styled(
                " …",
                Style::default().fg(pal.faint).bg(pal.bg),
            ));
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(pal.bg)),
            area,
        );
        for (index, rect) in rects {
            self.hitmap.add(rect, Action::filter(page, index));
        }
    }

    /// The palette the `nightlight` drawing layer should use right now.
    pub(in crate::tui::app) fn nightlight_palette(&self) -> Palette {
        self.nightlight_palette_at(self.nightlight_minute())
    }

    /// Local minutes, or the pinned value in tests. Snapshots and the static
    /// frame assertions need a clock that does not advance between runs.
    fn nightlight_minute(&self) -> Option<u32> {
        #[cfg(test)]
        if let Some(minute) = self.nightlight_minute.get() {
            return Some(minute);
        }
        local_minutes_since_midnight()
    }

    /// Palette for an explicit clock reading. Tests pin the minute here so a
    /// run that straddles a minute boundary cannot look like a cache miss.
    fn nightlight_palette_at(&self, minute: Option<u32>) -> Palette {
        let ui = &self.config.state.ui;
        let mode = if self.light_terminal() {
            ThemeMode::Light
        } else {
            ui.theme_mode
        };
        let key = PaletteKey {
            minute: minute.map(|value| value as u16),
            theme: ui.nightlight_theme,
            preset: ui.preset,
            mode,
            terminal: ui.theme_mode == ThemeMode::Terminal,
            high_contrast: ui.high_contrast,
            live_accent: ui.live_accent,
            accent: ui.accent_color,
        };
        if let Some((cached, palette)) = self.nightlight_cache.get() {
            if cached == key {
                return palette;
            }
        }
        let mut palette = tone::palette(
            key.theme,
            key.mode,
            self.nightlight_seed(minute),
            key.high_contrast,
        );
        // `tone::palette` only withdraws the fills for `Terminal`; a light
        // terminal is still following, so the flag is set from the mode the
        // user picked rather than the ladder that was drawn.
        palette.transparent = key.terminal;
        self.nightlight_cache.set(Some((key, palette)));
        #[cfg(test)]
        self.nightlight_computes
            .set(self.nightlight_computes.get() + 1);
        palette
    }

    /// Local time for the sidebar status block: minutes since midnight plus a
    /// `HH:MM` label. `None` when the clock is unreadable, which the status
    /// block renders as `--:--`.
    pub(in crate::tui::app) fn nightlight_clock(&self) -> Option<(u32, String)> {
        self.nightlight_minute()
            .map(|minute| (minute, format!("{:02}:{:02}", minute / 60, minute % 60)))
    }

    /// Only `live` accepts a seed; the other seven themes are fixed designs.
    fn nightlight_seed(&self, minute: Option<u32>) -> Option<Color> {
        let ui = &self.config.state.ui;
        if ui.nightlight_theme != NightlightTheme::Live {
            return None;
        }
        if ui.live_accent {
            Some(live::accent_at(minute.unwrap_or(FALLBACK_MINUTE)))
        } else {
            Some(tone::to_color(tone::accent_seed(ui.accent_color)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::models::AccentColor;

    fn nightlight_app() -> App {
        let mut app = App::new();
        app.config.state.ui.preset = UiPreset::Nightlight;
        app
    }

    #[test]
    fn the_same_minute_reuses_one_palette() {
        let app = nightlight_app();
        counter::reset(&app);
        let first = app.nightlight_palette_at(Some(720));
        for _ in 0..100 {
            assert_eq!(app.nightlight_palette_at(Some(720)), first);
        }
        assert_eq!(counter::computes(&app), 1);
        // A different minute does recompute.
        let later = app.nightlight_palette_at(Some(1140));
        assert_ne!(later.bg, first.bg, "the live ladder follows the clock");
        assert_eq!(counter::computes(&app), 2);
    }

    #[test]
    fn settings_changes_invalidate_the_cache() {
        let mut app = nightlight_app();
        let first = app.nightlight_palette_at(Some(720));

        app.config.state.ui.nightlight_theme = NightlightTheme::Tokyo;
        let tokyo = app.nightlight_palette_at(Some(720));
        assert_ne!(tokyo, first);
        assert_eq!(tokyo.bg, Color::Rgb(0x1A, 0x1B, 0x26));

        app.config.state.ui.theme_mode = ThemeMode::Light;
        let light = app.nightlight_palette_at(Some(720));
        assert_ne!(light, tokyo);
        assert_eq!(light.bg, Color::Rgb(0xE6, 0xE7, 0xED));

        app.config.state.ui.high_contrast = true;
        let lifted = app.nightlight_palette_at(Some(720));
        assert_ne!(lifted, light);
        assert_eq!(lifted.text, light.text);
        assert_ne!(lifted.muted, light.muted);
    }

    #[test]
    fn the_clock_seed_only_applies_to_live() {
        let mut app = nightlight_app();
        app.config.state.ui.live_accent = true;
        let dawn = app.nightlight_palette_at(Some(6 * 60));
        let noon = app.nightlight_palette_at(Some(12 * 60));
        assert_ne!(dawn.accent, noon.accent);

        // Another theme ignores the clock entirely.
        app.config.state.ui.nightlight_theme = NightlightTheme::Nord;
        assert_eq!(
            app.nightlight_palette_at(Some(6 * 60)),
            app.nightlight_palette_at(Some(12 * 60))
        );
    }

    #[test]
    fn switching_the_clock_off_falls_back_to_the_accent_colour() {
        let mut app = nightlight_app();
        app.config.state.ui.live_accent = false;
        app.config.state.ui.accent_color = AccentColor::Cyan;
        let fixed = app.nightlight_palette_at(Some(23 * 60));
        assert_eq!(fixed.accent, Color::Rgb(0x88, 0xC0, 0xD0));
        assert_eq!(fixed, app.nightlight_palette_at(Some(12 * 60)));
    }

    #[test]
    fn switching_the_clock_off_inside_one_minute_repaints() {
        let mut app = nightlight_app();
        app.config.state.ui.live_accent = true;
        let clock = app.nightlight_palette_at(Some(12 * 60));

        // Same minute, different inputs: the cache must not win. Both fields
        // feed `nightlight_seed`, so both belong in the key.
        app.config.state.ui.live_accent = false;
        app.config.state.ui.accent_color = AccentColor::Cyan;
        let fixed = app.nightlight_palette_at(Some(12 * 60));
        assert_ne!(
            fixed.accent, clock.accent,
            "turning the clock off must repaint immediately"
        );
        assert_eq!(fixed.accent, Color::Rgb(0x88, 0xC0, 0xD0));

        app.config.state.ui.live_accent = true;
        assert_eq!(
            app.nightlight_palette_at(Some(12 * 60)).accent,
            clock.accent,
            "turning it back on must return to the clock colour"
        );
    }

    #[test]
    fn an_unreadable_clock_falls_back_to_daylight() {
        let app = nightlight_app();
        let none = app.nightlight_palette_at(None);
        let noon = app.nightlight_palette_at(Some(FALLBACK_MINUTE));
        assert_eq!(none, noon);
    }

    #[test]
    fn the_workspace_draws_both_rails() {
        use crate::tui::pages::instances::InstanceSummary;
        use crate::tui::pages::workspace::Workspace;
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let mut app = nightlight_app();
        let root = std::env::temp_dir().join(format!("tmcl-ws-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create workspace dir");
        let summary = InstanceSummary {
            id: uuid::Uuid::new_v4(),
            name: "TMCL-Test".to_owned(),
            version: "1.21.4".to_owned(),
            loader: "Fabric".to_owned(),
            loader_version: Some("0.16.9".to_owned()),
            status: "ready".to_owned(),
            path: root.clone(),
            launch_count: 3,
            play_time_seconds: 3600,
        };
        app.instances.apply(vec![summary.clone()]);
        app.instances.set_current(Some(summary.id));
        app.workspace = Some(Workspace::open(&summary));
        app.page = Page::Instances;

        for (width, height) in [(120, 40), (80, 24)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).expect("terminal");
            terminal
                .draw(|frame| app.draw(frame))
                .unwrap_or_else(|error| panic!("workspace {width}x{height}: {error}"));
            let buffer = terminal.backend().buffer();
            let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
            assert!(
                text.contains("TMCL-Test"),
                "workspace lost the instance name"
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn nightlight_draws_every_page_without_panicking() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let mut app = nightlight_app();
        for (width, height) in [(120, 40), (80, 24)] {
            for page in Page::ALL {
                app.page = page;
                let backend = TestBackend::new(width, height);
                let mut terminal = Terminal::new(backend).expect("terminal");
                terminal
                    .draw(|frame| app.draw(frame))
                    .unwrap_or_else(|error| panic!("{page:?} at {width}x{height}: {error}"));
                // The brand sits one row below the sidebar's top edge, never in
                // a banner and never jammed into the corner. The icon rail has
                // room for the moon only.
                let buffer = terminal.backend().buffer();
                // The brand is a band now: four rows of block glyphs on a wide
                // column, the single line ` moon TMCL ` on a rail.
                let column = app.regions.get().header.width;
                let brand_rows = crate::tui::app::view::nightlight::sidebar::brand_rows(
                    ratatui::layout::Rect::new(0, 0, column, height),
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
                let top: String = (0..width).map(|x| buffer[(x, 0)].symbol()).collect();
                assert!(
                    top.trim().is_empty(),
                    "{page:?} at {width}x{height} put something above the brand: {top:?}"
                );
                if column >= 20 {
                    assert!(
                        band.contains('▛') && band.contains('█'),
                        "{page:?} at {width}x{height} lost the block wordmark: {band:?}"
                    );
                } else {
                    assert!(
                        band.starts_with("  ☾"),
                        "{page:?} at {width}x{height} lost the rail brand: {band:?}"
                    );
                }
                // No page may end in a band of page colour. The page keeps a
                // one-row margin, so the last content row is `bottom - 2`, and
                // the settings page pins a one-line description there. Every
                // other page must paint that row edge to edge inside the margin;
                // a single painted cell is not enough.
                let palette = app.nightlight_palette();
                let regions = app.regions.get();
                let last = regions.main.bottom().saturating_sub(2);
                let left = regions.main.x.saturating_add(1);
                let right = regions.main.right().saturating_sub(1);
                if page == Page::Settings {
                    let described = (left..right).any(|x| buffer[(x, last)].symbol() != " ");
                    assert!(
                        described,
                        "{page:?} at {width}x{height} lost its description line"
                    );
                } else {
                    let bare = (left..right)
                        .filter(|x| buffer[(*x, last)].bg == palette.bg)
                        .count();
                    assert_eq!(
                        bare, 0,
                        "{page:?} at {width}x{height} left {bare} page-coloured cells on the last content row"
                    );
                }
            }
        }
    }
}
