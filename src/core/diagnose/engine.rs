//! The text a rule is matched against, and the six shapes it can ask for.
//!
//! A log line arrives with two prefixes already on it: TMCL writes `[stdout] `,
//! and the game writes `[19:02:11] [main/ERROR]: `. Anchoring a matcher at the
//! start of the line therefore finds nothing, so [`Matcher::After`] searches
//! anywhere in the line and then takes the text that follows.

use super::rule::Matcher;
use super::{Capture, Rule};

/// How far apart the two markers of a [`Matcher::Pair`] may sit. One entity
/// crash writes both lines next to each other; an hour-old `Entity Type:`
/// further up the log must not pair with today's coordinates.
pub const PAIR_WINDOW: usize = 60;

/// Longest captured value kept. Mod names are short; a stack trace is not.
const MAX_VALUE: usize = 240;

/// Lines plus their ASCII-lowercase twin.
///
/// Lowercasing ASCII never changes a byte length, so an offset found in `lower`
/// also points at the right place in `lines`. The matcher can therefore read the
/// original spelling (a Chinese mod name survives) without a regex engine's index
/// bookkeeping.
#[derive(Debug, Clone, Default)]
pub struct Text {
    lines: Vec<String>,
    lower: Vec<String>,
    crash_start: Option<usize>,
}

impl Text {
    pub fn parse(text: &str) -> Self {
        let lines = text
            .lines()
            .map(|line| line.trim_end_matches('\r').to_owned())
            .collect::<Vec<_>>();
        let lower = lines.iter().map(|line| line.to_ascii_lowercase()).collect();
        let crash_start = lines
            .iter()
            .position(|line| line.contains("---- Minecraft Crash Report ----"));
        Self {
            lines,
            lower,
            crash_start,
        }
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn line(&self, index: usize) -> &str {
        self.lines.get(index).map_or("", String::as_str)
    }

    /// The same line, ASCII-lowercased. Cached at parse time so a caller can
    /// test several needles without allocating.
    pub fn lower_line(&self, index: usize) -> &str {
        self.lower.get(index).map_or("", String::as_str)
    }

    pub fn crash_start(&self) -> Option<usize> {
        self.crash_start
    }

    /// The line without the stream prefix TMCL adds when it writes the log.
    pub fn payload(line: &str) -> &str {
        line.strip_prefix("[stdout] ")
            .or_else(|| line.strip_prefix("[stderr] "))
            .unwrap_or(line)
    }
}

/// One rule's answer: where it hit and what it pulled out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// First line that carried the signal; drives the position bonus.
    pub line: usize,
    pub captures: Vec<(Capture, String)>,
    /// Lines worth showing. At most three survive into the finding.
    pub evidence: Vec<usize>,
}

pub fn scan(text: &Text, rule: &Rule) -> Option<Hit> {
    if text.is_empty() {
        return None;
    }
    match rule.matcher {
        Matcher::Any(needles) => any(text, needles),
        Matcher::All(needles) => all(text, needles),
        Matcher::After {
            needle,
            end,
            capture,
        } => after(text, needle, end, capture),
        Matcher::Between {
            open,
            close,
            capture,
        } => between(text, open, close, capture),
        Matcher::Pair {
            first,
            second,
            a,
            b,
        } => pair(text, first, second, a, b),
        Matcher::Block { start, max } => block(text, start, max),
    }
}

fn lowered(values: &[&str]) -> Vec<String> {
    values
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect()
}

fn any(text: &Text, needles: &[&str]) -> Option<Hit> {
    let needles = lowered(needles);
    let mut evidence = Vec::new();
    for index in 0..text.len() {
        let line = text.lower_line(index);
        if needles.iter().any(|needle| line.contains(needle.as_str())) {
            evidence.push(index);
            if evidence.len() == 3 {
                break;
            }
        }
    }
    let line = *evidence.first()?;
    Some(Hit {
        line,
        captures: Vec::new(),
        evidence,
    })
}

fn all(text: &Text, needles: &[&str]) -> Option<Hit> {
    let mut evidence = Vec::new();
    for needle in needles {
        let needle = needle.to_ascii_lowercase();
        let index = (0..text.len()).find(|&index| text.lower_line(index).contains(&needle))?;
        evidence.push(index);
    }
    evidence.sort_unstable();
    evidence.dedup();
    evidence.truncate(3);
    let line = *evidence.first()?;
    Some(Hit {
        line,
        captures: Vec::new(),
        evidence,
    })
}

fn after(text: &Text, needle: &str, end: Option<&str>, capture: Capture) -> Option<Hit> {
    let needle_lower = needle.to_ascii_lowercase();
    for index in 0..text.len() {
        let lower = text.lower_line(index);
        let Some(offset) = lower.find(&needle_lower) else {
            continue;
        };
        let original = text.line(index);
        let rest = original.get(offset + needle.len()..).unwrap_or("");
        let value = match end {
            Some(end) => rest.split_once(end).map_or(rest, |(value, _)| value),
            None => rest,
        };
        let value = clean(value);
        if value.is_empty() {
            continue;
        }
        // A class file says `61.0`; the player needs "Java 17". The mapping
        // (major − 44) has held since Java 1.2, and doing it here keeps the
        // number out of every message template.
        let value = match capture {
            Capture::ExpectedJava => match value.parse::<u32>() {
                Ok(major) if major >= 45 => (major - 44).to_string(),
                _ => value,
            },
            _ => value,
        };
        return Some(Hit {
            line: index,
            captures: vec![(capture, value)],
            evidence: vec![index],
        });
    }
    None
}

fn between(text: &Text, open: &str, close: &str, capture: Capture) -> Option<Hit> {
    let open_lower = open.to_ascii_lowercase();
    let close_lower = close.to_ascii_lowercase();
    for index in 0..text.len() {
        let lower = text.lower_line(index);
        let Some(start) = lower.find(&open_lower) else {
            continue;
        };
        let body_start = start + open.len();
        let Some(relative_end) = lower
            .get(body_start..)
            .and_then(|rest| rest.find(&close_lower))
        else {
            continue;
        };
        let body = text
            .line(index)
            .get(body_start..body_start + relative_end)
            .unwrap_or("");
        let value = clean(body);
        if value.is_empty() {
            continue;
        }
        return Some(Hit {
            line: index,
            captures: vec![(capture, value)],
            evidence: vec![index],
        });
    }
    None
}

fn pair(text: &Text, first: &str, second: &str, a: Capture, b: Capture) -> Option<Hit> {
    let first_lower = first.to_ascii_lowercase();
    let second_lower = second.to_ascii_lowercase();
    for start in 0..text.len() {
        if !text.lower_line(start).contains(&first_lower) {
            continue;
        }
        let end = (start + PAIR_WINDOW).min(text.len());
        for index in start..end {
            if !text.lower_line(index).contains(&second_lower) {
                continue;
            }
            let first_value = value_after_colon(text.line(start), first);
            let second_value = value_after_colon(text.line(index), second);
            if first_value.is_empty() || second_value.is_empty() {
                continue;
            }
            return Some(Hit {
                line: start,
                captures: vec![(a, first_value), (b, second_value)],
                evidence: vec![start, index],
            });
        }
    }
    None
}

fn block(text: &Text, start: &str, max: usize) -> Option<Hit> {
    let start_lower = start.to_ascii_lowercase();
    let index = (0..text.len()).find(|&index| text.lower_line(index).contains(&start_lower))?;
    let mut evidence = vec![index];
    for next in index + 1..text.len() {
        if evidence.len() > max {
            break;
        }
        let payload = Text::payload(text.line(next));
        if payload.trim().is_empty() {
            continue;
        }
        if !payload.starts_with([' ', '\t']) {
            break;
        }
        evidence.push(next);
    }
    Some(Hit {
        line: index,
        captures: Vec::new(),
        evidence,
    })
}

/// Text after `marker` on this line, up to a `:` separator.
///
/// `Block: minecraft:stone` must not be split at the namespace colon, so the
/// separator is `": "` whenever it exists.
fn value_after_colon(line: &str, marker: &str) -> String {
    let payload = Text::payload(line);
    let marker_lower = marker.to_ascii_lowercase();
    let lower = payload.to_ascii_lowercase();
    let Some(offset) = lower.find(&marker_lower) else {
        return String::new();
    };
    let rest = payload.get(offset + marker.len()..).unwrap_or("");
    let rest = rest
        .strip_prefix(": ")
        .or_else(|| rest.strip_prefix(':'))
        .unwrap_or(rest);
    clean(rest)
}

fn clean(value: &str) -> String {
    let value = value.trim().trim_matches('"').trim();
    if value.chars().count() <= MAX_VALUE {
        return value.to_owned();
    }
    let mut kept = value.chars().take(MAX_VALUE).collect::<String>();
    kept.push('…');
    kept
}

#[cfg(test)]
mod tests {
    use super::{Hit, Text, scan};
    use crate::core::diagnose::rule::{Matcher, r};
    use crate::core::diagnose::{Capture, Category, Family, Platform, RuleId, StageMask};
    use crate::core::launch::failure::Action;

    fn sample(matcher: Matcher) -> crate::core::diagnose::Rule {
        r(
            RuleId::OutOfMemory,
            Category::Memory,
            Family::Memory,
            StageMask::ALL,
            5,
            matcher,
            "diagnose.reason.out_of_memory",
            "diagnose.advice.raise_memory",
            &[Action::RaiseMemory],
        )
    }

    #[test]
    fn any_reports_the_first_matching_lines() {
        let text = Text::parse("one\ntwo boom\nthree\nboom again");
        let hit = scan(&text, &sample(Matcher::Any(&["boom"]))).expect("hit");
        assert_eq!(hit.line, 1);
        assert_eq!(hit.evidence, [1, 3]);
    }

    #[test]
    fn all_requires_every_needle() {
        let text = Text::parse("alpha\nbeta");
        assert!(scan(&text, &sample(Matcher::All(&["alpha", "beta"]))).is_some());
        assert!(scan(&text, &sample(Matcher::All(&["alpha", "gamma"]))).is_none());
    }

    #[test]
    fn after_takes_the_text_after_the_marker_despite_the_log_prefixes() {
        let text = Text::parse(
            "[stdout] [19:02:11] [main/ERROR]: Found a duplicate mod create at mods/create.jar",
        );
        let hit = scan(
            &text,
            &sample(Matcher::After {
                needle: "Found a duplicate mod ",
                end: Some(" at "),
                capture: Capture::Mod,
            }),
        )
        .expect("hit");
        assert_eq!(hit.captures, [(Capture::Mod, "create".to_owned())]);
    }

    #[test]
    fn after_rejects_an_empty_value() {
        let text = Text::parse("[stdout] Found a duplicate mod ");
        assert!(
            scan(
                &text,
                &sample(Matcher::After {
                    needle: "Found a duplicate mod ",
                    end: Some(" at "),
                    capture: Capture::Mod,
                }),
            )
            .is_none()
        );
    }

    #[test]
    fn between_keeps_the_namespace_colon() {
        let text = Text::parse("[stdout] Block: minecraft:stone has crashed");
        let hit = scan(
            &text,
            &sample(Matcher::Between {
                open: "Block: ",
                close: " has crashed",
                capture: Capture::EntityType,
            }),
        )
        .expect("hit");
        assert_eq!(
            hit.captures,
            [(Capture::EntityType, "minecraft:stone".to_owned())]
        );
    }

    #[test]
    fn pair_needs_both_markers_within_the_window() {
        let close =
            "[stdout] Entity Type: minecraft:zombie\n[stdout] Entity's Exact location: 1, 2, 3";
        let hit = scan(
            &Text::parse(close),
            &sample(Matcher::Pair {
                first: "Entity Type:",
                second: "Entity's Exact location:",
                a: Capture::EntityType,
                b: Capture::Location,
            }),
        )
        .expect("hit");
        assert_eq!(hit.captures[0].1, "minecraft:zombie");
        assert_eq!(hit.captures[1].1, "1, 2, 3");

        let far = format!(
            "{}\n{}",
            "[stdout] Entity Type: minecraft:zombie",
            "x\n".repeat(super::PAIR_WINDOW + 5) + "[stdout] Entity's Exact location: 1, 2, 3"
        );
        assert!(
            scan(
                &Text::parse(&far),
                &sample(Matcher::Pair {
                    first: "Entity Type:",
                    second: "Entity's Exact location:",
                    a: Capture::EntityType,
                    b: Capture::Location,
                }),
            )
            .is_none()
        );
    }

    #[test]
    fn block_collects_indented_payload_lines_and_stops_at_the_first_flat_one() {
        let text = Text::parse(
            "[stdout] Missing or unsupported mandatory dependencies:\n\
             [stdout] \tMod ID: 'cloth-config', Requested by: 'sodium-extra'\n\
             [stdout] \tMod ID: 'fabric-api', Requested by: 'sodium'\n\
             [stdout] Done",
        );
        let hit = scan(
            &text,
            &sample(Matcher::Block {
                start: "Missing or unsupported mandatory dependencies",
                max: 8,
            }),
        )
        .expect("hit");
        assert_eq!(hit.evidence.len(), 3);
        assert!(hit.evidence.contains(&1));
    }

    #[test]
    fn an_empty_text_never_hits() {
        assert!(scan(&Text::default(), &sample(Matcher::Any(&["x"]))).is_none());
        let _: Option<Hit> = None;
    }

    #[test]
    fn the_platform_argument_is_carried_into_the_rule() {
        let rule = super::super::rule::r_on(
            Platform::MacOs,
            RuleId::MacWindowServicePort,
            Category::Graphics,
            Family::Graphics,
            StageMask::IN_GAME,
            4,
            Matcher::Any(&["GLFW error before init"]),
            "diagnose.reason.graphics_driver",
            "diagnose.advice.update_driver",
            &[],
        );
        assert_eq!(rule.platform, Platform::MacOs);
    }

    // ---- one shape at a time: a hit, a miss, and what it took -------------

    #[test]
    fn any_misses_when_no_needle_is_present() {
        assert!(scan(&Text::parse("one\ntwo"), &sample(Matcher::Any(&["boom"]))).is_none());
    }

    /// `Any` means any one of them, case-insensitively, and it names nothing.
    #[test]
    fn any_accepts_the_second_needle_alone() {
        let text = Text::parse("[stdout] Out Of Memory Error");
        let hit = scan(
            &text,
            &sample(Matcher::Any(&["nothing here", "out of memory error"])),
        )
        .expect("hit");
        assert_eq!(hit.line, 0);
        assert!(hit.captures.is_empty());
    }

    /// `All` keeps one evidence line per needle, in log order, and a line that
    /// satisfies both needles is counted once.
    #[test]
    fn all_reports_each_needle_once_in_order() {
        let hit = scan(
            &Text::parse("beta\nalpha\nbeta again"),
            &sample(Matcher::All(&["beta", "alpha"])),
        )
        .expect("hit");
        assert_eq!(hit.line, 0);
        assert_eq!(hit.evidence, [0, 1]);

        let same = scan(
            &Text::parse("alpha and beta on one line"),
            &sample(Matcher::All(&["alpha", "beta"])),
        )
        .expect("hit");
        assert_eq!(same.evidence, [0], "one line is one evidence entry");
    }

    /// Without an `end` marker the value runs to the end of the line.
    #[test]
    fn after_without_end_keeps_the_rest_of_the_line() {
        let hit = scan(
            &Text::parse("[stdout] Mod File: create-0.5.1.jar"),
            &sample(Matcher::After {
                needle: "Mod File: ",
                end: None,
                capture: Capture::Class,
            }),
        )
        .expect("hit");
        assert_eq!(
            hit.captures,
            [(Capture::Class, "create-0.5.1.jar".to_owned())]
        );
    }

    #[test]
    fn after_misses_when_the_needle_is_absent() {
        assert!(
            scan(
                &Text::parse("[stdout] nothing to see"),
                &sample(Matcher::After {
                    needle: "Mod File: ",
                    end: None,
                    capture: Capture::Class,
                }),
            )
            .is_none()
        );
    }

    #[test]
    fn between_needs_both_markers_on_one_line() {
        assert!(
            scan(
                &Text::parse("[stdout] Block: minecraft:stone"),
                &sample(Matcher::Between {
                    open: "Block: ",
                    close: " has crashed",
                    capture: Capture::EntityType,
                }),
            )
            .is_none(),
            "no close marker"
        );
        assert!(
            scan(
                &Text::parse("[stdout] minecraft:stone has crashed"),
                &sample(Matcher::Between {
                    open: "Block: ",
                    close: " has crashed",
                    capture: Capture::EntityType,
                }),
            )
            .is_none(),
            "no open marker"
        );
    }

    /// The captured body comes from the original line, not the lowered twin:
    /// byte offsets line up because ASCII lowering keeps the length, and a
    /// non-ASCII name has to survive the round trip.
    #[test]
    fn between_keeps_the_original_spelling() {
        let hit = scan(
            &Text::parse("[stdout] Block: minecraft:/u544a/u793a/u724c has crashed"),
            &sample(Matcher::Between {
                open: "Block: ",
                close: " has crashed",
                capture: Capture::EntityType,
            }),
        )
        .expect("hit");
        assert_eq!(
            hit.captures,
            [(
                Capture::EntityType,
                "minecraft:/u544a/u793a/u724c".to_owned()
            )]
        );
    }

    #[test]
    fn pair_misses_when_only_one_marker_is_there() {
        assert!(
            scan(
                &Text::parse("[stdout] Entity's Exact location: 1, 2, 3"),
                &sample(Matcher::Pair {
                    first: "Entity Type:",
                    second: "Entity's Exact location:",
                    a: Capture::EntityType,
                    b: Capture::Location,
                }),
            )
            .is_none()
        );
    }

    /// The header alone is still a hit: Forge prints the list, and a mod that
    /// printed none is still a missing dependency.
    #[test]
    fn block_reports_only_the_header_when_nothing_is_indented() {
        let hit = scan(
            &Text::parse("Missing or unsupported mandatory dependencies:\nDone"),
            &sample(Matcher::Block {
                start: "Missing or unsupported",
                max: 8,
            }),
        )
        .expect("hit");
        assert_eq!(hit.evidence, [0]);
    }

    #[test]
    fn block_caps_the_continuation_lines() {
        let tab = char::from(9u8);
        let mut body = String::from("Missing or unsupported mandatory dependencies:\n");
        for index in 0..20 {
            body.push_str(&format!("[stdout] {tab}Mod ID: 'mod{index}'\n"));
        }
        let hit = scan(
            &Text::parse(&body),
            &sample(Matcher::Block {
                start: "Missing or unsupported",
                max: 3,
            }),
        )
        .expect("hit");
        assert_eq!(hit.evidence.len(), 4, "the header plus three continuations");
    }
}
