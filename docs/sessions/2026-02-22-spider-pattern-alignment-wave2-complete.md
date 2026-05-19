# Spider Pattern Alignment — Wave 2 Complete

**Date:** 2026-02-22
**Branch:** `perf/command-performance-fixes`
**Plan file:** `iridescent-sleeping-anchor.md`
**Tests at session start:** 321 passing | **Tests at session end:** 336 passing (+15)

---

## Session Overview

Completed Wave 2 (lead orchestration) of the Spider Pattern Alignment plan. Five parallel agents ran in Wave 1 during the prior session; this session resolved all compilation errors introduced by those agents, fixed a pre-existing logic bug in `select_diverse_candidates`, and split an over-limit file as requested by the user. Final state: all quality gates green.

**Agents run:** engine-agent (shut down end of session), config-agent (task backfill for types.rs), ops-agent, commands-agent, content-agent — all completed in prior session.

---

## Timeline

| Time | Activity |
|------|----------|
| Session start | Resumed from context-compacted prior session; 3 categories of compile errors pending |
| T+5m | Fixed `select_diverse_candidates_from_indices` — Pass 2 was re-selecting pass-1 indices |
| T+10m | `cargo test --lib` → 336 passed, 0 failed |
| T+15m | `cargo fmt` applied; `cargo clippy` clean |
| T+20m | Monolith check — `ranking.rs` 508 lines (8 over) |
| T+25m | User: "I'd rather you split it" — split `ranking.rs` → `ranking/mod.rs` + `ranking/snippet.rs` |
| T+35m | All gates green; engine-agent shutdown confirmed |

---

## Key Findings

1. **`select_diverse_candidates_from_indices` bug** (`crates/vector/ops/ranking.rs:135`): Pass 2 iterates `candidate_indices` from the start, including indices already selected in Pass 1. No `selected_set` guard existed, so the same index could be returned twice. Fix: add `HashSet<usize> selected_set` and skip already-selected indices in Pass 2.

2. **Config-agent never wrote `types.rs`** (prior session): Agent reported "All 13 fields added" but `git diff` showed `types.rs` unchanged. Cargo incremental cache masked the missing fields. All 13 fields were added manually by the lead in the prior session.

3. **collector.rs `let _ = tx.send(...)`** (`crates/crawl/engine/collector.rs`): `Sender::send` is async; `let _ =` drops the future unpolled — clippy lint. Fixed to `tx.send(summary.clone()).await.ok()`.

4. **`should_fallback_to_chrome` test isolation** (`crates/crawl/engine/tests.rs`): The function has two independent checks (thin-ratio AND coverage). Tests using `max_pages=200` caused the coverage check `(200/10).max(n)` to fire alongside the thin-ratio check, masking which check was being tested. Fixed by using `max_pages=0` (bypasses coverage) or `max_pages=80` (isolates coverage).

5. **Stale cargo cache** masked compile errors after `types.rs` was updated. `touch crates/crawl/engine.rs` forced cache invalidation.

---

## Files Modified

| File | Action | Purpose |
|------|--------|---------|
| `crates/vector/ops/ranking/mod.rs` | **Created** | Core ranking: tokenizers, `AskCandidate`, reranker, diversity selection (190 lines) |
| `crates/vector/ops/ranking/snippet.rs` | **Created** | Snippet extraction pipeline: `get_meaningful_snippet`, `select_best_preview_chunk`, private helpers (322 lines) |
| `crates/vector/ops/ranking/ranking_test.rs` | **Created** (moved) | Tests moved from `ops/ranking_test.rs` alongside new module |
| `crates/vector/ops/ranking.rs` | **Deleted** | Replaced by `ranking/mod.rs` + `ranking/snippet.rs` |
| `crates/vector/ops/ranking_test.rs` | **Deleted** | Moved to `ranking/ranking_test.rs` |
| `crates/crawl/engine/collector.rs` | **Modified** | Fixed `let _ = tx.send(...)` → `.await.ok()` (clippy async lint) |
| `crates/crawl/engine/tests.rs` | **Modified** | Fixed 2 test assertions isolating thin-ratio vs coverage check |
| `crates/core/config/types.rs` | **Modified** (prior session) | Added 13 new Config fields (P2+P3+P4) |
| `crates/core/config/parse/mod.rs` | **Modified** (prior session) | Wired 13 new fields in `into_config()` |
| `crates/core/config/parse/performance.rs` | **Modified** (prior session) | Added `ProfileSettings` struct + broadcast buffer profile values |
| `crates/core/config/cli.rs` | **Modified** (prior session) | Added 11 new CLI flags |
| `crates/crawl/engine.rs` | **Modified** (prior session) | P1+P2+P3 fixes (build, CDP, Config fields, spider builder methods, dedup) |
| `crates/core/content.rs` | **Modified** (prior session) | `canonicalize_url()` upgraded with default-port stripping |
| `crates/cli/commands/search.rs` | **Modified** (prior session) | `search_time_range` wired to `TimeRange` enum |
| `crates/cli/commands/research.rs` | **Modified** (prior session) | `research_depth` TODO documented |
| `crates/cli/commands/extract.rs` | **Modified** (prior session) | Design note comment added |
| `crates/vector/ops/input.rs` | **Modified** (prior session) | `chunk_text("")` early return fix |
| `.monolith-allowlist` | **Modified** | Reverted ranking.rs entry (split instead); cleaned up entries |

---

## Wave 1 Changes Summary (all 5 agents)

### Agent 1 — config-agent
13 new fields added to `Config` struct in `types.rs`:

```rust
// P2 — engine tuning
pub chrome_network_idle_timeout_secs: u64,  // default: 15
pub auto_switch_thin_ratio: f64,             // default: 0.60
pub auto_switch_min_pages: usize,            // default: 10
pub crawl_broadcast_buffer_min: usize,       // default: 4096
pub crawl_broadcast_buffer_max: usize,       // default: 16_384
// P3 — missing spider builder methods
pub url_whitelist: Vec<String>,              // default: []
pub block_assets: bool,                      // default: false
pub max_page_bytes: Option<u64>,             // default: None
pub redirect_policy_strict: bool,           // default: false
pub chrome_wait_for_selector: Option<String>, // default: None
pub chrome_screenshot: bool,                // default: false
// P4 — spider_agent improvements
pub research_depth: Option<usize>,          // default: None
pub search_time_range: Option<String>,      // default: None
```

Profile broadcast buffer defaults in `performance.rs`:
- `high-stable`: min=4096, max=16_384
- `balanced`: min=4096, max=8_192
- `extreme`: min=8_192, max=32_768
- `max`: min=16_384, max=65_536

### Agent 2 — engine-agent
- **P1 Fix 1**: `.build()` moved from Chrome branch to end of `configure_website()` — all render modes now call it
- **P1 Fix 2**: Chrome CDP fallback returns actionable error instead of raw URL: `cdp_discovery_url(remote_url).ok_or_else(|| format!("Cannot resolve Chrome CDP endpoint from '{remote_url}'..."))?`
- **P2**: `cfg.chrome_network_idle_timeout_secs` replaces hardcoded 15 in `WaitForIdleNetwork`
- **P2**: `cfg.auto_switch_thin_ratio` and `cfg.auto_switch_min_pages` replace hardcoded 0.60/10 in `should_fallback_to_chrome`
- **P2**: `cfg.crawl_broadcast_buffer_min/max` replace hardcoded 4096/16384 in subscribe buffer clamp
- **P3**: `url_whitelist`, `block_assets`, `max_page_bytes`, `redirect_policy_strict`, `chrome_wait_for_selector`, `chrome_screenshot` all wired
- **P3**: `canonicalize_url_for_dedupe` removed; uses `crate::crates::core::content::canonicalize_url` instead
- **P3**: WebDriver pre-flight 3s HTTP check before configuring spider
- `should_fallback_to_chrome` signature: `(summary, max_pages, cfg: &Config)`

### Agent 3 — content-agent
`canonicalize_url()` upgraded to strip default ports:
```rust
match (parsed.scheme(), parsed.port()) {
    ("http", Some(80)) | ("https", Some(443)) => { let _ = parsed.set_port(None); }
    _ => {}
}
```
4 new tests added covering port stripping, fragment removal, trailing slash normalization.

### Agent 4 — commands-agent
- `search.rs`: `TimeRange` IS available in `spider_agent` — `cfg.search_time_range` wired to `SearchOptions`
- `research.rs`: `ResearchOptions::with_depth` NOT available in current spider_agent — TODO comment added
- `extract.rs`: Design note comment explaining `DeterministicExtractionEngine` vs `spider_agent::Agent::extract()`

### Agent 5 — ops-agent
- `chunk_text("")` → early return `vec![]` when `text.trim().is_empty()`
- `select_diverse_candidates_from_indices`: added `selected_set: HashSet<usize>` to prevent Pass 2 from re-selecting Pass 1 indices

---

## Behavior Changes (Before/After)

| Behavior | Before | After |
|----------|--------|-------|
| `configure_website()` HTTP mode | No `.build()` call — spider website potentially unconfigured | `.build()` called for all render modes |
| Chrome CDP failure | Falls back to raw unvalidated URL string | Returns error: "Cannot resolve Chrome CDP endpoint from '...'" |
| `should_fallback_to_chrome` | Hardcoded 0.60 thin ratio, 10 min pages | Configurable via `--auto-switch-thin-ratio`, `--auto-switch-min-pages` |
| Broadcast channel buffer | Hardcoded `clamp(4096, 16_384)` | Profile-driven; configurable per performance profile |
| `canonicalize_url()` | Did not strip default ports (`:80`, `:443`) | Strips `:80` for http and `:443` for https |
| `chunk_text("")` | Returned `[""]` (single empty chunk) | Returns `[]` (empty Vec) |
| `select_diverse_candidates` | Could return duplicate indices when all from same URL | Correctly caps at `max_per_url` with no duplicates |
| `url_whitelist` | Not supported | `--url-whitelist <regex>` (repeatable) |
| `block_assets` | Not supported | `--block-assets` flag wires `website.with_block_assets(true)` |
| `max_page_bytes` | Not supported | `--max-page-bytes <bytes>` wires `website.with_max_page_bytes(Some(n))` |
| `redirect_policy_strict` | Not supported | `--redirect-policy-strict` wires `RedirectPolicy::Strict` |
| `search_time_range` | Not supported | `--search-time-range day\|week\|month\|year` wires `SearchOptions::with_time_range()` |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo build --bin axon` | exit 0 | exit 0 (30s) | ✅ |
| `cargo test --lib` | ≥321 pass, 0 fail | 336 pass, 0 fail | ✅ |
| `cargo clippy -- -D warnings` | 0 warnings | 0 warnings | ✅ |
| `cargo fmt --check` | no diff | no diff | ✅ |
| `python3 scripts/enforce_monoliths.py --base main --head HEAD` | no violations | no violations (22 warnings only) | ✅ |
| `wc -l crates/vector/ops/ranking/mod.rs` | <500 | 190 | ✅ |
| `wc -l crates/vector/ops/ranking/snippet.rs` | <500 | 322 | ✅ |
| `cargo test select_diverse --lib` | 4 pass | 4 pass | ✅ |
| `./scripts/axon map https://docs.rust-lang.org/ --max-depth 1` | URLs listed, no panic | 80+ URLs, clean exit | ✅ |
| `target/release/axon` binary exists | 29M release binary | 29M, Feb 22 | ✅ |

---

## Source IDs + Collections Touched

None — no embed/retrieve operations performed during this session (all work was code compilation and testing).

---

## Risks and Rollback

- **`should_fallback_to_chrome` signature change**: All callers updated (`sync_crawl.rs:78`, `worker_process.rs:217`). If any missed caller exists in test code, it would surface as a compile error — already verified clean.
- **`canonicalize_url` now strips default ports**: Could affect existing Qdrant dedup if indexed URLs have `:80`/`:443` — new crawls will normalize these away, but existing points remain unaffected.
- **Rollback**: `git revert` or `git checkout main -- <file>` per file; no DB migrations, no infrastructure changes.

---

## Decisions Not Taken

1. **`ranking.rs` allowlist entry**: User rejected adding `ranking.rs` to `.monolith-allowlist`; split into `ranking/mod.rs` + `ranking/snippet.rs` instead. Better long-term architecture.
2. **`research_depth` wiring**: `ResearchOptions::with_depth` not present in the pinned `spider_agent` version. Chose TODO comment over fabricating an API call that would compile-fail.
3. **Removing `canonicalize_url_for_dedupe` tests from engine/tests.rs**: Tests at lines 89–103 called the now-removed function. Updated to use `canonicalize_url` from `content.rs` (same behavior with port stripping added).

---

## Open Questions

1. **`spider_agent` version pin**: `ResearchOptions::with_depth` is missing. When spider_agent is upgraded past the version that adds it, `research.rs` should be updated to wire `cfg.research_depth`.
2. **`chrome_screenshot` output directory**: Wired as `cfg.output_dir` — may need a dedicated `screenshots/` subdirectory to avoid mixing with markdown output.
3. **Existing Qdrant points with `:80`/`:443` ports**: `canonicalize_url` now strips these. URLs already indexed with explicit ports will be treated as distinct from newly-crawled normalized URLs until re-crawled.

---

## Next Steps

1. Run remaining live integration tests (A–F, H, I) from the plan once Chrome is accessible
2. Write `docs/reports/spider-pattern-alignment-implementation.md` (evidence report)
3. Update `docs/reports/spider-pattern-alignment-review.md` with "Status: FIXED" per recommendation
4. `superpowers:finishing-a-development-branch` to finalize PR
