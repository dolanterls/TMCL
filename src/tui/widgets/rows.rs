use std::cell::Cell;

use ratatui::{
    layout::Rect,
    widgets::{Block, Borders},
};

/// One list that remembers where its window sat.
///
/// Keyed by the list itself, not by the page or the preset: the three presets
/// draw the same list with different geometry, so leaving for another page or
/// switching preset must not lose the position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowWindow {
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
    Workspace,
}

impl RowWindow {
    /// Slots in [`RowWindows`]; keep in step with the variants above.
    pub const COUNT: usize = 11;
}

/// Scroll memory for [`RowWindow`]s: where each list's window sat the last time
/// it was drawn.
///
/// Without it the offset can only be derived from the selection, so every
/// selection change re-anchors the window and clicking a row that is already on
/// screen scrolls the list out from under the pointer. Drawing writes the
/// window back through [`Cell`]s, so `App::draw(&self)` stays a shared borrow.
pub struct RowWindows {
    offsets: [Cell<Option<usize>>; RowWindow::COUNT],
}

impl Default for RowWindows {
    fn default() -> Self {
        Self {
            offsets: std::array::from_fn(|_| Cell::new(None)),
        }
    }
}

impl RowWindows {
    /// Where `window` sat last time, or `None` if it was never drawn.
    pub fn offset(&self, window: RowWindow) -> Option<usize> {
        self.offsets[window as usize].get()
    }

    /// Resolves `item_count` items with `visible_rows` on screen into the first
    /// row to show, and remembers it for the next frame.
    pub fn resolve_offset(
        &self,
        window: RowWindow,
        item_count: usize,
        selected: usize,
        visible_rows: usize,
    ) -> usize {
        let slot = &self.offsets[window as usize];
        let offset = window_offset(item_count, selected, visible_rows, slot.get());
        slot.set(Some(offset));
        offset
    }

    /// [`Self::resolve_offset`] for a layout that already knows its geometry.
    pub fn resolve(&self, window: RowWindow, layout: RowLayout) -> RowLayout {
        let slot = &self.offsets[window as usize];
        let layout = layout.anchored_at(slot.get());
        slot.set(Some(layout.offset()));
        layout
    }
}

/// First item a window of `visible_rows` shows.
///
/// `previous` is the offset the same list used on the previous frame. The window
/// then moves only when `selected` has left it, which keeps a click on a drawn
/// row from scrolling the list. `None`, a list drawn for the first time, keeps
/// the selection visible with the smallest scroll instead.
pub fn window_offset(
    item_count: usize,
    selected: usize,
    visible_rows: usize,
    previous: Option<usize>,
) -> usize {
    if item_count == 0 || visible_rows == 0 {
        return 0;
    }
    let selected = selected.min(item_count - 1);
    let last = item_count.saturating_sub(visible_rows);
    let Some(previous) = previous else {
        return selected.saturating_sub(visible_rows - 1).min(last);
    };
    let first = previous.min(last);
    if selected < first {
        selected
    } else if selected >= first + visible_rows {
        (selected + 1 - visible_rows).min(last)
    } else {
        first
    }
}

/// Geometry shared by a rendered collection and its mouse hit testing.
///
/// Ratatui applies the block's inner area and the table header before it
/// decides which item is visible. Keeping those values here prevents the
/// renderer and the event handler from maintaining two subtly different
/// versions of the same calculation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowLayout {
    inner: Rect,
    first_row: u16,
    visible_rows: usize,
    offset: usize,
    selected: usize,
    item_count: usize,
    card_height: u16,
    gap: u16,
}

impl RowLayout {
    pub fn list(area: Rect, item_count: usize, selected: usize) -> Self {
        Self::new(area, item_count, selected, 0, 1, 0)
    }

    pub fn table(area: Rect, item_count: usize, selected: usize) -> Self {
        Self::new(area, item_count, selected, 1, 1, 0)
    }

    /// Multi-row cards inside a bordered block: every `card_height` rows hold
    /// one item, and a click anywhere inside a card selects it.
    pub fn cards(area: Rect, item_count: usize, selected: usize, card_height: u16) -> Self {
        Self::cards_with_gap(area, item_count, selected, card_height, 0)
    }

    /// Cards separated by `gap` blank rows. The gap is part of the stride but
    /// is not a hit box, so clicking between two cards selects nothing.
    pub fn cards_with_gap(
        area: Rect,
        item_count: usize,
        selected: usize,
        card_height: u16,
        gap: u16,
    ) -> Self {
        Self::new(area, item_count, selected, 1, card_height.max(1), gap)
    }

    /// [`Self::table`] for a list drawn without a block border, where `area` is
    /// already the content rectangle.
    ///
    /// The bordered constructors subtract a one-cell frame because callers pass
    /// the block's outer rectangle. A borderless shell passes its content
    /// rectangle, so subtracting again would shift every hit box down and right
    /// by one: the rows would look right and click wrong.
    pub fn table_bare(area: Rect, item_count: usize, selected: usize) -> Self {
        Self::with_inner(area, item_count, selected, 1, 1, 0)
    }

    /// Rows of a borderless list whose header row the caller already excluded.
    pub fn list_bare(area: Rect, item_count: usize, selected: usize) -> Self {
        Self::with_inner(area, item_count, selected, 0, 1, 0)
    }

    /// [`Self::cards_with_gap`] without the implied block border.
    pub fn cards_bare(
        area: Rect,
        item_count: usize,
        selected: usize,
        card_height: u16,
        gap: u16,
    ) -> Self {
        Self::with_inner(area, item_count, selected, 0, card_height.max(1), gap)
    }

    pub fn offset(self) -> usize {
        self.offset
    }

    /// Selection this layout was built for.
    pub fn selected(self) -> usize {
        self.selected
    }

    /// The same geometry scrolled to `offset`, clamped to the last window.
    pub fn moved_to(self, offset: usize) -> Self {
        Self {
            offset: offset.min(self.item_count.saturating_sub(self.visible_rows)),
            ..self
        }
    }

    /// Re-resolves the window against `previous`, the offset this list used last
    /// frame. See [`window_offset`] for what "previous" buys.
    pub fn anchored_at(self, previous: Option<usize>) -> Self {
        self.moved_to(window_offset(
            self.item_count,
            self.selected,
            self.visible_rows,
            previous,
        ))
    }

    pub fn item_count(self) -> usize {
        self.item_count
    }

    /// The block's inner area: rows never overlap the border.
    pub fn inner(self) -> Rect {
        self.inner
    }

    /// One `(item index, rectangle)` pair per drawn row, in screen order.
    ///
    /// This is the single source of truth for "what is clickable": the hit map
    /// asks the layout the renderer already used, so a row cannot be drawn in
    /// one place and hit-tested in another. Gap rows between cards are omitted.
    pub fn rows(self) -> impl Iterator<Item = (usize, Rect)> {
        (0..self.visible_rows).filter_map(move |visual| {
            let y = self
                .first_row
                .saturating_add((visual as u16).saturating_mul(self.stride()));
            let index = self.index_at(y)?;
            Some((
                index,
                Rect::new(self.inner.x, y, self.inner.width, self.card_height),
            ))
        })
    }

    fn stride(self) -> u16 {
        self.card_height.saturating_add(self.gap).max(1)
    }

    pub fn visible_rows(self) -> usize {
        self.visible_rows
    }

    pub fn card_height(self) -> u16 {
        self.card_height
    }

    pub fn gap(self) -> u16 {
        self.gap
    }

    /// Row of the first card/row inside the block border and header.
    pub fn first_row(self) -> u16 {
        self.first_row
    }

    pub fn index_at(self, row: u16) -> Option<usize> {
        if self.item_count == 0 || row < self.first_row {
            return None;
        }

        let stride = usize::from(self.stride());
        let offset = usize::from(row - self.first_row);
        // A reserved blank row belongs to no card.
        if offset % stride >= usize::from(self.card_height) {
            return None;
        }
        let visual_index = offset / stride;
        if visual_index >= self.visible_rows {
            return None;
        }

        let index = self.offset + visual_index;
        (index < self.item_count).then_some(index)
    }

    fn new(
        area: Rect,
        item_count: usize,
        selected: usize,
        header_height: u16,
        card_height: u16,
        gap: u16,
    ) -> Self {
        let inner = Block::default().borders(Borders::ALL).inner(area);
        Self::with_inner(inner, item_count, selected, header_height, card_height, gap)
    }

    /// `inner` is the area rows may occupy, borders already accounted for.
    fn with_inner(
        inner: Rect,
        item_count: usize,
        selected: usize,
        header_height: u16,
        card_height: u16,
        gap: u16,
    ) -> Self {
        let header_height = header_height.min(inner.height);
        let first_row = inner.y.saturating_add(header_height);
        let available = inner.height.saturating_sub(header_height);
        let stride = card_height.saturating_add(gap).max(1);
        // The last card needs no trailing gap, hence `available + gap`.
        let visible_rows = usize::from(available.saturating_add(gap) / stride);
        let offset = window_offset(item_count, selected, visible_rows, None);

        Self {
            inner,
            first_row,
            visible_rows,
            offset,
            selected,
            item_count,
            card_height,
            gap,
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::{RowLayout, RowWindow, RowWindows};

    #[test]
    fn list_layout_starts_at_the_block_inner_row() {
        let layout = RowLayout::list(Rect::new(4, 7, 30, 6), 10, 0);

        assert_eq!(layout.index_at(7), None);
        assert_eq!(layout.index_at(8), Some(0));
        assert_eq!(layout.index_at(11), Some(3));
        assert_eq!(layout.index_at(12), None);

        let selected_near_top = RowLayout::list(Rect::new(4, 7, 30, 6), 10, 1);
        assert_eq!(selected_near_top.offset(), 0);
    }

    #[test]
    fn table_layout_accounts_for_the_header_and_scroll_offset() {
        let layout = RowLayout::table(Rect::new(4, 7, 30, 6), 10, 8);

        assert_eq!(layout.offset(), 6);
        assert_eq!(layout.index_at(8), None);
        assert_eq!(layout.index_at(9), Some(6));
        assert_eq!(layout.index_at(11), Some(8));
        assert_eq!(layout.index_at(12), None);
    }

    #[test]
    fn card_gap_is_not_a_hit_box() {
        let layout = RowLayout::cards_with_gap(Rect::new(0, 0, 30, 17), 3, 0, 3, 1);

        assert_eq!(layout.visible_rows(), 3);
        assert_eq!(layout.gap(), 1);
        assert_eq!(layout.index_at(2), Some(0));
        assert_eq!(layout.index_at(4), Some(0));
        // Blank row between the first and second card.
        assert_eq!(layout.index_at(5), None);
        assert_eq!(layout.index_at(6), Some(1));
        assert_eq!(layout.index_at(9), None);
        assert_eq!(layout.index_at(10), Some(2));
    }

    #[test]
    fn card_layout_maps_every_row_of_a_card_to_its_item() {
        let layout = RowLayout::cards(Rect::new(0, 0, 30, 11), 3, 0, 3);

        assert_eq!(layout.card_height(), 3);
        assert_eq!(layout.visible_rows(), 2);
        assert_eq!(layout.first_row(), 2);
        assert_eq!(layout.index_at(1), None);
        assert_eq!(layout.index_at(2), Some(0));
        assert_eq!(layout.index_at(4), Some(0));
        assert_eq!(layout.index_at(5), Some(1));
        assert_eq!(layout.index_at(7), Some(1));
        assert_eq!(layout.index_at(8), None);
    }

    #[test]
    fn card_layout_scrolls_so_the_selection_stays_visible() {
        let layout = RowLayout::cards(Rect::new(0, 0, 30, 11), 6, 5, 3);

        assert_eq!(layout.visible_rows(), 2);
        assert_eq!(layout.offset(), 4);
        assert_eq!(layout.index_at(2), Some(4));
        assert_eq!(layout.index_at(5), Some(5));
    }

    #[test]
    fn rows_pairs_every_card_with_the_rectangle_it_occupies() {
        let area = Rect::new(4, 6, 30, 17);
        // Three rows per card, one blank row between them.
        let layout = RowLayout::cards_with_gap(area, 3, 0, 3, 1);
        let rows = layout.rows().collect::<Vec<_>>();

        assert_eq!(layout.visible_rows(), 3);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].0, 0);
        assert_eq!(rows[0].1, Rect::new(5, 8, 28, 3));
        // The gap row shifts the second card by the full stride.
        assert_eq!(rows[1].0, 1);
        assert_eq!(rows[1].1.y, 12);
        assert_eq!(rows[2].1.y, 16);
        // Every rectangle already passes the arithmetic hit test.
        for (index, rect) in &rows {
            assert_eq!(layout.index_at(rect.y), Some(*index));
        }
    }

    #[test]
    fn rows_accounts_for_the_window_offset() {
        let layout = RowLayout::table(Rect::new(0, 0, 20, 8), 40, 30);
        let rows = layout.rows().collect::<Vec<_>>();

        assert_eq!(layout.offset(), 26);
        assert_eq!(rows.first().map(|row| row.0), Some(26));
        assert_eq!(rows.len(), 5);
    }

    #[test]
    fn an_anchored_window_leaves_a_visible_selection_where_it_is() {
        // Rows 10..16 are on screen; the player clicks row 12 of 40. The window
        // must not move: re-anchoring would drag the row to the bottom edge.
        let layout = RowLayout::table(Rect::new(0, 0, 20, 8), 40, 12).anchored_at(Some(10));

        assert_eq!(layout.offset(), 10);
        assert_eq!(layout.index_at(2), Some(10));
        assert_eq!(layout.selected(), 12);
    }

    #[test]
    fn an_anchored_window_scrolls_only_until_the_selection_is_back_on_screen() {
        let area = Rect::new(0, 0, 20, 8);
        // Below the window: one row of scroll is enough, not a full page.
        assert_eq!(
            RowLayout::table(area, 40, 16)
                .anchored_at(Some(10))
                .offset(),
            12
        );
        // Above the window: the selection becomes the first row.
        assert_eq!(
            RowLayout::table(area, 40, 3).anchored_at(Some(10)).offset(),
            3
        );
        // Past the end of a shorter list: clamp, never scroll into blank rows.
        assert_eq!(
            RowLayout::table(area, 10, 9).anchored_at(Some(30)).offset(),
            5
        );
    }

    #[test]
    fn every_row_window_fits_the_slot_array() {
        // `COUNT` cannot be derived from the enum, so a new variant has to bump
        // it. A missed bump indexes past the end of the array, and this fails
        // first instead of somewhere inside a draw call.
        assert!((RowWindow::Workspace as usize) < RowWindow::COUNT);
    }

    #[test]
    fn the_window_memory_keeps_one_slot_per_list() {
        let windows = RowWindows::default();
        let instances = RowLayout::table(Rect::new(0, 0, 20, 8), 40, 12);
        let tasks = RowLayout::table(Rect::new(0, 0, 20, 8), 40, 30);

        assert_eq!(windows.offset(RowWindow::Instances), None);
        windows.resolve(RowWindow::Instances, instances);
        windows.resolve(RowWindow::Tasks, tasks);

        assert_eq!(windows.offset(RowWindow::Instances), Some(8));
        assert_eq!(windows.offset(RowWindow::Tasks), Some(26));
        // The next frame starts from the window the last one used, so clicking
        // a row that is already drawn changes nothing.
        let again = windows.resolve(
            RowWindow::Instances,
            RowLayout::table(Rect::new(0, 0, 20, 8), 40, 12),
        );
        assert_eq!(again.offset(), 8);
    }
}
