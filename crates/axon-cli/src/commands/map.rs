use axon_api::source::{ResponseMode, SourceIntent, SourceRequest, SourceScope};
use axon_core::config::Config;
use axon_core::logging::log_done;
use axon_core::ui::{Spinner, primary, print_option, print_phase};
use axon_services::context::ServiceContext;
use axon_services::map::discover as map_discover;
use axon_services::types::MapOptions;
use std::error::Error;

use crate::commands::source::run_source_request;

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

pub(crate) fn build_map_source_request(
    cfg: &Config,
    start_url: &str,
    urls: &[String],
) -> SourceRequest {
    let mut request = SourceRequest::new(start_url.to_string());
    request.intent = SourceIntent::Map;
    request.scope = Some(SourceScope::Map);
    request.embed = false;
    request.collection = Some(cfg.collection.clone());
    request.output.json = cfg.json_output;
    request.output.response_mode = ResponseMode::Auto;
    request
        .options
        .values
        .insert("map_urls".to_string(), serde_json::json!(urls));
    request.limits.max_pages = Some(urls.len() as u64);
    request
}

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

    if let Some(s) = map_spinner {
        s.finish(&format!(
            "map complete (source={map_source} urls={mapped_urls} sitemap_urls={sitemap_urls})"
        ));
    }

    log_done(&format!(
        "command=map mapped_urls={mapped_urls} map_source={map_source} sitemap_urls={sitemap_urls} pages_seen={pages_seen} thin_pages={thin_pages} elapsed_ms={elapsed_ms}"
    ));

    let request = build_map_source_request(cfg, start_url, &result.urls);
    run_source_request(cfg, service_context, request).await
}

#[cfg(test)]
mod map_migration_tests;
#[cfg(test)]
mod map_sitemap_tests;
