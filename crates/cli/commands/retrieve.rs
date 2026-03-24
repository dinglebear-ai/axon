use crate::crates::core::config::Config;
use crate::crates::core::logging::log_info;
use crate::crates::core::ui::{muted, primary};
use crate::crates::services::query as query_svc;
use crate::crates::services::types::RetrieveOptions;
use std::error::Error;

pub async fn run_retrieve(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let target = cfg.positional.first().ok_or("retrieve requires URL")?;
    // TODO: cfg.quiet — suppress progress log when quiet mode lands
    if !cfg.json_output {
        log_info(&format!("command=retrieve url={target}"));
    }

    let opts = RetrieveOptions { max_points: None };
    let result = query_svc::retrieve(cfg, target, opts).await?;

    let first_chunk = result.chunks.first();
    let chunk_count = first_chunk
        .and_then(|c| c["chunk_count"].as_u64())
        .unwrap_or(0) as usize;
    let content = first_chunk
        .and_then(|c| c["content"].as_str())
        .unwrap_or("");

    if chunk_count == 0 {
        // Note: returns Err when no content found (changed from Ok in v0.33.x for consistency
        // with query/search behavior — callers should treat "not found" as an error, not silence).
        return Err(format!(
            "no content found for URL: {target} — run 'axon sources' to list indexed URLs"
        )
        .into());
    }

    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "url": target,
                "chunks": chunk_count,
                "content": content.trim()
            }))?
        );
    } else {
        println!("{}", primary(&format!("Retrieve Result for {target}")));
        println!("{} {}\n", muted("Chunks:"), chunk_count);
        println!("{}", content.trim());
    }

    Ok(())
}
