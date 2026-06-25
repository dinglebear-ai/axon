use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

/// Parse a human-friendly date string into a UTC `DateTime`.
///
/// Accepted formats:
/// - `Nd`  — N days ago (e.g. `7d`, `30d`)
/// - `Nw`  — N weeks ago (e.g. `1w`, `4w`)
/// - `YYYY-MM-DD` — start of that day UTC
/// - RFC3339 string (e.g. `2026-01-01T00:00:00Z`)
pub(crate) fn parse_time_filter(s: &str) -> Result<DateTime<Utc>, String> {
    // Nd shorthand: 7d, 30d, 90d
    if let Some(rest) = s.strip_suffix('d') {
        let n: i64 = rest
            .parse()
            .map_err(|_| format!("invalid day count: {s}"))?;
        if n <= 0 {
            return Err(format!("day count must be positive: {s}"));
        }
        return Ok(Utc::now() - Duration::days(n));
    }
    // Nw shorthand: 1w, 4w
    if let Some(rest) = s.strip_suffix('w') {
        let n: i64 = rest
            .parse()
            .map_err(|_| format!("invalid week count: {s}"))?;
        if n <= 0 {
            return Err(format!("week count must be positive: {s}"));
        }
        return Ok(Utc::now() - Duration::weeks(n));
    }
    // YYYY-MM-DD
    if s.len() == 10 && s.chars().nth(4) == Some('-') {
        let date = NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .map_err(|e| format!("invalid date '{s}': {e}"))?;
        return date
            .and_hms_opt(0, 0, 0)
            .and_then(|dt| Utc.from_local_datetime(&dt).single())
            .ok_or_else(|| format!("could not convert '{s}' to UTC datetime"));
    }
    // RFC3339
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| format!("invalid RFC3339 date '{s}': {e}"))
}

/// Process-level memoization for parsed `--since`/`--before` strings.
///
/// `dispatch_vector_search` calls `build_scraped_at_filter` on every retrieval,
/// and the dual-embedding ask path calls it twice per question. The same
/// `cfg.since`/`cfg.before` strings recur for the entire process lifetime, so
/// caching the parsed RFC3339 result avoids re-parsing on the hot path.
///
/// **Relative shorthand caveat:** `7d` / `1w` resolve relative to `Utc::now()`.
/// Once cached, subsequent calls return the timestamp anchored to the *first*
/// resolution time. For typical CLI/MCP runs this is intended (one query per
/// invocation; relative bounds shouldn't drift mid-run); for very long-running
/// processes this means relative bounds freeze at first use rather than rolling
/// forward. Acceptable trade-off for the perf gain. (bd axon_rust-d71.23)
type FilterCacheKey = (Option<String>, Option<String>);
type FilterCacheMap = HashMap<FilterCacheKey, CachedFilter>;

static FILTER_CACHE: LazyLock<RwLock<FilterCacheMap>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

#[derive(Clone)]
struct CachedFilter {
    gte: Option<String>,
    lte: Option<String>,
}

fn parse_one(label: &str, s: Option<&str>) -> Result<Option<String>, String> {
    s.map(|raw| {
        parse_time_filter(raw)
            .map(|dt| dt.to_rfc3339())
            .map_err(|e| format!("{label} parse error: {e}"))
    })
    .transpose()
}

/// Build a Qdrant filter value constraining `scraped_at` to [since, before].
///
/// Returns `Ok(None)` when both arguments are `None` (no filter applied).
/// Invalid date values are returned as errors so callers never silently widen
/// retrieval by dropping a bad bound.
pub(crate) fn build_scraped_at_filter(
    since: Option<&str>,
    before: Option<&str>,
) -> Result<Option<serde_json::Value>, String> {
    let key = (since.map(str::to_string), before.map(str::to_string));
    if let Some(cached) = FILTER_CACHE.read().ok().and_then(|m| m.get(&key).cloned()) {
        return Ok(build_filter_value(cached.gte, cached.lte));
    }

    let gte = parse_one("--since", since)?;
    let lte = parse_one("--before", before)?;

    if let Ok(mut m) = FILTER_CACHE.write() {
        m.insert(
            key,
            CachedFilter {
                gte: gte.clone(),
                lte: lte.clone(),
            },
        );
    }

    Ok(build_filter_value(gte, lte))
}

fn build_filter_value(gte: Option<String>, lte: Option<String>) -> Option<serde_json::Value> {
    if gte.is_none() && lte.is_none() {
        return None;
    }
    let mut range = serde_json::Map::new();
    if let Some(v) = gte {
        range.insert("gte".to_string(), serde_json::Value::String(v));
    }
    if let Some(v) = lte {
        range.insert("lte".to_string(), serde_json::Value::String(v));
    }
    Some(serde_json::json!({
        "must": [{
            "key": "scraped_at",
            "range": range
        }]
    }))
}

pub(crate) fn url_filter(url_match: &str) -> serde_json::Value {
    serde_json::json!({
        "must": [{
            "key": "url",
            "match": {"value": url_match}
        }]
    })
}

/// Build a Qdrant filter constraining `payload_schema_version >= min`.
///
/// Returns `None` when `min` is `None` so callers can compose the filter
/// without changing behavior for legacy retrieval paths. Existing points
/// indexed before `axon_rust-lu6a` have no `payload_schema_version` field
/// and will be excluded by any such filter — that's the intended behavior
/// when retrieval needs vertical-aware fields.
///
/// Default ask/query retrieval does NOT apply this filter — backward
/// compatibility with the ~3.79M pre-lu6a points is preserved.
#[allow(dead_code)] // wired for xvu9 / future vertical-aware retrieval paths
pub(crate) fn build_schema_version_filter(min: Option<u32>) -> Option<serde_json::Value> {
    let min = min?;
    Some(serde_json::json!({
        "must": [{
            "key": "payload_schema_version",
            "range": { "gte": min }
        }]
    }))
}

pub(crate) fn combine_must_filters(filters: &[serde_json::Value]) -> serde_json::Value {
    let mut must = Vec::new();
    let mut must_not = Vec::new();
    for filter in filters {
        if let Some(values) = filter.get("must").and_then(|v| v.as_array()) {
            must.extend(values.iter().cloned());
        }
        if let Some(values) = filter.get("must_not").and_then(|v| v.as_array()) {
            must_not.extend(values.iter().cloned());
        }
    }
    let mut filter = serde_json::Map::new();
    filter.insert("must".to_string(), serde_json::Value::Array(must));
    if !must_not.is_empty() {
        filter.insert("must_not".to_string(), serde_json::Value::Array(must_not));
    }
    serde_json::Value::Object(filter)
}

pub fn exclude_local_code_filter() -> serde_json::Value {
    serde_json::json!({
        "must_not": [{
            "key": "source_type",
            "match": {"value": "local_code"}
        }]
    })
}

pub fn build_local_project_code_filter(
    project_key: &str,
    generation: i64,
    path_prefix: Option<&str>,
) -> serde_json::Value {
    let mut must = vec![
        serde_json::json!({"key": "source_type", "match": {"value": "local_code"}}),
        serde_json::json!({"key": "local_project_key", "match": {"value": project_key}}),
        serde_json::json!({"key": "local_index_version", "match": {"value": axon_core::CODE_INDEX_VERSION}}),
        serde_json::json!({"key": "local_generation", "match": {"value": generation}}),
    ];
    if let Some(prefix) = path_prefix {
        must.push(serde_json::json!({"key": "code_path_prefixes", "match": {"value": prefix}}));
    }
    serde_json::json!({ "must": must })
}

#[cfg(test)]
#[path = "filter_tests.rs"]
mod tests;
