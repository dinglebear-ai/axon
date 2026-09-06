use std::error::Error;
use std::time::Duration;

use async_trait::async_trait;
use axon_api::source::{AuthSnapshot, OperationKind};
use axon_core::config::{Config, ConfigOverrides, ScrapeFormat};
use axon_retrieval::retrieve::{
    FullDocumentStore, RetrieveVariantError, RetrievedDocument, retrieve_document,
};
use axon_vectors::qdrant::{QdrantVectorStore, render_full_doc_from_points, retrieve_by_url};

use super::provider_execution::ReadExecution;
use crate::context::ServiceContext;
use crate::document::{
    decode_document_cursor_backend, is_stale, paginate_document, read_latest_stored_source,
};
use crate::scrape as scrape_svc;
use crate::types::{DocumentBackend, RetrieveOptions, RetrieveResult, ServiceRetrieveVariantError};

/// Provider id tag for the read-only Qdrant store `retrieve` constructs
/// directly from `cfg.qdrant_url` (mirrors `suggest`'s equivalent — this path
/// has no dependency on the injected-runtime read stores).
const RETRIEVE_VECTOR_PROVIDER_ID: &str = "axon-services-retrieve";

const RETRIEVE_STALE_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

struct ResolvedDocument {
    backend: DocumentBackend,
    content: String,
    chunk_count: usize,
    matched_url: Option<String>,
    warnings: Vec<String>,
    variant_errors: Vec<ServiceRetrieveVariantError>,
    source_truncated: bool,
    refresh_status: Option<String>,
}

/// Retrieve stored document chunks for a URL. Foreground retrieval stays
/// synchronous at the transport, but Qdrant capacity is reserved through the
/// same durable provider scheduler used by source ingestion.
#[must_use = "retrieve returns a Result that should be handled"]
pub async fn retrieve(
    ctx: &ServiceContext,
    cfg: &Config,
    url: &str,
    opts: RetrieveOptions,
) -> Result<RetrieveResult, Box<dyn Error + Send + Sync>> {
    retrieve_with_auth(ctx, cfg, url, opts, None).await
}

/// Auth-aware retrieve path for authenticated transports.
pub async fn retrieve_with_auth(
    ctx: &ServiceContext,
    cfg: &Config,
    url: &str,
    opts: RetrieveOptions,
    auth_snapshot: Option<AuthSnapshot>,
) -> Result<RetrieveResult, Box<dyn Error + Send + Sync>> {
    if url.starts_with("local-code://") {
        return Err("local-code documents are only available through code_search".into());
    }
    let execution = ReadExecution::begin(
        ctx,
        cfg,
        OperationKind::Retrieve,
        serde_json::json!({ "url": url, "collection": cfg.collection }),
        auth_snapshot,
    )
    .await
    .map_err(|error| -> Box<dyn Error + Send + Sync> { error.to_string().into() })?;
    let result = retrieve_inner(&execution, cfg, url, opts).await;
    execution.finish(ctx, &result).await;
    result
}

async fn retrieve_inner(
    execution: &ReadExecution,
    cfg: &Config,
    url: &str,
    opts: RetrieveOptions,
) -> Result<RetrieveResult, Box<dyn Error + Send + Sync>> {
    if url.starts_with("local-code://") {
        return Err("local-code documents are only available through code_search".into());
    }
    let pinned_backend = decode_document_cursor_backend(opts.cursor.as_deref()).map_err(
        |e| -> Box<dyn Error + Send + Sync> {
            format!("invalid retrieve cursor for {url}: {e}").into()
        },
    )?;
    let resolved = resolve_document(execution, cfg, url, opts.max_points, pinned_backend).await?;
    let page = paginate_document(
        &resolved.content,
        opts.cursor.as_deref(),
        opts.token_budget,
        resolved.backend,
    )
    .map_err(|e| -> Box<dyn Error + Send + Sync> {
        format!("paginate retrieve result for {url}: {e}").into()
    })?;
    Ok(RetrieveResult {
        chunk_count: resolved.chunk_count,
        content: page.content,
        requested_url: Some(url.to_string()),
        matched_url: resolved.matched_url,
        truncated: page.truncated || resolved.source_truncated,
        warnings: resolved.warnings,
        variant_errors: resolved.variant_errors,
        token_estimate: page.token_estimate,
        next_cursor: page.next_cursor,
        remaining_tokens_estimate: page.remaining_tokens_estimate,
        backend: Some(page.backend),
        refresh_status: resolved.refresh_status,
    })
}

async fn resolve_document(
    execution: &ReadExecution,
    cfg: &Config,
    url: &str,
    max_points: Option<usize>,
    pinned_backend: Option<DocumentBackend>,
) -> Result<ResolvedDocument, Box<dyn Error + Send + Sync>> {
    if let Some(backend) = pinned_backend {
        return match backend {
            DocumentBackend::Qdrant => resolve_qdrant_document(execution, cfg, url, max_points)
                .await?
                .ok_or_else(|| {
                    "retrieve cursor requires qdrant backend but no stored chunks exist"
                        .to_string()
                        .into()
                }),
            DocumentBackend::StoredSource => resolve_stored_source_document(cfg, url)
                .await?
                .ok_or_else(|| {
                    "retrieve cursor requires stored_source backend but no source file exists"
                        .to_string()
                        .into()
                }),
            DocumentBackend::LiveScrape => resolve_live_scrape_document(cfg, url, "cursor").await,
        };
    }

    let mut qdrant_error: Option<String> = None;
    match resolve_qdrant_document(execution, cfg, url, max_points).await {
        Ok(Some(qdrant)) => return Ok(qdrant),
        Ok(None) => {}
        Err(err) => qdrant_error = Some(err.to_string()),
    }

    if let Some(stored) = resolve_stored_source_document(cfg, url).await? {
        if stored.refresh_status.as_deref() == Some("stale") {
            match resolve_live_scrape_document(cfg, url, "stale").await {
                Ok(mut refreshed) => {
                    refreshed.warnings.extend(stored.warnings);
                    if let Some(err) = qdrant_error {
                        refreshed
                            .warnings
                            .push(format!("qdrant backend unavailable during retrieve: {err}"));
                    }
                    return Ok(refreshed);
                }
                Err(err) => {
                    let mut stale = stored;
                    stale.warnings.push(format!(
                        "live scrape refresh failed; falling back to stale stored source: {err}"
                    ));
                    if let Some(qdrant_err) = qdrant_error {
                        stale.warnings.push(format!(
                            "qdrant backend unavailable during retrieve: {qdrant_err}"
                        ));
                    }
                    return Ok(stale);
                }
            }
        }
        let mut stored = stored;
        if let Some(err) = qdrant_error {
            stored
                .warnings
                .push(format!("qdrant backend unavailable during retrieve: {err}"));
        }
        return Ok(stored);
    }

    let mut live = resolve_live_scrape_document(cfg, url, "miss").await?;
    if let Some(err) = qdrant_error {
        live.warnings
            .push(format!("qdrant backend unavailable during retrieve: {err}"));
    }
    Ok(live)
}

async fn resolve_qdrant_document(
    execution: &ReadExecution,
    cfg: &Config,
    url: &str,
    max_points: Option<usize>,
) -> Result<Option<ResolvedDocument>, Box<dyn Error + Send + Sync>> {
    let mut store = QdrantVectorStore::new(cfg.qdrant_url.clone(), RETRIEVE_VECTOR_PROVIDER_ID);
    axon_vectors::qdrant::configure_point_buffer(&mut store, cfg.qdrant_point_buffer);
    let collection = cfg.collection.clone();
    let target = url.to_string();
    let doc = execution
        .vector_operation("retrieve-by-url", || async {
            retrieve_document(
                &QdrantFullDocumentAdapter(&store),
                &collection,
                &target,
                max_points,
            )
            .await
        })
        .await
        .map_err(|e| -> Box<dyn Error + Send + Sync> {
            format!("qdrant retrieve failed for {url}: {e}").into()
        })?;
    Ok(map_retrieved_document(url, doc))
}

struct QdrantFullDocumentAdapter<'a>(&'a QdrantVectorStore);

#[async_trait]
impl FullDocumentStore for QdrantFullDocumentAdapter<'_> {
    async fn retrieve_full_document(
        &self,
        collection: &str,
        target: &str,
        max_points: Option<usize>,
    ) -> Result<RetrievedDocument, axon_api::source::ApiError> {
        let result = retrieve_by_url(self.0, collection, target, max_points).await?;
        Ok(RetrievedDocument {
            requested_url: result.requested_url,
            matched_url: result.matched_url,
            chunk_count: result.points.len(),
            max_points: result.max_points,
            truncated: result.truncated,
            variant_errors: result
                .variant_errors
                .into_iter()
                .map(|error| RetrieveVariantError {
                    url: error.url,
                    error: error.error,
                })
                .collect(),
            content: render_full_doc_from_points(&result.points),
        })
    }
}

/// Map a fetched [`RetrievedDocument`] into the service-level
/// [`ResolvedDocument`] shape, or `None` when nothing was indexed for the URL.
///
/// Ports legacy `axon-services`' `map_direct_retrieve_result` for the new
/// `axon_retrieval::retrieve::RetrievedDocument` shape.
///
/// **Disclosed gap:** legacy `retrieve_result` also warned on "N malformed
/// Qdrant point(s) skipped for URL variant X" when a stored payload failed to
/// parse into its typed struct. `QdrantVectorStore::retrieve_by_url` reads raw
/// JSON payload fields instead of a strongly-typed struct, so there is no
/// "malformed point" concept to warn about any more — this diagnostic is
/// intentionally dropped (not a correctness change: the underlying point is
/// still included either way).
fn map_retrieved_document(requested_url: &str, doc: RetrievedDocument) -> Option<ResolvedDocument> {
    if doc.chunk_count == 0 {
        return None;
    }
    let chunk_count = doc.chunk_count;
    let mut warnings = Vec::new();
    if doc.truncated {
        warnings.push(format!(
            "retrieve result truncated at {} point(s) for URL variant {}",
            doc.max_points,
            doc.matched_url.as_deref().unwrap_or(requested_url),
        ));
    }
    Some(ResolvedDocument {
        backend: DocumentBackend::Qdrant,
        content: doc.content,
        chunk_count,
        matched_url: doc.matched_url,
        warnings,
        variant_errors: doc
            .variant_errors
            .into_iter()
            .map(|e| ServiceRetrieveVariantError {
                url: e.url,
                error: e.error,
            })
            .collect(),
        source_truncated: doc.truncated,
        refresh_status: None,
    })
}

async fn resolve_stored_source_document(
    cfg: &Config,
    url: &str,
) -> Result<Option<ResolvedDocument>, Box<dyn Error + Send + Sync>> {
    let Some(stored) = read_latest_stored_source(&cfg.output_dir, url)
        .await
        .map_err(|e| -> Box<dyn Error + Send + Sync> {
            format!("stored source lookup failed for {url}: {e}").into()
        })?
    else {
        return Ok(None);
    };
    let stale = is_stale(stored.modified_at, RETRIEVE_STALE_AFTER);
    let mut warnings = Vec::new();
    if stale {
        warnings.push(format!(
            "stored source is stale (> {} hours old); attempting live refresh",
            RETRIEVE_STALE_AFTER.as_secs() / 3600
        ));
    }
    warnings.push(format!(
        "using stored source file {}",
        stored.path.display()
    ));
    Ok(Some(ResolvedDocument {
        backend: DocumentBackend::StoredSource,
        content: stored.content,
        chunk_count: 0,
        matched_url: Some(url.to_string()),
        warnings,
        variant_errors: Vec::new(),
        source_truncated: false,
        refresh_status: stale.then(|| "stale".to_string()),
    }))
}

async fn resolve_live_scrape_document(
    cfg: &Config,
    url: &str,
    reason: &str,
) -> Result<ResolvedDocument, Box<dyn Error + Send + Sync>> {
    let scrape_cfg = cfg.apply_overrides(&ConfigOverrides {
        format: Some(ScrapeFormat::Markdown),
        output_path: Some(None),
        ..ConfigOverrides::default()
    });
    let result = scrape_svc::scrape(&scrape_cfg, url, None).await.map_err(
        |e| -> Box<dyn Error + Send + Sync> {
            format!("live scrape refresh failed for {url}: {e}").into()
        },
    )?;
    let refresh_status = match reason {
        "stale" => Some("refreshed_stale".to_string()),
        "miss" => Some("refreshed_missing".to_string()),
        "cursor" => Some("cursor_live_scrape".to_string()),
        _ => Some(reason.to_string()),
    };
    let warning = match reason {
        "stale" => "served fresh live scrape because stored source was stale",
        "miss" => "served fresh live scrape because no indexed or stored content was available",
        "cursor" => "continued retrieve via live scrape backend",
        _ => "served fresh live scrape content",
    };
    Ok(ResolvedDocument {
        backend: DocumentBackend::LiveScrape,
        content: result.output,
        chunk_count: 0,
        matched_url: Some(result.url),
        warnings: vec![warning.to_string()],
        variant_errors: Vec::new(),
        source_truncated: false,
        refresh_status,
    })
}

#[cfg(test)]
#[path = "retrieve_map_tests.rs"]
mod map_tests;
