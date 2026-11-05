//! `paper` instances: a printed list, with the selection in the margin.

use super::*;
use crate::tui::hit::RowKind;
use crate::tui::palette;
use crate::tui::widgets::rows::{RowLayout, RowWindow};

/// Two cells for the margin bar, one for air.
const MARGIN: u16 = 3;
/// Widest status the column will grow to before it is truncated: the longest
/// localized word ("Installing", "需要修复") still fits.
const STATUS_W: u16 = 14;
const PLAYED_W: u16 = 7;
const LOADER_W: u16 = 14;
const VERSION_W: u16 = 8;
const GAP: u16 = 2;

impl App {
    pub(in crate::tui::app) fn draw_paper_instances(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &PaperPalette,
    ) {
        if area.width < 20 || area.height < 3 {
            return;
        }
        let visible = self.visible_instance_indices();
        self.paper_instance_filters(frame, area, visible.len(), pal);

        // One blank row under the rule is what separates a page from a table.
        let list_y = area.y.saturating_add(3);
        let note_rows = if area.height >= 9 { 2 } else { 0 };
        let list_height = area.height.saturating_sub(3).saturating_sub(note_rows);
        if list_height == 0 {
            return;
        }
        if visible.is_empty() {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    self.i18n.text("paper.empty"),
                    ink(pal, pal.faint),
                ))),
                Rect::new(area.x, list_y, area.width, 1),
            );
            return;
        }

        let selected = visible
            .iter()
            .position(|index| *index == self.instances.focused)
            .unwrap_or(0);
        let list = Rect::new(area.x, list_y, area.width, list_height);
        let layout = self.row_windows.resolve(
            RowWindow::Instances,
            RowLayout::list_bare(list, visible.len(), selected),
        );
        self.hitmap.rows(layout, RowKind::Instances);

        // The status column is as wide as the longest word on screen, so a
        // localized "Installing" is not clipped into "Installi". The current
        // instance prints `Current` in place of its status, and that word has
        // to fit too.
        let current = self.i18n.text("common.current");
        let status_width = visible
            .iter()
            .filter_map(|index| self.instances.items.get(*index))
            .map(|item| {
                if self.instances.current_id == Some(item.id) {
                    palette::width(&current) as u16
                } else {
                    palette::width(&localize_instance_status(&item.status, &self.i18n)) as u16
                }
            })
            .max()
            .unwrap_or(6)
            .clamp(6, STATUS_W);

        for (visual, position) in (layout.offset()..visible.len())
            .take(layout.visible_rows())
            .enumerate()
        {
            let Some(item) = self.instances.items.get(visible[position]) else {
                continue;
            };
            let y = list.y.saturating_add(visual as u16);
            let row = Rect::new(list.x, y, list.width, 1);
            let active = position == selected;
            self.paper_instance_row(frame, row, item, active, status_width, pal);
        }

        if note_rows > 0 {
            let selected_item = self.instances.items.get(visible[selected]);
            let note = selected_item.map_or_else(String::new, |item| {
                format!("{} · {}", item.name, display_path(&item.path))
            });
            kit::rule(
                frame,
                Rect::new(
                    area.x,
                    area.y.saturating_add(area.height).saturating_sub(2),
                    area.width,
                    1,
                ),
                pal,
                pal.rule_soft,
            );
            kit::note(
                frame,
                Rect::new(
                    area.x,
                    area.y.saturating_add(area.height).saturating_sub(1),
                    area.width,
                    1,
                ),
                pal,
                &note,
            );
        }
    }

    /// The filter row reads as a line of running text, not as pills: the active
    /// one is the only underlined word on the page.
    fn paper_instance_filters(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        count: usize,
        pal: &PaperPalette,
    ) {
        let labels = self.instance_filter_labels();
        let mut x = area.x;
        for (index, label) in labels.iter().enumerate() {
            let width = palette::width(label) as u16;
            let rect = Rect::new(x, area.y, width, 1);
            if rect.right() > area.right() {
                break;
            }
            let style = if index == self.instance_filter {
                ink(pal, pal.accent).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                ink(pal, pal.faint)
            };
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(label.clone(), style))),
                rect,
            );
            self.hitmap
                .add(rect, Action::filter(Page::Instances, index));
            x = x.saturating_add(width).saturating_add(3);
        }
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("{} / {count}", self.position_in(area)),
                ink(pal, pal.faint),
            )))
            .alignment(Alignment::Right),
            Rect::new(area.x, area.y, area.width, 1),
        );
        kit::rule(
            frame,
            Rect::new(area.x, area.y.saturating_add(1), area.width, 1),
            pal,
            pal.rule,
        );
    }

    /// 1-based position of the selected row, for the right edge of the header.
    fn position_in(&self, _area: Rect) -> usize {
        let visible = self.visible_instance_indices();
        visible
            .iter()
            .position(|index| *index == self.instances.focused)
            .map_or(0, |position| position + 1)
    }

    #[allow(clippy::too_many_arguments)]
    fn paper_instance_row(
        &self,
        frame: &mut Frame<'_>,
        row: Rect,
        item: &crate::tui::pages::instances::InstanceSummary,
        active: bool,
        status_width: u16,
        pal: &PaperPalette,
    ) {
        if active {
            kit::selection(frame, row, pal);
        }
        let name_x = row.x.saturating_add(MARGIN);
        // Right-anchored columns: status, play time, loader, version.
        let cols = kit::columns(row, &[status_width, PLAYED_W, LOADER_W, VERSION_W], GAP);
        let (status, played, loader, version) = (cols[0], cols[1], cols[2], cols[3]);
        let name_width = version.x.saturating_sub(name_x + GAP);

        let name_style = if active {
            ink_marked(pal)
        } else {
            ink(pal, pal.soft)
        };
        // Narrow terminals print two columns only — name and status — even
        // when the four would technically fit: at 80 cells they leave the name
        // so little room that the row reads as a table, not a list.
        let (name_width, columns) = if !self.is_narrow() && name_width >= 12 {
            (name_width, true)
        } else {
            (status.x.saturating_sub(name_x + GAP), false)
        };
        if name_width > 0 {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    palette::truncate(&item.name, usize::from(name_width)),
                    name_style,
                ))),
                Rect::new(name_x, row.y, name_width, 1),
            );
        }
        if columns {
            let style = if active {
                ink(pal, pal.soft)
            } else {
                ink(pal, pal.faint)
            };
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    palette::fit(&item.version, usize::from(VERSION_W)),
                    style,
                ))),
                version,
            );
            let loader_text = item.loader_version.as_ref().map_or_else(
                || item.loader.clone(),
                |version| format!("{} {version}", item.loader),
            );
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    palette::truncate(&loader_text, usize::from(LOADER_W)),
                    style,
                ))),
                loader,
            );
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format_play_time(item.play_time_seconds),
                    style,
                )))
                .alignment(Alignment::Right),
                played,
            );
        }
        // The instance the launcher will start is marked in the status column,
        // the way classic and nightlight mark it: otherwise the row still reads
        // `Ready` after `Space`, and nothing on the page says it took.
        let is_current = self.instances.current_id == Some(item.id);
        let (text, color) = if is_current {
            (self.i18n.text("common.current"), pal.accent)
        } else {
            (
                localize_instance_status(&item.status, &self.i18n),
                self.paper_status_color(&item.status, pal),
            )
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(text, ink(pal, color))))
                .alignment(Alignment::Right),
            status,
        );
    }

    /// Status is written out; the colour only confirms what the word says.
    fn paper_status_color(&self, status: &str, pal: &PaperPalette) -> Color {
        match instance_status_kind(status) {
            StatusKind::Ready | StatusKind::Current | StatusKind::Completed => pal.ok,
            StatusKind::Warning | StatusKind::Queued | StatusKind::Running => pal.warn,
            StatusKind::Failed | StatusKind::Missing => pal.accent,
            StatusKind::Info => pal.dim,
        }
    }
}
