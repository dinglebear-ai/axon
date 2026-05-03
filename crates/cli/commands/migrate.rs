//! CLI thin wrapper for `axon migrate` — delegates all business logic to the
//! services layer (`crates/services/migrate.rs`).

use crate::crates::core::config::Config;
use crate::crates::core::logging::log_info;
use crate::crates::core::ui::{accent, muted};
use crate::crates::services::migrate as migrate_service;
use crate::crates::vector::ops::tei::qdrant_store::clear_collection_mode_cache;
use std::error::Error;

pub async fn run_migrate(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let result = migrate_service::migrate(cfg).await?;

    // Invalidate the process-wide VectorMode cache for both collections so that
    // long-running workers re-detect the new schema on their next embed/query.
    // Without this, workers that cached VectorMode::Unnamed for `from` (or an
    // earlier run targeting `to`) continue using dense-only search paths even
    // after migration completes.
    clear_collection_mode_cache(&result.from);
    clear_collection_mode_cache(&result.to);
    log_info(&format!(
        "migrate cache_cleared from={} to={} — separate worker processes revalidate cached legacy Unnamed mode on their next hybrid embed/query",
        result.from, result.to
    ));

    if cfg.json_output {
        println!(
            "{}",
            serde_json::json!({
                "ok": true,
                "from": result.from,
                "to": result.to,
                "points_migrated": result.points_migrated,
                "pages_processed": result.pages_processed,
            })
        );
    } else {
        println!(
            "Migration complete: {} points copied from '{}' → '{}'",
            result.points_migrated,
            accent(&result.from),
            accent(&result.to),
        );
        println!(
            "Next: set {} in your .env to use hybrid search.",
            muted(&format!("AXON_COLLECTION={}", result.to))
        );
    }

    Ok(())
}
