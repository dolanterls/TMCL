pub mod cache;
pub mod executor;
pub mod fetch;
#[cfg(test)]
mod file;
pub mod hash;
pub mod request;
pub mod resolver;
pub mod source;
pub mod verify;

pub use cache::DownloadCache;
pub use executor::{DownloadExecutor, DownloadExecutorConfig};
pub(crate) use fetch::blocking;
pub use fetch::{DownloadOutcome, download_request};
pub use hash::{sha1_file, verify_sha1};
pub use request::{CachePolicy, DestinationPolicy, DownloadAuthentication, DownloadRequest};
pub use resolver::resolve_candidates;
pub use source::{
    DownloadSource, DownloadSourceMode, DownloadSourceSnapshot, Region, Resource,
    fetch_with_fallback,
};
pub use verify::{IntegrityKind, verify_download};
