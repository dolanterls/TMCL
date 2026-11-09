//! Category lists for the download-center filter.
//!
//! P2 is the cache/state layer only: the filter dialog (P3) and its entry point
//! (P4) consume these methods, but nothing renders them yet.

use std::collections::HashMap;
use std::sync::Arc;

use crate::core::resource::model::{ResourceCategory, ResourceKind, ResourceSource};
use crate::core::version::http::HttpClient;
use crate::core::version::models::VersionCategory;
use crate::tui::event::AppEvent;
use crate::tui::modal::{ChecklistItem, Modal};
use crate::tui::pages::download_center::DownloadCategory;

use crate::tui::app::App;

type CategoryKey = (ResourceSource, ResourceKind);

/// Load state of one `(source, kind)` category list.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(in crate::tui::app) enum CategoryState {
    #[default]
    Idle,
    Loading,
    Ready,
    Error(String),
}

#[derive(Debug, Default)]
struct CategoryEntry {
    state: CategoryState,
    options: Vec<ResourceCategory>,
    /// Request that owns the current `Loading` state; stale answers are dropped.
    request_id: u64,
}

/// Per-`(source, kind)` category cache.
///
/// Lives on `App` rather than in `DownloadCenterState`: switching pages or
/// sources must not throw away a list that is still valid, and the page state
/// only stores the current *selection*.
#[derive(Debug, Default)]
pub(in crate::tui::app) struct CategoryCache {
    entries: HashMap<CategoryKey, CategoryEntry>,
}

impl CategoryCache {
    pub(in crate::tui::app) fn state(&self, key: CategoryKey) -> CategoryState {
        self.entries
            .get(&key)
            .map(|entry| entry.state.clone())
            .unwrap_or_default()
    }

    pub(in crate::tui::app) fn options(&self, key: CategoryKey) -> &[ResourceCategory] {
        self.entries
            .get(&key)
            .map(|entry| entry.options.as_slice())
            .unwrap_or_default()
    }

    /// Claims the fetch slot. `false` means "already loading or loaded".
    pub(in crate::tui::app) fn begin(&mut self, key: CategoryKey, request_id: u64) -> bool {
        let entry = self.entries.entry(key).or_default();
        if matches!(entry.state, CategoryState::Loading | CategoryState::Ready) {
            return false;
        }
        entry.state = CategoryState::Loading;
        entry.options.clear();
        entry.request_id = request_id;
        true
    }

    /// Stores one answer. `false` when it belongs to a superseded request.
    pub(in crate::tui::app) fn finish(
        &mut self,
        key: CategoryKey,
        request_id: u64,
        result: Result<Vec<ResourceCategory>, String>,
    ) -> bool {
        let Some(entry) = self.entries.get_mut(&key) else {
            return false;
        };
        if entry.request_id != request_id {
            return false;
        }
        match result {
            Ok(options) => {
                entry.options = options;
                entry.state = CategoryState::Ready;
            }
            Err(error) => {
                entry.options.clear();
                entry.state = CategoryState::Error(error);
            }
        }
        true
    }

    pub(in crate::tui::app) fn invalidate(&mut self, key: CategoryKey) {
        self.entries.remove(&key);
    }
}

impl App {
    pub(in crate::tui::app) fn current_category_key(&self) -> Option<CategoryKey> {
        Some((
            self.download_center.source,
            self.download_center.category.resource_kind()?,
        ))
    }

    /// Loads the current `(source, kind)` list once; a no-op while loading or
    /// once loaded.
    pub(in crate::tui::app) fn ensure_resource_categories(&mut self) {
        let Some(key) = self.current_category_key() else {
            return;
        };
        let request_id = self.category_request_seq.saturating_add(1);
        if !self.resource_categories.begin(key, request_id) {
            return;
        }
        self.category_request_seq = request_id;
        self.spawn_resource_category_fetch(key, request_id);
    }

    /// Drops the cached list and loads it again (the filter dialog's retry).
    pub(in crate::tui::app) fn reload_resource_categories(&mut self) {
        let Some(key) = self.current_category_key() else {
            return;
        };
        self.resource_categories.invalidate(key);
        self.ensure_resource_categories();
    }

    pub(in crate::tui::app) fn category_state(&self) -> CategoryState {
        self.current_category_key()
            .map(|key| self.resource_categories.state(key))
            .unwrap_or_default()
    }

    pub(in crate::tui::app) fn category_options(&self) -> &[ResourceCategory] {
        self.current_category_key()
            .map(|key| self.resource_categories.options(key))
            .unwrap_or_default()
    }

    /// Opens the category filter for the current resource page, loading the
    /// list first when it is not cached yet.
    pub(in crate::tui::app) fn open_filter_dialog(&mut self) {
        // Game versions are filtered by kind, not by a resource category: the
        // same checklist UI, a different set of rows.
        if self.download_center.category == DownloadCategory::GameVersions {
            self.open_version_filter_dialog();
            return;
        }
        let Some(_) = self.current_category_key() else {
            return;
        };
        match self.category_state() {
            CategoryState::Ready | CategoryState::Loading => {}
            CategoryState::Idle | CategoryState::Error(_) => self.reload_resource_categories(),
        }
        self.filter_open = true;
        self.modal = Some(self.filter_dialog());
    }

    fn filter_dialog(&self) -> Modal {
        let single = self.download_center.source == ResourceSource::CurseForge;
        let (items, note, retryable) = match self.category_state() {
            CategoryState::Ready => {
                let mut categories: Vec<ChecklistItem> = self
                    .category_options()
                    .iter()
                    .map(|category| {
                        ChecklistItem::new(category.id.clone(), self.category_label(category))
                    })
                    .collect();
                // Present the list in the language the user is reading.
                categories.sort_by(|left, right| left.label.cmp(&right.label));
                let empty = categories.is_empty();
                let mut items = Vec::with_capacity(categories.len() + 1);
                items.push(ChecklistItem::new(
                    "",
                    self.i18n.text("download.filter_all"),
                ));
                items.extend(categories);
                let note = if empty {
                    Some(self.i18n.text("download.filter_empty"))
                } else if single {
                    Some(self.i18n.text("download.filter_single_hint"))
                } else {
                    None
                };
                (items, note, false)
            }
            CategoryState::Loading | CategoryState::Idle => (
                Vec::new(),
                Some(self.i18n.text("download.filter_loading")),
                false,
            ),
            CategoryState::Error(error) => (
                Vec::new(),
                Some(format!(
                    "{}: {error}",
                    self.i18n.text("download.filter_failed")
                )),
                true,
            ),
        };
        let title = self.i18n.text("download.filter_title");
        // Empty selection means "all", so the all row starts checked.
        let selected = if self.download_center.categories.is_empty() {
            vec![String::new()]
        } else {
            self.download_center.categories.clone()
        };
        Modal::checklist(title, items, &selected, single)
            .with_note(note)
            .with_retry(retryable)
    }

    /// `f` on the game-versions page: the same checklist the resource pages
    /// use, filled with the four kinds instead of resource categories.
    fn open_version_filter_dialog(&mut self) {
        self.version_filter_open = true;
        self.modal = Some(self.version_filter_dialog());
    }

    fn version_filter_dialog(&self) -> Modal {
        let counts = self.download_center.version_filter.counts(&self.versions);
        let mut items = Vec::with_capacity(VersionCategory::ALL.len() + 1);
        items.push(ChecklistItem::new(
            "",
            self.i18n.text("download.filter_all"),
        ));
        for category in VersionCategory::ALL {
            items.push(ChecklistItem::new(
                version_filter_id(category),
                format!(
                    "{} ({})",
                    self.i18n.text(category.label_key()),
                    counts[category.index()]
                ),
            ));
        }
        // "All" is a row of its own, as in the resource dialog, so the two
        // filters look and behave the same. When everything is on, only that row
        // is checked; checking it and the four kinds at once would read as five
        // separate selections.
        let mut selected = Vec::new();
        if self.download_center.version_filter.all_enabled() {
            selected.push(String::new());
        } else {
            for category in VersionCategory::ALL {
                if self.download_center.version_filter.is_enabled(category) {
                    selected.push(version_filter_id(category).to_owned());
                }
            }
        }
        Modal::checklist(
            self.i18n.text("download.filter_title"),
            items,
            &selected,
            false,
        )
    }

    /// Applies a confirmed checklist to the version filter.
    ///
    /// `Modal::ApplyChecks` drops the empty id of the "all" row, so "only all
    /// was checked" arrives here as an empty slice. Showing no kind at all is
    /// just as useless, so an empty selection means every kind.
    pub(in crate::tui::app) fn apply_version_filter(&mut self, selected: &[String]) {
        let all = selected.is_empty() || selected.iter().any(String::is_empty);
        for category in VersionCategory::ALL {
            let enabled = all || selected.iter().any(|id| id == version_filter_id(category));
            self.download_center
                .version_filter
                .set_enabled(category, enabled);
        }
        self.reset_version_selection();
    }

    /// Display name for one category: a curated translation keyed by the
    /// normalised API name (both sources return usable names), else the API
    /// name itself.
    pub(in crate::tui::app) fn category_label(&self, category: &ResourceCategory) -> String {
        self.category_name_label(&category.name)
    }

    /// Same fallback chain for a bare API name (detail-pane project tags are
    /// plain strings, not `ResourceCategory` rows).
    pub(in crate::tui::app) fn category_name_label(&self, name: &str) -> String {
        let key = format!("download.category.{}", category_slug(name));
        self.i18n.try_text(&key).unwrap_or_else(|| name.to_owned())
    }

    /// Rebuilds the open filter dialog once the list arrives (or fails).
    fn refresh_filter_dialog(&mut self) {
        if !self.filter_open {
            return;
        }
        if matches!(self.modal, Some(Modal::Checklist { .. })) {
            self.modal = Some(self.filter_dialog());
        }
    }

    /// `R` on a failed filter: reload the list and repaint the dialog.
    pub(in crate::tui::app) fn retry_filter_dialog(&mut self) {
        self.reload_resource_categories();
        self.refresh_filter_dialog();
    }

    pub(in crate::tui::app) fn apply_resource_categories(
        &mut self,
        request_id: u64,
        source: ResourceSource,
        kind: ResourceKind,
        result: Result<Vec<ResourceCategory>, String>,
    ) {
        self.resource_categories
            .finish((source, kind), request_id, result);
        self.refresh_filter_dialog();
    }

    fn spawn_resource_category_fetch(&self, key: CategoryKey, request_id: u64) {
        let sender = self.sender.clone();
        tokio::spawn(async move {
            let result = async {
                let client = Arc::new(HttpClient::new()?);
                let service =
                    crate::core::resource::ResourceService::new(super::resource_registry(client));
                let request = service.begin(key.0);
                service.categories(request, key.1).await
            }
            .await;
            let _ = sender.send(AppEvent::ResourceCategoriesLoaded {
                request_id,
                source: key.0,
                kind: key.1,
                result: result.map_err(|error| error.to_string()),
            });
        });
    }
}

/// Stable id for one version kind inside the filter checklist.
fn version_filter_id(category: VersionCategory) -> &'static str {
    match category {
        VersionCategory::Release => "release",
        VersionCategory::Snapshot => "snapshot",
        VersionCategory::Legacy => "legacy",
        VersionCategory::AprilFools => "april-fools",
    }
}

/// Lower-case slug used for the curated category translations.
fn category_slug(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::pages::download_center::DownloadCategory;

    const KEY: CategoryKey = (ResourceSource::Modrinth, ResourceKind::Mod);

    fn category(name: &str) -> ResourceCategory {
        ResourceCategory {
            id: name.to_lowercase(),
            name: name.into(),
            kind: ResourceKind::Mod,
        }
    }

    #[test]
    fn a_fetch_moves_the_entry_from_idle_to_ready() {
        let mut cache = CategoryCache::default();
        assert_eq!(cache.state(KEY), CategoryState::Idle);
        assert!(cache.begin(KEY, 1));
        assert_eq!(cache.state(KEY), CategoryState::Loading);
        assert!(cache.finish(KEY, 1, Ok(vec![category("Adventure")])));
        assert_eq!(cache.state(KEY), CategoryState::Ready);
        assert_eq!(cache.options(KEY).len(), 1);
    }

    #[test]
    fn begin_is_ignored_while_loading_or_ready() {
        let mut cache = CategoryCache::default();
        assert!(cache.begin(KEY, 1));
        assert!(!cache.begin(KEY, 2), "loading must not restart the fetch");
        assert!(cache.finish(KEY, 1, Ok(Vec::new())));
        assert!(!cache.begin(KEY, 3), "ready must not refetch");
    }

    #[test]
    fn stale_answers_are_dropped_without_touching_other_keys() {
        let mut cache = CategoryCache::default();
        assert!(cache.begin(KEY, 7));
        let other = (ResourceSource::CurseForge, ResourceKind::Modpack);
        assert!(cache.begin(other, 9));

        assert!(!cache.finish(KEY, 6, Ok(vec![category("Old")])));
        assert_eq!(cache.state(KEY), CategoryState::Loading);

        assert!(cache.finish(other, 9, Ok(vec![category("Adventure")])));
        assert_eq!(cache.state(other), CategoryState::Ready);
        assert_eq!(cache.state(KEY), CategoryState::Loading);
    }

    #[test]
    fn errors_are_kept_until_the_entry_is_invalidated() {
        let mut cache = CategoryCache::default();
        assert!(cache.begin(KEY, 1));
        assert!(cache.finish(KEY, 1, Err("offline".into())));
        assert_eq!(cache.state(KEY), CategoryState::Error("offline".into()));
        assert!(cache.options(KEY).is_empty());
        // An error may be retried on the next call, and `invalidate` forces it.
        assert!(cache.begin(KEY, 2));
        assert_eq!(cache.state(KEY), CategoryState::Loading);
        assert!(cache.finish(KEY, 2, Err("still offline".into())));
        cache.invalidate(KEY);
        assert_eq!(cache.state(KEY), CategoryState::Idle);
        assert!(cache.begin(KEY, 3));
    }

    #[test]
    fn an_empty_list_is_a_successful_load() {
        let key = (ResourceSource::Modrinth, ResourceKind::World);
        let mut cache = CategoryCache::default();
        assert!(cache.begin(key, 1));
        assert!(cache.finish(key, 1, Ok(Vec::new())));
        assert_eq!(cache.state(key), CategoryState::Ready);
        assert!(cache.options(key).is_empty());
    }

    #[test]
    fn app_stores_results_under_the_current_source_and_kind() {
        let mut app = App::new();
        app.download_center.category = DownloadCategory::Resource(ResourceKind::Modpack);
        app.download_center.source = ResourceSource::CurseForge;
        let key = app.current_category_key().expect("resource page");
        assert_eq!(key, (ResourceSource::CurseForge, ResourceKind::Modpack));

        assert!(app.resource_categories.begin(key, 11));
        app.apply_resource_categories(
            11,
            key.0,
            key.1,
            Ok(vec![ResourceCategory {
                id: "4471".into(),
                name: "Adventure".into(),
                kind: ResourceKind::Modpack,
            }]),
        );
        assert_eq!(app.category_state(), CategoryState::Ready);
        assert_eq!(app.category_options().len(), 1);

        // A stale answer changes nothing.
        app.apply_resource_categories(10, key.0, key.1, Ok(vec![category("Stale")]));
        assert_eq!(app.category_options().len(), 1);
    }

    #[test]
    fn category_labels_prefer_curated_translations_then_the_api_name() {
        let mut app = App::new();
        app.download_center.category = DownloadCategory::Resource(ResourceKind::Mod);
        app.download_center.source = ResourceSource::Modrinth;
        app.i18n = crate::i18n::I18n::new(crate::core::config::models::Language::SimplifiedChinese);
        let known = ResourceCategory {
            id: "adventure".into(),
            name: "Adventure".into(),
            kind: ResourceKind::Mod,
        };
        assert_eq!(app.category_label(&known), "冒险");
        let unknown = ResourceCategory {
            id: "custom".into(),
            name: "Custom Pack".into(),
            kind: ResourceKind::Mod,
        };
        assert_eq!(app.category_label(&unknown), "Custom Pack");
    }

    #[test]
    fn the_info_line_summary_uses_translated_labels() {
        let mut app = App::new();
        app.download_center.category = DownloadCategory::Resource(ResourceKind::Mod);
        app.download_center.source = ResourceSource::Modrinth;
        app.i18n = crate::i18n::I18n::new(crate::core::config::models::Language::SimplifiedChinese);
        let key = app.current_category_key().expect("resource page");
        assert!(app.resource_categories.begin(key, 2));
        app.apply_resource_categories(
            2,
            key.0,
            key.1,
            Ok(vec![ResourceCategory {
                id: "adventure".into(),
                name: "Adventure".into(),
                kind: key.1,
            }]),
        );
        app.download_center.categories = vec!["adventure".into()];
        assert_eq!(app.download_filter_summary(), "冒险");
        app.download_center.categories.clear();
        assert_eq!(
            app.download_filter_summary(),
            app.i18n.text("download.filter_all")
        );
    }

    #[test]
    fn slug_normalises_api_names_for_the_translation_table() {
        assert_eq!(super::category_slug("Game Mechanics"), "game-mechanics");
        assert_eq!(super::category_slug("Vanilla+"), "vanilla");
        assert_eq!(super::category_slug("API and Library"), "api-and-library");
    }

    #[test]
    fn a_ready_but_empty_list_explains_that_there_is_no_filter() {
        let mut app = App::new();
        app.download_center.category = DownloadCategory::Resource(ResourceKind::Mod);
        let key = app.current_category_key().expect("resource page");
        assert!(app.resource_categories.begin(key, 3));
        app.apply_resource_categories(3, key.0, key.1, Ok(Vec::new()));
        app.open_filter_dialog();
        match &app.modal {
            Some(Modal::Checklist { items, note, .. }) => {
                assert_eq!(items.len(), 1, "only the all row");
                assert_eq!(items[0].id, "");
                assert_eq!(
                    note.as_deref(),
                    Some(app.i18n.text("download.filter_empty").as_str())
                );
            }
            other => panic!("checklist expected, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod i18n_coverage {
    use crate::core::config::models::Language;
    use crate::i18n::I18n;

    /// Snapshot of `/v2/tag/category` (all project types TMCL serves):
    /// resourcepack, mod, modpack and shader. The dialog must never fall back to
    /// an English slug for a category Modrinth returns.
    const MODRINTH_SLUGS: &[&str] = &[
        "128x",
        "16x",
        "256x",
        "32x",
        "48x",
        "512x",
        "64x",
        "8x",
        "adventure",
        "atmosphere",
        "audio",
        "blocks",
        "bloom",
        "cartoon",
        "challenging",
        "colored-lighting",
        "combat",
        "core-shaders",
        "cursed",
        "decoration",
        "economy",
        "entities",
        "environment",
        "equipment",
        "fantasy",
        "foliage",
        "fonts",
        "food",
        "game-mechanics",
        "gui",
        "high",
        "items",
        "kitchen-sink",
        "library",
        "lightweight",
        "locale",
        "low",
        "magic",
        "management",
        "medium",
        "minigame",
        "mobs",
        "models",
        "modded",
        "multiplayer",
        "optimization",
        "path-tracing",
        "pbr",
        "potato",
        "quests",
        "realistic",
        "reflections",
        "screenshot",
        "semi-realistic",
        "shadows",
        "simplistic",
        "social",
        "storage",
        "technology",
        "themed",
        "tweaks",
        "transportation",
        "utility",
        "vanilla-like",
        "worldgen",
    ];

    #[test]
    fn every_modrinth_category_has_a_translation_in_both_languages() {
        for language in [Language::English, Language::SimplifiedChinese] {
            let i18n = I18n::new(language);
            for slug in MODRINTH_SLUGS {
                let key = format!("download.category.{slug}");
                assert!(
                    i18n.try_text(&key).is_some(),
                    "missing {key} for {language:?}"
                );
            }
        }
    }
}
