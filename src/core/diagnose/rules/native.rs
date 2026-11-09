//! Missing classes, missing methods, missing native libraries.

use crate::core::launch::failure::{Action, UrlKey};

use super::super::rule::{Matcher, Rule, r};
use super::super::{Capture, Category, Family, RuleId, StageMask};

pub(super) const NATIVE_LIBRARY_MISSING: Rule = r(
    RuleId::NativeLibraryMissing,
    Category::Native,
    Family::NativeClass,
    StageMask::ALL,
    4,
    Matcher::After {
        needle: "Failed to locate library: ",
        end: None,
        capture: Capture::File,
    },
    "diagnose.reason.native_library",
    "diagnose.advice.check_java_path",
    // Repair for a missing library, and the system-locale page for the other
    // half of the advice: a path with non-ASCII characters is the usual reason
    // the library cannot be located at all.
    &[Action::Repair, Action::OpenUrl(UrlKey::Locale)],
);

pub(super) const NO_SUCH_METHOD: Rule = r(
    RuleId::NoSuchMethod,
    Category::Native,
    Family::NativeClass,
    StageMask::ALL,
    3,
    Matcher::After {
        needle: "java.lang.NoSuchMethodError: ",
        end: None,
        capture: Capture::Class,
    },
    "diagnose.reason.no_such_method",
    "diagnose.advice.repair",
    &[Action::Repair],
);

pub(super) const NO_CLASS_DEF_FOUND: Rule = r(
    RuleId::NoClassDefFound,
    Category::Native,
    Family::NativeClass,
    StageMask::ALL,
    3,
    Matcher::After {
        needle: "java.lang.NoClassDefFoundError: ",
        end: None,
        capture: Capture::Class,
    },
    "diagnose.reason.no_class_def",
    "diagnose.advice.repair",
    &[Action::Repair],
);

pub(super) const ILLEGAL_ACCESS: Rule = r(
    RuleId::IllegalAccess,
    Category::Native,
    Family::NativeClass,
    StageMask::ALL,
    3,
    Matcher::After {
        needle: "java.lang.IllegalAccessError: ",
        end: None,
        capture: Capture::Reason,
    },
    "diagnose.reason.illegal_access",
    "diagnose.advice.remove_mod",
    &[Action::OpenContentDirectory(
        crate::core::launch::failure::ContentDir::Mods,
    )],
);

#[cfg(test)]
mod tests {
    use super::super::super::rule_id::RuleId;
    use super::super::testkit::primary;

    #[test]
    fn a_missing_native_library_names_the_library() {
        let primary = primary(
            "[stdout] java.lang.UnsatisfiedLinkError: Failed to locate library: lwjgl_opengl.dll",
        )
        .expect("finding");
        assert_eq!(primary.rule, RuleId::NativeLibraryMissing);
        assert_eq!(primary.captures[0].1, "lwjgl_opengl.dll");
    }

    #[test]
    fn a_missing_class_names_the_class() {
        let primary = primary("[stdout] java.lang.NoClassDefFoundError: com/example/api/Thing")
            .expect("finding");
        assert_eq!(primary.rule, RuleId::NoClassDefFound);
        assert_eq!(primary.captures[0].1, "com/example/api/Thing");
    }
}
