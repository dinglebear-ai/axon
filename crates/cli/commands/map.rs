use crate::crates::core::config::Config;
use crate::crates::core::logging::log_done;
use crate::crates::core::ui::{Spinner, muted, primary, print_option, print_phase};
use crate::crates::services::map::discover as map_discover;
use crate::crates::services::types::MapOptions;
use std::error::Error;

/// Return the map result as a raw JSON value.
///
/// Exists for backward-compat with migration tests that assert JSON field
/// shapes. Internally calls the typed service and serializes via serde.
pub async fn map_payload(
    cfg: &Config,
    start_url: &str,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let result = map_discover(
        cfg,
        start_url,
        MapOptions {
            limit: 0,
            offset: 0,
        },
        None,
    )
    .await?;
    Ok(serde_json::to_value(&result)?)
}

pub async fn run_map(cfg: &Config, start_url: &str) -> Result<(), Box<dyn Error>> {
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

    let result = map_discover(
        cfg,
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
    let warning = result.warning.as_deref();

    if let Some(s) = map_spinner {
        s.finish(&format!(
            "map complete (source={map_source} urls={mapped_urls} sitemap_urls={sitemap_urls})"
        ));
    }

    if cfg.json_output {
        println!("{}", serde_json::to_string(&result)?);
    } else {
        println!("{}", primary(&format!("Map Results for {start_url}")));
        println!(
            "{} {} (source: {})",
            muted("Showing"),
            mapped_urls,
            map_source
        );
        if let Some(w) = warning {
            println!("{} {}", muted("Warning:"), w);
        }
        println!();
        for url in &result.urls {
            println!("  • {url}");
        }
    }

    log_done(&format!(
        "command=map mapped_urls={mapped_urls} map_source={map_source} sitemap_urls={sitemap_urls} pages_seen={pages_seen} thin_pages={thin_pages} elapsed_ms={elapsed_ms}"
    ));

    Ok(())
}

#[cfg(test)]
mod map_migration_tests;
#[cfg(test)]
mod map_sitemap_tests;
