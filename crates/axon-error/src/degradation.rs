//! Graceful-degradation decisions.
//!
//! See "Degraded vs Failed" in
//! `docs/pipeline-unification/runtime/error-handling.md`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Whether an error path can degrade rather than fail, and why.
#[derive(
    Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
pub struct DegradationPolicy {
    /// Whether the pipeline may degrade (continue with reduced behavior).
    pub degradable: bool,
    /// Human-readable reason for the degradation decision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl DegradationPolicy {
    /// A policy that permits degradation with a reason.
    pub fn degradable(reason: impl Into<String>) -> Self {
        Self {
            degradable: true,
            reason: Some(reason.into()),
        }
    }

    /// A policy that forbids degradation (required work must fail hard).
    pub fn not_degradable() -> Self {
        Self {
            degradable: false,
            reason: None,
        }
    }
}

#[cfg(test)]
#[path = "degradation_tests.rs"]
mod tests;
