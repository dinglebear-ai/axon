use crate::contract_write::{
    adhoc_generation, embed_and_upsert_documents, prepare_document, retain_contract_fields,
    stable_token,
};
use crate::events::{LogLevel, ServiceEvent, emit};
use crate::types::ScrapeResult;
use axon_api::result::DocumentBackend;
use axon_api::source::{
    ChunkHint, ContentKind, ContentRef, DocumentId, MetadataMap, ParserHint, PreparedDocument,
    SourceDocument, SourceId, SourceItemKey, SourceScope,
};
use axon_core::config::Config;
use axon_core::http::normalize_url;
use axon_extract::{VerticalContext, dispatch_by_url};
use futures_util::stream::{self, StreamExt};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

pub use axon_adapters::web_engine::scrape::map_scrape_payload;

pub const MAX_PUBLIC_STRUCTURED_BYTES: usize = 16 * 1024;

/// Scrape a single URL and return a typed [`ScrapeResult`].
///
/// Runs the restored vertical-extractor catalog first when
/// `cfg.enable_verticals` is true, then falls back to the generic HTTP/Chrome
/// scrape path when no extractor claims the URL or automatic extraction
/// degrades. This keeps single-page `scrape` aligned with the unified web
/// source adapter's vertical acquisition behavior.
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
    let normalized = validate_and_normalize_scrape_url(url, &tx).await?;
    let mut result = if let Some(result) = try_vertical_scrape(cfg, &normalized, &tx).await? {
        result
    } else {
        axon_adapters::web_engine::scrape::scrape_to_result(cfg, &normalized).await?
    };
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("scrape complete: {normalized}"),
        },
    )
    .await;
    // Service-side artifact write: if output_path is configured, write atomically
    // so all callers (CLI, MCP, /v1/actions) share identical write semantics.
    if let Some(output_path) = cfg.output_path.as_ref() {
        axon_core::artifacts::atomic_write_explicit(output_path, result.output.as_bytes())
            .await
            .map_err(|err| -> Box<dyn Error> { err.to_string().into() })?;
        result.artifact_handle = axon_api::contract::ArtifactHandle::try_from_path(
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

async fn try_vertical_scrape(
    cfg: &Config,
    normalized: &str,
    tx: &Option<mpsc::Sender<ServiceEvent>>,
) -> Result<Option<ScrapeResult>, Box<dyn Error>> {
    if !cfg.enable_verticals {
        return Ok(None);
    }
    let ctx = VerticalContext::new(Arc::new(cfg.clone()));
    match tokio::time::timeout(Duration::from_secs(120), dispatch_by_url(normalized, &ctx)).await {
        Ok(Some(Ok(doc))) => {
            let result = vertical_doc_to_scrape_result(doc)?;
            emit(
                tx,
                ServiceEvent::Log {
                    level: LogLevel::Info,
                    message: format!(
                        "scrape vertical extractor complete: {}",
                        result.extractor_name.as_deref().unwrap_or("unknown")
                    ),
                },
            )
            .await;
            Ok(Some(result))
        }
        Ok(Some(Err(err))) => {
            emit(
                tx,
                ServiceEvent::Log {
                    level: LogLevel::Warn,
                    message: format!(
                        "vertical extractor failed for {normalized}; falling back to generic scrape: {err}"
                    ),
                },
            )
            .await;
            Ok(None)
        }
        Ok(None) => Ok(None),
        Err(_) => {
            emit(
                tx,
                ServiceEvent::Log {
                    level: LogLevel::Warn,
                    message: format!(
                        "vertical extractor timed out for {normalized}; falling back to generic scrape"
                    ),
                },
            )
            .await;
            Ok(None)
        }
    }
}

pub fn vertical_doc_to_scrape_result(
    doc: axon_extract::ScrapedDoc,
) -> Result<ScrapeResult, Box<dyn Error>> {
    let links = extract_markdown_links(&doc.markdown);
    let payload = serde_json::json!({
        "url": doc.url,
        "markdown": doc.markdown,
        "links": links
    });
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

pub fn extract_markdown_links(markdown: &str) -> Vec<serde_json::Value> {
    const LIMIT: usize = 512;
    let mut links = Vec::new();
    let bytes = markdown.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i + 3 < len && links.len() < LIMIT {
        if bytes[i] == b']' && bytes[i + 1] == b'(' {
            let href_start = i + 2;
            if let Some(rel) = bytes[href_start..].iter().position(|&b| b == b')') {
                let href = &markdown[href_start..href_start + rel];
                if href.starts_with("http://") || href.starts_with("https://") {
                    let text_end = i;
                    let text_start = markdown[..text_end]
                        .rfind('[')
                        .map(|position| position + 1)
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
    embed_and_upsert_documents(cfg, &cfg.collection, docs)
        .await
        .map_err(|err| -> Box<dyn Error> { format!("{label}: {err}").into() })?;
    Ok(())
}

/// Source-family-specific fields this function stamps directly into
/// `metadata` before building the [`SourceDocument`] (kept via
/// `retain_contract_fields`). Not exhaustive for the `"web"` family: the
/// `web_structured_kind`/`web_structured_blob` fields
/// (`axon_vectors::payload_families::VECTOR_SOURCE_FAMILY_FIELDS`) are added
/// later, downstream, by `axon_document::preparer::project_structured_payload_metadata`
/// from `SourceDocument::structured_payload` rather than here.
const WEB_PAYLOAD_ALLOWED_FIELDS: &[&str] = &["web_title", "web_domain"];

/// Build a [`PreparedDocument`] from a scrape result: a `"web"`-family
/// [`SourceDocument`] (markdown content, routed to `MarkdownSections`
/// chunking) run through `DocumentPreparer`.
///
/// Behavior note: vertical scrapes can attach structured data via
/// [`ScrapeResult::structured_for_embedding`]. `axon_document::preparer`
/// projects that payload to `web_structured_kind`/`web_structured_blob`, the
/// fields declared by the `"web"` vector payload family. Generic HTML scrapes
/// usually leave it empty; vertical outputs preserve the richer structured
/// payload through the same contract path.
pub async fn scrape_result_to_prepared_doc(
    cfg: &Config,
    result: &ScrapeResult,
) -> anyhow::Result<PreparedDocument> {
    let _ = cfg; // kept for API stability; structured-data sizing no longer applies here
    let token = stable_token(&format!("scrape:{}", result.url));
    let mut metadata = MetadataMap::new();
    metadata.insert("source_family".to_string(), serde_json::json!("web"));
    metadata.insert("source_type".to_string(), serde_json::json!("scrape"));
    metadata.insert("source_kind".to_string(), serde_json::json!("web"));
    metadata.insert(
        "source_adapter".to_string(),
        serde_json::json!("web_scrape"),
    );
    metadata.insert(
        "source_scope".to_string(),
        serde_json::json!(SourceScope::Page),
    );
    if let Some(title) = &result.title {
        metadata.insert("web_title".to_string(), serde_json::json!(title));
    }
    metadata.insert(
        "web_domain".to_string(),
        serde_json::json!(axon_core::content::url_to_domain(&result.url)),
    );
    retain_contract_fields(&mut metadata, WEB_PAYLOAD_ALLOWED_FIELDS);

    let document = SourceDocument {
        document_id: DocumentId::new(format!("doc_scrape_{token}")),
        source_id: SourceId::new(format!("src_scrape_{token}")),
        source_item_key: SourceItemKey::new(result.url.clone()),
        canonical_uri: result.url.clone(),
        content_kind: ContentKind::Markdown,
        content: ContentRef::InlineText {
            text: result.markdown.clone(),
        },
        metadata,
        title: result.title.clone(),
        language: None,
        path: None,
        mime_type: None,
        structured_payload: result.structured_for_embedding.clone(),
        artifact_id: None,
        chunk_hints: Vec::<ChunkHint>::new(),
        parser_hints: Vec::<ParserHint>::new(),
    };
    prepare_document(document, adhoc_generation()).map_err(|err| anyhow::anyhow!(err))
}

#[cfg(test)]
#[path = "scrape_tests.rs"]
mod tests;
