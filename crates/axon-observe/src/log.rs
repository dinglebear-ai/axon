//! Structured log field registry for the unified observability boundary.
//!
//! `LogFieldSet` is the canonical shape backing every structured log line
//! emitted for source pipeline jobs and interactive operations, per the
//! "Logs" section of
//! `docs/pipeline-unification/runtime/observability-contract.md` and the
//! `LogFieldSet` `$def` required by
//! `docs/pipeline-unification/schemas/event-schema.md`. Every log line that
//! carries job/source/phase/provider correlation should be constructed
//! through this type rather than an ad hoc `tracing` field list, so CLI/MCP/
//! REST logs and durable log sinks agree on field names.
//!
//! Redaction is a mandatory hook point: [`LogFieldSet::new`] runs `message`
//! through the shared secret redactor
//! ([`axon_core::redact::redact_secrets`]) before it is stored, and
//! [`LogFieldSet::redact_message`] re-applies the same hook for callers that
//! mutate `message` directly after construction.

pub const MODULE_NAME: &str = "log";

use axon_api::source::{JobId, PipelinePhase, ProviderId, SourceId, Timestamp};
use axon_core::redact::redact_secrets;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Log severity. Mirrors `tracing::Level`'s five levels so `LogFieldSet` can
/// be built from either a `tracing` event or a non-`tracing` log sink.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl LogLevel {
    /// The stable wire label (matches this type's serde representation).
    pub fn as_str(self) -> &'static str {
        match self {
            LogLevel::Trace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }
}

impl From<tracing::Level> for LogLevel {
    fn from(level: tracing::Level) -> Self {
        match level {
            tracing::Level::TRACE => LogLevel::Trace,
            tracing::Level::DEBUG => LogLevel::Debug,
            tracing::Level::INFO => LogLevel::Info,
            tracing::Level::WARN => LogLevel::Warn,
            tracing::Level::ERROR => LogLevel::Error,
        }
    }
}

/// The canonical structured-log field set. See the "Logs" table in
/// `runtime/observability-contract.md` for the required-field list and the
/// "Forbidden in logs" list (raw auth headers, tokens/API keys/cookies, raw
/// env values, unredacted private prompts/responses, unredacted local
/// absolute paths in public logs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LogFieldSet {
    pub timestamp: Timestamp,
    pub level: LogLevel,
    pub target: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<JobId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<SourceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<PipelinePhase>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<ProviderId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

impl LogFieldSet {
    /// Start a field set at `timestamp`/`level`/`target`, running `message`
    /// through the shared secret redactor — the mandatory redaction hook
    /// point before any log line leaves this crate.
    pub fn new(
        timestamp: Timestamp,
        level: LogLevel,
        target: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            timestamp,
            level,
            target: target.into(),
            message: redact_secrets(&message.into()),
            job_id: None,
            request_id: None,
            source_id: None,
            phase: None,
            provider_id: None,
            error_code: None,
        }
    }

    pub fn with_job_id(mut self, job_id: JobId) -> Self {
        self.job_id = Some(job_id);
        self
    }

    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }

    pub fn with_source_id(mut self, source_id: SourceId) -> Self {
        self.source_id = Some(source_id);
        self
    }

    pub fn with_phase(mut self, phase: PipelinePhase) -> Self {
        self.phase = Some(phase);
        self
    }

    pub fn with_provider_id(mut self, provider_id: ProviderId) -> Self {
        self.provider_id = Some(provider_id);
        self
    }

    pub fn with_error_code(mut self, error_code: impl Into<String>) -> Self {
        self.error_code = Some(error_code.into());
        self
    }

    /// Re-run the redaction hook over `message`. Call this after mutating
    /// `message` directly (the field is public for serde round-tripping)
    /// instead of routing the new text through [`LogFieldSet::new`].
    pub fn redact_message(&mut self) {
        self.message = redact_secrets(&self.message);
    }
}

#[cfg(test)]
#[path = "log_tests.rs"]
mod tests;
