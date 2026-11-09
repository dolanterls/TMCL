//! The rule table, in one place.
//!
//! Each category lives in its own file so a reader can check eight Java rules
//! without scrolling past thirty-six loader rules. The order here is the tie
//! breaker when two rules score the same.

mod graphics;
mod integrity;
mod java;
mod loader;
mod memory;
mod native;
mod world;

use super::rule::Rule;
use super::{Family, RuleId};

/// Every rule, in table order. Fifty-nine entries: the two macOS-only rules are
/// present but never fire on Windows (their `platform` gate is checked first).
pub static RULES: &[Rule] = &[
    java::OPENJ9,
    java::JAVA11_REQUIRED,
    java::JAVA_TOO_OLD,
    java::HEAP_TOO_LARGE_FOR_32BIT_JVM,
    java::JAVA_TOO_NEW_FOR_LEGACY_MODS,
    java::JAVA_TOO_NEW,
    java::MAC_JAVA8U261_WINDOW_CRASH,
    java::FORGE_JAVA8U321_CONFLICT,
    memory::OUT_OF_MEMORY,
    memory::SYSTEM_MEMORY_EXHAUSTED,
    graphics::GL_INVALID_OPERATION,
    graphics::OPENGL_UNSUPPORTED,
    graphics::GRAPHICS_DRIVER_CRASH,
    graphics::RESOURCE_PACK_RESOLUTION_TOO_HIGH,
    graphics::MAC_WINDOW_SERVICE_PORT,
    loader::DUPLICATE_MOD,
    loader::FABRIC_MOD_RESOLUTION,
    loader::FORGE_MISSING_DEPENDENCIES,
    loader::FORGE_DUPLICATE_MODS,
    loader::FABRIC_MOD_CONFLICT,
    loader::FABRIC_MISSING_DEPENDENCY,
    loader::FABRIC_WRONG_GAME_VERSION,
    loader::FABRIC_DEPENDENCY_VERSION_MISMATCH,
    loader::FORGE_MOD_CRASHED_ON_LOAD,
    loader::MOD_BOOTSTRAP_FAILED,
    loader::FABRIC_MOD_CRASHED_ON_LOAD,
    loader::FABRIC_LOADER_TOO_NEW,
    loader::MANUAL_DEBUG_CRASH,
    loader::MOD_CONFIG_UNREADABLE,
    loader::FABRIC_WARNINGS,
    loader::OPTIFINE_FORGE_MISMATCH,
    loader::MOD_FILE_EXTRACTED,
    loader::OPTIFINE_WORLD_TICKET_MANAGER,
    loader::BLOCK_ID_LIMIT_EXCEEDED,
    loader::MIXIN_APPLY_ERROR,
    loader::MIXIN_APPLY_FOR_MOD_FAILED,
    loader::FORGE_REPORTED_ERROR,
    loader::LOADER_REPORTED_MOD_FAILURE,
    loader::FORGE_INSTALLED_TWICE,
    loader::OPTIFINE_INSTALLED_TWICE,
    loader::MIXIN_BOOTSTRAP_MISSING,
    loader::MOD_FILE_MODULE_NAME_INVALID,
    loader::FORGE_INSTALL_INCOMPLETE,
    loader::NIGHT_CONFIG_PARSE_ERROR,
    loader::SHADERS_MOD_ALONGSIDE_OPTIFINE,
    loader::COREMOD_TRANSFORM_OPTIFINE,
    loader::PERFORMANT_OPTIFINE_MIXIN,
    loader::TWILIGHT_FOREST_OPTIFINE_MIXIN,
    loader::JADE_OPTIFINE_MIXIN,
    loader::NEOFORGE_OPTIFINE_INVALID,
    loader::RTSS_SODIUM_INCOMPATIBLE,
    native::NATIVE_LIBRARY_MISSING,
    native::NO_SUCH_METHOD,
    native::NO_CLASS_DEF_FOUND,
    native::ILLEGAL_ACCESS,
    integrity::FILE_DIGEST_MISMATCH,
    integrity::FILE_ALREADY_EXISTS,
    world::ENTITY_CRASH,
    world::BLOCK_CRASH,
];

pub fn rule_for(id: RuleId) -> &'static Rule {
    RULES
        .iter()
        .find(|rule| rule.id == id)
        .expect("every RuleId has a rule in the table")
}

pub fn family_of(id: RuleId) -> Family {
    rule_for(id).family
}

pub fn weight_of(id: RuleId) -> u8 {
    rule_for(id).weight
}

/// Helpers used by the per-category tests and by the fixture guard. Stage is
/// derived from the text exactly as `evidence` does it, minus the clock.
#[cfg(test)]
pub(crate) mod testkit {
    use crate::core::diagnose::engine::Text;
    use crate::core::diagnose::finding::{self, Finding};
    use crate::core::diagnose::{ExitKind, RuleId};

    pub(crate) fn primary(body: &str) -> Option<Finding> {
        let text = Text::parse(body);
        // The real classifier, with the shortest duration that counts as "the
        // process lived long enough to have opened a window": a fixture has no
        // clock, and a second copy of this rule would drift from the one the
        // launcher uses.
        let stage = crate::core::diagnose::evidence::stage_of(&text, 2);
        finding::rank(finding::collect(&text, stage, ExitKind::Crashed)).0
    }

    pub(crate) fn fires_on(body: &str) -> Option<RuleId> {
        primary(body).map(|finding| finding.rule)
    }
}

#[cfg(test)]
mod fixture_guard {
    use std::collections::BTreeSet;

    use super::{RuleId, rule_for, testkit};
    use crate::core::diagnose::Platform;

    const FIXTURES: &[(RuleId, &str)] = &[
        (RuleId::OpenJ9, include_str!("../fixtures/openj9.txt")),
        (
            RuleId::Java11Required,
            include_str!("../fixtures/java11_required.txt"),
        ),
        (
            RuleId::JavaTooOld,
            include_str!("../fixtures/java_too_old.txt"),
        ),
        (
            RuleId::HeapTooLargeFor32BitJvm,
            include_str!("../fixtures/heap_too_large_for_32bit_jvm.txt"),
        ),
        (
            RuleId::JavaTooNewForLegacyMods,
            include_str!("../fixtures/java_too_new_for_legacy_mods.txt"),
        ),
        (
            RuleId::JavaTooNew,
            include_str!("../fixtures/java_too_new.txt"),
        ),
        (
            RuleId::ForgeJava8u321Conflict,
            include_str!("../fixtures/forge_java8u321_conflict.txt"),
        ),
        (
            RuleId::OutOfMemory,
            include_str!("../fixtures/out_of_memory.txt"),
        ),
        (
            RuleId::SystemMemoryExhausted,
            include_str!("../fixtures/system_memory_exhausted.txt"),
        ),
        (
            RuleId::GlInvalidOperation,
            include_str!("../fixtures/gl_invalid_operation.txt"),
        ),
        (
            RuleId::OpenGlUnsupported,
            include_str!("../fixtures/opengl_unsupported.txt"),
        ),
        (
            RuleId::GraphicsDriverCrash,
            include_str!("../fixtures/graphics_driver_crash.txt"),
        ),
        (
            RuleId::ResourcePackResolutionTooHigh,
            include_str!("../fixtures/resource_pack_resolution_too_high.txt"),
        ),
        (
            RuleId::DuplicateMod,
            include_str!("../fixtures/duplicate_mod.txt"),
        ),
        (
            RuleId::FabricModResolution,
            include_str!("../fixtures/fabric_mod_resolution.txt"),
        ),
        (
            RuleId::ForgeMissingDependencies,
            include_str!("../fixtures/forge_missing_dependencies.txt"),
        ),
        (
            RuleId::ForgeDuplicateMods,
            include_str!("../fixtures/forge_duplicate_mods.txt"),
        ),
        (
            RuleId::FabricModConflict,
            include_str!("../fixtures/fabric_mod_conflict.txt"),
        ),
        (
            RuleId::FabricMissingDependency,
            include_str!("../fixtures/fabric_missing_dependency.txt"),
        ),
        (
            RuleId::FabricWrongGameVersion,
            include_str!("../fixtures/fabric_wrong_game_version.txt"),
        ),
        (
            RuleId::FabricDependencyVersionMismatch,
            include_str!("../fixtures/fabric_dependency_version_mismatch.txt"),
        ),
        (
            RuleId::ForgeModCrashedOnLoad,
            include_str!("../fixtures/forge_mod_crashed_on_load.txt"),
        ),
        (
            RuleId::ModBootstrapFailed,
            include_str!("../fixtures/mod_bootstrap_failed.txt"),
        ),
        (
            RuleId::FabricModCrashedOnLoad,
            include_str!("../fixtures/fabric_mod_crashed_on_load.txt"),
        ),
        (
            RuleId::FabricLoaderTooNew,
            include_str!("../fixtures/fabric_loader_too_new.txt"),
        ),
        (
            RuleId::ManualDebugCrash,
            include_str!("../fixtures/manual_debug_crash.txt"),
        ),
        (
            RuleId::ModConfigUnreadable,
            include_str!("../fixtures/mod_config_unreadable.txt"),
        ),
        (
            RuleId::FabricWarnings,
            include_str!("../fixtures/fabric_warnings.txt"),
        ),
        (
            RuleId::OptiFineForgeMismatch,
            include_str!("../fixtures/optifine_forge_mismatch.txt"),
        ),
        (
            RuleId::ModFileExtracted,
            include_str!("../fixtures/mod_file_extracted.txt"),
        ),
        (
            RuleId::OptiFineWorldTicketManager,
            include_str!("../fixtures/optifine_world_ticket_manager.txt"),
        ),
        (
            RuleId::BlockIdLimitExceeded,
            include_str!("../fixtures/block_id_limit_exceeded.txt"),
        ),
        (
            RuleId::MixinApplyError,
            include_str!("../fixtures/mixin_apply_error.txt"),
        ),
        (
            RuleId::MixinApplyForModFailed,
            include_str!("../fixtures/mixin_apply_for_mod_failed.txt"),
        ),
        (
            RuleId::ForgeReportedError,
            include_str!("../fixtures/forge_reported_error.txt"),
        ),
        (
            RuleId::LoaderReportedModFailure,
            include_str!("../fixtures/loader_reported_mod_failure.txt"),
        ),
        (
            RuleId::ForgeInstalledTwice,
            include_str!("../fixtures/forge_installed_twice.txt"),
        ),
        (
            RuleId::OptiFineInstalledTwice,
            include_str!("../fixtures/optifine_installed_twice.txt"),
        ),
        (
            RuleId::MixinBootstrapMissing,
            include_str!("../fixtures/mixin_bootstrap_missing.txt"),
        ),
        (
            RuleId::ModFileModuleNameInvalid,
            include_str!("../fixtures/mod_file_module_name_invalid.txt"),
        ),
        (
            RuleId::ForgeInstallIncomplete,
            include_str!("../fixtures/forge_install_incomplete.txt"),
        ),
        (
            RuleId::NightConfigParseError,
            include_str!("../fixtures/night_config_parse_error.txt"),
        ),
        (
            RuleId::ShadersModAlongsideOptiFine,
            include_str!("../fixtures/shaders_mod_alongside_optifine.txt"),
        ),
        (
            RuleId::CoremodTransformOptiFine,
            include_str!("../fixtures/coremod_transform_optifine.txt"),
        ),
        (
            RuleId::PerformantOptiFineMixin,
            include_str!("../fixtures/performant_optifine_mixin.txt"),
        ),
        (
            RuleId::TwilightForestOptiFineMixin,
            include_str!("../fixtures/twilight_forest_optifine_mixin.txt"),
        ),
        (
            RuleId::JadeOptiFineMixin,
            include_str!("../fixtures/jade_optifine_mixin.txt"),
        ),
        (
            RuleId::NeoForgeOptiFineInvalid,
            include_str!("../fixtures/neoforge_optifine_invalid.txt"),
        ),
        (
            RuleId::RtssSodiumIncompatible,
            include_str!("../fixtures/rtss_sodium_incompatible.txt"),
        ),
        (
            RuleId::NativeLibraryMissing,
            include_str!("../fixtures/native_library_missing.txt"),
        ),
        (
            RuleId::NoSuchMethod,
            include_str!("../fixtures/no_such_method.txt"),
        ),
        (
            RuleId::NoClassDefFound,
            include_str!("../fixtures/no_class_def_found.txt"),
        ),
        (
            RuleId::IllegalAccess,
            include_str!("../fixtures/illegal_access.txt"),
        ),
        (
            RuleId::FileDigestMismatch,
            include_str!("../fixtures/file_digest_mismatch.txt"),
        ),
        (
            RuleId::FileAlreadyExists,
            include_str!("../fixtures/file_already_exists.txt"),
        ),
        (
            RuleId::EntityCrash,
            include_str!("../fixtures/entity_crash.txt"),
        ),
        (
            RuleId::BlockCrash,
            include_str!("../fixtures/block_crash.txt"),
        ),
    ];

    /// One sample per rule, and the sample must make that rule the verdict.
    #[test]
    fn every_rule_has_a_fixture_and_it_fires() {
        for (id, body) in FIXTURES {
            let primary = testkit::primary(body);
            assert_eq!(
                primary.as_ref().map(|finding| finding.rule),
                Some(*id),
                "the fixture for {id:?} did not make it the verdict:\n{body}"
            );
        }
    }

    /// The two macOS-only rules have no fixture because they cannot fire here;
    /// their matchers are tested directly in their own modules.
    #[test]
    fn the_only_rules_without_a_fixture_are_the_macos_ones() {
        let covered = FIXTURES.iter().map(|(id, _)| *id).collect::<BTreeSet<_>>();
        let missing = RuleId::ALL
            .iter()
            .copied()
            .filter(|id| !covered.contains(id))
            .collect::<Vec<_>>();
        assert_eq!(missing.len(), 2, "{missing:?}");
        assert!(
            missing
                .iter()
                .all(|id| rule_for(*id).platform != Platform::Any)
        );
    }

    /// `rule_for` takes the first match, so a duplicate entry would silently
    /// shadow the later one and every fixture would still pass. The count also
    /// has to match the enum: a rule with no entry panics only when it fires.
    #[test]
    fn every_rule_id_appears_in_the_table_exactly_once() {
        let ids = super::RULES.iter().map(|rule| rule.id).collect::<Vec<_>>();
        let unique = ids.iter().copied().collect::<BTreeSet<_>>();
        assert_eq!(unique.len(), ids.len(), "a rule id is listed twice");
        assert_eq!(
            ids.len(),
            RuleId::ALL.len(),
            "the table and RuleId::ALL disagree"
        );
        for id in RuleId::ALL {
            assert!(unique.contains(&id), "{id:?} has no rule in the table");
        }
    }

    #[test]
    fn a_healthy_log_has_no_verdict() {
        let body = include_str!("../fixtures/clean.txt");
        assert!(testkit::primary(body).is_none());
    }
}
