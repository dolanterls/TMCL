//! Java: wrong version, wrong vendor, wrong bitness.

use crate::core::launch::failure::{Action, UrlKey};

use super::super::rule::{Rule, r, r_on};
use super::super::{Category, Family, Platform, RuleId, StageMask, rule::Matcher};

const CHANGE_JAVA: &[Action] = &[Action::ChangeJava];
const INSTALL_JAVA: &[Action] = &[Action::InstallJava, Action::ChangeJava];
const LOWER_MEMORY: &[Action] = &[Action::LowerMemory, Action::ChangeJava];

pub(super) const OPENJ9: Rule = r(
    RuleId::OpenJ9,
    Category::Java,
    Family::JavaVendor,
    StageMask::STARTUP,
    5,
    Matcher::Any(&[
        "Open J9 is not supported",
        "OpenJ9 is incompatible",
        ".J9VMInternals.",
    ]),
    "diagnose.reason.java_vendor",
    "diagnose.advice.change_java",
    CHANGE_JAVA,
);

pub(super) const JAVA11_REQUIRED: Rule = r(
    RuleId::Java11Required,
    Category::Java,
    Family::JavaVersion,
    StageMask::STARTUP,
    4,
    Matcher::Any(&[
        "no such method: sun.misc.Unsafe.defineAnonymousClass",
        "The requested compatibility level JAVA_11 could not be set",
        "UnsupportedClassVersionError: icyllis/modernui",
    ]),
    "diagnose.reason.java11_required",
    "diagnose.advice.install_java11",
    INSTALL_JAVA,
);

pub(super) const JAVA_TOO_OLD: Rule = r(
    RuleId::JavaTooOld,
    Category::Java,
    Family::JavaVersion,
    StageMask::STARTUP,
    5,
    // The required version is the first "class file version N.0" on the line;
    // the second half of the message ("only recognizes … up to 52.0") uses the
    // plural "versions", so it cannot be picked up by mistake. The capture is
    // converted from a class-file major to a Java release by the engine.
    Matcher::After {
        needle: "class file version ",
        end: Some(".0"),
        capture: super::super::Capture::ExpectedJava,
    },
    "diagnose.reason.java_too_old",
    "diagnose.advice.change_java",
    CHANGE_JAVA,
);

pub(super) const HEAP_TOO_LARGE_FOR_32BIT_JVM: Rule = r(
    RuleId::HeapTooLargeFor32BitJvm,
    Category::Java,
    Family::Memory,
    StageMask::STARTUP,
    4,
    Matcher::Any(&[
        "Could not reserve enough space for",
        "Invalid maximum heap size",
        "The specified size exceeds the maximum representable size",
    ]),
    "diagnose.reason.heap_too_large_32bit",
    "diagnose.advice.lower_memory",
    LOWER_MEMORY,
);

pub(super) const JAVA_TOO_NEW_FOR_LEGACY_MODS: Rule = r(
    RuleId::JavaTooNewForLegacyMods,
    Category::Java,
    Family::JavaVersion,
    StageMask::STARTUP,
    4,
    Matcher::Any(&[
        "ClassCastException: java.base/jdk",
        "ClassCastException: class jdk",
    ]),
    "diagnose.reason.java_too_new",
    "diagnose.advice.change_java",
    CHANGE_JAVA,
);

pub(super) const JAVA_TOO_NEW: Rule = r(
    RuleId::JavaTooNew,
    Category::Java,
    Family::JavaVersion,
    StageMask::STARTUP,
    4,
    Matcher::Any(&[
        "Unsupported class file major version",
        "because module java.base does not export",
        "Unable to make protected final java.lang.Class java.lang.ClassLoader.defineClass",
        "NoSuchFieldException: ucp",
        "ClassNotFoundException: jdk.nashorn",
        "ClassNotFoundException: java.lang.invoke.LambdaMetafactory",
        "Cannot read the array length because \"urls\" is null",
    ]),
    "diagnose.reason.java_too_new",
    "diagnose.advice.change_java",
    CHANGE_JAVA,
);

pub(super) const MAC_JAVA8U261_WINDOW_CRASH: Rule = r_on(
    Platform::MacOs,
    RuleId::MacJava8u261WindowCrash,
    Category::Java,
    Family::JavaVersion,
    StageMask::STARTUP,
    4,
    Matcher::Any(&["NSWindow drag regions should only be invalidated on the Main Thread"]),
    "diagnose.reason.forge_java_conflict",
    "diagnose.advice.change_java",
    CHANGE_JAVA,
);

pub(super) const FORGE_JAVA8U321_CONFLICT: Rule = r(
    RuleId::ForgeJava8u321Conflict,
    Category::Java,
    Family::JavaVersion,
    StageMask::STARTUP,
    4,
    Matcher::Any(&["ManifestEntryVerifier.<init>(java.util.jar.Manifest)"]),
    "diagnose.reason.forge_java_conflict",
    "diagnose.advice.change_java",
    &[Action::ChangeJava, Action::OpenUrl(UrlKey::Forge)],
);

#[cfg(test)]
mod tests {
    use super::super::super::Capture;
    use super::super::super::rule_id::RuleId;
    use super::super::testkit::{fires_on, primary};

    #[test]
    fn openj9_is_named() {
        assert_eq!(
            fires_on("OpenJ9 is incompatible with Minecraft"),
            Some(RuleId::OpenJ9)
        );
    }

    /// No fixture: the platform gate keeps this rule from ever firing on
    /// Windows. The matcher itself still has to match its line.
    #[test]
    fn the_macos_window_crash_matches_its_line() {
        let text = crate::core::diagnose::engine::Text::parse(
            "Terminating app due to uncaught exception 'NSInternalInconsistencyException', reason: 'NSWindow drag regions should only be invalidated on the Main Thread!'",
        );
        assert!(
            crate::core::diagnose::engine::scan(&text, &super::MAC_JAVA8U261_WINDOW_CRASH)
                .is_some()
        );
    }

    #[test]
    fn a_class_file_version_becomes_the_required_major() {
        let primary = primary(
            "[stdout] java.lang.UnsupportedClassVersionError: net/fabricmc/loader/impl/launch/knot/KnotClient has been compiled by a more recent version of the Java Runtime (class file version 61.0), this version of the Java Runtime only recognizes class file versions up to 52.0",
        )
        .expect("finding");
        assert_eq!(primary.rule, RuleId::JavaTooOld);
        assert_eq!(primary.captures, [(Capture::ExpectedJava, "17".to_owned())]);
    }
}
