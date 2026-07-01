//! Provider saturation / cool-down classification.
//!
//! See "Provider cooling" in
//! `docs/pipeline-unification/runtime/error-handling.md`.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A provider cooling window that prevents tight retry loops.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct ProviderCooling {
    /// The provider that is cooling, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    /// When the cooling window ends.
    pub cooldown_until: DateTime<Utc>,
    /// Human-readable reason for the cooling window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl ProviderCooling {
    /// Construct a cooling window ending at `cooldown_until`.
    pub fn new(cooldown_until: DateTime<Utc>) -> Self {
        Self {
            provider_id: None,
            cooldown_until,
            reason: None,
        }
    }

    /// Attach the cooling provider id.
    pub fn with_provider(mut self, provider_id: impl Into<String>) -> Self {
        self.provider_id = Some(provider_id.into());
        self
    }

    /// Attach a reason.
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }
}

#[cfg(test)]
#[path = "cooling_tests.rs"]
mod tests;
