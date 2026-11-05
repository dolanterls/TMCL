//! World lists for the instance page's world panel, kept between frames.
//!
//! The draw path only has `&self`, so the cache uses `RefCell` the way
//! `covers` and `row_windows` do. A short interval is enough: the list is a
//! summary, and a world that appears while the player watches the page can wait
//! a couple of seconds.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::core::instance::worlds::WorldSummary;

/// How long a list answers from memory. Longer than the cover interval because
/// a scan already stats every world, and a world list changes far less often
/// than a screenshot does.
const CHECK_INTERVAL: Duration = Duration::from_secs(2);

struct Cached {
    worlds: Vec<WorldSummary>,
    checked_at: Instant,
}

#[derive(Default)]
pub(in crate::tui::app) struct WorldCache {
    entries: HashMap<PathBuf, Cached>,
}

impl WorldCache {
    /// The worlds of `instance`, newest first.
    pub(in crate::tui::app) fn ensure(&mut self, instance: &Path) -> Vec<WorldSummary> {
        let now = Instant::now();
        if let Some(entry) = self.entries.get(instance)
            && now.duration_since(entry.checked_at) < CHECK_INTERVAL
        {
            return entry.worlds.clone();
        }
        let worlds = crate::core::instance::worlds::list(instance);
        self.entries.insert(
            instance.to_path_buf(),
            Cached {
                worlds: worlds.clone(),
                checked_at: now,
            },
        );
        worlds
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use super::WorldCache;

    fn temp_instance(tag: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("tmcl-world-cache-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).expect("create instance");
        path
    }

    fn world(instance: &Path, name: &str) {
        let dir = instance.join("saves").join(name);
        std::fs::create_dir_all(&dir).expect("create world");
        std::fs::write(dir.join("level.dat"), b"level").expect("write level");
    }

    fn age(cache: &mut WorldCache, instance: &Path) {
        cache.entries.get_mut(instance).expect("entry").checked_at =
            Instant::now() - Duration::from_secs(5);
    }

    #[test]
    fn a_list_is_reused_until_the_interval_expires() {
        let root = temp_instance("reuse");
        world(&root, "Castle");
        let mut cache = WorldCache::default();

        let first = cache.ensure(&root);
        assert_eq!(first.len(), 1);

        // A world added inside the interval is not seen yet: the cache answers
        // from memory, which is the whole point of having one.
        world(&root, "Skyblock");
        assert_eq!(cache.ensure(&root).len(), 1);

        age(&mut cache, &root);
        assert_eq!(cache.ensure(&root).len(), 2);
        std::fs::remove_dir_all(root).expect("clean");
    }
}
