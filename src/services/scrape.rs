use crate::core::config::Config;
use crate::core::content::build_selector_config;
use crate::core::http::normalize_url;
use crate::crawl::scrape::{build_scrape_website, fetch_single_page, select_output};
use crate::services::events::ServiceEvent;
use crate::services::types::{ArtifactHandle, DocumentBackend, ScrapeResult};
use std::error::Error;
use tokio::sync::mpsc;

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
    let output = markdown.clone();
    Ok(ScrapeResult {
        payload,
        url,
        markdown,
        output,
        artifact_handle: None,
        truncated: false,
        token_estimate: None,
        next_cursor: None,
        remaining_tokens_estimate: None,
        backend: Some(DocumentBackend::LiveScrape),
    })
}

/// Scrape a single URL and return a typed [`ScrapeResult`].
///
/// Delegates to [`scrape_payload`] from the crawl layer; wraps the raw
/// JSON value into the typed service result.
///
/// `tx` is an optional progress channel. Pass `None` when progress events are
/// not needed (CLI) or `Some(sender)` when the caller wants to observe
/// intermediate log events (web / MCP streaming paths). The `tx` parameter
/// is accepted for API consistency with other multi-step service functions
/// but is currently unused — scrape is a single network round-trip with no
/// intermediate steps to report.
#[must_use = "scrape returns a Result that should be handled"]
pub async fn scrape(
    cfg: &Config,
    url: &str,
    _tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<ScrapeResult, Box<dyn Error>> {
    let normalized = normalize_url(url);
    crate::core::http::validate_url(&normalized).map_err(|e| -> Box<dyn Error> {
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
    let payload = crate::crawl::scrape::build_scrape_json(
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
    result.artifact_handle = cfg.output_path.as_ref().and_then(|path| {
        ArtifactHandle::try_from_path(
            "scrape",
            &cfg.output_dir,
            path,
            result.output.len() as u64,
            Some(result.output.lines().count() as u64),
            None,
            Some(normalized.to_string()),
        )
    });
    Ok(result)
}

#[cfg(test)]
#[path = "scrape_tests.rs"]
mod tests;
