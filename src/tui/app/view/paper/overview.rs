//! `paper` overview: a title block, the current setup, and three figures.
//!
//! The layout follows the printed reference: a wordmark set in block type, the
//! facts under it, a rule, the current instance beside its key legend, and three
//! figures — number above label, separated by hairlines — closing the block.
//!
//! A terminal cannot change font size, so the wordmark is set as block glyphs
//! four rows tall — the same `tui-big-text` face the nightlight preset uses;
//! everything else earns its rank from weight, spacing and the rules around it.

use super::*;
use crate::tui::palette;
use tui_big_text::{BigTextBuilder, PixelSize};

/// Where the right-hand column starts, as a share of the page width.
const RIGHT_COLUMN: u32 = 62;

/// Rows the wordmark needs, and the height below which it is set on one row.
const BANNER_ROWS: u16 = 4;
/// Columns the wordmark needs: four glyphs of four cells each.
const BANNER_COLS: u16 = 16;

impl App {
    pub(in crate::tui::app) fn draw_paper_overview(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &PaperPalette,
    ) {
        if area.width < 24 || area.height < 6 {
            return;
        }
        let wordmark = area.height >= 14;
        // The path is the first thing to go when the page narrows: the name and
        // the runtime already say which instance this is.
        let with_path = area.height >= 12 && !self.is_narrow();
        let mut y = area.y;

        if wordmark {
            self.paper_wordmark(frame, area, y, pal);
            y = y.saturating_add(BANNER_ROWS);
        } else {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("TMCL", ink(pal, pal.ink).add_modifier(Modifier::BOLD)),
                    Span::styled(
                        format!("  {}", env!("CARGO_PKG_VERSION")),
                        ink(pal, pal.faint),
                    ),
                ])),
                Rect::new(area.x, y, area.width, 1),
            );
            y = y.saturating_add(1);
        }

        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                self.paper_facts(),
                ink(pal, pal.faint),
            ))),
            Rect::new(area.x, y, area.width, 1),
        );
        y = y.saturating_add(2);
        kit::rule(frame, Rect::new(area.x, y, area.width, 1), pal, pal.rule);
        y = y.saturating_add(2);

        let left_width = area.width * RIGHT_COLUMN as u16 / 100;
        let left = Rect::new(
            area.x,
            y,
            left_width,
            area.height.saturating_sub(y - area.y),
        );
        let right = Rect::new(
            area.x.saturating_add(left_width).saturating_add(2),
            y,
            area.width.saturating_sub(left_width).saturating_sub(2),
            left.height,
        );
        self.paper_current_instance(frame, left, with_path, pal);
        self.paper_launch_block(frame, right, pal);

        // Figures close the block. They used to share one inline line; the
        // reference sets each number over its own label with a rule between, and
        // that is what gives a number weight when the font cannot grow.
        //
        // On a tall page they sit on the bottom edge, the way the reference card
        // does; on a short one they follow the text so they cannot collide with
        // it.
        y = y.saturating_add(if with_path { 4 } else { 3 });
        let stats = if area.height >= 22 {
            area.y.saturating_add(area.height).saturating_sub(4)
        } else {
            y.saturating_add(1)
        };
        if stats.saturating_add(4) > area.y.saturating_add(area.height) {
            return;
        }
        kit::rule(
            frame,
            Rect::new(area.x, stats, area.width, 1),
            pal,
            pal.rule_soft,
        );
        self.paper_stat_columns(
            frame,
            Rect::new(area.x, stats.saturating_add(2), area.width, 2),
            pal,
        );
    }

    /// The wordmark, four rows of block glyphs with the version on its
    /// baseline: the same `tui-big-text` face the nightlight sidebar sets.
    fn paper_wordmark(&self, frame: &mut Frame<'_>, area: Rect, y: u16, pal: &PaperPalette) {
        let banner = BigTextBuilder::default()
            .pixel_size(PixelSize::Quadrant)
            .style(Style::default().fg(pal.ink).bg(pal.bg))
            .lines([Line::from("TMCL")])
            .build();
        frame.render_widget(
            banner,
            Rect::new(area.x, y, BANNER_COLS.min(area.width), BANNER_ROWS),
        );
        let version_x = area.x.saturating_add(BANNER_COLS).saturating_add(3);
        if version_x < area.right() {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    env!("CARGO_PKG_VERSION"),
                    ink(pal, pal.faint),
                ))),
                Rect::new(
                    version_x,
                    y.saturating_add(BANNER_ROWS).saturating_sub(1),
                    area.right().saturating_sub(version_x),
                    1,
                ),
            );
        }
    }

    /// `19 个实例 · Java 21.0.5 · 2026-11-02 13:37` — the masthead line: how much
    /// is here, which runtime it uses, and when the page was set.
    ///
    /// A missing runtime is left out rather than printed as "none": on a page
    /// this sparse, a hole reads worse than a short line.
    fn paper_facts(&self) -> String {
        let mut parts = vec![self.i18n.format(
            "settings.directory_instances",
            &[("count", &self.instances.items.len().to_string())],
        )];
        if let Some(java) = self
            .config
            .java
            .selected_path
            .as_ref()
            .and_then(|path| self.java.iter().find(|java| &java.executable == path))
        {
            parts.push(format!("Java {}", palette::truncate(&java.version, 16)));
        }
        // The masthead date, the way a newspaper sets it. The game directory
        // used to sit here; a path belongs in the instance block, not in the
        // line that says what this page is.
        if let Some(now) = crate::platform::windows::local_now() {
            parts.push(now.stamp());
        }
        parts.join(" · ")
    }

    /// The current instance, set like a definition list: label above, value in
    /// bold with the underline that marks it as the subject of the page.
    fn paper_current_instance(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        with_path: bool,
        pal: &PaperPalette,
    ) {
        if area.width < 12 || area.height < 2 {
            return;
        }
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                self.i18n.text("paper.current"),
                ink(pal, pal.faint),
            ))),
            Rect::new(area.x, area.y, area.width, 1),
        );

        let (name, detail, path) = match self.current_instance() {
            Some(item) => {
                let loader = item.loader_version.as_ref().map_or_else(
                    || item.loader.clone(),
                    |version| format!("{} {version}", item.loader),
                );
                (
                    item.name.clone(),
                    format!("{} · {loader}", item.version),
                    display_path(&item.path),
                )
            }
            None => (self.i18n.text("paper.empty"), String::new(), String::new()),
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                palette::truncate(&name, usize::from(area.width)),
                ink_marked(pal),
            ))),
            Rect::new(area.x, area.y.saturating_add(1), area.width, 1),
        );
        if area.height >= 3 && !detail.is_empty() {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    palette::truncate(&detail, usize::from(area.width)),
                    ink(pal, pal.soft),
                ))),
                Rect::new(area.x, area.y.saturating_add(2), area.width, 1),
            );
        }
        if with_path && area.height >= 4 && !path.is_empty() {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    palette::truncate(&path, usize::from(area.width)),
                    ink(pal, pal.faint),
                ))),
                Rect::new(area.x, area.y.saturating_add(3), area.width, 1),
            );
        }
    }

    /// `L` and `enter`, printed as a keyboard legend — the key is the heading,
    /// so the block needs no title of its own.
    fn paper_launch_block(&self, frame: &mut Frame<'_>, area: Rect, pal: &PaperPalette) {
        if area.width < 10 || area.height < 3 {
            return;
        }
        let legend = |key: &str, label: String| {
            Line::from(vec![
                Span::styled(
                    key.to_owned(),
                    ink(pal, pal.accent).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("  {label}"), ink(pal, pal.ink)),
            ])
        };
        // The second key is a footnote to the first, not its equal.
        let quiet = |key: &str, label: &str| {
            Line::from(vec![
                Span::styled(key.to_owned(), ink(pal, pal.faint)),
                Span::styled(format!("  {label}"), ink(pal, pal.dim)),
            ])
        };
        // One row down, so the legend lines up with the instance name and its
        // detail line instead of with the label above them.
        //
        // These are the keys the page actually binds: Enter launches the
        // current instance and `n` creates one. The block used to advertise
        // `L` and "enter opens the details", which belong to the Instances
        // page — pressing Enter here launched a game while the legend said it
        // would open something.
        frame.render_widget(
            Paragraph::new(legend("Enter", self.i18n.text("action.launch"))),
            Rect::new(area.x, area.y.saturating_add(1), area.width, 1),
        );
        frame.render_widget(
            Paragraph::new(quiet("n", &self.i18n.text("action.new"))),
            Rect::new(area.x, area.y.saturating_add(2), area.width, 1),
        );
    }

    /// Three figures side by side: number over label, hairline in each gutter.
    fn paper_stat_columns(&self, frame: &mut Frame<'_>, area: Rect, pal: &PaperPalette) {
        let running = self
            .task_snapshots
            .iter()
            .filter(|task| {
                matches!(
                    task.state,
                    crate::core::task::TaskState::Running | crate::core::task::TaskState::Paused
                )
            })
            .count();
        let played = self
            .instances
            .items
            .iter()
            .map(|item| item.play_time_seconds)
            .sum::<u64>()
            / 3600;
        let figures = [
            (
                self.instances.items.len().to_string(),
                self.i18n.text("paper.stat.instances"),
            ),
            (running.to_string(), self.i18n.text("paper.stat.tasks")),
            (played.to_string(), self.i18n.text("paper.stat.played")),
        ];
        if area.width < 48 {
            return;
        }
        let third = area.width / 3;
        let mut columns = kit::columns(area, &[third, third, third], 2);
        columns.reverse();
        for (index, (value, label)) in figures.iter().enumerate() {
            let Some(rect) = columns.get(index).copied() else {
                continue;
            };
            if rect.width == 0 {
                continue;
            }
            // The rule sits in the gutter, one cell before the column, and runs
            // the height of the number plus its label.
            if index > 0 {
                for offset in 0..2u16 {
                    frame.render_widget(
                        Paragraph::new(Line::from(Span::styled("│", ink(pal, pal.rule)))),
                        Rect::new(
                            rect.x.saturating_sub(1),
                            area.y.saturating_add(offset),
                            1,
                            1,
                        ),
                    );
                }
            }
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    palette::truncate(value, usize::from(rect.width)),
                    ink(pal, pal.ink).add_modifier(Modifier::BOLD),
                ))),
                Rect::new(rect.x, area.y, rect.width, 1),
            );
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    palette::truncate(label, usize::from(rect.width)),
                    ink(pal, pal.faint),
                ))),
                Rect::new(rect.x, area.y.saturating_add(1), rect.width, 1),
            );
        }
    }
}
