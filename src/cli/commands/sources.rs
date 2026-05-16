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
    let result = if cfg.sources_by_schema_version {
        system::sources_with_breakdown(cfg, pagination).await?
    } else {
        system::sources(cfg, pagination).await?
    };
    let url_count = result.urls.len();
    if cfg.json_output {
        let mut json = serde_json::json!({
            "count": result.count,
            "limit": result.limit,
            "offset": result.offset,
            "urls": result.urls,
        });
        if let Some(ref breakdown) = result.schema_version_breakdown {
            // BTreeMap<u32, usize> -> serde_json::Map keyed by stringified version.
            let mut bd = serde_json::Map::new();
            for (k, v) in breakdown {
                bd.insert(k.to_string(), serde_json::Value::from(*v));
            }
            json["schema_version_breakdown"] = serde_json::Value::Object(bd);
        }
        println!("{}", serde_json::to_string_pretty(&json)?);
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
        if let Some(ref breakdown) = result.schema_version_breakdown {
            println!();
            println!("{}", primary("Payload schema version breakdown"));
            for (version, count) in breakdown {
                println!(
                    "  {} {}",
                    accent(&format!("v{version}")),
                    muted(&format!("(chunks: {count})"))
                );
            }
        }
    }
    Ok(())
}
