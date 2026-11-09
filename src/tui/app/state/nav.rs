//! Page order, tab labels and back navigation.

use super::*;

/// Tabs shown in the header, in display order.
pub(in crate::tui::app) const PAGES: [Page; 7] = Page::ALL;

pub(in crate::tui::app) fn tab_index(page: Page) -> Option<usize> {
    PAGES.iter().position(|candidate| *candidate == page)
}

impl App {
    pub(in crate::tui::app) fn tab_labels(&self, mode: LayoutMode) -> Vec<String> {
        let narrow = mode == LayoutMode::Narrow;
        PAGES
            .iter()
            .map(|page| {
                if narrow {
                    self.i18n.text(&format!("{}.short", page.title_key()))
                } else {
                    self.i18n.text(page.title_key())
                }
            })
            .collect()
    }

    pub(in crate::tui::app) fn mode(&self) -> Mode {
        // Same order as `top_layer` (its inverse, since that one walks from the
        // top down): a dialog owns the keyboard, then the help card, then the
        // diagnosis card. The two must agree about help and dialog; keeping them
        // apart only works while `?` cannot be pressed with a dialog open.
        if let Some(modal) = &self.modal {
            match modal {
                Modal::Confirm { action, .. } => Mode::Confirm(*action),
                Modal::Form { action, .. } | Modal::Prompt { action, .. } => Mode::Form(*action),
                Modal::Checklist { .. } => Mode::Filter,
            }
        } else if self.help_open {
            Mode::Help
        } else if self.diagnosis_open {
            Mode::Diagnosis
        } else if self.detail_open {
            Mode::Detail(self.detail_target())
        } else if let Some(state) = &self.search {
            // The workspace keeps a filter field of its own, so focus has to be
            // able to win over the page: with the workspace first the field
            // took the state but the keys went to the row cursor.
            Mode::Search(state.target)
        } else if self.workspace_is_open() {
            Mode::Workspace
        } else {
            Mode::Normal
        }
    }

    fn detail_target(&self) -> crate::tui::command::DetailTarget {
        match self.page {
            Page::Overview => crate::tui::command::DetailTarget::Overview,
            Page::Instances => crate::tui::command::DetailTarget::Instances,
            Page::Tasks => crate::tui::command::DetailTarget::Tasks,
            Page::Accounts => crate::tui::command::DetailTarget::Accounts,
            Page::DownloadCenter => crate::tui::command::DetailTarget::Download,
            Page::Logs | Page::Settings => crate::tui::command::DetailTarget::Overview,
        }
    }

    /// `Esc`: close the topmost layer, otherwise leave the page.
    pub(in crate::tui::app) fn go_back(&mut self) {
        if self.help_open {
            self.help_open = false;
            return;
        }
        if self.diagnosis_open {
            self.diagnosis_open = false;
            return;
        }
        if self.detail_open {
            self.detail_open = false;
            return;
        }
        if self.workspace_is_open() {
            // A world's data packs are a sub-view: Esc returns to the world
            // list before it closes the workspace.
            if self.leave_workspace_datapacks() {
                return;
            }
            self.close_workspace();
            return;
        }
        if self.page == Page::DownloadCenter && self.resource_detail.is_some() {
            self.resource_detail = None;
            self.resource_files.clear();
            self.resource_detail_project = None;
            self.download_center.selected = 0;
            self.reset_detail_scroll();
            return;
        }
        if false {
            return;
        }
        self.quit = true;
    }

    pub fn page(&self) -> Page {
        self.page
    }

    pub(in crate::tui::app) fn current_regions(&self) -> ChromeRegions {
        let size = (self.terminal_width, self.terminal_height);
        if self.regions_size.get() == size {
            self.regions.get()
        } else {
            self.layout_chrome(Rect::new(0, 0, size.0, size.1))
        }
    }

    pub fn selected(&self) -> usize {
        match self.page {
            Page::Instances => self.instances.focused,
            Page::Accounts => self.accounts.selected,
            Page::DownloadCenter => {
                if self.download_center.category == DownloadCategory::GameVersions {
                    self.version_selected
                } else {
                    self.download_center.selected
                }
            }
            Page::Tasks => self.task_selected,
            Page::Settings => {
                if self.settings.category == SettingsCategory::Java {
                    self.java_selected
                } else {
                    self.settings.selected
                }
            }
            Page::Overview | Page::Logs => 0,
        }
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    pub fn mouse_capture_enabled(&self) -> bool {
        self.config.state.ui.show_mouse
    }

    pub(in crate::tui::app) fn set_page(&mut self, page: Page) {
        if page != self.page {
            // An inline search field only exists on the page that drew it.
            self.search = None;
            self.search_backup = None;
            self.settings_search = None;
            // Hints differ per page and the creeper footer may need another
            // row for them, so the cached chrome no longer applies.
            self.regions_size.set((0, 0));
            // The workspace is a second-level screen of the instances page; it
            // must not keep answering keys once another page is showing.
            if page != Page::Instances {
                self.workspace = None;
            }
        }
        self.page = page;
        self.reset_detail_scroll();
        if page == Page::DownloadCenter
            && (self.download_center.category == DownloadCategory::GameVersions
                && self.versions.is_empty())
        {
            self.refresh_download_center();
        }
    }

    pub(in crate::tui::app) fn next_page(&mut self) {
        let index = tab_index(self.page).unwrap_or(0);
        self.set_page(PAGES[(index + 1) % PAGES.len()]);
    }

    pub(in crate::tui::app) fn previous_page(&mut self) {
        let index = tab_index(self.page).unwrap_or(0);
        self.set_page(PAGES[(index + PAGES.len() - 1) % PAGES.len()]);
    }
}

impl App {
    pub(in crate::tui::app) fn scroll_help(&mut self, delta: isize) {
        let next = if delta < 0 {
            self.help_scroll.saturating_sub(delta.unsigned_abs())
        } else {
            self.help_scroll.saturating_add(delta as usize)
        };
        // Without the clamp an overscroll accumulates behind the overlay: the
        // page stays at the end while the next scroll back seems to do nothing.
        self.help_scroll = next.min(self.help_max_scroll());
    }
}

#[cfg(test)]
mod tests {
    use super::{PAGES, tab_index};
    use crate::tui::app::{App, Page};
    use crate::tui::shell::LayoutMode;

    #[test]
    fn narrow_tabs_use_short_labels() {
        let app = App::new();
        let labels = app.tab_labels(LayoutMode::Narrow);
        assert_eq!(labels[0], "Home");
        assert_eq!(labels[1], "Inst");
        assert_eq!(labels.len(), 7);
    }

    #[test]
    fn pages_keep_the_tab_order() {
        assert_eq!(PAGES.len(), 7);
        assert_eq!(tab_index(Page::Overview), Some(0));
        assert_eq!(tab_index(Page::DownloadCenter), Some(2));
        assert_eq!(tab_index(Page::Settings), Some(6));
    }
}
