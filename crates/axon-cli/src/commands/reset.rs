//! CLI thin wrapper for `axon reset` — delegates all destruction logic to the
//! services layer (`axon_services::reset`). Reset is dry-run by default and
//! prints the exact plan; `--yes` is required to mutate.

use axon_api::reset::ResetResult;
use axon_core::config::Config;
use axon_core::ui::{accent, muted, success, warning};
use std::error::Error;

pub async fn run_reset(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let result = axon_services::reset(cfg).await?;

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        render_human(&result);
    }
    Ok(())
}

fn render_human(result: &ResetResult) {
    if result.dry_run {
        println!(
            "{}  reset plan (dry-run — nothing was changed)",
            accent("▸")
        );
    } else {
        println!("{}  reset complete", success("✓"));
    }
    println!("{}", muted(&format!("reset_id: {}", result.reset_id)));
    println!(
        "{}",
        muted(&format!("stores: {}", result.stores.join(", ")))
    );

    for row in &result.plan {
        let marker = if row.non_empty {
            accent("•")
        } else {
            muted("•")
        };
        let count = row
            .item_count
            .map(|c| c.to_string())
            .unwrap_or_else(|| "?".to_string());
        println!(
            "  {marker} {:<9} {} items  {}",
            accent(&row.store),
            count,
            muted(&row.location)
        );
        println!("      {}", muted(&row.detail));
    }

    if result.dry_run {
        if result.all_empty() {
            println!("{}", muted("all selected stores are already empty."));
        }
        println!(
            "{}",
            muted("run again with --yes to destroy and recreate these stores."),
        );
    } else {
        println!(
            "{}",
            muted(&format!(
                "deleted: {} sqlite tables, {} qdrant collections, {} artifact files",
                result.deleted.sqlite_tables,
                result.deleted.qdrant_collections.len(),
                result.deleted.artifact_files,
            ))
        );
        println!(
            "{}",
            muted(&format!(
                "created: sqlite schema v{}, qdrant collections [{}]",
                result.created.sqlite_schema_version,
                result.created.qdrant_collections.join(", "),
            ))
        );
        if let Some(path) = &result.receipt_path {
            println!("{}", muted(&format!("receipt: {path}")));
        }
    }

    for w in &result.warnings {
        println!("  {} {}", warning("⚠"), warning(w));
    }
}
