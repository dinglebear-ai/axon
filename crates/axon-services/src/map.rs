use crate::events::{LogLevel, ServiceEvent, emit};
use crate::source::classify::SourceInputKind;
use crate::source::routing::resolve_source_route;
use crate::types::{MapOptions, MapResult};
use axon_adapters::web_engine::engine::map_with_sitemap;
use axon_api::source::{SourceIntent, SourceRequest, SourceScope};
use axon_core::config::Config;
use std::error::Error;
use std::time::Duration;
use tokio::sync::mpsc;

/// Discover all URLs for a site starting at `url`.
///
/// Constructs a `SourceRequest { intent: Map, embed: false, scope: Map }`
/// (source-pipeline.md `SourceRequest.intent` row) and routes it through the
/// canonical `SourceResolver` -> `SourceRouter` pair
/// (`crate::source::routing::resolve_source_route`) instead of calling the
/// crawl engine directly, so non-web sources get a real unsupported/degraded
/// [`MapResult`] instead of silently running web-only crawl behavior. Web
/// sources still discover via [`map_with_sitemap`] — the web adapter's map
/// scope has no other discovery implementation yet; swapping that acquisition
/// call for a full adapter dispatch is out of scope here (legacy crawl-crate
/// removal is a separate workstream). No vectors are written for a map
/// request (`embed = false`). Applies `opts.limit`/`opts.offset` pagination
/// and emits log events when a `tx` sender is provided.
#[must_use = "discover returns a Result that should be handled"]
pub async fn discover(
    cfg: &Config,
    url: &str,
    opts: MapOptions,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<MapResult, Box<dyn Error>> {
    tokio::time::timeout(
        Duration::from_millis(2000),
        axon_core::http::validate_url_with_dns(url),
    )
    .await
    .map_err(|_| -> Box<dyn Error> {
        format!("invalid map url {url}: DNS validation timed out").into()
    })?
    .map_err(|e| -> Box<dyn Error> { format!("invalid map url {url}: {e}").into() })?;

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("starting map: {url}"),
        },
    )
    .await;

    // Route through the pipeline's SourceResolver -> SourceRouter (Stage
    // Registry: `resolving`/`routing` may degrade, never mutate). A routing
    // failure (e.g. an adapter that does not support `scope = Map`, such as
    // `git`) degrades to an unsupported `MapResult` rather than bubbling an
    // `Err`, matching `index_source`'s `route_error_result` precedent.
    let request = build_map_request(url);
    let routed = match resolve_source_route(&request) {
        Ok(routed) => routed,
        Err(err) => {
            emit(
                &tx,
                ServiceEvent::Log {
                    level: LogLevel::Warn,
                    message: format!("map route degraded for {url}: {err}"),
                },
            )
            .await;
            return Ok(unsupported_map_result(
                url,
                format!("map route error: {err}"),
            ));
        }
    };

    if routed.kind != SourceInputKind::Web {
        emit(
            &tx,
            ServiceEvent::Log {
                level: LogLevel::Warn,
                message: format!(
                    "map unsupported for resolved source kind {:?}: {url}",
                    routed.kind
                ),
            },
        )
        .await;
        return Ok(unsupported_map_result(
            url,
            format!(
                "map is only supported for web sources today; resolved source kind = {:?}",
                routed.kind
            ),
        ));
    }

    let result = map_with_sitemap(cfg, url).await?;

    // Record the pre-pagination total before consuming the iterator.
    let total = result.urls.len() as u64;

    let limit = if opts.limit == 0 {
        usize::MAX
    } else {
        opts.limit
    };

    let urls: Vec<String> = result
        .urls
        .into_iter()
        .skip(opts.offset)
        .take(limit)
        .collect();

    let mapped_count = urls.len() as u64;

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("map complete: {mapped_count} urls"),
        },
    )
    .await;

    // pages_seen is 0 in sitemap/bounded-structure modes (no pages were crawled).
    // In crawl mode, summary.pages_seen carries the actual crawl count.
    let pages_seen = result.summary.pages_seen;
    let thin_pages = result.summary.thin_pages;

    Ok(MapResult {
        url: url.to_string(),
        returned_url_count: mapped_count,
        total,
        sitemap_urls: result.sitemap_urls,
        pages_seen,
        thin_pages,
        elapsed_ms: result.summary.elapsed_ms as u64,
        map_source: result.map_source,
        warning: result.warning,
        urls,
    })
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
