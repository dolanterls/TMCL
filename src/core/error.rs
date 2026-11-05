use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    Config,
    Network,
    Download,
    Version,
    Java,
    Account,
    Loader,
    Launch,
    Terminal,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("HTTP status {status} for {request}")]
    HttpStatus { status: u16, request: String },
    #[error("download error: {0}")]
    Download(String),
    #[error("version error: {0}")]
    Version(String),
    #[error("Java error: {0}")]
    Java(String),
    #[error("account error: {0}")]
    Account(String),
    #[error("loader error: {0}")]
    Loader(String),
    #[error("launch error: {0}")]
    Launch(String),
    #[error("terminal error: {0}")]
    Terminal(String),
    /// A local cover image could not be read or decoded. Covers are optional,
    /// so callers fall back to a drawn placeholder instead of failing a page.
    #[error("cover error: {0}")]
    Cover(String),
    #[error("{context}: {source}")]
    Context {
        context: String,
        #[source]
        source: Box<Error>,
    },
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl Error {
    pub fn with_context(self, context: impl Into<String>) -> Self {
        Self::Context {
            context: context.into(),
            source: Box::new(self),
        }
    }

    pub fn category(&self) -> ErrorCategory {
        match self {
            Self::Config(_) | Self::Cover(_) | Self::Io(_) | Self::Json(_) => ErrorCategory::Config,
            Self::Network(_) | Self::HttpStatus { .. } => ErrorCategory::Network,
            Self::Download(_) => ErrorCategory::Download,
            Self::Version(_) => ErrorCategory::Version,
            Self::Java(_) => ErrorCategory::Java,
            Self::Account(_) => ErrorCategory::Account,
            Self::Loader(_) => ErrorCategory::Loader,
            Self::Launch(_) => ErrorCategory::Launch,
            Self::Terminal(_) => ErrorCategory::Terminal,
            Self::Context { source, .. } => source.category(),
        }
    }

    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Network(_) | Self::Download(_) | Self::Io(_) => true,
            Self::HttpStatus { status, .. } => {
                matches!(*status, 408 | 425 | 429) || (500..=599).contains(status)
            }
            Self::Context { source, .. } => source.is_retryable(),
            _ => false,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::{Error, ErrorCategory};

    #[test]
    fn categorizes_domain_errors() {
        assert_eq!(
            Error::Java("missing".into()).category(),
            ErrorCategory::Java
        );
        assert_eq!(
            Error::Launch("failed".into()).category(),
            ErrorCategory::Launch
        );
    }

    #[test]
    fn contextual_errors_preserve_retryability_and_http_status_semantics() {
        let wrapped_network =
            Error::Network("connection reset".into()).with_context("fetch Fabric profile");
        assert_eq!(wrapped_network.category(), ErrorCategory::Network);
        assert!(wrapped_network.is_retryable());

        let bad_request = Error::HttpStatus {
            status: 400,
            request: "GET profile".into(),
        }
        .with_context("fetch Fabric profile");
        assert!(!bad_request.is_retryable());

        let unavailable = Error::HttpStatus {
            status: 503,
            request: "GET profile".into(),
        }
        .with_context("fetch Fabric profile");
        assert!(unavailable.is_retryable());
    }
}
