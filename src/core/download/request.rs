use std::path::PathBuf;

use super::{source::DownloadSourceSnapshot, verify::IntegrityKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CachePolicy {
    ContentAddressed,
    Etag,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadAuthentication {
    None,
    CurseForge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestinationPolicy {
    Replace,
    NoReplace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadRequest {
    pub candidate_urls: Vec<String>,
    pub destination: PathBuf,
    pub expected_sha1: Option<String>,
    pub expected_size: Option<u64>,
    pub integrity: IntegrityKind,
    pub cache_policy: CachePolicy,
    pub artifact_label: String,
    pub authentication: DownloadAuthentication,
    pub destination_policy: DestinationPolicy,
    pub policy: DownloadSourceSnapshot,
}

impl DownloadRequest {
    pub fn new(
        candidate_urls: Vec<String>,
        destination: impl Into<PathBuf>,
        expected_sha1: Option<String>,
        expected_size: Option<u64>,
        artifact_label: impl Into<String>,
    ) -> Self {
        Self {
            candidate_urls,
            destination: destination.into(),
            expected_sha1,
            expected_size,
            integrity: IntegrityKind::File,
            cache_policy: CachePolicy::ContentAddressed,
            artifact_label: artifact_label.into(),
            authentication: DownloadAuthentication::None,
            destination_policy: DestinationPolicy::Replace,
            policy: DownloadSourceSnapshot::default(),
        }
    }

    pub fn with_integrity(mut self, integrity: IntegrityKind) -> Self {
        self.integrity = integrity;
        self
    }

    pub fn with_cache_policy(mut self, cache_policy: CachePolicy) -> Self {
        self.cache_policy = cache_policy;
        self
    }

    pub fn with_authentication(mut self, authentication: DownloadAuthentication) -> Self {
        self.authentication = authentication;
        self
    }

    pub fn without_replacing(mut self) -> Self {
        self.destination_policy = DestinationPolicy::NoReplace;
        self
    }

    pub fn with_policy(mut self, policy: DownloadSourceSnapshot) -> Self {
        self.policy = policy;
        self
    }
}
