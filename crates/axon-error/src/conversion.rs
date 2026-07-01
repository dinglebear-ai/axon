//! The error-projection boundary.
//!
//! [`IntoApiError`] is the single conversion trait every domain/provider/store
//! error implements so it can be projected into the shared [`ApiError`] shape
//! without exposing provider internals. Concrete provider/store specifics live
//! in their own crates — `axon-error` only provides the trait and generic
//! helpers.

use crate::api_error::ApiError;
use crate::code::ErrorCode;
use crate::stage::ErrorStage;

/// Project a domain/provider/store error into the shared [`ApiError`].
///
/// This is the "ErrorProjection" boundary: implementers map their root-cause
/// class into a stable code + stage + redacted message, never leaking raw
/// provider internals into `message` or `details`.
pub trait IntoApiError {
    /// Convert `self` into the shared error shape.
    fn into_api_error(self) -> ApiError;
}

impl IntoApiError for ApiError {
    fn into_api_error(self) -> ApiError {
        self
    }
}

/// Build an [`ApiError`] from parts, deriving severity/retryable from the code.
///
/// A generic helper for conversions that only have a code, stage, and a
/// redacted message.
pub fn api_error_from_parts(
    code: impl Into<ErrorCode>,
    stage: ErrorStage,
    message: impl Into<String>,
) -> ApiError {
    ApiError::new(code, stage, message)
}

/// Project any `IntoApiError` value.
///
/// Convenience wrapper so call sites can write `project(err)` uniformly.
pub fn project(error: impl IntoApiError) -> ApiError {
    error.into_api_error()
}

#[cfg(test)]
#[path = "conversion_tests.rs"]
mod tests;
