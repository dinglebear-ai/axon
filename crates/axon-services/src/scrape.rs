use crate::events::ServiceEvent;
use crate::types::ScrapeResult;
use axon_core::config::Config;
use axon_core::http::normalize_url;
use axon_vector::ops::{
    SourceDocument, embed_prepared_docs, prepare_source_document,
    structured_payload_from_vertical_summary,
};
use futures_util::stream::{self, StreamExt};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::time::Duration;
use tokio::sync::mpsc;

/// Single-URL scrape + its helpers now live in `axon_extract::scrape`; re-exported
/// so existing `crate::scrape::*` call sites (and sidecar tests) resolve
/// unchanged. Batch + embed orchestration stays in this module.
pub use axon_extract::scrape::{
    MAX_PUBLIC_STRUCTURED_BYTES, extract_markdown_links, map_scrape_payload, scrape,
    scrape_with_vertical_timeout, validate_and_normalize_scrape_url, vertical_doc_to_scrape_result,
};

pub const MAX_SCRAPE_BATCH_URLS: usize = 50;

#[derive(Debug, Clone)]
pub struct FollowCrawlQueueResult {
    pub url: String,
    pub job_id: Option<String>,
    pub error: Option<String>,
}

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
                axon_core::http::validate_url_with_dns(&url),
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
        embed_scrape_results(cfg, &results, "scrape batch embed").await?;
    }
    Ok(results)
}

pub async fn embed_scrape_results(
    cfg: &Config,
    results: &[ScrapeResult],
    label: &'static str,
) -> Result<(), Box<dyn Error>> {
    let mut docs = Vec::with_capacity(results.len());
    for result in results {
        docs.push(scrape_result_to_prepared_doc(cfg, result).await?);
    }
    embed_prepared_docs(cfg, docs, None)
        .await?
        .require_success(label)
        .map_err(|err| anyhow::anyhow!(err))?;
    Ok(())
}

pub async fn enqueue_follow_crawl_jobs(
    cfg: &Config,
    source_url: &str,
    follow_crawl_urls: &[String],
    limit: usize,
) -> Vec<FollowCrawlQueueResult> {
    let mut unique: Vec<&String> = follow_crawl_urls
        .iter()
        .filter(|url| url.as_str() != source_url)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .take(limit)
        .collect();
    unique.sort();

    let mut results = Vec::with_capacity(unique.len());
    for follow_url in unique {
        match axon_jobs::crawl::start_crawl_job(cfg, follow_url).await {
            Ok(job_id) => results.push(FollowCrawlQueueResult {
                url: follow_url.clone(),
                job_id: Some(job_id.to_string()),
                error: None,
            }),
            Err(error) => results.push(FollowCrawlQueueResult {
                url: follow_url.clone(),
                job_id: None,
                error: Some(error.to_string()),
            }),
        }
    }
    results
}

pub async fn scrape_result_to_prepared_doc(
    cfg: &Config,
    result: &ScrapeResult,
) -> anyhow::Result<axon_vector::ops::PreparedDoc> {
    let structured_source = result
        .structured_for_embedding
        .clone()
        .or_else(|| result.structured.clone());
    let structured = structured_source.and_then(|value| {
        structured_payload_from_vertical_summary(
            result.extractor_name.as_deref().unwrap_or("vertical"),
            value,
            cfg.structured_data_max_bytes,
        )
    });
    let source = SourceDocument::try_new_web_markdown(
        result.url.clone(),
        result.markdown.clone(),
        "scrape",
        result.title.clone(),
        result.extra.clone(),
        result.extractor_name.clone(),
        structured,
    )
    .map_err(|err| anyhow::anyhow!(err))?;
    prepare_source_document(source)
        .await
        .map_err(|err| anyhow::anyhow!(err))
}

#[cfg(test)]
#[path = "scrape_tests.rs"]
mod tests;
