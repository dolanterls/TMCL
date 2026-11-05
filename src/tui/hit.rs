//! Frame-local hit regions for the mouse.
//!
//! The renderer registers one [`Hitbox`] for every interactive rectangle it
//! actually draws; the event loop looks the pointer up in that map after the
//! frame and turns the result into an [`Action`]. Drawing writes, events read —
//! the same contract `regions: Cell<ChromeRegions>` already follows, so the
//! clickable area cannot drift away from what is on screen.
//!
//! Mechanism only: draw-time registration, last-registered-wins, declarative
//! hit values. The same immediate-mode pattern is used by Hot-RS-Launcher
//! (GPL-3.0-only); this implementation is independent and MIT-licensed.

use std::cell::{Cell, RefCell};

use ratatui::layout::{Position, Rect};

use crate::tui::action::{Action, PageAct, ScrollTarget};
use crate::tui::app::Page;
use crate::tui::widgets::rows::RowLayout;

/// Stacking order for hit regions, lowest first; `Layer` order is draw order.
///
/// A blocking overlay registers a full-screen box in its own layer, so the
/// layers below it become unreachable without any coordinate arithmetic.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Layer {
    /// Chrome and the current page.
    #[default]
    Base,
    /// The detail popup.
    Detail,
    /// The help overlay.
    Help,
    /// A dialog.
    Modal,
}

impl Layer {
    /// The next layer down, or `None` at the bottom.
    fn below(self) -> Option<Self> {
        match self {
            Self::Modal => Some(Self::Help),
            Self::Help => Some(Self::Detail),
            Self::Detail => Some(Self::Base),
            Self::Base => None,
        }
    }
}

/// One registered rectangle.
#[derive(Clone, Copy, Debug)]
pub struct Hitbox {
    pub rect: Rect,
    pub layer: Layer,
    pub action: Action,
}

/// A lookup result: which layer answered and what it meant.
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub layer: Layer,
    pub action: Action,
}

/// Which collection a row belongs to. The variants differ only in the page the
/// resulting [`Action`] targets, so a draw site names the list it paints
/// instead of repeating the page at every row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    OverviewInstances,
    Instances,
    Accounts,
    Tasks,
    Versions,
    ResourceProjects,
    ResourceFiles,
    Settings,
    Java,
    Directories,
}

impl RowKind {
    pub fn page(self) -> Page {
        match self {
            Self::OverviewInstances => Page::Overview,
            Self::Instances => Page::Instances,
            Self::Accounts => Page::Accounts,
            Self::Tasks => Page::Tasks,
            Self::Versions | Self::ResourceProjects | Self::ResourceFiles => Page::DownloadCenter,
            Self::Settings | Self::Java | Self::Directories => Page::Settings,
        }
    }

    pub fn action(self, index: usize) -> Action {
        Action::Page {
            page: self.page(),
            act: PageAct::Row(index),
        }
    }
}

/// The hit regions of the frame that was drawn last.
///
/// Interior mutability keeps `App::draw(&self)` unchanged: tests and the event
/// loop keep calling `draw` on a shared borrow and read the map back.
#[derive(Default)]
pub struct Hitmap {
    size: Cell<(u16, u16)>,
    generation: Cell<u64>,
    layer: Cell<Layer>,
    boxes: RefCell<Vec<Hitbox>>,
    scrolls: RefCell<Vec<(Rect, Layer, ScrollTarget)>>,
}

impl Hitmap {
    /// Starts a frame: clears the previous regions and records which frame this
    /// is (`generation` is the app's resize counter, see `App::resize_gen`).
    pub fn begin(&self, size: (u16, u16), generation: u64) {
        self.boxes.borrow_mut().clear();
        self.scrolls.borrow_mut().clear();
        self.size.set(size);
        self.generation.set(generation);
        self.layer.set(Layer::Base);
    }

    /// Generation of the frame these regions came from.
    pub fn generation(&self) -> u64 {
        self.generation.get()
    }

    /// Frame size these regions were registered against.
    pub fn size(&self) -> (u16, u16) {
        self.size.get()
    }

    /// Layer that subsequent [`Hitmap::add`] calls belong to.
    pub fn set_layer(&self, layer: Layer) {
        self.layer.set(layer);
    }

    /// Current registration layer.
    pub fn layer(&self) -> Layer {
        self.layer.get()
    }

    /// Registers `rect` in the current layer.
    pub fn add(&self, rect: Rect, action: Action) {
        self.add_on(self.layer.get(), rect, action);
    }

    /// Registers `rect` in `layer` explicitly.
    pub fn add_on(&self, layer: Layer, rect: Rect, action: Action) {
        if rect.width == 0 || rect.height == 0 {
            return;
        }
        let (width, height) = self.size.get();
        if rect.x >= width || rect.y >= height {
            return;
        }
        self.boxes.borrow_mut().push(Hitbox {
            rect,
            layer,
            action,
        });
    }

    /// Registers one box per drawn row of `layout`.
    pub fn rows(&self, layout: RowLayout, kind: RowKind) {
        self.rows_at(layout, kind, 0);
    }

    /// Same, for a list rendered from a pre-sliced window: `base` is the index
    /// of the first item in that window (the version list scrolls this way).
    pub fn rows_at(&self, layout: RowLayout, kind: RowKind, base: usize) {
        for (index, rect) in layout.rows() {
            self.add(rect, kind.action(base + index));
        }
        // The whole list scrolls, not just its painted rows: a notch over the
        // header or the border belongs to the same list.
        self.scroll(layout.inner(), ScrollTarget::Rows(kind));
    }

    /// Topmost region under `(column, row)`, considering `top` and below.
    pub fn at(&self, column: u16, row: u16, top: Layer) -> Option<Hit> {
        let boxes = self.boxes.borrow();
        let point = Position::new(column, row);
        let mut layer = Some(top);
        while let Some(current) = layer {
            if let Some(hitbox) = boxes
                .iter()
                .rev()
                .find(|hitbox| hitbox.layer == current && hitbox.rect.contains(point))
            {
                return Some(Hit {
                    layer: current,
                    action: hitbox.action,
                });
            }
            layer = current.below();
        }
        None
    }

    /// Number of registered regions in the current frame.
    pub fn len(&self) -> usize {
        self.boxes.borrow().len()
    }

    /// Every region registered this frame, in registration order.
    ///
    /// Used by the coverage test that walks a drawn page and proves each box is
    /// reachable; production code looks regions up by coordinate instead.
    pub fn hitboxes(&self) -> Vec<Hitbox> {
        self.boxes.borrow().clone()
    }

    pub fn is_empty(&self) -> bool {
        self.boxes.borrow().is_empty()
    }

    /// Registers a wheel region in the current layer.
    pub fn scroll(&self, rect: Rect, target: ScrollTarget) {
        self.scroll_on(self.layer.get(), rect, target);
    }

    pub fn scroll_on(&self, layer: Layer, rect: Rect, target: ScrollTarget) {
        if rect.width == 0 || rect.height == 0 {
            return;
        }
        self.scrolls.borrow_mut().push((rect, layer, target));
    }

    /// Topmost scroll region under `(column, row)`, considering `top` and below.
    pub fn scroll_at(&self, column: u16, row: u16, top: Layer) -> Option<ScrollTarget> {
        let scrolls = self.scrolls.borrow();
        let point = Position::new(column, row);
        let mut layer = Some(top);
        while let Some(current) = layer {
            if let Some((_, _, target)) = scrolls
                .iter()
                .rev()
                .find(|(rect, layer, _)| *layer == current && rect.contains(point))
            {
                return Some(*target);
            }
            layer = current.below();
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{Hitmap, Layer, RowKind};
    use crate::tui::action::{Action, OverlayAct};
    use crate::tui::app::Page;
    use crate::tui::widgets::rows::RowLayout;
    use ratatui::layout::Rect;

    fn nav(page: Page) -> Action {
        Action::Nav(page)
    }

    fn map() -> Hitmap {
        let map = Hitmap::default();
        map.begin((80, 24), 0);
        map
    }

    #[test]
    fn later_registrations_win_inside_a_layer() {
        let map = map();
        let area = Rect::new(2, 2, 10, 1);
        map.add(area, nav(Page::Overview));
        map.add(area, nav(Page::Tasks));

        let hit = map.at(4, 2, Layer::Base).expect("hit");
        assert_eq!(hit.action, nav(Page::Tasks));
        assert_eq!(hit.layer, Layer::Base);
    }

    #[test]
    fn zero_sized_and_off_frame_regions_are_dropped() {
        let map = map();
        map.add(Rect::new(1, 1, 0, 1), nav(Page::Overview));
        map.add(Rect::new(1, 1, 1, 0), nav(Page::Overview));
        map.add(Rect::new(80, 0, 4, 1), nav(Page::Overview));
        map.add(Rect::new(0, 24, 4, 1), nav(Page::Overview));

        assert!(map.is_empty());
        assert!(map.at(0, 0, Layer::Base).is_none());
    }

    #[test]
    fn a_higher_layer_blocks_the_ones_below_it() {
        let map = map();
        map.add(Rect::new(0, 0, 80, 24), nav(Page::Overview));
        map.add_on(
            Layer::Help,
            Rect::new(0, 0, 80, 24),
            Action::Overlay(OverlayAct::Close),
        );

        let hit = map.at(10, 10, Layer::Help).expect("blocker hit");
        assert_eq!(hit.layer, Layer::Help);
        assert_eq!(hit.action, Action::Overlay(OverlayAct::Close));

        // With the overlay gone the page box answers again.
        assert_eq!(
            map.at(10, 10, Layer::Base).map(|hit| hit.layer),
            Some(Layer::Base)
        );
    }

    #[test]
    fn the_popup_body_wins_over_the_full_screen_blocker() {
        let map = map();
        map.add_on(
            Layer::Detail,
            Rect::new(0, 0, 80, 24),
            Action::Overlay(OverlayAct::Close),
        );
        map.add_on(
            Layer::Detail,
            Rect::new(20, 5, 40, 10),
            Action::Overlay(OverlayAct::Noop),
        );

        assert_eq!(
            map.at(30, 8, Layer::Detail).map(|hit| hit.action),
            Some(Action::Overlay(OverlayAct::Noop))
        );
        assert_eq!(
            map.at(2, 2, Layer::Detail).map(|hit| hit.action),
            Some(Action::Overlay(OverlayAct::Close))
        );
    }

    #[test]
    fn lookup_skips_layers_with_no_box_under_the_pointer() {
        let map = map();
        map.add(Rect::new(0, 0, 40, 1), nav(Page::Instances));
        map.add_on(
            Layer::Modal,
            Rect::new(50, 10, 10, 3),
            Action::Modal(crate::tui::action::ModalAct::Noop),
        );

        // Pointer outside the dialog: the modal layer has nothing there, so the
        // search continues downwards.
        assert_eq!(
            map.at(4, 0, Layer::Modal).map(|hit| hit.layer),
            Some(Layer::Base)
        );
    }

    #[test]
    fn begin_resets_size_generation_and_layer() {
        let map = map();
        map.set_layer(Layer::Help);
        map.add(Rect::new(0, 0, 4, 1), nav(Page::Overview));
        assert_eq!(map.len(), 1);
        assert_eq!(map.layer(), Layer::Help);

        map.begin((120, 40), 7);
        assert_eq!(map.len(), 0);
        assert_eq!(map.size(), (120, 40));
        assert_eq!(map.generation(), 7);
        assert_eq!(map.layer(), Layer::Base);
    }

    #[test]
    fn rows_register_one_clickable_box_per_visible_row() {
        let map = map();
        let layout = RowLayout::table(Rect::new(0, 0, 40, 8), 40, 30);
        map.rows(layout, RowKind::Instances);

        // Offset 26 → the first visible row is item 26, not item 0.
        let first = layout.rows().next().expect("first row");
        let hit = map.at(first.1.x + 1, first.1.y, Layer::Base).expect("hit");
        assert_eq!(hit.action, RowKind::Instances.action(26));
    }

    #[test]
    fn rows_at_prefixes_the_window_offset() {
        let map = map();
        // A pre-sliced page of versions: rows already start at item 0 of the
        // slice, but the slice itself begins at item 40 of the full list.
        let layout = RowLayout::table(Rect::new(0, 0, 40, 8), 5, 0);
        map.rows_at(layout, RowKind::Versions, 40);

        let first = layout.rows().next().expect("first row");
        assert_eq!(
            map.at(first.1.x, first.1.y, Layer::Base)
                .map(|hit| hit.action),
            Some(RowKind::Versions.action(40))
        );
    }
}
