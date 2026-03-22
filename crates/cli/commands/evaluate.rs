use crate::crates::cli::commands::resolve_input_text;
use crate::crates::core::config::Config;
use crate::crates::services::query as query_service;
use std::error::Error;

/// CLI shim for the evaluate command.
pub async fn run_evaluate(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let question = resolve_input_text(cfg).ok_or("evaluate requires a question")?;
    let result = query_service::evaluate(cfg, &question).await?;
    println!("{}", serde_json::to_string_pretty(&result.payload)?);
    Ok(())
}
