# Temporal Search Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `--since` / `--before` time-range filters to `query`, `ask`, and `evaluate` commands, backed by a Qdrant `datetime` payload index on the existing `scraped_at` field.

**Architecture:** Every Qdrant point already stores `scraped_at` as an RFC3339 string (set at embed time in `pipeline.rs`). We add a `datetime` payload index in `ensure_payload_indexes()` so Qdrant can evaluate range filters efficiently. A new pure-logic `filter.rs` module parses human-friendly date strings (`7d`, `2026-01-01`, RFC3339) into a Qdrant filter JSON value. That filter is threaded as `Option<&serde_json::Value>` through the three Qdrant search functions, then wired at `query.rs` and `ask/context/retrieval.rs` call sites. `evaluate` picks it up for free because it calls `build_ask_context` which calls `retrieve_ask_candidates`.

**Tech Stack:** Rust, chrono (already in `crates/vector`), Qdrant REST API, httpmock (already used in `hybrid.rs` tests)

---

## File Map

| File | Action | What changes |
|------|--------|-------------|
| `crates/vector/ops/tei/qdrant_store.rs` | Modify | Add `"scraped_at"` with `"field_schema": "datetime"` to `ensure_payload_indexes()` |
| `crates/core/config/types/config.rs` | Modify | Add `since: Option<String>` and `before: Option<String>` fields |
| `crates/core/config/types/config_impls.rs` | Modify | Default both to `None` in `Config::default()` |
| `crates/core/config/cli/global_args.rs` | Modify | Add `--since` and `--before` `#[arg(global=true)]` flags |
| `crates/core/config/parse/build_config.rs` | Modify | Map `global.since` / `global.before` → `Config` |
| `crates/vector/ops/qdrant/filter.rs` | **Create** | `build_scraped_at_filter()`, `parse_time_filter()`, unit tests |
| `crates/vector/ops/qdrant.rs` | Modify | Declare `mod filter`, re-export `build_scraped_at_filter` |
| `crates/vector/ops/qdrant/client.rs` | Modify | Add `filter: Option<&serde_json::Value>` param to `qdrant_search()` |
| `crates/vector/ops/qdrant/hybrid.rs` | Modify | Add `filter` param to `qdrant_hybrid_search()` and `qdrant_named_dense_search()`, update their tests |
| `crates/vector/ops/commands/query.rs` | Modify | Build filter from `cfg`, pass to `dispatch_search()` |
| `crates/vector/ops/commands/ask/context/retrieval.rs` | Modify | Build filter from `cfg`, pass to `dispatch_ask_search()` |

**No new files needed** beyond `filter.rs`. `evaluate` is covered automatically — it calls `build_ask_context` → `retrieve_ask_candidates` → `dispatch_ask_search`.

---

## Chunk 1: Qdrant datetime index

### Task 1: Register `scraped_at` as a datetime payload index

**Files:**
- Modify: `crates/vector/ops/tei/qdrant_store.rs` (function `ensure_payload_indexes`, lines ~192–209)

The `ensure_payload_indexes` function already loops over keyword fields and PUTs them. Add `"scraped_at"` with `"field_schema": "datetime"` after the keyword loop. The operation is idempotent — Qdrant returns HTTP 200 when the index already exists.

- [ ] **Step 1: Write the failing test**

Add to `crates/vector/ops/tei/tests.rs` (find the `#[cfg(test)] mod tests` block and add inside it):

```rust
#[tokio::test]
async fn ensure_payload_indexes_registers_scraped_at_as_datetime() {
    use httpmock::prelude::*;

    let server = MockServer::start_async().await;
    // Accept all PUT /index calls — we assert on the datetime one specifically
    let keyword_mock = server
        .mock_async(|when, then| {
            when.method(PUT)
                .path_contains("/index")
                .json_body_includes(r#""field_schema":"keyword""#);
            then.status(200).json_body(serde_json::json!({"result": true, "status": "ok", "time": 0.001}));
        })
        .await;
    let datetime_mock = server
        .mock_async(|when, then| {
            when.method(PUT)
                .path_contains("/index")
                .json_body_includes(r#""field_schema":"datetime""#)
                .json_body_includes(r#""field_name":"scraped_at""#);
            then.status(200).json_body(serde_json::json!({"result": true, "status": "ok", "time": 0.001}));
        })
        .await;

    let mut cfg = crate::crates::jobs::common::test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.qdrant_url = server.base_url();
    cfg.collection = "test_col".to_string();

    super::ensure_payload_indexes(&cfg).await.unwrap();

    datetime_mock.assert_async().await;
    let _ = keyword_mock; // don't assert count — just confirm datetime was called
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test --lib ensure_payload_indexes_registers_scraped_at
```

Expected: FAIL — `scraped_at` index PUT is not sent yet.

- [ ] **Step 3: Add the datetime index to `ensure_payload_indexes()`**

In `crates/vector/ops/tei/qdrant_store.rs`, after the `for field in &[...]` keyword loop, add:

```rust
// datetime index enables efficient range queries on scraped_at (--since / --before)
client
    .put(&index_url)
    .json(&serde_json::json!({
        "field_name": "scraped_at",
        "field_schema": "datetime"
    }))
    .send()
    .await?
    .error_for_status()?;
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo test --lib ensure_payload_indexes_registers_scraped_at
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/vector/ops/tei/qdrant_store.rs crates/vector/ops/tei/tests.rs
git commit -m "feat(vector): register scraped_at datetime payload index in ensure_collection"
```

---

## Chunk 2: Filter builder (pure logic)

### Task 2: Create `filter.rs` — parse date strings, build Qdrant filter JSON

**Files:**
- Create: `crates/vector/ops/qdrant/filter.rs`
- Modify: `crates/vector/ops/qdrant.rs` (declare module, re-export)

The filter builder is pure logic with no I/O. All date parsing and JSON construction happens here. This is the most testable piece — write all the tests first.

Accepted date string formats for `--since` / `--before`:
- `7d` → now minus 7 days
- `30d` → now minus 30 days
- `1w` → now minus 1 week (7 days)
- `4w` → now minus 4 weeks
- `YYYY-MM-DD` → that date at 00:00:00 UTC
- RFC3339 (e.g. `2026-01-01T00:00:00Z`) → parsed directly

Qdrant datetime range filter shape:
```json
{
  "must": [
    { "key": "scraped_at", "range": { "gte": "2026-01-01T00:00:00+00:00" } }
  ]
}
```

- [ ] **Step 1: Write all tests in the new file**

Create `crates/vector/ops/qdrant/filter.rs` with the tests only (functions not yet implemented):

```rust
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};

/// Parse a human-friendly date string into a UTC `DateTime`.
///
/// Accepted formats:
/// - `Nd`  — N days ago (e.g. `7d`, `30d`)
/// - `Nw`  — N weeks ago (e.g. `1w`, `4w`)
/// - `YYYY-MM-DD` — start of that day UTC
/// - RFC3339 string (e.g. `2026-01-01T00:00:00Z`)
pub(crate) fn parse_time_filter(s: &str) -> Result<DateTime<Utc>, String> {
    todo!()
}

/// Build a Qdrant filter value constraining `scraped_at` to [since, before].
///
/// Returns `None` when both arguments are `None` (no filter applied).
/// On parse error the bad argument is ignored and a warning is logged.
pub(crate) fn build_scraped_at_filter(
    since: Option<&str>,
    before: Option<&str>,
) -> Option<serde_json::Value> {
    todo!()
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
        assert_eq!(dt.format("%Y-%m-%dT%H:%M:%SZ").to_string(), "2026-01-15T00:00:00Z");
    }

    #[test]
    fn parse_rfc3339() {
        let result = parse_time_filter("2026-06-01T12:00:00Z");
        assert!(result.is_ok(), "RFC3339 must parse: {:?}", result);
        let dt = result.unwrap();
        assert_eq!(dt.format("%Y-%m-%dT%H:%M:%SZ").to_string(), "2026-06-01T12:00:00Z");
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
        assert!(range["gte"].is_null(), "gte must not be set for before-only");
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
        let parsed = chrono::DateTime::parse_from_rfc3339(gte);
        assert!(parsed.is_ok(), "gte must be valid RFC3339: {gte}");
    }
}
```

- [ ] **Step 2: Run tests to confirm they fail (todo!() panics)**

```bash
cargo test --lib filter
```

Expected: FAIL with `not yet implemented`

- [ ] **Step 3: Implement `parse_time_filter`**

Replace `todo!()` in `parse_time_filter`:

```rust
pub(crate) fn parse_time_filter(s: &str) -> Result<DateTime<Utc>, String> {
    // Nd shorthand: 7d, 30d, 90d
    if let Some(rest) = s.strip_suffix('d') {
        let n: i64 = rest.parse().map_err(|_| format!("invalid day count: {s}"))?;
        if n <= 0 {
            return Err(format!("day count must be positive: {s}"));
        }
        return Ok(Utc::now() - Duration::days(n));
    }
    // Nw shorthand: 1w, 4w
    if let Some(rest) = s.strip_suffix('w') {
        let n: i64 = rest.parse().map_err(|_| format!("invalid week count: {s}"))?;
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
```

- [ ] **Step 4: Implement `build_scraped_at_filter`**

Replace `todo!()` in `build_scraped_at_filter`:

```rust
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
```

- [ ] **Step 5: Run tests to verify all pass**

```bash
cargo test --lib filter
```

Expected: all 10 tests PASS

- [ ] **Step 6: Register module in `crates/vector/ops/qdrant.rs`**

Add `mod filter;` and re-export:

```rust
// in qdrant.rs — add after existing mod declarations:
mod filter;

// add to pub(crate) re-exports:
pub(crate) use filter::build_scraped_at_filter;
```

- [ ] **Step 7: cargo check**

```bash
cargo check
```

Expected: clean

- [ ] **Step 8: Commit**

```bash
git add crates/vector/ops/qdrant/filter.rs crates/vector/ops/qdrant.rs
git commit -m "feat(vector): add scraped_at filter builder with date shorthand parsing"
```

---

## Chunk 3: Config fields and CLI flags

### Task 3: Add `since` / `before` to `Config` and CLI

**Files:**
- Modify: `crates/core/config/types/config.rs`
- Modify: `crates/core/config/types/config_impls.rs`
- Modify: `crates/core/config/cli/global_args.rs`
- Modify: `crates/core/config/parse/build_config.rs`

Both fields are `Option<String>` — same pattern as `search_time_range`. Because they're `Option`, existing `..Default::default()` struct literals in test helpers compile without changes.

- [ ] **Step 1: Add fields to `Config`**

In `crates/core/config/types/config.rs`, find the `search_time_range` field (around line 435) and add after it:

```rust
/// Lower bound for `scraped_at` payload filter on query/ask. Accepts `7d`, `30d`, `1w`,
/// `YYYY-MM-DD`, or RFC3339. Default: None (no lower bound). Flag: `--since`.
pub since: Option<String>,

/// Upper bound for `scraped_at` payload filter on query/ask. Same formats as `--since`.
/// Default: None (no upper bound). Flag: `--before`.
pub before: Option<String>,
```

- [ ] **Step 2: Write test for config defaults**

In `crates/core/config/types/config.rs` test module (or add one):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_since_before_default_none() {
        let cfg = Config::default();
        assert!(cfg.since.is_none());
        assert!(cfg.before.is_none());
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

```bash
cargo test --lib config_since_before_default_none
```

Expected: FAIL — fields don't exist yet (compile error)

- [ ] **Step 4: Add defaults in `config_impls.rs`**

In `crates/core/config/types/config_impls.rs`, find `Config::default()` implementation and add:

```rust
since: None,
before: None,
```

Also add to `fmt::Debug` impl if it manually lists fields (search for `search_time_range` in the Debug impl and add the two fields next to it):

```rust
.field("since", &self.since)
.field("before", &self.before)
```

- [ ] **Step 5: Run test to verify it passes**

```bash
cargo test --lib config_since_before_default_none
```

Expected: PASS

- [ ] **Step 6: Add CLI flags to `global_args.rs`**

In `crates/core/config/cli/global_args.rs`, find `search_time_range` (around line 285) and add after it:

```rust
/// Lower bound for temporal search filter. Formats: 7d, 30d, 1w, YYYY-MM-DD, RFC3339.
/// Filters query/ask results to content indexed on or after this date. Default: none.
#[arg(global = true, long)]
pub(in crate::crates::core::config) since: Option<String>,

/// Upper bound for temporal search filter. Same formats as --since.
/// Filters query/ask results to content indexed on or before this date. Default: none.
#[arg(global = true, long)]
pub(in crate::crates::core::config) before: Option<String>,
```

- [ ] **Step 7: Map fields in `build_config.rs`**

In `crates/core/config/parse/build_config.rs`, find `search_time_range: global.search_time_range,` (around line 513) and add after it:

```rust
since: global.since,
before: global.before,
```

- [ ] **Step 8: cargo check**

```bash
cargo check
```

Expected: clean

- [ ] **Step 9: Commit**

```bash
git add crates/core/config/types/config.rs crates/core/config/types/config_impls.rs \
        crates/core/config/cli/global_args.rs crates/core/config/parse/build_config.rs
git commit -m "feat(config): add --since and --before temporal search flags"
```

---

## Chunk 4: Thread filter through Qdrant search functions

### Task 4: Add `filter` parameter to the three search functions

**Files:**
- Modify: `crates/vector/ops/qdrant/client.rs` — `qdrant_search`
- Modify: `crates/vector/ops/qdrant/hybrid.rs` — `qdrant_hybrid_search`, `qdrant_named_dense_search`

Each function currently builds a fixed request body. We add `filter: Option<&serde_json::Value>` as the last parameter and conditionally insert it into the JSON body before sending.

For `qdrant_hybrid_search`, the filter goes at the **top level** of the `/query` request (post-fusion filter), not inside the prefetch arms. This is correct for recency filtering — Qdrant evaluates it against the merged RRF results using the `scraped_at` datetime index.

The existing httpmock tests in `hybrid.rs` pass `None` as the new parameter — no logic changes to test behavior.

- [ ] **Step 1: Update httpmock tests in `hybrid.rs` to pass `None` (compile-only change)**

The existing tests will fail to compile when the signatures change. Update them pre-emptively by adding `, None` to each call. Find all calls to `qdrant_hybrid_search` and `qdrant_named_dense_search` in the test module and add the trailing `None`:

```rust
// Before:
let result = qdrant_hybrid_search(&cfg, &dense, &sparse, 5).await;
// After:
let result = qdrant_hybrid_search(&cfg, &dense, &sparse, 5, None).await;

// Before:
let result = qdrant_named_dense_search(&cfg, &dense, 5).await;
// After:
let result = qdrant_named_dense_search(&cfg, &dense, 5, None).await;
```

Also add a new test asserting the filter IS forwarded when `Some`:

```rust
#[tokio::test]
async fn qdrant_hybrid_search_includes_filter_when_some() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/collections/test_col/points/query")
                .json_body_includes(r#""scraped_at""#)
                .json_body_includes(r#""gte""#);
            then.status(200)
                .json_body(make_search_response(vec![("https://example.com/a", 0.9)]));
        })
        .await;

    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.qdrant_url = server.base_url();
    cfg.collection = "test_col".to_string();

    let dense = vec![0.1f32, 0.2, 0.3, 0.4];
    let sparse = compute_sparse_vector("hybrid search test");
    let filter = serde_json::json!({
        "must": [{"key": "scraped_at", "range": {"gte": "2026-01-01T00:00:00+00:00"}}]
    });
    let result = qdrant_hybrid_search(&cfg, &dense, &sparse, 5, Some(&filter)).await;

    mock.assert_async().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn qdrant_named_dense_search_includes_filter_when_some() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/collections/test_col/points/query")
                .json_body_includes(r#""scraped_at""#);
            then.status(200)
                .json_body(make_search_response(vec![("https://example.com/dense", 0.88)]));
        })
        .await;

    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.qdrant_url = server.base_url();
    cfg.collection = "test_col".to_string();

    let filter = serde_json::json!({
        "must": [{"key": "scraped_at", "range": {"gte": "2026-01-01T00:00:00+00:00"}}]
    });
    let result = qdrant_named_dense_search(&cfg, &[0.1f32, 0.2, 0.3, 0.4], 5, Some(&filter)).await;

    mock.assert_async().await;
    assert!(result.is_ok());
}
```

- [ ] **Step 2: Run tests to verify they fail (compile errors on signature mismatch)**

```bash
cargo test --lib qdrant_hybrid_search
```

Expected: FAIL — compiler error, signatures not yet changed

- [ ] **Step 3: Update `qdrant_search` signature in `client.rs`**

Find the function signature and body in `crates/vector/ops/qdrant/client.rs`:

```rust
// Change signature from:
pub(crate) async fn qdrant_search(
    cfg: &Config,
    vector: &[f32],
    limit: usize,
) -> Result<Vec<QdrantSearchHit>> {

// To:
pub(crate) async fn qdrant_search(
    cfg: &Config,
    vector: &[f32],
    limit: usize,
    filter: Option<&serde_json::Value>,
) -> Result<Vec<QdrantSearchHit>> {
```

Inside the function, replace the hardcoded json body with:

```rust
let mut body = serde_json::json!({
    "vector": vector,
    "limit": limit,
    "with_payload": true,
    "with_vector": false
});
if let Some(f) = filter {
    body["filter"] = f.clone();
}
```

Then pass `body` (not the literal) to `.json(&body)`.

- [ ] **Step 4: Update `qdrant_hybrid_search` and `qdrant_named_dense_search` in `hybrid.rs`**

For `qdrant_hybrid_search`, change signature and add filter at top level:

```rust
pub(crate) async fn qdrant_hybrid_search(
    cfg: &Config,
    dense_vector: &[f32],
    sparse_vector: &SparseVector,
    limit: usize,
    filter: Option<&serde_json::Value>,
) -> Result<Vec<QdrantSearchHit>> {
    // ...
    let mut body = serde_json::json!({
        "prefetch": [
            { "query": dense_vector, "using": "dense", "limit": candidates },
            { "query": sparse_vector.to_json(), "using": "bm42", "limit": candidates }
        ],
        "query": {"fusion": "rrf"},
        "limit": limit,
        "with_payload": true,
        "with_vector": false
    });
    if let Some(f) = filter {
        body["filter"] = f.clone();
    }
    // ...
}
```

For `qdrant_named_dense_search`:

```rust
pub(crate) async fn qdrant_named_dense_search(
    cfg: &Config,
    dense_vector: &[f32],
    limit: usize,
    filter: Option<&serde_json::Value>,
) -> Result<Vec<QdrantSearchHit>> {
    // ...
    let mut body = serde_json::json!({
        "query": dense_vector,
        "using": "dense",
        "limit": limit,
        "with_payload": true,
        "with_vector": false
    });
    if let Some(f) = filter {
        body["filter"] = f.clone();
    }
    // ...
}
```

- [ ] **Step 5: Update all internal callers that don't yet pass `None`**

`cargo check` will identify them. Common locations to check:
- `crates/vector/ops/commands/query.rs` — `dispatch_search` calls all three
- `crates/vector/ops/commands/ask/context/retrieval.rs` — `dispatch_ask_search` calls all three

For now, just add `, None` to each call site to make it compile. The filter wiring comes in Task 5.

Also update the re-export in `crates/vector/ops/qdrant.rs` — since signatures changed, re-exports are still valid (just function pointers update automatically).

- [ ] **Step 6: Run all qdrant tests**

```bash
cargo test --lib qdrant
```

Expected: all existing tests PASS, two new filter tests PASS

- [ ] **Step 7: Commit**

```bash
git add crates/vector/ops/qdrant/client.rs crates/vector/ops/qdrant/hybrid.rs
git commit -m "feat(vector): thread filter param through qdrant_search, hybrid_search, named_dense_search"
```

---

## Chunk 5: Wire filter at call sites

### Task 5: Build filter from `Config` in `query.rs` and `retrieval.rs`

**Files:**
- Modify: `crates/vector/ops/commands/query.rs`
- Modify: `crates/vector/ops/commands/ask/context/retrieval.rs`

Both files have a `dispatch_*_search` helper that calls the three Qdrant search functions. Replace the `None` placeholders added in Task 4 with a real filter built from `cfg.since` / `cfg.before`.

- [ ] **Step 1: Update `query.rs`**

In `dispatch_search`, build the filter from config before calling search:

```rust
async fn dispatch_search(
    cfg: &Config,
    vector: &[f32],
    query: &str,
    limit: usize,
) -> Result<Vec<qdrant::QdrantSearchHit>, Box<dyn Error>> {
    let filter = qdrant::build_scraped_at_filter(cfg.since.as_deref(), cfg.before.as_deref());
    let filter_ref = filter.as_ref();
    let mode = get_or_fetch_vector_mode(cfg).await?;
    match mode {
        VectorMode::Named => {
            let sv = sparse::compute_sparse_vector(query);
            if cfg.hybrid_search_enabled && !sv.is_empty() {
                qdrant::qdrant_hybrid_search(cfg, vector, &sv, limit, filter_ref)
                    .await
                    .map_err(|e| -> Box<dyn Error> { e.to_string().into() })
            } else {
                qdrant::qdrant_named_dense_search(cfg, vector, limit, filter_ref)
                    .await
                    .map_err(|e| -> Box<dyn Error> { e.to_string().into() })
            }
        }
        VectorMode::Unnamed => qdrant::qdrant_search(cfg, vector, limit, filter_ref)
            .await
            .map_err(|e| -> Box<dyn Error> { e.to_string().into() }),
    }
}
```

- [ ] **Step 2: Update `retrieval.rs`**

Same pattern in `dispatch_ask_search`:

```rust
async fn dispatch_ask_search(
    cfg: &Config,
    vector: &[f32],
    query: &str,
    limit: usize,
) -> Result<Vec<qdrant::QdrantSearchHit>> {
    let filter = qdrant::build_scraped_at_filter(cfg.since.as_deref(), cfg.before.as_deref());
    let filter_ref = filter.as_ref();
    let mode = get_or_fetch_vector_mode(cfg)
        .await
        .map_err(|e| anyhow!(e.to_string()))?;
    match mode {
        VectorMode::Named => {
            let sv = sparse::compute_sparse_vector(query);
            if cfg.hybrid_search_enabled && !sv.is_empty() {
                qdrant::qdrant_hybrid_search(cfg, vector, &sv, limit, filter_ref).await
            } else {
                qdrant::qdrant_named_dense_search(cfg, vector, limit, filter_ref).await
            }
        }
        VectorMode::Unnamed => qdrant::qdrant_search(cfg, vector, limit, filter_ref)
            .await
            .map_err(|e| anyhow!(e.to_string())),
    }
}
```

- [ ] **Step 3: cargo check**

```bash
cargo check
```

Expected: clean

- [ ] **Step 4: Run full test suite**

```bash
cargo test --lib
```

Expected: all existing tests pass, no regressions

- [ ] **Step 5: cargo clippy**

```bash
cargo clippy
```

Expected: clean (fix any warnings before committing)

- [ ] **Step 6: Commit**

```bash
git add crates/vector/ops/commands/query.rs \
        crates/vector/ops/commands/ask/context/retrieval.rs
git commit -m "feat(vector): wire --since/--before filter into query and ask dispatch"
```

---

## Chunk 6: Verification

### Task 6: End-to-end smoke test and final gate

- [ ] **Step 1: Full test suite**

```bash
cargo test --lib
```

Expected: all tests pass

- [ ] **Step 2: Lint gate**

```bash
cargo clippy && cargo fmt --check
```

Expected: clean

- [ ] **Step 3: Manual smoke test (requires running services)**

```bash
# No filter — baseline
./scripts/axon query "rust async patterns" --limit 3

# Since 7 days
./scripts/axon query "rust async patterns" --since 7d --limit 3

# Specific date
./scripts/axon query "rust async patterns" --since 2026-01-01 --limit 3

# Both bounds
./scripts/axon query "rust async patterns" --since 2026-01-01 --before 2026-03-01 --limit 3

# ask also gets it for free
./scripts/axon ask "what is RRF?" --since 7d
```

Expected:
- No filter: returns results regardless of age
- `--since 7d`: returns only content indexed in last 7 days (fewer results expected on a real collection)
- Invalid date: warning logged, query proceeds without filter (graceful degradation)

- [ ] **Step 4: Final commit if any cleanup needed**

```bash
git add -p
git commit -m "chore: cleanup temporal search implementation"
```

---

## Notes

### Graceful degradation
`build_scraped_at_filter` logs a warning and returns `None` when a date string fails to parse. The query proceeds without a filter rather than failing. This is intentional — a misconfigured `--since` should degrade gracefully, not break the command.

### Existing collections without datetime index
`ensure_payload_indexes` is called every time `ensure_collection` runs (on each embed). The `datetime` index PUT is idempotent (HTTP 200 on existing index). Existing collections get the index registered on the next embed job — until then, range queries on `scraped_at` still work but do a full payload scan instead of using the index. Not a correctness issue.

### evaluate is covered for free
`evaluate.rs` calls `build_ask_context` → `retrieve_ask_candidates` → `dispatch_ask_search`. Once `retrieval.rs` is updated (Task 5), evaluate automatically honours `--since` / `--before` with no additional changes.

### Future optimization: pre-filter inside prefetch arms
Currently the filter is applied at the top-level `/query` body (post-fusion). For large collections where most content is filtered out, it's more efficient to push the filter into each prefetch arm:
```json
"prefetch": [
  { "query": ..., "using": "dense", "filter": {...}, "limit": N },
  { "query": ..., "using": "bm42",  "filter": {...}, "limit": N }
]
```
This is a follow-up optimization — correct and useful, but not needed for the initial implementation.
