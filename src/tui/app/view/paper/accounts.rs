//! `paper` accounts: a short directory of profiles.
//!
//! Each account is two lines — name, then kind and credential lifetime — with
//! the current profile marked at the right edge. A device-code login prints its
//! code above the list in the same block rhythm.

use super::*;
use crate::core::account::models::Account;
use crate::tui::hit::RowKind;
use crate::tui::palette;
use crate::tui::widgets::rows::{RowLayout, RowWindow};

/// Rows each account occupies: name, then the detail line.
const ACCOUNT_ROWS: u16 = 2;
const MARGIN: u16 = 3;

impl App {
    pub(in crate::tui::app) fn draw_paper_accounts(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &PaperPalette,
    ) {
        if area.width < 24 || area.height < 3 {
            return;
        }
        let title = self.i18n.text("page.accounts");
        let count = self.i18n.format(
            "account.count",
            &[("count", &self.accounts.accounts.len().to_string())],
        );
        let mut body = kit::section(frame, area, pal, &title, Some(&count));
        if let Some(device) = &self.accounts.device_code {
            body = self.paper_device_code(frame, body, pal, device);
        }
        self.hitmap.scroll(
            body,
            crate::tui::action::ScrollTarget::Rows(RowKind::Accounts),
        );
        if self.accounts.accounts.is_empty() {
            self.paper_accounts_empty(frame, body, pal);
            return;
        }
        if body.height < ACCOUNT_ROWS {
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
            self.paper_account_row(frame, rect, pal, account, index == selected);
        }
    }

    fn paper_account_row(
        &self,
        frame: &mut Frame<'_>,
        row: Rect,
        pal: &PaperPalette,
        account: &Account,
        active: bool,
    ) {
        if row.height == 0 {
            return;
        }
        if active {
            kit::selection(frame, row, pal);
        }
        let current = self.accounts.current == Some(account.id);
        let marker = if current {
            self.i18n.text("common.current")
        } else {
            String::new()
        };
        let marker_width = palette::width(&marker) as u16;
        let name_x = row.x.saturating_add(MARGIN);
        let name_width = row
            .width
            .saturating_sub(MARGIN + marker_width)
            .saturating_sub(1);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                palette::truncate(&account.username, usize::from(name_width)),
                if active {
                    ink_marked(pal)
                } else {
                    ink(pal, pal.ink)
                },
            ))),
            Rect::new(name_x, row.y, name_width, 1),
        );
        if current {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(marker, ink(pal, pal.accent)))),
                Rect::new(
                    row.right().saturating_sub(marker_width),
                    row.y,
                    marker_width,
                    1,
                ),
            );
        }
        if row.height < 2 {
            return;
        }
        let detail = self.paper_account_detail(account);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                palette::truncate(&detail, usize::from(row.width.saturating_sub(MARGIN))),
                ink(pal, pal.faint),
            ))),
            Rect::new(
                name_x,
                row.y.saturating_add(1),
                row.width.saturating_sub(MARGIN),
                1,
            ),
        );
    }

    /// `微软 · 有效 1 小时` / `离线 · 本地档案`.
    fn paper_account_detail(&self, account: &Account) -> String {
        self.account_detail_line(account)
    }

    /// Prints the device code and returns the rectangle left for the list.
    fn paper_device_code(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &PaperPalette,
        device: &crate::tui::pages::accounts::DeviceCodeView,
    ) -> Rect {
        if area.height < 5 {
            return area;
        }
        // The status line already says "waiting in the browser" in the user's
        // language; the URI is what they have to type, so it gets its own line.
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                palette::truncate(&device.status, usize::from(area.width)),
                ink(pal, pal.dim),
            ))),
            Rect::new(area.x, area.y, area.width, 1),
        );
        let code = device
            .user_code
            .chars()
            .map(|character| character.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                code,
                ink(pal, pal.accent).add_modifier(Modifier::BOLD),
            ))),
            Rect::new(area.x, area.y.saturating_add(1), area.width, 1),
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                palette::truncate(&device.verification_uri, usize::from(area.width)),
                ink(pal, pal.faint),
            ))),
            Rect::new(area.x, area.y.saturating_add(2), area.width, 1),
        );
        kit::rule(
            frame,
            Rect::new(area.x, area.y.saturating_add(3), area.width, 1),
            pal,
            pal.rule_soft,
        );
        Rect::new(
            area.x,
            area.y.saturating_add(4),
            area.width,
            area.height.saturating_sub(4),
        )
    }

    fn paper_accounts_empty(&self, frame: &mut Frame<'_>, area: Rect, pal: &PaperPalette) {
        if area.height == 0 {
            return;
        }
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                palette::truncate(
                    &self.i18n.text("account.login_empty"),
                    usize::from(area.width),
                ),
                ink(pal, pal.faint),
            ))),
            Rect::new(area.x, area.y, area.width, 1),
        );
        if area.height < 2 {
            return;
        }
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("n", ink(pal, pal.accent)),
                Span::styled(
                    format!(" {}", self.i18n.text("action.offline_account")),
                    ink(pal, pal.dim),
                ),
                Span::styled("   m", ink(pal, pal.accent)),
                Span::styled(
                    format!(" {}", self.i18n.text("action.microsoft_login")),
                    ink(pal, pal.dim),
                ),
            ])),
            Rect::new(area.x, area.y.saturating_add(1), area.width, 1),
        );
    }
}
