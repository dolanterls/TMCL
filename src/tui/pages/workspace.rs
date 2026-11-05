//! Instance workspace state.
//!
//! The workspace is a second-level screen under `Page::Instances`, not a new
//! page: it reuses the instances tab in the header and returns with `Esc`.
//! Keeping it out of `Page` avoids touching the seven `match Page` sites that
//! drive tabs, icons, accents and hints.

use crate::core::content::{ContentEntry, ContentKind};
use crate::core::instance::models::InstanceConfig;
use crate::tui::pages::instances::InstanceSummary;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceCategory {
    Overview,
    Launch,
    Install,
    Mods,
    ResourcePacks,
    Shaders,
    Worlds,
    /// A sub-view of [`WorkspaceCategory::Worlds`]: the data packs of one
    /// world. It is not in [`WorkspaceCategory::ALL`] because the rail keeps
    /// one tab per top-level list.
    Datapacks,
    Manage,
}

impl WorkspaceCategory {
    pub const ALL: [Self; 8] = [
        Self::Overview,
        Self::Launch,
        Self::Install,
        Self::Mods,
        Self::ResourcePacks,
        Self::Shaders,
        Self::Worlds,
        Self::Manage,
    ];

    pub fn title_key(self) -> &'static str {
        match self {
            Self::Overview => "workspace.category.overview",
            Self::Launch => "workspace.category.launch",
            Self::Install => "workspace.category.install",
            Self::Mods => "workspace.category.mods",
            Self::ResourcePacks => "workspace.category.resource_packs",
            Self::Shaders => "workspace.category.shaders",
            Self::Worlds => "workspace.category.worlds",
            Self::Datapacks => "workspace.category.datapacks",
            Self::Manage => "workspace.category.manage",
        }
    }

    pub fn index(self) -> usize {
        if self == Self::Datapacks {
            // The rail highlights the world tab the sub-view belongs to.
            return Self::Worlds.index();
        }
        Self::ALL
            .iter()
            .position(|category| *category == self)
            .unwrap_or(0)
    }

    pub fn from_index(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or(Self::Overview)
    }

    /// Content directory this category lists, if it is a content category.
    pub fn content_kind(self) -> Option<ContentKind> {
        match self {
            Self::Mods => Some(ContentKind::Mod),
            Self::ResourcePacks => Some(ContentKind::ResourcePack),
            Self::Shaders => Some(ContentKind::Shader),
            Self::Worlds => Some(ContentKind::World),
            Self::Datapacks => Some(ContentKind::Datapack),
            _ => None,
        }
    }
}

/// One adjustable launch setting of the instance workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchField {
    MemoryMode,
    MemoryMin,
    MemoryMax,
    WindowMode,
    WindowSize,
    Java,
    JvmArgs,
    GameArgs,
    Server,
}

impl LaunchField {
    pub const ALL: [Self; 9] = [
        Self::MemoryMode,
        Self::MemoryMin,
        Self::MemoryMax,
        Self::WindowMode,
        Self::WindowSize,
        Self::Java,
        Self::JvmArgs,
        Self::GameArgs,
        Self::Server,
    ];

    pub fn label_key(self) -> &'static str {
        match self {
            Self::MemoryMode => "workspace.launch.memory_mode",
            Self::MemoryMin => "workspace.launch.memory_min",
            Self::MemoryMax => "workspace.launch.memory_max",
            Self::WindowMode => "workspace.launch.window_mode",
            Self::WindowSize => "workspace.launch.window_size",
            Self::Java => "workspace.launch.java",
            Self::JvmArgs => "workspace.launch.jvm_args",
            Self::GameArgs => "workspace.launch.game_args",
            Self::Server => "workspace.launch.server",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|field| *field == self)
            .unwrap_or(0)
    }

    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }
}

/// Scanned entries per content kind. Filled by the scanner stage; until then
/// every list is empty and the rail shows zero counts.
#[derive(Debug, Clone, Default)]
pub struct ContentCache {
    pub mods: Vec<ContentEntry>,
    pub packs: Vec<ContentEntry>,
    pub shaders: Vec<ContentEntry>,
    pub worlds: Vec<ContentEntry>,
    /// Data packs of the world currently open; empty while the worlds list is
    /// shown.
    pub datapacks: Vec<ContentEntry>,
}

impl ContentCache {
    pub fn entries(&self, kind: ContentKind) -> &[ContentEntry] {
        match kind {
            ContentKind::Mod => &self.mods,
            ContentKind::ResourcePack => &self.packs,
            ContentKind::Shader => &self.shaders,
            ContentKind::World => &self.worlds,
            ContentKind::Datapack => &self.datapacks,
        }
    }

    pub fn count(&self, kind: ContentKind) -> usize {
        self.entries(kind).len()
    }

    pub fn set(&mut self, kind: ContentKind, entries: Vec<ContentEntry>) {
        match kind {
            ContentKind::Mod => self.mods = entries,
            ContentKind::ResourcePack => self.packs = entries,
            ContentKind::Shader => self.shaders = entries,
            ContentKind::World => self.worlds = entries,
            ContentKind::Datapack => self.datapacks = entries,
        }
    }
}

/// Ordering of a content list.
///
/// The rule lives in `core::content::order`; only the label key is a
/// presentation concern and stays here.
pub use crate::core::content::order::ContentSort;

pub fn sort_label_key(sort: ContentSort) -> &'static str {
    match sort {
        ContentSort::Name => "workspace.sort.name",
        ContentSort::Size => "workspace.sort.size",
        ContentSort::Status => "workspace.sort.status",
    }
}

#[derive(Debug, Clone)]
pub struct Workspace {
    pub instance: Uuid,
    pub name: String,
    /// Instance directory; also the content root for every category.
    pub root: PathBuf,
    pub category: WorkspaceCategory,
    pub selected: usize,
    /// Instance metadata, read once on open. Edits write straight back here and
    /// then to disk, so the rendered view never reads from the file system.
    pub draft: InstanceConfig,
    pub content: ContentCache,
    /// True while at least one category scan is in flight. Opening the
    /// workspace starts all four, so this must not clear on the first result.
    pub loading: bool,
    /// Scans requested but not answered yet.
    pub pending_scans: u32,
    pub scroll: usize,
    /// Total size of the instance directory, computed off the UI thread.
    pub size_bytes: Option<u64>,
    /// Live filter of the current content category.
    pub search: String,
    pub sort: ContentSort,
    /// Entry whose detail is open in the shared detail popup.
    pub detail: Option<ContentEntry>,
    /// Shader loader found by the last scan of this instance, if any.
    pub shader_loader: Option<crate::core::content::shader::ShaderLoader>,
    /// Directory stamp the cached list was produced from: (mtime, count).
    pub scan_stamps: HashMap<ContentKind, Option<(SystemTime, usize)>>,
    /// World whose data packs are listed; set only while
    /// [`WorkspaceCategory::Datapacks`] is the current view.
    pub datapack_world: Option<String>,
}

impl Workspace {
    pub fn open(summary: &InstanceSummary) -> Self {
        let draft = crate::core::config::storage::read_json::<InstanceConfig>(
            &summary.path.join("tmcl.json"),
        )
        .map(|mut config| {
            config.path = summary.path.clone();
            config
        })
        .unwrap_or_else(|_| {
            InstanceConfig::new(&summary.path, Uuid::nil(), &summary.name, &summary.version)
        });
        Self {
            instance: summary.id,
            name: summary.name.clone(),
            root: summary.path.clone(),
            category: WorkspaceCategory::Overview,
            selected: 0,
            draft,
            content: ContentCache::default(),
            loading: false,
            pending_scans: 0,
            scroll: 0,
            size_bytes: None,
            search: String::new(),
            sort: ContentSort::Name,
            detail: None,
            shader_loader: None,
            scan_stamps: HashMap::new(),
            datapack_world: None,
        }
    }

    /// Directory the current content list reads from.
    ///
    /// The rule lives in `core` so the CLI lists the same directory: a data pack
    /// lives under one world, everything else directly under the instance.
    pub fn content_directory(&self) -> PathBuf {
        match self.category.content_kind() {
            Some(kind) => crate::core::content::content_directory(
                &self.root,
                kind,
                self.datapack_world.as_deref(),
            ),
            None => self.root.clone(),
        }
    }

    pub fn move_selection(&mut self, count: usize, direction: isize) {
        if count == 0 {
            self.selected = 0;
            return;
        }
        let last = count - 1;
        self.selected = self.selected.min(last);
        if direction < 0 {
            self.selected = self.selected.saturating_sub(direction.unsigned_abs());
        } else {
            self.selected = (self.selected + direction as usize).min(last);
        }
    }

    pub fn move_category(&mut self, direction: isize) {
        let count = WorkspaceCategory::ALL.len();
        let current = self.category.index();
        let next = if direction < 0 {
            (current + count - 1) % count
        } else {
            (current + 1) % count
        };
        self.category = WorkspaceCategory::from_index(next);
        self.selected = 0;
        self.scroll = 0;
    }

    /// Rows of the current category. Content categories honour the live filter
    /// and sort order, so the cursor and the drawn list can never disagree.
    pub fn row_count(&self) -> usize {
        match self.category {
            WorkspaceCategory::Overview => 9,
            WorkspaceCategory::Launch => 9,
            WorkspaceCategory::Install => 6,
            WorkspaceCategory::Manage => 5,
            other => other
                .content_kind()
                .map(|kind| self.visible_entries(kind).len())
                .unwrap_or(0),
        }
    }

    /// Content entries after the search filter and the sort order.
    ///
    /// The order itself comes from `core::content::order`, so the CLI list and
    /// this list cannot disagree about where a disabled pack belongs.
    pub fn visible_entries(&self, kind: ContentKind) -> Vec<&ContentEntry> {
        let needle = self.search.trim().to_lowercase();
        let entries = self
            .content
            .entries(kind)
            .iter()
            .filter(|entry| {
                needle.is_empty()
                    || entry.name.to_lowercase().contains(&needle)
                    || entry.display_file_name().to_lowercase().contains(&needle)
            })
            .collect::<Vec<_>>();
        crate::core::content::order::sorted(entries, kind, self.sort)
    }

    pub fn overridden_count(&self) -> usize {
        self.draft.overrides.overridden_count()
    }
}

#[cfg(test)]
mod tests {
    use super::{ContentSort, Workspace, WorkspaceCategory};

    #[test]
    fn workspace_categories_round_trip() {
        for (index, category) in WorkspaceCategory::ALL.iter().enumerate() {
            assert_eq!(category.index(), index);
            assert_eq!(WorkspaceCategory::from_index(index), *category);
            assert!(!category.title_key().is_empty());
        }
    }

    #[test]
    fn only_content_categories_map_to_a_directory() {
        assert!(WorkspaceCategory::Overview.content_kind().is_none());
        assert!(WorkspaceCategory::Launch.content_kind().is_none());
        assert_eq!(
            WorkspaceCategory::Mods.content_kind(),
            Some(crate::core::content::ContentKind::Mod)
        );
        assert_eq!(
            WorkspaceCategory::Worlds.content_kind(),
            Some(crate::core::content::ContentKind::World)
        );
    }

    #[test]
    fn a_datapack_view_reads_inside_the_selected_world() {
        let mut workspace = Workspace::open(&crate::tui::pages::instances::InstanceSummary {
            id: uuid::Uuid::nil(),
            name: "demo".into(),
            version: "1.20.1".into(),
            loader: "Fabric".into(),
            loader_version: None,
            status: "ready".into(),
            path: std::path::PathBuf::from("C:/demo"),
            launch_count: 0,
            play_time_seconds: 0,
        });
        // A fresh workspace opens on the overview, which has no content list.
        assert_eq!(
            workspace.content_directory(),
            std::path::PathBuf::from("C:/demo")
        );
        workspace.category = WorkspaceCategory::Mods;
        assert_eq!(
            workspace.content_directory(),
            std::path::PathBuf::from("C:/demo/mods")
        );

        workspace.category = WorkspaceCategory::Datapacks;
        workspace.datapack_world = Some("survival".into());
        assert_eq!(
            workspace.content_directory(),
            std::path::PathBuf::from("C:/demo/saves/survival/datapacks")
        );
        // The rail highlights the world tab the sub-view belongs to.
        assert_eq!(
            WorkspaceCategory::Datapacks.index(),
            WorkspaceCategory::Worlds.index()
        );
        assert_eq!(
            WorkspaceCategory::Datapacks.content_kind(),
            Some(crate::core::content::ContentKind::Datapack)
        );
    }

    #[test]
    fn category_movement_wraps_and_resets_selection() {
        let mut workspace = Workspace {
            instance: uuid::Uuid::nil(),
            name: "demo".into(),
            root: std::path::PathBuf::from("C:/demo"),
            category: WorkspaceCategory::Overview,
            selected: 3,
            draft: crate::core::instance::models::InstanceConfig::new(
                "C:/demo",
                uuid::Uuid::nil(),
                "demo",
                "1.20.1",
            ),
            content: Default::default(),
            loading: false,
            pending_scans: 0,
            scroll: 2,
            size_bytes: None,
            search: String::new(),
            sort: ContentSort::Name,
            detail: None,
            shader_loader: None,
            scan_stamps: Default::default(),
            datapack_world: None,
        };
        workspace.move_category(-1);
        assert_eq!(workspace.category, WorkspaceCategory::Manage);
        assert_eq!(workspace.selected, 0);
        workspace.move_category(1);
        assert_eq!(workspace.category, WorkspaceCategory::Overview);
    }
}
