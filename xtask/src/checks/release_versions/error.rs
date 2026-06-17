use std::fmt;

use super::ReleaseResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseVersionError {
    message: String,
}

impl ReleaseVersionError {
    pub(super) fn msg(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ReleaseVersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(formatter)
    }
}

impl std::error::Error for ReleaseVersionError {}

pub(super) trait ReleaseContext<T> {
    fn release_context(self, message: impl Into<String>) -> ReleaseResult<T>;
    fn with_release_context(self, message: impl FnOnce() -> String) -> ReleaseResult<T>;
}

impl<T, E> ReleaseContext<T> for std::result::Result<T, E>
where
    E: fmt::Display,
{
    fn release_context(self, message: impl Into<String>) -> ReleaseResult<T> {
        let message = message.into();
        self.map_err(|error| ReleaseVersionError::msg(format!("{message}: {error}")))
    }

    fn with_release_context(self, message: impl FnOnce() -> String) -> ReleaseResult<T> {
        self.map_err(|error| ReleaseVersionError::msg(format!("{}: {error}", message())))
    }
}

impl<T> ReleaseContext<T> for Option<T> {
    fn release_context(self, message: impl Into<String>) -> ReleaseResult<T> {
        self.ok_or_else(|| ReleaseVersionError::msg(message))
    }

    fn with_release_context(self, message: impl FnOnce() -> String) -> ReleaseResult<T> {
        self.ok_or_else(|| ReleaseVersionError::msg(message()))
    }
}
