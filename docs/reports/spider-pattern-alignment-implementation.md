# Spider Pattern Alignment — Implementation Report
**Date:** 2026-02-22
**Branch:** `perf/command-performance-fixes`
**Plan file:** `iridescent-sleeping-anchor.md`
**Review doc:** `docs/reports/spider-pattern-alignment-review.md`
**Tests before:** 321 passing | **Tests after:** 336 passing (+15)

---

## Summary

All P1, P2, P3 (dedup + 6 builder methods), and P4 items identified in the Spider Pattern Alignment Review were implemented across two sessions using a 5-agent parallel Wave 1 followed by lead Wave 2 orchestration. All 9 live integration tests pass against real sites.

---

## Implementation Evidence

### P1 — Bug Fixes

#### P1.1: Missing `.build()` call in HTTP mode
**File:** `crates/crawl/engine.rs`

**Before:**
```rust
// configure_website() returned without calling .build()
// HTTP mode skipped validation entirely
let mut website = configure_website(cfg, start_url, mode).await?;
website.crawl_raw().await;  // unconfigured website
```

**After:**
```rust
// .build() moved to end of configure_website() — all render modes call it
let website = website
    .build()
    .map_err(|e| format!("failed to build website for '{start_url}': {e}"))?;
Ok(website)
```

**Verification:** `cargo build --bin axon` passes; map test completed 80+ URLs.

---

#### P1.2: Chrome CDP fallback passed raw unvalidated URL
**File:** `crates/crawl/engine.rs`

**Before:**
```rust
let chrome_url = match resolve_cdp_ws_url(remote_url).await {
    Some(ws_url) => ws_url,
    None => cdp_discovery_url(remote_url).unwrap_or_else(|| remote_url.to_string()),  // raw fallback
};
```

**After:**
```rust
let chrome_url = match resolve_cdp_ws_url(remote_url).await {
    Some(ws_url) => ws_url,
    None => cdp_discovery_url(remote_url).ok_or_else(|| {
        format!(
            "Cannot resolve Chrome CDP endpoint from '{remote_url}'. \
             Ensure axon-chrome is running and AXON_CHROME_REMOTE_URL is \
             set to its HTTP management URL (e.g. http://host:6000)."
        )
    })?,
};
```

**Verification:** `cargo test engine --lib` — tests for Chrome mode error path pass.

---

### P2 — Config Exposure (Hardcoded Values → Configurable)

All 5 hardcoded values promoted to `Config` fields with CLI flags:

| Field | Default | CLI Flag | File |
|-------|---------|----------|------|
| `chrome_network_idle_timeout_secs: u64` | 15 | `--chrome-network-idle-timeout` | `types.rs` |
| `auto_switch_thin_ratio: f64` | 0.60 | `--auto-switch-thin-ratio` | `types.rs` |
| `auto_switch_min_pages: usize` | 10 | `--auto-switch-min-pages` | `types.rs` |
| `crawl_broadcast_buffer_min: usize` | 4096 | (profile-driven) | `types.rs` |
| `crawl_broadcast_buffer_max: usize` | 16_384 | (profile-driven) | `types.rs` |

Profile broadcast buffer defaults (added to `parse/performance.rs`):

| Profile | min | max |
|---------|-----|-----|
| `high-stable` | 4096 | 16_384 |
| `balanced` | 4096 | 8_192 |
| `extreme` | 8_192 | 32_768 |
| `max` | 16_384 | 65_536 |

**Wire location:** All fields wired in `parse/mod.rs:into_config()` and `engine.rs`.

**Live test evidence (Test F — auto-switch-thin-ratio):**
```
./target/release/axon crawl https://react.dev/ \
  --max-pages 5 --auto-switch-thin-ratio 0.3 --wait true --embed false
EXIT: 0  |  5 files produced  |  12,866 chars/page
```

---

### P2 — WebDriver Pre-flight Connectivity Check

**File:** `crates/crawl/engine.rs`

Added 3-second HTTP pre-flight check before configuring WebDriver spider:
```rust
let client = crate::crates::core::http::http_client()
    .map_err(|e| format!("failed to build HTTP client for WebDriver check: {e}"))?;
let wd_check = client.get(wd_url).timeout(Duration::from_secs(3)).send().await;
if wd_check.is_err() {
    return Err(format!(
        "WebDriver server at '{wd_url}' is not reachable. \
         Start a Selenium/WebDriver server before using --webdriver-url."
    ).into());
}
```

---

### P3 — Missing Spider Builder Methods (6 wired)

All 6 high-priority builder methods added to `Config`, `cli.rs`, `parse/mod.rs`, and `engine.rs`:

| Method | Config Field | CLI Flag | Live Test |
|--------|-------------|----------|-----------|
| `with_whitelist_url()` | `url_whitelist: Vec<String>` | `--url-whitelist <regex>` | Test C ✅ |
| `with_block_assets()` | `block_assets: bool` | `--block-assets` | Test B ✅ |
| `with_max_page_bytes()` | `max_page_bytes: Option<u64>` | `--max-page-bytes` | Test D ✅ |
| `with_redirect_policy()` | `redirect_policy_strict: bool` | `--redirect-policy-strict` | Test E ✅ |
| `with_wait_for_selector()` | `chrome_wait_for_selector: Option<String>` | `--chrome-wait-for-selector` | (unit) |
| `with_screenshot()` | `chrome_screenshot: bool` | `--chrome-screenshot` | (unit) |

**Wire sample (`engine.rs`, whitelist):**
```rust
if !cfg.url_whitelist.is_empty() {
    let patterns: Vec<spider::compact_str::CompactString> = cfg.url_whitelist
        .iter()
        .map(|s| s.as_str().into())
        .collect();
    website.with_whitelist_url(Some(patterns));
}
```

**Live test evidence (Test C — URL whitelist):**
```
./target/release/axon crawl https://docs.rust-lang.org/book/ \
  --max-pages 30 --url-whitelist ".*book.*" --wait true --embed false
EXIT: 0  |  30 files produced  |  0 non-book URLs
```

---

### P3 — Code Deduplication

**`canonicalize_url_for_dedupe()` removed from `engine.rs`.**
**`canonicalize_url()` in `content.rs` upgraded** to match the behavior — added default port stripping:

```rust
// content.rs — canonicalize_url() now strips :80 and :443
match (parsed.scheme(), parsed.port()) {
    ("http", Some(80)) | ("https", Some(443)) => { let _ = parsed.set_port(None); }
    _ => {}
}
```

`engine.rs` now imports `canonicalize_url` from `crates::core::content` directly.

**4 new tests** added to `content.rs` covering port stripping, fragment removal, trailing slash normalization.

---

### P4 — spider_agent Improvements

#### P4.1: `--search-time-range` wired
**File:** `crates/cli/commands/search.rs`

`TimeRange` IS available in `spider_agent` — `cfg.search_time_range` wired to `SearchOptions`:
```rust
if let Some(ref range) = cfg.search_time_range {
    let tr = match range.as_str() {
        "day"   => Some(TimeRange::Day),
        "week"  => Some(TimeRange::Week),
        "month" => Some(TimeRange::Month),
        "year"  => Some(TimeRange::Year),
        other   => { log_warn(...); None }
    };
    if let Some(tr) = tr { opts = opts.with_time_range(tr); }
}
```

**Live test evidence (Test I — search-time-range):**
```
./target/release/axon search "Rust 2025 releases" --search-time-range week
EXIT: 0  |  9 results returned  |  time-filtered
```

#### P4.2: `--research-depth` Config field added
**File:** `crates/cli/commands/research.rs`

`ResearchOptions::with_depth` is NOT available in pinned `spider_agent` version. Config field `research_depth: Option<usize>` added (default: None). TODO comment added:
```rust
// TODO: spider_agent v2.46+ exposes ResearchOptions::with_depth().
// When spider_agent is upgraded, wire cfg.research_depth here.
```

**Live test evidence (Test H — research):**
```
./target/release/axon research "Rust async runtime comparison 2025"
EXIT: 0  |  10 sources  |  full LLM synthesis with citations
```

#### P4.3: `extract.rs` design note documented
```rust
// Design note: axon_rust uses its own DeterministicExtractionEngine rather than
// spider_agent::Agent::extract() for performance reasons — deterministic parsing
// is O(1) in LLM calls and works offline, while spider_agent's extraction requires
// an LLM API call per page.
```

---

## New Config Fields Reference

13 new fields added to `crates/core/config/types.rs`:

```rust
// P2 — engine tuning (previously hardcoded)
pub chrome_network_idle_timeout_secs: u64,  // 15
pub auto_switch_thin_ratio: f64,             // 0.60
pub auto_switch_min_pages: usize,            // 10
pub crawl_broadcast_buffer_min: usize,       // 4096
pub crawl_broadcast_buffer_max: usize,       // 16_384

// P3 — spider builder methods
pub url_whitelist: Vec<String>,              // []
pub block_assets: bool,                      // false
pub max_page_bytes: Option<u64>,             // None
pub redirect_policy_strict: bool,            // false
pub chrome_wait_for_selector: Option<String>, // None
pub chrome_screenshot: bool,                 // false

// P4 — spider_agent
pub research_depth: Option<usize>,           // None
pub search_time_range: Option<String>,       // None
```

---

## Live Integration Test Evidence

All tests run against real public sites using `./target/release/axon` with `.env` sourced.

| Test | Command | Expected | Files/Output | Status |
|------|---------|----------|--------------|--------|
| A | `crawl docs.rust-lang.org/book --render-mode http --max-pages 20` | pages > 0, no crash | 10 md files | ✅ |
| B | `crawl doc.rust-lang.org --block-assets true --max-pages 5` | completes, faster | 5 md files | ✅ |
| C | `crawl docs.rust-lang.org/book --url-whitelist ".*book.*" --max-pages 30` | only book URLs | 30 files, 0 non-book | ✅ |
| D | `crawl crates.io --max-page-bytes 102400 --max-pages 10` | no crash, pages produced | 10 md files | ✅ |
| E | `crawl www.rust-lang.org --redirect-policy-strict true --max-pages 5` | no cross-domain | 5 rust-lang.org files | ✅ |
| F | `crawl react.dev --auto-switch-thin-ratio 0.3 --max-pages 5` | flag accepted, completes | 5 files, exit 0 | ✅ |
| G | `map docs.rust-lang.org --max-depth 1` | 80+ URLs, no panic | 80+ URLs (prior session) | ✅ |
| H | `research "Rust async runtime comparison 2025"` | multi-source synthesis | 10 sources, full summary | ✅ |
| I | `search "Rust 2025 releases" --search-time-range week` | time-filtered results | 9 results, exit 0 | ✅ |

---

## Files Modified

### This Sprint (Wave 1 + Wave 2)

| File | Action | Purpose |
|------|--------|---------|
| `crates/core/config/types.rs` | Modified | 13 new Config fields |
| `crates/core/config/cli.rs` | Modified | 11 new CLI flags |
| `crates/core/config/parse/mod.rs` | Modified | Wire 13 fields in `into_config()` |
| `crates/core/config/parse/performance.rs` | Modified | Profile broadcast buffer defaults |
| `crates/crawl/engine.rs` | Modified | P1+P2+P3 fixes (build, CDP, Config, spider builders, dedup) |
| `crates/core/content.rs` | Modified | `canonicalize_url()` port stripping + 4 tests |
| `crates/cli/commands/search.rs` | Modified | `TimeRange` wired |
| `crates/cli/commands/research.rs` | Modified | `research_depth` TODO documented |
| `crates/cli/commands/extract.rs` | Modified | Design note comment |
| `crates/vector/ops/input.rs` | Modified | `chunk_text("")` early return |
| `crates/vector/ops/ranking/mod.rs` | Created | Core ranking (190 lines); bug fix |
| `crates/vector/ops/ranking/snippet.rs` | Created | Snippet extraction (322 lines) |
| `crates/vector/ops/ranking/ranking_test.rs` | Moved | Tests alongside module |
| `crates/vector/ops/ranking.rs` | Deleted | Replaced by ranking/ submodule |
| `crates/vector/ops/ranking_test.rs` | Deleted | Moved to ranking/ |
| `crates/crawl/engine/collector.rs` | Modified | `tx.send().await.ok()` clippy fix |
| `crates/crawl/engine/tests.rs` | Modified | Test isolation for thin-ratio vs coverage |
| `.monolith-allowlist` | Cleaned | Removed ranking.rs entry (split instead) |

---

## Quality Gate Results

| Gate | Command | Result |
|------|---------|--------|
| Build | `cargo build --bin axon` | ✅ exit 0 |
| Tests | `cargo test --lib` | ✅ 336 pass, 0 fail |
| Lint | `cargo clippy -- -D warnings` | ✅ 0 warnings |
| Format | `cargo fmt --check` | ✅ no diff |
| Monolith | `python3 scripts/enforce_monoliths.py --base main --head HEAD` | ✅ 0 violations (22 warnings) |
| ranking/mod.rs | `wc -l` | ✅ 190 lines |
| ranking/snippet.rs | `wc -l` | ✅ 322 lines |

---

## Decisions Not Taken

1. **`ranking.rs` monolith allowlist**: Rejected by user. Split into `ranking/mod.rs` + `ranking/snippet.rs` instead — better long-term architecture separating core ranking from snippet extraction.
2. **`research_depth` wiring**: `ResearchOptions::with_depth` absent in pinned `spider_agent`. Chose TODO comment over fabricating an API call.
3. **`is_excluded_url_path()` consolidation**: engine.rs now imports from `content.rs` (canonical). The `is_path_prefix_excluded` helper stays in engine.rs as it's internal to path blacklist building.
4. **P4 items not in scope**: `with_connect_timeout_ms`, alternative search providers (Serper/Brave/Bing), `with_dismiss_dialogs`, `with_emulation`, browser automation (`navigate/click/type_text`) — deferred.

---

## Open Questions

1. **`research_depth`**: When `spider_agent` is upgraded past the version adding `ResearchOptions::with_depth`, wire `cfg.research_depth` in `research.rs`.
2. **`chrome_screenshot` output dir**: Currently uses `cfg.output_dir`. May need a dedicated `screenshots/` subdirectory to avoid mixing with markdown output.
3. **Existing Qdrant points with `:80`/`:443`**: `canonicalize_url` now strips these. URLs already indexed with explicit ports will be treated as distinct from newly-crawled normalized URLs until re-crawled.
4. **`with_max_bytes_allowed()` (total crawl cap)**: Not yet wired — distinct from `with_max_page_bytes()` (per-page cap). Deferred to future sprint.
