//! Palette, colour maths and text helpers shared by all three presets.
//!
//! The named colours are the `creeper` ladder, but the maths (`mix`, `dim`,
//! `pulse`) and the text helpers (`width`, `truncate`, `fit`, `wrap`) are what
//! every preset measures with — that is why this lives at `tui/` rather than
//! inside one preset's folder.
//!
//! Everything is expressed in RGB. Terminals without truecolour let crossterm
//! approximate the values instead of falling back to a 16-colour guess here.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

pub const BG: Color = Color::Rgb(10, 13, 20);
pub const SURFACE: Color = Color::Rgb(19, 24, 36);
pub const SURFACE_ALT: Color = Color::Rgb(26, 33, 48);
pub const SURFACE_HI: Color = Color::Rgb(33, 42, 60);
pub const BORDER: Color = Color::Rgb(46, 58, 78);
pub const BORDER_HI: Color = Color::Rgb(82, 102, 136);
pub const TEXT: Color = Color::Rgb(230, 236, 246);
pub const TEXT_DIM: Color = Color::Rgb(135, 149, 173);
pub const TEXT_FAINT: Color = Color::Rgb(82, 96, 120);
pub const GREEN: Color = Color::Rgb(120, 212, 99);
pub const GREEN_DARK: Color = Color::Rgb(55, 112, 54);
pub const LIME: Color = Color::Rgb(190, 246, 128);
pub const CYAN: Color = Color::Rgb(92, 205, 235);
pub const BLUE: Color = Color::Rgb(122, 160, 245);
pub const PURPLE: Color = Color::Rgb(176, 145, 245);
pub const GOLD: Color = Color::Rgb(245, 199, 91);
pub const RED: Color = Color::Rgb(240, 110, 120);
pub const PINK: Color = Color::Rgb(240, 150, 195);
pub const ORANGE: Color = Color::Rgb(245, 156, 92);

/// Blend u8 channels; `t` is clamped to 0..1.
pub fn lerp_u8(from: u8, to: u8, t: f64) -> u8 {
    let t = t.clamp(0.0, 1.0);
    (f64::from(from) + (f64::from(to) - f64::from(from)) * t)
        .round()
        .clamp(0.0, 255.0) as u8
}

/// Linear blend between two RGB colours. Non-RGB inputs return `b`.
pub fn mix(a: Color, b: Color, t: f64) -> Color {
    match (a, b) {
        (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) => {
            Color::Rgb(lerp_u8(ar, br, t), lerp_u8(ag, bg, t), lerp_u8(ab, bb, t))
        }
        _ => b,
    }
}

pub fn dim(color: Color, amount: f64) -> Color {
    mix(color, BG, amount.clamp(0.0, 1.0))
}

pub fn brighten(color: Color, amount: f64) -> Color {
    mix(color, Color::Rgb(255, 255, 255), amount.clamp(0.0, 1.0))
}

/// 0..1 triangle-ish ease used for glowing borders and badges.
pub fn pulse(t: f64, speed: f64) -> f64 {
    ((t * speed).sin() * 0.5 + 0.5).clamp(0.0, 1.0)
}

/// Overshooting ease used when a surface slides in.
pub fn ease_out_back(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    let c1 = 1.701_58;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

/// One span per character, colour ramping from `from` to `to`, with a soft
/// travelling highlight. Used for the wordmark and card headings.
pub fn gradient_spans(
    value: &str,
    from: Color,
    to: Color,
    bg: Color,
    phase: f64,
    bold: bool,
) -> Vec<Span<'static>> {
    let count = value.chars().count().max(1);
    value
        .chars()
        .enumerate()
        .map(|(index, ch)| {
            let t = index as f64 / (count.saturating_sub(1)).max(1) as f64;
            let shimmer = ((index as f64 * 0.55) - phase * 1.6).sin() * 0.5 + 0.5;
            let color = brighten(mix(from, to, t), shimmer.powf(8.0) * 0.25);
            let mut style = Style::default().fg(color).bg(bg);
            if bold {
                style = style.add_modifier(Modifier::BOLD);
            }
            Span::styled(ch.to_string(), style)
        })
        .collect()
}

/// Terminal width of a string, counting CJK and emoji as two cells.
pub fn width(value: &str) -> usize {
    Line::from(value).width()
}

/// Truncate to `max` terminal cells, ending with an ellipsis when cut.
pub fn truncate(value: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if width(value) <= max {
        return value.to_owned();
    }
    let budget = max.saturating_sub(1);
    let mut out = String::new();
    let mut used = 0;
    for ch in value.chars() {
        let cell = width(&ch.to_string());
        if used + cell > budget {
            break;
        }
        out.push(ch);
        used += cell;
    }
    out.push('…');
    out
}

/// Pad with spaces up to `target` cells, never truncating.
pub fn pad(value: &str, target: usize) -> String {
    let used = width(value);
    if used >= target {
        value.to_owned()
    } else {
        format!("{value}{}", " ".repeat(target - used))
    }
}

/// Trim a string to `max` cells and pad it to exactly that width.
pub fn fit(value: &str, target: usize) -> String {
    pad(&truncate(value, target), target)
}

/// `1h 04m` / `12m 03s` label for an elapsed span.
pub fn format_elapsed(seconds: u64) -> String {
    if seconds >= 3600 {
        format!("{}h {:02}m", seconds / 3600, (seconds % 3600) / 60)
    } else if seconds >= 60 {
        format!("{}m {:02}s", seconds / 60, seconds % 60)
    } else {
        format!("{seconds}s")
    }
}

/// Collapse control characters so partial log lines stay on one row.
pub fn one_line(value: &str, max: usize) -> String {
    truncate(clean(value).trim(), max)
}

/// Replace newlines, tabs and other control characters with spaces.
fn clean(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch == '\n' || ch == '\r' || ch == '\t' || ch.is_control() {
                ' '
            } else {
                ch
            }
        })
        .collect()
}

/// Soft-wrap `value` into lines of at most `max` cells.
///
/// A space in the second half of the line is used as the break point so Latin
/// text keeps its words; CJK text, which has no spaces, may break anywhere.
pub fn wrap(value: &str, max: usize) -> Vec<String> {
    let max = max.max(1);
    let cleaned = clean(value);
    let mut lines = Vec::new();
    let mut line: Vec<(char, usize)> = Vec::new();
    let mut used = 0usize;
    for ch in cleaned.chars() {
        let cell = width(&ch.to_string());
        if used + cell > max && !line.is_empty() {
            let space = line
                .iter()
                .rposition(|(candidate, _)| *candidate == ' ')
                .filter(|index| *index * 2 >= line.len());
            let rest = if let Some(index) = space {
                let rest = line.split_off(index + 1);
                line.truncate(index);
                rest
            } else {
                Vec::new()
            };
            lines.push(
                line.iter()
                    .map(|(character, _)| *character)
                    .collect::<String>()
                    .trim_end()
                    .to_owned(),
            );
            used = rest.iter().map(|(_, cells)| *cells).sum();
            line = rest;
        }
        line.push((ch, cell));
        used += cell;
    }
    let tail = line
        .iter()
        .map(|(character, _)| *character)
        .collect::<String>();
    if !tail.trim().is_empty() {
        lines.push(tail.trim_end().to_owned());
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixing_moves_between_endpoints() {
        assert_eq!(mix(GREEN, CYAN, 0.0), GREEN);
        assert_eq!(mix(GREEN, CYAN, 1.0), CYAN);
        assert_eq!(mix(BG, BG, 0.5), BG);
        assert_ne!(mix(BG, TEXT, 0.5), BG);
    }

    #[test]
    fn easing_stays_bounded() {
        for t in [-1.0, 0.0, 0.25, 0.5, 1.0, 2.0] {
            assert!((0.0..=1.0).contains(&pulse(t, 2.0)));
        }
        assert!((ease_out_back(0.0)).abs() < 1e-9);
        assert!((ease_out_back(1.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn truncation_respects_double_width_characters() {
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate("abc", 4), "abc");
        assert_eq!(width("实例"), 4);
        assert_eq!(truncate("实例列表", 5), "实例…");
        assert_eq!(fit("ab", 4), "ab  ");
    }

    #[test]
    fn gradient_returns_one_span_per_character() {
        let spans = gradient_spans("hello", GREEN, CYAN, SURFACE, 1.0, true);
        assert_eq!(spans.len(), 5);
        assert!(spans.iter().all(|span| span.style.fg.is_some()));
    }

    #[test]
    fn wrap_breaks_on_spaces_and_counts_cells() {
        let text = "Lightweight and modular API providing common hooks";
        let lines = wrap(text, 20);
        assert!(lines.iter().all(|line| width(line) <= 20));
        assert!(lines.iter().all(|line| !line.starts_with(' ')));
        // Words survive the wrap instead of being cut in half.
        assert_eq!(lines.join(" "), text);

        let cjk = wrap("僵尸入侵一百天高版本惊变重制兼容原版存档", 8);
        assert!(cjk.iter().all(|line| width(line) <= 8));
        assert!(cjk.len() > 1);
    }
}
