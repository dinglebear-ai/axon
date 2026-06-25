//! CLI thin wrapper for `axon migrate` — delegates all business logic to the
//! services layer (`src/services/migrate.rs`).

use axon_core::config::Config;
use axon_core::ui::{accent, muted};
use axon_services::migrate as migrate_service;
use std::error::Error;

pub async fn run_migrate(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let result = migrate_service::migrate(cfg).await?;

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
