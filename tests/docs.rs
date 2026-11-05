//! Guards the two CLI manuals against drifting apart.
//!
//! The Chinese and English manuals describe the same tool, so they must have
//! the same sections in the same order. A count of headings alone would pass
//! if one section were replaced by another of the same size, so each heading's
//! number and the command names it mentions are compared in order.

use std::fs;
use std::path::Path;

fn manual(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("docs")
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// Headings of exactly this level, without the marker.
fn headings(document: &str, level: usize) -> Vec<String> {
    let marker = "#".repeat(level);
    document
        .lines()
        .filter_map(|line| line.strip_prefix(&marker))
        .filter(|rest| rest.starts_with(' '))
        .map(|rest| rest.trim().to_owned())
        .collect()
}

fn code_spans(text: &str) -> Vec<String> {
    let mut spans = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('`') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('`') else { break };
        spans.push(after[..end].to_owned());
        rest = &after[end + 1..];
    }
    spans
}

fn is_section_number(token: &str) -> bool {
    token.chars().any(|c| c.is_ascii_digit())
        && token.chars().all(|c| c.is_ascii_digit() || c == '.')
}

/// What a heading promises, independent of the language it is written in:
/// its section number and the inline code (command names) it names.
fn signature(heading: &str) -> (Option<String>, Vec<String>) {
    let number = heading
        .split_whitespace()
        .next()
        .filter(|token| is_section_number(token))
        .map(str::to_owned);
    (number, code_spans(heading))
}

#[test]
fn manual_sections_line_up_between_languages() {
    let chinese = manual("cli.md");
    let english = manual("cli.en.md");

    for level in [2, 3] {
        let zh = headings(&chinese, level);
        let en = headings(&english, level);
        assert_eq!(
            zh.len(),
            en.len(),
            "level {level} headings: the manuals have drifted apart"
        );
        assert!(
            zh.len() >= if level == 2 { 18 } else { 60 },
            "level {level} headings: the manuals look empty or truncated"
        );

        for (zh_heading, en_heading) in zh.iter().zip(&en) {
            assert_eq!(
                signature(zh_heading),
                signature(en_heading),
                "these headings do not describe the same section: {zh_heading:?} / {en_heading:?}"
            );
        }
    }
}

#[test]
fn manual_examples_and_code_blocks_match_between_languages() {
    let chinese = manual("cli.md");
    let english = manual("cli.en.md");

    let fences = |document: &str| {
        document
            .lines()
            .filter(|line| line.trim_start().starts_with("```"))
            .count()
    };
    assert_eq!(
        fences(&chinese),
        fences(&english),
        "the manuals have a different number of code blocks"
    );
    assert_eq!(fences(&chinese) % 2, 0, "a code block is never closed");

    let examples = |document: &str| {
        document
            .lines()
            .filter(|line| line.starts_with("tmcl "))
            .count()
    };
    assert!(
        examples(&chinese) >= 150,
        "the Chinese manual lost its examples"
    );
    assert_eq!(
        examples(&chinese),
        examples(&english),
        "the manuals have a different number of `tmcl` examples"
    );

    let table_rows = |document: &str| {
        document
            .lines()
            .filter(|line| line.starts_with('|'))
            .count()
    };
    assert_eq!(
        table_rows(&chinese),
        table_rows(&english),
        "the manuals have a different number of table rows"
    );
}
