//! Modal keyboard and mouse handling.

use super::*;

impl Modal {
    pub fn handle_key(&mut self, key: KeyEvent) -> ModalResult {
        // Structural keys take the same path as the pointer: one implementation
        // in `apply`, so a key and a click can never disagree about the result.
        if let Some(act) = self.structural_key(key) {
            return self.apply(act);
        }
        match self {
            Self::Form {
                labels,
                values,
                cursors,
                focused,
                action,
                directory_field,
                file_field,
                selection_options,
                selection_states,
                selection_picker_open,
                selection_selected,
                selection_scroll,
                version_picker,
                ..
            } => {
                if let Some(field) = *selection_picker_open {
                    if let Some(picker) = version_picker
                        .as_mut()
                        .filter(|picker| picker.field == field)
                    {
                        if picker.editing_search {
                            match key.code {
                                KeyCode::Esc => {
                                    picker.filter.clear_search();
                                    picker.editing_search = false;
                                    picker.search_cursor = 0;
                                    *selection_selected = 0;
                                    *selection_scroll = 0;
                                }
                                KeyCode::Enter => picker.editing_search = false,
                                _ => {
                                    // Same editor as every other field, so the
                                    // cursor moves here too.
                                    let mut value = picker.filter.search().to_owned();
                                    let mut cursor = picker.search_cursor;
                                    if crate::tui::text::edit(&mut value, &mut cursor, key) {
                                        picker.filter.set_search(&value);
                                        picker.search_cursor = cursor;
                                        *selection_selected = 0;
                                        *selection_scroll = 0;
                                    }
                                }
                            }
                            return ModalResult::Continue;
                        }

                        if key.code == KeyCode::Char('/') {
                            picker.editing_search = true;
                            return ModalResult::Continue;
                        }
                        let category_index = match key.code {
                            KeyCode::Char('1') => Some(0),
                            KeyCode::Char('2') => Some(1),
                            KeyCode::Char('3') => Some(2),
                            KeyCode::Char('4') => Some(3),
                            _ => None,
                        };
                        if let Some(index) = category_index {
                            picker.toggle(VersionCategory::ALL[index]);
                            *selection_selected = 0;
                            *selection_scroll = 0;
                            return ModalResult::Continue;
                        }
                    }

                    let options = selection_picker_options(
                        field,
                        selection_options,
                        version_picker.as_deref(),
                    );
                    match key.code {
                        KeyCode::Esc => {
                            *selection_picker_open = None;
                            return ModalResult::Continue;
                        }
                        KeyCode::Char('r') | KeyCode::Char('R')
                            if matches!(
                                selection_states.get(&field),
                                Some(SelectionState::Error(_))
                            ) || (options.is_empty()
                                && !selection_states.contains_key(&field)
                                && !version_picker.as_ref().is_some_and(|picker| {
                                    picker.field == field && !picker.versions.is_empty()
                                })) =>
                        {
                            selection_states.insert(field, SelectionState::Loading);
                            return ModalResult::RetrySelectionPicker { field };
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            *selection_selected = selection_selected.saturating_sub(1);
                            keep_selection_visible(
                                *selection_selected,
                                options.len(),
                                selection_scroll,
                            );
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            *selection_selected = selection_selected
                                .saturating_add(1)
                                .min(options.len().saturating_sub(1));
                            keep_selection_visible(
                                *selection_selected,
                                options.len(),
                                selection_scroll,
                            );
                        }
                        KeyCode::PageUp => {
                            *selection_selected =
                                selection_selected.saturating_sub(PICKER_VISIBLE_ROWS);
                            keep_selection_visible(
                                *selection_selected,
                                options.len(),
                                selection_scroll,
                            );
                        }
                        KeyCode::PageDown => {
                            *selection_selected = selection_selected
                                .saturating_add(PICKER_VISIBLE_ROWS)
                                .min(options.len().saturating_sub(1));
                            keep_selection_visible(
                                *selection_selected,
                                options.len(),
                                selection_scroll,
                            );
                        }
                        KeyCode::Home => {
                            *selection_selected = 0;
                            *selection_scroll = 0;
                        }
                        KeyCode::End => {
                            *selection_selected = options.len().saturating_sub(1);
                            keep_selection_visible(
                                *selection_selected,
                                options.len(),
                                selection_scroll,
                            );
                        }
                        KeyCode::Enter => {
                            if let Some(option) = options.get(*selection_selected) {
                                let changed = values[field] != *option;
                                values[field] = option.clone();
                                *selection_picker_open = None;
                                if *focused == field && *focused + 1 < labels.len() {
                                    *focused += 1;
                                }
                                return if changed {
                                    ModalResult::SelectionChanged {
                                        field,
                                        value: option.clone(),
                                    }
                                } else {
                                    ModalResult::Continue
                                };
                            }
                        }
                        _ => {}
                    }
                    return ModalResult::Continue;
                }

                match key.code {
                    KeyCode::F(5) => ModalResult::Submit(*action, values.clone()),
                    KeyCode::Esc => ModalResult::Cancel,
                    KeyCode::F(2) if *directory_field == Some(*focused) => {
                        ModalResult::BrowseDirectory(
                            values
                                .get(*focused)
                                .filter(|value| !value.trim().is_empty())
                                .map(PathBuf::from)
                                .unwrap_or_default(),
                        )
                    }
                    KeyCode::F(2) if *file_field == Some(*focused) => ModalResult::BrowseFile(
                        values
                            .get(*focused)
                            .filter(|value| !value.trim().is_empty())
                            .map(PathBuf::from)
                            .unwrap_or_default(),
                    ),
                    KeyCode::Tab | KeyCode::Down => {
                        *focused = (*focused + 1) % values.len().max(1);
                        ModalResult::Continue
                    }
                    KeyCode::BackTab | KeyCode::Up => {
                        *focused = focused
                            .checked_sub(1)
                            .unwrap_or(values.len().saturating_sub(1));
                        ModalResult::Continue
                    }
                    // Text fields share the editor used by prompts and the
                    // inline search, so the arrow keys mean the same thing
                    // everywhere. Option fields are picked, never typed.
                    KeyCode::Left
                    | KeyCode::Right
                    | KeyCode::Home
                    | KeyCode::End
                    | KeyCode::Backspace
                    | KeyCode::Delete
                    | KeyCode::Char(_)
                        if !selection_options.contains_key(focused) =>
                    {
                        crate::tui::text::edit(&mut values[*focused], &mut cursors[*focused], key);
                        ModalResult::Continue
                    }
                    KeyCode::Enter if selection_options.contains_key(focused) => {
                        if selection_options.get(focused).is_none_or(Vec::is_empty) {
                            *selection_picker_open = Some(*focused);
                            *selection_selected = 0;
                            *selection_scroll = 0;
                            ModalResult::OpenSelectionPicker { field: *focused }
                        } else {
                            let options = selection_picker_options(
                                *focused,
                                selection_options,
                                version_picker.as_deref(),
                            );
                            *selection_selected = values
                                .get(*focused)
                                .and_then(|value| options.iter().position(|option| option == value))
                                .unwrap_or(0);
                            *selection_picker_open = Some(*focused);
                            keep_selection_visible(
                                *selection_selected,
                                options.len(),
                                selection_scroll,
                            );
                            ModalResult::Continue
                        }
                    }
                    _ => ModalResult::Continue,
                }
            }
            Self::Confirm { .. } => ModalResult::Continue,
            Self::Checklist { .. } => ModalResult::Continue,
            Self::Prompt { value, cursor, .. } => {
                // The prompt and every other field share one editor, so the
                // cursor keys behave the same wherever typing happens.
                crate::tui::text::edit(value, cursor, key);
                ModalResult::Continue
            }
        }
    }
    /// Keys whose only effect is structural.
    ///
    /// Option-list keys (Up/Down while a list is open, `/`, `1`-`4`, `r`) and
    /// text editing stay in the match above: they depend on the picker's
    /// internal state, not just on which dialog is open.
    fn structural_key(&self, key: KeyEvent) -> Option<ModalAct> {
        let ctrl = key
            .modifiers
            .contains(crossterm::event::KeyModifiers::CONTROL);
        match self {
            Self::Confirm { .. } => match key.code {
                KeyCode::Enter | KeyCode::Char('y') => Some(ModalAct::Confirm),
                KeyCode::Esc | KeyCode::Char('n') => Some(ModalAct::Reject),
                _ => None,
            },
            Self::Prompt { .. } => match key.code {
                KeyCode::Enter => Some(ModalAct::Submit),
                KeyCode::Esc => Some(ModalAct::Cancel),
                _ => None,
            },
            Self::Checklist { items, focused, .. } => match key.code {
                KeyCode::Esc => Some(ModalAct::Cancel),
                KeyCode::Enter => Some(ModalAct::ApplyChecks),
                KeyCode::Char('r') | KeyCode::Char('R') => Some(ModalAct::ResetChecks),
                KeyCode::Char(' ') => Some(ModalAct::CheckItem(*focused)),
                KeyCode::Up | KeyCode::Char('k') => {
                    Some(ModalAct::FocusCheck(focused.saturating_sub(1)))
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    Some(ModalAct::FocusCheck(focused.saturating_add(1)))
                }
                KeyCode::Home => Some(ModalAct::FocusCheck(0)),
                KeyCode::End => Some(ModalAct::FocusCheck(items.len().saturating_sub(1))),
                _ => None,
            },
            Self::Form {
                selection_picker_open: Some(_),
                ..
            } => None,
            Self::Form {
                labels,
                values,
                focused,
                directory_field,
                file_field,
                selection_options,
                ..
            } => match key.code {
                KeyCode::Enter if ctrl => Some(ModalAct::Submit),
                KeyCode::F(5) => Some(ModalAct::Submit),
                KeyCode::Esc => Some(ModalAct::Cancel),
                KeyCode::F(2) if *directory_field == Some(*focused) => {
                    Some(ModalAct::Browse(*focused))
                }
                KeyCode::F(2) if *file_field == Some(*focused) => {
                    Some(ModalAct::BrowseFile(*focused))
                }
                KeyCode::Tab | KeyCode::Down => {
                    Some(ModalAct::Focus((*focused + 1) % values.len().max(1)))
                }
                KeyCode::BackTab | KeyCode::Up => Some(ModalAct::Focus(
                    focused
                        .checked_sub(1)
                        .unwrap_or(values.len().saturating_sub(1)),
                )),
                // Opening the option list is the picker's own business.
                KeyCode::Enter if selection_options.contains_key(focused) => None,
                KeyCode::Enter if *focused + 1 < labels.len() => {
                    Some(ModalAct::Focus(*focused + 1))
                }
                KeyCode::Enter => Some(ModalAct::Submit),
                _ => None,
            },
        }
    }
}
