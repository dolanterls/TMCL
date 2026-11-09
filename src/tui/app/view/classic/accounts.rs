//! Accounts page.

use super::*;
use crate::tui::hit::RowKind;

impl App {
    pub(in crate::tui::app) fn accounts_hints(&self) -> Vec<crate::tui::chrome::Hint> {
        vec![
            self.hint("Enter", "action.use_current"),
            self.hint("n", "action.offline_account"),
            self.hint("m", "action.microsoft_login"),
            self.hint("d", "action.remove"),
        ]
    }

    pub(super) fn draw_accounts_page(&self, frame: &mut Frame<'_>, area: Rect) {
        if self.accounts.accounts.is_empty() {
            crate::tui::widgets::empty::render(
                frame,
                area,
                &self.i18n.text("page.accounts"),
                &self.i18n.text("account.login_empty"),
                self.theme,
            );
            return;
        }
        if self.is_narrow() {
            let items = self
                .accounts
                .accounts
                .iter()
                .map(|account| {
                    let current = if self.accounts.current == Some(account.id) {
                        format!(" · {}", self.i18n.text("account.current_short"))
                    } else {
                        String::new()
                    };
                    format!("{} · {:?}{current}", account.username, account.kind)
                })
                .collect::<Vec<_>>();
            self.render_compact_list(
                frame,
                area,
                &self.i18n.text("page.accounts"),
                &items,
                self.accounts.selected,
                RowWindow::Accounts,
            );
            return;
        }
        let mut table_area = area;
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
                Paragraph::new(Line::from(Span::styled(line, self.theme.dim()))),
                sections[0],
            );
            table_area = sections[1];
        }
        let rows: Vec<Vec<String>> = self
            .accounts
            .accounts
            .iter()
            .map(|account| {
                let current = self.accounts.current == Some(account.id);
                vec![
                    account.username.clone(),
                    format!("{:?}", account.kind),
                    crate::tui::widgets::status::badge_with_theme(
                        if current {
                            StatusKind::Current
                        } else {
                            StatusKind::Ready
                        },
                        if current {
                            self.i18n.text("common.current")
                        } else {
                            self.i18n.text("status.ready")
                        },
                        self.theme,
                    )
                    .to_string(),
                ]
            })
            .collect();
        let selected = self.accounts.selected.min(self.accounts.accounts.len() - 1);
        let layout = self.row_windows.resolve(
            RowWindow::Accounts,
            RowLayout::table(table_area, rows.len(), selected),
        );
        crate::tui::widgets::table::render_layout(
            layout,
            frame,
            table_area,
            self.i18n.text("page.accounts"),
            vec![
                self.i18n.text("account.username"),
                self.i18n.text("common.type"),
                self.i18n.text("overview.status"),
            ],
            rows,
            Some(selected),
            &[
                Constraint::Percentage(40),
                Constraint::Length(16),
                Constraint::Min(1),
            ],
            true,
            self.theme,
        );
        self.hitmap.rows(layout, RowKind::Accounts);
    }

    pub(in crate::tui::app) fn open_offline_account_form(&mut self) {
        self.modal = Some(Modal::form(
            self.i18n.text("form.add_offline_account"),
            &[self.i18n.text("form.username")],
            &[String::new()],
            FormAction::AddOfflineAccount,
        ));
    }

    pub(in crate::tui::app) fn select_current_account(&mut self) {
        let Some(account) = self.accounts.accounts.get(self.accounts.selected) else {
            self.status = Status::info(self.i18n.text("status.add_account"));
            return;
        };
        self.config.state.current_account_id = Some(account.id.to_string());
        if let Err(error) = self.save_config() {
            self.status = Status::error(error.to_string());
        } else {
            self.accounts.current = Some(account.id);
            self.status = Status::info(
                self.i18n
                    .format("status.account_current", &[("name", &account.username)]),
            );
        }
    }

    pub(in crate::tui::app) fn start_microsoft_login(&mut self) {
        let paths = self.paths.clone();
        let sender = self.sender.clone();
        let language = self.i18n.language();
        self.status = Status::info(self.i18n.text("status.microsoft_device_requested"));
        tokio::spawn(async move {
            let i18n = I18n::new(language);
            let result = async {
                let oauth = MicrosoftOAuth::new(HttpClient::new()?);
                let device = oauth.request_device_code().await?;
                let browser_status =
                    match crate::platform::windows::open_url(&device.verification_uri) {
                        Ok(()) => i18n.text("status.browser_waiting"),
                        Err(error) => {
                            i18n.format("status.browser_not_open", &[("error", &error.to_string())])
                        }
                    };
                let _ = sender.send(AppEvent::DeviceCode {
                    user_code: device.user_code.clone(),
                    verification_uri: device.verification_uri.clone(),
                    expires_in: device.expires_in,
                    browser_status,
                });
                let _ = sender.send(AppEvent::Status(i18n.text("status.microsoft_waiting")));
                let session = oauth.login(&device).await?;
                let id = Uuid::new_v4();
                let refresh_token = session
                    .refresh_token
                    .clone()
                    .ok_or_else(|| Error::Account("Microsoft returned no refresh token".into()))?;
                let store = SecretStore::new(paths.root.join("secrets"));
                let secret_ref = store.save(id, refresh_token.as_bytes())?;
                crate::core::account::microsoft::session::remember(&store, id, &session);
                let account = Account {
                    id,
                    kind: AccountKind::Microsoft,
                    username: session.username,
                    uuid: session.uuid,
                    skin: None,
                    secret_ref: Some(secret_ref),
                    access_expires_at: Some(unix_seconds() + session.expires_in),
                };
                AccountStore::new(paths.accounts_file).add(account.clone())?;
                Ok::<_, Error>(account.username)
            }
            .await;
            let message = result.map_or_else(
                |error| {
                    i18n.format(
                        "status.microsoft_login_failed",
                        &[("error", &error.to_string())],
                    )
                },
                |username| i18n.format("status.microsoft_account_added", &[("name", &username)]),
            );
            let _ = sender.send(AppEvent::Status(message));
            let _ = sender.send(AppEvent::Reload);
        });
    }
}
