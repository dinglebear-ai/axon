use axon_core::config::Config;
use axon_core::env::env_usize_clamped;
use axon_core::logging::log_info;
use axon_core::ui::{hyperlink, muted, primary, print_aurora_table};
use axon_services::system;
use axon_services::types::Pagination;
use std::error::Error;

pub async fn run_sources(cfg: &Config) -> Result<(), Box<dyn Error>> {
    log_info("command=sources");
    if let Some(domain) = cfg.sources_domain.as_deref() {
        return run_domain_sources(cfg, domain).await;
    }

    let facet_limit = env_usize_clamped("AXON_SOURCES_FACET_LIMIT", 100_000, 1, 1_000_000);
    let pagination = Pagination {
        limit: facet_limit,
        offset: 0,
    };
    let result = system::sources(cfg, pagination).await?;
    let url_count = result.urls.len();
    if cfg.json_output {
        let json = serde_json::json!({
            "count": result.count,
            "limit": result.limit,
            "offset": result.offset,
            "urls": result.urls,
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        println!("{}", primary("Sources"));
        print_aurora_table(
            &["URL", "Chunks"],
            result
                .urls
                .iter()
                .map(|(url, chunks)| vec![hyperlink(url, url), chunks.to_string()]),
        );
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

async fn run_domain_sources(cfg: &Config, domain: &str) -> Result<(), Box<dyn Error>> {
    let limit = if cfg.sources_domain_all {
        env_usize_clamped("AXON_SOURCES_DOMAIN_LIMIT", 10_000, 1, 10_000)
    } else {
        cfg.search_limit.clamp(1, 10_000)
    };
    let result =
        system::sources_for_domain(cfg, domain, Pagination { limit, offset: 0 }, None).await?;

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    println!("{}", primary(&format!("Sources for {}", result.domain)));
    print_aurora_table(
        &["URL"],
        result.urls.iter().map(|url| vec![hyperlink(url, url)]),
    );
    if result.truncated {
        println!(
            "{}",
            muted(&format!(
                "Showing {} of at least {} matching sources. Use --limit {} or --all to fetch more.",
                result.urls.len(),
                result.urls.len() + 1,
                result.limit.saturating_mul(2)
            ))
        );
    }
    Ok(())
}
