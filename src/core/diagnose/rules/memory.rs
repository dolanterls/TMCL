//! Memory: the heap is gone, or the machine has none left.

use crate::core::launch::failure::{Action, UrlKey};

use super::super::rule::{Matcher, Rule, r};
use super::super::{Category, Family, RuleId, StageMask};

pub(super) const OUT_OF_MEMORY: Rule = r(
    RuleId::OutOfMemory,
    Category::Memory,
    Family::Memory,
    StageMask::ALL,
    5,
    Matcher::Any(&[
        "java.lang.OutOfMemoryError",
        "Out of Memory Error",
        "The system is out of physical RAM or swap space",
        "Too small maximum heap",
    ]),
    "diagnose.reason.out_of_memory",
    "diagnose.advice.raise_memory",
    &[Action::RaiseMemory],
);

pub(super) const SYSTEM_MEMORY_EXHAUSTED: Rule = r(
    RuleId::SystemMemoryExhausted,
    Category::Memory,
    Family::Memory,
    StageMask::STARTUP,
    5,
    Matcher::Any(&["There is insufficient memory for the Java Runtime Environment to continue"]),
    "diagnose.reason.system_memory",
    "diagnose.advice.lower_memory",
    &[Action::LowerMemory, Action::OpenUrl(UrlKey::Pagefile)],
);

#[cfg(test)]
mod tests {
    use super::super::super::rule_id::RuleId;
    use super::super::testkit::fires_on;

    #[test]
    fn a_heap_error_is_named() {
        assert_eq!(
            fires_on("[stdout] java.lang.OutOfMemoryError: Java heap space"),
            Some(RuleId::OutOfMemory)
        );
    }

    #[test]
    fn the_vm_refusing_to_start_for_memory_is_a_different_rule() {
        assert_eq!(
            fires_on("There is insufficient memory for the Java Runtime Environment to continue"),
            Some(RuleId::SystemMemoryExhausted)
        );
    }
}
