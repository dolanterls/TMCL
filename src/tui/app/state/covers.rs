//! Decoded covers, kept between frames.
//!
//! The draw path only has `&self`, so the cache uses `RefCell` the same way
//! `row_windows` and `pane_max_scroll` do. Each entry remembers which file it
//! decoded and that file's mtime; a new world screenshot or a replaced
//! `icon.png` invalidates it without rescanning every frame.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use crate::core::cover::Cover;
use crate::tui::app::App;

/// How long an entry answers from memory before the candidates are checked
/// again. One second is short enough that a freshly written world icon shows
/// up while the player is still on the page, and long enough that a 4 fps draw
/// loop never spins the disk.
const CHECK_INTERVAL: Duration = Duration::from_secs(1);

/// Covers are resized once, to the largest size any preset asks for. The
/// widget shrinks this further to the slot it was given.
const CACHE_EDGE: u32 = 64;

struct CachedCover {
    path: PathBuf,
    modified: Option<SystemTime>,
    cover: Option<Cover>,
    checked_at: Instant,
}

#[derive(Default)]
pub(in crate::tui::app) struct CoverCache {
    entries: HashMap<PathBuf, CachedCover>,
    #[cfg(test)]
    decodes: std::cell::Cell<usize>,
}

impl CoverCache {
    /// The cover for `instance`, decoded at most once per second.
    pub(in crate::tui::app) fn ensure(&mut self, instance: &Path) -> Option<Cover> {
        let now = Instant::now();
        if let Some(entry) = self.entries.get(instance)
            && now.duration_since(entry.checked_at) < CHECK_INTERVAL
        {
            return entry.cover.clone();
        }

        let candidates = crate::core::instance::cover::candidates(instance);
        let first = candidates.first().cloned();
        let modified = first
            .as_deref()
            .and_then(|path| std::fs::metadata(path).ok())
            .and_then(|meta| meta.modified().ok());

        if let Some(entry) = self.entries.get(instance)
            && entry.path == first.clone().unwrap_or_default()
            && entry.modified == modified
        {
            let cover = entry.cover.clone();
            self.entries.insert(
                instance.to_path_buf(),
                CachedCover {
                    path: first.unwrap_or_default(),
                    modified,
                    cover: cover.clone(),
                    checked_at: now,
                },
            );
            return cover;
        }

        #[cfg(test)]
        self.decodes.set(self.decodes.get() + 1);
        let cover = candidates
            .iter()
            .find_map(|path| Cover::load(path).ok())
            .map(|cover| cover.square().resized(CACHE_EDGE, CACHE_EDGE));
        self.entries.insert(
            instance.to_path_buf(),
            CachedCover {
                path: first.unwrap_or_default(),
                modified,
                cover: cover.clone(),
                checked_at: now,
            },
        );
        cover
    }

    /// Forgets one instance so the next `ensure` re-reads its candidates.
    ///
    /// A modpack install writes the pack's artwork while its instance may
    /// already be on screen showing a placeholder; waiting for [`CHECK_INTERVAL`]
    /// would leave that placeholder up for a second after the install finished.
    pub(in crate::tui::app) fn invalidate(&mut self, instance: &Path) {
        self.entries.remove(instance);
    }

    #[cfg(test)]
    fn decodes(&self) -> usize {
        self.decodes.get()
    }
}

impl App {
    /// Drops the cached cover of `instance`, if any.
    ///
    /// Called right after something writes `<instance>/icon.png`; the draw path
    /// only ever reads the cache, so it cannot invalidate itself.
    pub(in crate::tui::app) fn refresh_cover(&self, instance: &Path) {
        self.covers.borrow_mut().invalidate(instance);
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::BufWriter;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant, SystemTime};

    use super::CoverCache;

    fn temp_instance(tag: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("tmcl-cover-cache-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).expect("create instance");
        path
    }

    fn write_icon(path: &Path, rgba: [u8; 4]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        let file = File::create(path).expect("create png");
        let mut encoder = png::Encoder::new(BufWriter::new(file), 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("png header");
        writer.write_image_data(&rgba).expect("png data");
        writer.finish().expect("finish png");
    }

    fn age(cache: &mut CoverCache, instance: &Path) {
        cache.entries.get_mut(instance).expect("entry").checked_at =
            Instant::now() - Duration::from_secs(2);
    }

    #[test]
    fn a_cover_is_decoded_once_and_then_reused() {
        let root = temp_instance("reuse");
        write_icon(&root.join("icon.png"), [10, 20, 30, 255]);
        let mut cache = CoverCache::default();

        let first = cache.ensure(&root).expect("cover");
        assert_eq!((first.width(), first.height()), (64, 64));
        assert_eq!(cache.decodes(), 1);

        let second = cache.ensure(&root).expect("cover");
        assert_eq!(second.pixel(0, 0), [10, 20, 30, 255]);
        assert_eq!(cache.decodes(), 1, "the second frame must not decode");
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn a_changed_mtime_forces_a_new_decode() {
        let root = temp_instance("mtime");
        write_icon(&root.join("icon.png"), [1, 2, 3, 255]);
        let mut cache = CoverCache::default();
        cache.ensure(&root).expect("cover");

        write_icon(&root.join("icon.png"), [4, 5, 6, 255]);
        let changed = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        File::options()
            .write(true)
            .open(root.join("icon.png"))
            .expect("open")
            .set_modified(changed)
            .expect("set mtime");
        age(&mut cache, &root);

        let cover = cache.ensure(&root).expect("cover");
        assert_eq!(cover.pixel(0, 0), [4, 5, 6, 255]);
        assert_eq!(cache.decodes(), 2);
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn a_corrupt_icon_is_cached_as_none_until_the_interval_expires() {
        let root = temp_instance("corrupt");
        std::fs::write(root.join("icon.png"), b"not a png").expect("write");
        let mut cache = CoverCache::default();

        assert!(cache.ensure(&root).is_none());
        assert!(cache.ensure(&root).is_none());
        assert_eq!(cache.decodes(), 1, "the second call must hit the cache");
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn invalidate_forces_the_next_lookup_to_read_the_disk() {
        let root = temp_instance("invalidate");
        let mut cache = CoverCache::default();
        assert!(cache.ensure(&root).is_none());
        assert!(cache.ensure(&root).is_none(), "the empty answer is cached");

        write_icon(&root.join("icon.png"), [11, 22, 33, 255]);
        // Still inside the interval: the cache has no reason to look again.
        assert!(cache.ensure(&root).is_none());

        cache.invalidate(&root);
        let cover = cache.ensure(&root).expect("the new icon");
        assert_eq!(cover.pixel(0, 0), [11, 22, 33, 255]);
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn a_new_world_icon_is_picked_up_after_the_interval() {
        let root = temp_instance("new-world");
        let mut cache = CoverCache::default();
        assert!(cache.ensure(&root).is_none());

        write_icon(&root.join("saves/Castle/icon.png"), [7, 8, 9, 255]);
        age(&mut cache, &root);
        let cover = cache.ensure(&root).expect("world cover");
        assert_eq!(cover.pixel(0, 0), [7, 8, 9, 255]);
        std::fs::remove_dir_all(root).expect("clean");
    }
}
