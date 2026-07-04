use std::error::Error;
use std::time::Duration;

use axon_core::config::{Config, ConfigOverrides, ScrapeFormat};
use axon_vector::ops::qdrant::retrieve_result;

use crate::document::{
    decode_document_cursor_backend, is_stale, paginate_document, read_latest_stored_source,
};
use crate::query::map_direct_retrieve_result;
use crate::scrape as scrape_svc;
use crate::types::{DocumentBackend, RetrieveOptions, RetrieveResult, ServiceRetrieveVariantError};

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

/// Retrieve stored document chunks for a URL.
#[must_use = "retrieve returns a Result that should be handled"]
pub async fn retrieve(
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
    let resolved = resolve_document(cfg, url, opts.max_points, pinned_backend).await?;
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
    cfg: &Config,
    url: &str,
    max_points: Option<usize>,
    pinned_backend: Option<DocumentBackend>,
) -> Result<ResolvedDocument, Box<dyn Error + Send + Sync>> {
    if let Some(backend) = pinned_backend {
        return match backend {
            DocumentBackend::Qdrant => resolve_qdrant_document(cfg, url, max_points)
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
    match resolve_qdrant_document(cfg, url, max_points).await {
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
    cfg: &Config,
    url: &str,
    max_points: Option<usize>,
) -> Result<Option<ResolvedDocument>, Box<dyn Error + Send + Sync>> {
    let result = retrieve_result(cfg, url, max_points).await.map_err(
        |e| -> Box<dyn Error + Send + Sync> {
            format!("qdrant retrieve failed for {url}: {e}").into()
        },
    )?;
    if result.chunk_count == 0 {
        return Ok(None);
    }
    let mapped = map_direct_retrieve_result(result);
    Ok(Some(ResolvedDocument {
        backend: DocumentBackend::Qdrant,
        content: mapped.content,
        chunk_count: mapped.chunk_count,
        matched_url: mapped.matched_url,
        warnings: mapped.warnings,
        variant_errors: mapped.variant_errors,
        source_truncated: mapped.truncated,
        refresh_status: None,
    }))
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
