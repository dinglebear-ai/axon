//! Contract `Redactor` boundary for vector payloads.
//!
//! Implements the shared redaction boundary from
//! `docs/pipeline-unification/runtime/redaction-contract.md` for the point-build
//! path. The payload builder runs metadata through [`Redactor::redact_json`]
//! before validation so that `redaction_status` reflects whether redaction
//! actually occurred (`clean` vs `redacted`) rather than a hardcoded value.
//!
//! This is deliberately self-contained in `axon-vectors`: it reuses the crate's
//! existing forbidden-value/forbidden-field detectors (`payload_redaction`) so
//! the redactor and the payload validator agree on what a secret is, without
//! pulling `axon-core` into this crate's dependency graph.
//!
//! Secret-bearing *values* whose field is not itself a required identity field
//! are scrubbed to [`REDACTION_PLACEHOLDER`] and the field is recorded in the
//! [`RedactionReport`]. Fields whose *name* is forbidden are dropped. The
//! per-chunk hard-skip for forbidden required-field values (e.g. a secret that
//! lands in `chunk_text`) stays in `point.rs` — the redactor never masks the
//! full-text body, so a genuine secret in retrievable text still trips the
//! validator and skips the chunk.

use axon_api::source::{MetadataMap, SourceKind, Visibility};
use serde_json::Value;

use crate::payload_redaction::{
    forbidden_field_name, secret_like_field_name, value_contains_secret,
    value_is_absolute_local_path,
};

pub const MODULE_NAME: &str = "redactor";

/// Placeholder substituted for every scrubbed secret value.
pub const REDACTION_PLACEHOLDER: &str = "[REDACTED]";

/// Redaction contract version stamped alongside `redaction_status` so a payload
/// records which detector generation classified it.
pub const REDACTION_VERSION: &str = "2026-07-01";

/// Surface a redaction pass is scrubbing for. Mirrors the surface table in the
/// redaction contract; the payload builder always uses [`RedactionSurface::VectorPayload`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedactionSurface {
    Logs,
    JobEvents,
    VectorPayload,
    GraphEvidence,
    MemoryRecords,
    Artifacts,
    CliJson,
    McpResponse,
}

/// Redaction status recorded on every public payload write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedactionStatus {
    /// No detector fired; the payload is unmodified.
    Clean,
    /// At least one value was scrubbed or one field was dropped.
    Redacted,
    /// Redaction could not be completed safely; the write must be blocked.
    Failed,
}

impl RedactionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::Redacted => "redacted",
            Self::Failed => "failed",
        }
    }
}

/// Context passed to every redaction call.
#[derive(Debug, Clone)]
pub struct RedactionContext {
    pub visibility_ceiling: Visibility,
    pub surface: RedactionSurface,
    pub source_kind: Option<SourceKind>,
    pub allow_internal_paths: bool,
}

impl RedactionContext {
    /// Default context for a vector-payload write: `internal` ceiling, no
    /// internal-path allowance.
    pub fn vector_payload(source_kind: Option<SourceKind>) -> Self {
        Self {
            visibility_ceiling: Visibility::Internal,
            surface: RedactionSurface::VectorPayload,
            source_kind,
            allow_internal_paths: false,
        }
    }
}

/// Outcome of a redaction pass over a JSON value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RedactionReport {
    pub status_redacted: bool,
    pub redacted_fields: Vec<String>,
    pub dropped_fields: Vec<String>,
    pub detectors_triggered: Vec<String>,
}

impl RedactionReport {
    /// Redaction status derived from what the pass actually did.
    pub fn status(&self) -> RedactionStatus {
        if self.status_redacted {
            RedactionStatus::Redacted
        } else {
            RedactionStatus::Clean
        }
    }

    fn record_redacted(&mut self, field: &str, detector: &str) {
        self.status_redacted = true;
        self.redacted_fields.push(field.to_string());
        self.push_detector(detector);
    }

    fn record_dropped(&mut self, field: &str, detector: &str) {
        self.status_redacted = true;
        self.dropped_fields.push(field.to_string());
        self.push_detector(detector);
    }

    fn push_detector(&mut self, detector: &str) {
        if !self.detectors_triggered.iter().any(|d| d == detector) {
            self.detectors_triggered.push(detector.to_string());
        }
    }
}

/// The redaction boundary. Same input + context is deterministic.
pub trait Redactor: Send + Sync {
    /// Redact secrets from free text, returning the scrubbed string.
    fn redact_text(&self, input: &str, context: &RedactionContext) -> String;

    /// Redact a JSON value in place-of, returning the scrubbed value plus a
    /// report of what changed.
    fn redact_json(&self, input: Value, context: &RedactionContext) -> (Value, RedactionReport);

    /// Classify a metadata field into a visibility band.
    fn classify_field(&self, field: &str, value: &Value) -> Visibility;
}

/// Default detector-backed redactor used by the payload builder.
///
/// Reuses `payload_redaction`'s field-name and value detectors so the redactor
/// and the payload validator never disagree.
#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultRedactor;

impl DefaultRedactor {
    pub const fn new() -> Self {
        Self
    }

    /// Fields that carry structural identity and are never dropped even if their
    /// name superficially resembles a sensitive fragment. These are stamped by
    /// the payload builder and validated as required.
    fn is_structural_field(field: &str) -> bool {
        matches!(
            field,
            "redaction_status"
                | "redaction_version"
                | "visibility"
                | "chunk_text"
                | "chunk_key"
                | "chunk_id"
                | "chunk_hash"
                | "content_hash"
        )
    }
}

impl Redactor for DefaultRedactor {
    fn redact_text(&self, input: &str, context: &RedactionContext) -> String {
        // Free-text surface (logs/traces): scrub secret-shaped values, and also
        // sensitive local paths unless the surface allows internal paths. This is
        // NOT the payload `chunk_text` path — that stays untouched in
        // `redact_json` so a body secret still skips the chunk.
        if value_contains_secret(input)
            || (!context.allow_internal_paths && value_is_absolute_local_path(input))
        {
            REDACTION_PLACEHOLDER.to_string()
        } else {
            input.to_string()
        }
    }

    fn redact_json(&self, input: Value, _context: &RedactionContext) -> (Value, RedactionReport) {
        let mut report = RedactionReport::default();
        let redacted = self.redact_value("", input, &mut report);
        (redacted, report)
    }

    fn classify_field(&self, field: &str, value: &Value) -> Visibility {
        if Self::is_structural_field(field) {
            return Visibility::Internal;
        }
        if forbidden_field_name(field) || secret_like_field_name(field) {
            return Visibility::Sensitive;
        }
        if let Value::String(text) = value
            && value_contains_secret(text)
        {
            return Visibility::Sensitive;
        }
        // Unknown metadata defaults non-public per the contract.
        Visibility::Internal
    }
}

impl DefaultRedactor {
    fn redact_value(&self, path: &str, value: Value, report: &mut RedactionReport) -> Value {
        match value {
            Value::String(text) => {
                // `chunk_text` (the retrievable body) is intentionally NOT
                // masked here: masking it would silently ship a scrubbed body,
                // hiding a real secret from the hard-skip validator. A secret in
                // the body must skip the chunk, not be laundered into the index.
                if !Self::is_structural_field(path) && value_contains_secret(&text) {
                    report.record_redacted(path, "secret_value");
                    Value::String(REDACTION_PLACEHOLDER.to_string())
                } else {
                    Value::String(text)
                }
            }
            Value::Array(items) => Value::Array(
                items
                    .into_iter()
                    .enumerate()
                    .map(|(index, item)| {
                        self.redact_value(&format!("{path}[{index}]"), item, report)
                    })
                    .collect(),
            ),
            Value::Object(map) => {
                let mut out = serde_json::Map::with_capacity(map.len());
                for (field, child) in map {
                    let child_path = if path.is_empty() {
                        field.clone()
                    } else {
                        format!("{path}.{field}")
                    };
                    // Drop fields whose *name* is secret-like (access_token,
                    // client_secret, …) — a non-fatal scrub. Hard *forbidden*
                    // field names (raw auth headers, cookies, api_key, …) are
                    // deliberately left for the payload validator to reject with
                    // a fatal `ForbiddenField`: the contract forbids those fields
                    // outright, so the write must fail closed rather than
                    // silently drop them.
                    if !Self::is_structural_field(&field)
                        && !forbidden_field_name(&field)
                        && secret_like_field_name(&field)
                    {
                        report.record_dropped(&child_path, "secret_field_name");
                        continue;
                    }
                    out.insert(field, self.redact_value(&child_path, child, report));
                }
                Value::Object(out)
            }
            other => other,
        }
    }
}

/// Redact a payload metadata map, returning the scrubbed map plus its report.
///
/// The payload builder calls this before validation; the returned status is
/// stamped into `redaction_status`.
pub fn redact_metadata(
    metadata: MetadataMap,
    context: &RedactionContext,
    redactor: &dyn Redactor,
) -> (MetadataMap, RedactionReport) {
    let value = Value::Object(metadata.0.into_iter().collect());
    let (redacted, report) = redactor.redact_json(value, context);
    let map = match redacted {
        Value::Object(map) => map.into_iter().collect(),
        _ => std::collections::BTreeMap::new(),
    };
    (MetadataMap(map), report)
}

#[cfg(test)]
#[path = "redactor_tests.rs"]
mod tests;
