use crate::crates::cli::commands::resolve_input_text;
use crate::crates::core::config::Config;
use crate::crates::services::query as query_service;
use std::error::Error;

/// CLI shim for the suggest command.
pub async fn run_suggest(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let focus = resolve_input_text(cfg);
    let result = query_service::suggest(cfg, focus.as_deref()).await?;
    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "suggestions": result.urls
            }))?
        );
    } else {
        for url in &result.urls {
            println!("{url}");
        }
    }
    Ok(())
}
