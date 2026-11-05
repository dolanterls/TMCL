//! One content list, in the state the player sees it.
//!
//! Reading a list is three steps and they are always the same three: scan the
//! directory, flag duplicates (the second copy of a mod id silently loses to
//! the first), then overlay the on/off state the game itself reads. A caller
//! that skips a step gets a list that disagrees with the others, so the order
//! is fixed here instead of being repeated by each frontend.

use std::path::Path;

use crate::core::content::{ContentEntry, ContentKind, scan};

/// Entries of `kind` for one instance, ready to display.
///
/// `world` is only read for [`ContentKind::Datapack`], which lives inside one
/// world instead of at the instance root.
pub fn entries(root: &Path, kind: ContentKind, world: Option<&str>) -> Vec<ContentEntry> {
    let directory = crate::core::content::content_directory(root, kind, world);
    let mut items = scan::scan(&directory, kind);
    scan::flag_duplicate_mods(&mut items);
    scan::apply_saved_state(root, kind, &mut items);
    items
}

#[cfg(test)]
mod tests {
    use super::entries;
    use crate::core::content::ContentKind;

    fn instance(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("tmcl-load-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("mods")).expect("mods");
        root
    }

    #[test]
    fn a_missing_directory_lists_nothing_instead_of_failing() {
        let root = std::env::temp_dir().join(format!("tmcl-load-none-{}", uuid::Uuid::new_v4()));
        assert!(entries(&root, ContentKind::Mod, None).is_empty());
    }

    #[test]
    fn the_disabled_marker_is_reflected_in_the_state() {
        let root = instance("disabled");
        std::fs::write(root.join("mods/a.jar"), b"a").expect("write a");
        std::fs::write(root.join("mods/b.jar.disabled"), b"b").expect("write b");

        let items = entries(&root, ContentKind::Mod, None);
        assert_eq!(items.len(), 2);
        assert!(
            items
                .iter()
                .any(|item| item.display_file_name() == "a.jar" && item.enabled)
        );
        assert!(
            items
                .iter()
                .any(|item| item.display_file_name() == "b.jar" && !item.enabled)
        );
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn a_resource_pack_is_on_only_when_options_txt_names_it() {
        let root = instance("packs");
        std::fs::create_dir_all(root.join("resourcepacks")).expect("packs");
        std::fs::write(root.join("resourcepacks/on.zip"), b"a").expect("write on");
        std::fs::write(root.join("resourcepacks/off.zip"), b"b").expect("write off");
        std::fs::write(
            root.join("options.txt"),
            "resourcePacks:[\"vanilla\",\"file/on.zip\"]\n",
        )
        .expect("options");

        let items = entries(&root, ContentKind::ResourcePack, None);
        let on = items
            .iter()
            .find(|item| item.display_file_name() == "on.zip")
            .expect("on");
        let off = items
            .iter()
            .find(|item| item.display_file_name() == "off.zip")
            .expect("off");
        assert!(on.enabled);
        assert_eq!(on.order, Some(1), "position 1, after vanilla");
        assert!(!off.enabled);
        assert_eq!(off.order, None);
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn a_data_pack_is_read_from_inside_its_world() {
        let root = instance("datapacks");
        let packs = root.join("saves/World/datapacks");
        std::fs::create_dir_all(&packs).expect("datapacks");
        std::fs::write(packs.join("terrain.zip"), b"x").expect("write pack");

        assert!(entries(&root, ContentKind::Datapack, None).is_empty());
        assert_eq!(
            entries(&root, ContentKind::Datapack, Some("World")).len(),
            1
        );
        std::fs::remove_dir_all(root).expect("clean");
    }
}
