//! Shared single-line search input helpers.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchTarget {
    Logs,
    VersionFilter,
    Resources,
    Settings,
    /// Filter of the instance workspace content list.
    Workspace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchState {
    pub target: SearchTarget,
    pub cursor: usize,
}

impl SearchState {
    pub fn new(target: SearchTarget) -> Self {
        Self { target, cursor: 0 }
    }
}

pub fn byte_index(value: &str, char_index: usize) -> usize {
    value
        .char_indices()
        .nth(char_index)
        .map_or(value.len(), |(index, _)| index)
}

pub fn insert_at(value: &str, char_index: usize, character: char) -> String {
    let index = byte_index(value, char_index);
    let mut out = value.to_owned();
    out.insert(index, character);
    out
}

pub fn remove_before(value: &str, char_index: usize) -> String {
    if char_index == 0 {
        return value.to_owned();
    }
    let start = byte_index(value, char_index - 1);
    let end = byte_index(value, char_index);
    let mut out = value.to_owned();
    out.replace_range(start..end, "");
    out
}

pub fn remove_at(value: &str, char_index: usize) -> String {
    let start = byte_index(value, char_index);
    let end = byte_index(value, char_index + 1);
    let mut out = value.to_owned();
    out.replace_range(start..end, "");
    out
}

pub fn push_char(value: &mut String, character: char) {
    value.push(character);
}

pub fn pop_char(value: &mut String) {
    value.pop();
}

#[cfg(test)]
mod tests {
    use super::{pop_char, push_char};

    #[test]
    fn push_and_pop_edit_the_value() {
        let mut value = String::from("ab");
        push_char(&mut value, 'c');
        assert_eq!(value, "abc");
        pop_char(&mut value);
        assert_eq!(value, "ab");
    }
}
