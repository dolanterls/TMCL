//! Preset dispatch, shared layout geometry and the footer hint rows.
//!
//! This is the only place that decides which preset draws. `classic`'s own
//! frame lives in `view/classic/frame.rs`, beside the pages it composes.

use super::*;
use crate::tui::action::{ModalAct, OverlayAct};
use crate::tui::app::view::sheet::Sheet;
use crate::tui::hit::Layer;
use crate::tui::palette;

impl App {
    pub(in crate::tui::app) fn layout_chrome(&self, area: Rect) -> ChromeRegions {
        let mut regions = match self.preset() {
            UiPreset::Nightlight => crate::tui::shell::layout_chrome_nightlight_with_footer(
                area,
                self.preset_footer_rows(area.width),
            ),
            UiPreset::Paper => crate::tui::shell::layout_chrome_paper_with_footer(
                area,
                self.preset_footer_rows(area.width),
            ),
            UiPreset::Shelf => crate::tui::shell::layout_chrome_shelf_with_footer(
                area,
                self.preset_footer_rows(area.width),
            ),
            UiPreset::Classic => crate::tui::shell::layout_chrome_with_footer(
                area,
                self.preset_footer_rows(area.width),
            ),
        };
        regions.modal = self.modal.as_ref().map(|modal| modal.area(area));
        regions
    }

    /// Rows the footer needs at `width`, per preset.
    ///
    /// `classic`, `nightlight` and `shelf` return 2, so neither a long status
    /// message nor a narrow hint row can change the page height. `paper` adds
    /// one for its rule. Counted from the page hints, not the active mode, so a
    /// dialog cannot resize the page.
    fn preset_footer_rows(&self, _width: u16) -> u16 {
        match self.preset() {
            UiPreset::Nightlight => 2,
            // Paper spends one row on the rule above its footer, so the two
            // bands the other presets draw need three.
            UiPreset::Paper => 3,
            UiPreset::Shelf => 2,
            UiPreset::Classic => 2,
        }
    }

    pub(in crate::tui::app) fn is_narrow(&self) -> bool {
        self.current_regions().mode == LayoutMode::Narrow
    }

    /// Where the download page's list body starts, per preset.
    ///
    /// `classic` lays the page out in five sections; `shelf` puts the
    /// categories, a rule, the search and a second rule above the list. The
    /// painter, the wheel window and the tests all read this one rectangle, so
    /// a preset cannot scroll one area while drawing another.
    pub(in crate::tui::app) fn download_list_area(&self, area: Rect) -> Rect {
        if self.preset() == UiPreset::Shelf {
            return crate::tui::app::view::shelf::download_body(area);
        }
        self.current_download_page_sections(area)
            .map_or(area, |sections| sections[4])
    }

    pub(in crate::tui::app) fn render_compact_list(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        title: &str,
        items: &[String],
        selected: usize,
        window: RowWindow,
    ) -> RowLayout {
        let layout = self
            .row_windows
            .resolve(window, RowLayout::list(area, items.len(), selected));
        crate::tui::widgets::list::render_layout(
            layout, frame, area, title, items, selected, true, self.theme,
        );
        layout
    }

    /// Which scroll slot the download list uses right now.
    ///
    /// The page hosts three different lists behind one body; keying the window
    /// by the one on screen keeps a category switch from inheriting the other's
    /// position.
    pub(in crate::tui::app) fn download_row_window(&self) -> RowWindow {
        if self.download_center.category == DownloadCategory::GameVersions {
            RowWindow::Versions
        } else if self.resource_detail.is_some() {
            RowWindow::ResourceFiles
        } else {
            RowWindow::ResourceProjects
        }
    }

    /// Shared geometry for a rendered collection: a table or a plain list.
    ///
    /// `nightlight` and `shelf` answer for themselves; every other preset takes
    /// a table in the full size classes and a list when narrow.
    pub(in crate::tui::app) fn list_layout(
        &self,
        area: Rect,
        item_count: usize,
        selected: usize,
    ) -> RowLayout {
        if self.preset() == UiPreset::Nightlight {
            // Three-row cards with one blank row between them read as separate
            // panels; narrow terminals drop to a single line per item so the
            // page still fits.
            if self.is_narrow() {
                return RowLayout::list(area, item_count, selected);
            }
            let stride = match self.page {
                Page::Instances | Page::Accounts | Page::Tasks => 3,
                _ => 1,
            };
            return if stride > 1 {
                RowLayout::cards_with_gap(area, item_count, selected, stride, 1)
            } else {
                RowLayout::table(area, item_count, selected)
            };
        }
        if self.preset() == UiPreset::Shelf {
            // No column header and no borders on the result lists, in every
            // size class: the body is the window. Checked before `is_narrow`,
            // whose bordered `list` would make `version_visible_rows` two rows
            // shorter than the rows the painter draws at 80 columns.
            RowLayout::list_bare(area, item_count, selected)
        } else if self.is_narrow() {
            RowLayout::list(area, item_count, selected)
        } else {
            RowLayout::table(area, item_count, selected)
        }
    }

    /// The one entry point the event loop calls.
    ///
    /// The `match` is exhaustive on purpose: a new `UiPreset` variant fails to
    /// compile here (and in `layout_chrome` / `preset_footer_rows`), which is
    /// the reminder to add its directory under `app/view/`. Written as three
    /// `if`s, a new preset would silently draw as `classic`.
    pub fn draw(&self, frame: &mut Frame<'_>) {
        let area = frame.area();
        // Every frame starts a fresh hit map: whatever is not registered now is
        // not clickable, so the map can never describe a stale screen.
        self.hitmap
            .begin((area.width, area.height), self.resize_gen);
        match self.preset() {
            UiPreset::Classic => self.draw_classic(frame, area),
            UiPreset::Nightlight => self.draw_nightlight(frame),
            UiPreset::Paper => self.draw_paper(frame, area),
            UiPreset::Shelf => self.draw_shelf(frame, area),
        }
        if self.config.state.ui.theme_mode == ThemeMode::Terminal {
            // Order matters: the fills go first, so the ink pass can test
            // `bg == Reset` to mean "this glyph sits straight on the terminal".
            self.punch_through_terminal(frame);
            self.lift_terminal_ink(frame);
        }
        // Drawn after the punch, so the page has already handed its fills back
        // and a dialog keeps its own background. Drawing overlays inside a
        // preset's draw would make `跟随终端` skip the punch while one is open.
        self.draw_overlays(frame, area);
    }

    /// Detail, help and dialogs, in whichever frame the preset uses.
    ///
    /// One dispatch so the punch above cannot be skipped by a preset that drew
    /// its own overlays inline.
    fn draw_overlays(&self, frame: &mut Frame<'_>, area: Rect) {
        match self.preset() {
            UiPreset::Classic => self.classic_overlays(frame, area),
            UiPreset::Nightlight => self.nightlight_overlays(frame, area),
            UiPreset::Paper => self.paper_overlays(frame, area),
            UiPreset::Shelf => self.shelf_overlays(frame, area),
        }
    }

    /// Hands literal page fills back to the terminal by setting their
    /// background to `Reset`, so the terminal background shows through.
    ///
    /// Covers only cells painted with a literal `pal.surface` or `pal.bg`. Rows
    /// and selection bands take their colours from [`Sheet::fill`], which
    /// returns `Reset` on a following sheet; borders, text, pills, progress
    /// bars and key caps keep their own paints because they carry information
    /// rather than containment.
    fn punch_through_terminal(&self, frame: &mut Frame<'_>) {
        // No overlay guard: `App::draw` runs this between the page and the
        // overlays, so a dialog's own background is never punched. Including it
        // would leave the dialog text on the terminal background with no fill
        // behind it.
        let fills = self.terminal_fills();
        if fills.iter().all(Option::is_none) {
            return;
        }
        let area = frame.area();
        let buffer = frame.buffer_mut();
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                let Some(cell) = buffer.cell_mut((x, y)) else {
                    continue;
                };
                if fills.contains(&Some(cell.bg)) {
                    cell.bg = Color::Reset;
                }
            }
        }
    }

    /// Whether start-up should ask the terminal for its background colour.
    ///
    /// Only when the answer would change a frame: the switch is on *and* the
    /// sheet is following the terminal. Asking costs a round trip on every
    /// launch, so nobody who runs `深色` pays for it.
    pub(crate) fn wants_terminal_background(&self) -> bool {
        self.config.state.ui.detect_terminal
            && self.config.state.ui.theme_mode == ThemeMode::Terminal
    }

    pub(crate) fn set_terminal_background(&mut self, light: Option<bool>) {
        self.terminal_background = light;
    }

    /// True when the sheet should be the light one because the terminal is.
    ///
    /// Only `跟随终端` can flip: `深色` and `浅色` already name the sheet the
    /// user asked for.
    pub(in crate::tui::app) fn light_terminal(&self) -> bool {
        self.config.state.ui.detect_terminal
            && self.config.state.ui.theme_mode == ThemeMode::Terminal
            && self.terminal_background == Some(true)
    }

    /// Raises faint glyphs to a legible floor, but only the ones painted
    /// straight on the terminal.
    ///
    /// A cell whose background is a pill, a key cap or a selection band is
    /// left alone: its ink was chosen against that fill, and lifting it would
    /// lower the contrast the fill was built to guarantee.
    fn lift_terminal_ink(&self, frame: &mut Frame<'_>) {
        if !self.config.state.ui.lift_dim_text {
            return;
        }
        // `classic` declares no fills: it never painted a page, so its ink was
        // chosen for the terminal's own palette and needs no help. Lifting it
        // would quietly rewrite a preset that opted out of the whole feature.
        if self.terminal_fills().iter().all(Option::is_none) {
            return;
        }
        // 0.30 is a little below the 4.5:1 ratio against mid-grey (#808080 is
        // 0.216): dim enough to keep the secondary tone, bright enough to stay
        // readable on a light background.
        const FLOOR: f64 = 0.30;
        for cell in frame.buffer_mut().content.iter_mut() {
            if cell.bg != Color::Reset || cell.symbol() == " " {
                continue;
            }
            if palette::relative_luminance(cell.fg) >= FLOOR {
                continue;
            }
            cell.fg = palette::lift_to_luminance(cell.fg, FLOOR);
        }
    }

    /// The fills each preset lays under its content, lowest first.
    ///
    /// Each palette answers for itself through [`Sheet`]: a preset that keeps
    /// its selected-row band in a slot (`nightlight`'s `selection`) has to say
    /// so next to its colours, not here.
    fn terminal_fills(&self) -> [Option<Color>; 4] {
        match self.preset() {
            // Classic paints no background at all; its `Theme` keeps `Reset`.
            UiPreset::Classic => [None; 4],
            UiPreset::Nightlight => self.nightlight_palette().fills(),
            UiPreset::Paper => self.paper_palette().fills(),
            UiPreset::Shelf => self.shelf_palette().fills(),
        }
    }

    /// Tints the region under the pointer.
    ///
    /// Hover is derived from the same hit map that routes clicks, so every
    /// registered target is hoverable by construction. Only the cell background
    /// changes: glyphs, line counts and rectangles are untouched, which is what
    /// keeps the hit boxes stable while the pointer moves.
    pub(in crate::tui::app) fn paint_hover(&self, frame: &mut Frame<'_>) {
        let Some(hover) = self.hover else {
            return;
        };
        // Overlay blockers are not drawn controls, and neither is the dialog
        // body; everything else inside a dialog (fields, options, key caps) is
        // hoverable like any page target.
        match hover {
            Action::Overlay(_) => return,
            Action::Modal(ModalAct::Noop) => return,
            _ => {}
        }
        // The selected row keeps its own style; hover never overrides it.
        if self.is_selected_target(hover) {
            return;
        }
        // The action is not enough to find the rectangle: a key cap and the row
        // it acts on can share one (`Option(n)` on both the picker's `Enter` and
        // the option under the highlight), and matching on the action alone
        // tinted whichever was registered last. The pointer cell says which.
        // Without one (nothing but tests set `hover` by hand) the action alone
        // still answers, which is what this did before.
        let point = self
            .hover_at
            .map(|(column, row)| ratatui::layout::Position::new(column, row));
        let Some(rect) = self
            .hitmap
            .hitboxes()
            .iter()
            .rev()
            .find(|hitbox| {
                hitbox.action == hover && point.is_none_or(|point| hitbox.rect.contains(point))
            })
            .map(|hitbox| hitbox.rect)
        else {
            return;
        };
        let tint = if self.preset() != UiPreset::Classic {
            self.page_accent()
        } else {
            self.theme.accent
        };
        let area = frame.area();
        let rect = rect.intersection(area);
        let buffer = frame.buffer_mut();
        for y in rect.top()..rect.bottom() {
            for x in rect.left()..rect.right() {
                let cell = &mut buffer[(x, y)];
                // Presets that paint RGB surfaces shift the cell background on
                // hover. Under `跟随终端` `classic` has no cell of its own
                // (`Color::Reset`), where there is nothing to darken; there the
                // highlight goes to the foreground so it survives any palette.
                if let (
                    ratatui::style::Color::Rgb(r, g, b),
                    ratatui::style::Color::Rgb(tr, tg, tb),
                ) = (cell.bg, tint)
                {
                    let mix = |from: u8, to: u8| -> u8 {
                        ((u16::from(from) * 86 + u16::from(to) * 14) / 100)
                            .try_into()
                            .unwrap_or(0)
                    };
                    cell.bg = ratatui::style::Color::Rgb(mix(r, tr), mix(g, tg), mix(b, tb));
                } else {
                    cell.fg = tint;
                }
            }
        }
    }

    /// Registers a full-screen blocker under each blocking overlay.
    ///
    /// Both presets call this from their own draw path so the layer contract
    /// stays identical. The blocker is registered before the overlay body, so a
    /// hit test walking layers in reverse closes on a click outside and does
    /// nothing inside, without comparing coordinates.
    pub(in crate::tui::app) fn register_overlay_hits(&self, area: Rect) {
        if self.detail_open {
            // `paper` draws a right-hand column or a full page, so the rect that
            // counts as "inside the card" is its own, not the centred popup.
            let card = if self.preset() == UiPreset::Paper {
                self.paper_detail_rect(area)
            } else {
                crate::tui::chrome::popup_rect(area)
            };
            self.block_overlay(Layer::Detail, area, card);
        }
        if self.diagnosis_open {
            // Same rectangle the painter will ask for, measured from the
            // verdict's own row count.
            let card = self.diagnosis_rect(area);
            self.block_overlay(Layer::Diagnosis, area, card);
        }
        if self.help_open {
            // The help card is sized from its own rows, so the blocker has to
            // ask for the same rectangle the painter will use. A fixed 16-row
            // box closed the panel from its own lower half.
            let lines = self.help_lines();
            let card = help_rect(area, &lines);
            self.block_overlay(Layer::Help, area, card);
        }
        if let Some(modal) = &self.modal {
            self.hitmap.set_layer(Layer::Modal);
            self.hitmap.add(area, Action::Modal(ModalAct::Noop));
            self.hitmap
                .scroll(area, crate::tui::action::ScrollTarget::Blocked);
            let dialog = if self.preset() == UiPreset::Paper {
                // The slip reclaims the two rows classic spends on its frame;
                // using `area()` here would leave the wheel dead on those rows.
                modal.paper_area(area)
            } else {
                modal.area(area)
            };
            self.hitmap.add(dialog, Action::Modal(ModalAct::Noop));
            self.hitmap
                .scroll(dialog, crate::tui::action::ScrollTarget::ModalOptions);
        }
        self.hitmap.set_layer(Layer::Base);
    }

    /// One blocking overlay: the whole screen closes it, its own card does not.
    fn block_overlay(&self, layer: Layer, area: Rect, card: Rect) {
        self.hitmap.set_layer(layer);
        self.hitmap.add(area, Action::Overlay(OverlayAct::Close));
        self.hitmap
            .scroll(area, crate::tui::action::ScrollTarget::Blocked);
        self.hitmap.add(card, Action::Overlay(OverlayAct::Noop));
    }

    pub(in crate::tui::app) fn hint(&self, key: &str, label_key: &str) -> crate::tui::chrome::Hint {
        crate::tui::chrome::Hint::new(key, label_key)
    }

    pub(in crate::tui::app) fn hints(&self) -> Vec<crate::tui::chrome::Hint> {
        match self.mode() {
            Mode::Help => {
                let mut hints = vec![self.hint("Esc", "action.back")];
                // Only worth saying when the reference is longer than the card.
                if self.help_max_scroll() > 0 {
                    hints.push(self.hint("↑↓", "action.scroll"));
                }
                hints.push(self.hint("q", "common.quit"));
                hints
            }
            // The dialog prints its own key row with clickable capsules and is
            // the only thing on screen that accepts input, so the footer is
            // left empty.
            Mode::Form(_) | Mode::Filter | Mode::Confirm(_) => Vec::new(),
            Mode::Diagnosis => {
                let mut hints = vec![
                    self.hint("Esc", "hint.close"),
                    self.hint("↑↓", "action.scroll"),
                ];
                if self.diagnosis_max_scroll() > 0 {
                    hints.push(self.hint("PgUp/PgDn", "hint.page"));
                }
                hints.push(self.hint("F1", "action.help"));
                hints
            }
            Mode::Detail(target) => {
                let mut hints = vec![
                    self.hint("Esc", "hint.close"),
                    self.hint("↑↓", "action.navigate"),
                ];
                if self.page == Page::DownloadCenter
                    && self.download_center.category != DownloadCategory::GameVersions
                {
                    hints.push(self.hint("Enter", "action.open_install"));
                }
                // `L` starts the game from the instance detail; it is bound in
                // this mode but was never printed, so the shortcut read as
                // missing.
                if matches!(target, crate::tui::command::DetailTarget::Instances) {
                    hints.push(self.hint("L", "action.launch"));
                }
                hints.push(self.hint("?", "action.help").with_priority(2));
                hints.push(self.hint("q", "common.quit").with_priority(0));
                hints
            }
            Mode::Search(target) => {
                let enter = if matches!(target, SearchTarget::Settings) {
                    "action.jump"
                } else {
                    "action.search"
                };
                vec![self.hint("Enter", enter), self.hint("Esc", "hint.cancel")]
            }
            Mode::Workspace => {
                let category = self.workspace.as_ref().map(|workspace| workspace.category);
                let mut hints = vec![self.hint("↑↓", "action.navigate")];
                let mut enter_listed = false;
                match category {
                    Some(crate::tui::pages::workspace::WorkspaceCategory::Launch) => {
                        hints.push(self.hint("←/→", "action.adjust"));
                        hints.push(self.hint("Space", "action.toggle"));
                        hints.push(self.hint("R", "action.reset"));
                    }
                    Some(crate::tui::pages::workspace::WorkspaceCategory::ResourcePacks) => {
                        let empty = self.workspace.as_ref().is_some_and(|workspace| {
                            workspace
                                .content
                                .count(crate::core::content::ContentKind::ResourcePack)
                                == 0
                        });
                        if empty {
                            hints.push(self.hint("Enter", "action.download"));
                        } else {
                            hints.push(self.hint("Space", "action.toggle"));
                            hints.push(self.hint("J/K", "action.reorder"));
                            hints.push(self.hint("Enter", "action.details"));
                            hints.push(self.hint("d", "action.delete"));
                        }
                        enter_listed = true;
                        hints.push(self.hint("i", "action.import"));
                        hints.push(self.hint("o", "action.open"));
                        hints.push(self.hint("/", "action.search"));
                    }
                    Some(crate::tui::pages::workspace::WorkspaceCategory::Install) => {
                        hints.push(self.hint("Enter", "action.install"));
                        enter_listed = true;
                        hints.push(self.hint("d", "action.uninstall"));
                    }
                    Some(crate::tui::pages::workspace::WorkspaceCategory::Worlds) => {
                        let empty = self.workspace.as_ref().is_some_and(|workspace| {
                            workspace
                                .content
                                .count(crate::core::content::ContentKind::World)
                                == 0
                        });
                        if empty {
                            hints.push(self.hint("Enter", "action.download"));
                        } else {
                            hints.push(self.hint("b", "action.backup"));
                            hints.push(self.hint("r", "action.restore"));
                            hints.push(self.hint("c", "action.clone"));
                            hints.push(self.hint("Enter", "action.details"));
                            hints.push(self.hint("d", "action.delete"));
                        }
                        enter_listed = true;
                        hints.push(self.hint("p", "action.datapacks"));
                        hints.push(self.hint("o", "action.open"));
                    }
                    Some(category) if category.content_kind().is_some() => {
                        let kind = category.content_kind().expect("content category");
                        let empty = self
                            .workspace
                            .as_ref()
                            .is_some_and(|workspace| workspace.content.count(kind) == 0);
                        if empty {
                            // A data pack has no download category: `i`
                            // imports a zip straight into the world.
                            if kind != crate::core::content::ContentKind::Datapack {
                                hints.push(self.hint("Enter", "action.download"));
                                enter_listed = true;
                            }
                        } else {
                            hints.push(self.hint("Space", "action.toggle"));
                            hints.push(self.hint("Enter", "action.details"));
                            enter_listed = true;
                            hints.push(self.hint("d", "action.delete"));
                        }
                        hints.push(self.hint("i", "action.import"));
                        if kind == crate::core::content::ContentKind::Mod {
                            hints.push(self.hint("u", "action.check_updates"));
                        }
                        hints.push(self.hint("o", "action.open"));
                        hints.push(self.hint("/", "action.search"));
                    }
                    Some(crate::tui::pages::workspace::WorkspaceCategory::Manage) => {
                        hints.push(self.hint("Enter", "action.run"));
                        hints.push(self.hint("d", "action.delete"));
                        enter_listed = true;
                    }
                    Some(crate::tui::pages::workspace::WorkspaceCategory::Overview) => {
                        // Enter has no job here; the row actions are the only
                        // things that do anything on a read-only summary.
                        hints.push(self.hint("L", "action.launch"));
                        hints.push(self.hint("e", "action.rename"));
                        hints.push(self.hint("o", "action.open"));
                        // Least important row action: on a narrow terminal it
                        // yields to the category and back keys.
                        hints.push(self.hint("x", "action.export_script").with_priority(11));
                        hints.push(self.hint("U", "action.update_modpack").with_priority(12));
                        enter_listed = true;
                    }
                    _ => {}
                }
                hints.push(self.hint("[ ]", "action.category"));
                if !enter_listed {
                    hints.push(self.hint("Enter", "action.open"));
                }
                hints.push(self.hint("Esc", "action.back"));
                // `F5` (rescan) and `?` work here too, but the row is already at
                // capacity: adding them pushed `U` (update modpack) and even
                // `Esc` off the 120-column footer. They stay keyboard-only until
                // the workspace has a second hint row to put them on.
                hints
            }
            Mode::Normal => self.page_hints(),
        }
    }

    pub(in crate::tui::app) fn page_hints(&self) -> Vec<crate::tui::chrome::Hint> {
        let mut hints = match self.page {
            Page::Overview => self.overview_hints(),
            Page::Instances => self.instances_hints(),
            Page::DownloadCenter => self.downloads_hints(),
            Page::Tasks => self.tasks_hints(),
            Page::Accounts => self.accounts_hints(),
            Page::Logs => self.logs_hints(),
            Page::Settings => self.settings_hints(),
        };
        // Advertise the help key: it existed all along but nothing on screen
        // mentioned it, so it read as missing.
        hints.push(self.hint("?", "action.help").with_priority(2));
        hints.push(self.hint("Tab", "action.next_page").with_priority(1));
        hints.push(self.hint("q", "common.quit").with_priority(0));
        hints
    }
}
