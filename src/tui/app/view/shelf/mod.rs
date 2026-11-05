//! `shelf`: the cover-led preset.
//!
//! One instance list on the left, a framed cover on the right, a lavender
//! accent on deep indigo. The preset never adds keys or modes; it only changes
//! what a frame looks like.

mod accounts;
mod downloads;
mod instances;
mod logs;
mod overlays;
mod overview;
mod settings;
mod tasks;
pub(crate) mod tone;
pub(in crate::tui::app) mod widgets;

pub(in crate::tui::app) use downloads::download_body;

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use tui_big_text::{BigTextBuilder, PixelSize};

use super::*;
use crate::tui::action::Action;
use crate::tui::app::state::nav::PAGES;
use crate::tui::hit::Layer;
use crate::tui::palette::{self, mix};
use crate::tui::shell::{LayoutMode, SHELF_WORDMARK_COLS, SHELF_WORDMARK_ROWS};
use crate::tui::theme::Theme;

impl App {
    pub(in crate::tui::app) fn shelf_palette(&self) -> &'static tone::Palette {
        tone::palette(
            self.config.state.ui.shelf_theme,
            self.config.state.ui.theme_mode,
        )
    }

    pub(in crate::tui::app) fn shelf_theme(&self) -> Theme {
        tone::theme(
            self.config.state.ui.shelf_theme,
            self.config.state.ui.theme_mode,
        )
    }

    pub(in crate::tui::app) fn draw_shelf(&self, frame: &mut Frame<'_>, area: Rect) {
        let pal = self.shelf_palette();
        frame.render_widget(Block::default().style(Style::default().bg(pal.bg)), area);
        let regions = self.layout_chrome(area);
        self.regions.set(regions);
        self.regions_size.set((area.width, area.height));
        if regions.mode == LayoutMode::TooSmall {
            self.shelf_too_small(frame, area, pal);
            // The page has no key row down here, so the one key that always
            // works has to be printed; `classic` keeps its `q` for the same
            // reason.
            let footer = regions.footer;
            if footer.width > 0 && footer.height > 0 {
                frame.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::styled("q", Style::default().fg(pal.accent).bg(pal.bg)),
                        Span::styled(
                            format!(" {}", self.i18n.text("common.quit")),
                            Style::default().fg(pal.dim).bg(pal.bg),
                        ),
                    ])),
                    Rect::new(footer.x, footer.y, footer.width, 1),
                );
            }
            return;
        }

        self.shelf_head(frame, &regions, pal);
        self.shelf_tabs(frame, &regions, pal);
        // The workspace replaces the instance list, not the whole screen: the
        // shell, the tabs and the footer stay, exactly as in `classic`.
        if self.page == Page::Instances && self.workspace_is_open() {
            self.draw_workspace_page(frame, regions.main, self.anim_time());
            self.shelf_footer(frame, &regions, pal);
            self.shelf_overlays(frame, area);
            return;
        }
        match self.page {
            Page::Overview => self.draw_shelf_overview(frame, regions.main, pal),
            Page::Instances => self.draw_shelf_instances(frame, regions.main, pal),
            Page::DownloadCenter => self.draw_shelf_downloads(frame, regions.main, pal),
            Page::Tasks => self.draw_shelf_tasks(frame, regions.main, pal),
            Page::Accounts => self.draw_shelf_accounts(frame, regions.main, pal),
            Page::Logs => self.draw_shelf_logs(frame, regions.main, pal),
            Page::Settings => self.draw_shelf_settings(frame, regions.main, pal),
        }
        self.shelf_footer(frame, &regions, pal);
        self.shelf_overlays(frame, area);
    }

    /// Detail, help and dialogs, all painted with shelf's palette.
    ///
    /// The overlays use the shared geometry from `view/overlays.rs`; the dialog
    /// is `modal/shelf.rs`, which owns its own slab.
    fn shelf_overlays(&self, frame: &mut Frame<'_>, area: Rect) {
        if self.detail_open || self.help_open || self.modal.is_some() {
            crate::tui::widgets::scrim::dim(frame);
        }
        self.register_overlay_hits(area);
        if self.detail_open {
            self.draw_shelf_detail(frame);
        }
        if self.help_open {
            self.draw_shelf_help(frame);
        }
        if let Some(modal) = &self.modal {
            self.hitmap.set_layer(Layer::Modal);
            modal.draw_shelf(
                frame,
                &self.i18n,
                self.shelf_palette(),
                self.anim_time(),
                &self.hitmap,
            );
            self.hitmap.set_layer(Layer::Base);
        }
        self.paint_hover(frame);
    }

    fn shelf_too_small(&self, frame: &mut Frame<'_>, area: Rect, pal: &tone::Palette) {
        let row = Rect::new(
            area.x,
            area.y.saturating_add(area.height / 2).saturating_sub(1),
            area.width,
            3.min(area.height),
        );
        frame.render_widget(
            Paragraph::new(self.i18n.text("shelf.too_small"))
                .alignment(Alignment::Center)
                .style(Style::default().fg(pal.dim).bg(pal.bg)),
            row,
        );
    }

    /// Running head: a four-row wordmark on the left, the version and the
    /// counts in the right-hand context block.
    fn shelf_head(&self, frame: &mut Frame<'_>, regions: &ChromeRegions, pal: &tone::Palette) {
        let header = regions.header;
        if header.height == 0 {
            return;
        }
        if header.height >= SHELF_WORDMARK_ROWS {
            shelf_wordmark(
                frame,
                Rect::new(
                    header.x,
                    header.y,
                    SHELF_WORDMARK_COLS.min(header.width),
                    SHELF_WORDMARK_ROWS,
                ),
                pal,
            );
        } else {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        "TMCL",
                        Style::default()
                            .fg(pal.accent_light)
                            .bg(pal.bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("  {}", env!("CARGO_PKG_VERSION")),
                        Style::default().fg(pal.faint).bg(pal.bg),
                    ),
                ]))
                .style(Style::default().bg(pal.bg)),
                Rect::new(header.x, header.y, header.width, 1),
            );
        }

        if header.height < 2 || regions.context.width < 20 {
            return;
        }
        let instances = self.i18n.format(
            "settings.directory_instances",
            &[("count", &self.instances.items.len().to_string())],
        );
        // Two keys, not one sentence: the first half carries the accent and the
        // second is the quiet tail, and a language that reorders them would
        // break a split on ` · `.
        let tagline = self.i18n.text("shelf.tagline");
        let note = self.i18n.text("shelf.tagline_note");
        let tagline_text = format!("{tagline} · {note}");
        let tagline_width = u16::try_from(palette::width(&tagline_text)).unwrap_or(u16::MAX);
        let count_width = u16::try_from(palette::width(&instances)).unwrap_or(u16::MAX);
        // The tagline is right-aligned, so its left edge is a computed column,
        // not the block's: the version lines up under the `M` of `Minecraft`
        // rather than under the count.
        let version_x = regions.context.right().saturating_sub(tagline_width);
        let count_x = regions.context.right().saturating_sub(count_width);
        let row = regions.context.y.saturating_add(2);

        frame.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled(tagline, Style::default().fg(pal.accent).bg(pal.bg)),
                    Span::styled(
                        format!(" · {note}"),
                        Style::default().fg(pal.dim).bg(pal.bg),
                    ),
                ]),
                Line::from(Span::styled(
                    instances,
                    Style::default().fg(pal.dim).bg(pal.bg),
                )),
            ])
            .alignment(Alignment::Right)
            .style(Style::default().bg(pal.bg)),
            Rect::new(
                regions.context.x,
                regions.context.y.saturating_add(1),
                regions.context.width,
                2.min(regions.context.height.saturating_sub(1)),
            ),
        );

        // Same row as the count, at the tagline's left edge; skipped when the
        // two would touch.
        let version = env!("CARGO_PKG_VERSION");
        let version_width = u16::try_from(palette::width(version)).unwrap_or(u16::MAX);
        if regions.context.height >= 3
            && version_x >= regions.context.x
            && version_x.saturating_add(version_width).saturating_add(2) <= count_x
        {
            frame.render_widget(
                Paragraph::new(Span::styled(
                    version,
                    Style::default().fg(pal.faint).bg(pal.bg),
                )),
                Rect::new(
                    version_x,
                    row,
                    count_x.saturating_sub(version_x).saturating_sub(1),
                    1,
                ),
            );
        }
    }

    fn shelf_tabs(&self, frame: &mut Frame<'_>, regions: &ChromeRegions, pal: &tone::Palette) {
        let tabs = regions.tabs;
        if tabs.height == 0 {
            return;
        }
        let labels = self.tab_labels(regions.mode);
        let rects = widgets::tab_rects(tabs, &labels);
        let active = PAGES.iter().position(|page| *page == self.page);
        for (index, rect) in rects.iter().enumerate() {
            let Some(page) = PAGES.get(index) else {
                break;
            };
            let label = labels.get(index).map_or("", String::as_str);
            let style = if Some(index) == active {
                Style::default()
                    .fg(pal.accent_light)
                    .bg(pal.bg)
                    .add_modifier(Modifier::UNDERLINED | Modifier::BOLD)
            } else {
                Style::default().fg(pal.dim).bg(pal.bg)
            };
            frame.render_widget(
                Paragraph::new(Span::styled(label.to_owned(), style))
                    .style(Style::default().bg(pal.bg)),
                *rect,
            );
            self.hitmap.add(*rect, Action::Nav(*page));
        }

        // The hairline sits one row below the tabs. It fades from the accent
        // into the page so it reads as a rule rather than a second tab bar.
        let rule = Rect::new(tabs.x, tabs.y.saturating_add(1), tabs.width, 1);
        if rule.bottom() <= regions.body.y {
            let width = usize::from(rule.width);
            let mut spans = Vec::with_capacity(width);
            for column in 0..width {
                let t = if width <= 1 {
                    0.5
                } else {
                    0.85 - 0.7 * (column as f64 / (width - 1) as f64)
                };
                spans.push(Span::styled(
                    "─",
                    Style::default().fg(mix(pal.accent, pal.bg, t)).bg(pal.bg),
                ));
            }
            frame.render_widget(
                Paragraph::new(Line::from(spans)).style(Style::default().bg(pal.bg)),
                rule,
            );
        }
    }

    fn shelf_footer(&self, frame: &mut Frame<'_>, regions: &ChromeRegions, pal: &tone::Palette) {
        let footer = regions.footer;
        if footer.height == 0 {
            return;
        }
        if footer.height >= 2 {
            frame.render_widget(
                Paragraph::new(self.shelf_hints(footer.width, pal))
                    .style(Style::default().bg(pal.bg)),
                Rect::new(footer.x, footer.y, footer.width, 1),
            );
        }
        let status_row = if footer.height >= 2 {
            footer.y.saturating_add(1)
        } else {
            footer.y
        };
        let color = match self.status.kind() {
            crate::tui::widgets::status::StatusKind::Warning => pal.warn,
            crate::tui::widgets::status::StatusKind::Failed => pal.error,
            crate::tui::widgets::status::StatusKind::Completed => pal.ok,
            _ => pal.faint,
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("● ", Style::default().fg(color).bg(pal.bg)),
                Span::styled(
                    self.status.as_str().to_owned(),
                    Style::default().fg(pal.dim).bg(pal.bg),
                ),
            ]))
            .style(Style::default().bg(pal.bg)),
            Rect::new(footer.x, status_row, footer.width, 1),
        );
    }

    /// Key hints without the classic brackets, dropped by the same priority
    /// rule as `chrome::render_footer`: the highest priority number goes first,
    /// and the last entry (`q`) is never removed.
    fn shelf_hints(&self, width: u16, pal: &tone::Palette) -> Line<'static> {
        let mut hints = self.hints();
        let cost = |hint: &crate::tui::chrome::Hint| {
            crate::tui::palette::width(&hint.key)
                + 1
                + crate::tui::palette::width(&self.i18n.text_or_raw(&hint.action))
        };
        let mut total: usize = hints
            .iter()
            .map(|hint| cost(hint) + 3)
            .sum::<usize>()
            .saturating_sub(3);
        while total > usize::from(width) && hints.len() > 1 {
            let Some(index) =
                (0..hints.len() - 1).max_by_key(|index| (hints[*index].priority, *index))
            else {
                break;
            };
            total = total.saturating_sub(cost(&hints[index]) + 3);
            hints.remove(index);
        }
        let mut spans = Vec::new();
        for hint in hints {
            if !spans.is_empty() {
                spans.push(Span::styled("   ", Style::default().bg(pal.bg)));
            }
            let label = self.i18n.text_or_raw(&hint.action);
            spans.push(Span::styled(
                hint.key,
                Style::default().fg(pal.accent).bg(pal.bg),
            ));
            spans.push(Span::styled(
                format!(" {label}"),
                Style::default().fg(pal.dim).bg(pal.bg),
            ));
        }
        Line::from(spans)
    }
}

/// The block wordmark, ramped instead of filled flat.
///
/// `tui-big-text` paints one colour for the whole widget, so the ramp is
/// applied to the cells it just wrote: a diagonal from a near-white lavender at
/// the top-left corner to the accent at the bottom-right. The wordmark is the
/// one place the preset raises its voice, and a flat lavender fill reads as a
/// second heading rather than as a mark.
fn shelf_wordmark(frame: &mut Frame<'_>, rect: Rect, pal: &tone::Palette) {
    let banner = BigTextBuilder::default()
        .pixel_size(PixelSize::Quadrant)
        .style(Style::default().fg(pal.accent_light).bg(pal.bg))
        .lines([Line::from("TMCL")])
        .build();
    frame.render_widget(banner, rect);

    let (top, bottom) = tone::wordmark_ramp(pal);
    let rows = f64::from(rect.height.saturating_sub(1)).max(1.0);
    let columns = f64::from(rect.width.saturating_sub(1)).max(1.0);
    let buffer = frame.buffer_mut();
    for y in rect.top()..rect.bottom() {
        let down = f64::from(y.saturating_sub(rect.top())) / rows;
        for x in rect.left()..rect.right() {
            let across = f64::from(x.saturating_sub(rect.left())) / columns;
            let Some(cell) = buffer.cell_mut((x, y)) else {
                continue;
            };
            // The holes inside the glyphs keep the page colour; only the strokes
            // carry the ramp.
            if cell.symbol() == " " {
                continue;
            }
            // Weighted to the vertical so the mark reads as lit from above even
            // though the ramp is diagonal.
            cell.set_fg(mix(top, bottom, 0.66 * down + 0.34 * across));
        }
    }
}
