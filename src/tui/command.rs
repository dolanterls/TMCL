//! Key map: the single place where a key becomes an application command.
//!
//! The footer hints are generated from the per-page hint tables, and the
//! command handlers consume the variants below, so a key cannot be advertised
//! without being dispatchable.
//!
//! The command/keymap structure follows binsider `src/tui/command.rs`
//! (commit 7f0633d1, MIT OR Apache-2.0); the command set is TMCL-specific.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::app::Page;
use crate::tui::input::SearchTarget;
use crate::tui::modal::{ConfirmAction, FormAction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailTarget {
    Overview,
    Instances,
    Tasks,
    Accounts,
    Download,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    /// The instance workspace is a second-level screen; the category lives in
    /// `Workspace`, not here, so `Mode` can stay `Copy` and comparable.
    Workspace,
    Search(SearchTarget),
    Detail(DetailTarget),
    Form(FormAction),
    Confirm(ConfirmAction),
    /// A checklist dialog; its keys are handled by `Modal`, not by commands.
    Filter,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    NextTab,
    PrevTab,
    Back,
    Quit,
    Help,
    PrevRow,
    NextRow,
    PrevPage,
    NextPage,
    Top,
    Bottom,
    Activate,
    Launch,
    SetCurrent,
    Create,
    Import,
    /// Create an instance from a modpack archive picked from disk.
    ImportModpack,
    Rename,
    Delete,
    NewAccount,
    MicrosoftLogin,
    Search,
    Source,
    Sort,
    Target,
    /// Open the download-centre category filter.
    FilterCategories,
    PrevBlock,
    NextBlock,
    Refresh,
    /// Re-read the workspace category on screen, ignoring the scan cache.
    RescanWorkspace,
    ToggleVersionCategory(usize),
    Pause,
    Resume,
    Retry,
    ToggleFollow,
    FollowBottom,
    Clear,
    /// Reveal the workspace content directory in the file manager.
    OpenFolder,
    /// Write the launch command of the selected instance to a script.
    ExportScript,
    /// Open the data packs of the selected world.
    OpenDatapacks,
    /// Ask the mod source whether the installed mods have newer versions.
    CheckModUpdates,
    /// Update the open instance from a newer modpack archive.
    UpdateModpack,
    /// Drop every launch override of the instance workspace.
    ResetOverrides,
    /// Move the selected resource pack up (-1) or down (+1) the stack.
    ReorderContent(i8),
    /// Archive the selected world into the instance backup folder.
    BackupWorld,
    /// Pick one of the world backups and restore it.
    RestoreWorld,
    /// Duplicate the selected world.
    CloneWorld,
    ActivateSetting,
    AdjustSetting(isize),
    SettingAction(char),
    Input(char),
    Backspace,
    CursorLeft,
    CursorRight,
    CursorHome,
    CursorEnd,
    /// Forward delete inside an inline search field.
    DeleteForward,
    Submit,
    Cancel,
    Nothing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKeyState {
    Pause,
    Resume,
    Retry,
    None,
}

pub fn from_key(mode: Mode, page: Page, key: KeyEvent) -> Command {
    from_key_with_task(mode, page, TaskKeyState::None, key)
}

pub fn from_key_with_task(
    mode: Mode,
    page: Page,
    task_action: TaskKeyState,
    key: KeyEvent,
) -> Command {
    match mode {
        Mode::Help => match key.code {
            KeyCode::Esc
            | KeyCode::Char('q')
            | KeyCode::Char('?')
            | KeyCode::Char('？')
            | KeyCode::F(1) => Command::Back,
            // The reference is longer than the card, so the same keys that move
            // a list move the page here.
            KeyCode::Up | KeyCode::Char('k') => Command::PrevRow,
            KeyCode::Down | KeyCode::Char('j') => Command::NextRow,
            KeyCode::PageUp => Command::PrevPage,
            KeyCode::PageDown => Command::NextPage,
            KeyCode::Home => Command::Top,
            KeyCode::End => Command::Bottom,
            _ => Command::Nothing,
        },
        Mode::Form(_) | Mode::Confirm(_) | Mode::Filter => Command::Nothing,
        Mode::Workspace => match key.code {
            // `?` is a global key, not a page key: it has to work with the
            // workspace open too, or the help page looks broken exactly where
            // the key map is biggest.
            KeyCode::Char('?') | KeyCode::Char('？') | KeyCode::F(1) => Command::Help,
            // Some terminals report Shift+/ as `Char('/')` plus SHIFT instead
            // of `Char('?')`; without this the help key is unreachable there.
            KeyCode::Char('/') if key.modifiers.contains(KeyModifiers::SHIFT) => Command::Help,
            KeyCode::Esc => Command::Back,
            KeyCode::Up | KeyCode::Char('k') => Command::PrevRow,
            KeyCode::Down | KeyCode::Char('j') => Command::NextRow,
            KeyCode::Left => Command::AdjustSetting(-1),
            KeyCode::Right => Command::AdjustSetting(1),
            KeyCode::Char('[') | KeyCode::Char('h') => Command::PrevBlock,
            KeyCode::Char(']') | KeyCode::Char('l') => Command::NextBlock,
            KeyCode::Char(' ') => Command::ToggleFollow,
            KeyCode::Char('R') => Command::ResetOverrides,
            // Uppercase on purpose: lowercase j/k already move the cursor.
            KeyCode::Char('K') => Command::ReorderContent(-1),
            KeyCode::Char('J') => Command::ReorderContent(1),
            KeyCode::Char('b') => Command::BackupWorld,
            KeyCode::Char('r') => Command::RestoreWorld,
            KeyCode::Char('c') => Command::CloneWorld,
            // Uppercase, so it cannot collide with the lowercase row motion.
            KeyCode::Char('L') => Command::Launch,
            // Rename without leaving the workspace (design 6.3.1).
            KeyCode::Char('e') => Command::Rename,
            // r already means restore-world here, so refresh gets its own key.
            KeyCode::F(5) => Command::RescanWorkspace,
            KeyCode::Char('d') => Command::Delete,
            KeyCode::Char('i') => Command::Import,
            KeyCode::Char('o') => Command::OpenFolder,
            // `x` exports the launch command; it has no other job here.
            KeyCode::Char('x') => Command::ExportScript,
            // `p` opens the data packs of the selected world.
            KeyCode::Char('p') => Command::OpenDatapacks,
            // `u` checks the mods of this instance for updates.
            KeyCode::Char('u') => Command::CheckModUpdates,
            // Uppercase: `u` checks mods, `U` swaps the whole pack.
            KeyCode::Char('U') => Command::UpdateModpack,
            KeyCode::Char('/') => Command::Search,
            KeyCode::Char('s') => Command::Sort,
            KeyCode::PageUp => Command::PrevPage,
            KeyCode::PageDown => Command::NextPage,
            KeyCode::Home => Command::Top,
            KeyCode::End => Command::Bottom,
            KeyCode::Enter => Command::Activate,
            KeyCode::Char('q') if key.modifiers.is_empty() => Command::Quit,
            _ => Command::Nothing,
        },
        Mode::Detail(_) => match key.code {
            KeyCode::Char('?') | KeyCode::Char('？') | KeyCode::F(1) => Command::Help,
            KeyCode::Char('/') if key.modifiers.contains(KeyModifiers::SHIFT) => Command::Help,
            // Some terminals report Shift+/ as `Char('/')` plus SHIFT instead
            // of `Char('?')`; without this the help key is unreachable there.
            KeyCode::Char('/') if key.modifiers.contains(KeyModifiers::SHIFT) => Command::Help,
            KeyCode::Esc => Command::Back,
            KeyCode::Up | KeyCode::Char('k') => Command::PrevRow,
            KeyCode::Down | KeyCode::Char('j') => Command::NextRow,
            KeyCode::PageUp => Command::PrevPage,
            KeyCode::PageDown => Command::NextPage,
            KeyCode::Home => Command::Top,
            KeyCode::End => Command::Bottom,
            KeyCode::Enter => Command::Activate,
            KeyCode::Char('L') => Command::Launch,
            KeyCode::Char('q') if key.modifiers.is_empty() => Command::Quit,
            _ => Command::Nothing,
        },
        Mode::Search(target) => match key.code {
            KeyCode::Esc => Command::Cancel,
            KeyCode::Enter => Command::Submit,
            KeyCode::Backspace => Command::Backspace,
            KeyCode::Left => Command::CursorLeft,
            KeyCode::Right => Command::CursorRight,
            KeyCode::Delete => Command::DeleteForward,
            KeyCode::Home => Command::CursorHome,
            KeyCode::End => Command::CursorEnd,
            // The settings search is a result list, so vertical keys move the
            // highlight instead of being swallowed.
            KeyCode::Up | KeyCode::Char('k') if matches!(target, SearchTarget::Settings) => {
                Command::PrevRow
            }
            KeyCode::Down | KeyCode::Char('j') if matches!(target, SearchTarget::Settings) => {
                Command::NextRow
            }
            KeyCode::Char(character) if key.modifiers.is_empty() => Command::Input(character),
            _ => Command::Nothing,
        },
        Mode::Normal => global(page, task_action, key),
    }
}

fn global(page: Page, task_action: TaskKeyState, key: KeyEvent) -> Command {
    match key.code {
        KeyCode::Tab => Command::NextTab,
        KeyCode::BackTab => Command::PrevTab,
        KeyCode::Esc => Command::Back,
        KeyCode::Char('q') if key.modifiers.is_empty() => Command::Quit,
        // `？` is what a Chinese IME produces for Shift+/; `F1` cannot be
        // eaten by an IME or a keyboard layout at all; and some terminals send
        // `Char('/')` + SHIFT rather than `Char('?')`.
        KeyCode::Char('?') | KeyCode::Char('？') | KeyCode::F(1) => Command::Help,
        KeyCode::Char('/') if key.modifiers.contains(KeyModifiers::SHIFT) => Command::Help,
        KeyCode::Up | KeyCode::Char('k') => Command::PrevRow,
        KeyCode::Down | KeyCode::Char('j') => Command::NextRow,
        KeyCode::PageUp => Command::PrevPage,
        KeyCode::PageDown => Command::NextPage,
        KeyCode::Home => Command::Top,
        KeyCode::End => Command::Bottom,
        KeyCode::Char('t') if key.modifiers.is_empty() && page != Page::DownloadCenter => {
            Command::Top
        }
        KeyCode::Char('b') if key.modifiers.is_empty() => Command::Bottom,
        KeyCode::Enter => Command::Activate,
        _ => page_command(page, task_action, key),
    }
}

fn page_command(page: Page, task_action: TaskKeyState, key: KeyEvent) -> Command {
    match page {
        Page::Overview => crate::tui::app::state::commands::overview_command(Mode::Normal, key),
        Page::Instances => crate::tui::app::state::instances::command(Mode::Normal, key),
        Page::DownloadCenter => crate::tui::app::state::downloads::command(Mode::Normal, key),
        Page::Tasks => crate::tui::app::state::tasks::command(Mode::Normal, key, task_action),
        Page::Accounts => crate::tui::app::state::commands::accounts_command(Mode::Normal, key),
        Page::Logs => crate::tui::app::state::commands::logs_command(Mode::Normal, key),
        Page::Settings => crate::tui::app::state::settings::command(Mode::Normal, key),
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    use super::{Command, Mode, from_key};
    use crate::tui::app::Page;
    use crate::tui::input::SearchTarget;

    /// `?` has four spellings in the wild: the character itself, the full-width
    /// form a Chinese IME produces, `Shift+/` from terminals that do not
    /// translate it, and `F1` for when nothing else arrives. All of them have to
    /// open the help from every mode, and the plain `/` must stay search.
    #[test]
    fn the_help_key_is_reachable_from_every_spelling() {
        let shift_slash = KeyEvent {
            code: KeyCode::Char('/'),
            modifiers: KeyModifiers::SHIFT,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };
        for (label, event) in [
            ("?", key(KeyCode::Char('?'))),
            ("？", key(KeyCode::Char('？'))),
            ("F1", key(KeyCode::F(1))),
            ("shift+/", shift_slash),
        ] {
            for (mode, page) in [
                (Mode::Normal, Page::Overview),
                (Mode::Workspace, Page::Instances),
                (
                    Mode::Detail(crate::tui::command::DetailTarget::Instances),
                    Page::Instances,
                ),
            ] {
                assert_eq!(
                    from_key(mode, page, event),
                    Command::Help,
                    "{label} must open the help in {mode:?}"
                );
            }
        }

        // Plain `/` stays the search key on the pages that have one.
        assert_eq!(
            from_key(Mode::Workspace, Page::Instances, key(KeyCode::Char('/'))),
            Command::Search
        );
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn global_keys_map_to_commands() {
        assert_eq!(
            from_key(Mode::Normal, Page::Instances, key(KeyCode::Tab)),
            Command::NextTab
        );
        assert_eq!(
            from_key(Mode::Normal, Page::Instances, key(KeyCode::BackTab)),
            Command::PrevTab
        );
        assert_eq!(
            from_key(Mode::Normal, Page::Logs, key(KeyCode::Esc)),
            Command::Back
        );
        assert_eq!(
            from_key(Mode::Normal, Page::Tasks, key(KeyCode::Char('q'))),
            Command::Quit
        );
        assert_eq!(
            from_key(Mode::Normal, Page::Overview, key(KeyCode::Char('?'))),
            Command::Help
        );
    }

    #[test]
    fn page_keys_do_not_leak_across_pages() {
        assert_eq!(
            from_key(Mode::Normal, Page::Instances, key(KeyCode::Char('l'))),
            Command::Launch
        );
        assert_eq!(
            from_key(Mode::Normal, Page::Instances, key(KeyCode::Char('s'))),
            Command::Nothing
        );
        assert_eq!(
            from_key(Mode::Normal, Page::DownloadCenter, key(KeyCode::Char('s'))),
            Command::Source
        );
        assert_eq!(
            from_key(Mode::Normal, Page::Settings, key(KeyCode::Char('x'))),
            Command::SettingAction('x')
        );
        assert_eq!(
            from_key(Mode::Normal, Page::Instances, key(KeyCode::Char('z'))),
            Command::Nothing
        );
    }

    #[test]
    fn search_mode_only_produces_input_commands() {
        assert_eq!(
            from_key(
                Mode::Search(SearchTarget::VersionFilter),
                Page::DownloadCenter,
                key(KeyCode::Char('q')),
            ),
            Command::Input('q')
        );
        assert_eq!(
            from_key(
                Mode::Search(SearchTarget::VersionFilter),
                Page::DownloadCenter,
                key(KeyCode::Backspace),
            ),
            Command::Backspace
        );
        assert_eq!(
            from_key(
                Mode::Search(SearchTarget::VersionFilter),
                Page::DownloadCenter,
                key(KeyCode::Esc),
            ),
            Command::Cancel
        );
        assert_eq!(
            from_key(
                Mode::Search(SearchTarget::VersionFilter),
                Page::DownloadCenter,
                key(KeyCode::Down),
            ),
            Command::Nothing
        );
    }

    #[test]
    fn every_page_key_returns_its_command() {
        let cases: &[(Page, KeyCode, Command)] = &[
            (Page::Overview, KeyCode::Char('n'), Command::Create),
            (Page::Overview, KeyCode::Char('i'), Command::Import),
            (Page::Instances, KeyCode::Char('l'), Command::Launch),
            (Page::Instances, KeyCode::Char(' '), Command::SetCurrent),
            (Page::Instances, KeyCode::Char('n'), Command::Create),
            (Page::Instances, KeyCode::Char('i'), Command::Import),
            (Page::Instances, KeyCode::Char('r'), Command::Rename),
            (Page::Instances, KeyCode::Char('d'), Command::Delete),
            (Page::Instances, KeyCode::Char('x'), Command::ExportScript),
            (Page::Instances, KeyCode::Char('['), Command::PrevBlock),
            (Page::Instances, KeyCode::Char(']'), Command::NextBlock),
            (Page::Instances, KeyCode::Char('h'), Command::PrevBlock),
            (Page::Instances, KeyCode::Left, Command::PrevBlock),
            (Page::Instances, KeyCode::Right, Command::NextBlock),
            (Page::DownloadCenter, KeyCode::Char('/'), Command::Search),
            (Page::DownloadCenter, KeyCode::Char('s'), Command::Source),
            (Page::DownloadCenter, KeyCode::Char('o'), Command::Sort),
            (Page::DownloadCenter, KeyCode::Char('t'), Command::Target),
            (Page::DownloadCenter, KeyCode::Char('r'), Command::Refresh),
            (Page::DownloadCenter, KeyCode::Char('['), Command::PrevBlock),
            (Page::DownloadCenter, KeyCode::Char(']'), Command::NextBlock),
            (Page::DownloadCenter, KeyCode::Char('h'), Command::PrevBlock),
            (Page::DownloadCenter, KeyCode::Char('l'), Command::NextBlock),
            (
                Page::DownloadCenter,
                KeyCode::Char('1'),
                Command::ToggleVersionCategory(0),
            ),
            (
                Page::DownloadCenter,
                KeyCode::Char('2'),
                Command::ToggleVersionCategory(1),
            ),
            (
                Page::DownloadCenter,
                KeyCode::Char('3'),
                Command::ToggleVersionCategory(2),
            ),
            (
                Page::DownloadCenter,
                KeyCode::Char('4'),
                Command::ToggleVersionCategory(3),
            ),
            (Page::Tasks, KeyCode::Char('p'), Command::Pause),
            (Page::Tasks, KeyCode::Char('d'), Command::Delete),
            (Page::Tasks, KeyCode::Char('['), Command::PrevBlock),
            (Page::Tasks, KeyCode::Char(']'), Command::NextBlock),
            (Page::Tasks, KeyCode::Char('h'), Command::PrevBlock),
            (Page::Tasks, KeyCode::Char('l'), Command::NextBlock),
            (Page::Accounts, KeyCode::Char('n'), Command::NewAccount),
            (Page::Accounts, KeyCode::Char('m'), Command::MicrosoftLogin),
            (Page::Accounts, KeyCode::Char('d'), Command::Delete),
            (Page::Logs, KeyCode::Char('/'), Command::Search),
            (Page::Logs, KeyCode::Char('f'), Command::ToggleFollow),
            (Page::Logs, KeyCode::Char('g'), Command::FollowBottom),
            (Page::Logs, KeyCode::Char('c'), Command::Clear),
            (Page::Settings, KeyCode::Char('h'), Command::PrevBlock),
            (Page::Settings, KeyCode::Char('l'), Command::NextBlock),
            (Page::Settings, KeyCode::Left, Command::AdjustSetting(-1)),
            (Page::Settings, KeyCode::Right, Command::AdjustSetting(1)),
            (Page::Settings, KeyCode::Char('['), Command::PrevBlock),
            (Page::Settings, KeyCode::Char(']'), Command::NextBlock),
            (Page::Settings, KeyCode::Char(' '), Command::ActivateSetting),
            (Page::Settings, KeyCode::Char('/'), Command::Search),
            (
                Page::Settings,
                KeyCode::Char('a'),
                Command::SettingAction('a'),
            ),
            (
                Page::Settings,
                KeyCode::Char('i'),
                Command::SettingAction('i'),
            ),
            (
                Page::Settings,
                KeyCode::Char('r'),
                Command::SettingAction('r'),
            ),
            (
                Page::Settings,
                KeyCode::Char('s'),
                Command::SettingAction('s'),
            ),
            (
                Page::Settings,
                KeyCode::Char('x'),
                Command::SettingAction('x'),
            ),
        ];
        for (page, code, expected) in cases {
            assert_eq!(
                from_key(Mode::Normal, *page, key(*code)),
                *expected,
                "{page:?} {code:?}"
            );
        }
    }

    #[test]
    fn task_key_follows_the_selected_task_state() {
        use super::{TaskKeyState, from_key_with_task};

        let press = key(KeyCode::Char('p'));
        assert_eq!(
            from_key_with_task(Mode::Normal, Page::Tasks, TaskKeyState::Pause, press),
            Command::Pause
        );
        assert_eq!(
            from_key_with_task(Mode::Normal, Page::Tasks, TaskKeyState::Resume, press),
            Command::Resume
        );
        assert_eq!(
            from_key_with_task(Mode::Normal, Page::Tasks, TaskKeyState::Retry, press),
            Command::Retry
        );
        assert_eq!(
            from_key_with_task(Mode::Normal, Page::Tasks, TaskKeyState::None, press),
            Command::Pause
        );
    }

    #[test]
    fn help_mode_closes_on_escape_and_question_mark() {
        assert_eq!(
            from_key(Mode::Help, Page::Overview, key(KeyCode::Esc)),
            Command::Back
        );
        assert_eq!(
            from_key(Mode::Help, Page::Overview, key(KeyCode::Char('?'))),
            Command::Back
        );
    }
}
