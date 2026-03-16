use crate::crates::core::config::Config;
use crate::crates::core::http::validate_url;
use crate::crates::crawl::engine::map_with_sitemap;
use crate::crates::services::events::{LogLevel, ServiceEvent, emit};
use crate::crates::services::types::{MapOptions, MapResult};
use std::error::Error;
use tokio::sync::mpsc;

/// Discover all URLs for a site starting at `url`.
///
/// Calls [`map_with_sitemap`] from the crawl engine directly, applies
/// `opts.limit`/`opts.offset` pagination, and wraps the result into a typed
/// [`MapResult`]. Emits log events when a `tx` sender is provided.
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

    let mapped_count = urls.len();

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("map complete: {mapped_count} urls"),
        },
    )
    .await;

    let payload = serde_json::json!({
        "url": url,
        "mapped_urls": mapped_count,
        "sitemap_urls": result.sitemap_urls,
        "pages_seen": result.summary.pages_seen,
        "thin_pages": result.summary.thin_pages,
        "elapsed_ms": result.summary.elapsed_ms,
        "urls": urls,
    });

    Ok(MapResult { payload })
}
