use crate::crates::core::config::Config;
use crate::crates::core::content::build_selector_config;
use crate::crates::core::http::normalize_url;
use crate::crates::crawl::scrape::{build_scrape_website, fetch_single_page, select_output};
use crate::crates::services::types::ScrapeResult;
use std::error::Error;

/// Map a raw JSON payload into a [`ScrapeResult`].
///
/// This is a pure function — no network required. Tests call it with JSON literals.
pub fn map_scrape_payload(payload: serde_json::Value) -> Result<ScrapeResult, Box<dyn Error>> {
    let url = payload
        .get("url")
        .and_then(serde_json::Value::as_str)
        .ok_or("scrape payload missing url")?
        .to_string();
    let markdown = payload
        .get("markdown")
        .and_then(serde_json::Value::as_str)
        .ok_or("scrape payload missing markdown")?
        .to_string();
    Ok(ScrapeResult {
        payload,
        url,
        markdown: markdown.clone(),
        output: markdown,
    })
}

/// Scrape a single URL and return a typed [`ScrapeResult`].
///
/// Delegates to [`scrape_payload`] from the crawl layer; wraps the raw
/// JSON value into the typed service result.
pub async fn scrape(cfg: &Config, url: &str) -> Result<ScrapeResult, Box<dyn Error>> {
    let normalized = normalize_url(url);
    crate::crates::core::http::validate_url(&normalized).map_err(|e| -> Box<dyn Error> {
        format!("invalid scrape url {normalized}: {e}").into()
    })?;
    let mut website = build_scrape_website(cfg, &normalized).map_err(|e| -> Box<dyn Error> {
        format!("failed to build scrape config for {normalized}: {e}").into()
    })?;
    let page = fetch_single_page(cfg, &mut website, &normalized)
        .await
        .map_err(|e| -> Box<dyn Error> { format!("fetch failed for {normalized}: {e}").into() })?;
    let status_code = page.status_code;
    if !(200..300).contains(&status_code) {
        return Err(format!("scrape failed: HTTP {} for {}", status_code, normalized).into());
    }

    let selector_config = build_selector_config(cfg);
    let payload = crate::crates::crawl::scrape::build_scrape_json(
        &normalized,
        &page.html,
        status_code,
        selector_config.as_ref(),
    );
    let output = select_output(
        cfg.format,
        &normalized,
        &page.html,
        status_code,
        selector_config.as_ref(),
    )?;
    let mut result = map_scrape_payload(payload)?;
    result.output = output;
    Ok(result)
}
