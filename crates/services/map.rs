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
        mapped_urls: mapped_count,
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
mod tests {
    use super::parse_map_result;
    use serde_json::json;

    // ── parse_map_result ──────────────────────────────────────────────────────

    #[test]
    fn parse_map_result_valid_full() {
        let v = json!({
            "url": "https://example.com",
            "mapped_urls": 3u64,
            "total": 3u64,
            "sitemap_urls": 3usize,
            "pages_seen": 2u32,
            "thin_pages": 0u32,
            "elapsed_ms": 100u64,
            "map_source": "sitemap",
            "warning": null,
            "urls": [
                "https://example.com/a",
                "https://example.com/b",
                "https://example.com/c"
            ]
        });
        let result = parse_map_result(v).unwrap();
        assert_eq!(result.url, "https://example.com");
        assert_eq!(result.mapped_urls, 3);
        assert_eq!(result.sitemap_urls, 3);
        assert_eq!(result.pages_seen, 2);
        assert_eq!(result.thin_pages, 0);
        assert_eq!(result.elapsed_ms, 100);
        assert_eq!(result.map_source, "sitemap");
        assert!(result.warning.is_none());
        assert_eq!(result.urls.len(), 3);
        assert_eq!(result.urls[0], "https://example.com/a");
    }

    #[test]
    fn parse_map_result_with_warning() {
        let v = json!({
            "url": "https://example.com",
            "mapped_urls": 1u64,
            "total": 1u64,
            "sitemap_urls": 0usize,
            "pages_seen": 0u32,
            "thin_pages": 0u32,
            "elapsed_ms": 50u64,
            "map_source": "bounded-structure",
            "warning": "too few urls found",
            "urls": ["https://example.com/"]
        });
        let result = parse_map_result(v).unwrap();
        assert_eq!(result.warning.as_deref(), Some("too few urls found"));
        assert_eq!(result.map_source, "bounded-structure");
    }

    #[test]
    fn parse_map_result_missing_url() {
        let v = json!({
            "mapped_urls": 0u64,
            "total": 0u64,
            "sitemap_urls": 0usize,
            "pages_seen": 0u32,
            "thin_pages": 0u32,
            "elapsed_ms": 0u64,
            "map_source": "sitemap",
            "warning": null,
            "urls": []
        });
        let err = parse_map_result(v).unwrap_err();
        assert!(
            err.to_string().contains("url") || err.to_string().contains("missing field"),
            "error must mention missing field, got: {err}"
        );
    }

    #[test]
    fn parse_map_result_missing_mapped_urls() {
        let v = json!({
            "url": "https://example.com",
            "total": 0u64,
            "sitemap_urls": 0usize,
            "pages_seen": 0u32,
            "thin_pages": 0u32,
            "elapsed_ms": 0u64,
            "map_source": "sitemap",
            "warning": null,
            "urls": []
        });
        let err = parse_map_result(v).unwrap_err();
        assert!(
            err.to_string().contains("mapped_urls") || err.to_string().contains("missing field"),
            "error must mention missing field, got: {err}"
        );
    }

    #[test]
    fn parse_map_result_missing_urls_array() {
        let v = json!({
            "url": "https://example.com",
            "mapped_urls": 0u64,
            "total": 0u64,
            "sitemap_urls": 0usize,
            "pages_seen": 0u32,
            "thin_pages": 0u32,
            "elapsed_ms": 0u64,
            "map_source": "sitemap",
            "warning": null
        });
        let err = parse_map_result(v).unwrap_err();
        assert!(
            err.to_string().contains("urls") || err.to_string().contains("missing field"),
            "error must mention missing field, got: {err}"
        );
    }

    #[test]
    fn parse_map_result_empty_urls_array() {
        let v = json!({
            "url": "https://example.com",
            "mapped_urls": 0u64,
            "total": 0u64,
            "sitemap_urls": 0usize,
            "pages_seen": 0u32,
            "thin_pages": 0u32,
            "elapsed_ms": 0u64,
            "map_source": "crawl",
            "warning": null,
            "urls": []
        });
        let result = parse_map_result(v).unwrap();
        assert!(result.urls.is_empty());
        assert_eq!(result.mapped_urls, 0);
    }

    #[test]
    fn parse_map_result_round_trips_via_serde() {
        let original = crate::crates::services::types::MapResult {
            url: "https://example.com".to_string(),
            mapped_urls: 2,
            total: 10,
            sitemap_urls: 5,
            pages_seen: 1,
            thin_pages: 0,
            elapsed_ms: 300,
            map_source: "crawl".to_string(),
            warning: Some("low coverage".to_string()),
            urls: vec![
                "https://example.com/a".to_string(),
                "https://example.com/b".to_string(),
            ],
        };
        let v = serde_json::to_value(&original).unwrap();
        let parsed = parse_map_result(v).unwrap();
        assert_eq!(original, parsed);
    }
}
