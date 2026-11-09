use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};

use crate::core::config::models::UiPreset;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    Full,
    Compact,
    Narrow,
    TooSmall,
}

/// Geometry of the binsider-style shell.
///
/// Every consumer (rendering and mouse hit-testing) reads the same struct, so
/// the click targets cannot drift away from what is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChromeRegions {
    pub mode: LayoutMode,
    pub header: Rect,
    pub tabs: Rect,
    pub context: Rect,
    pub body: Rect,
    pub content: Rect,
    pub main: Rect,
    pub footer: Rect,
    pub modal: Option<Rect>,
}

pub fn layout_mode(area: Rect) -> LayoutMode {
    if area.width < 80 || area.height < 24 {
        LayoutMode::TooSmall
    } else if area.width < 100 || area.height < 30 {
        LayoutMode::Narrow
    } else if area.width < 120 || area.height < 40 {
        LayoutMode::Compact
    } else {
        LayoutMode::Full
    }
}

/// Preset-aware geometry. `classic` keeps the binsider frame; `creeper` uses a
/// brand row, a gradient rule and a sidebar, and reuses the same
/// [`ChromeRegions`] fields so mouse handling and paging stay preset-agnostic.
pub fn layout_chrome_for(area: Rect, preset: UiPreset) -> ChromeRegions {
    match preset {
        UiPreset::Classic => layout_chrome(area),
        UiPreset::Creeper => layout_chrome_creeper(area),
        UiPreset::Nightlight => layout_chrome_nightlight(area),
        UiPreset::Paper => layout_chrome_paper(area),
        UiPreset::Shelf => layout_chrome_shelf(area),
    }
}

/// Size class for the `creeper` preset. It needs a little less room than the
/// framed shell because the sidebar doubles as navigation.
pub fn layout_mode_creeper(area: Rect) -> LayoutMode {
    if area.width < 60 || area.height < 18 {
        LayoutMode::TooSmall
    } else if area.width < 90 || area.height < 30 {
        LayoutMode::Narrow
    } else if area.width < 120 || area.height < 40 {
        LayoutMode::Compact
    } else {
        LayoutMode::Full
    }
}

/// Width of the sidebar navigation column, in terminal cells.
pub fn creeper_sidebar_width(mode: LayoutMode) -> u16 {
    match mode {
        LayoutMode::Full => 22,
        LayoutMode::Compact => 20,
        LayoutMode::Narrow => 6,
        LayoutMode::TooSmall => 0,
    }
}

pub fn layout_chrome_creeper(area: Rect) -> ChromeRegions {
    // The preset always shows a hints row above a status band; callers that
    // know the page hints ask for more rows through the `_with_footer` form.
    layout_chrome_creeper_with_footer(area, 2)
}

/// Same layout with a footer that may span several rows.
///
/// The preset wraps its key hints instead of dropping them on a narrow
/// terminal, so the body shrinks by the extra footer rows.
pub fn layout_chrome_creeper_with_footer(area: Rect, footer_rows: u16) -> ChromeRegions {
    let mode = layout_mode_creeper(area);
    if mode == LayoutMode::TooSmall {
        let footer = Rect::new(
            area.x,
            area.y + area.height.saturating_sub(1),
            area.width,
            1,
        );
        return ChromeRegions {
            mode,
            header: Rect::new(area.x, area.y, area.width, 1),
            tabs: Rect::default(),
            context: Rect::default(),
            body: area,
            content: area,
            main: area,
            footer,
            modal: None,
        };
    }

    let footer_rows = footer_rows.max(1).min(area.height.saturating_sub(2).max(1));
    let inner_width = area.width.saturating_sub(2);
    let header = Rect::new(area.x + 1, area.y, inner_width, 1);
    let body = Rect::new(
        area.x + 1,
        area.y.saturating_add(2),
        inner_width,
        area.height.saturating_sub(2 + footer_rows),
    );
    let footer = Rect::new(
        area.x + 1,
        area.y + area.height.saturating_sub(footer_rows),
        inner_width,
        footer_rows,
    );
    let sidebar_width = creeper_sidebar_width(mode);
    let sidebar = Rect::new(body.x, body.y, sidebar_width.min(body.width), body.height);
    let content_x = sidebar.right().saturating_add(1);
    let content = Rect::new(
        content_x,
        body.y,
        body.right().saturating_sub(content_x),
        body.height,
    );
    let context_width = if mode == LayoutMode::Full {
        46.min(header.width / 2)
    } else {
        0
    };
    let context = Rect::new(
        header.right().saturating_sub(context_width),
        header.y,
        context_width,
        header.height,
    );

    ChromeRegions {
        mode,
        header,
        tabs: sidebar,
        context,
        body,
        content,
        main: content,
        footer,
        modal: None,
    }
}

/// Size class for the `nightlight` preset.
///
/// Every threshold sits above `creeper` except `Full`: 120×40 stays the
/// reference canvas for all three presets, so the sidebar gets its full 26
/// columns there instead of being demoted to `Compact`.
pub fn layout_mode_nightlight(area: Rect) -> LayoutMode {
    if area.width < 64 || area.height < 18 {
        LayoutMode::TooSmall
    } else if area.width < 96 || area.height < 30 {
        LayoutMode::Narrow
    } else if area.width < 120 || area.height < 40 {
        LayoutMode::Compact
    } else {
        LayoutMode::Full
    }
}

/// Width of the sidebar column, in terminal cells. `Narrow` keeps only the
/// icon rail so the content grid still gets a usable width at 80 columns.
pub fn nightlight_sidebar_width(mode: LayoutMode) -> u16 {
    match mode {
        LayoutMode::Full => 26,
        LayoutMode::Compact => 22,
        LayoutMode::Narrow => 6,
        LayoutMode::TooSmall => 0,
    }
}

pub fn layout_chrome_nightlight(area: Rect) -> ChromeRegions {
    layout_chrome_nightlight_with_footer(area, 1)
}

/// The `nightlight` shell: brand row is the sidebar's first row, there is no
/// banner, and the footer spans the full width under the sidebar as well.
///
/// `footer.y - 1` is left blank for the hairline; the body stops one row above
/// the footer so a card can never grow into the rule.
pub fn layout_chrome_nightlight_with_footer(area: Rect, footer_rows: u16) -> ChromeRegions {
    let mode = layout_mode_nightlight(area);
    if mode == LayoutMode::TooSmall {
        let footer = Rect::new(
            area.x,
            area.y + area.height.saturating_sub(1),
            area.width,
            1,
        );
        return ChromeRegions {
            mode,
            header: Rect::new(area.x, area.y, area.width, 1),
            tabs: Rect::default(),
            context: Rect::default(),
            body: area,
            content: area,
            main: area,
            footer,
            modal: None,
        };
    }

    let footer_rows = footer_rows.max(1).min(area.height.saturating_sub(3).max(1));
    let footer = Rect::new(
        area.x,
        area.y + area.height.saturating_sub(footer_rows),
        area.width,
        footer_rows,
    );
    // One row for the hairline directly above the footer.
    let body = Rect::new(
        area.x,
        area.y,
        area.width,
        area.height.saturating_sub(footer_rows + 1),
    );

    let sidebar_width = nightlight_sidebar_width(mode).min(body.width);
    let sidebar = Rect::new(body.x, body.y, sidebar_width, body.height);
    let content_x = sidebar.right().saturating_add(1).min(body.right());
    let content = Rect::new(
        content_x,
        body.y,
        body.right().saturating_sub(content_x),
        body.height,
    );

    ChromeRegions {
        mode,
        header: Rect::new(sidebar.x, sidebar.y, sidebar.width, 1),
        tabs: sidebar_nav_rect(sidebar),
        context: Rect::default(),
        body,
        content,
        main: content,
        footer,
        modal: None,
    }
}

/// Nav block inside the sidebar column: below the brand row and the blank line
/// after it, above the three-row summary plus its separator.
pub fn sidebar_nav_rect(sidebar: Rect) -> Rect {
    const BRAND_ROWS: u16 = 2;
    const SUMMARY_RESERVE: u16 = 4;
    Rect::new(
        sidebar.x,
        sidebar.y.saturating_add(BRAND_ROWS),
        sidebar.width,
        sidebar.height.saturating_sub(BRAND_ROWS + SUMMARY_RESERVE),
    )
}

/// The framed shell. The footer is a two-band block: key hints on top, the status
/// line underneath, the same rhythm `creeper` and `nightlight` use, so switching
/// presets does not move the status bar.
pub fn layout_chrome(area: Rect) -> ChromeRegions {
    layout_chrome_with_footer(area, 2)
}

/// Same layout with an explicit footer height. A terminal too small for the
/// frame keeps a single row, because 24 lines minus the header leaves nothing
/// to give.
pub fn layout_chrome_with_footer(area: Rect, footer_rows: u16) -> ChromeRegions {
    let mode = layout_mode(area);
    if mode == LayoutMode::TooSmall {
        let footer = Rect::new(
            area.x,
            area.y + area.height.saturating_sub(1),
            area.width,
            1,
        );
        return ChromeRegions {
            mode,
            header: Rect::new(area.x, area.y, area.width, 1),
            tabs: Rect::default(),
            context: Rect::default(),
            body: area,
            content: area,
            main: area,
            footer,
            modal: None,
        };
    }

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .margin(1)
        .split(area);
    let header = outer[0];
    let body = outer[1];

    let context_width = if mode == LayoutMode::Full { 32 } else { 0 };
    let header_inner = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(context_width)])
        .margin(1)
        .split(header);

    let inner = body.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    let footer_rows = footer_rows.clamp(1, inner.height.max(1));
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(footer_rows)])
        .split(inner);
    let content = rows[0];
    let footer = rows[1];

    let main = content;

    ChromeRegions {
        mode,
        header,
        tabs: header_inner[0],
        context: header_inner[1],
        body,
        content,
        main,
        footer,
        modal: None,
    }
}

/// Rows the paper shell spends above the page: running head, hairline rule,
/// chapter row, one blank row of air.
pub const PAPER_HEADER_ROWS: u16 = 4;

/// Size class for the `paper` preset.
///
/// It needs less than the framed shell: its chrome is four rows instead of five
/// and it has no inner border, so a 56×14 terminal still shows a page.
pub fn layout_mode_paper(area: Rect) -> LayoutMode {
    if area.width < 56 || area.height < 14 {
        LayoutMode::TooSmall
    } else if area.width < 90 || area.height < 26 {
        LayoutMode::Narrow
    } else if area.width < 120 || area.height < 36 {
        LayoutMode::Compact
    } else {
        LayoutMode::Full
    }
}

/// The paper shell: running head, hairline rule, chapter row, blank row, page,
/// then rule + key hints + status.
///
/// Only `header`, `tabs` and `footer` are drawn by the preset itself; the rule
/// rows are decoration the renderer derives from them (the head's row + 1, the
/// footer's first row). `main` and `content` alias `body` because a paper page
/// never splits into cards.
pub fn layout_chrome_paper(area: Rect) -> ChromeRegions {
    layout_chrome_paper_with_footer(area, 3)
}

pub fn layout_chrome_paper_with_footer(area: Rect, footer_rows: u16) -> ChromeRegions {
    let mode = layout_mode_paper(area);
    if mode == LayoutMode::TooSmall {
        let footer = Rect::new(
            area.x,
            area.y + area.height.saturating_sub(1),
            area.width,
            1,
        );
        return ChromeRegions {
            mode,
            header: Rect::new(area.x, area.y, area.width, 1),
            tabs: Rect::default(),
            context: Rect::default(),
            body: area,
            content: area,
            main: area,
            footer,
            modal: None,
        };
    }

    // The margin is what makes the page read as a page: four cells of paper on
    // a wide terminal, two on a narrow one.
    let margin: u16 = if area.width >= 100 { 2 } else { 1 };
    let width = area.width.saturating_sub(margin.saturating_mul(2));
    let x = area.x.saturating_add(margin);
    let footer_rows = footer_rows
        .max(1)
        .min(area.height.saturating_sub(PAPER_HEADER_ROWS).max(1));
    let body_height = area
        .height
        .saturating_sub(PAPER_HEADER_ROWS)
        .saturating_sub(footer_rows);

    let header = Rect::new(x, area.y, width, 1);
    let tabs = Rect::new(x, area.y.saturating_add(2), width, 1);
    let body = Rect::new(
        x,
        area.y.saturating_add(PAPER_HEADER_ROWS),
        width,
        body_height,
    );
    let footer = Rect::new(
        x,
        area.y
            .saturating_add(area.height)
            .saturating_sub(footer_rows),
        width,
        footer_rows,
    );

    ChromeRegions {
        mode,
        header,
        tabs,
        context: Rect::default(),
        body,
        content: body,
        main: body,
        footer,
        modal: None,
    }
}

/// Height of the `shelf` wordmark on a full-size terminal.
pub const SHELF_WORDMARK_ROWS: u16 = 4;

/// Columns `tui-big-text` needs for `TMCL` at the Quadrant size. The paper
/// masthead uses the same 16-column face, so the two marks are the same width.
pub const SHELF_WORDMARK_COLS: u16 = 16;

/// Rows the `shelf` head takes: the wordmark on every size class that can
/// spare four rows.
///
/// `Compact` collapses to a single text line only when the page cannot spare the
/// rows; a 120×35 terminal, four rows short of `Full`, keeps the mark. Four rows
/// out of thirty still leaves the pages their room; `Narrow` keeps the plain line
/// because at 80×24 the block face would eat a quarter of the page and the
/// cover column is gone at that width anyway.
pub fn shelf_header_rows(mode: LayoutMode) -> u16 {
    match mode {
        LayoutMode::Full | LayoutMode::Compact => SHELF_WORDMARK_ROWS,
        LayoutMode::Narrow | LayoutMode::TooSmall => 1,
    }
}

/// Size class for the `shelf` preset.
///
/// It needs a little less than the framed shell because it has no outer frame,
/// but more than `paper` because the four-row wordmark and the two-band footer
/// both need room before they are worth showing.
pub fn layout_mode_shelf(area: Rect) -> LayoutMode {
    if area.width < 64 || area.height < 18 {
        LayoutMode::TooSmall
    } else if area.width < 96 || area.height < 30 {
        LayoutMode::Narrow
    } else if area.width < 120 || area.height < 38 {
        LayoutMode::Compact
    } else {
        LayoutMode::Full
    }
}

pub fn layout_chrome_shelf(area: Rect) -> ChromeRegions {
    layout_chrome_shelf_with_footer(area, 2)
}

/// The `shelf` shell: head, tab row, hairline, page, two-band footer.
///
/// The hairline lives on the row between `tabs` and `body`; the renderer can
/// derive it from those two rectangles, so it is not a field of its own.
pub fn layout_chrome_shelf_with_footer(area: Rect, footer_rows: u16) -> ChromeRegions {
    let mode = layout_mode_shelf(area);
    if mode == LayoutMode::TooSmall {
        let footer = Rect::new(
            area.x,
            area.y + area.height.saturating_sub(1),
            area.width,
            1,
        );
        return ChromeRegions {
            mode,
            header: Rect::new(area.x, area.y, area.width, 1),
            tabs: Rect::default(),
            context: Rect::default(),
            body: area,
            content: area,
            main: area,
            footer,
            modal: None,
        };
    }

    let margin: u16 = if area.width >= 100 { 2 } else { 1 };
    let x = area.x.saturating_add(margin);
    let width = area.width.saturating_sub(margin.saturating_mul(2));
    let header_rows = shelf_header_rows(mode);
    let tabs_y = area.y.saturating_add(header_rows);
    let body_y = tabs_y
        .saturating_add(2)
        .saturating_add(if mode == LayoutMode::Full { 1 } else { 0 });
    let footer_rows = footer_rows.max(1).min(
        area.height
            .saturating_sub(body_y.saturating_sub(area.y))
            .max(1),
    );
    let body_height = area
        .height
        .saturating_sub(body_y.saturating_sub(area.y))
        .saturating_sub(footer_rows);

    let header = Rect::new(x, area.y, width, header_rows);
    let tabs = Rect::new(x, tabs_y, width, 1);
    let body = Rect::new(x, body_y, width, body_height);
    let footer = Rect::new(
        x,
        area.y
            .saturating_add(area.height)
            .saturating_sub(footer_rows),
        width,
        footer_rows,
    );
    // The context block hangs beside the wordmark, so it exists exactly when
    // the wordmark does.
    let context = if header_rows >= SHELF_WORDMARK_ROWS {
        Rect::new(
            x.saturating_add(width / 2),
            area.y,
            width.saturating_sub(width / 2),
            header_rows,
        )
    } else {
        Rect::default()
    };

    ChromeRegions {
        mode,
        header,
        tabs,
        context,
        body,
        content: body,
        main: body,
        footer,
        modal: None,
    }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::{LayoutMode, layout_chrome, layout_mode};

    #[test]
    fn layout_modes_follow_the_documented_size_thresholds() {
        assert_eq!(layout_mode(Rect::new(0, 0, 120, 40)), LayoutMode::Full);
        assert_eq!(layout_mode(Rect::new(0, 0, 110, 35)), LayoutMode::Compact);
        assert_eq!(layout_mode(Rect::new(0, 0, 95, 29)), LayoutMode::Narrow);
        assert_eq!(layout_mode(Rect::new(0, 0, 79, 23)), LayoutMode::TooSmall);
    }

    #[test]
    fn full_layout_reserves_a_header_body_and_footer() {
        let regions = layout_chrome(Rect::new(0, 0, 120, 40));
        assert_eq!(regions.header.height, 3);
        assert_eq!(regions.content.height, 31);
        // Two bands: key hints above the status line.
        assert_eq!(regions.footer.height, 2);
        assert_eq!(regions.main, regions.content);
        assert_eq!(regions.body.y, regions.header.bottom());
    }

    #[test]
    fn narrow_layout_keeps_a_single_content_column() {
        let regions = layout_chrome(Rect::new(0, 0, 95, 29));
        assert_eq!(regions.mode, LayoutMode::Narrow);
        assert_eq!(regions.main, regions.content);
        assert_eq!(regions.content.height, 20);
    }

    #[test]
    fn minimum_size_still_leaves_fifteen_content_rows() {
        let regions = layout_chrome(Rect::new(0, 0, 80, 24));
        assert_eq!(regions.mode, LayoutMode::Narrow);
        // One row went to the second footer band; the table still gets 15
        // rows, well above the ten the design asks for.
        assert_eq!(regions.content.height, 15);
    }

    #[test]
    fn too_small_layout_keeps_a_footer_row() {
        let regions = layout_chrome(Rect::new(0, 0, 79, 23));
        assert_eq!(regions.mode, LayoutMode::TooSmall);
        assert_eq!(regions.footer.height, 1);
    }

    #[test]
    fn nightlight_full_layout_puts_the_brand_in_the_sidebar() {
        let regions = super::layout_chrome_nightlight(Rect::new(0, 0, 120, 40));
        assert_eq!(regions.mode, LayoutMode::Full);
        // Brand row is the sidebar's first row, not a banner.
        assert_eq!(regions.header.x, 0);
        assert_eq!(regions.header.y, 0);
        assert_eq!(regions.header.width, 26);
        assert_eq!(regions.header.height, 1);
        // Content starts one column right of the sidebar, on the first row.
        assert_eq!(regions.content.x, regions.header.right() + 1);
        assert_eq!(regions.content.x, 27);
        assert_eq!(regions.main, regions.content);
        assert_eq!(regions.main.y, 0);
        // No top-right context at all.
        assert!(regions.context.is_empty());
        // Footer spans the full width and owns the last row.
        assert_eq!(regions.footer.x, 0);
        assert_eq!(regions.footer.width, 120);
        assert_eq!(regions.footer.y, 39);
        // One row is left between the body and the footer for the hairline.
        assert_eq!(regions.body.bottom(), regions.footer.y - 1);
        // Navigation stops above the three-row summary.
        assert!(regions.tabs.y >= regions.header.bottom());
        assert!(regions.tabs.bottom() <= regions.body.bottom() - 3);
        assert!(regions.tabs.x + regions.tabs.width <= regions.header.right());
    }

    #[test]
    fn nightlight_narrow_keeps_an_icon_rail() {
        let regions = super::layout_chrome_nightlight(Rect::new(0, 0, 80, 24));
        assert_eq!(regions.mode, LayoutMode::Narrow);
        assert_eq!(regions.header.width, 6);
        assert_eq!(regions.content.x, 7);
        assert_eq!(regions.main, regions.content);
    }

    #[test]
    fn nightlight_footer_rows_shrink_the_body() {
        let one = super::layout_chrome_nightlight_with_footer(Rect::new(0, 0, 120, 40), 1);
        let two = super::layout_chrome_nightlight_with_footer(Rect::new(0, 0, 120, 40), 2);
        assert_eq!(one.body.height, two.body.height + 1);
        assert_eq!(two.footer.height, 2);
        assert_eq!(two.body.bottom(), two.footer.y - 1);
    }

    #[test]
    fn nightlight_too_small_still_has_a_footer() {
        let regions = super::layout_chrome_nightlight(Rect::new(0, 0, 63, 17));
        assert_eq!(regions.mode, LayoutMode::TooSmall);
        assert_eq!(regions.footer.height, 1);
        assert_eq!(regions.footer.width, 63);
    }

    #[test]
    fn paper_keeps_a_margin_and_three_footer_rows() {
        let regions = super::layout_chrome_paper(Rect::new(0, 0, 120, 40));
        assert_eq!(regions.mode, LayoutMode::Full);
        assert_eq!(regions.header.x, 2);
        assert_eq!(regions.header.width, 116);
        assert_eq!(regions.tabs.y, 2);
        assert_eq!(regions.body.y, super::PAPER_HEADER_ROWS);
        assert_eq!(regions.footer.height, 3);
        assert_eq!(regions.body.bottom(), regions.footer.y);
        assert_eq!(regions.main, regions.body);

        // A narrow terminal still gets a page, with a two-cell margin.
        let narrow = super::layout_chrome_paper(Rect::new(0, 0, 80, 24));
        assert_eq!(narrow.mode, LayoutMode::Narrow);
        assert_eq!(narrow.header.x, 1);
        assert!(narrow.body.height > 10);
    }

    #[test]
    fn shelf_full_layout_reserves_a_wordmark_tabs_and_two_footer_rows() {
        let regions = super::layout_chrome_shelf(Rect::new(0, 0, 120, 40));
        assert_eq!(regions.mode, LayoutMode::Full);
        assert_eq!(regions.header.height, 4);
        assert_eq!(regions.tabs.y, 4);
        assert_eq!(regions.body.y, 7);
        assert_eq!(regions.footer.height, 2);
        assert_eq!(regions.footer.y, 38);
        assert_eq!(regions.body.bottom(), regions.footer.y);
        assert!(regions.context.width > 0);
    }

    #[test]
    fn shelf_compact_keeps_the_block_wordmark_and_its_context() {
        // 120×35: four rows short of `Full`, and the page has room to spare. This
        // size keeps the block wordmark.
        let regions = super::layout_chrome_shelf(Rect::new(0, 0, 120, 35));
        assert_eq!(regions.mode, LayoutMode::Compact);
        assert_eq!(regions.header.height, 4);
        assert_eq!(regions.tabs.y, 4);
        assert!(!regions.context.is_empty());
        assert!(regions.body.height >= 20);
    }

    #[test]
    fn shelf_narrow_collapses_the_wordmark_to_one_row() {
        let regions = super::layout_chrome_shelf(Rect::new(0, 0, 80, 24));
        assert_eq!(regions.mode, LayoutMode::Narrow);
        assert_eq!(regions.header.height, 1);
        assert_eq!(regions.tabs.y, 1);
        assert_eq!(regions.body.y, 3);
        assert_eq!(regions.footer.height, 2);
        assert!(regions.context.is_empty());
    }

    #[test]
    fn shelf_too_small_still_has_a_footer() {
        let regions = super::layout_chrome_shelf(Rect::new(0, 0, 63, 17));
        assert_eq!(regions.mode, LayoutMode::TooSmall);
        assert_eq!(regions.footer.height, 1);
        assert_eq!(regions.footer.width, 63);
    }
}
