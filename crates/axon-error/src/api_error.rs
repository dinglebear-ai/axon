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

/// Item-level source failure projection.
///
/// This lives beside [`ApiError`] so ledger/jobs/services can attach per-item
/// failures without creating local error DTOs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct SourceItemError {
    pub source_id: String,
    pub source_item_key: String,
    pub generation: String,
    pub status: String,
    pub error_code: ErrorCode,
    /// Redacted human-readable failure message.
    pub message: String,
    pub error_stage: ErrorStage,
    pub retryable: bool,
    pub severity: ErrorSeverity,
    pub visibility: ErrorVisibility,
    pub attempt: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooldown_until: Option<DateTime<Utc>>,
    pub details: BTreeMap<String, String>,
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

    /// Set the source item key.
    pub fn with_source_item_key(mut self, source_item_key: impl Into<String>) -> Self {
        self.source_item_key = Some(source_item_key.into());
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

    /// Set the minimum retry delay in milliseconds.
    pub fn with_retry_after_ms(mut self, retry_after_ms: u64) -> Self {
        self.retry_after_ms = Some(retry_after_ms);
        self
    }

    /// Apply a full retry policy projection to this error.
    pub fn with_retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.retryable = retry_policy.retryable;
        self.retry_after_ms = retry_policy.retry_after_ms;
        self.cooldown_until = retry_policy.cooldown_until;
        self.details.insert(
            "retry_scope".to_string(),
            format!("{:?}", retry_policy.retry_scope),
        );
        self.details
            .insert("attempt".to_string(), retry_policy.attempt.to_string());
        if let Some(max_attempts) = retry_policy.max_attempts {
            self.details
                .insert("max_attempts".to_string(), max_attempts.to_string());
        }
        if let Some(backoff_ms) = retry_policy.backoff_ms {
            self.details
                .insert("backoff_ms".to_string(), backoff_ms.to_string());
        }
        self
    }

    /// Apply provider cooling metadata and mark the error retryable.
    pub fn with_provider_cooling(mut self, cooling: ProviderCooling) -> Self {
        self.provider_id = cooling.provider_id;
        self.cooldown_until = Some(cooling.cooldown_until);
        if let Some(reason) = cooling.reason {
            self.details.insert("cooling_reason".to_string(), reason);
        }
        self.retryable = true;
        self
    }

    /// Construct the fail-closed redaction error used before public writes.
    pub fn redaction_failed(surface: impl Into<String>) -> Self {
        Self::new(
            "redaction.failed",
            ErrorStage::Authorizing,
            "content could not be safely redacted",
        )
        .with_context("surface", surface.into())
        .with_severity(ErrorSeverity::Fatal)
        .with_visibility(ErrorVisibility::Public)
    }

    /// Project this error into a per-source-item error record.
    pub fn to_source_item_error(
        &self,
        source_id: impl Into<String>,
        source_item_key: impl Into<String>,
        generation: impl Into<String>,
        status: impl Into<String>,
        attempt: u32,
    ) -> SourceItemError {
        SourceItemError {
            source_id: source_id.into(),
            source_item_key: source_item_key.into(),
            generation: generation.into(),
            status: status.into(),
            error_code: self.code.clone(),
            message: self.message.clone(),
            error_stage: self.stage,
            retryable: self.retryable,
            severity: self.severity,
            visibility: self.visibility,
            attempt,
            provider_id: self.provider_id.clone(),
            retry_after_ms: self.retry_after_ms,
            cooldown_until: self.cooldown_until,
            details: self.details.clone(),
        }
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
