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
mod tests {
    use super::*;

    #[test]
    fn parses_valid_headers() {
        let raw = vec![
            "Authorization: Bearer token123".to_string(),
            "X-Custom: value".to_string(),
        ];
        let map = parse_custom_headers(&raw);
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("authorization").unwrap(), "Bearer token123");
        assert_eq!(map.get("x-custom").unwrap(), "value");
    }

    #[test]
    fn skips_malformed_headers() {
        let raw = vec![
            "Valid: header".to_string(),
            "no-colon-space".to_string(),
            "".to_string(),
        ];
        let map = parse_custom_headers(&raw);
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn empty_input_returns_empty_map() {
        let map = parse_custom_headers(&[]);
        assert!(map.is_empty());
    }
}
