//! Shared fixtures for the `map` command test modules.
//!
//! Consumed by `map_sitemap_tests` (sitemap layer) and
//! `map_fallback_tests` (anchor + llms.txt layers).
//!
//! All tests use httpmock for network isolation.

use axon_core::config::{Config, RenderMode};
use axon_services::context::ServiceContext;
use axon_services::map::discover_with_context;
use axon_services::types::MapOptions;
use httpmock::prelude::*;
use std::sync::Arc;

pub(super) async fn map_payload(
    cfg: &Config,
    start_url: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let mut isolated_cfg = cfg.clone();
    isolated_cfg.sqlite_path = temp.path().join("jobs.db");
    isolated_cfg.output_dir = temp.path().join("output");
    let context = ServiceContext::new_with_workers(Arc::new(isolated_cfg))
        .await
        .map_err(|error| -> Box<dyn std::error::Error> { error.to_string().into() })?;
    let result = discover_with_context(
        &context,
        start_url,
        MapOptions {
            limit: 0,
            offset: 0,
        },
        None,
    )
    .await?;
    if result.map_source == "unsupported" {
        return Err(format!(
            "map source pipeline failed: {}",
            result.warning.as_deref().unwrap_or("no warning")
        )
        .into());
    }
    Ok(serde_json::to_value(result)?)
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

pub(super) fn base_config() -> Config {
    Config {
        json_output: true,
        discover_sitemaps: true,
        fetch_retries: 0,
        retry_backoff_ms: 0,
        request_timeout_ms: Some(5_000),
        render_mode: RenderMode::Http,
        ..Config::default()
    }
}

pub(super) fn sitemap_xml(urls: &[&str]) -> String {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
"#,
    );
    for url in urls {
        xml.push_str(&format!("  <url><loc>{url}</loc></url>\n"));
    }
    xml.push_str("</urlset>\n");
    xml
}

pub(super) fn sitemap_index_xml(child_urls: &[&str]) -> String {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
"#,
    );
    for url in child_urls {
        xml.push_str(&format!("  <sitemap><loc>{url}</loc></sitemap>\n"));
    }
    xml.push_str("</sitemapindex>\n");
    xml
}

/// Register all default sitemap seed paths as 404, except the one being tested.
pub(super) fn mock_all_sitemaps_404(server: &MockServer) {
    for path in &[
        "/sitemap.xml",
        "/sitemap_index.xml",
        "/sitemap-index.xml",
        "/sitemap1.xml",
        "/sitemaps.xml",
        "/sitemap/index.xml",
        "/wp-sitemap.xml",
        "/sitemap/sitemap-index.xml",
    ] {
        server.mock(|when, then| {
            when.method(GET).path(*path);
            then.status(404);
        });
    }
    server.mock(|when, then| {
        when.method(GET).path("/robots.txt");
        then.status(404);
    });
}

/// Register the 4 non-`/sitemap.xml` default seed paths as 404. Used by tests that mock
/// `/sitemap.xml` (and `/robots.txt`) themselves but need the remaining index seeds absent.
pub(super) fn mock_index_seeds_404(server: &MockServer) {
    for path in &[
        "/sitemap_index.xml",
        "/sitemap-index.xml",
        "/sitemap1.xml",
        "/sitemaps.xml",
        "/sitemap/index.xml",
        "/wp-sitemap.xml",
        "/sitemap/sitemap-index.xml",
    ] {
        server.mock(|when, then| {
            when.method(GET).path(*path);
            then.status(404);
        });
    }
}

// ---------------------------------------------------------------------------
// Test 1: Sitemap-first behavior — robots.txt → sitemap.xml → 10 URLs
// ---------------------------------------------------------------------------
