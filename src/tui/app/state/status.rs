//! Typed status message shown in the footer.

use crate::tui::widgets::status::StatusKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    kind: StatusKind,
    text: String,
}

impl Status {
    pub(in crate::tui::app) fn new(kind: StatusKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
        }
    }

    pub(in crate::tui::app) fn info(text: impl Into<String>) -> Self {
        Self::new(StatusKind::Info, text)
    }

    pub(in crate::tui::app) fn success(text: impl Into<String>) -> Self {
        Self::new(StatusKind::Completed, text)
    }

    pub(in crate::tui::app) fn warning(text: impl Into<String>) -> Self {
        Self::new(StatusKind::Warning, text)
    }

    pub(in crate::tui::app) fn error(text: impl Into<String>) -> Self {
        Self::new(StatusKind::Failed, text)
    }

    pub(in crate::tui::app) fn kind(&self) -> StatusKind {
        self.kind
    }

    pub(in crate::tui::app) fn as_str(&self) -> &str {
        &self.text
    }
}

impl std::ops::Deref for Status {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.text
    }
}

impl AsRef<str> for Status {
    fn as_ref(&self) -> &str {
        &self.text
    }
}

#[cfg(test)]
mod tests {
    use super::Status;
    use crate::tui::widgets::status::StatusKind;

    #[test]
    fn constructors_set_the_kind_explicitly() {
        assert_eq!(Status::info("i").kind(), StatusKind::Info);
        assert_eq!(Status::success("ok").kind(), StatusKind::Completed);
        assert_eq!(Status::warning("careful").kind(), StatusKind::Warning);
        assert_eq!(Status::error("boom").kind(), StatusKind::Failed);
        assert_eq!(Status::error("boom").as_str(), "boom");
        assert!(Status::info("text").contains("ex"));
    }
}
