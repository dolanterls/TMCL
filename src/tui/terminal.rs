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
}

impl Session {
    pub fn enter() -> Result<Self> {
        enable_raw_mode().map_err(|error| Error::Terminal(format!("enable raw mode: {error}")))?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen, EnableMouseCapture) {
            let _ = disable_raw_mode();
            return Err(Error::Terminal(format!("enter alternate screen: {error}")));
        }
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)
            .map_err(|error| Error::Terminal(format!("create terminal: {error}")))?;
        Ok(Self {
            terminal,
            suspended: false,
            mouse_capture_enabled: true,
            mouse_capture_active: true,
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
