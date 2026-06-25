use serde::{Deserialize, Serialize};

use axon_core::config::{AdaptiveConcurrencyConfig, Config};

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

impl AdaptiveConcurrencySnapshot {
    pub(super) fn into_config_for(self, cfg: &Config) -> Result<AdaptiveConcurrencyConfig, String> {
        let mut value: AdaptiveConcurrencyConfig = self.into();
        if !value.enabled {
            return Ok(value);
        }

        let resolved_max = value
            .max
            .unwrap_or_else(|| cfg.crawl_concurrency_limit.unwrap_or(1))
            .max(1);
        let min = value.min.max(1);
        let cap = cfg.crawl_broadcast_buffer_max.min(1024);

        if min > resolved_max {
            return Err("workers.adaptive-concurrency.min must be <= max".to_string());
        }
        if resolved_max > cap {
            return Err(
                "workers.adaptive-concurrency.max must be <= min(crawl-broadcast-buffer-max, 1024)"
                    .to_string(),
            );
        }

        value.min = min;
        value.max = Some(resolved_max);
        Ok(value)
    }
}
