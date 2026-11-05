//! `classic` frame: the framed shell, its per-page dispatch and the overlays.
//!
//! Everything here is `classic`-only. What is shared — layout geometry, hover
//! tinting, overlay blockers and the key-hint rows — stays in
//! `app/view/dispatch.rs`, which also owns the one `match` that picks a preset.

use super::*;
use crate::tui::hit::Layer;

impl App {
    pub(in crate::tui::app) fn draw_classic(&self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(Block::default().style(self.theme.base()), area);
        // Classic always lays out with its own geometry, never with the active
        // preset's: `paper` borrows this frame for the pages it has not drawn
        // yet, and those pages must keep classic's header, tabs and footer.
        let regions = crate::tui::shell::layout_chrome_with_footer(area, 2);
        self.regions.set(regions);
        self.regions_size.set((area.width, area.height));
        if regions.mode == LayoutMode::TooSmall {
            self.draw_too_small(frame, &regions);
            return;
        }

        let labels = self.tab_labels(regions.mode);
        let active = tab_index(self.page);
        let context = self.header_context();
        let tab_rects = crate::tui::chrome::render_header(
            frame, &regions, &labels, active, &context, self.theme,
        );
        // The header already returns the rectangles it drew, so registration
        // uses those exact values instead of re-deriving them.
        for (index, rect) in tab_rects.iter().enumerate() {
            if let Some(page) = PAGES.get(index) {
                self.hitmap.add(*rect, Action::Nav(*page));
            }
        }
        crate::tui::chrome::render_body(frame, &regions, self.theme);

        self.draw_page(frame, &regions);

        crate::tui::chrome::render_footer(
            frame,
            regions.footer,
            &self.hints(),
            Some((self.status.as_str(), self.status.kind())),
            &self.i18n,
            self.theme,
        );

        if self.detail_open || self.help_open || self.modal.is_some() {
            crate::tui::widgets::scrim::dim(frame);
        }

        self.register_overlay_hits(area);

        if self.detail_open {
            self.draw_detail_popup(frame);
        }

        if self.help_open {
            self.draw_help_overlay(frame);
        }

        if let Some(modal) = &self.modal {
            self.hitmap.set_layer(Layer::Modal);
            modal.draw(
                frame,
                self.theme,
                &self.i18n,
                self.anim_time(),
                &self.hitmap,
            );
            self.hitmap.set_layer(Layer::Base);
        }
        self.paint_hover(frame);
    }

    fn draw_page(&self, frame: &mut Frame<'_>, regions: &ChromeRegions) {
        match self.page {
            Page::Overview => self.draw_overview_page(frame, regions.main),
            Page::Instances => {
                if self.workspace_is_open() {
                    self.draw_workspace_page(frame, regions.main, 0.0);
                } else {
                    self.draw_instances_page(frame, regions.main);
                }
            }
            Page::Tasks => self.draw_tasks_page(frame, regions.main),
            Page::Accounts => self.draw_accounts_page(frame, regions.main),
            Page::Settings => self.draw_settings_page(frame, regions.main),
            Page::DownloadCenter => self.draw_version_page(frame, regions.main),
            Page::Logs => self.draw_logs_page(frame, regions.main),
        }
    }

    fn draw_too_small(&self, frame: &mut Frame<'_>, regions: &ChromeRegions) {
        let area = regions.body;
        let width = area.width.min(60);
        let height = area.height.min(6);
        let panel = Rect::new(
            area.x + area.width.saturating_sub(width) / 2,
            area.y + area.height.saturating_sub(height) / 2,
            width,
            height,
        );
        frame.render_widget(
            Paragraph::new(self.i18n.text("layout.too_small"))
                .block(crate::tui::chrome::panel("tmcl", true, self.theme))
                .style(self.theme.base())
                .wrap(Wrap { trim: false }),
            panel,
        );
        crate::tui::chrome::render_footer(
            frame,
            regions.footer,
            &[crate::tui::chrome::Hint::new(
                "q",
                self.i18n.text("common.quit"),
            )],
            None,
            &self.i18n,
            self.theme,
        );
    }

    fn header_context(&self) -> Vec<Span<'static>> {
        let instance = self
            .instances
            .current_id
            .and_then(|id| self.instances.items.iter().find(|item| item.id == id))
            .map_or_else(
                || self.i18n.text("header.no_instance"),
                |item| item.name.clone(),
            );
        let account = self.current_account().map_or_else(
            || self.i18n.text("header.no_account"),
            |account| account.username,
        );
        vec![
            Span::styled(
                truncate_text(&instance, 16),
                Style::default().add_modifier(Modifier::ITALIC),
            ),
            Span::styled(" · ", self.theme.dim()),
            Span::styled(
                truncate_text(&account, 12),
                Style::default().add_modifier(Modifier::ITALIC),
            ),
            Span::raw(" "),
        ]
    }

    /// Shared by every preset that has no help screen of its own — the overlay
    /// is chrome, not a page, and `paper` borrows it until it draws one.
    fn draw_help_overlay(&self, frame: &mut Frame<'_>) {
        self.draw_help_overlay_with(frame, self.theme);
    }
}
