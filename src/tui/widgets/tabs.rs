use ratatui::{layout::Rect, text::Line, widgets::Tabs};

const DIVIDER: &str = "|";
const PADDING_LEFT: &str = " ";
const PADDING_RIGHT: &str = " ";

/// A tab bar whose rendered widget and mouse hit-test share one geometry model.
///
/// Ratatui measures tab titles by terminal display width, not by the number of
/// Rust `char`s. Keeping the labels and spacing in this component prevents
/// translated or otherwise wide labels from drifting away from their click
/// targets.
///
/// The widget is always drawn without a block, so geometry is the area itself;
/// callers that want a frame draw it separately around the same rectangle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabBar {
    labels: Vec<String>,
}

impl TabBar {
    pub fn new(labels: Vec<String>) -> Self {
        Self { labels }
    }

    pub fn widget(&self) -> Tabs<'static> {
        Tabs::new(
            self.labels
                .iter()
                .cloned()
                .map(Line::from)
                .collect::<Vec<_>>(),
        )
        .divider(DIVIDER)
        .padding(PADDING_LEFT, PADDING_RIGHT)
    }

    /// Rectangle of every label, in draw order.
    ///
    /// This is what the renderer advances through, so registration cannot drift
    /// from the drawn tabs; only the label text itself is a target, exactly
    /// like the previous arithmetic hit test.
    pub fn rects(&self, area: Rect) -> Vec<Rect> {
        let mut cursor = usize::from(area.x) + Line::from(PADDING_LEFT).width();
        let right_padding_width = Line::from(PADDING_RIGHT).width();
        let divider_width = Line::from(DIVIDER).width();

        self.labels
            .iter()
            .map(|label| {
                let width = Line::from(label.as_str()).width();
                let rect = Rect::new(
                    u16::try_from(cursor).unwrap_or(u16::MAX),
                    area.y,
                    u16::try_from(width).unwrap_or(u16::MAX),
                    1,
                );
                cursor = cursor
                    .saturating_add(width)
                    .saturating_add(right_padding_width)
                    .saturating_add(divider_width)
                    .saturating_add(Line::from(PADDING_LEFT).width());
                rect
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{
        buffer::Buffer,
        layout::Rect,
        text::Line,
        widgets::{Block, Borders, Widget},
    };

    use super::TabBar;

    #[test]
    fn hit_testing_matches_ratatui_rendering_for_wide_labels() {
        let tabs = TabBar::new(vec![
            "游戏版本".into(),
            "模组".into(),
            "整合包".into(),
            "资源包".into(),
            "光影".into(),
            "世界".into(),
        ]);
        let area = Rect::new(10, 2, 100, 1);
        let mut buffer = Buffer::empty(area);
        tabs.widget().render(area, &mut buffer);

        let rects = tabs.rects(area);
        assert_eq!(rects.len(), tabs.labels.len());
        for (index, rect) in rects.iter().enumerate() {
            let label_width = Line::from(tabs.labels[index].as_str()).width();
            assert_eq!(usize::from(rect.width), label_width);
            let click_column = rect.x + rect.width / 2;
            assert!(buffer.cell((click_column, area.y)).is_some());
        }
        // Labels must not overlap, or two tabs would share a click.
        for pair in rects.windows(2) {
            assert!(pair[0].right() <= pair[1].x);
        }
    }

    #[test]
    fn hit_testing_starts_one_column_in_for_the_leading_pad() {
        let tabs = TabBar::new(vec!["All".into(), "运行中".into()]);
        let area = Rect::new(10, 2, 40, 1);

        // Padding is one cell wide, so the first label starts at area.x + 1.
        assert_eq!(tabs.rects(area)[0], Rect::new(area.x + 1, area.y, 3, 1));
        // A frame drawn around the same area is the caller's business; the
        // geometry above is what the widget itself paints.
        let mut buffer = Buffer::empty(area);
        Block::default()
            .borders(Borders::ALL)
            .render(Rect::new(area.x, area.y, area.width, 3), &mut buffer);
    }
}
