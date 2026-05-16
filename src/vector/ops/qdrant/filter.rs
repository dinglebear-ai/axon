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
    for filter in filters {
        if let Some(values) = filter.get("must").and_then(|v| v.as_array()) {
            must.extend(values.iter().cloned());
        }
    }
    serde_json::json!({ "must": must })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    // ── parse_time_filter ──────────────────────────────────────────────────────

    #[test]
    fn parse_days_shorthand() {
        let result = parse_time_filter("7d");
        assert!(result.is_ok(), "7d must parse: {:?}", result);
        let dt = result.unwrap();
        let diff = Utc::now() - dt;
        // Should be approximately 7 days ago (allow ±5 seconds for test execution time)
        assert!(diff.num_seconds() >= 7 * 86_400 - 5);
        assert!(diff.num_seconds() <= 7 * 86_400 + 5);
    }

    #[test]
    fn parse_weeks_shorthand() {
        let result = parse_time_filter("2w");
        assert!(result.is_ok(), "2w must parse: {:?}", result);
        let dt = result.unwrap();
        let diff = Utc::now() - dt;
        assert!(diff.num_seconds() >= 14 * 86_400 - 5);
        assert!(diff.num_seconds() <= 14 * 86_400 + 5);
    }

    #[test]
    fn parse_iso_date() {
        let result = parse_time_filter("2026-01-15");
        assert!(result.is_ok(), "YYYY-MM-DD must parse: {:?}", result);
        let dt = result.unwrap();
        assert_eq!(
            dt.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            "2026-01-15T00:00:00Z"
        );
    }

    #[test]
    fn parse_rfc3339() {
        let result = parse_time_filter("2026-06-01T12:00:00Z");
        assert!(result.is_ok(), "RFC3339 must parse: {:?}", result);
        let dt = result.unwrap();
        assert_eq!(
            dt.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            "2026-06-01T12:00:00Z"
        );
    }

    #[test]
    fn parse_invalid_returns_err() {
        assert!(parse_time_filter("banana").is_err());
        assert!(parse_time_filter("0d").is_err());
        assert!(parse_time_filter("-7d").is_err());
        assert!(parse_time_filter("2026-99-99").is_err());
    }

    // ── build_scraped_at_filter ───────────────────────────────────────────────

    #[test]
    fn both_none_returns_none() {
        assert!(build_scraped_at_filter(None, None).unwrap().is_none());
    }

    #[test]
    fn since_only_builds_gte_range() {
        let f = build_scraped_at_filter(Some("2026-01-01"), None).unwrap();
        assert!(f.is_some());
        let f = f.unwrap();
        let range = &f["must"][0]["range"];
        assert!(range["gte"].as_str().is_some(), "gte must be set");
        assert!(range["lte"].is_null(), "lte must not be set for since-only");
        assert_eq!(f["must"][0]["key"].as_str(), Some("scraped_at"));
    }

    #[test]
    fn before_only_builds_lte_range() {
        let f = build_scraped_at_filter(None, Some("2026-03-01")).unwrap();
        assert!(f.is_some());
        let f = f.unwrap();
        let range = &f["must"][0]["range"];
        assert!(range["lte"].as_str().is_some(), "lte must be set");
        assert!(
            range["gte"].is_null(),
            "gte must not be set for before-only"
        );
    }

    #[test]
    fn both_bounds_set_correctly() {
        let f = build_scraped_at_filter(Some("2026-01-01"), Some("2026-03-01")).unwrap();
        assert!(f.is_some());
        let f = f.unwrap();
        let range = &f["must"][0]["range"];
        assert!(range["gte"].as_str().is_some());
        assert!(range["lte"].as_str().is_some());
    }

    #[test]
    fn invalid_since_returns_error() {
        let err = build_scraped_at_filter(Some("not-a-date"), None).unwrap_err();
        assert!(
            err.contains("--since parse error"),
            "invalid since must return an error, got: {err}"
        );
    }

    #[test]
    fn invalid_before_returns_error_even_with_valid_since() {
        let err = build_scraped_at_filter(Some("2026-01-01"), Some("not-a-date")).unwrap_err();
        assert!(
            err.contains("--before parse error"),
            "invalid before must return an error, got: {err}"
        );
    }

    #[test]
    fn shorthand_since_produces_valid_rfc3339_in_filter() {
        let f = build_scraped_at_filter(Some("7d"), None).unwrap().unwrap();
        let gte = f["must"][0]["range"]["gte"].as_str().unwrap();
        // Must be parseable as RFC3339
        let parsed = DateTime::parse_from_rfc3339(gte);
        assert!(parsed.is_ok(), "gte must be valid RFC3339: {gte}");
    }

    #[test]
    fn build_schema_version_filter_none_returns_none() {
        assert!(build_schema_version_filter(None).is_none());
    }

    #[test]
    fn build_schema_version_filter_some_emits_range_gte() {
        let f = build_schema_version_filter(Some(2)).expect("filter");
        let must = f["must"].as_array().expect("must array");
        assert_eq!(must.len(), 1);
        assert_eq!(must[0]["key"].as_str(), Some("payload_schema_version"));
        assert_eq!(must[0]["range"]["gte"].as_u64(), Some(2));
    }

    #[test]
    fn schema_version_filter_composes_with_scraped_at() {
        let scraped = build_scraped_at_filter(Some("2026-01-01"), None)
            .unwrap()
            .unwrap();
        let version = build_schema_version_filter(Some(2)).unwrap();
        let combined = combine_must_filters(&[scraped, version]);
        let must = combined["must"].as_array().unwrap();
        assert_eq!(must.len(), 2);
        // Both keys must be present after composition.
        let keys: Vec<&str> = must.iter().filter_map(|m| m["key"].as_str()).collect();
        assert!(keys.contains(&"scraped_at"));
        assert!(keys.contains(&"payload_schema_version"));
    }

    #[test]
    fn combine_must_filters_concatenates_conditions() {
        let combined = combine_must_filters(&[
            url_filter("https://example.com/a"),
            build_scraped_at_filter(Some("2026-01-01"), None)
                .unwrap()
                .unwrap(),
        ]);
        let must = combined["must"].as_array().unwrap();
        assert_eq!(must.len(), 2);
        assert_eq!(must[0]["key"].as_str(), Some("url"));
        assert_eq!(must[1]["key"].as_str(), Some("scraped_at"));
    }
}
