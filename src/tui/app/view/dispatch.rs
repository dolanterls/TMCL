//! Preset dispatch, shared layout geometry and the footer hint rows.
//!
//! This is the only place that decides which preset draws. `classic`'s own
//! frame lives in `view/classic/frame.rs`, beside the pages it composes.

use super::*;
use crate::tui::action::{ModalAct, OverlayAct};
use crate::tui::app::view::sheet::Sheet;
use crate::tui::hit::Layer;

impl App {
    pub(in crate::tui::app) fn layout_chrome(&self, area: Rect) -> ChromeRegions {
        let mut regions = match self.preset() {
            UiPreset::Creeper => crate::tui::shell::layout_chrome_creeper_with_footer(
                area,
                self.preset_footer_rows(area.width),
            ),
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
    /// Every preset keeps a two-band footer: key hints on top, the status
    /// line underneath. `classic` and `nightlight` reserve exactly two rows —
    /// the row count must not depend on the message (a long failure text used
    /// to shave a row off every card) nor on the hint width (which once merged
    /// the two bands back into one whenever the page had few hints). `creeper`
    /// wraps its hint row instead of dropping hints, so it may ask for a third.
    ///
    /// Keyed off the page hints (not the active mode) so opening a dialog or
    /// entering search cannot resize the page under the user.
    fn preset_footer_rows(&self, width: u16) -> u16 {
        match self.preset() {
            UiPreset::Creeper => {
                let available = width.saturating_sub(2).max(1);
                let needed = crate::tui::app::view::hints_width(self, &self.page_hints());
                let hint_rows = needed.div_ceil(usize::from(available)).clamp(1, 2) as u16;
                hint_rows + 1
            }
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
    /// `classic` and `creeper` lay the page out in five sections; `shelf` puts
    /// the categories, a rule, the search and a second rule above the list. The
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

    /// Card height used by the `creeper` preset for list pages; narrow layouts
    /// drop the third detail line so two rows per card are enough.
    pub(in crate::tui::app) fn creeper_card_height(&self) -> u16 {
        if self.is_narrow() { 2 } else { 3 }
    }

    /// Shared geometry for a rendered collection: cards in `creeper`, a table
    /// or a plain list in `classic`.
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
        if self.preset() == UiPreset::Creeper {
            // Resource projects are a dense one-line index; their metadata lives
            // in the detail card. Versions and project files stay tables.
            if self.page == Page::DownloadCenter
                && self.download_center.category != DownloadCategory::GameVersions
                && self.resource_detail.is_none()
            {
                return RowLayout::list(area, item_count, selected);
            }
            let stride = match self.page {
                Page::Instances | Page::Accounts | Page::Tasks => self.creeper_card_height(),
                _ => 1,
            };
            return if stride > 1 {
                // Instance cards get one blank row between them; the shared
                // layout keeps that gap click-through (no accidental select).
                let gap = if self.page == Page::Instances { 1 } else { 0 };
                RowLayout::cards_with_gap(area, item_count, selected, stride, gap)
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
        // The shared `kit` primitives paint from the sheet in force; every
        // frame sets it before anything is drawn, so a preset switch or a mode
        // change cannot leave the previous sheet behind.
        crate::tui::widgets::kit::use_skin(match self.preset() {
            UiPreset::Creeper => crate::tui::app::view::creeper::tone::skin(self.creeper_palette()),
            _ => crate::tui::widgets::kit::Skin::DEFAULT,
        });
        match self.preset() {
            UiPreset::Classic => self.draw_classic(frame, area),
            UiPreset::Creeper => self.draw_creeper(frame),
            UiPreset::Nightlight => self.draw_nightlight(frame),
            UiPreset::Paper => self.draw_paper(frame, area),
            UiPreset::Shelf => self.draw_shelf(frame, area),
        }
        if self.config.state.ui.theme_mode == ThemeMode::Terminal {
            self.punch_through_terminal(frame);
        }
    }

    /// `跟随终端` means "let the terminal own the page colour", and a terminal
    /// colour is the only one that can carry transparency — acrylic, blur, an
    /// image behind the window. An explicit RGB is opaque by definition, which
    /// is why only `classic` used to show it: its `Theme` keeps `Color::Reset`
    /// and paints no background at all.
    ///
    /// The other four keep painting real fills — every blend and gradient needs
    /// RGB — and withdraw them in two places. Rows and selection bands go
    /// through [`Sheet::fill`], which returns `Reset` on a following sheet:
    /// their colours are derived (`mix(surface, page hue, 0.18)`), so no list of
    /// constants could ever name them. Everything painted as a literal
    /// `pal.surface` or `pal.bg` is handed back here, at the end of the frame —
    /// those are dozens of call sites across five presets, and rewriting each
    /// into `fill` would be noise for no gain.
    ///
    /// Borders, rules, text, pills, progress bars and key caps keep their own
    /// paints: they carry information rather than containment.
    fn punch_through_terminal(&self, frame: &mut Frame<'_>) {
        // An overlay dims every cell to a colour of its own and lays a solid
        // slab; there is nothing left to hand back, and punching the slab would
        // make the dialog see-through over a dimmed page.
        if self.detail_open || self.help_open || self.modal.is_some() {
            return;
        }
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

    /// The fills each preset lays under its content, lowest first.
    ///
    /// Each palette answers for itself through [`Sheet`]: a preset that keeps
    /// its selected-row band in a slot (`nightlight`'s `selection`) or derives
    /// its rows from the page hue (`creeper`) has to say so next to its
    /// colours, not here.
    fn terminal_fills(&self) -> [Option<Color>; 4] {
        match self.preset() {
            // Classic paints no background at all; its `Theme` keeps `Reset`.
            UiPreset::Classic => [None; 4],
            UiPreset::Creeper => self.creeper_palette().fills(),
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
        let Some(rect) = self
            .hitmap
            .hitboxes()
            .iter()
            .rev()
            .find(|hitbox| hitbox.action == hover)
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
                // `creeper` paints RGB surfaces, so hover shifts the cell
                // background. `classic` inherits the terminal background
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

    /// Full-screen blockers for the blocking overlays.
    ///
    /// Both presets call this from their own draw path, so the layer contract
    /// cannot drift between them. Each overlay registers the blocker first and
    /// its own body second: reverse lookup then closes on an outside click and
    /// does nothing inside — no coordinate arithmetic involved.
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
            // Every dialog prints its own key row, with clickable capsules. The
            // footer repeating the same keys was pure noise — the dialog is the
            // only thing on screen that accepts input.
            Mode::Form(_) | Mode::Filter | Mode::Confirm(_) => Vec::new(),
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
