//! Keywords for the case where no rule matched.
//!
//! A crash report's stack trace names packages and classes. Most of them are
//! Minecraft, Java or the loader; what is left is usually the mod that broke.
//! Searching those six words is what a support answer would tell the player to
//! do, so the report says it directly instead of "cause unknown".

use super::engine::Text;

/// Words that appear in every stack trace and therefore say nothing.
const COMMON: &[&str] = &[
    "net",
    "minecraft",
    "mojang",
    "java",
    "javax",
    "lang",
    "util",
    "nio",
    "io",
    "sun",
    "jdk",
    "internal",
    "reflect",
    "invoke",
    "concurrent",
    "thread",
    "runtime",
    "forge",
    "fml",
    "minecraftforge",
    "modlauncher",
    "launchwrapper",
    "cpw",
    "objectweb",
    "asm",
    "fabric",
    "fabricmc",
    "loader",
    "knot",
    "mixin",
    "spongepowered",
    "mods",
    "mod",
    "impl",
    "org",
    "com",
    "cn",
    "client",
    "server",
    "common",
    "core",
    "api",
    "lib",
    "event",
    "events",
    "handler",
    "render",
    "renderer",
    "world",
    "entity",
    "block",
    "item",
    "items",
    "level",
    "game",
    "main",
    "launch",
    "init",
    "config",
    "registry",
    "registry",
    "texture",
    "model",
    "chunk",
    "player",
    "gui",
    "screen",
    "data",
    "codec",
    "network",
    "channel",
    "server",
];

const MAX_KEYWORDS: usize = 6;
/// Segments shorter than this are noise (`v1`, `nms`, `$1`).
const MIN_SEGMENT: usize = 3;

/// Top package segments from the stack frames, most frequent first.
pub fn extract(text: &Text, start: Option<usize>) -> Vec<String> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for index in start.unwrap_or(0)..text.len() {
        let Some(frame) = frame_method(text.line(index)) else {
            continue;
        };
        let mut segments = frame.split('.').collect::<Vec<_>>();
        // The last segment is the method, not a package.
        segments.pop();
        for segment in segments {
            if !worth_keeping(segment) {
                continue;
            }
            match counts.iter_mut().find(|(word, _)| word == segment) {
                Some((_, count)) => *count += 1,
                None => counts.push((segment.to_owned(), 1)),
            }
        }
    }
    counts.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    counts
        .into_iter()
        .map(|(word, _)| word)
        .take(MAX_KEYWORDS)
        .collect()
}

/// `  at com.example.mod.MixinThing.apply(MixinThing.java:42)` → the method path.
fn frame_method(line: &str) -> Option<&str> {
    let line = Text::payload(line);
    let start = line.find("at ")? + 3;
    let rest = line.get(start..)?;
    let end = rest.find(['(', ' ']).unwrap_or(rest.len());
    let method = rest.get(..end)?.trim();
    if method.contains('.') {
        Some(method)
    } else {
        None
    }
}

fn worth_keeping(segment: &str) -> bool {
    let lowered = segment.to_ascii_lowercase();
    segment.chars().count() >= MIN_SEGMENT
        && !COMMON.contains(&lowered.as_str())
        && !segment
            .chars()
            .all(|letter| letter.is_ascii_digit() || letter == '$')
}

#[cfg(test)]
mod tests {
    use super::{Text, extract};

    #[test]
    fn extracts_the_mod_package_and_drops_the_common_layers() {
        let text = Text::parse(
            "Description: Unexpected error\n\
             \tat net.minecraft.client.Minecraft.run(Minecraft.java:1000)\n\
             \tat com.example.flywheel.FlywheelMixin.inject(FlywheelMixin.java:42)\n\
             \tat java.lang.Thread.run(Thread.java:840)\n\
             \tat com.example.flywheel.FlywheelMixin.tick(FlywheelMixin.java:51)",
        );
        let keywords = extract(&text, None);
        assert!(keywords.contains(&"example".to_owned()), "{keywords:?}");
        assert!(keywords.contains(&"flywheel".to_owned()), "{keywords:?}");
        assert!(!keywords.contains(&"minecraft".to_owned()));
        assert!(!keywords.contains(&"lang".to_owned()));
        assert!(keywords.len() <= 6);
    }

    #[test]
    fn a_log_without_frames_has_no_keywords() {
        let text = Text::parse("Starting Minecraft\nStopping!");
        assert!(extract(&text, None).is_empty());
    }
}
