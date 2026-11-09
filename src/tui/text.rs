//! Shared single-line text input.
//!
//! One editor ([`edit`]) and one renderer ([`render`]) serve every text field in
//! the interface: the inline search bars, the dialog form fields, the single-line
//! prompts and the picker filter. Four hand-rolled inputs would put the cursor in a
//! different place in each.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::tui::input;

/// Colours one field needs. Callers pass their preset's tokens, so `classic`
/// and `creeper` share the same renderer instead of growing their own.
#[derive(Debug, Clone, Copy)]
pub struct FieldStyle {
    pub text: Color,
    pub dim: Color,
    pub bg: Color,
    pub cursor_fg: Color,
    pub cursor_bg: Color,
}

/// Replaces control characters with a space.
///
/// `Buffer::set_stringn` filters them, but `Paragraph` writes graphemes
/// directly into cells, so a tab is stored as a cell symbol and printing it
/// moves the cursor to the next tab stop. The rest of the line is then drawn
/// outside the widget, and no later frame repaints those cells.
pub fn printable(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect()
}

/// Half a second on, half a second off.
///
/// `App::anim_time` freezes at zero when motion is disabled, so the block stays
/// solid. Every preset follows the same rule for `TMCL_NO_ANIM`.
pub fn blink_on(t: f64) -> bool {
    (t * 2.0).fract() < 0.55
}

/// Applies one editing key to `value` / `cursor`.
///
/// Returns true when the key belonged to the editor, even if it only moved the
/// cursor, so callers can stop it before their own bindings see it.
pub fn edit(value: &mut String, cursor: &mut usize, key: KeyEvent) -> bool {
    let length = value.chars().count();
    match key.code {
        KeyCode::Left => *cursor = (*cursor).saturating_sub(1),
        KeyCode::Right => *cursor = (*cursor).saturating_add(1).min(length),
        KeyCode::Home => *cursor = 0,
        KeyCode::End => *cursor = length,
        KeyCode::Backspace if *cursor > 0 => {
            *value = input::remove_before(value, *cursor);
            *cursor -= 1;
        }
        KeyCode::Delete if *cursor < length => *value = input::remove_at(value, *cursor),
        KeyCode::Char(character)
            if !character.is_control() && !key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            *value = input::insert_at(value, *cursor, character);
            *cursor += 1;
        }
        _ => return false,
    }
    true
}

/// Draws one field of `width` cells.
///
/// The focused field always shows its cursor: the visible window scrolls far enough
/// to keep the block inside, so a long path never hides the point where typing
/// lands. An unfocused field shows the tail of a long value, marked with `‹`,
/// because the end of a path is the informative part.
pub fn render(
    value: &str,
    cursor: usize,
    width: u16,
    focused: bool,
    blink: bool,
    style: FieldStyle,
    placeholder: &str,
) -> Line<'static> {
    let chars: Vec<char> = value.chars().collect();
    let width = usize::from(width.max(1));
    if !focused {
        return unfocused(&chars, width, style, placeholder);
    }

    let cursor = cursor.min(chars.len());
    let cursor_col: usize = chars[..cursor].iter().map(|ch| cell(*ch)).sum();
    // One column is kept for the block when the cursor sits at the end.
    let start = cursor_col.saturating_sub(width.saturating_sub(1));

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut column = 0usize;
    let mut used = 0usize;
    let mut cursor_drawn = false;

    for (index, ch) in chars.iter().enumerate() {
        let cells = cell(*ch);
        if column + cells <= start {
            column += cells;
            continue;
        }
        if column < start {
            // A wide glyph straddles the left edge: pad instead of drawing
            // half of it.
            let pad = start - column;
            spans.push(Span::styled(" ".repeat(pad), Style::default().bg(style.bg)));
            used += pad;
            column = start;
        }
        if used >= width {
            break;
        }
        if index == cursor {
            spans.push(cursor_cell(Some(*ch), blink, style));
            used += cells.max(1);
            column += cells;
            cursor_drawn = true;
            continue;
        }
        if used + cells > width {
            break;
        }
        spans.push(Span::styled(
            ch.to_string(),
            Style::default().fg(style.text).bg(style.bg),
        ));
        used += cells;
        column += cells;
    }

    if !cursor_drawn && used < width {
        spans.push(cursor_cell(None, blink, style));
    }
    Line::from(spans)
}

/// Cursor block: reversed cell when the blink is on, plain text when off.
fn cursor_cell(ch: Option<char>, blink: bool, style: FieldStyle) -> Span<'static> {
    let text = ch.map_or_else(|| " ".to_owned(), String::from);
    if blink {
        Span::styled(
            text,
            Style::default()
                .fg(style.cursor_fg)
                .bg(style.cursor_bg)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(text, Style::default().fg(style.text).bg(style.bg))
    }
}

fn unfocused(chars: &[char], width: usize, style: FieldStyle, placeholder: &str) -> Line<'static> {
    if chars.is_empty() {
        return Line::from(Span::styled(
            clip(placeholder, width),
            Style::default().fg(style.dim).bg(style.bg),
        ));
    }
    let total: usize = chars.iter().map(|ch| cell(*ch)).sum();
    if total <= width {
        return Line::from(Span::styled(
            chars.iter().collect::<String>(),
            Style::default().fg(style.text).bg(style.bg),
        ));
    }
    let mut tail: Vec<char> = Vec::new();
    let mut used = 1usize;
    for ch in chars.iter().rev() {
        let cells = cell(*ch);
        if used + cells > width {
            break;
        }
        tail.push(*ch);
        used += cells;
    }
    tail.reverse();
    let mut text = String::with_capacity(width);
    text.push('‹');
    text.extend(tail);
    Line::from(Span::styled(
        text,
        Style::default().fg(style.text).bg(style.bg),
    ))
}

fn clip(value: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for ch in value.chars() {
        let cells = cell(ch);
        if used + cells > width {
            break;
        }
        out.push(ch);
        used += cells;
    }
    out
}

/// Display width of one character in terminal cells.
fn cell(ch: char) -> usize {
    let mut buffer = [0u8; 4];
    Line::from(&*ch.encode_utf8(&mut buffer)).width()
}

#[cfg(test)]
mod tests {
    use super::{FieldStyle, blink_on, edit, render};
    use crate::tui::input;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::style::Color;

    fn style() -> FieldStyle {
        FieldStyle {
            text: Color::White,
            dim: Color::Gray,
            bg: Color::Black,
            cursor_fg: Color::Black,
            cursor_bg: Color::Green,
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn typing_inserts_at_the_cursor_not_at_the_end() {
        let mut value = String::from("ac");
        let mut cursor = 1;

        assert!(edit(&mut value, &mut cursor, key(KeyCode::Char('b'))));
        assert_eq!(value, "abc");
        assert_eq!(cursor, 2);

        assert!(edit(&mut value, &mut cursor, key(KeyCode::Left)));
        assert_eq!(cursor, 1);
        assert!(edit(&mut value, &mut cursor, key(KeyCode::Backspace)));
        assert_eq!(value, "bc");
        assert_eq!(cursor, 0);

        assert!(edit(&mut value, &mut cursor, key(KeyCode::Delete)));
        assert_eq!(value, "c");
    }

    #[test]
    fn home_and_end_jump_to_the_edges() {
        let mut value = String::from("minecraft");
        let mut cursor = 4;
        assert!(edit(&mut value, &mut cursor, key(KeyCode::Home)));
        assert_eq!(cursor, 0);
        assert!(edit(&mut value, &mut cursor, key(KeyCode::End)));
        assert_eq!(cursor, value.chars().count());
    }

    #[test]
    fn control_chords_are_left_to_the_caller() {
        let mut value = String::from("ab");
        let mut cursor = 1;
        let chord = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
        assert!(!edit(&mut value, &mut cursor, chord));
        assert_eq!(value, "ab");
    }

    #[test]
    fn focused_field_scrolls_to_keep_the_cursor_visible() {
        let value = "abcdefghij".repeat(4);
        let cursor = value.chars().count();
        let line = render(&value, cursor, 10, true, true, style(), "");
        assert_eq!(line.width(), 10, "the window must fill its cells exactly");
        let text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        // The block sits on the trailing cell, one past the last character.
        assert!(
            text.trim_end().ends_with('j'),
            "cursor follows the last character: {text:?}"
        );
        assert!(!text.contains('a'), "the head scrolled away: {text:?}");
    }

    #[test]
    fn cursor_at_the_start_shows_the_head() {
        let value = "abcdefghijklmnop";
        let line = render(value, 0, 6, true, true, style(), "");
        let text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(text.starts_with("abcde"), "{text:?}");
    }

    #[test]
    fn empty_field_draws_the_block_before_the_placeholder() {
        let line = render("", 0, 8, true, true, style(), "type here");
        let text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(text.starts_with(' '), "empty field shows a block: {text:?}");
    }

    #[test]
    fn wide_characters_do_not_break_the_window() {
        let value = "我的世界启动器";
        let cursor = value.chars().count();
        let line = render(value, cursor, 6, true, true, style(), "");
        assert!(
            line.width() <= 6,
            "wide glyphs overflowed: {}",
            line.width()
        );
    }

    #[test]
    fn unfocused_long_value_shows_its_tail() {
        let line = render("/home/user/.minecraft", 0, 10, false, true, style(), "");
        let text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(text.starts_with('‹'), "{text:?}");
        assert!(
            text.ends_with("craft") || text.ends_with("craft"),
            "{text:?}"
        );
    }

    #[test]
    fn blink_freezes_on_when_motion_is_disabled() {
        assert!(blink_on(0.0));
        assert!(blink_on(0.1));
        assert!(!blink_on(0.4));
    }

    #[test]
    fn insert_helpers_match_the_editor() {
        let mut value = String::from("ab");
        value = input::insert_at(&value, 1, 'x');
        assert_eq!(value, "axb");
    }
}
