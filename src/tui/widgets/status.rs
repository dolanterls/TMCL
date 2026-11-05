use ratatui::{
    style::Style,
    text::{Line, Span},
};

use crate::tui::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Ready,
    Current,
    Running,
    Completed,
    Queued,
    Warning,
    Failed,
    Missing,
    Info,
}

impl StatusKind {
    fn glyph(self) -> &'static str {
        match self {
            Self::Ready => "●",
            Self::Current | Self::Running => "▶",
            Self::Completed => "✓",
            Self::Queued => "…",
            Self::Warning => "!",
            Self::Failed => "×",
            Self::Missing => "—",
            Self::Info => "·",
        }
    }

    fn style(self, theme: Theme) -> Style {
        let color = match self {
            Self::Ready | Self::Completed => theme.success,
            Self::Current | Self::Running | Self::Info => theme.label,
            Self::Queued | Self::Warning => theme.error,
            Self::Failed => theme.error,
            Self::Missing => theme.dim,
        };
        Style::default().fg(color)
    }
}

pub fn badge(kind: StatusKind, label: impl Into<String>) -> Line<'static> {
    badge_with_theme(kind, label, Theme::default())
}

pub fn badge_with_theme(kind: StatusKind, label: impl Into<String>, theme: Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(kind.glyph(), kind.style(theme)),
        Span::raw(format!(" {}", label.into())),
    ])
}

#[cfg(test)]
mod tests {
    use super::{StatusKind, badge};

    #[test]
    fn badges_include_a_symbol_and_label() {
        assert_eq!(badge(StatusKind::Ready, "Ready").to_string(), "● Ready");
        assert_eq!(badge(StatusKind::Failed, "Failed").to_string(), "× Failed");
    }
}
