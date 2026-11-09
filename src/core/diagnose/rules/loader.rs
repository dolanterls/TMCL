//! The loader and the mods it loads: 36 phenomena, one file.
//!
//! Most of these come from a loader message that already names the culprit.
//! Where it does, the rule captures the name and the advice says what to do
//! with that one mod; where the loader only printed a wall of text, the rule
//! keeps the wall as evidence and the advice says to read it.

use crate::core::launch::failure::{Action, ContentDir, UrlKey};

use super::super::rule::{Matcher, Rule, r};
use super::super::{Capture, Category, Family, RuleId, StageMask};

const MODS: &[Action] = &[Action::OpenContentDirectory(ContentDir::Mods)];
const DOWNLOADS: &[Action] = &[Action::OpenDownloadCenter];
const REPAIR: &[Action] = &[Action::Repair];
const OPTIFINE: &[Action] = &[Action::OpenUrl(UrlKey::OptiFine)];

pub(super) const DUPLICATE_MOD: Rule = r(
    RuleId::DuplicateMod,
    Category::Loader,
    Family::ModResolution,
    StageMask::ALL,
    4,
    Matcher::After {
        needle: "Found a duplicate mod ",
        end: Some(" at "),
        capture: Capture::Mod,
    },
    "diagnose.reason.duplicate_mod",
    "diagnose.advice.remove_duplicates",
    MODS,
);

pub(super) const FABRIC_MOD_RESOLUTION: Rule = r(
    RuleId::FabricModResolution,
    Category::Loader,
    Family::ModResolution,
    StageMask::STARTUP,
    3,
    Matcher::Any(&["ModResolutionException:"]),
    "diagnose.reason.loader_reported",
    "diagnose.advice.read_log",
    DOWNLOADS,
);

pub(super) const FORGE_MISSING_DEPENDENCIES: Rule = r(
    RuleId::ForgeMissingDependencies,
    Category::Loader,
    Family::ModResolution,
    StageMask::STARTUP,
    4,
    Matcher::Block {
        start: "Missing or unsupported mandatory dependencies",
        max: 8,
    },
    "diagnose.reason.missing_dependency_list",
    "diagnose.advice.install_dependency",
    DOWNLOADS,
);

pub(super) const FORGE_DUPLICATE_MODS: Rule = r(
    RuleId::ForgeDuplicateMods,
    Category::Loader,
    Family::ModResolution,
    StageMask::STARTUP,
    4,
    Matcher::Block {
        start: "Found duplicate mods:",
        max: 8,
    },
    "diagnose.reason.duplicate_mods_list",
    "diagnose.advice.remove_duplicates",
    MODS,
);

pub(super) const FABRIC_MOD_CONFLICT: Rule = r(
    RuleId::FabricModConflict,
    Category::Loader,
    Family::ModResolution,
    StageMask::STARTUP,
    4,
    Matcher::After {
        needle: "Found conflicting mods: ",
        end: Some(" conflicts with "),
        capture: Capture::Mod,
    },
    "diagnose.reason.mod_conflict",
    "diagnose.advice.remove_mod",
    MODS,
);

pub(super) const FABRIC_MISSING_DEPENDENCY: Rule = r(
    RuleId::FabricMissingDependency,
    Category::Loader,
    Family::ModResolution,
    StageMask::STARTUP,
    4,
    Matcher::After {
        needle: "Could not find required mod: ",
        end: Some(" requires "),
        capture: Capture::Mod,
    },
    "diagnose.reason.missing_dependency",
    "diagnose.advice.install_dependency",
    DOWNLOADS,
);

pub(super) const FABRIC_WRONG_GAME_VERSION: Rule = r(
    RuleId::FabricWrongGameVersion,
    Category::Loader,
    Family::ModResolution,
    StageMask::STARTUP,
    // Five, so it beats the generic `Could not find required mod` rule that
    // matches the same line: naming the required Minecraft version is the whole
    // diagnosis.
    5,
    Matcher::Between {
        open: "{minecraft @ ",
        close: "}",
        capture: Capture::Version,
    },
    "diagnose.reason.wrong_game_version",
    "diagnose.advice.change_versions",
    DOWNLOADS,
);

pub(super) const FABRIC_DEPENDENCY_VERSION_MISMATCH: Rule = r(
    RuleId::FabricDependencyVersionMismatch,
    Category::Loader,
    Family::ModResolution,
    StageMask::STARTUP,
    3,
    Matcher::After {
        needle: "(at: ",
        end: Some(" requires "),
        capture: Capture::Mod,
    },
    "diagnose.reason.dependency_version",
    "diagnose.advice.install_dependency",
    DOWNLOADS,
);

pub(super) const FORGE_MOD_CRASHED_ON_LOAD: Rule = r(
    RuleId::ForgeModCrashedOnLoad,
    Category::Loader,
    Family::ModResolution,
    StageMask::STARTUP,
    4,
    Matcher::Between {
        open: "Caught exception from ",
        close: " (",
        capture: Capture::Mod,
    },
    "diagnose.reason.mod_crashed",
    "diagnose.advice.remove_mod",
    MODS,
);

pub(super) const MOD_BOOTSTRAP_FAILED: Rule = r(
    RuleId::ModBootstrapFailed,
    Category::Loader,
    Family::ModResolution,
    StageMask::STARTUP,
    4,
    Matcher::After {
        needle: "Failed to create mod instance. ModID: ",
        end: Some(","),
        capture: Capture::ModId,
    },
    // Its own sentence: the capture is the mod id, and the shared `mod_crashed`
    // template asks for `{mod}`, which the placeholder guard rejects.
    "diagnose.reason.mod_bootstrap",
    "diagnose.advice.remove_mod",
    MODS,
);

pub(super) const FABRIC_MOD_CRASHED_ON_LOAD: Rule = r(
    RuleId::FabricModCrashedOnLoad,
    Category::Loader,
    Family::ModResolution,
    StageMask::STARTUP,
    4,
    Matcher::Between {
        open: "provided by '",
        close: "'",
        capture: Capture::Mod,
    },
    "diagnose.reason.mod_crashed",
    "diagnose.advice.remove_mod",
    MODS,
);

pub(super) const FABRIC_LOADER_TOO_NEW: Rule = r(
    RuleId::FabricLoaderTooNew,
    Category::Loader,
    Family::ModResolution,
    StageMask::STARTUP,
    4,
    Matcher::Any(&["FabricMixinTransformerProxy"]),
    "diagnose.reason.fabric_loader_too_new",
    "diagnose.advice.change_versions",
    // No remedy on purpose: repairing re-checks files against the manifest,
    // and the manifest's loader version is what is too new. The design says
    // "read the log", and an action that cannot help is worse than none.
    &[],
);

pub(super) const MANUAL_DEBUG_CRASH: Rule = r(
    RuleId::ManualDebugCrash,
    Category::Loader,
    Family::ModResolution,
    StageMask::ALL,
    5,
    Matcher::Any(&["Manually triggered debug crash"]),
    "diagnose.reason.debug_crash",
    "diagnose.advice.read_log",
    &[],
);

pub(super) const MOD_CONFIG_UNREADABLE: Rule = r(
    RuleId::ModConfigUnreadable,
    Category::Loader,
    Family::ModResolution,
    StageMask::STARTUP,
    3,
    Matcher::Between {
        open: "Failed loading config file ",
        close: " of type",
        capture: Capture::File,
    },
    "diagnose.reason.mod_config",
    "diagnose.advice.mod_config",
    MODS,
);

pub(super) const FABRIC_WARNINGS: Rule = r(
    RuleId::FabricWarnings,
    Category::Loader,
    Family::ModResolution,
    StageMask::STARTUP,
    2,
    Matcher::Any(&[
        "Warnings were found!",
        "Incompatible mod set!",
        "Incompatible mods found!",
    ]),
    "diagnose.reason.fabric_warnings",
    "diagnose.advice.read_log",
    &[],
);

pub(super) const OPTIFINE_FORGE_MISMATCH: Rule = r(
    RuleId::OptiFineForgeMismatch,
    Category::Loader,
    Family::OptiFine,
    StageMask::STARTUP,
    4,
    Matcher::All(&["NoSuchMethodError", "optifine"]),
    "diagnose.reason.optifine_conflict",
    "diagnose.advice.change_optifine",
    OPTIFINE,
);

pub(super) const MOD_FILE_EXTRACTED: Rule = r(
    RuleId::ModFileExtracted,
    Category::Loader,
    Family::Integrity,
    StageMask::STARTUP,
    5,
    Matcher::Any(&[
        "appear to be extracted jar files",
        "Extracted mod jars found",
    ]),
    "diagnose.reason.mod_file_extracted",
    "diagnose.advice.remove_mod",
    MODS,
);

pub(super) const OPTIFINE_WORLD_TICKET_MANAGER: Rule = r(
    RuleId::OptiFineWorldTicketManager,
    Category::Loader,
    Family::OptiFine,
    StageMask::IN_GAME,
    4,
    Matcher::Any(&["ProxyTicketManager.shouldForceTicks"]),
    "diagnose.reason.optifine_conflict",
    "diagnose.advice.change_optifine",
    OPTIFINE,
);

pub(super) const BLOCK_ID_LIMIT_EXCEEDED: Rule = r(
    RuleId::BlockIdLimitExceeded,
    Category::Loader,
    Family::ModResolution,
    StageMask::STARTUP,
    4,
    Matcher::Any(&["maximum id range exceeded"]),
    "diagnose.reason.block_id_limit",
    "diagnose.advice.block_id_limit",
    &[
        Action::OpenUrl(UrlKey::Jeid),
        Action::OpenContentDirectory(ContentDir::Mods),
    ],
);

pub(super) const MIXIN_APPLY_ERROR: Rule = r(
    RuleId::MixinApplyError,
    Category::Loader,
    Family::Mixin,
    StageMask::STARTUP,
    3,
    Matcher::Any(&[
        "MixinApplyError",
        "Mixin prepare failed",
        "Mixin apply failed",
        // Mixin's own per-config summary: `[create.mixins.json] FAILED during
        // APPLY`. Without it the rule misses the one line a player can search.
        ".mixins.json] FAILED during",
        "mixin.injection.throwables.",
    ]),
    "diagnose.reason.mixin_failed",
    "diagnose.advice.remove_mod",
    MODS,
);

pub(super) const MIXIN_APPLY_FOR_MOD_FAILED: Rule = r(
    RuleId::MixinApplyForModFailed,
    Category::Loader,
    Family::Mixin,
    StageMask::STARTUP,
    4,
    Matcher::Between {
        open: "Mixin apply for mod ",
        close: " failed",
        capture: Capture::Mod,
    },
    "diagnose.reason.mixin_failed",
    "diagnose.advice.remove_mod",
    MODS,
);

pub(super) const FORGE_REPORTED_ERROR: Rule = r(
    RuleId::ForgeReportedError,
    Category::Loader,
    Family::ModResolution,
    StageMask::STARTUP,
    2,
    Matcher::Block {
        start: "An exception was thrown, the game will display an error screen and halt",
        max: 6,
    },
    "diagnose.reason.loader_reported",
    "diagnose.advice.read_log",
    &[],
);

pub(super) const LOADER_REPORTED_MOD_FAILURE: Rule = r(
    RuleId::LoaderReportedModFailure,
    Category::Loader,
    Family::ModResolution,
    StageMask::STARTUP,
    2,
    Matcher::Any(&["Mod File:", "-- MOD ", "Failure message:"]),
    "diagnose.reason.loader_reported",
    "diagnose.advice.read_log",
    &[],
);

pub(super) const FORGE_INSTALLED_TWICE: Rule = r(
    RuleId::ForgeInstalledTwice,
    Category::Loader,
    Family::Integrity,
    StageMask::STARTUP,
    4,
    Matcher::Any(&["Found multiple arguments for option"]),
    "diagnose.reason.forge_twice",
    "diagnose.advice.repair",
    REPAIR,
);

pub(super) const OPTIFINE_INSTALLED_TWICE: Rule = r(
    RuleId::OptiFineInstalledTwice,
    Category::Loader,
    Family::Integrity,
    StageMask::STARTUP,
    4,
    Matcher::Any(&["Module optifine reads another module named optifine"]),
    "diagnose.reason.optifine_twice",
    "diagnose.advice.remove_mod",
    MODS,
);

pub(super) const MIXIN_BOOTSTRAP_MISSING: Rule = r(
    RuleId::MixinBootstrapMissing,
    Category::Loader,
    Family::Mixin,
    StageMask::STARTUP,
    4,
    Matcher::Any(&["org.spongepowered.asm.launch.MixinTweaker"]),
    "diagnose.reason.mixin_bootstrap",
    "diagnose.advice.install_mixinbootstrap",
    &[Action::OpenUrl(UrlKey::MixinBootstrap)],
);

pub(super) const MOD_FILE_MODULE_NAME_INVALID: Rule = r(
    RuleId::ModFileModuleNameInvalid,
    Category::Loader,
    Family::Integrity,
    StageMask::STARTUP,
    4,
    Matcher::Any(&["Invalid module name: '' is not a Java identifier"]),
    "diagnose.reason.mod_name",
    "diagnose.advice.mod_name",
    MODS,
);

pub(super) const FORGE_INSTALL_INCOMPLETE: Rule = r(
    RuleId::ForgeInstallIncomplete,
    Category::Loader,
    Family::Integrity,
    StageMask::STARTUP,
    4,
    Matcher::Any(&[
        "Invalid paths argument, contained no existing paths",
        "Cannot find launch target fmlclient",
        "Failed to find Minecraft resource version",
    ]),
    "diagnose.reason.forge_install",
    "diagnose.advice.repair",
    REPAIR,
);

pub(super) const NIGHT_CONFIG_PARSE_ERROR: Rule = r(
    RuleId::NightConfigParseError,
    Category::Loader,
    Family::ModResolution,
    StageMask::STARTUP,
    4,
    Matcher::Any(&["com.electronwill.nightconfig.core.io.ParsingException"]),
    "diagnose.reason.night_config",
    "diagnose.advice.night_config",
    &[Action::OpenUrl(UrlKey::NightConfig)],
);

pub(super) const SHADERS_MOD_ALONGSIDE_OPTIFINE: Rule = r(
    RuleId::ShadersModAlongsideOptiFine,
    Category::Loader,
    Family::OptiFine,
    StageMask::STARTUP,
    5,
    Matcher::Any(&["Shaders Mod detected. Please remove it"]),
    "diagnose.reason.shaders_mod",
    "diagnose.advice.remove_shaders_mod",
    MODS,
);

pub(super) const COREMOD_TRANSFORM_OPTIFINE: Rule = r(
    RuleId::CoremodTransformOptiFine,
    Category::Loader,
    Family::OptiFine,
    StageMask::STARTUP,
    4,
    Matcher::Any(&["Error occurred applying transform of coremod META-INF/asm/multipart.js"]),
    "diagnose.reason.optifine_conflict",
    "diagnose.advice.change_optifine",
    OPTIFINE,
);

pub(super) const PERFORMANT_OPTIFINE_MIXIN: Rule = r(
    RuleId::PerformantOptiFineMixin,
    Category::Loader,
    Family::OptiFine,
    StageMask::STARTUP,
    3,
    Matcher::All(&["Redirector OnisOnLadder", "performant.mixins.json"]),
    "diagnose.reason.optifine_conflict",
    "diagnose.advice.remove_mod",
    MODS,
);

pub(super) const TWILIGHT_FOREST_OPTIFINE_MIXIN: Rule = r(
    RuleId::TwilightForestOptiFineMixin,
    Category::Loader,
    Family::OptiFine,
    StageMask::STARTUP,
    3,
    Matcher::All(&["IllegalArgumentException", "outside of image bounds"]),
    "diagnose.reason.optifine_conflict",
    "diagnose.advice.change_optifine",
    OPTIFINE,
);

pub(super) const JADE_OPTIFINE_MIXIN: Rule = r(
    RuleId::JadeOptiFineMixin,
    Category::Loader,
    Family::OptiFine,
    StageMask::STARTUP,
    3,
    Matcher::All(&["jade.mixins.json", "Critical injection failure"]),
    "diagnose.reason.optifine_conflict",
    "diagnose.advice.remove_mod",
    MODS,
);

pub(super) const NEOFORGE_OPTIFINE_INVALID: Rule = r(
    RuleId::NeoForgeOptiFineInvalid,
    Category::Loader,
    Family::OptiFine,
    StageMask::STARTUP,
    4,
    Matcher::Any(&["Invalid Services found OptiFine"]),
    "diagnose.reason.optifine_conflict",
    "diagnose.advice.change_optifine",
    OPTIFINE,
);

pub(super) const RTSS_SODIUM_INCOMPATIBLE: Rule = r(
    RuleId::RtssSodiumIncompatible,
    Category::Loader,
    Family::ModResolution,
    StageMask::STARTUP,
    5,
    Matcher::Any(&["RivaTuner Statistics Server (RTSS) is not compatible with Sodium"]),
    "diagnose.reason.rtss",
    "diagnose.advice.rtss",
    &[Action::OpenUrl(UrlKey::Sodium)],
);

#[cfg(test)]
mod tests {
    use super::super::super::rule_id::RuleId;
    use super::super::testkit::primary;

    #[test]
    fn a_duplicate_mod_names_the_mod() {
        let primary = primary("[stdout] Found a duplicate mod create at mods/create-0.5.jar")
            .expect("finding");
        assert_eq!(primary.rule, RuleId::DuplicateMod);
        assert_eq!(primary.captures[0].1, "create");
    }

    #[test]
    fn a_conflict_names_the_mod_that_has_to_go() {
        let primary = primary(
            "[stdout] ModResolutionException: Found conflicting mods: create conflicts with flywheel",
        )
        .expect("finding");
        assert_eq!(primary.rule, RuleId::FabricModConflict);
        assert_eq!(primary.captures[0].1, "create");
    }

    #[test]
    fn a_wrong_game_version_beats_the_generic_missing_dependency_rule() {
        let primary = primary(
            "[stdout] ModResolutionException: Could not find required mod: create requires {minecraft @ 1.20.1}",
        )
        .expect("finding");
        assert_eq!(primary.rule, RuleId::FabricWrongGameVersion);
        assert_eq!(primary.captures[0].1, "1.20.1");
    }

    #[test]
    fn a_forge_mod_crash_keeps_the_name_and_the_id() {
        let primary =
            primary("[stdout] LoaderExceptionModCrash: Caught exception from Create (create)")
                .expect("finding");
        assert_eq!(primary.rule, RuleId::ForgeModCrashedOnLoad);
        assert_eq!(primary.captures[0].1, "Create");
    }
}
