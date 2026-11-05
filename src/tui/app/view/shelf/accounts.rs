//! `shelf` accounts: a short stack of profiles.
//!
//! Three lines per account — name, kind, credential lifetime — with the current
//! profile marked on the name line. No avatar: there is no skin renderer in this
//! build, and a grey square pretending to be one is worse than nothing.

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::tone::Palette;
use super::widgets;
use super::*;
use crate::core::account::models::Account;
use crate::tui::action::ScrollTarget;
use crate::tui::hit::RowKind;
use crate::tui::pages::accounts::DeviceCodeView;
use crate::tui::palette;
use crate::tui::widgets::rows::{RowLayout, RowWindow};

/// Rows one account occupies: name, kind, credential lifetime.
const ACCOUNT_ROWS: u16 = 3;
/// Rows the device-code block reserves: status, code, URL, rule.
const DEVICE_ROWS: u16 = 4;
/// Columns between the selection bar and the text.
const TEXT_INSET: u16 = 2;

impl App {
    pub(in crate::tui::app) fn shelf_account_area(&self, main: Rect) -> Rect {
        let head = if self.accounts.device_code.is_some() {
            DEVICE_ROWS.saturating_add(1)
        } else {
            1
        };
        Rect::new(
            main.x,
            main.y.saturating_add(head),
            main.width,
            main.height.saturating_sub(head),
        )
    }

    pub(in crate::tui::app) fn draw_shelf_accounts(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &Palette,
    ) {
        if area.width < 24 || area.height < 3 {
            return;
        }
        let count = self.i18n.format(
            "account.count",
            &[("count", &self.accounts.accounts.len().to_string())],
        );
        frame.render_widget(
            Paragraph::new(Span::styled(
                count,
                Style::default().fg(pal.faint).bg(pal.bg),
            )),
            Rect::new(area.x, area.y, area.width, 1),
        );
        if let Some(device) = &self.accounts.device_code {
            self.shelf_device_code(frame, area, pal, device);
        }

        let body = self.shelf_account_area(area);
        if body.height == 0 {
            return;
        }
        self.hitmap
            .scroll(body, ScrollTarget::Rows(RowKind::Accounts));
        if self.accounts.accounts.is_empty() {
            self.shelf_accounts_empty(frame, body, pal);
            return;
        }
        let selected = self
            .accounts
            .selected
            .min(self.accounts.accounts.len().saturating_sub(1));
        let layout = self.row_windows.resolve(
            RowWindow::Accounts,
            RowLayout::cards_bare(
                body,
                self.accounts.accounts.len(),
                selected,
                ACCOUNT_ROWS,
                0,
            ),
        );
        self.hitmap.rows(layout, RowKind::Accounts);
        for (index, rect) in layout.rows() {
            let Some(account) = self.accounts.accounts.get(index) else {
                continue;
            };
            self.shelf_account_card(frame, rect, pal, account, index == selected);
        }
    }

    fn shelf_account_card(
        &self,
        frame: &mut Frame<'_>,
        card: Rect,
        pal: &Palette,
        account: &Account,
        active: bool,
    ) {
        if card.height == 0 {
            return;
        }
        if active {
            widgets::selection_band(frame, card, pal);
        }
        let background = if active { pal.surface } else { pal.bg };
        let current = self.accounts.current == Some(account.id);
        let marker = if current {
            self.i18n.text("account.current_short")
        } else {
            String::new()
        };
        let name_x = card.x.saturating_add(TEXT_INSET);
        let marker_width = palette::width(&marker) as u16;
        let name_width = card
            .right()
            .saturating_sub(name_x)
            .saturating_sub(marker_width)
            .saturating_sub(2);
        frame.render_widget(
            Paragraph::new(Span::styled(
                palette::truncate(&account.username, usize::from(name_width)),
                Style::default()
                    .fg(if active { pal.text } else { pal.dim })
                    .bg(background)
                    .add_modifier(if active {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            )),
            Rect::new(name_x, card.y, name_width, 1),
        );
        if current && marker_width > 0 {
            frame.render_widget(
                Paragraph::new(Span::styled(
                    marker,
                    Style::default()
                        .fg(pal.accent)
                        .bg(background)
                        .add_modifier(Modifier::BOLD),
                ))
                .alignment(Alignment::Right),
                Rect::new(
                    card.right().saturating_sub(marker_width),
                    card.y,
                    marker_width,
                    1,
                ),
            );
        }
        if card.height < 2 {
            return;
        }
        // The kind keeps the accent: it is what decides whether the profile can
        // join a server, and it is the first thing a player looks for.
        frame.render_widget(
            Paragraph::new(Span::styled(
                self.account_kind_label(account.kind),
                Style::default().fg(pal.accent_light).bg(background),
            )),
            Rect::new(name_x, card.y.saturating_add(1), card.width, 1),
        );
        if card.height < 3 {
            return;
        }
        frame.render_widget(
            Paragraph::new(Span::styled(
                palette::truncate(
                    &self.account_expiry_label(account),
                    usize::from(card.right().saturating_sub(name_x)),
                ),
                Style::default()
                    .fg(if active { pal.dim } else { pal.faint })
                    .bg(background),
            )),
            Rect::new(name_x, card.y.saturating_add(2), card.width, 1),
        );
    }

    /// The device-code block above the list: status, the code itself, the URL.
    ///
    /// The code is spaced out because it is read off the screen and typed into
    /// a phone; `ABCD1234` at this size is one word, `A B C D 1 2 3 4` is eight.
    fn shelf_device_code(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &Palette,
        device: &DeviceCodeView,
    ) {
        if area.height < DEVICE_ROWS.saturating_add(2) {
            return;
        }
        frame.render_widget(
            Paragraph::new(Span::styled(
                palette::truncate(&device.status, usize::from(area.width)),
                Style::default().fg(pal.dim).bg(pal.bg),
            )),
            Rect::new(area.x, area.y.saturating_add(1), area.width, 1),
        );
        let code = device
            .user_code
            .chars()
            .map(|character| character.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        frame.render_widget(
            Paragraph::new(Span::styled(
                code,
                Style::default()
                    .fg(pal.accent_light)
                    .bg(pal.bg)
                    .add_modifier(Modifier::BOLD),
            )),
            Rect::new(area.x, area.y.saturating_add(2), area.width, 1),
        );
        frame.render_widget(
            Paragraph::new(Span::styled(
                palette::truncate(&device.verification_uri, usize::from(area.width)),
                Style::default().fg(pal.faint).bg(pal.bg),
            )),
            Rect::new(area.x, area.y.saturating_add(3), area.width, 1),
        );
        widgets::rule(
            frame,
            Rect::new(area.x, area.y.saturating_add(4), area.width, 1),
            pal,
        );
    }

    fn shelf_accounts_empty(&self, frame: &mut Frame<'_>, area: Rect, pal: &Palette) {
        frame.render_widget(
            Paragraph::new(Span::styled(
                palette::truncate(
                    &self.i18n.text("account.login_empty"),
                    usize::from(area.width),
                ),
                Style::default().fg(pal.dim).bg(pal.bg),
            )),
            Rect::new(area.x, area.y, area.width, 1),
        );
        if area.height < 2 {
            return;
        }
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("n", Style::default().fg(pal.accent).bg(pal.bg)),
                Span::styled(
                    format!(" {}", self.i18n.text("action.offline_account")),
                    Style::default().fg(pal.dim).bg(pal.bg),
                ),
                Span::styled("   m", Style::default().fg(pal.accent).bg(pal.bg)),
                Span::styled(
                    format!(" {}", self.i18n.text("action.microsoft_login")),
                    Style::default().fg(pal.dim).bg(pal.bg),
                ),
            ])),
            Rect::new(area.x, area.y.saturating_add(1), area.width, 1),
        );
    }
}
