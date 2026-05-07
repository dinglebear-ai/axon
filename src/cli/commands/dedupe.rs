use crate::core::config::Config;
use crate::core::logging::log_info;
use crate::core::ui::{accent, symbol_for_status};
use crate::services::system;
use std::error::Error;

pub async fn run_dedupe(cfg: &Config) -> Result<(), Box<dyn Error>> {
    log_info("command=dedupe");

    let result = system::dedupe(cfg, None).await?;

    if cfg.json_output {
        println!(
            "{}",
            serde_json::json!({
                "duplicate_groups": result.duplicate_groups,
                "deleted": result.deleted,
                "collection": cfg.collection,
            })
        );
    } else {
        println!(
            "{} deduplicated {} groups, deleted {} points from {}",
            symbol_for_status("completed"),
            result.duplicate_groups,
            result.deleted,
            accent(&cfg.collection)
        );
    }
    Ok(())
}
