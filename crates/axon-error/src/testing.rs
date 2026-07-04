//! Fixture errors and fakes for tests and schema snapshots.
//!
//! These builders produce stable, deterministic [`ApiError`] values so tests
//! and generated schema snapshots stay reproducible.

use chrono::{DateTime, TimeZone, Utc};

use crate::api_error::ApiError;
use crate::context::ErrorVisibility;
use crate::severity::ErrorSeverity;
use crate::stage::ErrorStage;

/// A fixed timestamp used by fixtures for reproducible snapshots.
fn fixture_cooldown() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 6, 30, 20, 25, 0)
        .single()
        .expect("fixture cooldown timestamp is a valid, unambiguous UTC instant")
}

/// Construct a minimal test error from a code string and stage.
pub fn test_error(code: &str, stage: ErrorStage) -> ApiError {
    ApiError::new(code, stage, format!("test error: {code}"))
}

/// A retryable provider-outage error (embedding provider unavailable, cooling).
pub fn retryable_provider_outage() -> ApiError {
    ApiError::new(
        "provider.unavailable",
        ErrorStage::Embedding,
        "Embedding provider is unavailable.",
    )
    .with_provider_id("tei")
    .with_context("provider", "tei")
    .with_cooldown_until(fixture_cooldown())
}

/// A fatal config-failure error (redaction/config safety boundary).
pub fn fatal_config_failure() -> ApiError {
    ApiError::new(
        "redaction.config_invalid",
        ErrorStage::Validation,
        "Content could not be safely processed.",
    )
    .with_visibility(ErrorVisibility::Internal)
}

/// A degraded parser error (parser fell back but chunks remain citable).
pub fn degraded_parser() -> ApiError {
    ApiError::new(
        "parser.fallback",
        ErrorStage::ParsingContent,
        "Parser fell back to a generic strategy.",
    )
    .with_severity(ErrorSeverity::Degraded)
}

#[cfg(test)]
#[path = "testing_tests.rs"]
mod tests;
