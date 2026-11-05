//! Accounts page: one card per account with its kind and validity.

use crate::tui::app::view::sheet::Sheet;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use super::*;
use crate::tui::hit::RowKind;
use crate::tui::palette;
use crate::tui::widgets::kit as widgets;

impl App {
    pub(in crate::tui::app) fn draw_creeper_accounts(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        t: f64,
    ) {
        let pal = self.creeper_palette();
        if self.accounts.accounts.is_empty() {
            widgets::empty_card(
                frame,
                area,
                &self.i18n.text("page.accounts"),
                &self.i18n.text("account.login_empty"),
                pal.pink,
                t,
            );
            return;
        }
        let mut list_area = area;
        if let Some(device_code) = &self.accounts.device_code {
            let sections = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Min(0)])
                .split(area);
            let line = self.i18n.format(
                "account.device_code",
                &[
                    ("code", &device_code.user_code),
                    ("uri", &device_code.verification_uri),
                    ("remaining", &device_code.remaining_seconds.to_string()),
                ],
            );
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    widgets::pill(
                        self.i18n.text("account.device_code_label"),
                        palette::mix(pal.pink, pal.text, 0.1),
                        pal.fill(pal.pink, 0.2),
                    ),
                    Span::styled(
                        format!(" {line}"),
                        Style::default().fg(pal.text_dim).bg(pal.surface),
                    ),
                ])),
                sections[0],
            );
            list_area = sections[1];
        }
        self.account_cards(frame, list_area, t);
    }

    fn account_cards(&self, frame: &mut Frame<'_>, area: Rect, t: f64) {
        let pal = self.creeper_palette();
        let block = widgets::with_counter(
            widgets::card(
                &self.i18n.text("page.accounts"),
                pal.pink,
                true,
                t,
                pal.surface,
            ),
            self.accounts.selected + 1,
            self.accounts.accounts.len(),
        );
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.height < 2 || inner.width < 12 {
            return;
        }
        let stride = self.creeper_card_height();
        let layout = self.row_windows.resolve(
            RowWindow::Accounts,
            RowLayout::cards(
                area,
                self.accounts.accounts.len(),
                self.accounts.selected,
                stride,
            ),
        );
        self.hitmap.rows(layout, RowKind::Accounts);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" ", Style::default().bg(pal.surface)),
                Span::styled(
                    format!(
                        "{} {}",
                        self.accounts.accounts.len(),
                        self.i18n.text("page.accounts")
                    ),
                    Style::default().fg(pal.text_faint).bg(pal.surface),
                ),
            ]))
            .style(Style::default().bg(pal.surface)),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );

        let start = layout.offset();
        for (visual, account) in self
            .accounts
            .accounts
            .iter()
            .skip(start)
            .take(layout.visible_rows())
            .enumerate()
        {
            let y = inner
                .y
                .saturating_add(1)
                .saturating_add(visual as u16 * stride);
            let height = stride.min(inner.bottom().saturating_sub(y));
            if height == 0 {
                break;
            }
            let active = start + visual == self.accounts.selected;
            let current = self.accounts.current == Some(account.id);
            let bg = if active {
                pal.fill(pal.pink, 0.2)
            } else if visual % 2 == 1 {
                pal.fill(pal.surface_alt, 0.5)
            } else {
                pal.surface
            };
            frame.render_widget(
                Block::default().style(Style::default().bg(bg)),
                Rect::new(inner.x, y, inner.width, height),
            );
            let initial = account
                .username
                .chars()
                .next()
                .map_or_else(|| "?".to_owned(), |ch| ch.to_uppercase().to_string());
            let kind_label = match account.kind {
                crate::core::account::models::AccountKind::Microsoft => {
                    self.i18n.text("creeper.account.microsoft")
                }
                crate::core::account::models::AccountKind::Offline => {
                    self.i18n.text("creeper.account.offline")
                }
            };
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        if active { "▌ " } else { "  " },
                        Style::default()
                            .fg(palette::mix(pal.pink, pal.text, 0.15))
                            .bg(bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!(" {initial} "),
                        Style::default()
                            .fg(pal.bg)
                            .bg(palette::mix(pal.pink, pal.text, 0.2))
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" ", Style::default().bg(bg)),
                    Span::styled(
                        palette::truncate(
                            &account.username,
                            usize::from(inner.width).saturating_sub(26),
                        ),
                        Style::default()
                            .fg(pal.text)
                            .bg(bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("  ", Style::default().bg(bg)),
                    widgets::soft_pill(kind_label, pal.pink, bg),
                    Span::styled(" ", Style::default().bg(bg)),
                    if current {
                        widgets::status_pill(
                            StatusKind::Current,
                            self.i18n.text("common.current"),
                            bg,
                        )
                    } else {
                        Span::styled(String::new(), Style::default().bg(bg))
                    },
                ]))
                .style(Style::default().bg(bg)),
                Rect::new(inner.x, y, inner.width, 1),
            );
            if height >= 2 {
                frame.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::styled("     ", Style::default().bg(bg)),
                        Span::styled(
                            palette::truncate(&account.uuid.to_string(), 36),
                            Style::default().fg(pal.text_faint).bg(bg),
                        ),
                    ]))
                    .style(Style::default().bg(bg)),
                    Rect::new(inner.x, y + 1, inner.width, 1),
                );
            }
            if height >= 3 {
                // One decision for every preset: the credential line cannot
                // say `已过期` here and `自动续期` one page over.
                let expiry = self.account_expiry_label(account);
                frame.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::styled("     ", Style::default().bg(bg)),
                        Span::styled(
                            palette::truncate(&expiry, usize::from(inner.width).saturating_sub(5)),
                            Style::default().fg(pal.text_dim).bg(bg),
                        ),
                    ]))
                    .style(Style::default().bg(bg)),
                    Rect::new(inner.x, y + 2, inner.width, 1),
                );
            }
        }
    }
}
