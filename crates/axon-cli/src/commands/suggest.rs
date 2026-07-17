use crate::commands::resolve_input_text;
use axon_core::config::Config;
use axon_core::ui::{hyperlink, muted, primary, print_aurora_table};
use axon_services::query as query_service;
use std::error::Error;

/// CLI shim for the suggest command.
pub async fn run_suggest(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let focus = resolve_input_text(cfg);
    let result = query_service::suggest(cfg, focus.as_deref()).await?;
    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "suggestions": result.suggestions.iter().map(|s| serde_json::json!({
                    "url": &s.url,
                    "reason": &s.reason,
                })).collect::<Vec<_>>()
            }))?
        );
    } else if result.suggestions.is_empty() {
        println!("{}", muted("No suggestions found."));
    } else {
        println!("{}", primary("Suggested sources to index"));
        print_aurora_table(
            &["URL", "Reason"],
            result
                .suggestions
                .iter()
                .map(|s| vec![hyperlink(&s.url, &s.url), s.reason.clone()]),
        );
    }
    Ok(())
}
