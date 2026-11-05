//! Where an instance's cover comes from.
//!
//! Two sources, in order:
//!
//! 1. `<instance>/icon.png` — a cover the launcher or the user put there. A
//!    modpack install can seed it from the project's artwork.
//! 2. `<instance>/saves/<world>/icon.png` — the screenshot Minecraft writes
//!    for the world list. The newest world wins, so an instance that has been
//!    played once already has a real cover without any download.
//!
//! When neither exists the caller draws a placeholder; a cover is decoration,
//! never a reason to fail a page.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::cover::Cover;

/// Cover files for `instance`, most preferred first.
///
/// The launcher-managed icon always beats a world screenshot: one is chosen
/// artwork, the other is whatever the player happened to be looking at.
pub fn candidates(instance: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    let own = instance.join("icon.png");
    if own.is_file() {
        result.push(own);
    }

    let saves = instance.join("saves");
    let Ok(entries) = std::fs::read_dir(&saves) else {
        return result;
    };
    let mut worlds: Vec<(SystemTime, PathBuf)> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|world| world.is_dir())
        .filter_map(|world| {
            let icon = world.join("icon.png");
            if !icon.is_file() {
                return None;
            }
            let modified = std::fs::metadata(&icon)
                .and_then(|meta| meta.modified())
                .unwrap_or(UNIX_EPOCH);
            Some((modified, icon))
        })
        .collect();
    // Newest first. The path tie-break keeps two icons written in the same
    // filesystem tick from swapping places between frames.
    worlds.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    result.extend(worlds.into_iter().map(|(_, path)| path));
    result
}

/// The first candidate that actually decodes.
///
/// A corrupt `icon.png` must not hide a perfectly good world screenshot, so
/// failures move on to the next path instead of aborting the lookup.
pub fn load_cover(instance: &Path) -> Option<Cover> {
    candidates(instance)
        .into_iter()
        .find_map(|path| Cover::load(&path).ok())
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::BufWriter;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime};

    use super::{candidates, load_cover};

    fn temp_instance(tag: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "tmcl-instance-cover-{tag}-{}",
            uuid::Uuid::new_v4()
        ));
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

    fn stamp(path: &Path, seconds: u64) {
        let time = SystemTime::UNIX_EPOCH + Duration::from_secs(seconds);
        File::options()
            .write(true)
            .open(path)
            .expect("open for mtime")
            .set_modified(time)
            .expect("set mtime");
    }

    #[test]
    fn the_launcher_icon_beats_every_world() {
        let root = temp_instance("own");
        write_icon(&root.join("icon.png"), [1, 2, 3, 255]);
        write_icon(&root.join("saves/New/icon.png"), [9, 9, 9, 255]);

        let found = candidates(&root);
        assert_eq!(found.first(), Some(&root.join("icon.png")));
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn the_newest_world_comes_first() {
        let root = temp_instance("newest");
        write_icon(&root.join("saves/Old/icon.png"), [1, 1, 1, 255]);
        write_icon(&root.join("saves/New/icon.png"), [2, 2, 2, 255]);
        stamp(&root.join("saves/Old/icon.png"), 1_600_000_000);
        stamp(&root.join("saves/New/icon.png"), 1_700_000_000);

        let found = candidates(&root);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0], root.join("saves/New/icon.png"));
        assert_eq!(found[1], root.join("saves/Old/icon.png"));
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn a_corrupt_launcher_icon_falls_through_to_the_world() {
        let root = temp_instance("corrupt");
        std::fs::write(root.join("icon.png"), b"not a png").expect("write corrupt");
        write_icon(&root.join("saves/Castle/icon.png"), [10, 20, 30, 255]);

        let found = load_cover(&root).expect("world cover");
        assert_eq!(found.pixel(0, 0), [10, 20, 30, 255]);
        std::fs::remove_dir_all(root).expect("clean");
    }

    #[test]
    fn a_missing_saves_directory_is_not_an_error() {
        let root = temp_instance("empty");
        assert!(candidates(&root).is_empty());
        assert!(load_cover(&root).is_none());
        std::fs::remove_dir_all(root).expect("clean");
    }
}
