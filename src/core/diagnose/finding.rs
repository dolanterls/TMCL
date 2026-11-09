//! Scoring and ranking.
//!
//! One crash usually leaves several plausible sentences in the log: a mod that
//! warned about `OutOfMemoryError` an hour ago, the real exception at the bottom,
//! and the loader's own summary in the middle. Three things separate them: how
//! unmistakable the signal is (`weight`), where it happened (tail, inside the crash
//! report), and whether the rule managed to name something.

use serde::Serialize;

use super::engine::{self, Text};
use super::rules::{RULES, family_of, weight_of};
use super::{Capture, ExitKind, LaunchStage, RuleId};

/// How many lines of a hit are worth showing. Three fits the card without
/// turning it into a second log page.
const MAX_EVIDENCE: usize = 3;
const MAX_EVIDENCE_LEN: usize = 240;
/// A hit this close to the end is what the game was doing when it died.
const TAIL_LINES: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EvidenceLine {
    pub line: usize,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub rule: RuleId,
    pub captures: Vec<(Capture, String)>,
    pub evidence: Vec<EvidenceLine>,
    pub score: u16,
}

/// A value for a message template, e.g. `{mod}`.
impl Finding {
    pub fn argument(&self, slot: &str) -> Option<(&str, &str)> {
        self.captures
            .iter()
            .find(|(capture, _)| capture.slot() == slot)
            .map(|(capture, value)| (capture.slot(), value.as_str()))
    }

    /// Every capture as a `{slot}` argument, ready for `I18n::format`.
    pub fn arguments(&self) -> Vec<(&str, &str)> {
        self.captures
            .iter()
            .map(|(capture, value)| (capture.slot(), value.as_str()))
            .collect()
    }
}

pub fn collect(text: &Text, stage: LaunchStage, exit: ExitKind) -> Vec<Finding> {
    let host = super::Platform::host();
    let mut findings = Vec::new();
    for rule in RULES {
        if !rule.stages.contains(stage) || !rule.platform.allows(host) {
            continue;
        }
        let Some(hit) = engine::scan(text, rule) else {
            continue;
        };
        let mut score = u16::from(rule.weight) * 10;
        if hit.line + TAIL_LINES >= text.len() {
            score += 3;
        }
        if text.crash_start().is_some_and(|start| hit.line >= start) {
            score += 2;
        }
        if !hit.captures.is_empty() {
            score += 1;
        }
        if exit == ExitKind::NativeAbort
            && matches!(
                rule.family,
                super::Family::Graphics | super::Family::NativeClass
            )
        {
            score += 5;
        }
        findings.push(Finding {
            rule: rule.id,
            captures: hit.captures,
            evidence: evidence_lines(text, &hit.evidence),
            score,
        });
    }
    findings
}

fn evidence_lines(text: &Text, indices: &[usize]) -> Vec<EvidenceLine> {
    indices
        .iter()
        .take(MAX_EVIDENCE)
        .map(|&index| EvidenceLine {
            line: index,
            text: truncate(text.line(index)),
        })
        .collect()
}

fn truncate(line: &str) -> String {
    if line.chars().count() <= MAX_EVIDENCE_LEN {
        return line.to_owned();
    }
    let mut value = line.chars().take(MAX_EVIDENCE_LEN).collect::<String>();
    value.push('…');
    value
}

/// Whether a finding is allowed to be the headline.
///
/// A weight-1 rule that named nothing is the log saying "something happened", and
/// that must not outrank "no idea, here are keywords". No rule in the table sits
/// that low today, so the gate needs a test: the next vague rule added must not
/// take the top of the card.
fn may_be_headline(weight: u8, captures: usize) -> bool {
    weight >= 2 || captures > 0
}

/// One finding per family, then at most one primary and two alternatives.
pub fn rank(mut findings: Vec<Finding>) -> (Option<Finding>, Vec<Finding>) {
    findings.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| weight_of(right.rule).cmp(&weight_of(left.rule)))
    });

    let mut kept: Vec<Finding> = Vec::new();
    for finding in findings {
        let family = family_of(finding.rule);
        if kept.iter().any(|other| family_of(other.rule) == family) {
            continue;
        }
        kept.push(finding);
    }

    let mut primary = None;
    let mut others = Vec::new();
    for finding in kept {
        if primary.is_none() && may_be_headline(weight_of(finding.rule), finding.captures.len()) {
            primary = Some(finding);
        } else if others.len() < 2 {
            others.push(finding);
        }
    }
    if primary.is_none() {
        others.clear();
    }
    (primary, others)
}

#[cfg(test)]
mod tests {
    use super::{Finding, collect, may_be_headline, rank};
    use crate::core::diagnose::engine::Text;
    use crate::core::diagnose::rules::{RULES, family_of, weight_of};
    use crate::core::diagnose::{ExitKind, LaunchStage, RuleId};

    fn finding(rule: RuleId, score: u16, captures: usize) -> Finding {
        Finding {
            rule,
            captures: (0..captures)
                .map(|index| (crate::core::diagnose::Capture::Mod, format!("m{index}")))
                .collect(),
            evidence: Vec::new(),
            score,
        }
    }

    /// Filler no rule matches, so a test controls the only hit in the text.
    fn filler(rows: usize) -> String {
        (0..rows)
            .map(|index| format!("[stdout] [19:02:14] [main/INFO]: tick {index}\n"))
            .collect()
    }

    /// The score `collect` gives one rule, with the bonuses already applied.
    fn score(rule: RuleId, body: &str, stage: LaunchStage, exit: ExitKind) -> u16 {
        let text = Text::parse(body);
        collect(&text, stage, exit)
            .into_iter()
            .find(|finding| finding.rule == rule)
            .unwrap_or_else(|| panic!("{rule:?} did not fire:\n{body}"))
            .score
    }

    /// The base score a rule starts from, before any positional bonus.
    fn base(rule: RuleId) -> u16 {
        u16::from(weight_of(rule)) * 10
    }

    #[test]
    fn one_finding_per_family_survives() {
        let ranked = rank(vec![
            finding(RuleId::OptiFineForgeMismatch, 42, 0),
            finding(RuleId::PerformantOptiFineMixin, 33, 0),
        ]);
        assert_eq!(
            ranked.0.expect("primary").rule,
            RuleId::OptiFineForgeMismatch
        );
        assert!(ranked.1.is_empty());
        assert_eq!(
            family_of(RuleId::OptiFineForgeMismatch),
            family_of(RuleId::PerformantOptiFineMixin)
        );
    }

    #[test]
    fn a_captured_value_lifts_a_rule_over_an_equally_heavy_one() {
        // Both weigh four; naming the mod is worth one more point, and that is
        // the difference between "something is duplicated" and "delete create".
        let ranked = rank(vec![
            finding(RuleId::ForgeInstallIncomplete, 43, 0),
            finding(RuleId::DuplicateMod, 44, 1),
        ]);
        assert_eq!(ranked.0.expect("primary").rule, RuleId::DuplicateMod);
    }

    #[test]
    fn the_primary_comes_first_and_two_alternatives_follow() {
        let ranked = rank(vec![
            finding(RuleId::OutOfMemory, 50, 0),
            finding(RuleId::DuplicateMod, 40, 1),
            finding(RuleId::MixinApplyError, 30, 0),
            finding(RuleId::NoSuchMethod, 20, 1),
        ]);
        assert_eq!(ranked.0.expect("primary").rule, RuleId::OutOfMemory);
        assert_eq!(ranked.1.len(), 2);
        // Order matters: the alternatives are shown strongest first, and the
        // fourth finding is dropped because two is the cap.
        assert_eq!(ranked.1[0].rule, RuleId::DuplicateMod);
        assert_eq!(ranked.1[1].rule, RuleId::MixinApplyError);
    }

    /// Same rule, same signal, different distance from the end: the one the
    /// game printed last is what it was doing when it died.
    #[test]
    fn a_hit_near_the_end_scores_higher_than_the_same_hit_near_the_top() {
        let rule = RuleId::OutOfMemory;
        let line = "[stdout] java.lang.OutOfMemoryError: Java heap space\n";

        let at_top = format!("{line}{}", filler(500));
        assert_eq!(
            score(rule, &at_top, LaunchStage::InGame, ExitKind::Crashed),
            base(rule),
            "an early hit gets no positional bonus"
        );

        let at_bottom = format!("{}{line}", filler(500));
        assert_eq!(
            score(rule, &at_bottom, LaunchStage::InGame, ExitKind::Crashed),
            base(rule) + 3,
            "the tail is worth three"
        );
    }

    /// A crash report is the game's own account of the crash; a hit inside it
    /// outranks the same string printed before it.
    #[test]
    fn a_hit_inside_the_crash_report_outranks_one_before_it() {
        let rule = RuleId::OutOfMemory;
        let line = "[stdout] java.lang.OutOfMemoryError: Java heap space\n";
        let banner = "[stderr] ---- Minecraft Crash Report ----\n";

        let mut inside = String::from(banner);
        inside.push_str(&filler(248));
        inside.push_str(line);
        inside.push_str(&filler(250));
        assert_eq!(
            score(rule, &inside, LaunchStage::InGame, ExitKind::Crashed),
            base(rule) + 2
        );

        let mut before = filler(250);
        before.push_str(line);
        before.push_str(&filler(5));
        before.push_str(banner);
        before.push_str(&filler(244));
        assert_eq!(
            score(rule, &before, LaunchStage::InGame, ExitKind::Crashed),
            base(rule),
            "the bonus is for hits after the banner, not for the banner existing"
        );
    }

    /// Naming the offending mod is one more point than saying "something is
    /// duplicated", and that point decides ties.
    #[test]
    fn a_rule_that_named_the_mod_scores_one_more() {
        let rule = RuleId::DuplicateMod;
        let line = "[stdout] Found a duplicate mod create at mods/create-0.5.1.jar\n";
        let body = format!("{}{line}{}", filler(250), filler(250));
        assert_eq!(
            score(rule, &body, LaunchStage::NeverStarted, ExitKind::Crashed),
            base(rule) + 1
        );
    }

    /// A JVM-level abort (`0xC0000005`) is almost always the driver or a native
    /// library, so those families move ahead; memory findings do not.
    #[test]
    fn a_jvm_level_abort_pushes_graphics_and_native_causes_up() {
        let graphics = RuleId::GraphicsDriverCrash;
        let body = format!(
            "{}[stdout] Pixel format not accelerated\n{}",
            filler(250),
            filler(250)
        );
        assert_eq!(
            score(graphics, &body, LaunchStage::InGame, ExitKind::Crashed),
            base(graphics)
        );
        assert_eq!(
            score(graphics, &body, LaunchStage::InGame, ExitKind::NativeAbort),
            base(graphics) + 5
        );

        let memory = RuleId::OutOfMemory;
        let oom = format!(
            "{}[stdout] java.lang.OutOfMemoryError: Java heap space\n{}",
            filler(250),
            filler(250)
        );
        assert_eq!(
            score(memory, &oom, LaunchStage::InGame, ExitKind::NativeAbort),
            base(memory),
            "an abort does not make a memory finding more likely"
        );
    }

    /// A judgement call the table must not silently lose: "something happened"
    /// is not a headline, and "something happened to create" is.
    #[test]
    fn a_vague_hit_without_a_name_never_becomes_the_headline() {
        assert!(!may_be_headline(1, 0));
        assert!(may_be_headline(1, 1), "a name is enough at any weight");
        assert!(may_be_headline(2, 0));

        // No rule sits at weight 1 today, so the gate rejects nothing yet. If
        // one is added, this fails and forces the question to be asked again.
        assert!(
            RULES.iter().all(|rule| rule.weight >= 2),
            "the table grew a weight-1 rule; decide whether it may ever be the verdict"
        );
    }
}
