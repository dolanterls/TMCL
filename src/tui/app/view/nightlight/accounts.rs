//! Accounts page: one three-row card per account.
//! Design inspiration: nightlightd (GPL-3.0-or-later).
//! This file is an independent implementation; no source code is copied.

use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::core::account::models::{Account, AccountKind};
use crate::tui::app::view::nightlight::tone::Palette;
use crate::tui::app::view::nightlight::widgets::{self, card, card_inner, pill};
use crate::tui::hit::RowKind;
use crate::tui::palette::truncate;
use crate::tui::widgets::rows::{RowLayout, RowWindow};

use super::App;

const CARD_ROWS: u16 = 3;
const CARD_GAP: u16 = 1;

pub(in crate::tui::app) fn layout(area: Rect) -> Rect {
    area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    })
}

impl App {
    pub(in crate::tui::app) fn draw_nightlight_accounts(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        pal: &Palette,
    ) {
        let area = layout(area);
        card(frame, area, &self.i18n.text("page.accounts"), pal);
        let inner = card_inner(area);
        if inner.is_empty() {
            return;
        }
        self.hitmap.scroll(
            inner,
            crate::tui::action::ScrollTarget::Rows(RowKind::Accounts),
        );
        if self.accounts.accounts.is_empty() {
            frame.render_widget(
                Paragraph::new(self.i18n.text("account.login_empty"))
                    .style(Style::default().fg(pal.muted).bg(pal.surface)),
                inner,
            );
            return;
        }
        if let Some(device_code) = &self.accounts.device_code {
            let line = self.i18n.format(
                "account.device_code",
                &[
                    ("code", &device_code.user_code),
                    ("uri", &device_code.verification_uri),
                    ("remaining", &device_code.remaining_seconds.to_string()),
                ],
            );
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    truncate(&line, usize::from(inner.width)),
                    Style::default().fg(pal.accent2).bg(pal.surface),
                ))),
                Rect::new(inner.x, inner.y, inner.width, 1),
            );
        }
        let top = inner.y + u16::from(self.accounts.device_code.is_some());
        let rows = Rect::new(
            inner.x,
            top,
            inner.width,
            inner.bottom().saturating_sub(top),
        );
        if rows.is_empty() {
            return;
        }
        let selected = self
            .accounts
            .selected
            .min(self.accounts.accounts.len().saturating_sub(1));
        let layout = self.row_windows.resolve(
            RowWindow::Accounts,
            RowLayout::cards_bare(
                rows,
                self.accounts.accounts.len(),
                selected,
                CARD_ROWS,
                CARD_GAP,
            ),
        );
        self.hitmap.rows(layout, RowKind::Accounts);
        let stride = CARD_ROWS + CARD_GAP;
        let start = layout.offset();
        for (visual, index) in (start..self.accounts.accounts.len())
            .take(layout.visible_rows())
            .enumerate()
        {
            let Some(account) = self.accounts.accounts.get(index) else {
                continue;
            };
            let y = rows.y.saturating_add(visual as u16 * stride);
            let height = CARD_ROWS.min(rows.bottom().saturating_sub(y));
            if height == 0 {
                break;
            }
            self.account_card(
                frame,
                Rect::new(rows.x, y, rows.width, height),
                account,
                start + visual == selected,
                pal,
            );
        }
    }

    fn account_card(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        account: &Account,
        active: bool,
        pal: &Palette,
    ) {
        let background = if active { pal.selection } else { pal.surface };
        frame.render_widget(
            Block::default().style(Style::default().bg(background)),
            area,
        );
        let current = self.accounts.current == Some(account.id);
        let avatar = avatar_cells(&account.username);
        let kind = self.i18n.text(match account.kind {
            AccountKind::Microsoft => "creeper.account.microsoft",
            AccountKind::Offline => "creeper.account.offline",
        });
        let badge = pill(
            &kind,
            if account.kind == AccountKind::Microsoft {
                pal.accent2
            } else {
                pal.muted
            },
            pal,
        );
        // 4 = avatar cell (" ST "), 2 = the gap before the right-aligned
        // badge. The current marker is another 4 cells; leaving it out pushed
        // the badge two columns past the card edge on every selected account.
        let mut used = 6 + widgets::width(&kind) + 2;
        if current {
            used += 4;
        }
        let mut head = vec![
            Span::styled(
                format!(" {avatar} "),
                Style::default()
                    .fg(pal.accent)
                    .bg(background)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                truncate(
                    &account.username,
                    usize::from(area.width).saturating_sub(used).max(4),
                ),
                Style::default()
                    .fg(pal.text)
                    .bg(background)
                    .add_modifier(Modifier::BOLD),
            ),
        ];
        if current {
            head.push(Span::styled(
                "  ● ",
                Style::default().fg(pal.ok).bg(background),
            ));
        }
        push_right(&mut head, badge, usize::from(area.width), background);
        frame.render_widget(
            Paragraph::new(Line::from(head)).style(Style::default().bg(background)),
            Rect::new(area.x, area.y, area.width, 1),
        );
        if area.height >= 2 {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        if active { "▎  " } else { "   " },
                        Style::default().fg(pal.accent).bg(background),
                    ),
                    Span::styled(
                        truncate(
                            &account.uuid.to_string(),
                            usize::from(area.width).saturating_sub(4),
                        ),
                        Style::default().fg(pal.faint).bg(background),
                    ),
                ]))
                .style(Style::default().bg(background)),
                Rect::new(area.x, area.y + 1, area.width, 1),
            );
        }
        if area.height >= 3 {
            let session = self.account_expiry_label(account);
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        if active { "▎  " } else { "   " },
                        Style::default().fg(pal.accent).bg(background),
                    ),
                    Span::styled(
                        truncate(&session, usize::from(area.width).saturating_sub(4)),
                        Style::default().fg(pal.muted).bg(background),
                    ),
                ]))
                .style(Style::default().bg(background)),
                Rect::new(area.x, area.y + 2, area.width, 1),
            );
        }
    }
}

/// Two initials in a box-like cell: `ST` for `Steve`.
fn avatar_cells(name: &str) -> String {
    let mut letters = name.chars().filter(|character| character.is_alphanumeric());
    let first = letters.next().unwrap_or('?');
    let second = letters.next().unwrap_or(first);
    format!("{first}{second}").to_uppercase()
}

fn push_right(
    spans: &mut Vec<Span<'static>>,
    right: Span<'static>,
    width: usize,
    bg: ratatui::style::Color,
) {
    let used = spans
        .iter()
        .map(|span| widgets::width(&span.content))
        .sum::<usize>()
        + widgets::width(&right.content);
    if width > used {
        spans.push(Span::styled(
            " ".repeat(width - used),
            Style::default().bg(bg),
        ));
    }
    spans.push(right);
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::{avatar_cells, layout};

    #[test]
    fn the_accounts_page_fills_its_content_column() {
        let area = layout(Rect::new(10, 0, 93, 38));
        assert_eq!(area.x, 11);
        assert_eq!(area.bottom(), 37);
    }

    #[test]
    fn avatars_take_two_letters() {
        assert_eq!(avatar_cells("Steve"), "ST");
        assert_eq!(avatar_cells("alex"), "AL");
        assert_eq!(avatar_cells("x"), "XX");
        assert_eq!(avatar_cells(""), "??");
    }
}
