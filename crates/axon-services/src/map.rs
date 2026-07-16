use crate::context::ServiceContext;
use crate::events::{LogLevel, ServiceEvent, emit};
use crate::source::classify::SourceInputKind;
use crate::source::dispatch::web_options::web_crawl_options;
use crate::source::routing::resolve_source_route;
use crate::types::{MapOptions, MapResult};
use axon_api::source::{
    LifecycleStatus, MetadataMap, SourceIntent, SourceManifest, SourceRequest, SourceResult,
    SourceScope,
};
use axon_core::config::Config;
use std::error::Error;
use std::time::Duration;
use tokio::sync::mpsc;

/// Discover all URLs for a site starting at `url`, using a full
/// [`SourceRequest`] and web-adapter map-scope discovery.
///
/// This executes the same source pipeline as `axon source` with `intent=Map`,
/// `scope=Map`, and `embed=false`, then projects the committed manifest into
/// the retained `axon map` convenience result.
#[must_use = "discover returns a Result that should be handled"]
pub async fn discover_with_context(
    ctx: &ServiceContext,
    url: &str,
    opts: MapOptions,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<MapResult, Box<dyn Error>> {
    validate_map_url(url).await?;
    emit_start(&tx, url).await;

    let request = build_map_request_with_config(ctx.cfg(), url);
    let routed = match route_map_request(&request, url, &tx).await? {
        Some(routed) => routed,
        None => {
            return Ok(unsupported_map_result(
                url,
                "map route did not produce a routable source",
            ));
        }
    };
    if routed.kind != SourceInputKind::Web {
        emit_unsupported_kind(&tx, url, routed.kind).await;
        return Ok(unsupported_map_result(
            url,
            format!(
                "map is only supported for web sources today; resolved source kind = {:?}",
                routed.kind
            ),
        ));
    }

    let result = crate::source::index_source(request, ctx).await?;
    if result.status != LifecycleStatus::Completed {
        let mapped = source_result_map_failure(url, &result);
        emit_complete(&tx, mapped.returned_url_count).await;
        return Ok(mapped);
    }
    let Some(runtime) = ctx.target_local_source_runtime() else {
        let mapped = unsupported_map_result(url, "map source runtime is unavailable");
        emit_complete(&tx, mapped.returned_url_count).await;
        return Ok(mapped);
    };
    let Some(generation) = result.ledger.committed_generation.clone() else {
        let mapped = unsupported_map_result(
            url,
            format!("source pipeline completed without a committed generation for {url}"),
        );
        emit_complete(&tx, mapped.returned_url_count).await;
        return Ok(mapped);
    };
    let manifest = runtime
        .ledger
        .get_manifest(result.source_id, generation)
        .await?
        .ok_or_else(|| format!("map source generation did not write a manifest for {url}"))?;
    let mapped = project_manifest(url, manifest, opts);
    emit_complete(&tx, mapped.returned_url_count).await;
    Ok(mapped)
}

fn source_result_map_failure(url: &str, result: &SourceResult) -> MapResult {
    let reason = result
        .warnings
        .first()
        .map(|warning| warning.message.clone())
        .unwrap_or_else(|| format!("source pipeline did not complete: {:?}", result.status));
    unsupported_map_result(url, reason)
}

async fn validate_map_url(url: &str) -> Result<(), Box<dyn Error>> {
    tokio::time::timeout(
        Duration::from_millis(2000),
        axon_core::http::validate_url_with_dns(url),
    )
    .await
    .map_err(|_| -> Box<dyn Error> {
        format!("invalid map url {url}: DNS validation timed out").into()
    })?
    .map_err(|e| -> Box<dyn Error> { format!("invalid map url {url}: {e}").into() })
}

async fn emit_start(tx: &Option<mpsc::Sender<ServiceEvent>>, url: &str) {
    emit(
        tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("starting map: {url}"),
        },
    )
    .await;
}

async fn route_map_request(
    request: &SourceRequest,
    url: &str,
    tx: &Option<mpsc::Sender<ServiceEvent>>,
) -> Result<Option<crate::source::routing::RoutedSource>, Box<dyn Error>> {
    let routed = match resolve_source_route(request) {
        Ok(routed) => routed,
        Err(err) => {
            emit(
                tx,
                ServiceEvent::Log {
                    level: LogLevel::Warn,
                    message: format!("map route degraded for {url}: {err}"),
                },
            )
            .await;
            return Ok(None);
        }
    };
    Ok(Some(routed))
}

async fn emit_unsupported_kind(
    tx: &Option<mpsc::Sender<ServiceEvent>>,
    url: &str,
    kind: SourceInputKind,
) {
    emit(
        tx,
        ServiceEvent::Log {
            level: LogLevel::Warn,
            message: format!("map unsupported for resolved source kind {kind:?}: {url}"),
        },
    )
    .await;
}

async fn emit_complete(tx: &Option<mpsc::Sender<ServiceEvent>>, mapped_count: u64) {
    emit(
        tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("map complete: {mapped_count} urls"),
        },
    )
    .await;
}

fn project_manifest(url: &str, manifest: SourceManifest, opts: MapOptions) -> MapResult {
    let total = manifest.items.len() as u64;
    let limit = if opts.limit == 0 {
        usize::MAX
    } else {
        opts.limit
    };
    let urls: Vec<String> = manifest
        .items
        .into_iter()
        .map(|item| item.canonical_uri)
        .skip(opts.offset)
        .take(limit)
        .collect();
    let mapped_count = urls.len() as u64;

    MapResult {
        url: url.to_string(),
        returned_url_count: mapped_count,
        total,
        sitemap_urls: metadata_usize(&manifest.metadata, "sitemap_urls"),
        pages_seen: metadata_u32(&manifest.metadata, "pages_seen"),
        thin_pages: metadata_u32(&manifest.metadata, "thin_pages"),
        elapsed_ms: metadata_u64(&manifest.metadata, "elapsed_ms"),
        map_source: metadata_string(&manifest.metadata, "map_source")
            .unwrap_or_else(|| "adapter".to_string()),
        warning: metadata_string(&manifest.metadata, "warning"),
        urls,
    }
}

fn metadata_u64(metadata: &MetadataMap, key: &str) -> u64 {
    metadata
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
}

fn metadata_u32(metadata: &MetadataMap, key: &str) -> u32 {
    metadata_u64(metadata, key).try_into().unwrap_or(u32::MAX)
}

fn metadata_usize(metadata: &MetadataMap, key: &str) -> usize {
    metadata_u64(metadata, key).try_into().unwrap_or(usize::MAX)
}

fn metadata_string(metadata: &MetadataMap, key: &str) -> Option<String> {
    metadata
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

/// Build the `SourceRequest` a map operation routes through the pipeline.
///
/// `intent = Map`, `embed = false` (map never writes vectors — source-pipeline.md
/// Validation Checklist), `scope = Map` (adapter-declared map acquisition
/// strategy).
pub(crate) fn build_map_request(url: &str) -> SourceRequest {
    let mut request = SourceRequest::new(url.to_string());
    request.intent = SourceIntent::Map;
    request.embed = false;
    request.scope = Some(SourceScope::Map);
    request
}

fn build_map_request_with_config(cfg: &Config, url: &str) -> SourceRequest {
    let mut request = build_map_request(url);
    request.options.values = web_crawl_options(cfg, None, None);
    request
}

/// Degraded [`MapResult`] for a source that has no map discovery adapter yet
/// (anything other than `web`), or that failed pipeline routing/authorization.
pub(crate) fn unsupported_map_result(url: &str, reason: impl Into<String>) -> MapResult {
    MapResult {
        url: url.to_string(),
        returned_url_count: 0,
        total: 0,
        sitemap_urls: 0,
        pages_seen: 0,
        thin_pages: 0,
        elapsed_ms: 0,
        map_source: "unsupported".to_string(),
        warning: Some(reason.into()),
        urls: Vec::new(),
    }
}

/// Parse a raw JSON value into a typed [`MapResult`].
///
/// Pure function — no network required. Tests call this with JSON literals.
/// Returns an error if any required field is missing or has the wrong type.
pub fn parse_map_result(v: serde_json::Value) -> Result<MapResult, Box<dyn Error>> {
    serde_json::from_value(v)
        .map_err(|e| -> Box<dyn Error> { format!("map result parse error: {e}").into() })
}

#[cfg(test)]
#[path = "map_tests.rs"]
mod tests;
