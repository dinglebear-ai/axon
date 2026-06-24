use crate::logging::log_warn;

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
        if is_rejected_forwarded_header(k) {
            log_warn(&format!(
                "skipping header {k:?}: hop-by-hop and internal forwarding headers are not allowed"
            ));
            continue;
        }
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

pub fn validate_custom_header_policy(raw_headers: &[String]) -> Result<(), String> {
    for raw in raw_headers {
        let Some((name, _value)) = raw.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if is_rejected_forwarded_header(name) {
            return Err(format!(
                "header {name:?} is not allowed; hop-by-hop and internal forwarding headers cannot be forwarded"
            ));
        }
    }
    Ok(())
}

fn is_rejected_forwarded_header(name: &str) -> bool {
    let name = name.trim().to_ascii_lowercase();
    if name.starts_with("x-forwarded-") {
        return true;
    }
    matches!(
        name.as_str(),
        "connection"
            | "keep-alive"
            | "proxy-connection"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "host"
            | "content-length"
            | "forwarded"
            | "via"
            | "x-real-ip"
    )
}
#[cfg(test)]
#[path = "headers_tests.rs"]
mod tests;
