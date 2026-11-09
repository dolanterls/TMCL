//! A rule is data: what to look for, where it may fire, and what to offer.
//!
//! There is no regex engine here on purpose. Most phenomena are a substring of one
//! line, and the handful that carry a value are "take the text after this marker".
//! Six shapes cover all 59 rules, and each shape can be tested on its own instead
//! of hiding inside a pattern string.

use crate::core::launch::failure::Action;

use super::{Capture, Category, Family, Platform, RuleId, StageMask};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Matcher {
    /// One of these appears anywhere in the scanned text.
    Any(&'static [&'static str]),
    /// All of these appear. Proximity is not expressed here: three alternative
    /// vendor DLLs are an `Any`, and whether the hit happened at the end is the
    /// scoring step's job.
    All(&'static [&'static str]),
    /// The line contains `needle`; the text after it (up to `end`, or the end
    /// of the line) is the captured value.
    ///
    /// Not anchored to the start of the line: on disk every line carries
    /// `[stdout] ` and the game puts `[19:02:11] [main/ERROR]:` in front too.
    After {
        needle: &'static str,
        end: Option<&'static str>,
        capture: Capture,
    },
    /// The text between two markers on the same line is the captured value.
    Between {
        open: &'static str,
        close: &'static str,
        capture: Capture,
    },
    /// Both markers appear within `WINDOW` lines of each other, each with the
    /// text after their colon. Used by the entity/block crashes.
    Pair {
        first: &'static str,
        second: &'static str,
        a: Capture,
        b: Capture,
    },
    /// Starting at `start`, collect up to `max` following lines whose payload
    /// begins with whitespace. Loader dependency trees look like this.
    Block { start: &'static str, max: usize },
}

impl Matcher {
    /// The slots this shape can fill.
    ///
    /// Used by the guard that keeps a message template and its matcher in
    /// step: a template may ask for `{mod}`, and this says whether the rule can
    /// ever produce it. A rule with `Any` names nothing, so it may only use
    /// templates without placeholders.
    pub fn captures(&self) -> Vec<Capture> {
        match self {
            Self::Any(_) | Self::All(_) | Self::Block { .. } => Vec::new(),
            Self::After { capture, .. } | Self::Between { capture, .. } => vec![*capture],
            Self::Pair { a, b, .. } => vec![*a, *b],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    pub id: RuleId,
    pub category: Category,
    pub family: Family,
    pub stages: StageMask,
    pub platform: Platform,
    /// 1..=5. Five means the signal cannot plausibly mean anything else.
    pub weight: u8,
    pub matcher: Matcher,
    pub reason_key: &'static str,
    pub advice_key: &'static str,
    pub remedies: &'static [super::Remedy],
}

/// Shorthand for the common case: every platform, no remedies of its own.
#[allow(clippy::too_many_arguments)]
pub(crate) const fn r(
    id: RuleId,
    category: Category,
    family: Family,
    stages: StageMask,
    weight: u8,
    matcher: Matcher,
    reason_key: &'static str,
    advice_key: &'static str,
    remedies: &'static [Action],
) -> Rule {
    r_on(
        Platform::Any,
        id,
        category,
        family,
        stages,
        weight,
        matcher,
        reason_key,
        advice_key,
        remedies,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) const fn r_on(
    platform: Platform,
    id: RuleId,
    category: Category,
    family: Family,
    stages: StageMask,
    weight: u8,
    matcher: Matcher,
    reason_key: &'static str,
    advice_key: &'static str,
    remedies: &'static [Action],
) -> Rule {
    Rule {
        id,
        category,
        family,
        stages,
        platform,
        weight,
        matcher,
        reason_key,
        advice_key,
        remedies,
    }
}

#[cfg(test)]
mod tests {
    use super::Matcher;
    use crate::core::diagnose::rules::RULES;

    #[test]
    fn every_rule_weighs_one_to_five() {
        for rule in RULES {
            assert!(
                (1..=5).contains(&rule.weight),
                "{:?} has weight {}",
                rule.id,
                rule.weight
            );
        }
    }

    #[test]
    fn every_queried_shape_names_at_least_one_needle() {
        for rule in RULES {
            match rule.matcher {
                Matcher::Any(needles) | Matcher::All(needles) => {
                    assert!(
                        !needles.is_empty(),
                        "{:?} has an empty needle list",
                        rule.id
                    )
                }
                Matcher::After { needle, .. } | Matcher::Block { start: needle, .. } => {
                    assert!(!needle.is_empty(), "{:?} has an empty needle", rule.id)
                }
                Matcher::Between { open, close, .. } => {
                    assert!(!open.is_empty() && !close.is_empty(), "{:?}", rule.id)
                }
                Matcher::Pair { first, second, .. } => {
                    assert!(!first.is_empty() && !second.is_empty(), "{:?}", rule.id)
                }
            }
        }
    }
}
