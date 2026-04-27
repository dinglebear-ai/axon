use crate::crates::core::config::Config;
use crate::crates::core::logging::log_done;
use crate::crates::core::ui::{Spinner, muted, primary, print_option, print_phase};
use crate::crates::services::map::discover as map_discover;
use crate::crates::services::types::MapOptions;
use std::error::Error;

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
    Ok(result.payload)
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

    let pages_seen = result.payload["pages_seen"].as_u64().unwrap_or(0);
    let sitemap_urls = result.payload["sitemap_urls"].as_u64().unwrap_or(0);
    let mapped_urls = result.payload["mapped_urls"].as_u64().unwrap_or(0);
    let thin_pages = result.payload["thin_pages"].as_u64().unwrap_or(0);
    let elapsed_ms = result.payload["elapsed_ms"].as_u64().unwrap_or(0);
    let map_source = result.payload["map_source"].as_str().unwrap_or("unknown");
    let warning = result.payload["warning"].as_str();

    if let Some(s) = map_spinner {
        s.finish(&format!(
            "map complete (source={map_source} urls={mapped_urls} sitemap_urls={sitemap_urls})"
        ));
    }

    if cfg.json_output {
        println!("{}", result.payload);
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
        if let Some(urls) = result.payload["urls"].as_array() {
            for url in urls {
                if let Some(u) = url.as_str() {
                    println!("  • {u}");
                }
            }
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
