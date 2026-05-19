# Smart Artifact Responses Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the blunt `clip_inline_json` + `AXON_MCP_DEFAULT_RESPONSE_MODE` approach with per-action inline hints and a unified server-centric artifact access model where all clients — local and remote — use `artifacts.*` subactions via `relative_path`.

**Architecture:** Add an `InlineHint` type to `respond_with_mode` that lets each action declare which top-level fields are always inlined (e.g., `answer` for `ask`) vs. always artifact-only (e.g., `content` for `scrape`). Fix `clip_inline_json` to truncate at structural boundaries (complete array items, field-level string heads) instead of producing raw partial JSON inside a string wrapper.

**Tech Stack:** Rust, `serde_json`, `crates/mcp/server/artifacts/` module

---

## Background: Why This Change

### The three broken behaviors today

**1. `clip_inline_json` produces malformed output**
When payload exceeds 12 000 chars, `shape.rs:17-19` serializes it to a raw string, char-clips at byte position 12 000 (mid-object, mid-array — anywhere), then wraps it:
```json
{ "clipped_json": "{ \"query\": \"what is X\", \"ans" }
```
The LLM gets a partial JSON string inside an object wrapper. This is worse than no inline at all.

**2. `json_shape_preview` is blind to text-dominant responses**
`shape.rs:71` — any string >100 chars becomes `"<string N>"`. For `ask`, the `answer` field (the entire reason for the call) becomes `"<string 847>"` in path mode. Completely useless.

**3. `AXON_MCP_DEFAULT_RESPONSE_MODE=both` is the wrong fix**
Introduced last session as a remote deployment workaround. It fires on ALL large payloads — a `crawl list` response, a `sources` list, a `stats` dump — all get 12K of truncated (and malformed) JSON in context. The action semantics don't matter; the env var doesn't care.

**4. The local/remote distinction is a false dichotomy**
The assumption that local clients can just open the absolute `path` directly is wrong — it requires the client to have read access to the server's filesystem, which isn't guaranteed even locally (different users, containers, remote stdio). The unified model is: the artifact lives on the server, `relative_path` is the stable identifier, and `artifacts.*` subactions are the universal access mechanism for all clients. The absolute `path` field remains in the metadata for transparency but no client should depend on it.

### The correct model

Different actions have different primary values:

| Action | Primary value | Right behavior |
|--------|--------------|----------------|
| `ask` | `answer` string | Always inline `answer`, artifact holds rest |
| `research` | `summary` string | Always inline `summary`, artifact holds rest |
| `query` | result array | Shape sufficient, artifacts.head/grep for detail |
| `map` | url array | Shape sufficient (count visible) |
| `scrape` | scraped content | Always path — too large, use artifacts.head |
| `retrieve` | document content | Always path — always large, use artifacts.head |
| `sources`, `domains`, `stats` | structured lists | Shape sufficient |

---

## File Map

| File | Change |
|------|--------|
| `crates/mcp/server/artifacts/shape.rs` | Fix `clip_inline_json`; improve array shape preview |
| `crates/mcp/server/artifacts/respond.rs` | Add `InlineHint`; update `respond_with_mode` signature; remove `server_default_response_mode` |
| `crates/mcp/server/common.rs` | Re-export `InlineHint` |
| `crates/mcp/server/handlers_query.rs` | Wire `InlineHint` per action |
| `crates/mcp/server/handlers_system.rs` | Wire `InlineHint::Default` for system actions |
| `crates/mcp/server/handlers_crawl_extract.rs` | Pass `InlineHint::Default` to satisfy new signature |
| `crates/mcp/server/handlers_embed_ingest.rs` | Pass `InlineHint::Default` to satisfy new signature |
| `crates/mcp/server/handlers_refresh_status.rs` | Pass `InlineHint::Default` to satisfy new signature |
| `crates/mcp/server/handlers_graph.rs` | Pass `InlineHint::Default` to satisfy new signature |
| `crates/mcp/server/handlers_system/screenshot.rs` | Pass `InlineHint::Default` to satisfy new signature |
| `docs/MCP-TOOL-SCHEMA.md` | Document new response behavior |

---

## Task 1: Fix `clip_inline_json` — Structural Truncation

**Files:**
- Modify: `crates/mcp/server/artifacts/shape.rs`

This is the highest-priority fix. `clip_inline_json` currently produces malformed output. Replace it with a structural truncator that always emits valid JSON.

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block in `shape.rs`:

```rust
#[test]
fn clip_inline_json_array_truncates_at_item_boundaries() {
    // Build an array where complete items fit within budget but all items don't
    let items: Vec<_> = (0..5)
        .map(|i| serde_json::json!({"id": i, "text": "x".repeat(200)}))
        .collect();
    let val = serde_json::Value::Array(items);
    let (clipped, truncated) = clip_inline_json(&val, 600);
    assert!(truncated, "should be truncated");
    let arr = clipped.as_array().expect("must be array");
    // Last item must be the truncation marker
    let last = arr.last().expect("must have items");
    assert!(last.get("__truncated__").is_some(), "must have truncation marker");
    // All non-marker items must be complete valid objects (not raw strings)
    for item in &arr[..arr.len() - 1] {
        assert!(item.get("id").is_some(), "item must be complete object");
    }
}

#[test]
fn clip_inline_json_object_truncates_long_string_fields() {
    let long_val = "x".repeat(600);
    let val = serde_json::json!({
        "query": "short",
        "answer": long_val,
        "count": 42,
    });
    let (clipped, truncated) = clip_inline_json(&val, 300);
    assert!(truncated, "should be truncated");
    // All keys must be present
    assert!(clipped.get("query").is_some());
    assert!(clipped.get("answer").is_some());
    assert!(clipped.get("count").is_some());
    // Long string value must be a head object, not a raw string
    let answer = &clipped["answer"];
    assert!(answer.is_object(), "long string must become head object");
    assert!(answer.get("__head__").is_some(), "must have __head__ field");
    assert!(answer.get("__total_chars__").is_some(), "must have __total_chars__");
    // Short fields must remain as-is
    assert_eq!(clipped["query"], "short");
    assert_eq!(clipped["count"], 42);
}

#[test]
fn clip_inline_json_does_not_produce_clipped_json_wrapper() {
    // The old behavior: {"clipped_json": "<raw partial string>"}
    // This must NEVER appear in output regardless of input
    let large_obj = serde_json::json!({
        "a": "x".repeat(5000),
        "b": "y".repeat(5000),
        "c": "z".repeat(5000),
    });
    let (clipped, _) = clip_inline_json(&large_obj, 100);
    let serialized = serde_json::to_string(&clipped).unwrap();
    assert!(
        !serialized.contains("clipped_json"),
        "must not produce clipped_json wrapper"
    );
}

#[test]
fn clip_inline_json_small_payload_is_unchanged() {
    let val = serde_json::json!({"key": "value", "n": 42});
    let (clipped, truncated) = clip_inline_json(&val, 10_000);
    assert!(!truncated);
    assert_eq!(clipped, val);
}
```

- [ ] **Step 2: Run tests to confirm failures**

```bash
cargo test clip_inline_json -- --nocapture 2>&1 | tail -20
```

Expected: 3 failures (new tests), 1 pass (`small_payload_is_unchanged`).

- [ ] **Step 3: Replace `clip_inline_json` implementation**

Replace `shape.rs:14-26` with:

```rust
/// Truncate `value` to fit within `max_chars` of serialized JSON,
/// always producing valid JSON (never a raw-string wrapper).
///
/// - Arrays: include complete items until budget exhausted; append a
///   `{"__truncated__": N}` marker for omitted items.
/// - Objects: keep all keys; replace long string values with
///   `{"__head__": "...", "__total_chars__": N}`.
/// - Scalars/other: return as-is (they cannot be partially truncated).
pub fn clip_inline_json(value: &serde_json::Value, max_chars: usize) -> (serde_json::Value, bool) {
    match serde_json::to_string(value) {
        Ok(raw) if raw.chars().count() <= max_chars => (value.clone(), false),
        Ok(_) => match value {
            serde_json::Value::Array(arr) => clip_array(arr, max_chars),
            serde_json::Value::Object(map) => clip_object(map, max_chars),
            other => (other.clone(), false),
        },
        Err(_) => (serde_json::json!({"__error__": "serialization failed"}), true),
    }
}

fn clip_array(arr: &[serde_json::Value], max_chars: usize) -> (serde_json::Value, bool) {
    // Reserve ~30 chars for the truncation marker item.
    let budget = max_chars.saturating_sub(30);
    let mut out: Vec<serde_json::Value> = Vec::new();
    let mut used = 2usize; // "[]"
    for item in arr {
        let s = serde_json::to_string(item).unwrap_or_default();
        let cost = s.chars().count() + if out.is_empty() { 0 } else { 1 }; // comma
        if used + cost > budget {
            break;
        }
        out.push(item.clone());
        used += cost;
    }
    let remaining = arr.len() - out.len();
    if remaining > 0 {
        out.push(serde_json::json!({"__truncated__": remaining}));
        (serde_json::Value::Array(out), true)
    } else {
        (serde_json::Value::Array(out), false)
    }
}

fn clip_object(
    map: &serde_json::Map<String, serde_json::Value>,
    max_chars: usize,
) -> (serde_json::Value, bool) {
    // Per-string-field head length: distribute budget across fields.
    // Simple heuristic: cap each string at max_chars / 4.
    let string_cap = (max_chars / 4).max(200);
    let mut truncated = false;
    let out: serde_json::Map<String, serde_json::Value> = map
        .iter()
        .map(|(k, v)| {
            let v2 = match v {
                serde_json::Value::String(s) if s.chars().count() > string_cap => {
                    truncated = true;
                    let head: String = s.chars().take(string_cap).collect();
                    serde_json::json!({"__head__": head, "__total_chars__": s.chars().count()})
                }
                other => other.clone(),
            };
            (k.clone(), v2)
        })
        .collect();
    (serde_json::Value::Object(out), truncated)
}
```

Note: `clip_array` + `clip_object` are private helpers. `clip_inline_json` stays public. Check monolith line count after edit — `shape.rs` starts at 131 lines; this adds ~65 lines = ~196 total, well within 500.

- [ ] **Step 4: Run tests to confirm they pass**

```bash
cargo test clip_inline_json -- --nocapture
```

Expected: all 4 tests pass.

- [ ] **Step 5: Verify existing respond.rs tests still pass**

```bash
cargo test -p axon_cli -- artifacts 2>&1 | tail -20
```

Expected: all artifact tests pass (they use the existing `Inline`/`Both` mode code that calls `clip_inline_json`).

- [ ] **Step 6: Commit**

```bash
git add crates/mcp/server/artifacts/shape.rs
git commit -m "fix(mcp): clip_inline_json truncates at structural boundaries, not raw char offset"
```

---

## Task 2: Improve Array Shape Preview

**Files:**
- Modify: `crates/mcp/server/artifacts/shape.rs`

`json_shape_preview` for arrays without a status field currently returns `"<array[N]>"` — the count, but no indication of item shape. Add a 2-item sample so the LLM can understand what type of data is in the array.

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block:

```rust
#[test]
fn json_shape_preview_non_status_array_shows_sample_items() {
    let val = serde_json::json!({
        "results": [
            {"url": "https://a.com", "score": 0.95, "title": "A"},
            {"url": "https://b.com", "score": 0.91, "title": "B"},
            {"url": "https://c.com", "score": 0.88, "title": "C"},
        ]
    });
    let preview = json_shape_preview(&val);
    let results = &preview["results"];
    // Must include total and sample, not just "<array[3]>"
    assert_eq!(results["total"], 3);
    let sample = results["sample"].as_array().expect("sample must be array");
    assert_eq!(sample.len(), 2, "sample shows first 2 items");
    // Each sample item is itself shape-previewed (short strings verbatim)
    assert!(sample[0].get("url").is_some());
}

#[test]
fn json_shape_preview_status_array_unchanged() {
    // The existing status_histogram path must not change
    let val = serde_json::json!([
        {"status": "completed"}, {"status": "running"}, {"status": "completed"}
    ]);
    let preview = json_shape_preview(&val);
    assert_eq!(preview["total"], 3);
    assert!(preview.get("by_status").is_some());
    assert!(preview.get("sample").is_none(), "status arrays don't show sample");
}
```

- [ ] **Step 2: Run tests to confirm failure**

```bash
cargo test json_shape_preview -- --nocapture 2>&1 | tail -20
```

Expected: `non_status_array_shows_sample_items` fails.

- [ ] **Step 3: Update `json_shape_preview` for arrays**

Replace the `serde_json::Value::Array(arr)` arm in `json_shape_preview` (currently `shape.rs:64-67`):

```rust
serde_json::Value::Array(arr) => match status_histogram(arr) {
    Some(hist) => serde_json::json!({ "total": arr.len(), "by_status": hist }),
    None => {
        let sample: Vec<_> = arr
            .iter()
            .take(2)
            .map(json_shape_preview)
            .collect();
        serde_json::json!({ "total": arr.len(), "sample": sample })
    }
},
```

- [ ] **Step 4: Update the existing test assertion that checks array shape**

The existing test `json_shape_preview_short_strings_are_verbatim` at `shape.rs:91` currently asserts:
```rust
assert_eq!(preview["items"], "<array[3]>");  // OLD — no longer produced
```

This will fail after the array arm change. Update it:
```rust
// Before (shape.rs:91):
assert_eq!(preview["items"], "<array[3]>");
// After:
assert_eq!(preview["items"]["total"], 3);
assert!(preview["items"]["sample"].is_array());
```

- [ ] **Step 5: Run all shape tests**

```bash
cargo test json_shape_preview -- --nocapture
cargo test status_histogram -- --nocapture
```

Expected: all pass.

- [ ] **Step 5: Run full test suite to check for regressions**

```bash
cargo test --lib 2>&1 | tail -5
```

Expected: 0 failures.

- [ ] **Step 6: Commit**

```bash
git add crates/mcp/server/artifacts/shape.rs
git commit -m "feat(mcp): shape preview shows 2-item sample for non-status arrays"
```

---

## Task 3: Add `InlineHint` and Update `respond_with_mode`

**Files:**
- Modify: `crates/mcp/server/artifacts/respond.rs`

`InlineHint` tells `respond_with_mode` which fields matter to the caller. This replaces `AXON_MCP_DEFAULT_RESPONSE_MODE` as the remote-access solution.

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block in `respond.rs`:

```rust
/// InlineHint::Fields extracts named keys into the path-mode response.
#[tokio::test]
#[allow(unsafe_code)]
#[allow(clippy::await_holding_lock)]
async fn inline_hint_fields_included_in_path_mode_response() {
    let _guard = ARTIFACT_ENV_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (_tmp, prev) = scoped_artifact_root();
    let payload = serde_json::json!({
        "query": "test question",
        "answer": "This is a detailed answer that explains everything.",
        "timing_ms": {"total": 1234},
    });
    let resp = respond_with_mode(
        "ask", "ask",
        None,  // no explicit mode → path (default)
        "ask-test",
        payload,
        InlineHint::Fields(&["answer"]),
    )
    .await
    .unwrap();
    assert!(resp.ok);
    // In path mode with InlineHint::Fields, key_fields must be present
    assert!(resp.data.get("key_fields").is_some(), "key_fields missing");
    assert!(resp.data["key_fields"].get("answer").is_some(), "answer not extracted");
    // Artifact must still be written
    assert!(resp.data.get("artifact").is_some());
    restore_artifact_env(prev);
}

/// InlineHint::AlwaysPath forces path mode regardless of explicit inline request.
#[tokio::test]
#[allow(unsafe_code)]
#[allow(clippy::await_holding_lock)]
async fn inline_hint_always_path_overrides_explicit_inline_mode() {
    let _guard = ARTIFACT_ENV_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (_tmp, prev) = scoped_artifact_root();
    let payload = serde_json::json!({"content": "scraped page content here"});
    let resp = respond_with_mode(
        "scrape", "scrape",
        Some(ResponseMode::Inline),  // caller requested inline — must be ignored
        "scrape-test",
        payload,
        InlineHint::AlwaysPath,
    )
    .await
    .unwrap();
    assert!(resp.ok);
    // Must be path mode response, not inline
    assert_eq!(resp.data["response_mode"], "path");
    assert!(resp.data.get("inline").is_none(), "inline must not be present");
    restore_artifact_env(prev);
}
```

- [ ] **Step 2: Run tests to confirm they fail to compile** (new type doesn't exist yet)

```bash
cargo check --bin axon 2>&1 | grep "InlineHint"
```

Expected: `cannot find type InlineHint`.

- [ ] **Step 3: Add `InlineHint` type and update `respond_with_mode`**

At the top of `respond.rs`, after the `use` block, add:

```rust
/// Controls which fields are always surfaced inline in the MCP response,
/// regardless of response_mode or payload size.
#[derive(Debug, Clone)]
pub enum InlineHint {
    /// Normal auto-inline behavior based on payload size.
    Default,
    /// Extract these top-level fields into `key_fields` in the response.
    /// The full payload is still written to the artifact.
    /// The extracted fields bypass clip_inline_json — they are included verbatim
    /// (or truncated at a generous per-field cap to prevent abuse).
    Fields(&'static [&'static str]),
    /// Never inline. Force path mode regardless of the caller's explicit mode
    /// or AXON_MCP_DEFAULT_RESPONSE_MODE. Use for large document content
    /// (scrape, retrieve) where artifacts.head/grep are the right access pattern.
    AlwaysPath,
}
```

Update the `respond_with_mode` signature to accept `hint: InlineHint`:

```rust
pub async fn respond_with_mode(
    action: &str,
    subaction: &str,
    mode: Option<ResponseMode>,
    artifact_stem: &str,
    payload: serde_json::Value,
    hint: InlineHint,
) -> Result<AxonToolResponse, ErrorData>
```

Update the body. The effective mode resolution block:

```rust
// AlwaysPath overrides everything — don't even check mode or threshold.
if matches!(hint, InlineHint::AlwaysPath) {
    let artifact = write_json_artifact(artifact_stem, &payload).await?;
    let shape = json_shape_preview(&payload);
    return Ok(AxonToolResponse::ok(
        action,
        subaction,
        serde_json::json!({
            "response_mode": "path",
            "shape": shape,
            "artifact": artifact,
        }),
    ));
}
```

Then, before the existing `effective_mode` resolution, handle `InlineHint::Fields`:

```rust
// Fields hint: always write artifact, always extract named fields.
// The mode parameter only affects whether shape or inline is also included.
if let InlineHint::Fields(fields) = &hint {
    let artifact = write_json_artifact(artifact_stem, &payload).await?;
    let key_fields = extract_key_fields(&payload, fields);
    let shape = json_shape_preview(&payload);
    return Ok(AxonToolResponse::ok(
        action,
        subaction,
        serde_json::json!({
            "response_mode": "path",
            "key_fields": key_fields,
            "shape": shape,
            "artifact": artifact,
        }),
    ));
}
```

Add private helper (keep under 50 lines, under 120 per function limit):

```rust
/// Extract named top-level fields from `payload` into a new object.
/// String values are capped at 32 000 chars to prevent abuse.
/// Missing keys are silently omitted.
fn extract_key_fields(
    payload: &serde_json::Value,
    fields: &[&'static str],
) -> serde_json::Value {
    const STRING_CAP: usize = 32_000;
    let mut out = serde_json::Map::new();
    if let serde_json::Value::Object(map) = payload {
        for &field in fields {
            if let Some(v) = map.get(field) {
                let capped = match v {
                    serde_json::Value::String(s) if s.chars().count() > STRING_CAP => {
                        let head: String = s.chars().take(STRING_CAP).collect();
                        serde_json::Value::String(head)
                    }
                    other => other.clone(),
                };
                out.insert(field.to_string(), capped);
            }
        }
    }
    serde_json::Value::Object(out)
}
```

- [ ] **Step 4: Update all existing `respond_with_mode` tests to pass `InlineHint::Default`**

There are 4 existing tests in `respond.rs`. Each `respond_with_mode(...)` call needs `InlineHint::Default` appended as the 6th argument. Example — `auto_inline_when_mode_is_none_and_payload_small`:

```rust
// Before:
let resp = respond_with_mode("test", "sub", None, "test-artifact", payload.clone())
    .await
    .unwrap();
// After:
let resp = respond_with_mode("test", "sub", None, "test-artifact", payload.clone(), InlineHint::Default)
    .await
    .unwrap();
```

Apply the same pattern to the other three tests (`explicit_path_mode_respected_even_for_small_payload`, `explicit_inline_mode_returns_inline_data`, `both_mode_returns_inline_and_shape_and_artifact`).

- [ ] **Step 5: Remove `server_default_response_mode`**

Delete `respond.rs:158-168` (the `fn server_default_response_mode()` function). Replace `server_default_response_mode()` call at line `93` with `ResponseMode::Path` directly.

The `AXON_MCP_DEFAULT_RESPONSE_MODE` env var is removed — the `InlineHint::Fields` pattern makes it unnecessary for the actions where it was needed (`ask`, `research`), and `AlwaysPath` makes it irrelevant for document content.

- [ ] **Step 6: Verify compilation**

```bash
cargo check --bin axon 2>&1 | grep -E "error|warning" | head -20
```

Expected: compile errors for all call sites that don't yet pass `hint`. That's OK — fix in Tasks 4 and 5.

- [ ] **Step 7: Run the respond tests**

```bash
cargo test -p axon_cli -- respond 2>&1 | tail -10
```

Expected: all tests in `respond.rs` pass.

- [ ] **Step 8: Commit (after Tasks 4+5 compile)**

Hold this commit — combine with Task 4.

---

## Task 4: Re-export `InlineHint` and Fix All Call Sites — Default Hint

**Files:**
- Modify: `crates/mcp/server/common.rs`
- Modify: `crates/mcp/server/handlers_crawl_extract.rs`
- Modify: `crates/mcp/server/handlers_embed_ingest.rs`
- Modify: `crates/mcp/server/handlers_refresh_status.rs`
- Modify: `crates/mcp/server/handlers_system.rs`

These handlers don't need action-specific hints. All their `respond_with_mode` calls get `InlineHint::Default`.

- [ ] **Step 1: Re-export `InlineHint` from `common.rs`**

`common.rs` already re-exports `respond_with_mode`:
```rust
pub(super) use super::artifacts::respond_with_mode;
```

Add below it:
```rust
pub(super) use super::artifacts::InlineHint;
```

- [ ] **Step 2: Fix all `respond_with_mode` calls in the non-query handlers**

Search for all calls:
```bash
grep -n "respond_with_mode" \
  crates/mcp/server/handlers_crawl_extract.rs \
  crates/mcp/server/handlers_embed_ingest.rs \
  crates/mcp/server/handlers_refresh_status.rs \
  crates/mcp/server/handlers_system.rs
```

For every call that currently ends with `)`, add `, InlineHint::Default` before the closing `)`:

Pattern: `respond_with_mode(a, b, c, d, e).await` → `respond_with_mode(a, b, c, d, e, InlineHint::Default).await`

**`handlers_system.rs`** — calls to update: `list`, `search`, `clean`-routed calls in `handle_artifacts`; `handle_help`; `handle_doctor`; `handle_domains`; `handle_sources`; `handle_stats`; `handle_export`.

**`handlers_crawl_extract.rs`**, **`handlers_embed_ingest.rs`**, **`handlers_refresh_status.rs`** — check each handler for `respond_with_mode` calls and add `InlineHint::Default`.

**`handlers_graph.rs`** — 4 calls in `handle_graph` (`build`, `status`, `explore`, `stats`). Imports `respond_with_mode` from `common` — add `InlineHint` to the same import. Add `InlineHint::Default` to all 4 calls.

**`handlers_system/screenshot.rs`** — 1 call in the screenshot handler. This file imports `respond_with_mode` directly from `artifacts`. Add `InlineHint` to the import and `InlineHint::Default` to the call. The screenshot submodule is in `crates/mcp/server/handlers_system/screenshot.rs` — confirm with:
```bash
grep -n "respond_with_mode" crates/mcp/server/handlers_system/screenshot.rs
```

- [ ] **Step 3: Verify compilation**

```bash
cargo check --bin axon 2>&1 | grep "error" | grep -v "handlers_query" | head -20
```

Expected: zero errors outside of `handlers_query.rs` (Task 5 handles those).

- [ ] **Step 4: Run tests**

```bash
cargo test --lib 2>&1 | tail -5
```

Expected: 0 failures.

- [ ] **Step 5: Commit Tasks 3+4 together**

```bash
git add \
  crates/mcp/server/artifacts/respond.rs \
  crates/mcp/server/common.rs \
  crates/mcp/server/handlers_crawl_extract.rs \
  crates/mcp/server/handlers_embed_ingest.rs \
  crates/mcp/server/handlers_refresh_status.rs \
  crates/mcp/server/handlers_system.rs
git commit -m "feat(mcp): add InlineHint to respond_with_mode; remove AXON_MCP_DEFAULT_RESPONSE_MODE"
```

---

## Task 5: Wire Per-Action Hints in Query Handlers

**Files:**
- Modify: `crates/mcp/server/handlers_query.rs`

This is the payoff task. `ask` and `research` always inline their answer fields; `scrape` and `retrieve` are always path-only.

- [ ] **Step 1: Update `use` imports**

`handlers_query.rs` currently imports `respond_with_mode` from `common`:
```rust
use super::common::{
    ..., respond_with_mode, ...
};
```

Add `InlineHint` to that import list.

- [ ] **Step 2: Wire `InlineHint::Fields` for `ask`**

In `handle_ask` (`handlers_query.rs:261`), the `respond_with_mode` call is at the end of the function. Change:

```rust
respond_with_mode(
    "ask", "ask",
    response_mode,
    &format!("ask-{}", slugify(&query, 56)),
    result.payload,
)
.await
```

to:

```rust
respond_with_mode(
    "ask", "ask",
    response_mode,
    &format!("ask-{}", slugify(&query, 56)),
    result.payload,
    InlineHint::Fields(&["answer"]),
)
.await
```

- [ ] **Step 3: Wire `InlineHint::Fields` for `research`**

The research payload (from `crates/services/search.rs:190-200`) has `"summary"` as the LLM-generated field. In `handle_research` (`handlers_query.rs:224`), change the `respond_with_mode` call similarly:

```rust
respond_with_mode(
    "research", "research",
    response_mode,
    &format!("research-{}", slugify(&query, 56)),
    result.payload,
    InlineHint::Fields(&["summary"]),
)
.await
```

- [ ] **Step 4: Wire `InlineHint::AlwaysPath` for `scrape`**

`handle_scrape` (`handlers_query.rs:184`). The scraped payload can be megabytes of page content. Change:

```rust
respond_with_mode(
    "scrape", "scrape",
    response_mode,
    &format!("scrape-{}", slugify(&url, 56)),
    result.payload,
    InlineHint::AlwaysPath,
)
.await
```

- [ ] **Step 5: Wire `InlineHint::AlwaysPath` for `retrieve`**

`handle_retrieve` (`handlers_query.rs:59`). Document retrieval returns full document text.

```rust
respond_with_mode(
    "retrieve", "retrieve",
    response_mode,
    &format!("retrieve-{}", slugify(&target, 56)),
    serde_json::json!({
        "url": target,
        "chunks": chunk_count,
        "content": content,
    }),
    InlineHint::AlwaysPath,
)
.await
```

- [ ] **Step 6: Wire `InlineHint::Default` for remaining actions**

`handle_query`, `handle_map`, `handle_search` — all get `InlineHint::Default`. These return structured lists where shape is sufficient.

- [ ] **Step 7: Verify full compilation**

```bash
cargo check --bin axon 2>&1 | grep "error" | head -20
```

Expected: 0 errors.

- [ ] **Step 8: Run the full test suite**

```bash
cargo test --lib 2>&1 | tail -10
```

Expected: 0 failures.

- [ ] **Step 9: Run clippy**

```bash
cargo clippy -- -D warnings 2>&1 | grep "error" | head -10
```

Expected: 0 errors.

- [ ] **Step 10: Commit**

```bash
git add crates/mcp/server/handlers_query.rs
git commit -m "feat(mcp): wire per-action InlineHint — ask/research inline answer, scrape/retrieve always path"
```

---

## Task 6: Remove `AXON_MCP_DEFAULT_RESPONSE_MODE` from Documentation and `.env.example`

**Files:**
- Modify: `docs/MCP-TOOL-SCHEMA.md`
- Modify: `docs/MCP.md` (if it mentions the env var)
- Modify: `.env.example` (if it lists `AXON_MCP_DEFAULT_RESPONSE_MODE`)

- [ ] **Step 1: Search for references**

```bash
grep -rn "AXON_MCP_DEFAULT_RESPONSE_MODE" docs/ .env.example 2>/dev/null
```

- [ ] **Step 2: Remove or replace each reference**

For each occurrence, replace with a note about the new behavior:
> `ask` and `research` responses always include `key_fields.answer` / `key_fields.summary` in path mode. `scrape` and `retrieve` are always path-only. All artifact access uses `artifacts.*` subactions with `relative_path` — there is no distinction between local and remote clients.

- [ ] **Step 3: Update response contract documentation**

In `docs/MCP-TOOL-SCHEMA.md`, update the response behavior section to document:

1. **Unified artifact access model** — All clients (local stdio and remote HTTP) access artifact content via `artifacts.*` subactions using the `relative_path` field. The absolute `path` field is present for transparency/debugging only. No client should open it directly.
2. `key_fields` — present in path-mode responses for `ask` and `research`; contains the answer/summary field without clipping
3. Updated `shape` — arrays now show `{ "total": N, "sample": [...] }` instead of `"<array[N]>"`
4. `clip_inline_json` behavior — when `inline` or `both` mode is used and payload exceeds 12K chars, arrays truncate at item boundaries (with `{"__truncated__": N}` marker) and objects truncate long string fields (with `{"__head__": "...", "__total_chars__": N}`)
5. `AlwaysPath` behavior — `scrape` and `retrieve` ignore the `response_mode` field and always return path mode

Update `docs/MCP.md` artifact section to state: **The artifact cache is server-centric. Use `artifacts.head`, `artifacts.grep`, or `artifacts.read` with `relative_path` to access content. Do not depend on the absolute `path` field.**

- [ ] **Step 4: Commit**

```bash
git add docs/MCP-TOOL-SCHEMA.md docs/MCP.md .env.example
git commit -m "docs(mcp): document InlineHint response behavior; remove AXON_MCP_DEFAULT_RESPONSE_MODE"
```

---

## Task 7: Final Verification

- [ ] **Step 1: Run the monolith check**

```bash
just precommit 2>&1 | tail -20
```

Expected: passes. `shape.rs` should be under 500 lines (was 131 + ~90 new = ~221). `respond.rs` was 297 + ~50 new - 10 removed = ~337.

- [ ] **Step 2: Run the full test suite with output**

```bash
cargo test --lib -- --nocapture 2>&1 | grep -E "FAILED|test result" | tail -5
```

Expected: `test result: ok. N passed; 0 failed`.

- [ ] **Step 3: Smoke test `ask` via MCP — key_fields and artifact access**

```bash
cargo build --bin axon 2>&1 | tail -3
# In one terminal: ./target/debug/axon mcp
# In another:
mcporter call axon.axon action:ask query:"what is hybrid search?"
```

Verify:
- Response includes `key_fields.answer` (actual text, not `"<string N>"`)
- Response includes `artifact.relative_path` (e.g. `ask/what-is-hybrid-search.json`)
- Read the artifact via subaction: `mcporter call axon.axon action:artifacts subaction:head path:"ask/what-is-hybrid-search.json"`
- Confirm head returns full payload content using `relative_path`, not the absolute `path`

- [ ] **Step 4: Smoke test `scrape` via MCP — AlwaysPath and relative_path access**

```bash
mcporter call axon.axon action:scrape url:"https://example.com" response_mode:inline
```

Verify response is path mode despite `inline` request (`AlwaysPath` overrides). Then access content via:
```bash
mcporter call axon.axon action:artifacts subaction:head path:"<relative_path from response>"
```

Verify this works without needing the absolute `path`.

- [ ] **Step 5: Final commit if any tweaks needed**

```bash
git add -A
git commit -m "chore(mcp): final verification fixes"
```

---

## Summary of Behavior After This Plan

| Before | After |
|--------|-------|
| `ask` in path mode → `answer: "<string 847>"` in shape | `ask` always has `key_fields.answer` (full text, up to 32K) |
| `clip_inline_json` produces `{"clipped_json": "<partial json>"}` | Structural truncation: complete items + markers |
| `AXON_MCP_DEFAULT_RESPONSE_MODE=both` needed for remote `ask` | Removed — InlineHint::Fields handles it properly |
| `scrape` with `response_mode:inline` blasts page content into context | Ignored — `AlwaysPath` enforces path mode |
| Array shape: `"<array[20]>"` | Array shape: `{"total": 20, "sample": [{...}, {...}]}` |
| Local clients open absolute `path` directly | All clients (local + remote) use `artifacts.*` + `relative_path` |
| Two access models: local filesystem vs remote artifacts subactions | One model: server-centric cache, `artifacts.*` for everyone |
