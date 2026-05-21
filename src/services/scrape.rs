use crate::core::config::Config;
use crate::core::content::build_selector_config;
use crate::core::http::normalize_url;
use crate::crawl::scrape::{build_scrape_website, fetch_single_page, select_output};
use crate::extract::{VerticalContext, dispatch_by_url};
use crate::services::events::ServiceEvent;
use crate::services::types::{ArtifactHandle, DocumentBackend, ScrapeResult};
use futures_util::stream::{self, StreamExt};
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

pub const MAX_SCRAPE_BATCH_URLS: usize = 50;

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
        follow_crawl_urls: vec![],
        extra: None,
        extractor_name: None,
        title: None,
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
    tokio::time::timeout(
        Duration::from_millis(2000),
        crate::core::http::validate_url_with_dns(&normalized),
    )
    .await
    .map_err(|_| -> Box<dyn Error> {
        format!("invalid scrape url {normalized}: DNS validation timed out").into()
    })?
    .map_err(|e| -> Box<dyn Error> { format!("invalid scrape url {normalized}: {e}").into() })?;

    // Vertical-extractor fast path: if a registered extractor claims this URL,
    // use it instead of the generic HTTP scrape. Falls through on None (no match)
    // or on Err (extractor failure — caller decides whether to propagate or
    // continue with generic scrape). Today we propagate errors so agents get the
    // machine-readable error code (e.g. VerticalBlockedAntibot) rather than
    // silently falling back to HTML that looks like a CAPTCHA page.
    if cfg.enable_verticals {
        const VERTICAL_TIMEOUT: Duration = Duration::from_secs(120);
        let ctx = VerticalContext::new(Arc::new(cfg.clone()));
        let vertical_result =
            tokio::time::timeout(VERTICAL_TIMEOUT, dispatch_by_url(&normalized, &ctx)).await;
        match vertical_result {
            Ok(Some(result)) => {
                let doc = result.map_err(|e| -> Box<dyn Error> { e.to_string().into() })?;
                let payload = serde_json::json!({ "url": doc.url, "markdown": doc.markdown });
                let mut scrape_result = map_scrape_payload(payload)?;
                scrape_result.backend = Some(DocumentBackend::LiveScrape);
                scrape_result.follow_crawl_urls = doc.follow_crawl_urls;
                scrape_result.extra = doc.extra;
                scrape_result.extractor_name = Some(doc.extractor_name.to_string());
                scrape_result.title = doc.title;
                tracing::debug!(
                    url = %normalized,
                    extractor = doc.extractor_name,
                    has_extra = scrape_result.extra.is_some(),
                    "vertical.dispatched: extractor handled scrape"
                );
                // v1: LLM format is only applied on the generic HTTP scrape path.
                // Vertical extractors return structured markdown that should not be post-processed.
                return Ok(scrape_result);
            }
            Ok(None) => {} // no extractor claimed the URL — fall through to generic scrape
            Err(_) => {
                tracing::warn!(
                    url = %normalized,
                    timeout_secs = VERTICAL_TIMEOUT.as_secs(),
                    "vertical extractor timed out — falling through to generic scrape"
                );
            }
        }
    }

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

/// Scrape a bounded batch of URLs. The cap lives in the service layer so CLI,
/// MCP, and REST callers share the same protection.
#[must_use = "scrape_batch returns a Result that should be handled"]
pub async fn scrape_batch(
    cfg: &Config,
    urls: &[String],
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<Vec<ScrapeResult>, Box<dyn Error>> {
    if urls.is_empty() {
        return Err("at least one url is required".into());
    }
    if urls.len() > MAX_SCRAPE_BATCH_URLS {
        return Err(
            format!("scrape accepts at most {MAX_SCRAPE_BATCH_URLS} urls per request").into(),
        );
    }

    let normalized: Vec<(usize, String)> = urls
        .iter()
        .enumerate()
        .map(|(idx, url)| (idx, normalize_url(url).into_owned()))
        .collect();
    let validated = stream::iter(normalized)
        .map(|(idx, url)| async move {
            tokio::time::timeout(
                Duration::from_millis(2000),
                crate::core::http::validate_url_with_dns(&url),
            )
            .await
            .map_err(|_| format!("invalid scrape url {url}: DNS validation timed out"))?
            .map_err(|e| format!("invalid scrape url {url}: {e}"))?;
            Ok::<(usize, String), String>((idx, url))
        })
        .buffer_unordered(10)
        .collect::<Vec<_>>()
        .await;

    let mut ready = Vec::with_capacity(validated.len());
    for item in validated {
        ready.push(item.map_err(|message| -> Box<dyn Error> { message.into() })?);
    }
    ready.sort_by_key(|(idx, _)| *idx);

    let scraped = stream::iter(ready)
        .map(|(idx, url)| {
            let tx = tx.clone();
            async move {
                scrape(cfg, &url, tx)
                    .await
                    .map(|result| (idx, result))
                    .map_err(|err| err.to_string())
            }
        })
        .buffer_unordered(10)
        .collect::<Vec<_>>()
        .await;

    let mut indexed_results = Vec::with_capacity(scraped.len());
    for item in scraped {
        indexed_results.push(item.map_err(|message| -> Box<dyn Error> { message.into() })?);
    }
    indexed_results.sort_by_key(|(idx, _)| *idx);
    let results = indexed_results
        .into_iter()
        .map(|(_, result)| result)
        .collect();
    Ok(results)
}

#[cfg(test)]
#[path = "scrape_tests.rs"]
mod tests;
