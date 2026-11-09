//! The order a content list is shown in.
//!
//! For resource packs and shaders the order is not a display choice: the game
//! reads the enabled list in file order, so the list *is* that order. Everything
//! else is sorted by whatever the user picked.
//!
//! Both front ends ask this module. Two implementations would sooner or later
//! disagree about where a disabled pack belongs, and then reordering moves a row
//! past a neighbour that is not next in the file.

use crate::core::content::{ContentEntry, ContentKind};

/// Ordering of a content list the user can cycle through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentSort {
    Name,
    Size,
    Status,
}

impl ContentSort {
    pub const ALL: [Self; 3] = [Self::Name, Self::Size, Self::Status];

    pub fn next(self) -> Self {
        match self {
            Self::Name => Self::Size,
            Self::Size => Self::Status,
            Self::Status => Self::Name,
        }
    }
}

/// Sort `entries` for `kind`.
///
/// Resource packs and shaders ignore `sort` on purpose: enabled entries keep their
/// saved order, then the disabled ones follow by name. Sorting them any other way
/// makes the reorder keys move a row past a neighbour that is not adjacent in the
/// file the game reads.
pub fn sorted(
    mut entries: Vec<&ContentEntry>,
    kind: ContentKind,
    sort: ContentSort,
) -> Vec<&ContentEntry> {
    if matches!(kind, ContentKind::ResourcePack | ContentKind::Shader) {
        entries.sort_by_key(|entry| {
            (
                entry.order.is_none(),
                entry.order.unwrap_or(0),
                entry.name.to_lowercase(),
            )
        });
        return entries;
    }
    match sort {
        ContentSort::Name => entries.sort_by_key(|entry| entry.name.to_lowercase()),
        ContentSort::Size => entries.sort_by_key(|entry| std::cmp::Reverse(entry.size)),
        ContentSort::Status => entries.sort_by_key(|entry| {
            (
                !entry.enabled,
                entry.warning.is_none(),
                entry.name.to_lowercase(),
            )
        }),
    }
    entries
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{ContentSort, sorted};
    use crate::core::content::{ContentEntry, ContentKind, ContentMeta};

    fn entry(name: &str, size: u64, enabled: bool, order: Option<usize>) -> ContentEntry {
        ContentEntry {
            name: name.into(),
            version: String::new(),
            path: PathBuf::from(format!("{name}.jar")),
            size,
            enabled,
            order,
            badge: None,
            warning: None,
            meta: ContentMeta::Unknown,
        }
    }

    #[test]
    fn enabled_resource_packs_keep_their_saved_order() {
        let entries = [
            entry("third", 3, true, Some(1)),
            entry("first", 1, true, Some(0)),
            entry("off", 2, false, None),
        ];
        let borrowed = entries.iter().collect::<Vec<_>>();

        let names = sorted(borrowed, ContentKind::ResourcePack, ContentSort::Size)
            .iter()
            .map(|entry| entry.name.clone())
            .collect::<Vec<_>>();

        // `ContentSort::Size` is ignored: the file order wins.
        assert_eq!(names, ["first", "third", "off"]);
    }

    #[test]
    fn shaders_follow_the_same_rule_as_resource_packs() {
        let entries = [entry("b", 1, true, Some(1)), entry("a", 2, true, Some(0))];
        let borrowed = entries.iter().collect::<Vec<_>>();

        let names = sorted(borrowed, ContentKind::Shader, ContentSort::Name)
            .iter()
            .map(|entry| entry.name.clone())
            .collect::<Vec<_>>();

        assert_eq!(names, ["a", "b"]);
    }

    #[test]
    fn mods_sort_by_name_size_or_status() {
        let entries = [
            entry("zeta", 10, true, None),
            entry("alpha", 30, false, None),
            entry("mid", 20, true, None),
        ];

        let by_name = sorted(
            entries.iter().collect(),
            ContentKind::Mod,
            ContentSort::Name,
        )
        .iter()
        .map(|entry| entry.name.clone())
        .collect::<Vec<_>>();
        assert_eq!(by_name, ["alpha", "mid", "zeta"]);

        let by_size = sorted(
            entries.iter().collect(),
            ContentKind::Mod,
            ContentSort::Size,
        )
        .iter()
        .map(|entry| entry.name.clone())
        .collect::<Vec<_>>();
        assert_eq!(by_size, ["alpha", "mid", "zeta"]);

        let by_status = sorted(
            entries.iter().collect(),
            ContentKind::Mod,
            ContentSort::Status,
        )
        .iter()
        .map(|entry| entry.name.clone())
        .collect::<Vec<_>>();
        assert_eq!(by_status, ["mid", "zeta", "alpha"]);
    }

    #[test]
    fn a_sort_cycle_runs_name_size_status() {
        assert_eq!(ContentSort::Name.next(), ContentSort::Size);
        assert_eq!(ContentSort::Size.next(), ContentSort::Status);
        assert_eq!(ContentSort::Status.next(), ContentSort::Name);
    }
}
