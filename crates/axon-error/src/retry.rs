//! Machine-readable retry / fail-fast classification.
//!
//! Fields come from the "Retry and Cooling" section of
//! `docs/pipeline-unification/runtime/error-handling.md`.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The scope a retry decision applies to.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RetryScope {
    /// A single source item.
    Item,
    /// A single document.
    Document,
    /// A pipeline phase.
    Phase,
    /// The whole job.
    Job,
    /// A provider (cooling window applies).
    Provider,
}

/// Machine-readable retry policy for an error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct RetryPolicy {
    /// Whether retry may succeed.
    pub retryable: bool,
    /// Minimum delay before a retry, in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
    /// Current attempt number.
    pub attempt: u32,
    /// Configured max attempts, if bounded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_attempts: Option<u32>,
    /// Next backoff duration, in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backoff_ms: Option<u64>,
    /// Provider/source cooling window, if active.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooldown_until: Option<DateTime<Utc>>,
    /// The scope this retry decision applies to.
    pub retry_scope: RetryScope,
}

impl RetryPolicy {
    /// A non-retryable, fail-fast policy scoped to a single item.
    pub fn fail_fast() -> Self {
        Self {
            retryable: false,
            retry_after_ms: None,
            attempt: 0,
            max_attempts: None,
            backoff_ms: None,
            cooldown_until: None,
            retry_scope: RetryScope::Item,
        }
    }

    /// A retryable policy for a transient failure at the given scope.
    pub fn retryable(retry_scope: RetryScope) -> Self {
        Self {
            retryable: true,
            retry_after_ms: None,
            attempt: 0,
            max_attempts: None,
            backoff_ms: None,
            cooldown_until: None,
            retry_scope,
        }
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::fail_fast()
    }
}

#[cfg(test)]
#[path = "retry_tests.rs"]
mod tests;
