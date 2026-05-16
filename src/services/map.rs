use crate::core::config::Config;
use crate::core::http::validate_url;
use crate::crawl::engine::map_with_sitemap;
use crate::services::events::{LogLevel, ServiceEvent, emit};
use crate::services::types::{MapOptions, MapResult};
use std::error::Error;
use tokio::sync::mpsc;

/// Discover all URLs for a site starting at `url`.
///
/// Calls [`map_with_sitemap`] from the crawl engine directly, applies
/// `opts.limit`/`opts.offset` pagination, and wraps the result into a typed
/// [`MapResult`]. Emits log events when a `tx` sender is provided.
#[must_use = "discover returns a Result that should be handled"]
pub async fn discover(
    cfg: &Config,
    url: &str,
    opts: MapOptions,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<MapResult, Box<dyn Error>> {
    validate_url(url)?;

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("starting map: {url}"),
        },
    )
    .await;

    let result = map_with_sitemap(cfg, url).await?;

    // Record the pre-pagination total before consuming the iterator.
    let total = result.urls.len() as u64;

    // Apply pagination: skip `offset` entries, then take up to `limit` (0 = all).
    let urls: Vec<String> = result
        .urls
        .into_iter()
        .skip(opts.offset)
        .take(if opts.limit == 0 {
            usize::MAX
        } else {
            opts.limit
        })
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
