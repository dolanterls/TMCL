//! Large wordmark used on the overview page.
//!
//! The rendering approach follows binsider `src/tui/widgets/logo.rs`
//! (commit 7f0633d1, MIT OR Apache-2.0): a `tui-big-text` banner in the
//! accent color. Binsider uses `PixelSize::Sextant`; TMCL uses `Quadrant`
//! because the U+1FB00 block is missing from common Windows terminal fonts.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use tui_big_text::{BigTextBuilder, PixelSize};

use crate::tui::theme::Theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, theme: Theme) {
    let banner = BigTextBuilder::default()
        .pixel_size(PixelSize::Quadrant)
        .style(Style::default().fg(theme.accent))
        .lines(["tmcl.".into()])
        .centered()
        .build();
    frame.render_widget(banner, area);
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::render;
    use crate::tui::theme::Theme;

    #[test]
    fn logo_renders_block_glyphs_inside_its_area() {
        let backend = TestBackend::new(40, 8);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| render(frame, frame.area(), Theme::default()))
            .expect("draw logo");
        let rendered: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(
            rendered.chars().any(|character| matches!(
                character,
                '▘' | '▝' | '▖' | '▗' | '▀' | '▄' | '█' | '▚' | '▞'
            )),
            "logo produced no block glyphs: {rendered:?}"
        );
    }
}
