//! Stable machine error codes.
//!
//! An [`ErrorCode`] is a dotted string like `provider.unavailable`. The prefix
//! before the first `.` is its category. Classification (severity + retryable)
//! follows the "Error Categories" table in
//! `docs/pipeline-unification/runtime/error-handling.md`.

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::severity::ErrorSeverity;

/// A stable, machine-readable error code (e.g. `provider.unavailable`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(transparent)]
#[schema(value_type = String)]
pub struct ErrorCode(pub String);

impl ErrorCode {
    /// Construct a code from anything string-like.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The category — the prefix before the first `.` (or the whole code).
    pub fn category(&self) -> &str {
        self.0.split('.').next().unwrap_or(&self.0)
    }

    /// The dotted family — everything before the final `.` segment.
    ///
    /// Used for prefix matching like `source.acquire`.
    fn family(&self) -> &str {
        match self.0.rfind('.') {
            Some(idx) => &self.0[..idx],
            None => &self.0,
        }
    }

    /// Classify this code into `(severity, retryable)` per the error-handling
    /// contract's "Error Categories" table.
    ///
    /// Mapping (retry column of the table):
    /// - `command.*` / `action.*` / `route.*` / `source.scope.*` → not retryable
    /// - `redaction.*` → fatal, not retryable
    /// - `source.acquire.*` / `ledger.*` / `embedding.*` / `vector.*` /
    ///   `artifact.*` / `provider.*` / `prune.*` → retryable
    /// - `parser.*` / `graph.*` → degrade/depends → not retryable, degraded
    /// - `auth.*` / `source.resolve.*` / `output.*` → depends → not retryable
    /// - unknown categories default to a non-retryable failure
    pub fn classify(&self) -> (ErrorSeverity, bool) {
        let family = self.family();
        let category = self.category();

        // Longest-prefix families first.
        if family == "source.acquire" {
            return (ErrorSeverity::Failed, true);
        }
        if family == "source.scope" {
            return (ErrorSeverity::Failed, false);
        }
        if family == "source.resolve" {
            return (ErrorSeverity::Failed, false);
        }

        match category {
            // Parse/validation of a command/action/route: fail fast, no retry.
            "command" | "action" | "route" => (ErrorSeverity::Failed, false),
            // Auth failures: depends; conservatively not retryable here.
            "auth" => (ErrorSeverity::Failed, false),
            // Transient acquire/store/provider failures: retryable.
            "ledger" | "embedding" | "vector" | "artifact" | "provider" | "prune" => {
                (ErrorSeverity::Failed, true)
            }
            // Parser/graph: degrade with a fallback, so not a hard retry.
            "parser" | "graph" => (ErrorSeverity::Degraded, false),
            // Redaction: safety boundary — fatal, never retryable.
            "redaction" => (ErrorSeverity::Fatal, false),
            // Output too large / write failure: depends; not retryable.
            "output" => (ErrorSeverity::Failed, false),
            // Anything unrecognized: safe default is a non-retryable failure.
            _ => (ErrorSeverity::Failed, false),
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for ErrorCode {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<String> for ErrorCode {
    fn from(value: String) -> Self {
        Self(value)
    }
}

#[cfg(test)]
#[path = "code_tests.rs"]
mod tests;
