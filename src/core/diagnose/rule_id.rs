//! The 59 rules, as a closed enum.
//!
//! The ids are the stable spelling in `--json` and in fixture names; the keys
//! are the snake_case form used by anything that has to build a string from an
//! id. `OptiFine` and `Rtss` make a runtime converter impossible to trust, so
//! both spellings are written out.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum RuleId {
    OpenJ9,
    Java11Required,
    JavaTooOld,
    HeapTooLargeFor32BitJvm,
    JavaTooNewForLegacyMods,
    JavaTooNew,
    MacJava8u261WindowCrash,
    ForgeJava8u321Conflict,
    OutOfMemory,
    SystemMemoryExhausted,
    GlInvalidOperation,
    OpenGlUnsupported,
    GraphicsDriverCrash,
    ResourcePackResolutionTooHigh,
    MacWindowServicePort,
    DuplicateMod,
    FabricModResolution,
    ForgeMissingDependencies,
    ForgeDuplicateMods,
    FabricModConflict,
    FabricMissingDependency,
    FabricWrongGameVersion,
    FabricDependencyVersionMismatch,
    ForgeModCrashedOnLoad,
    ModBootstrapFailed,
    FabricModCrashedOnLoad,
    FabricLoaderTooNew,
    ManualDebugCrash,
    ModConfigUnreadable,
    FabricWarnings,
    OptiFineForgeMismatch,
    ModFileExtracted,
    OptiFineWorldTicketManager,
    BlockIdLimitExceeded,
    MixinApplyError,
    MixinApplyForModFailed,
    ForgeReportedError,
    LoaderReportedModFailure,
    ForgeInstalledTwice,
    OptiFineInstalledTwice,
    MixinBootstrapMissing,
    ModFileModuleNameInvalid,
    ForgeInstallIncomplete,
    NightConfigParseError,
    ShadersModAlongsideOptiFine,
    CoremodTransformOptiFine,
    PerformantOptiFineMixin,
    TwilightForestOptiFineMixin,
    JadeOptiFineMixin,
    NeoForgeOptiFineInvalid,
    RtssSodiumIncompatible,
    NativeLibraryMissing,
    NoSuchMethod,
    NoClassDefFound,
    IllegalAccess,
    FileDigestMismatch,
    FileAlreadyExists,
    EntityCrash,
    BlockCrash,
}

impl RuleId {
    pub const ALL: [Self; 59] = [
        Self::OpenJ9,
        Self::Java11Required,
        Self::JavaTooOld,
        Self::HeapTooLargeFor32BitJvm,
        Self::JavaTooNewForLegacyMods,
        Self::JavaTooNew,
        Self::MacJava8u261WindowCrash,
        Self::ForgeJava8u321Conflict,
        Self::OutOfMemory,
        Self::SystemMemoryExhausted,
        Self::GlInvalidOperation,
        Self::OpenGlUnsupported,
        Self::GraphicsDriverCrash,
        Self::ResourcePackResolutionTooHigh,
        Self::MacWindowServicePort,
        Self::DuplicateMod,
        Self::FabricModResolution,
        Self::ForgeMissingDependencies,
        Self::ForgeDuplicateMods,
        Self::FabricModConflict,
        Self::FabricMissingDependency,
        Self::FabricWrongGameVersion,
        Self::FabricDependencyVersionMismatch,
        Self::ForgeModCrashedOnLoad,
        Self::ModBootstrapFailed,
        Self::FabricModCrashedOnLoad,
        Self::FabricLoaderTooNew,
        Self::ManualDebugCrash,
        Self::ModConfigUnreadable,
        Self::FabricWarnings,
        Self::OptiFineForgeMismatch,
        Self::ModFileExtracted,
        Self::OptiFineWorldTicketManager,
        Self::BlockIdLimitExceeded,
        Self::MixinApplyError,
        Self::MixinApplyForModFailed,
        Self::ForgeReportedError,
        Self::LoaderReportedModFailure,
        Self::ForgeInstalledTwice,
        Self::OptiFineInstalledTwice,
        Self::MixinBootstrapMissing,
        Self::ModFileModuleNameInvalid,
        Self::ForgeInstallIncomplete,
        Self::NightConfigParseError,
        Self::ShadersModAlongsideOptiFine,
        Self::CoremodTransformOptiFine,
        Self::PerformantOptiFineMixin,
        Self::TwilightForestOptiFineMixin,
        Self::JadeOptiFineMixin,
        Self::NeoForgeOptiFineInvalid,
        Self::RtssSodiumIncompatible,
        Self::NativeLibraryMissing,
        Self::NoSuchMethod,
        Self::NoClassDefFound,
        Self::IllegalAccess,
        Self::FileDigestMismatch,
        Self::FileAlreadyExists,
        Self::EntityCrash,
        Self::BlockCrash,
    ];

    /// PascalCase spelling, used by `--json` and by fixture names.
    pub const fn id(self) -> &'static str {
        match self {
            Self::OpenJ9 => "OpenJ9",
            Self::Java11Required => "Java11Required",
            Self::JavaTooOld => "JavaTooOld",
            Self::HeapTooLargeFor32BitJvm => "HeapTooLargeFor32BitJvm",
            Self::JavaTooNewForLegacyMods => "JavaTooNewForLegacyMods",
            Self::JavaTooNew => "JavaTooNew",
            Self::MacJava8u261WindowCrash => "MacJava8u261WindowCrash",
            Self::ForgeJava8u321Conflict => "ForgeJava8u321Conflict",
            Self::OutOfMemory => "OutOfMemory",
            Self::SystemMemoryExhausted => "SystemMemoryExhausted",
            Self::GlInvalidOperation => "GlInvalidOperation",
            Self::OpenGlUnsupported => "OpenGlUnsupported",
            Self::GraphicsDriverCrash => "GraphicsDriverCrash",
            Self::ResourcePackResolutionTooHigh => "ResourcePackResolutionTooHigh",
            Self::MacWindowServicePort => "MacWindowServicePort",
            Self::DuplicateMod => "DuplicateMod",
            Self::FabricModResolution => "FabricModResolution",
            Self::ForgeMissingDependencies => "ForgeMissingDependencies",
            Self::ForgeDuplicateMods => "ForgeDuplicateMods",
            Self::FabricModConflict => "FabricModConflict",
            Self::FabricMissingDependency => "FabricMissingDependency",
            Self::FabricWrongGameVersion => "FabricWrongGameVersion",
            Self::FabricDependencyVersionMismatch => "FabricDependencyVersionMismatch",
            Self::ForgeModCrashedOnLoad => "ForgeModCrashedOnLoad",
            Self::ModBootstrapFailed => "ModBootstrapFailed",
            Self::FabricModCrashedOnLoad => "FabricModCrashedOnLoad",
            Self::FabricLoaderTooNew => "FabricLoaderTooNew",
            Self::ManualDebugCrash => "ManualDebugCrash",
            Self::ModConfigUnreadable => "ModConfigUnreadable",
            Self::FabricWarnings => "FabricWarnings",
            Self::OptiFineForgeMismatch => "OptiFineForgeMismatch",
            Self::ModFileExtracted => "ModFileExtracted",
            Self::OptiFineWorldTicketManager => "OptiFineWorldTicketManager",
            Self::BlockIdLimitExceeded => "BlockIdLimitExceeded",
            Self::MixinApplyError => "MixinApplyError",
            Self::MixinApplyForModFailed => "MixinApplyForModFailed",
            Self::ForgeReportedError => "ForgeReportedError",
            Self::LoaderReportedModFailure => "LoaderReportedModFailure",
            Self::ForgeInstalledTwice => "ForgeInstalledTwice",
            Self::OptiFineInstalledTwice => "OptiFineInstalledTwice",
            Self::MixinBootstrapMissing => "MixinBootstrapMissing",
            Self::ModFileModuleNameInvalid => "ModFileModuleNameInvalid",
            Self::ForgeInstallIncomplete => "ForgeInstallIncomplete",
            Self::NightConfigParseError => "NightConfigParseError",
            Self::ShadersModAlongsideOptiFine => "ShadersModAlongsideOptiFine",
            Self::CoremodTransformOptiFine => "CoremodTransformOptiFine",
            Self::PerformantOptiFineMixin => "PerformantOptiFineMixin",
            Self::TwilightForestOptiFineMixin => "TwilightForestOptiFineMixin",
            Self::JadeOptiFineMixin => "JadeOptiFineMixin",
            Self::NeoForgeOptiFineInvalid => "NeoForgeOptiFineInvalid",
            Self::RtssSodiumIncompatible => "RtssSodiumIncompatible",
            Self::NativeLibraryMissing => "NativeLibraryMissing",
            Self::NoSuchMethod => "NoSuchMethod",
            Self::NoClassDefFound => "NoClassDefFound",
            Self::IllegalAccess => "IllegalAccess",
            Self::FileDigestMismatch => "FileDigestMismatch",
            Self::FileAlreadyExists => "FileAlreadyExists",
            Self::EntityCrash => "EntityCrash",
            Self::BlockCrash => "BlockCrash",
        }
    }

    /// snake_case spelling, for keys and file names built at runtime.
    pub const fn key(self) -> &'static str {
        match self {
            Self::OpenJ9 => "openj9",
            Self::Java11Required => "java11_required",
            Self::JavaTooOld => "java_too_old",
            Self::HeapTooLargeFor32BitJvm => "heap_too_large_for_32bit_jvm",
            Self::JavaTooNewForLegacyMods => "java_too_new_for_legacy_mods",
            Self::JavaTooNew => "java_too_new",
            Self::MacJava8u261WindowCrash => "mac_java8u261_window_crash",
            Self::ForgeJava8u321Conflict => "forge_java8u321_conflict",
            Self::OutOfMemory => "out_of_memory",
            Self::SystemMemoryExhausted => "system_memory_exhausted",
            Self::GlInvalidOperation => "gl_invalid_operation",
            Self::OpenGlUnsupported => "opengl_unsupported",
            Self::GraphicsDriverCrash => "graphics_driver_crash",
            Self::ResourcePackResolutionTooHigh => "resource_pack_resolution_too_high",
            Self::MacWindowServicePort => "mac_window_service_port",
            Self::DuplicateMod => "duplicate_mod",
            Self::FabricModResolution => "fabric_mod_resolution",
            Self::ForgeMissingDependencies => "forge_missing_dependencies",
            Self::ForgeDuplicateMods => "forge_duplicate_mods",
            Self::FabricModConflict => "fabric_mod_conflict",
            Self::FabricMissingDependency => "fabric_missing_dependency",
            Self::FabricWrongGameVersion => "fabric_wrong_game_version",
            Self::FabricDependencyVersionMismatch => "fabric_dependency_version_mismatch",
            Self::ForgeModCrashedOnLoad => "forge_mod_crashed_on_load",
            Self::ModBootstrapFailed => "mod_bootstrap_failed",
            Self::FabricModCrashedOnLoad => "fabric_mod_crashed_on_load",
            Self::FabricLoaderTooNew => "fabric_loader_too_new",
            Self::ManualDebugCrash => "manual_debug_crash",
            Self::ModConfigUnreadable => "mod_config_unreadable",
            Self::FabricWarnings => "fabric_warnings",
            Self::OptiFineForgeMismatch => "optifine_forge_mismatch",
            Self::ModFileExtracted => "mod_file_extracted",
            Self::OptiFineWorldTicketManager => "optifine_world_ticket_manager",
            Self::BlockIdLimitExceeded => "block_id_limit_exceeded",
            Self::MixinApplyError => "mixin_apply_error",
            Self::MixinApplyForModFailed => "mixin_apply_for_mod_failed",
            Self::ForgeReportedError => "forge_reported_error",
            Self::LoaderReportedModFailure => "loader_reported_mod_failure",
            Self::ForgeInstalledTwice => "forge_installed_twice",
            Self::OptiFineInstalledTwice => "optifine_installed_twice",
            Self::MixinBootstrapMissing => "mixin_bootstrap_missing",
            Self::ModFileModuleNameInvalid => "mod_file_module_name_invalid",
            Self::ForgeInstallIncomplete => "forge_install_incomplete",
            Self::NightConfigParseError => "night_config_parse_error",
            Self::ShadersModAlongsideOptiFine => "shaders_mod_alongside_optifine",
            Self::CoremodTransformOptiFine => "coremod_transform_optifine",
            Self::PerformantOptiFineMixin => "performant_optifine_mixin",
            Self::TwilightForestOptiFineMixin => "twilight_forest_optifine_mixin",
            Self::JadeOptiFineMixin => "jade_optifine_mixin",
            Self::NeoForgeOptiFineInvalid => "neoforge_optifine_invalid",
            Self::RtssSodiumIncompatible => "rtss_sodium_incompatible",
            Self::NativeLibraryMissing => "native_library_missing",
            Self::NoSuchMethod => "no_such_method",
            Self::NoClassDefFound => "no_class_def_found",
            Self::IllegalAccess => "illegal_access",
            Self::FileDigestMismatch => "file_digest_mismatch",
            Self::FileAlreadyExists => "file_already_exists",
            Self::EntityCrash => "entity_crash",
            Self::BlockCrash => "block_crash",
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::RuleId;

    #[test]
    fn every_rule_id_has_a_distinct_id_and_key() {
        let ids = RuleId::ALL
            .iter()
            .map(|id| id.id())
            .collect::<BTreeSet<_>>();
        let keys = RuleId::ALL
            .iter()
            .map(|id| id.key())
            .collect::<BTreeSet<_>>();
        assert_eq!(ids.len(), RuleId::ALL.len());
        assert_eq!(keys.len(), RuleId::ALL.len());
    }

    #[test]
    fn keys_handle_the_awkward_spellings() {
        assert_eq!(
            RuleId::OptiFineForgeMismatch.key(),
            "optifine_forge_mismatch"
        );
        assert_eq!(
            RuleId::RtssSodiumIncompatible.key(),
            "rtss_sodium_incompatible"
        );
        assert_eq!(RuleId::Java11Required.key(), "java11_required");
        assert_eq!(
            RuleId::MacJava8u261WindowCrash.key(),
            "mac_java8u261_window_crash"
        );
        assert_eq!(
            RuleId::HeapTooLargeFor32BitJvm.key(),
            "heap_too_large_for_32bit_jvm"
        );
    }
}
