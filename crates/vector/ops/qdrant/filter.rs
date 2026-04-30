use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};

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

/// Build a Qdrant filter value constraining `scraped_at` to [since, before].
///
/// Returns `Ok(None)` when both arguments are `None` (no filter applied).
/// Invalid date values are returned as errors so callers never silently widen
/// retrieval by dropping a bad bound.
pub(crate) fn build_scraped_at_filter(
    since: Option<&str>,
    before: Option<&str>,
) -> Result<Option<serde_json::Value>, String> {
    let gte = since
        .map(|s| {
            parse_time_filter(s)
                .map(|dt| dt.to_rfc3339())
                .map_err(|e| format!("--since parse error: {e}"))
        })
        .transpose()?;

    let lte = before
        .map(|s| {
            parse_time_filter(s)
                .map(|dt| dt.to_rfc3339())
                .map_err(|e| format!("--before parse error: {e}"))
        })
        .transpose()?;

    if gte.is_none() && lte.is_none() {
        return Ok(None);
    }

    let mut range = serde_json::Map::new();
    if let Some(v) = gte {
        range.insert("gte".to_string(), serde_json::Value::String(v));
    }
    if let Some(v) = lte {
        range.insert("lte".to_string(), serde_json::Value::String(v));
    }

    Ok(Some(serde_json::json!({
        "must": [{
            "key": "scraped_at",
            "range": range
        }]
    })))
}

pub(crate) fn url_filter(url_match: &str) -> serde_json::Value {
    serde_json::json!({
        "must": [{
            "key": "url",
            "match": {"value": url_match}
        }]
    })
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
