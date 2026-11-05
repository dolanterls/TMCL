//! `creeper` preset: frame composition here plus `shell.rs`, one file per page.
//!
//! Only what this preset alone uses lives here. Palette and card/pill
//! primitives are shared with the other two presets and live in
//! `crate::tui::palette` and `crate::tui::widgets::kit`.

mod accounts;
mod downloads;
mod instances;
mod logs;
mod modals;
mod overview;
mod settings;
mod shell;

/// Footer width helper, re-exported by `view` for the layout's row count.
pub(in crate::tui::app) use shell::hints_width;
pub(in crate::tui::app) mod sidebar;
mod tasks;
pub(crate) mod tone;

use ratatui::style::Style;
use ratatui::widgets::Block;

use super::*;
#[cfg(test)]
pub(in crate::tui::app) use overview::overview_layout;

impl App {
    /// The sheet this preset paints with, chosen by `主题`.
    pub(in crate::tui::app) fn creeper_palette(&self) -> &'static tone::Palette {
        tone::palette(self.config.state.ui.theme_mode)
    }

    pub(in crate::tui::app) fn draw_creeper(&self, frame: &mut Frame<'_>) {
        let pal = self.creeper_palette();
        let area = frame.area();
        frame.render_widget(Block::default().style(Style::default().bg(pal.bg)), area);
        let regions = self.layout_chrome(area);
        self.regions.set(regions);
        self.regions_size.set((area.width, area.height));
        if regions.mode == LayoutMode::TooSmall {
            self.draw_creeper_too_small(frame, &regions);
            return;
        }

        let t = self.anim_time();
        shell::header(self, frame, &regions, t);
        shell::rule(self, frame, &regions, t);
        shell::sidebar(self, frame, &regions, t);
        shell::gutter(frame, &regions, pal);
        self.draw_creeper_page(frame, &regions, t);
        shell::footer(self, frame, &regions, t);
        shell::toast(self, frame, t);

        // Dim the page once, below every overlay, so stacked dialogs still
        // read as one layer above a single scrim.
        if self.detail_open || self.help_open || self.modal.is_some() {
            crate::tui::widgets::scrim::dim(frame);
        }
        // Same blocker contract as the classic preset; shared helper so the two
        // draw paths cannot drift apart.
        self.register_overlay_hits(area);
        if self.detail_open {
            modals::detail(self, frame, t);
        }
        if self.help_open {
            modals::help(self, frame, t);
        }
        if let Some(modal) = &self.modal {
            self.hitmap.set_layer(crate::tui::hit::Layer::Modal);
            modals::render(self, frame, modal, t, &self.hitmap);
            self.hitmap.set_layer(crate::tui::hit::Layer::Base);
        }
        self.paint_hover(frame);
    }

    pub(in crate::tui::app) fn creeper_detail_pane(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        t: f64,
    ) {
        modals::detail_pane(self, frame, area, t);
    }

    pub(in crate::tui::app) fn draw_creeper_page(
        &self,
        frame: &mut Frame<'_>,
        regions: &ChromeRegions,
        t: f64,
    ) {
        let pal = self.creeper_palette();
        // Every page is a stack of `pal.surface` cards. Painting the page column
        // first keeps the rows and columns between those cards on the card
        // surface; left on the page background they read as black seams.
        frame.render_widget(
            Block::default().style(Style::default().bg(pal.surface)),
            regions.main,
        );
        match self.page {
            Page::Overview => self.draw_creeper_overview(frame, regions.main, t),
            Page::Instances => {
                if self.workspace_is_open() {
                    self.draw_workspace_page(frame, regions.main, t);
                } else {
                    self.draw_creeper_instances(frame, regions.main, t);
                }
            }
            Page::DownloadCenter => self.draw_creeper_downloads(frame, regions.main, t),
            Page::Tasks => self.draw_creeper_tasks(frame, regions.main, t),
            Page::Accounts => self.draw_creeper_accounts(frame, regions.main, t),
            Page::Logs => self.draw_creeper_logs(frame, regions.main, t),
            Page::Settings => self.draw_creeper_settings(frame, regions.main, t),
        }
    }

    fn draw_creeper_too_small(&self, frame: &mut Frame<'_>, regions: &ChromeRegions) {
        let pal = self.creeper_palette();
        let area = regions.body;
        let width = area.width.clamp(20, 58).min(area.width);
        let height = area.height.clamp(3, 5).min(area.height);
        let panel = Rect::new(
            area.x + area.width.saturating_sub(width) / 2,
            area.y + area.height.saturating_sub(height) / 2,
            width,
            height,
        );
        frame.render_widget(
            ratatui::widgets::Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    self.i18n.text("layout.too_small"),
                    Style::default().fg(pal.text_dim).bg(pal.surface),
                )),
            ])
            .alignment(ratatui::layout::Alignment::Center)
            .block(crate::tui::widgets::kit::card(
                "TMCL",
                pal.lime,
                true,
                0.0,
                pal.surface,
            ))
            .style(Style::default().bg(pal.surface)),
            panel,
        );
        shell::footer(self, frame, regions, self.anim_time());
    }
}
