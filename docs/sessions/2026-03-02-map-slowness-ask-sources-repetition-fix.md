# Session: map Command Slowness + ask ## Sources Repetition Bug Fix

**Date:** 2026-03-02
**Branch:** feat/sidebar

---

## Session Overview

Two issues investigated this session:

1. **`axon map` taking 90+ seconds** — diagnosed why and confirmed no fast-path exists in spider.rs OSS crate; documented workarounds.
2. **`axon ask` streaming `## Sources` hundreds of times** — root-caused to LLM repetition loop at low temperature combined with a streaming architecture that bypasses normalization, then fixed with a repetition guard in the SSE stream.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | User reports `axon map https://docs.rs/rmcp/latest/rmcp/` taking 90+ seconds |
| Phase 1 | Traced `map` → `crawl_and_collect_map` → `website.crawl_raw()` — full HTTP crawl, no links-only mode |
| Phase 2 | User ran `axon ask "crawl links only with spider.rs"` to research spider.rs API |
| Phase 3 | RAG response returned with `## Sources` repeated 100+ times — new bug identified |
| Phase 4 | Systematic debug of `## Sources` repetition — traced LLM stream → stdout flow |
| Phase 5 | Fix implemented in `streaming.rs`, 4 tests written, all 683 passing |

---

## Key Findings

### map Slowness
- `crawl_and_collect_map` (`crates/crawl/engine.rs:56`) calls `website.crawl_raw().await` — fetches every HTML page to extract `<a href>` links
- "Without scraping" in docs means without saving to disk/embedding, NOT without network fetching
- spider.rs OSS crate has no "links-only" mode that skips HTML parsing; that exists only in the cloud API
- `discover_sitemaps: true` (default) also fetches `/sitemap.xml` after the crawl — for docs.rs, the sitemap is enormous
- `max_pages: 0` (default = uncapped) means the crawl runs until spider exhausts all discoverable pages at depth 5
- docs.rs links across all crate versions on the same host, multiplying page count

### ## Sources Repetition Bug
- **Root cause**: LLM enters repetition loop at `temperature: 0.1` — after generating `## Sources` once, the context full of `## Top Chunk [S1]:` / `## Source Document [S2]:` headers makes `## ` the next highest-probability token
- **Compounding problem**: `run_sse_stream` (`streaming.rs:155`) prints every token to stdout immediately; `normalize_ask_answer` runs after and produces the clean answer, but `ask.rs:82-86` silently discards the normalized answer when `streamed_to_stdout = true`
- `strip_sources_section` (`normalize.rs:6`) correctly handles the case — finds first `\n## sources` and truncates there — but this normalized result never reached the terminal

---

## Technical Decisions

### Why early-exit in `run_sse_stream` (not post-processing)
The streaming is already on stdout before normalization runs. To fix the UX (stop showing garbage), the fix must be in the stream loop itself. Post-processing the `raw_answer` string already worked correctly — it just wasn't displayed.

### Why track second occurrence (not first)
The first `\n## Sources` is the correct, intended sources block. Truncating at the first would break the output. Only the second+ occurrence is the repetition loop.

### Why not add LLM stop sequences
Stop sequences are model-specific and not universally supported across OpenAI-compatible endpoints. A Rust-side fix is more robust and doesn't add API surface.

### Why not change context header format
Context headers (`## Top Chunk [S1]:`) are parsed by `parse_context_source_map` (`normalize.rs:44`). Changing them would require updating the parser and all tests. The streaming fix addresses the symptom correctly without a larger refactor.

### map workarounds (not changed — behavior is correct by design)
- `--max-pages N` — caps page count
- `--discover-sitemaps false` — skips sitemap fetch
- Both flags were already present; no code change made

---

## Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `crates/vector/ops/commands/streaming.rs` | Added `check_sources_repetition()` + repetition guard in `run_sse_stream` + 4 tests | Fix `## Sources` repetition loop |

---

## Commands Executed

```bash
# Diagnosis
grep -n "crawl_and_collect_map\|crawl_raw\|get_links" crates/crawl/engine.rs
grep -n "return_page_links\|with_block_assets\|only_html" ~/.cargo/registry/src/**/spider-2.45.12/src/configuration.rs

# Verification
cargo check --lib                            # clean, 0 errors
cargo test sources_repetition               # 4/4 green
cargo test                                  # 683 passed, 0 failed
wc -l crates/vector/ops/commands/streaming.rs  # 408 lines — within monolith limit
```

---

## Behavior Changes (Before/After)

| Scenario | Before | After |
|----------|--------|-------|
| LLM enters `## Sources` repetition loop | Streams 100+ `## Sources` to stdout until model stops | Stream exits early after detecting second `\n## Sources`; terminal shows clean first sources section |
| Normal ask with single `## Sources` | Works correctly | Still works correctly — single occurrence is never truncated |
| Case-insensitive sources header | Not tested | Handled correctly (lowercased before search) |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --lib` | 0 errors | 0 errors, 0 warnings | ✅ |
| `cargo test sources_repetition` | 4 pass | 4 pass | ✅ |
| `cargo test` | All pass | 683 pass, 0 fail | ✅ |
| `test_sources_repetition_no_sources` | None returned, first=None | Pass | ✅ |
| `test_sources_repetition_single_sources` | None returned, first recorded | Pass | ✅ |
| `test_sources_repetition_detects_second` | Second pos returned, truncated correctly | Pass | ✅ |
| `test_sources_repetition_case_insensitive` | Detects `## SOURCES` and `## sources` | Pass | ✅ |

---

## Source IDs + Collections Touched

None — no embedding operations performed this session.

---

## Risks and Rollback

**Risk:** `check_sources_repetition` scans with a `saturating_sub(10)` overlap window. For extremely short token batches, a `\n## Sources` split across the overlap could be missed. In practice, the needle is 11 chars and tokens are rarely that small.

**Risk:** If a legitimate ask answer contains two separate `## Sources` sections (e.g., a structured comparison), the second would be truncated. Current system prompt explicitly says "a single `## Sources` section" so this is correct behavior.

**Rollback:** Revert `check_sources_repetition` function and the guard block in `run_sse_stream`. The `process_sse_line` signature and all other callers are unchanged.

---

## Decisions Not Taken

| Alternative | Reason Rejected |
|-------------|-----------------|
| Add LLM stop sequences | Not universally supported across OpenAI-compatible endpoints |
| Change context header format from `## Top Chunk [S1]:` | Requires updating `parse_context_source_map` parser and multiple tests |
| Print normalized answer after streaming (overwrite terminal) | Complex ANSI escape handling; normalization already computes correct answer, this would duplicate display |
| Add `max_tokens` cap to LLM request | Would silently truncate legitimate long answers; repetition guard is surgical |

---

## Open Questions

- Should `map` default to `--max-pages 200` or similar to prevent 90-second hangs on large sites?
- Should `ask.rs` display the normalized answer (with a separator) even when `streamed_to_stdout = true`, at least when repetition was detected?
- The `## Sources` repeated in the streaming output is a local model quality issue — does this only happen with specific models, or universally at low temperature?

---

## Next Steps

- Consider a `--max-pages` default (e.g., 500) for `map` command to prevent runaway crawls
- Consider printing a "normalized" marker + clean answer after streaming if normalization changed anything material
- Monitor whether `## Sources` repetition recurs with different queries/models
