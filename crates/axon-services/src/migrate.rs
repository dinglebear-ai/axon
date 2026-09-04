//! Service-layer wrapper for collection migration (unnamed → named vectors).
//!
//! Scrolls the source Qdrant collection, computes BM42 sparse vectors from
//! `chunk_text` payloads, and upserts named-mode points (dense + bm42) to the
//! destination collection. No TEI calls; no re-crawling.

use crate::types::MigrateResult;
use axon_core::config::Config;
use axon_core::logging::{log_info, log_warn};
use std::error::Error;

/// Run the full migration from an unnamed-vector collection to a named-mode
/// collection (dense + bm42 sparse). Returns stats about the migration.
pub async fn migrate(cfg: &Config) -> Result<MigrateResult, Box<dyn Error>> {
    let from = cfg
        .positional
        .first()
        .ok_or("migrate requires --from <source-collection>")?
        .clone();
    let to = cfg
        .positional
        .get(1)
        .ok_or("migrate requires --to <destination-collection>")?
        .clone();

    if from == to {
        return Err(anyhow::anyhow!("--from and --to must be different collections").into());
    }

    log_info(&format!("command=migrate from={from} to={to}"));

    let receipt = axon_vectors::qdrant::migrate_unnamed_collection(
        &cfg.qdrant_url,
        "qdrant-migration",
        &from,
        &to,
        256,
    )
    .await
    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let total_points = receipt.points_migrated;
    let pages = receipt.pages_processed;

    log_info(&format!(
        "migrate complete from={from} to={to} points={total_points} pages={pages}"
    ));

    // No process-wide vector-mode cache to invalidate in this process:
    // `axon-vectors` (the replacement for the legacy `axon-vector` crate)
    // resolves a collection's vector mode per request rather than caching it
    // in a process-wide static, so there is nothing stale left behind here.
    //
    // O-L2: Loud success-gated worker-restart warning.
    // Any OTHER running process (a separate `axon serve` or `axon mcp`, or one
    // still on the legacy `axon-vector` path) may still hold a stale `Unnamed`
    // mode in memory and will use dense-only retrieval for queries/embeds
    // against the new named-mode collection until restarted.
    log_warn(&format!(
        "IMPORTANT: migration complete — restart all running axon workers/servers to flush \
         their stale VectorMode cache (from={from} to={to}). Workers that are not restarted \
         will continue using dense-only retrieval instead of hybrid RRF."
    ));

    Ok(MigrateResult {
        from,
        to,
        points_migrated: total_points,
        pages_processed: pages,
    })
}
