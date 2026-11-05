use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use reqwest::Url;

use crate::core::error::{Error, Result};

use super::source::{DownloadSourceMode, Region};

const BMCLAPI: &str = "https://bmclapi2.bangbang93.com";
const MOD_MIRROR: &str = "https://mod.mcimirror.top";
const MAVEN_MIRROR: &str = "https://mirrors.cloud.tencent.com/nexus/repository/maven-public";

/// How long a host stays demoted after repeated failures.
const FAILURE_MEMORY: Duration = Duration::from_secs(600);
/// One failure is a hiccup; two inside [`FAILURE_MEMORY`] is a bad host.
const FAILURES_BEFORE_DEMOTION: u32 = 2;

#[derive(Debug, Default)]
struct HostHealth {
    failures: u32,
    last_failure: Option<Instant>,
}

fn hosts() -> &'static Mutex<HashMap<String, HostHealth>> {
    static HOSTS: OnceLock<Mutex<HashMap<String, HostHealth>>> = OnceLock::new();
    HOSTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn host_of(url: &str) -> Option<String> {
    Url::parse(url).ok()?.host_str().map(str::to_owned)
}

/// A usable answer from this host resets its record: the mirror of a provider
/// can recover after an outage.
pub fn record_success(url: &str) {
    let Some(host) = host_of(url) else {
        return;
    };
    if let Ok(mut hosts) = hosts().lock() {
        hosts.remove(&host);
    }
}

/// Records a network-level failure (timeouts, resets, DNS), not a 404: a file
/// that does not exist on one host is not a reason to stop using that host.
pub fn record_failure(url: &str) {
    let Some(host) = host_of(url) else {
        return;
    };
    if let Ok(mut hosts) = hosts().lock() {
        let health = hosts.entry(host).or_default();
        health.failures = health.failures.saturating_add(1);
        health.last_failure = Some(Instant::now());
    }
}

/// `0` healthy, `1` unknown, `2` recently failing. Sorting is stable, so the
/// region/source preference decides among hosts with the same rank.
fn rank(url: &str) -> u8 {
    let Some(host) = host_of(url) else {
        return 1;
    };
    let Ok(hosts) = hosts().lock() else {
        return 1;
    };
    let Some(health) = hosts.get(&host) else {
        return 1;
    };
    let recent = health
        .last_failure
        .is_some_and(|at| at.elapsed() < FAILURE_MEMORY);
    if recent && health.failures >= FAILURES_BEFORE_DEMOTION {
        2
    } else {
        1
    }
}

pub fn resolve_candidates(
    raw_url: &str,
    mode: DownloadSourceMode,
    region: Region,
) -> Result<Vec<String>> {
    let parsed = Url::parse(raw_url)
        .map_err(|_| Error::Network(format!("invalid download URL: {raw_url}")))?;
    if !matches!(parsed.scheme(), "https" | "http") {
        return Err(Error::Network(format!(
            "unsupported download URL scheme: {raw_url}"
        )));
    }

    let mirror = mirror_url(raw_url);
    let mut urls = Vec::new();
    match mode {
        DownloadSourceMode::Official => urls.push(raw_url.to_owned()),
        DownloadSourceMode::Mirror => {
            if let Some(mirror) = mirror {
                urls.push(mirror);
            }
            urls.push(raw_url.to_owned());
        }
        DownloadSourceMode::Default => {
            let prefer_mirror = region == Region::ChinaMainland;
            if prefer_mirror {
                if let Some(mirror) = mirror {
                    urls.push(mirror);
                }
                urls.push(raw_url.to_owned());
            } else {
                urls.push(raw_url.to_owned());
                if let Some(mirror) = mirror {
                    urls.push(mirror);
                }
            }
        }
    }

    let mut seen = HashSet::new();
    urls.retain(|url| seen.insert(url.clone()));
    // A mirror that timed out twice should not be tried first for the next
    // hundred files; a working official host goes back in front.
    urls.sort_by_key(|url| rank(url));
    if urls.is_empty() {
        return Err(Error::Network(format!(
            "no download candidates generated for {raw_url}"
        )));
    }
    Ok(urls)
}

fn mirror_url(raw: &str) -> Option<String> {
    let special = [
        (
            "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
            format!("{BMCLAPI}/mc/game/version_manifest.json"),
        ),
        (
            "https://resources.download.minecraft.net/",
            format!("{BMCLAPI}/assets/"),
        ),
    ];
    for (prefix, replacement) in special {
        if raw == prefix.trim_end_matches('/') {
            return Some(replacement);
        }
        if let Some(suffix) = raw.strip_prefix(prefix) {
            return Some(format!("{replacement}{suffix}"));
        }
    }
    if let Some(package) = raw.strip_prefix("https://piston-meta.mojang.com/v1/packages/")
        && let Some(id) = package.strip_suffix(".json")
    {
        return Some(format!("{BMCLAPI}/version/{id}/json"));
    }

    let replacements = [
        ("https://launchermeta.mojang.com", BMCLAPI),
        ("https://piston-meta.mojang.com", BMCLAPI),
        ("https://piston-data.mojang.com", BMCLAPI),
        ("https://launcher.mojang.com", BMCLAPI),
        (
            "https://libraries.minecraft.net",
            "https://bmclapi2.bangbang93.com/libraries",
        ),
        (
            "http://files.minecraftforge.net/maven",
            "https://bmclapi2.bangbang93.com/maven",
        ),
        (
            "https://files.minecraftforge.net/maven",
            "https://bmclapi2.bangbang93.com/maven",
        ),
        (
            "https://maven.minecraftforge.net",
            "https://bmclapi2.bangbang93.com/maven",
        ),
        (
            "https://maven.neoforged.net/releases",
            "https://bmclapi2.bangbang93.com/maven",
        ),
        (
            "https://meta.fabricmc.net",
            "https://bmclapi2.bangbang93.com/fabric-meta",
        ),
        (
            "https://maven.fabricmc.net",
            "https://bmclapi2.bangbang93.com/maven",
        ),
        (
            "https://authlib-injector.yushi.moe",
            "https://bmclapi2.bangbang93.com/mirrors/authlib-injector",
        ),
        ("https://repo1.maven.org/maven2", MAVEN_MIRROR),
        ("https://repo.maven.apache.org/maven2", MAVEN_MIRROR),
    ];
    for (prefix, replacement) in replacements {
        if let Some(suffix) = exact_prefix_suffix(raw, prefix) {
            return Some(format!("{replacement}{suffix}"));
        }
    }

    let fallback_replacements = [
        ("https://api.modrinth.com", MOD_MIRROR),
        ("https://cdn.modrinth.com", MOD_MIRROR),
        ("https://api.curseforge.com", MOD_MIRROR),
        ("https://edge.forgecdn.net", MOD_MIRROR),
    ];
    for (prefix, replacement) in fallback_replacements {
        if let Some(suffix) = exact_prefix_suffix(raw, prefix) {
            return Some(format!("{replacement}{suffix}"));
        }
    }
    None
}

fn exact_prefix_suffix<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    let suffix = value.strip_prefix(prefix)?;
    if suffix.is_empty() || suffix.starts_with('/') {
        Some(suffix)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{record_failure, record_success, resolve_candidates};
    use crate::core::download::{DownloadSourceMode, Region};

    #[test]
    fn a_host_that_keeps_failing_moves_behind_its_mirror() {
        let official = "https://piston-data.mojang.com/v1/objects/deadbeef/client.jar";
        // Official-first region; the mirror is the second candidate.
        let before = resolve_candidates(official, DownloadSourceMode::Default, Region::Other)
            .expect("candidates");
        assert_eq!(before[0], official);

        // A single failure is a hiccup and must not reorder anything.
        record_failure(official);
        let after_one = resolve_candidates(official, DownloadSourceMode::Default, Region::Other)
            .expect("candidates");
        assert_eq!(after_one[0], official);

        // Two inside the memory window demote the host.
        record_failure(official);
        let after_two = resolve_candidates(official, DownloadSourceMode::Default, Region::Other)
            .expect("candidates");
        assert!(
            after_two[0].contains("bmclapi2.bangbang93.com"),
            "{after_two:?}"
        );
        assert_eq!(after_two[1], official);

        // A working answer puts it back in front.
        record_success(official);
        let recovered = resolve_candidates(official, DownloadSourceMode::Default, Region::Other)
            .expect("candidates");
        assert_eq!(recovered[0], official);
    }

    #[test]
    fn modes_preserve_order_and_deduplicate_candidates() {
        let official = resolve_candidates(
            "https://libraries.minecraft.net/com/example/a.jar",
            DownloadSourceMode::Official,
            Region::ChinaMainland,
        )
        .expect("official candidate");
        assert_eq!(
            official,
            ["https://libraries.minecraft.net/com/example/a.jar"]
        );

        let mirror = resolve_candidates(
            "https://libraries.minecraft.net/com/example/a.jar",
            DownloadSourceMode::Mirror,
            Region::ChinaMainland,
        )
        .expect("mirror candidates");
        assert_eq!(
            mirror,
            [
                "https://bmclapi2.bangbang93.com/libraries/com/example/a.jar",
                "https://libraries.minecraft.net/com/example/a.jar"
            ]
        );
    }

    #[test]
    fn default_order_depends_on_region() {
        let china = resolve_candidates(
            "https://piston-meta.mojang.com/v1/packages/version.json",
            DownloadSourceMode::Default,
            Region::ChinaMainland,
        )
        .expect("China candidates");
        assert_eq!(
            china[0],
            "https://bmclapi2.bangbang93.com/version/version/json"
        );

        let global = resolve_candidates(
            "https://piston-meta.mojang.com/v1/packages/version.json",
            DownloadSourceMode::Default,
            Region::Other,
        )
        .expect("global candidates");
        assert_eq!(
            global[0],
            "https://piston-meta.mojang.com/v1/packages/version.json"
        );
    }

    #[test]
    fn preserves_unknown_urls_and_rejects_prefix_collisions() {
        let unknown = resolve_candidates(
            "https://example.test/download?a=1#fragment",
            DownloadSourceMode::Mirror,
            Region::ChinaMainland,
        )
        .expect("unknown candidate");
        assert_eq!(unknown, ["https://example.test/download?a=1#fragment"]);

        let collision = resolve_candidates(
            "https://libraries.minecraft.net.evil.test/a.jar",
            DownloadSourceMode::Mirror,
            Region::ChinaMainland,
        )
        .expect("collision candidate");
        assert_eq!(
            collision,
            ["https://libraries.minecraft.net.evil.test/a.jar"]
        );
    }

    #[test]
    fn maps_assets_and_version_manifest_to_bmclapi_paths() {
        let asset = resolve_candidates(
            "https://resources.download.minecraft.net/ab/abcdef",
            DownloadSourceMode::Mirror,
            Region::ChinaMainland,
        )
        .expect("asset candidates");
        assert_eq!(asset[0], "https://bmclapi2.bangbang93.com/assets/ab/abcdef");

        let manifest = resolve_candidates(
            "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
            DownloadSourceMode::Mirror,
            Region::ChinaMainland,
        )
        .expect("manifest candidates");
        assert_eq!(
            manifest[0],
            "https://bmclapi2.bangbang93.com/mc/game/version_manifest.json"
        );
    }
}
