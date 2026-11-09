//! The diagnosis card: one ranked verdict, its evidence, and what to do.
//!
//! Shared by all five presets. Each one hands in a `Theme` built from its own
//! palette, so the card belongs to the page it covers without any preset
//! needing its own painter.
//!
//! Geometry follows the same rule as the help card: the rectangle is computed
//! once by [`App::diagnosis_layout`] and reused by the painter, the scroll
//! maximum and the hit map. Deriving it twice is how a panel ends up
//! unscrollable or closes when clicked from the inside.

use crate::core::config::models::UiPreset;
use crate::core::diagnose::{Diagnosis, ExitKind};
use crate::tui::app::App;
use crate::tui::theme::Theme;
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

/// Card width. `chrome::popup_rect_measured` clamps it to the screen.
const WIDTH: u16 = 72;
/// Height bounds: below this the evidence section is one line and a scrollbar;
/// above it the card covers the page it is explaining.
const MIN_ROWS: u16 = 14;
const MAX_ROWS: u16 = 26;
/// Rows the card spends on borders and the key row.
const CHROME_ROWS: u16 = 3;

pub(in crate::tui::app) struct DiagnosisLayout {
    pub rect: Rect,
    pub body: Rect,
    pub text: Rect,
    pub scroll: usize,
    pub max_scroll: usize,
}

impl App {
    /// The card's theme, borrowed from the preset on screen.
    ///
    /// A shared painter cannot use `self.theme` for every preset: on `paper`
    /// that is the terminal's own ink on a sheet that is nearly white.
    pub(in crate::tui::app) fn diagnosis_theme(&self) -> Theme {
        match self.preset() {
            UiPreset::Classic => self.theme,
            UiPreset::Nightlight => {
                let pal = self.nightlight_palette();
                Theme {
                    bg: pal.bg,
                    fg: pal.text,
                    dim: pal.muted,
                    accent: pal.accent,
                    label: pal.accent2,
                    key: pal.accent,
                    selection: pal.selection,
                    success: pal.ok,
                    error: pal.err,
                    ..Theme::default()
                }
            }
            UiPreset::Paper => {
                let pal = self.paper_palette();
                Theme {
                    bg: pal.sheet,
                    fg: pal.ink,
                    dim: pal.soft,
                    accent: pal.accent,
                    label: pal.soft,
                    key: pal.accent,
                    success: pal.ok,
                    ..Theme::default()
                }
            }
            UiPreset::Shelf => {
                let pal = self.shelf_palette();
                Theme {
                    bg: pal.surface,
                    fg: pal.text,
                    dim: pal.dim,
                    accent: pal.accent,
                    key: pal.accent_light,
                    success: pal.ok,
                    error: pal.error,
                    ..Theme::default()
                }
            }
        }
    }

    pub(in crate::tui::app) fn diagnosis_rect(&self, area: Rect) -> Rect {
        let rows = self
            .diagnosis
            .as_ref()
            .map_or(MIN_ROWS, diagnosis_rows)
            .clamp(MIN_ROWS, MAX_ROWS);
        crate::tui::chrome::popup_rect_measured(area, WIDTH, rows + CHROME_ROWS)
    }

    /// Rectangle, viewport and scroll window, computed once per frame.
    pub(in crate::tui::app) fn diagnosis_layout(&self, area: Rect) -> DiagnosisLayout {
        let rect = self.diagnosis_rect(area);
        let body = rect.inner(ratatui::layout::Margin::new(1, 1));
        let text = Rect {
            height: body.height.saturating_sub(1),
            ..body
        };
        let total = if text.is_empty() {
            0
        } else {
            let lines = self.diagnosis_lines(text.width);
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .line_count(text.width)
        };
        let max_scroll = total.saturating_sub(usize::from(text.height));
        self.hitmap.scroll_on(
            crate::tui::hit::Layer::Diagnosis,
            rect,
            crate::tui::action::ScrollTarget::DiagnosisBody,
        );
        DiagnosisLayout {
            rect,
            body,
            text,
            scroll: self.diagnosis_scroll.min(max_scroll),
            max_scroll,
        }
    }

    pub(in crate::tui::app) fn diagnosis_max_scroll(&self) -> usize {
        let area = Rect::new(0, 0, self.terminal_width, self.terminal_height);
        self.diagnosis_layout(area).max_scroll
    }

    pub(in crate::tui::app) fn diagnosis_page_rows(&self) -> isize {
        let area = Rect::new(0, 0, self.terminal_width, self.terminal_height);
        (self.diagnosis_layout(area).text.height as isize)
            .saturating_sub(1)
            .max(1)
    }

    pub(in crate::tui::app) fn scroll_diagnosis(&mut self, rows: isize) {
        let max = self.diagnosis_max_scroll();
        let next = if rows.is_negative() {
            self.diagnosis_scroll.saturating_sub(rows.unsigned_abs())
        } else {
            self.diagnosis_scroll.saturating_add(rows as usize)
        };
        self.diagnosis_scroll = next.min(max);
    }

    pub(in crate::tui::app) fn diagnosis_lines(&self, _width: u16) -> Vec<Line<'static>> {
        let Some(diagnosis) = &self.diagnosis else {
            return Vec::new();
        };
        let theme = self.diagnosis_theme();
        let mut lines: Vec<Line<'static>> = Vec::new();

        let conclusion = self.diagnosis_conclusion(diagnosis);
        lines.push(section(
            theme,
            &self.i18n.text("diagnose.section.conclusion"),
        ));
        lines.extend(wrapped(conclusion, theme.fg));
        lines.push(Line::default());

        if let Some(primary) = &diagnosis.primary {
            let rule = crate::core::diagnose::rules::rule_for(primary.rule);
            lines.push(section(theme, &self.i18n.text("diagnose.section.advice")));
            lines.extend(wrapped(
                self.i18n.format(rule.advice_key, &primary.arguments()),
                theme.fg,
            ));
            lines.push(Line::default());
            if !primary.evidence.is_empty() {
                lines.push(section(theme, &self.i18n.text("diagnose.section.evidence")));
                for evidence in primary.evidence.iter().take(3) {
                    lines.push(Line::from(Span::styled(
                        // The evidence is a raw log line: the loader puts a tab
                        // after `[stdout]`, and a `Paragraph` keeps it as a cell.
                        // Printed, it is a real tab stop and the rest of the row
                        // lands outside the card.
                        format!("  {}", crate::tui::text::printable(&evidence.text)),
                        Style::default().fg(theme.dim),
                    )));
                }
                lines.push(Line::default());
            }
            if !rule.remedies.is_empty() {
                lines.push(section(theme, &self.i18n.text("diagnose.section.remedies")));
                // One row per remedy with its cap in front: the cap is the key
                // that runs it, so a remedy without one is printed greyed out
                // rather than advertised as pressable.
                for action in rule.remedies {
                    let label = self.i18n.text(action.label_key());
                    let mut spans = vec![Span::styled("  ", Style::default().fg(theme.fg))];
                    match action.key_cap() {
                        Some(cap) => {
                            spans.push(Span::styled(
                                format!("[{cap}] "),
                                Style::default().fg(theme.key).add_modifier(Modifier::BOLD),
                            ));
                            spans.push(Span::styled(label, Style::default().fg(theme.fg)));
                        }
                        None => spans.push(Span::styled(label, Style::default().fg(theme.dim))),
                    }
                    lines.push(Line::from(spans));
                }
                lines.push(Line::default());
            }
        }

        if !diagnosis.others.is_empty() {
            lines.push(section(theme, &self.i18n.text("diagnose.section.others")));
            for other in &diagnosis.others {
                lines.push(Line::from(vec![
                    Span::styled("  · ", Style::default().fg(theme.dim)),
                    Span::styled(self.finding_reason(other), Style::default().fg(theme.dim)),
                ]));
            }
            lines.push(Line::default());
        }

        if !diagnosis.keywords.is_empty() {
            lines.push(section(theme, &self.i18n.text("diagnose.section.keywords")));
            lines.extend(wrapped(
                self.i18n.format(
                    "diagnose.unknown.keywords",
                    &[("keywords", &diagnosis.keywords.join(", "))],
                ),
                theme.fg,
            ));
            lines.push(Line::default());
        }

        if !diagnosis.sources.is_empty() {
            lines.push(section(theme, &self.i18n.text("diagnose.section.sources")));
            for source in &diagnosis.sources {
                let line = if source.present {
                    self.i18n.format(
                        "diagnose.sources.present",
                        &[
                            ("path", &source.path),
                            ("size", &crate::core::diagnose::report::size(source.bytes)),
                        ],
                    )
                } else {
                    self.i18n
                        .format("diagnose.sources.missing", &[("path", &source.path)])
                };
                lines.push(Line::from(Span::styled(
                    format!("  {}", crate::tui::text::printable(&line)),
                    Style::default().fg(theme.dim),
                )));
            }
        }
        while lines.last().is_some_and(|line| line.spans.is_empty()) {
            lines.pop();
        }
        // Last gate before the terminal: the reason and the advice carry values
        // captured from the log (`{class}`, `{path}`, `{reason}`), and a control
        // character in any of them would reach the backend as a real escape. A
        // tab becomes a tab stop and pushes the rest of the row out of the card,
        // so every string the card prints is sanitised here.
        lines
            .into_iter()
            .map(|line| {
                Line::from(
                    line.spans
                        .into_iter()
                        .map(|span| {
                            Span::styled(crate::tui::text::printable(&span.content), span.style)
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect()
    }

    fn diagnosis_conclusion(&self, diagnosis: &Diagnosis) -> String {
        if let Some(primary) = &diagnosis.primary {
            return self.finding_reason(primary);
        }
        match diagnosis.exit {
            ExitKind::Clean | ExitKind::UserKilled => self.i18n.text(diagnosis.exit.key()),
            _ => self.i18n.text("diagnose.unknown.reason"),
        }
    }

    /// Title row: the card's own name, with the facts on the right.
    ///
    /// The verdict lives in the body's first section and must not repeat in the
    /// border: a sentence is wider than a border title and arrives cut in half.
    pub(in crate::tui::app) fn diagnosis_title(&self) -> String {
        self.i18n.text("diagnose.title")
    }

    pub(in crate::tui::app) fn diagnosis_context(&self) -> String {
        let Some(diagnosis) = &self.diagnosis else {
            return String::new();
        };
        let stage = self.i18n.text(diagnosis.stage.key());
        let version = &diagnosis.facts.minecraft_version;
        let loader = &diagnosis.facts.loader;
        if version.is_empty() {
            stage
        } else {
            format!("{stage} · {version} · {loader}")
        }
    }

    pub(in crate::tui::app) fn draw_diagnosis_overlay_with(
        &self,
        frame: &mut Frame<'_>,
        theme: Theme,
    ) {
        let layout = self.diagnosis_layout(frame.area());
        frame.render_widget(ratatui::widgets::Clear, layout.rect);
        // Left, not centred: the facts sit on the right of the same row, and a
        // centred title meets them in the middle of a 72-column card.
        let block = crate::tui::chrome::panel_left(&self.diagnosis_title(), true, theme).title_top(
            // The padding is not cosmetic: right-aligned titles are drawn
            // flush against the corner, and the facts would touch both the
            // fill and the border.
            Line::from(Span::styled(
                format!(" {} ", self.diagnosis_context()),
                theme.dim(),
            ))
            .alignment(Alignment::Right),
        );
        frame.render_widget(block, layout.rect);
        if layout.text.is_empty() {
            return;
        }
        let lines = self.diagnosis_lines(layout.text.width);
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .scroll((u16::try_from(layout.scroll).unwrap_or(u16::MAX), 0)),
            layout.text,
        );
        // The key row always sits on the last body row, whatever the content
        // height: it is chrome, not the end of the text.
        let key_row = Rect {
            y: layout.body.bottom().saturating_sub(1),
            height: 1,
            ..layout.body
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                self.i18n.text("diagnose.keys"),
                theme.dim(),
            ))),
            key_row,
        );
    }
}

fn section(theme: Theme, title: &str) -> Line<'static> {
    Line::from(Span::styled(
        title.to_owned(),
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    ))
}

/// One paragraph as lines of its own, so long sentences wrap at the card width
/// instead of running off it. `Paragraph` would wrap them anyway, but then the
/// scroll maximum would double-count and the last rows would be unreachable.
fn wrapped(text: String, ink: Color) -> Vec<Line<'static>> {
    let style = Style::default().fg(ink);
    text.lines()
        .map(|line| Line::from(Span::styled(format!("  {line}"), style)))
        .collect()
}

/// Roughly how many rows the card wants, before wrapping.
fn diagnosis_rows(diagnosis: &Diagnosis) -> u16 {
    let mut rows = 6u16; // conclusion block + spacing
    if let Some(primary) = &diagnosis.primary {
        rows += 3; // advice
        rows += u16::try_from(primary.evidence.len()).unwrap_or(0).min(3) + 1;
        if !crate::core::diagnose::rules::rule_for(primary.rule)
            .remedies
            .is_empty()
        {
            rows += 2;
        }
    }
    rows += u16::try_from(diagnosis.others.len()).unwrap_or(0) + 1;
    rows += u16::try_from(diagnosis.sources.len()).unwrap_or(0) + 1;
    rows
}
