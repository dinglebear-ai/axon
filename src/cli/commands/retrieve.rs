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
    };
    let result = query_svc::retrieve(cfg, target, opts).await?;

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
                "url": target,
                "chunks": result.chunk_count,
                "content": result.content.trim()
            }))?
        );
    } else {
        println!("{}", primary(&format!("Retrieve Result for {target}")));
        println!("{} {}\n", muted("Chunks:"), result.chunk_count);
        println!("{}", result.content.trim());
    }

    Ok(())
}
