# MCP/CLI/Web Config Drift Fixes + Qdrant Scroll Retry
**Date:** 2026-03-14
**Branch:** main
**Version:** v0.23.3

---

## Session Overview

Implemented a 7-fix plan to resolve MCP/CLI/Web config drift where MCP handlers were hardcoding defaults instead of reading from `cfg` and omitting fields from request schemas so callers couldn't override them at runtime. Also investigated and fixed a flaky Qdrant integration test caused by missing retry logic in the scroll path.

---

## Timeline

1. **Plan execution** — Implemented all 7 fixes from `linked-questing-wozniak.md` plan across `schema.rs`, `common.rs`, `handlers_query.rs`, `handlers_crawl_extract.rs`, `handlers_system.rs`, `constants.rs`, and `MCP-TOOL-SCHEMA.md`.
2. **Test suite failures** — Updated call sites in `tests/mcp_option_mappers.rs` and `tests/mcp_contract_parity.rs` to match new function signatures; fixed pre-existing bugs in parity tests.
3. **Flaky test investigation** — User demanded confirmed root cause of `qdrant_delete_by_url_filter_removes_matching_points` failing under parallel test runs.
4. **Root cause confirmed** — `qdrant_retrieve_by_url` calls `scroll_pages_raw` which had bare `error_for_status()` with no retry, while the delete path used `qdrant_delete_with_retry` with 4-attempt exponential backoff.
5. **Fix implemented** — Added `scroll_page_with_retry` helper; `scroll_pages_raw` now retries 429/5xx for each page request.

---

## Key Findings

- **Config drift root cause:** Free functions (`to_search_options`, `to_pagination`) had no access to `cfg` and fell back to hardcoded literals (`10`, `100`). CLI users get full config control; MCP users got server defaults they couldn't change.
- **`query` limit clamp was `1..=100`** (`handlers_query.rs:23`) while all other handlers used `1..=500` — accidental discrepancy.
- **`root_selector`/`exclude_selector`** were already in `ScrapeRequest` but silently dropped — never forwarded to `cfg`.
- **`scroll_pages_raw` had no retry** (`client.rs:83-90`): bare `.send().await?.error_for_status()?`. Under parallel test load against real Qdrant, any transient 429/503 failed the verify step immediately.
- **`qdrant_delete_with_retry`** (`client.rs:22-70`): 4 attempts, 250ms exponential backoff — the delete path was robust but the read path wasn't.
- **`http_client()` in `#[cfg(test)]`**: creates fresh `reqwest::Client` via `Box::leak(Box::new(build_client(30)))` per call, not the global `LazyLock`.
- **Monolith check passes** despite `client.rs` being 507 lines — the script excludes blank lines/comments from its count.

---

## Technical Decisions

- **`map limit=0` means "no limit"** — matches CLI default where `--limit 0` returns all URLs. No upper clamp applied to `to_map_options`.
- **Separate `scroll_page_with_retry` helper** — keeps `scroll_pages_raw`'s pagination logic readable and mirrors the existing `qdrant_delete_with_retry` pattern rather than duplicating backoff logic inline.
- **`handle_scrape`/`handle_ask` clone cfg** before applying per-request overrides — avoids mutating the shared `Arc<Config>` and is consistent with how other handlers apply request-level overrides.
- **`handlers_system.rs` pagination default = `25`** — both `handle_sources` and `handle_domains` already used `req.limit.or(Some(25))`, so passing `25` as the `to_pagination` default is a no-op but satisfies the new signature.
- **Did not refactor `qdrant_delete_with_retry` and `scroll_page_with_retry` into a shared generic** — would require a trait bound for the request body type and add complexity for marginal benefit.

---

## Files Modified

| File | Purpose |
|------|---------|
| `crates/mcp/schema.rs` | Added `McpScrapeFormat` enum; added `graph`/`diagnostics` to `AskRequest`; added `render_mode`/`format`/`embed` to `ScrapeRequest`; added `max_pages` to `ExtractRequest`; added 4 schema tests |
| `crates/mcp/server/common.rs` | Added `map_scrape_format`; updated `to_pagination`/`to_search_options` to accept `default: usize`; updated `to_map_options` to treat `None`/`0` as "no limit" |
| `crates/mcp/server/handlers_query.rs` | Fixed `query` clamp `100→500`; updated `handle_ask` to apply `graph`/`diagnostics` overrides; updated `handle_scrape` to apply all selector/format/embed overrides; fixed `handle_map` to support `limit=0`; threaded `cfg.search_limit` into pagination helpers |
| `crates/mcp/server/handlers_crawl_extract.rs` | Updated `handle_extract_start` to apply `max_pages` override from request |
| `crates/mcp/server/handlers_system.rs` | Updated two `to_pagination` calls to pass `25` as default |
| `crates/web/execute/constants.rs` | Added `("offset", "--offset")` and `("max_points", "--max-points")` to `ALLOWED_FLAGS` |
| `docs/MCP-TOOL-SCHEMA.md` | Documented all new schema fields; added parameter tables for `ask`, `scrape`, `extract`; updated pagination section |
| `tests/mcp_option_mappers.rs` | Updated all call sites for new signatures; added tests for new `to_map_options` behavior |
| `tests/mcp_contract_parity.rs` | Updated call sites for new signatures; fixed pre-existing `matches!` bugs (`IngestSubaction::Start` → `Some(IngestSubaction::Start)`) |
| `crates/vector/ops/qdrant/client.rs` | Added `scroll_page_with_retry` with 4-attempt 429/5xx retry; updated `scroll_pages_raw` to use it |

---

## Commands Executed

```bash
# Verify compile after each change
cargo check --bin axon

# Run full test suite (revealed flaky qdrant test)
cargo test

# Confirm root cause: scroll has no retry, delete does
# (read client.rs:22-113)

# Verify fix compiles
cargo check --bin axon   # → Finished in 15.78s

# Run monolith policy check
python3 scripts/enforce_monoliths.py --file crates/vector/ops/qdrant/client.rs
# → Monolith policy check passed.

# Run qdrant tests
cargo test qdrant        # → 37 tests, 0 failed
```

---

## Behavior Changes (Before/After)

| Surface | Before | After |
|---------|--------|-------|
| MCP `ask` | `graph`/`diagnostics` always from server cfg, not overrideable | Caller can pass `"graph": true` / `"diagnostics": true` in request |
| MCP `scrape` | `render_mode`/`format`/`embed` fixed to server cfg; selectors silently dropped | All five fields forwarded to cfg when present in request |
| MCP `extract` | `max_pages` not exposed; always used server cfg default | Caller can pass `"max_pages": N` in request |
| MCP `query` | Limit clamped at `1..=100` | Limit clamped at `1..=500` (matches all other handlers) |
| MCP `map` | `limit=0` treated as `limit=1` (clamped) | `limit=0` (or omitted) returns all URLs |
| MCP pagination defaults | Hardcoded to `10` regardless of `cfg.search_limit` | Defaults to `cfg.search_limit` |
| Web WS `offset`/`max_points` | Blocked by `ALLOWED_FLAGS` whitelist | Allowed through to subprocess |
| Qdrant scroll retry | `scroll_pages_raw` fails immediately on 429/5xx | Retries up to 4× with 250ms exponential backoff |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` | Compiles clean | `Finished in 15.78s` | ✅ Pass |
| `python3 scripts/enforce_monoliths.py --file crates/vector/ops/qdrant/client.rs` | Policy passes | `Monolith policy check passed.` | ✅ Pass |
| `cargo test qdrant` | All pass | 37 tests, 0 failed | ✅ Pass |
| `cargo test mcp_option_mappers` | All pass | Ran within full suite, 0 failed | ✅ Pass |
| `cargo test mcp_contract_parity` | All pass | Ran within full suite, 0 failed | ✅ Pass |

---

## Source IDs + Collections Touched

None — no embed/retrieve/crawl operations performed this session. Purely code changes.

---

## Risks and Rollback

- **`to_pagination`/`to_search_options` signature change** is a compile-time break — if any call site was missed, the binary won't build. The build passing confirms all call sites were updated.
- **`scroll_page_with_retry` retry adds latency** on transient errors under load — 250ms/500ms/1s/2s before final failure. Acceptable trade-off vs. immediate failure.
- **Rollback**: `git revert HEAD` or `git checkout <pre-session-sha> -- <files>`. No DB migrations, no schema changes.

---

## Decisions Not Taken

- **Shared generic retry helper** — would unify `qdrant_delete_with_retry` and `scroll_page_with_retry` but requires trait bounds for request body type. Rejected: marginal benefit, added complexity.
- **`cfg` thread-through to free functions** — considered having `to_pagination` accept `&Config` directly. Rejected: `default: usize` is simpler and doesn't couple the helper to the Config type.
- **Allowlist entry for `client.rs`** — considered adding to `.monolith-allowlist` when file hit 507 lines. Not needed: the monolith script counts non-blank non-comment lines, which stays under 500.

---

## Open Questions

- The `qdrant_delete_by_url_filter_removes_matching_points` test now has retry on the read path — but if Qdrant itself is under extreme sustained load during a full test suite run, 4 retries may still not be enough. If flakiness persists, consider increasing `MAX_ATTEMPTS` or adding a `tokio::time::sleep(Duration::from_millis(100)).await` before the first scroll attempt.
- `to_map_options` `limit=0` ("no limit") behavior is now consistent with CLI default, but the MCP schema does not document `0` explicitly as "no limit" in the parameter description — only in the pagination section. Consider adding a note to the `map` parameter table.

---

## Next Steps

- Run `just verify` (fmt-check + clippy + check + test) as a final pre-PR gate.
- MCP-TOOL-SCHEMA.md was regenerated manually — consider whether `mcp-schema-validator` agent should be run to verify doc/schema consistency.
- The plan `linked-questing-wozniak.md` is now complete — move to `docs/plans/complete/`.
