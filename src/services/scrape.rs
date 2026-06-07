use crate::core::config::Config;
use crate::core::content::build_selector_config;
use crate::core::http::normalize_url;
use crate::crawl::scrape::{build_scrape_website, fetch_single_page, select_output};
use crate::extract::{VerticalContext, dispatch_by_url};
use crate::services::events::{LogLevel, ServiceEvent, emit};
use crate::services::types::{ArtifactHandle, DocumentBackend, ScrapeResult};
use crate::vector::ops::input::{chunk_markdown, chunk_text};
use crate::vector::ops::tei::{PreparedDoc, embed_prepared_docs};
use futures_util::stream::{self, StreamExt};
use spider::url::Url as SpiderUrl;
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

pub const MAX_SCRAPE_BATCH_URLS: usize = 50;

#[derive(Debug)]
enum ScrapeBatchError {
    Validation(String),
    Scrape(String),
}

impl fmt::Display for ScrapeBatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(message) | Self::Scrape(message) => f.write_str(message),
        }
    }
}

impl Error for ScrapeBatchError {}

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
/// start/complete log events (web / MCP streaming paths).
#[must_use = "scrape returns a Result that should be handled"]
pub async fn scrape(
    cfg: &Config,
    url: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<ScrapeResult, Box<dyn Error>> {
    scrape_with_vertical_timeout(cfg, url, tx, Duration::from_secs(120)).await
}

async fn scrape_with_vertical_timeout(
    cfg: &Config,
    url: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
    vertical_timeout: Duration,
) -> Result<ScrapeResult, Box<dyn Error>> {
    let normalized = validate_and_normalize_scrape_url(url, &tx).await?;
    if let Some(result) = try_vertical_scrape(cfg, &normalized, &tx, vertical_timeout).await? {
        return Ok(result);
    }
    let result = generic_scrape(cfg, &normalized).await?;
    emit_scrape_complete(&tx, &normalized).await;
    Ok(result)
}

async fn validate_and_normalize_scrape_url(
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
        crate::core::http::validate_url_with_dns(&normalized),
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

fn vertical_doc_to_scrape_result(
    doc: crate::extract::ScrapedDoc,
) -> Result<ScrapeResult, Box<dyn Error>> {
    // TODO(watch): vertical payloads omit `links`, so watch link-change
    // detection is markdown-only for vertical URLs.
    let payload = serde_json::json!({ "url": doc.url, "markdown": doc.markdown });
    let mut scrape_result = map_scrape_payload(payload)?;
    scrape_result.backend = Some(DocumentBackend::LiveScrape);
    scrape_result.follow_crawl_urls = doc.follow_crawl_urls;
    scrape_result.extra = doc.extra;
    scrape_result.extractor_name = Some(doc.extractor_name.to_string());
    scrape_result.title = doc.title;
    Ok(scrape_result)
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
    let payload = crate::crawl::scrape::build_scrape_json(
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
    let deadline = Duration::from_secs(cfg.scrape_batch_timeout_secs.max(1));
    run_with_scrape_batch_timeout(deadline, scrape_batch_inner(cfg, urls, tx)).await
}

async fn run_with_scrape_batch_timeout<F, T>(
    deadline: Duration,
    future: F,
) -> Result<T, Box<dyn Error>>
where
    F: Future<Output = Result<T, ScrapeBatchError>>,
{
    tokio::time::timeout(deadline, future)
        .await
        .map_err(|_| -> Box<dyn Error> {
            format!("scrape batch timed out after {}s", deadline.as_secs()).into()
        })?
        .map_err(|err| -> Box<dyn Error> { err.to_string().into() })
}

async fn scrape_batch_inner(
    cfg: &Config,
    urls: &[String],
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<Vec<ScrapeResult>, ScrapeBatchError> {
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
        ready.push(item.map_err(ScrapeBatchError::Validation)?);
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
        indexed_results.push(item.map_err(ScrapeBatchError::Scrape)?);
    }
    indexed_results.sort_by_key(|(idx, _)| *idx);
    let results = indexed_results
        .into_iter()
        .map(|(_, result)| result)
        .collect();
    Ok(results)
}

/// Scrape a batch and embed it when `cfg.embed` is true.
///
/// This is the shared service entry point for the `/v1` REST scrape endpoint.
/// It embeds the in-memory scrape result instead of round-tripping through the
/// output directory, so vertical metadata is preserved in Qdrant payloads.
#[must_use = "scrape_batch_with_optional_embed returns a Result that should be handled"]
pub async fn scrape_batch_with_optional_embed(
    cfg: &Config,
    urls: &[String],
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<Vec<ScrapeResult>, Box<dyn Error>> {
    let results = scrape_batch(cfg, urls, tx).await?;
    if cfg.embed {
        let docs = results.iter().map(scrape_result_to_prepared_doc).collect();
        embed_prepared_docs(cfg, docs, None).await?;
    }
    Ok(results)
}

pub(crate) fn scrape_result_to_prepared_doc(result: &ScrapeResult) -> PreparedDoc {
    let domain = SpiderUrl::parse(&result.url)
        .ok()
        .and_then(|url| url.host_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".to_string());
    PreparedDoc {
        url: result.url.clone(),
        domain,
        chunks: if result
            .markdown
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t')
        {
            chunk_text(&result.markdown)
        } else {
            chunk_markdown(&result.markdown)
        },
        source_type: "scrape".to_string(),
        content_type: "markdown",
        title: result.title.clone(),
        extra: result.extra.clone(),
        extractor_name: result.extractor_name.clone(),
        structured: None,
    }
}

#[cfg(test)]
#[path = "scrape_tests.rs"]
mod tests;
