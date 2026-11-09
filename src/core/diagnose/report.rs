//! The same diagnosis, as plain text.
//!
//! Used by the bundle's `report.txt` and by `tmcl diagnose`. The overlay paints
//! its own styled lines, but both take every sentence from the same keys: the
//! text lives in the catalog once, and only the layout exists twice.
//!
//! `core` cannot hold an `I18n`: `src/i18n.rs` depends on
//! `core::config::models::Language`, so the reverse dependency would be a cycle.
//! The front end therefore passes the two-argument formatter in.

use super::Diagnosis;
use super::rules::rule_for;

/// `I18n::format`'s shape: a key and its `{slot}` values.
pub type TextFn<'a> = &'a dyn Fn(&str, &[(&str, &str)]) -> String;

pub fn render(diagnosis: &Diagnosis, text: TextFn<'_>) -> String {
    let mut out = String::new();
    out.push_str(&text("diagnose.title", &[]));
    out.push('\n');

    let stage = text(diagnosis.stage.key(), &[]);
    let exit = text(diagnosis.exit.key(), &[]);
    out.push_str(&text(
        "diagnose.report.stage_exit",
        &[("stage", &stage), ("exit", &exit)],
    ));
    out.push('\n');
    out.push_str(&text(
        "diagnose.facts.summary",
        &[
            ("instance", &diagnosis.facts.instance_name),
            ("version", &diagnosis.facts.minecraft_version),
            ("loader", &diagnosis.facts.loader),
        ],
    ));
    out.push('\n');
    out.push_str(&text(
        "diagnose.facts.java",
        &[
            // "Java  · memory …" reads like a missing value, which it is: the
            // CLI does not probe a runtime the player never configured.
            ("java", java_version(diagnosis)),
            ("memory", &diagnosis.facts.max_memory_mb.to_string()),
            ("system", &diagnosis.facts.system_memory_mb.to_string()),
            ("mods", &diagnosis.facts.mod_count.to_string()),
        ],
    ));
    out.push('\n');

    if let Some(primary) = &diagnosis.primary {
        let rule = rule_for(primary.rule);
        out.push('\n');
        out.push_str(&section(text, "diagnose.section.conclusion"));
        out.push_str(&text(rule.reason_key, &primary.arguments()));
        out.push('\n');
        out.push_str(&section(text, "diagnose.section.advice"));
        out.push_str(&text(rule.advice_key, &primary.arguments()));
        out.push('\n');
        if !primary.evidence.is_empty() {
            out.push_str(&section(text, "diagnose.section.evidence"));
            for line in &primary.evidence {
                out.push_str("  ");
                out.push_str(&line.text);
                out.push('\n');
            }
        }
    } else if diagnosis.unknown {
        out.push('\n');
        out.push_str(&section(text, "diagnose.section.conclusion"));
        out.push_str(&text("diagnose.unknown.reason", &[]));
        out.push('\n');
        if !diagnosis.keywords.is_empty() {
            out.push_str(&section(text, "diagnose.section.keywords"));
            out.push_str(&text(
                "diagnose.unknown.keywords",
                &[("keywords", &diagnosis.keywords.join(", "))],
            ));
            out.push('\n');
        }
    } else {
        // Idle: a clean exit or a process that was killed. The card says which
        // (`diagnosis_conclusion`); without this branch a bundle exported from
        // such a run is a pile of facts with no answer in it.
        out.push('\n');
        out.push_str(&section(text, "diagnose.section.conclusion"));
        out.push_str(&text(diagnosis.exit.key(), &[]));
        out.push('\n');
    }

    if !diagnosis.others.is_empty() {
        out.push_str(&section(text, "diagnose.section.others"));
        for finding in &diagnosis.others {
            let rule = rule_for(finding.rule);
            out.push_str("  · ");
            out.push_str(&text(rule.reason_key, &finding.arguments()));
            out.push('\n');
        }
    }

    if !diagnosis.sources.is_empty() {
        out.push_str(&section(text, "diagnose.section.sources"));
        for source in &diagnosis.sources {
            if source.present {
                out.push_str(&text(
                    "diagnose.sources.present",
                    &[("path", &source.path), ("size", &size(source.bytes))],
                ));
            } else {
                out.push_str(&text("diagnose.sources.missing", &[("path", &source.path)]));
            }
            out.push('\n');
        }
    }
    collapse_blank_lines(out)
}

/// The Java version, or `-` when nobody could tell.
fn java_version(diagnosis: &Diagnosis) -> &str {
    if diagnosis.facts.java_version.is_empty() {
        "-"
    } else {
        &diagnosis.facts.java_version
    }
}

/// Runs of blank lines are a layout accident; two in a row is never intended.
fn collapse_blank_lines(text: String) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blanks = 0;
    for line in text.lines() {
        if line.trim().is_empty() {
            blanks += 1;
            if blanks > 1 {
                continue;
            }
        } else {
            blanks = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn section(text: TextFn<'_>, key: &str) -> String {
    let mut line = String::new();
    line.push('\n');
    line.push_str(&text(key, &[]));
    line.push('\n');
    line
}

/// `1.2 MiB`, `845 KiB`, `12 B`: the unit a player can compare to a file manager.
pub fn size(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    let bytes = bytes as f64;
    if bytes >= KIB * KIB {
        format!("{:.1} MiB", bytes / (KIB * KIB))
    } else if bytes >= KIB {
        format!("{:.0} KiB", bytes / KIB)
    } else {
        format!("{bytes:.0} B")
    }
}

#[cfg(test)]
mod tests {
    use super::{render, size};
    use crate::core::diagnose::finding::Finding;
    use crate::core::diagnose::{Diagnosis, ExitKind, Facts, LaunchStage, RuleId, Source};

    /// A stand-in catalog: every key renders as `key(...args)`, which is enough
    /// to prove the layout and the placeholders without a language file.
    fn text(key: &str, args: &[(&str, &str)]) -> String {
        if args.is_empty() {
            return key.to_owned();
        }
        let pairs = args
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join(",");
        format!("{key}({pairs})")
    }

    #[test]
    fn a_diagnosis_renders_every_section_in_order() {
        let diagnosis = Diagnosis {
            stage: LaunchStage::InGame,
            exit: ExitKind::Crashed,
            primary: Some(Finding {
                rule: RuleId::OutOfMemory,
                captures: Vec::new(),
                evidence: vec![crate::core::diagnose::finding::EvidenceLine {
                    line: 3,
                    text: "[stdout] java.lang.OutOfMemoryError".to_owned(),
                }],
                score: 53,
            }),
            others: Vec::new(),
            keywords: Vec::new(),
            facts: Facts {
                instance_name: "survival".to_owned(),
                minecraft_version: "1.21.1".to_owned(),
                loader: "fabric 0.16.9".to_owned(),
                java_version: "21.0.1".to_owned(),
                max_memory_mb: 2048,
                system_memory_mb: 16384,
                mod_count: 42,
                ..Facts::default()
            },
            sources: vec![Source {
                path: "game.log".to_owned(),
                present: true,
                bytes: 1024,
            }],
            unknown: false,
        };
        let report = render(&diagnosis, &|key, args| text(key, args));
        let order = [
            "diagnose.section.conclusion",
            "diagnose.section.advice",
            "diagnose.section.evidence",
            "diagnose.section.sources",
        ];
        let mut last = 0;
        for section in order {
            let at = report.find(section).expect("section");
            assert!(at >= last, "{section} out of order");
            last = at;
        }
        assert!(report.contains("diagnose.reason.out_of_memory"));
    }

    /// A missing source is stated, not omitted. The list exists so whoever reads
    /// the report can see what was looked for and not found; an empty list would
    /// leave them guessing whether the launcher tried.
    #[test]
    fn a_missing_source_is_marked_as_missing() {
        let diagnosis = Diagnosis {
            stage: LaunchStage::NoWindow,
            exit: ExitKind::Crashed,
            primary: None,
            others: Vec::new(),
            keywords: Vec::new(),
            facts: Facts::default(),
            sources: vec![
                Source {
                    path: "game.log".to_owned(),
                    present: true,
                    bytes: 2048,
                },
                Source {
                    path: "crash-reports/crash-1.txt".to_owned(),
                    present: false,
                    bytes: 0,
                },
            ],
            unknown: true,
        };
        let report = render(&diagnosis, &|key, args| text(key, args));
        assert!(
            report.contains("diagnose.sources.present(path=game.log,size=2 KiB)"),
            "{report}"
        );
        assert!(
            report.contains("diagnose.sources.missing(path=crash-reports/crash-1.txt)"),
            "{report}"
        );
        // Both lines belong to the sources section, and the present one comes
        // first because that is the order the evidence was collected in.
        let present = report.find("diagnose.sources.present").expect("present");
        let missing = report.find("diagnose.sources.missing").expect("missing");
        assert!(present < missing);
    }

    #[test]
    fn an_unknown_cause_prints_the_keywords_instead_of_a_reason() {
        let diagnosis = Diagnosis {
            stage: LaunchStage::NoWindow,
            exit: ExitKind::Crashed,
            primary: None,
            others: Vec::new(),
            keywords: vec!["flywheel".to_owned(), "embeddium".to_owned()],
            facts: Facts::default(),
            sources: Vec::new(),
            unknown: true,
        };
        let report = render(&diagnosis, &|key, args| text(key, args));
        assert!(report.contains("diagnose.unknown.reason"));
        assert!(report.contains("keywords=flywheel, embeddium"));
    }

    /// An idle diagnosis has no verdict to print, but it must still say how the run
    /// ended. The card does, and a bundle whose report stops after the facts answers
    /// nothing.
    #[test]
    fn an_idle_diagnosis_still_says_how_it_ended() {
        let diagnosis = Diagnosis {
            stage: LaunchStage::InGame,
            exit: ExitKind::Clean,
            primary: None,
            others: Vec::new(),
            keywords: Vec::new(),
            facts: Facts::default(),
            sources: Vec::new(),
            unknown: false,
        };
        let report = render(&diagnosis, &|key, args| text(key, args));
        assert!(report.contains("diagnose.section.conclusion"), "{report}");
        assert!(report.contains("diagnose.exit.clean"), "{report}");
    }

    #[test]
    fn sizes_use_the_unit_a_file_manager_shows() {
        assert_eq!(size(12), "12 B");
        assert_eq!(size(2048), "2 KiB");
        assert_eq!(size(1024 * 1024 * 3 / 2), "1.5 MiB");
    }
}
