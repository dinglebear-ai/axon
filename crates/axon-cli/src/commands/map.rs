use axon_core::config::Config;
use axon_core::logging::log_done;
use axon_core::ui::{Spinner, primary, print_option, print_phase};
use axon_services::context::ServiceContext;
use axon_services::types::MapOptions;
use std::error::Error;

pub async fn run_map(
    cfg: &Config,
    start_url: &str,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    if !cfg.json_output {
        print_phase("◐", "Mapping", start_url);
        println!("  {}", primary("Options:"));
        print_option("maxDepth", &cfg.max_depth.to_string());
        print_option("discoverSitemaps", &cfg.discover_sitemaps.to_string());
        println!();
    }

    let map_spinner = if cfg.json_output {
        None
    } else {
        Some(Spinner::new("mapping in progress"))
    };

    let result = axon_services::map::discover_with_context(
        service_context,
        start_url,
        MapOptions {
            limit: 0,
            offset: 0,
        },
        None,
    )
    .await?;

    let pages_seen = result.pages_seen;
    let sitemap_urls = result.sitemap_urls;
    let mapped_urls = result.returned_url_count;
    let thin_pages = result.thin_pages;
    let elapsed_ms = result.elapsed_ms;
    let map_source = result.map_source.as_str();

    if let Some(s) = map_spinner {
        s.finish(&format!(
            "map complete (source={map_source} urls={mapped_urls} sitemap_urls={sitemap_urls})"
        ));
    }

    log_done(&format!(
        "command=map mapped_urls={mapped_urls} map_source={map_source} sitemap_urls={sitemap_urls} pages_seen={pages_seen} thin_pages={thin_pages} elapsed_ms={elapsed_ms}"
    ));

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("{}", primary("Mapped URLs"));
        for url in &result.urls {
            println!("  {url}");
        }
        println!("  {mapped_urls} total");
    }
    Ok(())
}

#[cfg(test)]
mod map_migration_tests;
#[cfg(test)]
mod map_sitemap_tests;
