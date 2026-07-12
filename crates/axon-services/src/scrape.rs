use crate::contract_write::{
    adhoc_generation, embed_and_upsert_documents, prepare_document, retain_contract_fields,
    stable_token,
};
use crate::events::{LogLevel, ServiceEvent, emit};
use crate::types::ScrapeResult;
use axon_api::source::{
    ChunkHint, ContentKind, ContentRef, DocumentId, MetadataMap, ParserHint, PreparedDocument,
    SourceDocument, SourceId, SourceItemKey, SourceScope,
};
use axon_core::config::Config;
use axon_core::http::normalize_url;
use futures_util::stream::{self, StreamExt};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::time::Duration;
use tokio::sync::mpsc;

pub use axon_adapters::web_engine::scrape::map_scrape_payload;

/// Scrape a single URL and return a typed [`ScrapeResult`].
///
/// Generic HTTP-fetch path only — vertical-extractor auto-routing was removed
/// with `axon-extract` (see
/// docs/pipeline-unification/plans/2026-07-04-phase-12-old-crate-removal-final-issue-sync.md);
/// no per-site enrichment happens here today.
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
    let mut result = axon_adapters::web_engine::scrape::scrape_to_result(cfg, &normalized).await?;
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
/// Behavior note: the legacy `axon_vector` path also attached vertical
/// extractor structured-data payloads (`structured_kind`/`structured_type`/
/// `structured_blob`) to every chunk. `structured_payload` below now carries
/// [`ScrapeResult::structured_for_embedding`] through to
/// `axon_document::preparer::project_structured_payload_metadata`, which
/// projects it to the `web_structured_kind`/`web_structured_blob` fields the
/// vector payload contract's per-family allowlist declares for `"web"`
/// (`axon-vectors::payload_families::VECTOR_SOURCE_FAMILY_FIELDS`) — so the
/// wiring here is no longer a dead end. In practice `structured_for_embedding`
/// is always `None` today: the generic single-page scrape path
/// (`axon_crawl::scrape::scrape_to_result` / `map_scrape_payload`) does not
/// run any JSON-LD/`__NEXT_DATA__`/SvelteKit extraction (that ran only
/// through the vertical-extractor framework removed with `axon-extract`), so
/// this is currently inert rather than a functional restoration. It is also
/// not guaranteed to carry the same `{kind, blob, schema_type?, schema_id?}`
/// envelope the crawl-manifest path uses (`axon-adapters::web::bounded_structured_payload`,
/// `axon-crawl::engine::collector::page::extract_structured_blob`) if a
/// future producer repopulates it under the old vertical-extractor shape —
/// `project_structured_payload_metadata`'s `schema_type`/`kind` lookups
/// degrade gracefully (no `web_structured_kind`) but `web_structured_blob`
/// still gets the raw JSON-stringified value either way.
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
