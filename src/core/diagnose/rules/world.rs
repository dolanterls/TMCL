//! The world itself: an entity or a block that will not tick.

use crate::core::launch::failure::{Action, UrlKey};

use super::super::rule::{Matcher, Rule, r};
use super::super::{Capture, Category, Family, RuleId, StageMask};

pub(super) const ENTITY_CRASH: Rule = r(
    RuleId::EntityCrash,
    Category::World,
    Family::WorldData,
    StageMask::IN_GAME,
    4,
    Matcher::Pair {
        first: "Entity Type:",
        second: "Entity's Exact location:",
        a: Capture::EntityType,
        b: Capture::Location,
    },
    "diagnose.reason.world_entity",
    "diagnose.advice.edit_world",
    &[Action::OpenUrl(UrlKey::Map), Action::OpenCrashReport],
);

pub(super) const BLOCK_CRASH: Rule = r(
    RuleId::BlockCrash,
    Category::World,
    Family::WorldData,
    StageMask::IN_GAME,
    4,
    Matcher::Pair {
        first: "Block:",
        second: "Block location:",
        a: Capture::EntityType,
        b: Capture::Location,
    },
    "diagnose.reason.world_block",
    "diagnose.advice.edit_world",
    &[Action::OpenUrl(UrlKey::Map), Action::OpenCrashReport],
);

#[cfg(test)]
mod tests {
    use super::super::super::rule_id::RuleId;
    use super::super::testkit::primary;

    #[test]
    fn an_entity_crash_carries_the_type_and_the_coordinates() {
        let primary = primary(
            "[stdout] LWJGL Version 3.3.3\n\
             [stdout] Entity Type: minecraft:zombie\n\
             [stdout] Entity's Exact location: 12.5, 64.0, -3.5",
        )
        .expect("finding");
        assert_eq!(primary.rule, RuleId::EntityCrash);
        assert_eq!(primary.captures[0].1, "minecraft:zombie");
        assert_eq!(primary.captures[1].1, "12.5, 64.0, -3.5");
    }

    #[test]
    fn a_block_crash_keeps_the_namespace() {
        let primary = primary(
            "[stdout] LWJGL Version 3.3.3\n\
             [stdout] Block: minecraft:oak_sign\n\
             [stdout] Block location: World: (0,64,0)",
        )
        .expect("finding");
        assert_eq!(primary.rule, RuleId::BlockCrash);
        assert_eq!(primary.captures[0].1, "minecraft:oak_sign");
    }

    /// A world crash can only happen inside a running game. The same two lines in a
    /// bootstrap log are a mod printing its own diagnostics, and without the stage
    /// gate they are reported as a world that will not tick.
    #[test]
    fn a_world_crash_needs_a_running_game() {
        let bootstrap = "[stdout] [19:02:11] [main/INFO]: Loading 42 mods\n\
                         [stdout] [19:02:12] [main/INFO]: Entity Type: minecraft:zombie\n\
                         [stdout] [19:02:13] [main/INFO]: Entity's Exact location: 1, 2, 3\n\
                         [stdout] [19:02:14] [main/INFO]: Block: minecraft:oak_sign\n\
                         [stdout] [19:02:14] [main/INFO]: Block location: World: (0,64,0)";
        assert!(
            primary(bootstrap).is_none(),
            "no window, no world: the stage gate has to reject both rules"
        );
    }
}
