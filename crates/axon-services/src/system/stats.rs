//! Qdrant collection statistics.

use crate::types::StatsResult;
use axon_core::config::Config;
use axon_vector::ops::stats::{
    display::print_stats_human as print_vector_stats_human, stats_payload,
};
use std::error::Error;

pub fn map_stats_payload(payload: serde_json::Value) -> StatsResult {
    StatsResult { payload }
}

pub fn print_stats_human(stats: &serde_json::Value) {
    print_vector_stats_human(stats);
}

#[must_use = "stats returns a Result that should be handled"]
pub async fn stats(cfg: &Config) -> Result<StatsResult, Box<dyn Error>> {
    let payload = stats_payload(cfg)
        .await
        .map_err(|e| -> Box<dyn Error> { format!("stats query failed: {e}").into() })?;
    Ok(map_stats_payload(payload))
}
