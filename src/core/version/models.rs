use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionManifest {
    pub latest: LatestVersions,
    pub versions: Vec<VersionSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatestVersions {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionSummary {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub time: String,
    #[serde(rename = "releaseTime")]
    pub release_time: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub compliance_level: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VersionCategory {
    Release,
    Snapshot,
    Legacy,
    AprilFools,
}

impl VersionCategory {
    pub const ALL: [Self; 4] = [
        Self::Release,
        Self::Snapshot,
        Self::Legacy,
        Self::AprilFools,
    ];

    pub const fn index(self) -> usize {
        match self {
            Self::Release => 0,
            Self::Snapshot => 1,
            Self::Legacy => 2,
            Self::AprilFools => 3,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Release => "Release",
            Self::Snapshot => "Snapshot",
            Self::Legacy => "Legacy",
            Self::AprilFools => "April Fools",
        }
    }

    /// Translation key for the display name. Core stays language-free; the
    /// renderers look this up instead of printing [`Self::label`].
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Release => "download.version_kind.release",
            Self::Snapshot => "download.version_kind.snapshot",
            Self::Legacy => "download.version_kind.legacy",
            Self::AprilFools => "download.version_kind.april_fools",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionFilter {
    enabled: [bool; 4],
    search: String,
}

impl Default for VersionFilter {
    fn default() -> Self {
        Self {
            enabled: [true, false, false, false],
            search: String::new(),
        }
    }
}

impl VersionFilter {
    pub fn counts(&self, versions: &[VersionSummary]) -> [usize; 4] {
        let mut counts = [0; 4];
        for version in versions {
            if let Some(category) = version.category() {
                counts[category.index()] += 1;
            }
        }
        counts
    }

    pub fn visible_indices(&self, versions: &[VersionSummary]) -> Vec<usize> {
        let search = self.search.to_lowercase();
        versions
            .iter()
            .enumerate()
            .filter(|(_, version)| {
                version
                    .category()
                    .is_some_and(|category| self.enabled[category.index()])
                    && version.id.to_lowercase().contains(&search)
            })
            .map(|(index, _)| index)
            .collect()
    }

    pub const fn is_enabled(&self, category: VersionCategory) -> bool {
        self.enabled[category.index()]
    }

    pub fn toggle(&mut self, category: VersionCategory) {
        self.enabled[category.index()] = !self.enabled[category.index()];
    }

    /// Set one category on or off; the filter dialog applies a whole set at
    /// once instead of replaying toggles.
    pub fn set_enabled(&mut self, category: VersionCategory, enabled: bool) {
        self.enabled[category.index()] = enabled;
    }

    /// True when every kind is shown.
    pub fn all_enabled(&self) -> bool {
        self.enabled.iter().all(|enabled| *enabled)
    }

    /// Number of kinds currently shown.
    pub fn enabled_count(&self) -> usize {
        self.enabled.iter().filter(|enabled| **enabled).count()
    }

    pub fn search(&self) -> &str {
        &self.search
    }

    pub fn push_search(&mut self, character: char) {
        self.search.push(character);
    }

    pub fn set_search(&mut self, query: &str) {
        self.search.clear();
        self.search.push_str(query);
    }

    pub fn pop_search(&mut self) {
        self.search.pop();
    }

    pub fn clear_search(&mut self) {
        self.search.clear();
    }
}

impl VersionSummary {
    pub fn category(&self) -> Option<VersionCategory> {
        if self.release_time.contains("04-01") && semver::Version::parse(&self.id).is_err() {
            return Some(VersionCategory::AprilFools);
        }

        match self.kind.as_str() {
            "release" => Some(VersionCategory::Release),
            "snapshot" => Some(VersionCategory::Snapshot),
            "old_beta" | "old_alpha" => Some(VersionCategory::Legacy),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{VersionCategory, VersionFilter, VersionSummary};

    fn version(id: &str, kind: &str, release_time: &str) -> VersionSummary {
        VersionSummary {
            id: id.into(),
            kind: kind.into(),
            url: String::new(),
            time: release_time.into(),
            release_time: release_time.into(),
            sha1: None,
            compliance_level: None,
        }
    }

    #[test]
    fn categorizes_manifest_types_and_groups_legacy_versions() {
        assert_eq!(
            version("1.21.1", "release", "2024-09-01T00:00:00Z").category(),
            Some(VersionCategory::Release)
        );
        assert_eq!(
            version("25w14a", "snapshot", "2025-03-31T00:00:00Z").category(),
            Some(VersionCategory::Snapshot)
        );
        assert_eq!(
            version("b1.7.3", "old_beta", "2011-07-08T00:00:00Z").category(),
            Some(VersionCategory::Legacy)
        );
        assert_eq!(
            version("a1.2.3", "old_alpha", "2010-08-04T00:00:00Z").category(),
            Some(VersionCategory::Legacy)
        );
    }

    #[test]
    fn april_fools_rule_requires_april_first_and_a_non_semver_id() {
        assert_eq!(
            version("19w14∞", "snapshot", "2019-04-01T12:00:00Z").category(),
            Some(VersionCategory::AprilFools)
        );
        assert_eq!(
            version("1.14.0", "release", "2019-04-01T12:00:00Z").category(),
            Some(VersionCategory::Release)
        );
        assert_eq!(
            version("19w14∞", "snapshot", "2019-03-31T12:00:00Z").category(),
            Some(VersionCategory::Snapshot)
        );
    }

    #[test]
    fn ignores_unrecognized_manifest_types() {
        assert_eq!(
            version("custom", "unknown", "2024-09-01T00:00:00Z").category(),
            None
        );
    }

    #[test]
    fn version_filter_uses_category_union_and_case_insensitive_search() {
        let versions = vec![
            version("1.21.1", "release", "2024-09-01T00:00:00Z"),
            version("25w15a", "snapshot", "2025-04-08T00:00:00Z"),
            version("25w13a", "snapshot", "2025-03-24T00:00:00Z"),
        ];
        let mut filter = VersionFilter::default();
        assert_eq!(filter.visible_indices(&versions), vec![0]);
        filter.toggle(VersionCategory::Snapshot);
        assert_eq!(filter.visible_indices(&versions), vec![0, 1, 2]);
        filter.push_search('W');
        filter.push_search('1');
        filter.push_search('5');
        assert_eq!(filter.visible_indices(&versions), vec![1]);
        assert_eq!(filter.counts(&versions), [1, 2, 0, 0]);
    }
}
