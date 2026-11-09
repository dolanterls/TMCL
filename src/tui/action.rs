//! Declarative mouse intents.
//!
//! A [`Action`] is what a click *means*: it carries no references and no
//! allocated text, so it can be stored in the frame's hit map and compared for
//! hover and double-click detection. `App::apply_action` is the only place
//! these values change state.
//!
//! Later stages fill in the page arms (P2) and the modal arms (P4); the
//! vocabulary is defined here once so those stages only add behaviour.

use crate::tui::app::Page;
use crate::tui::input::SearchTarget;

/// What the pointer resolved to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Shell navigation: a header tab (classic) or a sidebar row (creeper).
    Nav(Page),
    /// Something inside the current page.
    Page { page: Page, act: PageAct },
    /// The help or detail overlay.
    Overlay(OverlayAct),
    /// A dialog.
    Modal(ModalAct),
}

/// Page-local intents. `page` on [`Action::Page`] disambiguates the two filter
/// rows of the download page (`Filter` vs `VersionFilter`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageAct {
    /// A list row, card or settings row.
    Row(usize),
    /// The row's `Enter` semantics.
    Activate(usize),
    /// Filter tabs: instances, tasks, download categories.
    Filter(usize),
    /// Download-centre version filter blocks.
    VersionFilter(usize),
    /// Settings category rail.
    Category(usize),
    /// The `‹ ›` step controls on a settings row.
    Step { row: usize, by: i8 },
    /// Overview stat tile.
    Metric(Metric),
    /// Overview play card.
    Launch,
    /// Overview activity feed.
    Activity,
    /// Focus an inline search field.
    Search(SearchTarget),
    /// Open the download-centre category filter dialog.
    FilterCategories,
    /// Instance-workspace category rail.
    WorkspaceCategory(usize),
    /// Relative step of the folded rail (a narrow terminal shows one line).
    WorkspaceCategoryBy(i8),
    /// A row inside the instance workspace.
    WorkspaceRow(usize),
    /// The enable mark of a content row, or the follow marker of a launch row.
    WorkspaceToggle(usize),
    /// A row of the `Manage` list: a single click runs the action, because the
    /// list is a menu of commands rather than a selection.
    WorkspaceRun(usize),
    /// The `‹ ›` controls of a launch row.
    WorkspaceStep { row: usize, by: i8 },
}

/// The four overview stat tiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Metric {
    Instances,
    Tasks,
    Accounts,
    Java,
}

/// Overlay hits: a click on the full-screen blocker closes, one inside the popup
/// does nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayAct {
    Close,
    Noop,
}

/// Dialog hits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalAct {
    /// Ignore clicks that land on the dialog background or outside it.
    Noop,
    /// Focus a field row.
    Focus(usize),
    /// Open or close a field's option list.
    Toggle(usize),
    /// Choose one option.
    Option(usize),
    /// The `F2` browse chip.
    Browse(usize),
    /// The `F2` chip on a field that holds a file path.
    BrowseFile(usize),
    Submit,
    Cancel,
    Confirm,
    Reject,
    /// Move the open option list by one row per notch.
    ScrollOptions(i8),
    /// Toggle one version-category chip in the version picker.
    ToggleVersionFilter(usize),
    /// Put the pointer in the picker search field (same as `/`).
    EditSearch,
    /// Close the open option list and keep the dialog (same as `Esc` there).
    ///
    /// Distinct from `Cancel`: the picker's `Esc` backs out of the list, not out
    /// of the form the list belongs to.
    CloseOptions,
    /// Ask for the option list again after it failed or came back empty.
    RetryOptions,
    /// Toggle (multi) or select (single) one checklist row.
    CheckItem(usize),
    /// Move the checklist cursor to a row.
    FocusCheck(usize),
    /// Apply the checklist selection.
    ApplyChecks,
    /// Clear every checklist row.
    ResetChecks,
}

/// How a click arrived, for targets whose single and double click differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Click {
    Single,
    Double,
}

// Convenience constructors. Draw sites use these so a page never has to name
// the `PageAct` variant (and can never register a hit for the wrong page).
impl Action {
    pub fn row(page: Page, index: usize) -> Self {
        Self::Page {
            page,
            act: PageAct::Row(index),
        }
    }

    pub fn filter(page: Page, index: usize) -> Self {
        Self::Page {
            page,
            act: PageAct::Filter(index),
        }
    }

    pub fn version_filter(index: usize) -> Self {
        Self::Page {
            page: Page::DownloadCenter,
            act: PageAct::VersionFilter(index),
        }
    }

    pub fn category(index: usize) -> Self {
        Self::Page {
            page: Page::Settings,
            act: PageAct::Category(index),
        }
    }

    pub fn workspace_category(index: usize) -> Self {
        Self::Page {
            page: Page::Instances,
            act: PageAct::WorkspaceCategory(index),
        }
    }

    pub fn workspace_category_by(by: i8) -> Self {
        Self::Page {
            page: Page::Instances,
            act: PageAct::WorkspaceCategoryBy(by),
        }
    }

    pub fn workspace_row(index: usize) -> Self {
        Self::Page {
            page: Page::Instances,
            act: PageAct::WorkspaceRow(index),
        }
    }

    pub fn workspace_toggle(index: usize) -> Self {
        Self::Page {
            page: Page::Instances,
            act: PageAct::WorkspaceToggle(index),
        }
    }

    pub fn workspace_step(row: usize, by: i8) -> Self {
        Self::Page {
            page: Page::Instances,
            act: PageAct::WorkspaceStep { row, by },
        }
    }

    pub fn step(row: usize, by: i8) -> Self {
        Self::Page {
            page: Page::Settings,
            act: PageAct::Step { row, by },
        }
    }

    pub fn metric(metric: Metric) -> Self {
        Self::Page {
            page: Page::Overview,
            act: PageAct::Metric(metric),
        }
    }

    pub const LAUNCH: Self = Self::Page {
        page: Page::Overview,
        act: PageAct::Launch,
    };
    pub const ACTIVITY: Self = Self::Page {
        page: Page::Overview,
        act: PageAct::Activity,
    };

    pub fn search(target: SearchTarget) -> Self {
        Self::Page {
            page: match target {
                SearchTarget::Resources | SearchTarget::VersionFilter => Page::DownloadCenter,
                SearchTarget::Settings => Page::Settings,
                SearchTarget::Workspace => Page::Instances,
                SearchTarget::Logs => Page::Logs,
            },
            act: PageAct::Search(target),
        }
    }
}

/// Wheel targets. The wheel never produces an [`Action`]: it scrolls whatever region
/// is under the pointer, so modals and panes need no special-casing in the router.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollTarget {
    /// Selection inside a list; one row per notch.
    Rows(crate::tui::hit::RowKind),
    /// The page's inline detail pane.
    Pane,
    /// The log viewport.
    LogLines,
    /// The detail popup body.
    DetailBody,
    /// The help overlay body.
    HelpBody,
    /// The crash-diagnosis card body.
    DiagnosisBody,
    /// The instance-workspace row list.
    WorkspaceRows,
    /// Discard the notch: a blocking overlay is open and has no scroll area under
    /// the pointer. Without this the lookup falls through to the page behind the
    /// overlay.
    Blocked,
    /// A dialog's option list (registered in P4).
    ModalOptions,
}
