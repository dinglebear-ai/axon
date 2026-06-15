use serde::{Deserialize, Serialize};

use crate::core::config::AdaptiveConcurrencyConfig;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub(super) struct AdaptiveConcurrencySnapshot {
    enabled: bool,
    min: usize,
    max: Option<usize>,
}

impl From<&AdaptiveConcurrencyConfig> for AdaptiveConcurrencySnapshot {
    fn from(value: &AdaptiveConcurrencyConfig) -> Self {
        Self {
            enabled: value.enabled,
            min: value.min,
            max: value.max,
        }
    }
}

impl From<AdaptiveConcurrencySnapshot> for AdaptiveConcurrencyConfig {
    fn from(value: AdaptiveConcurrencySnapshot) -> Self {
        Self {
            enabled: value.enabled,
            min: value.min,
            max: value.max,
        }
    }
}
