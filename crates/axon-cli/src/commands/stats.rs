use axon_core::config::Config;
use axon_core::logging::log_info;
use axon_services::system;
use std::error::Error;

pub async fn run_stats(cfg: &Config) -> Result<(), Box<dyn Error>> {
    log_info("command=stats");
    let result = system::stats(cfg).await?;
    if cfg.json_output {
        crate::json::print_json_gated(&result.payload)?;
    } else {
        system::print_stats_human(&result.payload);
    }
    Ok(())
}
