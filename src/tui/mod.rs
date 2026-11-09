pub mod action;
pub mod app;
pub mod chrome;
pub mod command;
pub mod event;
pub mod hit;
pub mod input;
pub mod modal;
pub mod pages;
pub mod palette;
pub mod shell;
pub mod terminal;
pub mod text;
pub mod theme;
pub mod widgets;

use std::io::IsTerminal;

use tokio::sync::mpsc;

use crate::core::error::{Error, Result};
use crate::tui::event::AppEvent;
use crossterm::event::MouseEventKind;

pub async fn run() -> Result<()> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(Error::Terminal(
            "TUI requires an interactive terminal".into(),
        ));
    }

    let (background_sender, mut background_events) = mpsc::unbounded_channel();
    let (input_sender, mut input_events) = mpsc::unbounded_channel();
    let mut app = app::App::load(background_sender.clone())?;
    app.start_update_check();
    // One line, once: a launcher that cannot be typed by name is worth
    // mentioning, and the next status message replaces it anyway.
    app.announce_missing_path_entry();
    let mut task_updates = app.subscribe_task_snapshots();
    let mut terminal = terminal::Session::enter(app.wants_terminal_background())?;
    app.set_terminal_background(terminal.take_background());
    let mut reader = Some(event::spawn_reader(input_sender.clone()));
    let mut tick_ms = app.refresh_millis();
    let mut redraw = tokio::time::interval(std::time::Duration::from_millis(tick_ms));
    redraw.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    while !app.should_quit() {
        terminal
            .terminal_mut()
            .draw(|frame| app.draw(frame))
            .map_err(|error| Error::Terminal(format!("draw TUI: {error}")))?;
        tokio::select! {
            biased;
            Some(event) = input_events.recv() => {
                // Coalesce the burst: hover sweeps arrive as one `Moved` per
                // cell and only the last position matters. Everything else is
                // applied in arrival order.
                let mut queued = Some(event);
                let mut moved = None;
                while let Some(next) = queued {
                    if matches!(&next, AppEvent::Mouse(mouse) if mouse.kind == MouseEventKind::Moved) {
                        moved = Some(next);
                    } else {
                        app.handle(next);
                    }
                    queued = input_events.try_recv().ok();
                }
                if let Some(event) = moved {
                    app.handle(event);
                }
                terminal.set_mouse_capture(app.mouse_capture_enabled())?;
            },
            changed = task_updates.changed() => {
                if changed.is_ok() {
                    // The channel only wakes the loop; the snapshots are read
                    // from the engine. A `watch::Ref` would hold a read lock
                    // that a publishing worker needs while it holds the engine
                    // state mutex, and applying a snapshot reaches back for
                    // that mutex (`TaskEngine::operation`).
                    app.refresh_task_snapshots();
                }
            }
            Some(event) = background_events.recv() => {
                app.handle(event);
                terminal.set_mouse_capture(app.mouse_capture_enabled())?;
                app::drain_pending_events(&mut app, &mut background_events);
                terminal.set_mouse_capture(app.mouse_capture_enabled())?;
            }
            _ = redraw.tick() => {}
            else => break,
        }
        // The animated preset asks for a faster clock; switching presets in
        // the settings page must take effect without a restart.
        if app.refresh_millis() != tick_ms {
            tick_ms = app.refresh_millis();
            redraw = tokio::time::interval(std::time::Duration::from_millis(tick_ms));
            redraw.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        }
        if let Some(initial_directory) = app.take_directory_browse_request() {
            if let Some((reader_stop, reader_task)) = reader.take() {
                drop(reader_stop);
                reader_task
                    .await
                    .map_err(|error| Error::Terminal(format!("terminal event worker: {error}")))?
                    .map_err(Error::Terminal)?;
            }
            terminal.suspend()?;
            let selection = crate::platform::windows::pick_directory(&initial_directory);
            terminal.resume()?;
            match selection {
                Ok(Some(path)) => app.set_browsed_directory(path),
                Ok(None) => {}
                Err(error) => app.set_status(error.to_string()),
            }
            reader = Some(event::spawn_reader(input_sender.clone()));
        }
        if let Some((initial_directory, suggested_name)) = app.take_save_file_browse_request() {
            if let Some((reader_stop, reader_task)) = reader.take() {
                drop(reader_stop);
                reader_task
                    .await
                    .map_err(|error| Error::Terminal(format!("terminal event worker: {error}")))?
                    .map_err(Error::Terminal)?;
            }
            terminal.suspend()?;
            let selection =
                crate::platform::windows::pick_save_file(&initial_directory, &suggested_name);
            terminal.resume()?;
            match selection {
                Ok(Some(path)) => app.set_browsed_save_file(path),
                Ok(None) => app.cancel_pending_save_file(),
                Err(error) => {
                    app.cancel_pending_save_file();
                    app.set_status(error.to_string());
                }
            }
            reader = Some(event::spawn_reader(input_sender.clone()));
        }
        if let Some(initial_directory) = app.take_modpack_browse_request() {
            if let Some((reader_stop, reader_task)) = reader.take() {
                drop(reader_stop);
                reader_task
                    .await
                    .map_err(|error| Error::Terminal(format!("terminal event worker: {error}")))?
                    .map_err(Error::Terminal)?;
            }
            terminal.suspend()?;
            let selection = crate::platform::windows::pick_modpack_file(&initial_directory);
            terminal.resume()?;
            match selection {
                Ok(Some(path)) => app.set_browsed_modpack_file(path),
                Ok(None) => {}
                Err(error) => app.set_status(error.to_string()),
            }
            reader = Some(event::spawn_reader(input_sender.clone()));
        }
    }
    if let Some((reader_stop, reader_task)) = reader.take() {
        drop(reader_stop);
        reader_task
            .await
            .map_err(|error| Error::Terminal(format!("terminal event worker: {error}")))?
            .map_err(Error::Terminal)?;
    }
    Ok(())
}
