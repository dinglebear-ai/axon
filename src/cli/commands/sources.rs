use crate::core::config::Config;
use crate::core::logging::log_info;
use crate::core::ui::{accent, muted, primary};
use crate::services::system;
use crate::services::types::Pagination;
use crate::vector::ops::qdrant::env_usize_clamped;
use std::error::Error;

pub async fn run_sources(cfg: &Config) -> Result<(), Box<dyn Error>> {
    log_info("command=sources");
    let facet_limit = env_usize_clamped("AXON_SOURCES_FACET_LIMIT", 100_000, 1, 1_000_000);
    let pagination = Pagination {
        limit: facet_limit,
        offset: 0,
    };
    let result = system::sources(cfg, pagination).await?;
    let url_count = result.urls.len();
    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "count": result.count,
                "limit": result.limit,
                "offset": result.offset,
                "urls": result.urls,
            }))?
        );
    } else {
        println!("{}", primary("Sources"));
        for (url, chunks) in &result.urls {
            println!(
                "  {} {}",
                accent(url),
                muted(&format!("(chunks: {chunks})"))
            );
        }
        if url_count == facet_limit {
            println!(
                "{}",
                muted(&format!(
                    "Showing top {facet_limit} sources. Set AXON_SOURCES_FACET_LIMIT to see more."
                ))
            );
        }
    }
    Ok(())
}
