//! Mouse intent dispatch.
//!
//! `mouse.rs` turns a hit into an [`Action`]; this module is the only place an
//! action changes state. Page actions land here, the dialog arms in P4 — the
//! match below grows with them.

use std::time::{Duration, Instant};

use super::*;
use crate::tui::action::{Action, Click, Metric, ModalAct, OverlayAct, PageAct, ScrollTarget};
use crate::tui::hit::Layer;

/// List discriminator for double-click pairing (see `App::click_context`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::tui::app) struct ClickContext {
    page: Page,
    settings_category: SettingsCategory,
    download_category: DownloadCategory,
    instance_filter: usize,
    task_filter: TaskFilter,
    resource_detail: bool,
    /// Instance and category the workspace is showing, so `Row(0)` of two
    /// different categories never pair up into a double click.
    workspace: Option<(uuid::Uuid, crate::tui::pages::workspace::WorkspaceCategory)>,
}

/// Two clicks on the same target within this window are a double click.
const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(400);

impl App {
    /// Topmost layer that currently owns input.
    ///
    /// Follows the draw order (`detail` → `help` → `modal`) rather than `Mode`:
    /// an inline search is page content, so it must not block the page.
    pub(in crate::tui::app) fn top_layer(&self) -> Layer {
        if self.modal.is_some() {
            Layer::Modal
        } else if self.help_open {
            Layer::Help
        } else if self.detail_open {
            Layer::Detail
        } else {
            Layer::Base
        }
    }

    /// Applies one hit. `click` tells single from double; only row activation
    /// distinguishes them.
    pub(in crate::tui::app) fn apply_action(&mut self, action: Action, click: Click) {
        match (action, click) {
            (Action::Nav(page), _) => self.set_page(page),
            (Action::Overlay(OverlayAct::Close), _) => self.go_back(),
            (Action::Overlay(OverlayAct::Noop), _) => {}
            (Action::Page { page, act }, click) => {
                // A hit from the previous frame can name a page the app already
                // left; never run a page action against another page's state.
                if page == self.page {
                    self.apply_page(page, act, click);
                }
            }
            (Action::Modal(act), _) => self.apply_modal(act),
        }
    }

    /// Runs one page-level intent.
    fn apply_page(&mut self, page: Page, act: PageAct, click: Click) {
        match act {
            // Double click runs the row's `Enter` behaviour; a single click only
            // selects, so there is never a timer between the two.
            PageAct::Row(index) if click == Click::Double => self.activate_row(page, index),
            PageAct::Row(index) => self.select_row(page, index),
            // P5 registers `Activate` for double clicks; the handler already
            // matches the keyboard `Enter` path.
            PageAct::Activate(index) => self.activate_row(page, index),
            PageAct::Filter(index) => self.select_filter(page, index),
            PageAct::VersionFilter(index) => self.toggle_version_category(index),
            PageAct::Category(index) => self.select_settings_category(index),
            PageAct::Step { row, by } => self.step_setting(page, row, by),
            PageAct::Metric(metric) => self.goto_metric(metric),
            PageAct::Launch => self.overview_launch(),
            PageAct::Activity => self.set_page(Page::Logs),
            PageAct::Search(target) => self.focus_search_target(target),
            PageAct::FilterCategories => self.open_filter_dialog(),
            PageAct::WorkspaceCategory(index) => self.select_workspace_category(index),
            PageAct::WorkspaceCategoryBy(by) => self.move_workspace_category(isize::from(by)),
            // A single click only moves the cursor; the double click runs the
            // same code the Enter key does, so the two never diverge.
            PageAct::WorkspaceRow(index) => {
                self.set_workspace_selection(index);
                if click == Click::Double {
                    self.activate_workspace_row();
                }
            }
            PageAct::WorkspaceRun(index) => {
                self.set_workspace_selection(index);
                self.activate_workspace_row();
            }
            PageAct::WorkspaceToggle(index) => {
                self.set_workspace_selection(index);
                self.toggle_workspace_current();
            }
            PageAct::WorkspaceStep { row, by } => {
                self.set_workspace_selection(row);
                self.step_workspace_value(isize::from(by));
            }
        }
    }

    /// Selects one row of the current page's list.
    ///
    /// This is the single implementation shared with the keyboard: `↑/↓` walks
    /// this method through the same primitives, so a click and a keypress can
    /// not disagree about what "selected" means.
    pub(in crate::tui::app) fn select_row(&mut self, page: Page, index: usize) {
        match page {
            Page::Overview => {
                if index < self.instances.items.len() {
                    self.instances.focused = index;
                }
            }
            Page::Instances => {
                let visible = self.visible_instance_indices();
                if let Some(actual) = visible.get(index) {
                    self.instances.focused = *actual;
                    self.pane_scroll = 0;
                }
            }
            Page::Accounts => {
                if index < self.accounts.accounts.len() {
                    self.accounts.selected = index;
                }
            }
            Page::Tasks => {
                if index < self.visible_task_indices().len() {
                    self.set_task_selection(index);
                }
            }
            Page::DownloadCenter => {
                if self.download_center.category == DownloadCategory::GameVersions {
                    if index < self.filtered_version_indices().len() {
                        self.version_selected = index;
                        self.reset_detail_scroll();
                        self.keep_version_selected_visible();
                    }
                } else {
                    let count = if self.resource_detail.is_some() {
                        self.resource_files.len()
                    } else {
                        self.resource_projects.len()
                    };
                    if index < count {
                        self.download_center.selected = index;
                        self.reset_detail_scroll();
                        self.pane_scroll = 0;
                    }
                }
            }
            Page::Settings => match self.settings.category {
                SettingsCategory::Java => {
                    if index < self.java.len() {
                        self.java_selected = index;
                    }
                }
                _ => {
                    if index < self.settings_row_count() {
                        self.settings.selected = index;
                    }
                }
            },
            Page::Logs => {}
        }
    }

    /// Selects the row and then runs the page's `Enter` behaviour.
    pub(in crate::tui::app) fn activate_row(&mut self, page: Page, index: usize) {
        self.select_row(page, index);
        if page == self.page {
            self.activate_selection();
        }
    }

    fn select_filter(&mut self, page: Page, index: usize) {
        match page {
            Page::Instances => self.set_instance_filter(index),
            Page::Tasks => {
                if let Some(filter) = TaskFilter::ALL.get(index) {
                    self.task_filter = *filter;
                    self.change_task_filter(0);
                }
            }
            Page::DownloadCenter => {
                if index >= DownloadCategory::ALL.len() {
                    return;
                }
                let current = DownloadCategory::ALL
                    .iter()
                    .position(|category| category == &self.download_center.category)
                    .unwrap_or(0);
                let delta = index as isize - current as isize;
                if delta != 0 {
                    self.change_download_category(delta);
                }
            }
            _ => {}
        }
    }

    fn select_settings_category(&mut self, index: usize) {
        if index >= SettingsCategory::ALL.len() {
            return;
        }
        self.settings
            .select_category(SettingsCategory::from_index(index));
        self.settings_search = None;
        self.search = None;
        self.settings.clamp_selection(self.settings_row_count());
        self.pane_scroll = 0;
    }

    fn step_setting(&mut self, page: Page, row: usize, by: i8) {
        if page != Page::Settings {
            return;
        }
        self.select_row(page, row);
        self.adjust_current_setting(isize::from(by));
    }

    fn goto_metric(&mut self, metric: Metric) {
        match metric {
            Metric::Instances => self.set_page(Page::Instances),
            Metric::Tasks => self.set_page(Page::Tasks),
            Metric::Accounts => self.set_page(Page::Accounts),
            Metric::Java => {
                self.settings.select_category(SettingsCategory::Java);
                self.settings.clamp_selection(self.settings_row_count());
                self.set_page(Page::Settings);
            }
        }
    }

    /// The overview play card: create an instance, ask for an account, or
    /// launch — mirroring what the card says.
    fn overview_launch(&mut self) {
        if self.instances.items.is_empty() {
            self.set_page(Page::Instances);
        } else if self.current_account().is_none() {
            self.set_page(Page::Accounts);
        } else {
            self.launch_current_instance();
        }
    }

    /// Focuses the inline search field the page actually draws. A hit from a
    /// page that has since changed never starts a search elsewhere.
    fn focus_search_target(&mut self, target: SearchTarget) {
        let expected = match self.page {
            Page::DownloadCenter
                if self.download_center.category == DownloadCategory::GameVersions =>
            {
                SearchTarget::VersionFilter
            }
            Page::DownloadCenter => SearchTarget::Resources,
            Page::Settings => SearchTarget::Settings,
            Page::Logs => SearchTarget::Logs,
            // The instance pages draw a filter field on their content
            // categories; anywhere else there is nothing to focus.
            Page::Instances if self.workspace_has_content_category() => SearchTarget::Workspace,
            _ => return,
        };
        if target == expected {
            self.start_search();
        }
    }

    /// Scrolls the region under the pointer. Each target keeps its own step
    /// size; nothing here guesses what the user meant.
    pub(in crate::tui::app) fn apply_scroll(&mut self, target: ScrollTarget, up: bool) {
        let rows = |delta: isize| if up { -delta } else { delta };
        match target {
            ScrollTarget::Rows(kind) => {
                if kind.page() == self.page && !self.detail_open {
                    // One row per notch, exactly like the Up/Down keys.
                    self.move_selection(rows(1));
                }
            }
            ScrollTarget::WorkspaceRows => {
                if self.workspace_is_open() && !self.detail_open {
                    self.move_workspace_row(rows(1));
                }
            }
            ScrollTarget::LogLines => {
                let max = self.log_max_scroll_offset();
                self.logs.scroll(rows(3), max);
            }
            ScrollTarget::Pane => self.scroll_detail_pane(rows(2)),
            ScrollTarget::DetailBody => self.move_detail_scroll(rows(2)),
            ScrollTarget::HelpBody => {
                // Same clamp as the arrow keys, so the wheel cannot leave the
                // offset past the last row either.
                self.scroll_help(if up { -3 } else { 3 });
            }
            ScrollTarget::Blocked => {}
            // Dialogs register their own option list in P4.
            ScrollTarget::ModalOptions => {
                self.apply_modal(ModalAct::ScrollOptions(if up { -1 } else { 1 }));
            }
        }
    }

    /// Applies one dialog action and settles the resulting state change through
    /// the same handler the keyboard uses.
    pub(in crate::tui::app) fn apply_modal(&mut self, act: ModalAct) {
        let result = match self.modal.as_mut() {
            Some(modal) => modal.apply(act),
            None => return,
        };
        self.settle_modal_result(result);
    }

    /// What makes two clicks "the same target".
    ///
    /// The action alone is not enough: `Row(0)` of the Java list and `Row(0)` of
    /// the game-directory list are the same value, yet they are different rows.
    /// Two clicks only pair up inside the same page *and* the same list surface.
    pub(in crate::tui::app) fn click_context(&self) -> ClickContext {
        ClickContext {
            page: self.page,
            settings_category: self.settings.category,
            download_category: self.download_center.category,
            instance_filter: self.instance_filter,
            task_filter: self.task_filter,
            resource_detail: self.resource_detail.is_some(),
            workspace: self
                .workspace
                .as_ref()
                .map(|workspace| (workspace.instance, workspace.category)),
        }
    }

    /// Whether `action` points at the row that is currently selected.
    ///
    /// Hover must not repaint the selection: §8.5 gives the selected style
    /// priority over the pointer tint, so `paint_hover` asks this first.
    pub(in crate::tui::app) fn is_selected_target(&self, action: Action) -> bool {
        let Action::Page { page, act } = action else {
            return false;
        };
        // The workspace lives on `Page::Instances`; its rows are a different
        // list from the instance table behind it, so they answer themselves.
        if let crate::tui::action::PageAct::WorkspaceRow(index) = act {
            return self
                .workspace
                .as_ref()
                .is_some_and(|workspace| workspace.selected == index);
        }
        let crate::tui::action::PageAct::Row(index) = act else {
            return false;
        };
        match page {
            Page::Overview => self.instances.focused == index,
            Page::Instances => {
                self.visible_instance_indices()
                    .iter()
                    .position(|entry| *entry == self.instances.focused)
                    == Some(index)
            }
            Page::Accounts => self.accounts.selected == index,
            Page::Tasks => self.task_selected == index,
            Page::DownloadCenter => {
                if self.download_center.category == DownloadCategory::GameVersions {
                    self.version_selected == index
                } else {
                    self.download_center.selected == index
                }
            }
            Page::Settings => match self.settings.category {
                SettingsCategory::Java => self.java_selected == index,
                _ => self.settings.selected == index,
            },
            Page::Logs => false,
        }
    }

    /// Records a click and reports whether it completed a double click on the
    /// same target. The field is deliberately read here: hover and click state
    /// must stay cheap `Copy` values, not retained widgets.
    pub(in crate::tui::app) fn is_double_click(&mut self, action: Action) -> bool {
        let context = self.click_context();
        let now = Instant::now();
        let double = self.last_click.is_some_and(|(last, last_context, at)| {
            last == action
                && last_context == context
                && now.duration_since(at) < DOUBLE_CLICK_WINDOW
        });
        self.last_click = Some((action, context, now));
        double
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use crate::tui::action::{Action, Click, OverlayAct};
    use crate::tui::app::Page;
    use crate::tui::hit::Layer;
    use crate::tui::modal::ConfirmAction;

    use super::DOUBLE_CLICK_WINDOW;
    use crate::tui::app::App;

    fn row(page: Page, index: usize) -> Action {
        Action::Page {
            page,
            act: crate::tui::action::PageAct::Row(index),
        }
    }

    #[test]
    fn top_layer_follows_the_draw_order() {
        let mut app = App::new();
        assert_eq!(app.top_layer(), Layer::Base);

        app.detail_open = true;
        assert_eq!(app.top_layer(), Layer::Detail);

        app.help_open = true;
        assert_eq!(app.top_layer(), Layer::Help);

        app.modal = Some(crate::tui::modal::Modal::confirm(
            "Delete instance?",
            ConfirmAction::RemoveInstance(uuid::Uuid::nil()),
        ));
        assert_eq!(app.top_layer(), Layer::Modal);
    }

    #[test]
    fn navigation_and_overlay_close_reach_the_state() {
        let mut app = App::new();
        app.apply_action(Action::Nav(Page::Tasks), Click::Single);
        assert_eq!(app.page, Page::Tasks);

        app.help_open = true;
        app.apply_action(Action::Overlay(OverlayAct::Close), Click::Single);
        assert!(!app.help_open);
    }

    #[test]
    fn double_click_needs_the_same_action_inside_the_window() {
        let mut app = App::new();
        let target = row(Page::Instances, 3);

        assert!(!app.is_double_click(target));
        assert!(app.is_double_click(target));
        // A different target starts a new pair instead of activating.
        assert!(!app.is_double_click(Action::Nav(Page::Tasks)));

        // Outside the window the pair is stale.
        let context = app.click_context();
        app.last_click = Some((
            target,
            context,
            Instant::now() - DOUBLE_CLICK_WINDOW - Duration::from_millis(1),
        ));
        assert!(!app.is_double_click(target));
    }
}
