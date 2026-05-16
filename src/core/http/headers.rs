use crate::core::logging::log_warn;

/// Parse `"Key: Value"` header strings into a `HeaderMap`.
///
/// Single source of truth for custom header parsing — used by extract engine,
/// crawl engine, and scrape paths. Malformed or invalid headers are skipped
/// with a warning log.
pub fn parse_custom_headers(raw_headers: &[String]) -> reqwest::header::HeaderMap {
    let mut map = reqwest::header::HeaderMap::new();
    for raw in raw_headers {
        let Some((k, v)) = raw.split_once(": ") else {
            log_warn("skipping malformed header (no ': ' separator)");
            continue;
        };
        match (
            reqwest::header::HeaderName::from_bytes(k.as_bytes()),
            reqwest::header::HeaderValue::from_str(v),
        ) {
            (Ok(name), Ok(val)) => {
                map.insert(name, val);
            }
            (Err(e), _) => {
                log_warn(&format!("skipping header with invalid name {k:?}: {e}"));
            }
            (_, Err(e)) => {
                log_warn(&format!("skipping header with invalid value {k:?}: {e}"));
            }
        }
    }
    map
}

#[cfg(test)]
#[path = "headers_tests.rs"]
mod tests;
