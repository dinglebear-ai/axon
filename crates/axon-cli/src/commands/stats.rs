use axon_core::config::Config;
use axon_core::logging::log_info;
use axon_services::system;
use axon_vector::ops::stats::display::print_stats_human;
use std::error::Error;

pub async fn run_stats(cfg: &Config) -> Result<(), Box<dyn Error>> {
    log_info("command=stats");
    let result = system::stats(cfg).await?;
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&result.payload)?);
    } else {
        print_stats_human(&result.payload);
    }
    Ok(())
}
