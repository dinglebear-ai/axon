//! Shared `Redactor` boundary for every public write surface.
//!
//! Implements the shared redaction boundary from
//! `docs/pipeline-unification/runtime/redaction-contract.md`. Every public
//! write — vector payloads, job events, artifacts, graph evidence, memory
//! rows, CLI JSON, MCP responses, REST responses, and trace/log fields —
//! runs its outgoing value through [`Redactor::redact_json`] (structured
//! data) or [`Redactor::redact_text`] (free text) before the write happens.
//! A [`RedactionStatus::Failed`] report means the caller must not perform
//! the write at all — this boundary is fail-closed, not best-effort.
//!
//! Originally implemented only inside `axon-vectors` for vector payloads;
//! promoted here so every crate above `axon-core` in the dependency graph
//! (`axon-jobs`, `axon-memory`, `axon-graph`, `axon-cli`, `axon-mcp`,
//! `axon-web`) can share one boundary instead of re-implementing detection.

use axon_api::source::{MetadataMap, SourceKind, Visibility};
use serde_json::Value;

use super::REDACTION_PLACEHOLDER;
use super::detectors::{
    field_is_opaque_token_context, forbidden_field_name, last_field_segment,
    secret_like_field_name, value_contains_secret, value_is_high_entropy_token,
};

/// Redaction contract version stamped alongside `redaction_status` so a
/// payload records which detector generation classified it.
pub const REDACTION_VERSION: &str = "2026-07-01";

/// Surface a redaction pass is scrubbing for. Mirrors the surface table in
/// the redaction contract.
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
    RestResponse,
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

    /// Default context for a job-event write: `public` ceiling (events are
    /// surfaced over REST/MCP/CLI as progress), no internal-path allowance.
    pub fn job_event() -> Self {
        Self {
            visibility_ceiling: Visibility::Public,
            surface: RedactionSurface::JobEvents,
            source_kind: None,
            allow_internal_paths: false,
        }
    }

    /// Default context for a memory-record write: `public` ceiling (a
    /// remembered body/title is recalled back through CLI/MCP/REST in later
    /// sessions), no internal-path allowance.
    pub fn memory_record() -> Self {
        Self {
            visibility_ceiling: Visibility::Public,
            surface: RedactionSurface::MemoryRecords,
            source_kind: None,
            allow_internal_paths: false,
        }
    }

    /// Default context for an artifact **metadata** write (reset receipts,
    /// extract run summaries, …) — structured JSON audit trails, not the
    /// scraped/rendered body content an artifact may also carry (which stays
    /// unmasked, same precedent as vector-payload `chunk_text`).
    pub fn artifact_metadata() -> Self {
        Self {
            visibility_ceiling: Visibility::Public,
            surface: RedactionSurface::Artifacts,
            source_kind: None,
            allow_internal_paths: false,
        }
    }

    /// Default context for a graph node/edge `properties` write — adapter-
    /// supplied evidence metadata, not user-authored body content.
    pub fn graph_evidence() -> Self {
        Self {
            visibility_ceiling: Visibility::Public,
            surface: RedactionSurface::GraphEvidence,
            source_kind: None,
            allow_internal_paths: false,
        }
    }

    /// Default context for a REST/MCP/CLI error-response envelope — the
    /// last-mile transport boundary where an underlying error's message and
    /// details cross into a body returned to an untrusted caller.
    pub fn transport_response() -> Self {
        Self {
            visibility_ceiling: Visibility::Public,
            surface: RedactionSurface::RestResponse,
            source_kind: None,
            allow_internal_paths: false,
        }
    }

    /// Default context for a CLI `--json` render — the last-mile stdout
    /// boundary where a command's result payload crosses into machine-
    /// readable output a caller may script against or paste elsewhere.
    pub fn cli_json() -> Self {
        Self {
            visibility_ceiling: Visibility::Public,
            surface: RedactionSurface::CliJson,
            source_kind: None,
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

    /// Field count actually scrubbed in place (value replaced), not dropped.
    pub fn redacted_field_count(&self) -> u32 {
        self.redacted_fields.len() as u32
    }

    /// Field count dropped outright (secret-shaped field name).
    pub fn dropped_field_count(&self) -> u32 {
        self.dropped_fields.len() as u32
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

    /// Redact a JSON value, returning the scrubbed value plus a report of
    /// what changed.
    fn redact_json(&self, input: Value, context: &RedactionContext) -> (Value, RedactionReport);

    /// Classify a metadata field into a visibility band.
    fn classify_field(&self, field: &str, value: &Value) -> Visibility;
}

/// Default detector-backed redactor used by every write surface.
#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultRedactor;

impl DefaultRedactor {
    pub const fn new() -> Self {
        Self
    }

    /// Fields that carry structural identity and are never dropped even if
    /// their name superficially resembles a sensitive fragment.
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
        // Free-text surface (logs/traces/messages): scrub secret-shaped
        // values, and also sensitive local paths unless the surface allows
        // internal paths. This is NOT the payload `chunk_text` path — that
        // stays untouched in `redact_json` so a body secret still skips the
        // chunk instead of being laundered into the index.
        if value_contains_secret(input)
            || (!context.allow_internal_paths
                && super::detectors::value_is_absolute_local_path(input))
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
        if let Value::String(text) = value {
            if value_contains_secret(text) {
                return Visibility::Sensitive;
            }
            // Gitea/GitLab/OAuth-style opaque tokens carry no fixed prefix:
            // classify by field-name context plus value entropy, per the
            // redaction contract's "entropy checks only as a secondary
            // signal with key/path context" rule.
            if field_is_opaque_token_context(last_field_segment(field))
                && value_is_high_entropy_token(text)
            {
                return Visibility::Sensitive;
            }
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
                // masked here: masking it would silently ship a scrubbed
                // body, hiding a real secret from the hard-skip validator. A
                // secret in the body must skip the chunk, not be laundered
                // into the index.
                if Self::is_structural_field(path) {
                    return Value::String(text);
                }
                if value_contains_secret(&text) {
                    report.record_redacted(path, "secret_value");
                    return Value::String(REDACTION_PLACEHOLDER.to_string());
                }
                // Gitea/GitLab/OAuth-style opaque tokens carry no fixed
                // prefix — classify by field-name context plus value
                // entropy (secondary signal, per the redaction contract).
                if field_is_opaque_token_context(last_field_segment(path))
                    && value_is_high_entropy_token(&text)
                {
                    report.record_redacted(path, "opaque_token_entropy");
                    return Value::String(REDACTION_PLACEHOLDER.to_string());
                }
                Value::String(text)
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
                    // field names (raw auth headers, cookies, api_key, …)
                    // are left for the payload validator to reject with a
                    // fatal error: the contract forbids those fields
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

/// Upper bound on free text the redaction boundary will scan. Unbounded regex
/// scanning over attacker-controlled text is itself a DoS surface, so the
/// fail-closed contract caps input size rather than silently skipping the
/// scan on oversized input — a write this large must block, not launder
/// unscrubbed content through.
pub const MAX_REDACTABLE_TEXT_BYTES: usize = 1_000_000;

/// Redact free text via `redactor`, but fail closed instead of silently
/// scanning (or silently skipping) oversized input: `Err(RedactionStatus::
/// Failed)` when `input` exceeds [`MAX_REDACTABLE_TEXT_BYTES`]. Callers at a
/// durable write boundary (e.g. `axon-memory`'s `remember`) must block the
/// write on `Err`, never fall back to persisting `input` verbatim.
pub fn redact_text_checked(
    redactor: &impl Redactor,
    input: &str,
    context: &RedactionContext,
) -> Result<String, RedactionStatus> {
    if input.len() > MAX_REDACTABLE_TEXT_BYTES {
        return Err(RedactionStatus::Failed);
    }
    Ok(redactor.redact_text(input, context))
}

/// Redact a payload metadata map, returning the scrubbed map plus its report.
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
#[path = "boundary_tests.rs"]
mod tests;
