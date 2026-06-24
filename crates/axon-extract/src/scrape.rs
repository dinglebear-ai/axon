//! Single-URL scrape: vertical-extractor dispatch with a generic HTTP fallback.
//!
//! Takes only `cfg` (+ optional progress channel) — no `ServiceContext`, no
//! jobs — so it lives here in `axon-extract` alongside the vertical framework
//! and is called by both `services::scrape` (re-exported) and the jobs watch
//! change-detector. Batch + embed orchestration stays in `services::scrape`.

use axon_api::contract::ArtifactHandle;
use axon_api::job_dto::ScrapeResult;
use axon_api::result::DocumentBackend;
use axon_core::config::Config;
use axon_core::content::build_selector_config;
use axon_core::events::{LogLevel, ServiceEvent, emit};
use axon_core::http::normalize_url;
use axon_crawl::scrape::{build_scrape_website, fetch_single_page, select_output};
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::{VerticalContext, dispatch_by_url};

pub const MAX_PUBLIC_STRUCTURED_BYTES: usize = 16 * 1024;

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
        structured: None,
        structured_for_embedding: None,
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
/// start/complete log events (web / MCP streaming paths).
#[must_use = "scrape returns a Result that should be handled"]
pub async fn scrape(
    cfg: &Config,
    url: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<ScrapeResult, Box<dyn Error>> {
    scrape_with_vertical_timeout(cfg, url, tx, Duration::from_secs(120)).await
}

pub async fn scrape_with_vertical_timeout(
    cfg: &Config,
    url: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
    vertical_timeout: Duration,
) -> Result<ScrapeResult, Box<dyn Error>> {
    let normalized = validate_and_normalize_scrape_url(url, &tx).await?;
    let mut result =
        if let Some(r) = try_vertical_scrape(cfg, &normalized, &tx, vertical_timeout).await? {
            r
        } else {
            let r = generic_scrape(cfg, &normalized).await?;
            emit_scrape_complete(&tx, &normalized).await;
            r
        };
    // Service-side artifact write: if output_path is configured, write atomically
    // so all callers (CLI, MCP, /v1/actions) share identical write semantics.
    if let Some(output_path) = cfg.output_path.as_ref() {
        axon_core::artifacts::atomic_write_explicit(output_path, result.output.as_bytes())
            .await
            .map_err(|err| -> Box<dyn Error> { err.to_string().into() })?;
        result.artifact_handle = ArtifactHandle::try_from_path(
            "scrape",
            &cfg.output_dir,
            output_path,
            result.output.len() as u64,
            Some(result.output.lines().count() as u64),
            None,
            Some(normalized.to_string()),
        );
    }
    Ok(result)
}

pub async fn validate_and_normalize_scrape_url(
    url: &str,
    tx: &Option<mpsc::Sender<ServiceEvent>>,
) -> Result<String, Box<dyn Error>> {
    let normalized = normalize_url(url);
    emit(
        tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("scrape starting: {normalized}"),
        },
    )
    .await;
    tokio::time::timeout(
        Duration::from_millis(2000),
        axon_core::http::validate_url_with_dns(&normalized),
    )
    .await
    .map_err(|_| -> Box<dyn Error> {
        format!("invalid scrape url {normalized}: DNS validation timed out").into()
    })?
    .map_err(|e| -> Box<dyn Error> { format!("invalid scrape url {normalized}: {e}").into() })?;
    Ok(normalized.into_owned())
}

async fn try_vertical_scrape(
    cfg: &Config,
    normalized: &str,
    tx: &Option<mpsc::Sender<ServiceEvent>>,
    vertical_timeout: Duration,
) -> Result<Option<ScrapeResult>, Box<dyn Error>> {
    if !cfg.enable_verticals {
        return Ok(None);
    }
    let ctx = VerticalContext::new(Arc::new(cfg.clone()));
    match tokio::time::timeout(vertical_timeout, dispatch_by_url(normalized, &ctx)).await {
        Ok(Some(result)) => {
            let doc = result.map_err(|e| -> Box<dyn Error> { e.to_string().into() })?;
            let scrape_result = vertical_doc_to_scrape_result(doc)?;
            tracing::debug!(
                url = %normalized,
                extractor = scrape_result.extractor_name.as_deref().unwrap_or("unknown"),
                has_extra = scrape_result.extra.is_some(),
                "vertical.dispatched: extractor handled scrape"
            );
            emit_scrape_complete(tx, normalized).await;
            Ok(Some(scrape_result))
        }
        Ok(None) => Ok(None),
        Err(_) => Err(format!(
            "vertical extractor timed out after {}s for {normalized}",
            vertical_timeout.as_secs()
        )
        .into()),
    }
}

/// Extract markdown hyperlinks from `[text](href)` patterns and return them as
/// `{href, text}` objects matching the format produced by `build_scrape_json`.
/// Capped at 512 entries (same limit as the generic HTML scrape path).
pub fn extract_markdown_links(markdown: &str) -> Vec<serde_json::Value> {
    // Matches [text](href) where href is an http/https URL.
    // Uses a simple byte scan rather than a regex dep: find `](` then scan for `)`.
    const LIMIT: usize = 512;
    let mut links = Vec::new();
    let bytes = markdown.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i + 3 < len && links.len() < LIMIT {
        // Find `](`
        if bytes[i] == b']' && bytes[i + 1] == b'(' {
            let href_start = i + 2;
            // Find the closing `)`
            if let Some(rel) = bytes[href_start..].iter().position(|&b| b == b')') {
                let href = &markdown[href_start..href_start + rel];
                if href.starts_with("http://") || href.starts_with("https://") {
                    // Extract the link text between the preceding `[` and `]`
                    let text_end = i;
                    let text_start = markdown[..text_end]
                        .rfind('[')
                        .map(|p| p + 1)
                        .unwrap_or(text_end);
                    let text = &markdown[text_start..text_end];
                    links.push(serde_json::json!({ "href": href, "text": text }));
                }
                i = href_start + rel + 1;
                continue;
            }
        }
        i += 1;
    }
    links
}

pub fn vertical_doc_to_scrape_result(
    doc: crate::ScrapedDoc,
) -> Result<ScrapeResult, Box<dyn Error>> {
    let links = extract_markdown_links(&doc.markdown);
    let payload = serde_json::json!({ "url": doc.url, "markdown": doc.markdown, "links": links });
    let mut scrape_result = map_scrape_payload(payload)?;
    scrape_result.backend = Some(DocumentBackend::LiveScrape);
    scrape_result.follow_crawl_urls = doc.follow_crawl_urls;
    let mut extra = doc.extra.unwrap_or_else(|| serde_json::json!({}));
    if let serde_json::Value::Object(map) = &mut extra {
        map.insert(
            "extractor_version".to_string(),
            doc.extractor_version.into(),
        );
    }
    scrape_result.extra = Some(extra);
    if let Some(structured) = doc.structured {
        let redacted = redact_sensitive_structured_keys(structured);
        scrape_result.structured_for_embedding = Some(redacted.clone());
        scrape_result.structured = capped_public_structured_summary(redacted);
    }
    scrape_result.extractor_name = Some(doc.extractor_name.to_string());
    scrape_result.title = doc.title;
    Ok(scrape_result)
}

fn capped_public_structured_summary(value: serde_json::Value) -> Option<serde_json::Value> {
    let bytes = serde_json::to_vec(&value).ok()?;
    if bytes.len() > MAX_PUBLIC_STRUCTURED_BYTES {
        None
    } else {
        Some(value)
    }
}

fn redact_sensitive_structured_keys(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.into_iter()
                .filter_map(|(key, value)| {
                    let lowered = key.to_ascii_lowercase();
                    let sensitive = [
                        "token",
                        "secret",
                        "password",
                        "authorization",
                        "cookie",
                        "api_key",
                    ]
                    .iter()
                    .any(|needle| lowered.contains(needle));
                    (!sensitive).then(|| (key, redact_sensitive_structured_keys(value)))
                })
                .collect(),
        ),
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items
                .into_iter()
                .map(redact_sensitive_structured_keys)
                .collect(),
        ),
        other => other,
    }
}

async fn generic_scrape(cfg: &Config, normalized: &str) -> Result<ScrapeResult, Box<dyn Error>> {
    let mut website = build_scrape_website(cfg, normalized).map_err(|e| -> Box<dyn Error> {
        format!("failed to build scrape config for {normalized}: {e}").into()
    })?;
    let page = fetch_single_page(cfg, &mut website, normalized)
        .await
        .map_err(|e| -> Box<dyn Error> { format!("fetch failed for {normalized}: {e}").into() })?;
    let status_code = page.status_code;
    if !(200..300).contains(&status_code) {
        return Err(format!("scrape failed: HTTP {} for {}", status_code, normalized).into());
    }

    let selector_config = build_selector_config(cfg);
    let payload = axon_crawl::scrape::build_scrape_json(
        normalized,
        &page.html,
        status_code,
        selector_config.as_ref(),
    );
    let output = select_output(
        cfg.format,
        normalized,
        &page.html,
        status_code,
        selector_config.as_ref(),
    )?;
    let mut result = map_scrape_payload(payload)?;
    result.output = output;
    Ok(result)
}

async fn emit_scrape_complete(tx: &Option<mpsc::Sender<ServiceEvent>>, normalized: &str) {
    emit(
        tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("scrape complete: {normalized}"),
        },
    )
    .await;
}
