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
/// Returns `None` when both arguments are `None` (no filter applied).
/// On parse error the bad argument is ignored and a warning is logged.
pub(crate) fn build_scraped_at_filter(
    since: Option<&str>,
    before: Option<&str>,
) -> Option<serde_json::Value> {
    use crate::crates::core::logging::log_warn;

    let gte = since.and_then(|s| {
        parse_time_filter(s)
            .map_err(|e| log_warn(&format!("--since parse error: {e}")))
            .ok()
            .map(|dt| dt.to_rfc3339())
    });

    let lte = before.and_then(|s| {
        parse_time_filter(s)
            .map_err(|e| log_warn(&format!("--before parse error: {e}")))
            .ok()
            .map(|dt| dt.to_rfc3339())
    });

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
        assert!(build_scraped_at_filter(None, None).is_none());
    }

    #[test]
    fn since_only_builds_gte_range() {
        let f = build_scraped_at_filter(Some("2026-01-01"), None);
        assert!(f.is_some());
        let f = f.unwrap();
        let range = &f["must"][0]["range"];
        assert!(range["gte"].as_str().is_some(), "gte must be set");
        assert!(range["lte"].is_null(), "lte must not be set for since-only");
        assert_eq!(f["must"][0]["key"].as_str(), Some("scraped_at"));
    }

    #[test]
    fn before_only_builds_lte_range() {
        let f = build_scraped_at_filter(None, Some("2026-03-01"));
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
        let f = build_scraped_at_filter(Some("2026-01-01"), Some("2026-03-01"));
        assert!(f.is_some());
        let f = f.unwrap();
        let range = &f["must"][0]["range"];
        assert!(range["gte"].as_str().is_some());
        assert!(range["lte"].as_str().is_some());
    }

    #[test]
    fn invalid_since_returns_none_when_no_valid_bounds() {
        // If since is invalid and before is None, result should be None
        let f = build_scraped_at_filter(Some("not-a-date"), None);
        assert!(f.is_none(), "invalid-only filter must return None");
    }

    #[test]
    fn shorthand_since_produces_valid_rfc3339_in_filter() {
        let f = build_scraped_at_filter(Some("7d"), None).unwrap();
        let gte = f["must"][0]["range"]["gte"].as_str().unwrap();
        // Must be parseable as RFC3339
        let parsed = DateTime::parse_from_rfc3339(gte);
        assert!(parsed.is_ok(), "gte must be valid RFC3339: {gte}");
    }
}
