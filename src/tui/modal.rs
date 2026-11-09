use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{Clear, Paragraph, Wrap},
};
use std::collections::HashMap;

use uuid::Uuid;

use crate::core::instance::path::display_path;
use crate::core::version::models::{VersionCategory, VersionFilter, VersionSummary};
use crate::i18n::I18n;
use crate::tui::action::{Action, ModalAct, ScrollTarget};
use crate::tui::hit::Hitmap;
use crate::tui::{theme::Theme, widgets::modal as modal_widgets};

/// Registers the click targets of a drawn key row.
///
/// The three preset renderers lay their caps out differently but mark the
/// same rectangles clickable, so this lives here rather than in one of them.
pub(super) fn register_caps(
    hits: &Hitmap,
    inner: Rect,
    line: usize,
    caps: &[(u16, u16)],
    acts: &[Option<ModalAct>],
) {
    for ((x, width), act) in caps.iter().zip(acts) {
        if let Some(act) = act {
            hits.add(
                Rect::new(
                    inner.x.saturating_add(*x),
                    inner
                        .y
                        .saturating_add(u16::try_from(line).unwrap_or(u16::MAX)),
                    *width,
                    1,
                ),
                Action::Modal(*act),
            );
        }
    }
}

const PICKER_VISIBLE_ROWS: usize = 12;
const CHECKLIST_VISIBLE_ROWS: usize = 12;

#[derive(Debug)]
pub struct VersionPickerState {
    field: usize,
    versions: Vec<VersionSummary>,
    filter: VersionFilter,
    editing_search: bool,
    /// Insertion point while the picker filter is being typed, in characters.
    search_cursor: usize,
}

impl VersionPickerState {
    fn new(field: usize, versions: Vec<VersionSummary>) -> Self {
        Self {
            field,
            versions,
            filter: VersionFilter::default(),
            editing_search: false,
            search_cursor: 0,
        }
    }

    fn counts(&self) -> [usize; 4] {
        self.filter.counts(&self.versions)
    }

    fn visible_options(&self) -> Vec<String> {
        self.filter
            .visible_indices(&self.versions)
            .into_iter()
            .filter_map(|index| self.versions.get(index).map(|version| version.id.clone()))
            .collect()
    }

    fn toggle(&mut self, category: VersionCategory) {
        self.filter.toggle(category);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionState {
    Loading,
    Error(String),
    Unavailable(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormAction {
    CreateInstance,
    SettingValue(crate::tui::pages::settings::SettingId),
    SearchLogs,
    SearchDownload,
    DownloadJavaRuntime,
    AddJavaRuntime,
    ImportInstance,
    RenameInstance,
    AddOfflineAccount,
    AddGameDirectory,
    SelectResourceTarget,
    InstallModpack,
    /// Create an instance from a modpack archive the player picked locally.
    ImportLocalModpack,
    InstallResource,
    /// A per-instance launch override edited from the instance workspace.
    LaunchOverride(crate::tui::pages::workspace::LaunchField),
    /// Import local files into a workspace content directory.
    ImportContent(crate::core::content::ContentKind),
    /// Install (or reinstall) one of the loaders on the install screen.
    InstallLoader(crate::core::instance::config::InstallableLoader),
    /// Restore a world from the backup chosen in the picker.
    RestoreWorld,
    /// Export the workspace instance as a modpack zip.
    ExportModpack,
    /// Update the workspace instance from a newer pack archive.
    UpdateModpack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmAction {
    /// Remove the instance with this id. The workspace opens its
    /// confirmation from a second-level screen, where \"the selected
    /// instance" may be a different row."
    RemoveInstance(Uuid),
    RemoveAccount,
    RemoveGameDirectory(usize),
    RemoveJavaRuntime(usize),
    ClearDownloadCache,
    /// Append this executable's directory to the user PATH.
    AddToPath,
    /// Empty the instance trash. Permanent, so it needs a confirmation.
    PurgeInstanceTrash(Uuid),
    /// The user asked to download the files an instance is missing after
    /// turning automatic repair off.
    RepairInstance(Uuid),
    /// Replace the loader of the instance the workspace is showing with the one
    /// waiting in `App::pending_loader_install`.
    SwitchLoader,
    /// Delete one content entry. The index points into the visible list that
    /// produced the prompt; a stale index is ignored at execution time.
    DeleteContent {
        instance: Uuid,
        kind: crate::core::content::ContentKind,
        index: usize,
    },
    /// Replace the outdated mods found by the last check.
    ApplyModUpdates,
}

/// One row of a [`Modal::Checklist`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecklistItem {
    pub id: String,
    pub label: String,
}

impl ChecklistItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }
}

#[derive(Debug)]
pub enum Modal {
    Form {
        title: String,
        labels: Vec<String>,
        values: Vec<String>,
        /// Insertion point per field, counted in characters. Kept beside
        /// `values` so every existing reader of the strings keeps working.
        cursors: Vec<usize>,
        focused: usize,
        action: FormAction,
        directory_field: Option<usize>,
        /// Field that addresses an existing file; `F2` opens a file picker.
        file_field: Option<usize>,
        selection_options: HashMap<usize, Vec<String>>,
        selection_states: Box<HashMap<usize, SelectionState>>,
        selection_picker_open: Option<usize>,
        selection_selected: usize,
        selection_scroll: usize,
        version_picker: Option<Box<VersionPickerState>>,
    },
    Confirm {
        prompt: String,
        action: ConfirmAction,
    },
    /// Single line text dialog: a framed input with a visible field background,
    /// used where an inline prompt would be too easy to miss.
    Prompt {
        title: String,
        label: String,
        help: String,
        value: String,
        cursor: usize,
        action: FormAction,
    },
    /// Selectable list used by the download filter: `single` picks one row,
    /// otherwise rows toggle independently. `note` explains loading/failure
    /// while the list is still empty. An item whose id is empty is the "all"
    /// row: choosing it clears every other row.
    Checklist {
        title: String,
        items: Vec<ChecklistItem>,
        checked: Vec<bool>,
        single: bool,
        focused: usize,
        scroll: usize,
        note: Option<String>,
        /// `R` asks the caller to reload instead of only clearing rows.
        retryable: bool,
    },
}

#[derive(Debug)]
pub enum ModalResult {
    Continue,
    Cancel,
    Submit(FormAction, Vec<String>),
    Confirm(ConfirmAction),
    BrowseDirectory(PathBuf),
    /// Pick an existing file for the form's file field.
    BrowseFile(PathBuf),
    OpenSelectionPicker {
        field: usize,
    },
    RetrySelectionPicker {
        field: usize,
    },
    SelectionChanged {
        field: usize,
        value: String,
    },
    /// The checklist was confirmed; the value is the selected item ids.
    FilterApply(Vec<String>),
    /// `R` on a checklist that can reload asked the caller to retry.
    FilterRetry,
    Reject,
}

mod classic;
mod creeper;
mod keys;
mod nightlight;
mod paper;
mod shelf;

impl Modal {
    pub fn form<T: AsRef<str>>(
        title: impl Into<String>,
        labels: &[T],
        values: &[String],
        action: FormAction,
    ) -> Self {
        Self::Form {
            title: title.into(),
            labels: labels
                .iter()
                .map(|label| label.as_ref().to_owned())
                .collect(),
            values: values.to_vec(),
            cursors: values.iter().map(|value| value.chars().count()).collect(),
            focused: 0,
            action,
            directory_field: None,
            file_field: None,
            selection_options: HashMap::new(),
            selection_states: Box::default(),
            selection_picker_open: None,
            selection_selected: 0,
            selection_scroll: 0,
            version_picker: None,
        }
    }

    pub fn with_file_picker(mut self, field: usize) -> Self {
        if let Self::Form { file_field, .. } = &mut self {
            *file_field = Some(field);
        }
        self
    }

    pub fn with_directory_picker(mut self, field: usize) -> Self {
        if let Self::Form {
            directory_field, ..
        } = &mut self
        {
            *directory_field = Some(field);
        }
        self
    }

    pub fn with_selection_picker(mut self, field: usize, options: Vec<String>) -> Self {
        if let Self::Form {
            selection_options, ..
        } = &mut self
        {
            selection_options.insert(field, options);
        }
        self
    }

    pub fn with_version_picker(mut self, field: usize, versions: Vec<VersionSummary>) -> Self {
        let options = versions.iter().map(|version| version.id.clone()).collect();
        if let Self::Form {
            selection_options,
            version_picker,
            ..
        } = &mut self
        {
            selection_options.insert(field, options);
            *version_picker = Some(Box::new(VersionPickerState::new(field, versions)));
        }
        self
    }

    pub fn set_version_picker_options(
        &mut self,
        field: usize,
        versions: Vec<VersionSummary>,
    ) -> bool {
        let Self::Form {
            selection_options,
            selection_states,
            selection_selected,
            selection_scroll,
            version_picker,
            ..
        } = self
        else {
            return false;
        };
        let Some(picker) = version_picker
            .as_mut()
            .filter(|picker| picker.field == field)
        else {
            return false;
        };
        selection_options.insert(
            field,
            versions.iter().map(|version| version.id.clone()).collect(),
        );
        picker.versions = versions;
        selection_states.remove(&field);
        *selection_selected = 0;
        *selection_scroll = 0;
        true
    }

    pub fn set_selection_options(&mut self, field: usize, options: Vec<String>) -> bool {
        let Self::Form {
            selection_options,
            selection_states,
            selection_selected,
            selection_scroll,
            ..
        } = self
        else {
            return false;
        };
        let Some(current) = selection_options.get_mut(&field) else {
            return false;
        };
        *current = options;
        selection_states.remove(&field);
        *selection_selected = (*selection_selected).min(current.len().saturating_sub(1));
        keep_selection_visible(*selection_selected, current.len(), selection_scroll);
        true
    }

    pub fn set_field_value(&mut self, field: usize, value: impl Into<String>) -> bool {
        let Self::Form {
            values, cursors, ..
        } = self
        else {
            return false;
        };
        let Some(current) = values.get_mut(field) else {
            return false;
        };
        *current = value.into();
        sync_cursor(cursors, field, current);
        true
    }

    pub fn open_selection_picker(&mut self, field: usize) -> bool {
        let Self::Form {
            focused,
            selection_options,
            selection_picker_open,
            selection_selected,
            selection_scroll,
            version_picker,
            ..
        } = self
        else {
            return false;
        };
        if !selection_options.contains_key(&field) {
            return false;
        }
        *focused = field;
        *selection_picker_open = Some(field);
        *selection_selected = 0;
        *selection_scroll = 0;
        if let Some(picker) = version_picker
            .as_mut()
            .filter(|picker| picker.field == field)
        {
            picker.editing_search = false;
        }
        true
    }

    pub fn set_selection_loading(&mut self, field: usize) -> bool {
        let Self::Form {
            selection_options,
            selection_states,
            ..
        } = self
        else {
            return false;
        };
        if !selection_options.contains_key(&field) {
            return false;
        }
        selection_states.insert(field, SelectionState::Loading);
        true
    }

    pub fn set_selection_error(&mut self, field: usize, message: impl Into<String>) -> bool {
        let Self::Form {
            selection_options,
            selection_states,
            ..
        } = self
        else {
            return false;
        };
        if !selection_options.contains_key(&field) {
            return false;
        }
        selection_states.insert(field, SelectionState::Error(message.into()));
        true
    }

    pub fn set_selection_unavailable(&mut self, field: usize, message: impl Into<String>) -> bool {
        let Self::Form {
            selection_options,
            selection_states,
            ..
        } = self
        else {
            return false;
        };
        if !selection_options.contains_key(&field) {
            return false;
        }
        selection_states.insert(field, SelectionState::Unavailable(message.into()));
        true
    }

    pub fn set_browsed_directory(&mut self, path: PathBuf) -> bool {
        let Self::Form {
            values,
            cursors,
            directory_field: Some(field),
            ..
        } = self
        else {
            return false;
        };
        let Some(value) = values.get_mut(*field) else {
            return false;
        };
        *value = display_path(&path);
        sync_cursor(cursors, *field, value);
        true
    }

    /// Write a path chosen outside the dialog into the file field.
    pub fn set_browsed_file(&mut self, path: PathBuf) -> bool {
        let Self::Form {
            values,
            cursors,
            file_field: Some(field),
            ..
        } = self
        else {
            return false;
        };
        let Some(value) = values.get_mut(*field) else {
            return false;
        };
        *value = display_path(&path);
        sync_cursor(cursors, *field, value);
        true
    }

    pub fn confirm(prompt: impl Into<String>, action: ConfirmAction) -> Self {
        Self::Confirm {
            prompt: prompt.into(),
            action,
        }
    }

    pub fn prompt(
        title: impl Into<String>,
        label: impl Into<String>,
        help: impl Into<String>,
        value: impl Into<String>,
        action: FormAction,
    ) -> Self {
        let value = value.into();
        let cursor = value.chars().count();
        Self::Prompt {
            title: title.into(),
            label: label.into(),
            help: help.into(),
            value,
            cursor,
            action,
        }
    }

    /// A checklist dialog; `selected` holds the ids that start checked.
    pub fn checklist(
        title: impl Into<String>,
        items: Vec<ChecklistItem>,
        selected: &[String],
        single: bool,
    ) -> Self {
        let checked = items
            .iter()
            .map(|item| selected.iter().any(|id| id == &item.id))
            .collect();
        Self::Checklist {
            title: title.into(),
            items,
            checked,
            single,
            focused: 0,
            scroll: 0,
            note: None,
            retryable: false,
        }
    }

    /// Adds the status line shown while a checklist has no rows yet.
    pub fn with_note(mut self, note: Option<String>) -> Self {
        if let Self::Checklist { note: slot, .. } = &mut self {
            *slot = note;
        }
        self
    }

    /// Makes `R` request a reload instead of only clearing the rows.
    pub fn with_retry(mut self, retryable: bool) -> Self {
        if let Self::Checklist {
            retryable: slot, ..
        } = &mut self
        {
            *slot = retryable;
        }
        self
    }

    pub fn prompt_value(&self) -> Option<&str> {
        match self {
            Self::Prompt { value, .. } => Some(value),
            _ => None,
        }
    }

    /// Runs one structural dialog action.
    ///
    /// Text editing (characters, cursor, backspace) stays in `handle_key`; this
    /// method covers the actions a pointer can express (focus a field, open a list,
    /// choose an option, browse, submit, cancel), so the mouse and the keyboard
    /// share one result vocabulary.
    pub fn apply(&mut self, act: ModalAct) -> ModalResult {
        match (self, act) {
            (
                Self::Form {
                    labels, focused, ..
                },
                ModalAct::Focus(field),
            ) => {
                if field < labels.len() {
                    *focused = field;
                }
                ModalResult::Continue
            }
            (
                Self::Form {
                    labels,
                    values,
                    focused,
                    selection_options,
                    selection_states,
                    selection_picker_open,
                    selection_selected,
                    selection_scroll,
                    version_picker,
                    ..
                },
                ModalAct::Toggle(field),
            ) => {
                if field >= labels.len() {
                    return ModalResult::Continue;
                }
                *focused = field;
                let has_options = selection_options.contains_key(&field)
                    || version_picker
                        .as_ref()
                        .is_some_and(|picker| picker.field == field);
                if !has_options {
                    return ModalResult::Continue;
                }
                let options =
                    selection_picker_options(field, selection_options, version_picker.as_deref());
                *selection_picker_open = Some(field);
                *selection_selected = values
                    .get(field)
                    .and_then(|value| options.iter().position(|option| option == value))
                    .unwrap_or(0);
                *selection_scroll = 0;
                keep_selection_visible(*selection_selected, options.len(), selection_scroll);
                if options.is_empty() {
                    selection_states.insert(field, SelectionState::Loading);
                    ModalResult::RetrySelectionPicker { field }
                } else {
                    ModalResult::Continue
                }
            }
            (
                Self::Form {
                    labels,
                    focused,
                    selection_picker_open,
                    selection_options,
                    selection_selected,
                    selection_scroll,
                    version_picker,
                    ..
                },
                ModalAct::ScrollOptions(delta),
            ) => {
                let Some(field) = *selection_picker_open else {
                    // No list open: the wheel walks the fields, like Tab / Down.
                    if labels.len() > 1 {
                        let last = labels.len() - 1;
                        *focused = if delta < 0 {
                            focused.saturating_sub(delta.unsigned_abs() as usize)
                        } else {
                            (*focused + delta as usize).min(last)
                        };
                    }
                    return ModalResult::Continue;
                };
                let options =
                    selection_picker_options(field, selection_options, version_picker.as_deref());
                let next = if delta < 0 {
                    selection_selected.saturating_sub(delta.unsigned_abs() as usize)
                } else {
                    selection_selected.saturating_add(delta as usize)
                };
                *selection_selected = next.min(options.len().saturating_sub(1));
                keep_selection_visible(*selection_selected, options.len(), selection_scroll);
                ModalResult::Continue
            }
            (
                Self::Form {
                    labels,
                    values,
                    cursors,
                    focused,
                    selection_options,
                    selection_picker_open,
                    version_picker,
                    ..
                },
                ModalAct::Option(index),
            ) => {
                let Some(field) = *selection_picker_open else {
                    return ModalResult::Continue;
                };
                let options =
                    selection_picker_options(field, selection_options, version_picker.as_deref());
                let Some(option) = options.get(index).cloned() else {
                    return ModalResult::Continue;
                };
                let changed = values.get(field) != Some(&option);
                if let Some(slot) = values.get_mut(field) {
                    *slot = option.clone();
                }
                if let Some(cursor) = cursors.get_mut(field) {
                    *cursor = option.chars().count();
                }
                *selection_picker_open = None;
                if *focused == field && *focused + 1 < labels.len() {
                    *focused += 1;
                }
                if changed {
                    ModalResult::SelectionChanged {
                        field,
                        value: option,
                    }
                } else {
                    ModalResult::Continue
                }
            }
            (
                Self::Form {
                    values,
                    directory_field,
                    ..
                },
                ModalAct::Browse(field),
            ) => {
                if *directory_field != Some(field) {
                    return ModalResult::Continue;
                }
                ModalResult::BrowseDirectory(
                    values
                        .get(field)
                        .filter(|value| !value.trim().is_empty())
                        .map(PathBuf::from)
                        .unwrap_or_default(),
                )
            }
            (
                Self::Form {
                    values, file_field, ..
                },
                ModalAct::BrowseFile(field),
            ) => {
                if *file_field != Some(field) {
                    return ModalResult::Continue;
                }
                ModalResult::BrowseFile(
                    values
                        .get(field)
                        .filter(|value| !value.trim().is_empty())
                        .map(PathBuf::from)
                        .unwrap_or_default(),
                )
            }
            (
                Self::Form {
                    selection_picker_open: Some(field),
                    version_picker: Some(picker),
                    selection_selected,
                    selection_scroll,
                    ..
                },
                ModalAct::ToggleVersionFilter(index),
            ) if picker.field == *field => {
                if let Some(category) = VersionCategory::ALL.get(index).copied() {
                    picker.toggle(category);
                    *selection_selected = 0;
                    *selection_scroll = 0;
                }
                ModalResult::Continue
            }
            (
                Self::Form {
                    selection_picker_open: Some(field),
                    version_picker: Some(picker),
                    ..
                },
                ModalAct::EditSearch,
            ) if picker.field == *field => {
                picker.editing_search = true;
                picker.search_cursor = picker.filter.search().chars().count();
                ModalResult::Continue
            }
            (Self::Form { action, values, .. }, ModalAct::Submit) => {
                ModalResult::Submit(*action, values.clone())
            }
            (
                Self::Form {
                    selection_picker_open,
                    ..
                },
                ModalAct::CloseOptions,
            ) => {
                *selection_picker_open = None;
                ModalResult::Continue
            }
            (
                Self::Form {
                    selection_picker_open: Some(field),
                    selection_states,
                    ..
                },
                ModalAct::RetryOptions,
            ) => {
                selection_states.insert(*field, SelectionState::Loading);
                ModalResult::RetrySelectionPicker { field: *field }
            }
            (
                Self::Checklist {
                    items,
                    checked,
                    focused,
                    single,
                    ..
                },
                ModalAct::CheckItem(index),
            ) => {
                if index >= items.len() {
                    return ModalResult::Continue;
                }
                *focused = index;
                let all_row = items[index].id.is_empty();
                if all_row {
                    // The "all" row is exclusive: it clears every other row.
                    checked.iter_mut().for_each(|checked| *checked = false);
                    checked[index] = true;
                } else if *single {
                    checked.iter_mut().for_each(|checked| *checked = false);
                    checked[index] = true;
                    if let Some(all) = items.iter().position(|item| item.id.is_empty()) {
                        checked[all] = false;
                    }
                } else {
                    checked[index] = !checked[index];
                    if checked[index]
                        && let Some(all) = items.iter().position(|item| item.id.is_empty())
                    {
                        checked[all] = false;
                    }
                }
                ModalResult::Continue
            }
            (
                Self::Checklist {
                    items,
                    focused,
                    scroll,
                    ..
                },
                ModalAct::FocusCheck(index),
            ) => {
                if items.is_empty() {
                    return ModalResult::Continue;
                }
                *focused = index.min(items.len() - 1);
                keep_index_visible(*focused, items.len(), CHECKLIST_VISIBLE_ROWS, scroll);
                ModalResult::Continue
            }
            (
                Self::Checklist {
                    items,
                    focused,
                    scroll,
                    ..
                },
                ModalAct::ScrollOptions(delta),
            ) => {
                if items.is_empty() {
                    return ModalResult::Continue;
                }
                let next = if delta < 0 {
                    focused.saturating_sub(delta.unsigned_abs() as usize)
                } else {
                    focused.saturating_add(delta as usize).min(items.len() - 1)
                };
                *focused = next;
                keep_index_visible(next, items.len(), CHECKLIST_VISIBLE_ROWS, scroll);
                ModalResult::Continue
            }
            (Self::Checklist { items, checked, .. }, ModalAct::ApplyChecks) => {
                let selected = items
                    .iter()
                    .zip(checked.iter())
                    .filter(|(item, checked)| **checked && !item.id.is_empty())
                    .map(|(item, _)| item.id.clone())
                    .collect();
                ModalResult::FilterApply(selected)
            }
            (
                Self::Checklist {
                    checked,
                    focused,
                    scroll,
                    retryable,
                    ..
                },
                ModalAct::ResetChecks,
            ) => {
                checked.iter_mut().for_each(|checked| *checked = false);
                *focused = 0;
                *scroll = 0;
                if *retryable {
                    ModalResult::FilterRetry
                } else {
                    ModalResult::Continue
                }
            }
            (Self::Checklist { .. }, ModalAct::Cancel) => ModalResult::Cancel,
            (Self::Form { .. }, ModalAct::Cancel) => ModalResult::Cancel,
            (Self::Prompt { action, value, .. }, ModalAct::Submit) => {
                ModalResult::Submit(*action, vec![value.clone()])
            }
            (Self::Prompt { .. }, ModalAct::Cancel) => ModalResult::Cancel,
            (Self::Confirm { action, .. }, ModalAct::Confirm) => ModalResult::Confirm(*action),
            (Self::Confirm { .. }, ModalAct::Reject) => ModalResult::Reject,
            _ => ModalResult::Continue,
        }
    }
    pub fn area(&self, terminal_area: Rect) -> Rect {
        match self {
            Self::Prompt { .. } => crate::tui::chrome::prompt_rect(terminal_area),
            Self::Confirm { prompt, .. } => {
                // The prompt wraps instead of being cut, and the card has to
                // grow with it: a destructive dialog that truncates hides the
                // object it is about to change, which is the one thing the
                // reader came for.
                let width = terminal_area.width.saturating_sub(4).clamp(20, 76);
                let rows = confirm_prompt_rows(prompt, width.saturating_sub(2));
                let height = u16::try_from(rows).unwrap_or(1).saturating_add(6);
                crate::tui::chrome::popup_rect_sized(terminal_area, height)
            }
            Self::Form {
                labels,
                selection_picker_open,
                ..
            } => {
                // A picker needs the full height for its option list; a plain
                // form only needs one row per field plus the key hints.
                let height = if selection_picker_open.is_some() {
                    16
                } else {
                    u16::try_from(labels.len()).unwrap_or(16).saturating_add(6)
                };
                crate::tui::chrome::popup_rect_sized(terminal_area, height)
            }
            Self::Checklist { items, note, .. } => {
                let visible = items.len().min(CHECKLIST_VISIBLE_ROWS);
                let note_rows = u16::from(note.is_some());
                let height = u16::try_from(visible)
                    .unwrap_or(10)
                    .saturating_add(4)
                    .saturating_add(note_rows);
                crate::tui::chrome::popup_rect_sized(terminal_area, height)
            }
        }
    }
}

fn keep_index_visible(index: usize, count: usize, visible: usize, scroll: &mut usize) {
    if count == 0 || visible == 0 {
        *scroll = 0;
        return;
    }
    if index < *scroll {
        *scroll = index;
    } else if index >= scroll.saturating_add(visible) {
        *scroll = index + 1 - visible;
    }
    *scroll = (*scroll).min(count.saturating_sub(visible));
}

/// Keeps a field's insertion point at the end after an outside writer replaced
/// its value (file picker, directory picker, preset choice).
fn sync_cursor(cursors: &mut [usize], field: usize, value: &str) {
    if let Some(cursor) = cursors.get_mut(field) {
        *cursor = value.chars().count();
    }
}

fn keep_selection_visible(selected: usize, option_count: usize, scroll: &mut usize) {
    keep_index_visible(selected, option_count, PICKER_VISIBLE_ROWS, scroll);
}

/// Rows a confirm prompt takes at `width` cells.
///
/// Measured with the `! ` badge two presets print in front of it, so a card sized
/// from this fits whichever renderer draws it; the plain prompt can only be
/// shorter.
fn confirm_prompt_rows(prompt: &str, width: u16) -> usize {
    crate::tui::palette::wrap(&format!("! {prompt}"), usize::from(width).max(1))
        .len()
        .max(1)
}

fn selection_display(value: &str) -> &str {
    value.split_once('|').map_or(value, |(display, _)| display)
}

fn selection_picker_options(
    field: usize,
    selection_options: &HashMap<usize, Vec<String>>,
    version_picker: Option<&VersionPickerState>,
) -> Vec<String> {
    version_picker
        .filter(|picker| picker.field == field)
        .map(VersionPickerState::visible_options)
        .or_else(|| selection_options.get(&field).cloned())
        .unwrap_or_default()
}

/// One cap in a picker's key row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct KeyHint {
    /// Printed inside the cap, exactly as the keyboard spells it.
    pub key: &'static str,
    /// i18n key of what the cap does.
    pub label: &'static str,
    /// What a click on the cap does.
    ///
    /// `None` where the pointer has no equivalent: moving the highlight, paging,
    /// typing. Those caps stay printed because the keyboard can do them, but they
    /// are not drawn as buttons.
    pub act: Option<ModalAct>,
}

impl KeyHint {
    const fn new(key: &'static str, label: &'static str, act: Option<ModalAct>) -> Self {
        Self { key, label, act }
    }
}

/// The caps a picker's foot shows, for the state the list is in.
///
/// A sentence here is not a key row: `Esc` then closes the list from the keyboard
/// and does nothing when clicked, while the same cap on a form row works. One
/// builder decides which caps exist, and each preset paints the same list in its
/// own colours.
pub(super) fn picker_key_hints(
    version_picker: Option<&VersionPickerState>,
    state: Option<&SelectionState>,
    selected: usize,
    options_empty: bool,
) -> Vec<KeyHint> {
    // `Esc` here backs out of the list and keeps the form: `CloseOptions`, not the
    // dialog's `Cancel`.
    let back = KeyHint::new("Esc", "action.back", Some(ModalAct::CloseOptions));
    if version_picker.is_some_and(|picker| picker.editing_search) {
        // The query owns the keyboard: `Enter` ends typing, `Esc` clears it.
        return vec![
            KeyHint::new("Backspace", "hint.erase", None),
            KeyHint::new("Enter", "hint.done", None),
            KeyHint::new("Esc", "hint.clear", None),
        ];
    }
    let has_filters = version_picker.is_some();
    let filters = KeyHint::new("1-4", "action.filter", None);
    let search = KeyHint::new("/", "action.search", Some(ModalAct::EditSearch));
    let choose = KeyHint::new("Enter", "hint.confirm", Some(ModalAct::Option(selected)));
    let retry = KeyHint::new("r", "action.retry", Some(ModalAct::RetryOptions));
    match state {
        Some(SelectionState::Error(_)) if has_filters => vec![retry, filters, search, back],
        Some(SelectionState::Error(_)) => vec![retry, back],
        // Nothing to retry into: the list is on its way in, and `Esc` is the
        // only key that does anything.
        Some(SelectionState::Loading | SelectionState::Unavailable(_)) => vec![back],
        _ if has_filters => vec![
            filters,
            search,
            KeyHint::new("↑/↓", "hint.move", None),
            choose,
            back,
        ],
        // Never loaded: `r` is what the keyboard handler retries on, and the
        // label has to say so rather than promise a choice from an empty list.
        _ if options_empty => vec![
            KeyHint::new("r", "hint.load_retry", Some(ModalAct::RetryOptions)),
            back,
        ],
        _ => vec![
            KeyHint::new("↑/↓", "action.select", None),
            KeyHint::new("PgUp/PgDn", "hint.page", None),
            choose,
            back,
        ],
    }
}

/// `(keys, acts)`: the pair every preset's key-row painter already takes.
pub(super) fn picker_key_row(
    hints: &[KeyHint],
) -> (Vec<(&'static str, &'static str)>, Vec<Option<ModalAct>>) {
    hints
        .iter()
        .map(|hint| ((hint.key, hint.label), hint.act))
        .unzip()
}

#[cfg(test)]
mod tests;
