use crate::core::config::Config;
use crate::core::logging::log_info;
use crate::core::ui::{muted, primary};
use crate::services::query as query_svc;
use crate::services::types::RetrieveOptions;
use std::error::Error;

pub async fn run_retrieve(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let target = cfg.positional.first().ok_or("retrieve requires URL")?;
    // TODO: cfg.quiet — suppress progress log when quiet mode lands
    if !cfg.json_output {
        log_info(&format!("command=retrieve url={target}"));
    }

    let opts = RetrieveOptions {
        max_points: cfg.retrieve_max_points,
        cursor: None,
        token_budget: None,
    };
    let result = query_svc::retrieve(cfg, target, opts)
        .await
        .map_err(|e| -> Box<dyn Error> { e.to_string().into() })?;

    if result.chunk_count == 0 {
        return Err(format!(
            "no content found for URL: {target} — run 'axon sources' to list indexed URLs"
        )
        .into());
    }

    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "requested_url": result.requested_url,
                "matched_url": result.matched_url,
                "chunks": result.chunk_count,
                "backend": result.backend,
                "truncated": result.truncated,
                "token_estimate": result.token_estimate,
                "next_cursor": result.next_cursor,
                "remaining_tokens_estimate": result.remaining_tokens_estimate,
                "warnings": result.warnings,
                "variant_errors": result.variant_errors,
                "refresh_status": result.refresh_status,
                "content": result.content.trim()
            }))?
        );
    } else {
        println!("{}", primary(&format!("Retrieve Result for {target}")));
        println!("{} {}\n", muted("Chunks:"), result.chunk_count);
        if let Some(backend) = result.backend {
            println!("{} {}", muted("Backend:"), backend);
        }
        if let Some(refresh_status) = result.refresh_status.as_deref() {
            println!("{} {}", muted("Refresh:"), refresh_status);
        }
        if let Some(next_cursor) = result.next_cursor.as_deref() {
            println!("{} {}", muted("Next cursor:"), next_cursor);
        }
        if !result.warnings.is_empty() {
            println!("{} {}", muted("Warnings:"), result.warnings.join(" | "));
        }
        if result.backend.is_some()
            || result.refresh_status.is_some()
            || result.next_cursor.is_some()
            || !result.warnings.is_empty()
        {
            println!();
        }
        println!("{}", result.content.trim());
    }

    Ok(())
}
