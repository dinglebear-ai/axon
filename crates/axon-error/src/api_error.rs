//! `ApiError` — the shared error type for the unified pipeline.
//!
//! Field shape follows the "Error Shape" section of
//! `docs/pipeline-unification/runtime/error-handling.md`. Correlation ids are
//! plain strings here because `axon-error` sits below `axon-api` and cannot see
//! its typed id newtypes.

use std::collections::BTreeMap;
use std::fmt;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::code::ErrorCode;
use crate::context::ErrorVisibility;
use crate::cooling::ProviderCooling;
use crate::degradation::DegradationPolicy;
use crate::retry::{RetryPolicy, RetryScope};
use crate::severity::ErrorSeverity;
use crate::stage::ErrorStage;

/// The shared, transport-neutral error shape.
///
/// Every crate reports failures as an `ApiError` so CLI, REST, MCP, jobs, logs,
/// and progress streams render one consistent structure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct ApiError {
    /// Stable machine code (e.g. `provider.unavailable`).
    pub code: ErrorCode,
    /// Redacted human-readable message.
    pub message: String,
    /// Pipeline/transport stage.
    pub stage: ErrorStage,
    /// Whether retry may succeed.
    pub retryable: bool,
    /// Severity classification.
    pub severity: ErrorSeverity,
    /// Where this error may be surfaced.
    pub visibility: ErrorVisibility,
    /// Redacted structured context (safe key/value pairs only).
    pub details: BTreeMap<String, String>,

    /// Job correlation id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    /// Source id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    /// Item/file/page key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_item_key: Option<String>,
    /// Document id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    /// Chunk id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chunk_id: Option<String>,
    /// Provider that failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    /// Suggested retry delay, in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
    /// Provider/job cooling timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooldown_until: Option<DateTime<Utc>>,
}

impl ApiError {
    /// Construct a new error from a code, stage, and message.
    ///
    /// `retryable` and `severity` are derived from the code's classification;
    /// `visibility` defaults to `Public`. Adjust with the builder methods or by
    /// mutating the fields directly.
    pub fn new(code: impl Into<ErrorCode>, stage: ErrorStage, message: impl Into<String>) -> Self {
        let code = code.into();
        let (severity, retryable) = code.classify();
        Self {
            code,
            message: message.into(),
            stage,
            retryable,
            severity,
            visibility: ErrorVisibility::Public,
            details: BTreeMap::new(),
            job_id: None,
            source_id: None,
            source_item_key: None,
            document_id: None,
            chunk_id: None,
            provider_id: None,
            retry_after_ms: None,
            cooldown_until: None,
        }
    }

    /// Attach a redacted context detail.
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.details.insert(key.into(), value.into());
        self
    }

    /// Set the source id.
    pub fn with_source_id(mut self, source_id: impl Into<String>) -> Self {
        self.source_id = Some(source_id.into());
        self
    }

    /// Set the job id.
    pub fn with_job_id(mut self, job_id: impl Into<String>) -> Self {
        self.job_id = Some(job_id.into());
        self
    }

    /// Set the provider id.
    pub fn with_provider_id(mut self, provider_id: impl Into<String>) -> Self {
        self.provider_id = Some(provider_id.into());
        self
    }

    /// Override the visibility of this error.
    pub fn with_visibility(mut self, visibility: ErrorVisibility) -> Self {
        self.visibility = visibility;
        self
    }

    /// Set the severity explicitly (overrides the code-derived default).
    pub fn with_severity(mut self, severity: ErrorSeverity) -> Self {
        self.severity = severity;
        self
    }

    /// Set a provider/job cooling window.
    pub fn with_cooldown_until(mut self, cooldown_until: DateTime<Utc>) -> Self {
        self.cooldown_until = Some(cooldown_until);
        self
    }

    /// Machine-readable retry policy for this error.
    ///
    /// Retry scope is inferred from the correlation ids present: a provider id
    /// yields `Provider`, otherwise a chunk/document/item/job id narrows the
    /// scope, defaulting to `Job`.
    pub fn retry_policy(&self) -> RetryPolicy {
        let retry_scope = if self.provider_id.is_some() {
            RetryScope::Provider
        } else if self.chunk_id.is_some() || self.document_id.is_some() {
            RetryScope::Document
        } else if self.source_item_key.is_some() {
            RetryScope::Item
        } else {
            RetryScope::Job
        };
        RetryPolicy {
            retryable: self.retryable,
            retry_after_ms: self.retry_after_ms,
            attempt: 0,
            max_attempts: None,
            backoff_ms: None,
            cooldown_until: self.cooldown_until,
            retry_scope,
        }
    }

    /// Graceful-degradation decision for this error.
    ///
    /// A `Degraded` severity is degradable; anything terminal is not.
    pub fn degradation_policy(&self) -> DegradationPolicy {
        if self.severity == ErrorSeverity::Degraded {
            DegradationPolicy::degradable(self.message.clone())
        } else {
            DegradationPolicy::not_degradable()
        }
    }

    /// Provider cooling window, when a cooldown is active.
    pub fn provider_cooling(&self) -> Option<ProviderCooling> {
        self.cooldown_until.map(|cooldown_until| ProviderCooling {
            provider_id: self.provider_id.clone(),
            cooldown_until,
            reason: None,
        })
    }
}

impl fmt::Display for ApiError {
    /// Redaction-safe rendering: emits only `code`, `stage`, and `message`.
    ///
    /// `details` and correlation ids are never written here — they may carry
    /// context marked non-public, and redaction is a renderer concern.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{code}] ({stage:?}) {message}",
            code = self.code,
            stage = self.stage,
            message = self.message
        )
    }
}

impl std::error::Error for ApiError {}

#[cfg(test)]
#[path = "api_error_tests.rs"]
mod tests;
