//! Redaction-aware structured error context.
//!
//! `axon-error` carries redaction **hints** only — the actual secret detection
//! and redaction implementation live in `axon-core` or the renderer boundary
//! (see `docs/pipeline-unification/crates/axon-error/README.md`). An
//! [`ErrorContext`] entry marked non-public must never be exposed by `Display`.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Where a context entry (or error) may be surfaced.
///
/// From the "visibility" field in the error shape: `public`, `internal`,
/// `sensitive`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ErrorVisibility {
    /// Safe to render to any caller, including remote/unauthenticated.
    Public,
    /// Internal diagnostics; render only to internal/admin surfaces.
    Internal,
    /// Sensitive/secret-adjacent; never render raw, redaction required.
    Sensitive,
}

impl ErrorVisibility {
    /// Whether a value at this visibility may be surfaced in public output.
    pub fn is_public(&self) -> bool {
        matches!(self, ErrorVisibility::Public)
    }
}

/// A single redaction-aware context detail.
///
/// The `value` is a redaction *hint*, not necessarily a redacted value — the
/// renderer decides what to emit based on `visibility` and `secret_class`. When
/// `visibility` is not `Public`, callers must not surface `value` to public
/// output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct ErrorContextEntry {
    /// Detail value (a redaction hint; may be a placeholder for sensitive data).
    pub value: String,
    /// Where this entry may be surfaced.
    pub visibility: ErrorVisibility,
    /// Optional secret-class hint (e.g. `api_key`, `local_path`, `token`).
    ///
    /// A hint only — `axon-error` does not classify or redact; downstream
    /// crates use it to pick the correct redaction policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_class: Option<String>,
}

impl ErrorContextEntry {
    /// A public, non-secret entry.
    pub fn public(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            visibility: ErrorVisibility::Public,
            secret_class: None,
        }
    }

    /// A sensitive entry carrying an optional secret-class hint.
    pub fn sensitive(value: impl Into<String>, secret_class: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            visibility: ErrorVisibility::Sensitive,
            secret_class: Some(secret_class.into()),
        }
    }

    /// Whether this entry is safe to surface in public output.
    pub fn is_public(&self) -> bool {
        self.visibility.is_public()
    }
}

/// Redacted key/value error details with per-entry visibility + secret hints.
#[derive(
    Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(transparent)]
pub struct ErrorContext {
    /// Ordered detail map, keyed by a stable field name.
    pub entries: BTreeMap<String, ErrorContextEntry>,
}

impl ErrorContext {
    /// An empty context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the context has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Insert or replace an entry, returning `self` for chaining.
    pub fn insert(mut self, key: impl Into<String>, entry: ErrorContextEntry) -> Self {
        self.entries.insert(key.into(), entry);
        self
    }

    /// Insert a public, non-secret detail.
    pub fn public(self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.insert(key, ErrorContextEntry::public(value))
    }

    /// Iterate only the entries safe to surface publicly.
    pub fn public_entries(&self) -> impl Iterator<Item = (&String, &ErrorContextEntry)> {
        self.entries.iter().filter(|(_, entry)| entry.is_public())
    }
}

#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;
