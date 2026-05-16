//! Qdrant collection statistics.

use crate::core::config::Config;
use crate::services::types::StatsResult;
use crate::vector::ops::stats::stats_payload;
use std::error::Error;

pub fn map_stats_payload(payload: serde_json::Value) -> StatsResult {
    StatsResult { payload }
}

#[must_use = "stats returns a Result that should be handled"]
pub async fn stats(cfg: &Config) -> Result<StatsResult, Box<dyn Error>> {
    let payload = stats_payload(cfg)
        .await
        .map_err(|e| -> Box<dyn Error> { format!("stats query failed: {e}").into() })?;
    Ok(map_stats_payload(payload))
}
