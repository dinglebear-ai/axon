//! `purge` service facade — delete indexed points by URL (or seed-URL prefix).
//!
//! The delete LOGIC lives in `axon-vector` (it owns the Qdrant data) and the
//! result DTO lives in `axon-api`. This is a **thin facade**, not a
//! reimplementation: it exists so every transport keeps one import surface
//! (`services::system::purge`) and gets the `Box<dyn Error>` error contract.
//! `dry_run` returns a preview (nothing deleted); destructive confirmation is
//! each transport's own concern.

use crate::types::PurgeResult;
use axon_core::config::Config;
use std::error::Error;

#[must_use = "purge returns a Result that should be handled"]
pub async fn purge(
    cfg: &Config,
    target: &str,
    prefix: bool,
    dry_run: bool,
) -> Result<PurgeResult, Box<dyn Error>> {
    axon_vector::purge(cfg, target, prefix, dry_run)
        .await
        .map_err(|e| -> Box<dyn Error> { e.into() })
}
