//! Error severity classification.
//!
//! Semantics come from the "Severity Semantics" table in
//! `docs/pipeline-unification/runtime/error-handling.md`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// How severe an error is and whether it terminates the item/job.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ErrorSeverity {
    /// Informational event; not terminal.
    Info,
    /// Non-fatal issue, full behavior preserved; not terminal.
    Warning,
    /// Behavior reduced but acceptable by policy; may or may not be terminal.
    Degraded,
    /// Required work failed; terminal for the affected item/job.
    Failed,
    /// Cannot continue safely; terminal.
    Fatal,
}

impl ErrorSeverity {
    /// Whether this severity terminates the affected item/job.
    ///
    /// `Failed` and `Fatal` are terminal; `Info`, `Warning`, and `Degraded`
    /// are not.
    pub fn is_terminal(&self) -> bool {
        matches!(self, ErrorSeverity::Failed | ErrorSeverity::Fatal)
    }
}

#[cfg(test)]
#[path = "severity_tests.rs"]
mod tests;
