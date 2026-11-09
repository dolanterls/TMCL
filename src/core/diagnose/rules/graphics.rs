//! Graphics: the driver, OpenGL, or a resource pack the GPU cannot hold.

use crate::core::launch::failure::{Action, ContentDir, UrlKey};

use super::super::rule::{Matcher, Rule, r, r_on};
use super::super::{Category, Family, Platform, RuleId, StageMask};

pub(super) const GL_INVALID_OPERATION: Rule = r(
    RuleId::GlInvalidOperation,
    Category::Graphics,
    Family::Graphics,
    StageMask::ALL,
    3,
    Matcher::Any(&["1282: Invalid operation"]),
    "diagnose.reason.gl_invalid",
    "diagnose.advice.disable_shaders",
    &[Action::OpenContentDirectory(ContentDir::Shaders)],
);

pub(super) const OPENGL_UNSUPPORTED: Rule = r(
    RuleId::OpenGlUnsupported,
    Category::Graphics,
    Family::Graphics,
    StageMask::ALL,
    4,
    Matcher::Any(&["The driver does not appear to support OpenGL"]),
    "diagnose.reason.opengl_unsupported",
    "diagnose.advice.update_driver",
    &[Action::OpenUrl(UrlKey::Gpu), Action::OpenInstanceDirectory],
);

pub(super) const GRAPHICS_DRIVER_CRASH: Rule = r(
    RuleId::GraphicsDriverCrash,
    Category::Graphics,
    Family::Graphics,
    StageMask::ALL,
    4,
    // Vendor DLLs by file name: bare `ig` or `atio` would match half the log.
    Matcher::Any(&[
        "Pixel format not accelerated",
        "Couldn't set pixel format",
        "GLX: Failed to create context",
        "org.lwjgl.LWJGLException",
        "nvoglv64.dll",
        "nvoglv32.dll",
        "atio6axx.dll",
        "atioogl64.dll",
        "ig7icd64.dll",
        "ig9icd64.dll",
        "igd10iumd64.dll",
    ]),
    "diagnose.reason.graphics_driver",
    "diagnose.advice.update_driver",
    &[Action::OpenUrl(UrlKey::Gpu)],
);

pub(super) const RESOURCE_PACK_RESOLUTION_TOO_HIGH: Rule = r(
    RuleId::ResourcePackResolutionTooHigh,
    Category::Graphics,
    Family::Graphics,
    StageMask::IN_GAME,
    3,
    Matcher::Any(&[
        "Maybe try a lower resolution resourcepack",
        "lowerresolution texturepack",
    ]),
    "diagnose.reason.resource_pack_too_high",
    "diagnose.advice.lower_resource_pack",
    &[Action::OpenContentDirectory(ContentDir::ResourcePacks)],
);

pub(super) const MAC_WINDOW_SERVICE_PORT: Rule = r_on(
    Platform::MacOs,
    RuleId::MacWindowServicePort,
    Category::Graphics,
    Family::Graphics,
    StageMask::STARTUP,
    4,
    Matcher::Any(&[
        "GLFW error before init: [0x10008]Cocoa: Failed to find service port for display",
    ]),
    "diagnose.reason.mac_window",
    "diagnose.advice.mac_window",
    &[],
);

#[cfg(test)]
mod tests {
    use super::super::super::rule_id::RuleId;
    use super::super::testkit::fires_on;

    #[test]
    fn a_pixel_format_failure_never_claims_a_lower_resolution() {
        assert_eq!(
            fires_on("[stdout] Pixel format not accelerated"),
            Some(RuleId::GraphicsDriverCrash)
        );
    }

    /// No fixture: never fires on Windows. The matcher is checked directly.
    #[test]
    fn the_macos_service_port_rule_matches_its_line() {
        let text = crate::core::diagnose::engine::Text::parse(
            "[stdout] java.lang.IllegalStateException: GLFW error before init: [0x10008]Cocoa: Failed to find service port for display",
        );
        assert!(
            crate::core::diagnose::engine::scan(&text, &super::MAC_WINDOW_SERVICE_PORT).is_some()
        );
    }

    #[test]
    fn the_resource_pack_hint_is_its_own_rule() {
        assert_eq!(
            fires_on(
                "[stdout] LWJGL Version 3.3.3\n[stdout] Maybe try a lower resolution resourcepack"
            ),
            Some(RuleId::ResourcePackResolutionTooHigh)
        );
        assert_eq!(
            fires_on("[stdout] LWJGL Version 3.3.3\n[stdout] 1282: Invalid operation"),
            Some(RuleId::GlInvalidOperation)
        );
    }
}
