//! Files that do not match what the launcher installed.

use crate::core::launch::failure::Action;

use super::super::rule::{Matcher, Rule, r};
use super::super::{Capture, Category, Family, RuleId, StageMask};

pub(super) const FILE_DIGEST_MISMATCH: Rule = r(
    RuleId::FileDigestMismatch,
    Category::Integrity,
    Family::Integrity,
    StageMask::STARTUP,
    4,
    Matcher::Any(&["SHA1 digest error for", "signer information does not match"]),
    "diagnose.reason.file_digest",
    "diagnose.advice.repair",
    &[Action::Repair],
);

pub(super) const FILE_ALREADY_EXISTS: Rule = r(
    RuleId::FileAlreadyExists,
    Category::Integrity,
    Family::Integrity,
    StageMask::ALL,
    3,
    Matcher::After {
        needle: "java.nio.file.FileAlreadyExistsException: ",
        end: None,
        capture: Capture::File,
    },
    "diagnose.reason.file_exists",
    "diagnose.advice.file_exists",
    &[Action::OpenInstanceDirectory],
);

#[cfg(test)]
mod tests {
    use super::super::super::rule_id::RuleId;
    use super::super::testkit::fires_on;

    #[test]
    fn a_digest_error_asks_for_a_repair() {
        assert_eq!(
            fires_on(
                "[stdout] java.lang.SecurityException: SHA1 digest error for net/minecraft/client/Main.class"
            ),
            Some(RuleId::FileDigestMismatch)
        );
    }

    #[test]
    fn an_existing_file_names_the_file() {
        assert_eq!(
            fires_on("[stdout] java.nio.file.FileAlreadyExistsException: mods/create.jar"),
            Some(RuleId::FileAlreadyExists)
        );
    }
}
