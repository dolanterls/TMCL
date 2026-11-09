use std::io::{self, Stdout};

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::core::error::{Error, Result};

pub struct Session {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    suspended: bool,
    mouse_capture_enabled: bool,
    mouse_capture_active: bool,
    /// `Some(true)` when the terminal answered OSC 11 with a light background.
    /// `None` covers both "did not ask" and "the terminal stayed silent".
    background: Option<bool>,
}

impl Session {
    pub fn enter(detect_background: bool) -> Result<Self> {
        enable_raw_mode().map_err(|error| Error::Terminal(format!("enable raw mode: {error}")))?;
        // Keys typed while the launcher was loading are still queued in the
        // console. Dropping them here keeps a stray Enter from reaching the
        // first frame, where it would activate the overview card and start a
        // game the user never asked for.
        crate::platform::windows::discard_pending_console_input();
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen, EnableMouseCapture) {
            let _ = disable_raw_mode();
            return Err(Error::Terminal(format!("enter alternate screen: {error}")));
        }
        // Asked after the alternate screen is up: the reply arrives on stdin
        // as ordinary characters, and asking earlier would paint the query
        // into the shell that is about to be replaced.
        let background = if detect_background {
            query_background()
        } else {
            None
        };
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)
            .map_err(|error| Error::Terminal(format!("create terminal: {error}")))?;
        Ok(Self {
            terminal,
            suspended: false,
            mouse_capture_enabled: true,
            mouse_capture_active: true,
            background,
        })
    }

    pub fn set_mouse_capture(&mut self, enabled: bool) -> Result<()> {
        self.mouse_capture_enabled = enabled;
        if self.suspended || self.mouse_capture_active == enabled {
            return Ok(());
        }
        let result = if enabled {
            execute!(self.terminal.backend_mut(), EnableMouseCapture)
        } else {
            execute!(self.terminal.backend_mut(), DisableMouseCapture)
        };
        result
            .map_err(|error| Error::Terminal(format!("set mouse capture: {error}")))
            .inspect(|_| self.mouse_capture_active = enabled)
    }

    /// Hands the detected tone to `App` once; the session does not keep it.
    pub fn take_background(&mut self) -> Option<bool> {
        self.background.take()
    }

    pub fn terminal_mut(&mut self) -> &mut Terminal<CrosstermBackend<Stdout>> {
        &mut self.terminal
    }

    pub fn suspend(&mut self) -> Result<()> {
        if self.suspended {
            return Ok(());
        }
        self.terminal
            .show_cursor()
            .map_err(|error| Error::Terminal(format!("show terminal cursor: {error}")))?;
        disable_raw_mode()
            .map_err(|error| Error::Terminal(format!("disable raw mode: {error}")))?;
        execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )
        .map_err(|error| Error::Terminal(format!("suspend terminal: {error}")))?;
        self.mouse_capture_active = false;
        self.suspended = true;
        Ok(())
    }

    pub fn resume(&mut self) -> Result<()> {
        if !self.suspended {
            return Ok(());
        }
        enable_raw_mode().map_err(|error| Error::Terminal(format!("enable raw mode: {error}")))?;
        if let Err(error) = execute!(self.terminal.backend_mut(), EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(Error::Terminal(format!("resume terminal: {error}")));
        }
        if self.mouse_capture_enabled {
            execute!(self.terminal.backend_mut(), EnableMouseCapture).map_err(|error| {
                let _ = disable_raw_mode();
                Error::Terminal(format!("resume mouse capture: {error}"))
            })?;
        }
        self.mouse_capture_active = self.mouse_capture_enabled;
        self.suspended = false;
        self.terminal
            .clear()
            .map_err(|error| Error::Terminal(format!("redraw terminal: {error}")))
    }
}

/// Asks the terminal for its background colour (OSC 11) and waits briefly.
///
/// The reply is `ESC ] 11 ; rgb:RRRR/GGGG/BBBB BEL`. crossterm surfaces it as
/// a burst of key events, so the characters are collected and parsed rather
/// than read as a raw stream: going around the event queue would fight the
/// reader thread that starts immediately afterwards.
///
/// A terminal that does not answer costs `TIMEOUT` and returns `None`. That is
/// the common case for conhost and for anything older than Windows Terminal
/// 1.6, so the timeout is short and the caller treats silence as "unknown".
fn query_background() -> Option<bool> {
    use std::io::Write;

    const TIMEOUT: std::time::Duration = std::time::Duration::from_millis(90);

    let mut stdout = io::stdout();
    stdout.write_all(b"\x1b]11;?\x07").ok()?;
    stdout.flush().ok()?;

    let deadline = std::time::Instant::now() + TIMEOUT;
    let mut text = String::new();
    while std::time::Instant::now() < deadline {
        if !crossterm::event::poll(std::time::Duration::from_millis(5)).ok()? {
            continue;
        }
        let Ok(crossterm::event::Event::Key(key)) = crossterm::event::read() else {
            continue;
        };
        match key.code {
            crossterm::event::KeyCode::Esc => text.push('\u{1b}'),
            crossterm::event::KeyCode::Char(ch) => text.push(ch),
            _ => continue,
        }
        if text.ends_with('\u{7}') || text.ends_with("\u{1b}\\") {
            break;
        }
    }
    parse_osc11(&text)
}

/// `ESC ] 11 ; rgb:RRRR/GGGG/BBBB BEL` → is the background light?
///
/// Split out so the parsing can be tested without a terminal. Channels come
/// back in 1, 2 or 4 hex digits depending on the terminal; all three are
/// scaled to 8 bits.
fn parse_osc11(text: &str) -> Option<bool> {
    let start = text.find("]11;")?;
    let body = text[start + 4..]
        .trim_end_matches('\u{7}')
        .trim_end_matches("\u{1b}\\");
    let rgb = body.trim().strip_prefix("rgb:")?;
    let mut channels = rgb.split('/');
    let red = osc_channel(channels.next()?)?;
    let green = osc_channel(channels.next()?)?;
    let blue = osc_channel(channels.next()?)?;
    let luminance =
        crate::tui::palette::relative_luminance(ratatui::style::Color::Rgb(red, green, blue));
    Some(luminance > 0.5)
}

fn osc_channel(value: &str) -> Option<u8> {
    let value = value.trim();
    let parsed = u32::from_str_radix(value, 16).ok()?;
    match value.len() {
        1 => Some((parsed * 17) as u8),
        2 => Some(parsed as u8),
        4 => Some((parsed / 257) as u8),
        _ => None,
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        if !self.suspended {
            let _ = execute!(
                self.terminal.backend_mut(),
                LeaveAlternateScreen,
                DisableMouseCapture
            );
        }
        let _ = self.terminal.show_cursor();
    }
}

#[cfg(test)]
mod tests {
    use super::parse_osc11;

    /// Every shape a terminal replies with: 4-digit, 2-digit and 1-digit
    /// channels, BEL and ST terminators.
    #[test]
    fn osc11_answers_light_and_dark_for_every_channel_width() {
        assert_eq!(parse_osc11("\u{1b}]11;rgb:ffff/ffff/ffff\u{7}"), Some(true));
        assert_eq!(
            parse_osc11("\u{1b}]11;rgb:ffff/ffff/ffff\u{1b}\\"),
            Some(true)
        );
        assert_eq!(
            parse_osc11("\u{1b}]11;rgb:0c0c/0c0c/0c0c\u{7}"),
            Some(false)
        );
        assert_eq!(parse_osc11("\u{1b}]11;rgb:1e/1e/1e\u{7}"), Some(false));
        assert_eq!(parse_osc11("\u{1b}]11;rgb:f/f/f\u{7}"), Some(true));
        assert_eq!(parse_osc11("\u{1b}]11;rgb:0/0/0\u{7}"), Some(false));
    }

    #[test]
    fn osc11_ignores_anything_it_does_not_understand() {
        assert_eq!(parse_osc11(""), None);
        assert_eq!(parse_osc11("garbage"), None);
        assert_eq!(parse_osc11("\u{1b}]11;not-a-colour\u{7}"), None);
        assert_eq!(parse_osc11("\u{1b}]11;rgb:12/34\u{7}"), None);
        // Silence is the common case: a terminal that does not know OSC 11.
        assert_eq!(parse_osc11("\u{1b}[A"), None);
    }
}
