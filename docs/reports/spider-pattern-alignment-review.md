# Spider Pattern Alignment Review
**Date:** 2026-02-22
**Branch:** `perf/command-performance-fixes`
**Scope:** All CLI commands, crawl engine, core utilities, jobs/workers
**Spider version reviewed:** v2.45.24

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Spider API Coverage — Missing Methods](#2-spider-api-coverage--missing-methods)
3. [Custom Reimplementations of Spider Features](#3-custom-reimplementations-of-spider-features)
4. [Incorrect or Suboptimal API Usage](#4-incorrect-or-suboptimal-api-usage)
5. [Configuration System Gaps](#5-configuration-system-gaps)
6. [Code Duplication (Internal)](#6-code-duplication-internal)
7. [spider_agent Underutilization](#7-spider_agent-underutilization)
8. [Architecture Observations](#8-architecture-observations)
9. [Intentional Deviations (Justified)](#9-intentional-deviations-justified)
10. [Prioritized Recommendations](#10-prioritized-recommendations)

---

## 1. Executive Summary

This review was conducted by a team of 4 parallel specialist agents:

| Agent | Domain | Files Reviewed |
|-------|--------|---------------|
| Spider API Cataloger | `~/workspace/spider` (all crates) | 25,000+ LOC spider, 100,000+ LOC spider_agent |
| Crawl Engine Reviewer | `crates/crawl/engine.rs`, `crates/core/config/` | engine.rs, config/types.rs, config/parse/ |
| Core Utilities Reviewer | `crates/core/` (all files) | http.rs, content.rs, config/, logging.rs, ui.rs, health.rs |
| CLI Commands & Jobs Reviewer | `crates/cli/commands/`, `crates/jobs/` | All command files, all job workers |

### Overall Verdict

**axon_rust demonstrates correct and deep understanding of spider's core channel/broadcast semantics.** The integration is fundamentally sound. No critical misuse of spider's API was found.

However, the review uncovered **30+ spider builder methods that are never called**, several **hardcoded values that belong in Config**, **two internal code duplications**, a **missing `.build()` call in HTTP mode**, and **significant underutilization of spider_agent's capabilities**.

### Severity Summary

| Severity | Count | Description |
|----------|-------|-------------|
| 🔴 **P1 — Bug/Contract** | 2 | Missing `.build()` call (HTTP mode); WebDriver path incomplete |
| 🟠 **P2 — Hardcoded/Config** | 5 | Network idle timeout, auto-switch thresholds, subscribe buffer bounds |
| 🟡 **P3 — Missing APIs** | 30 | spider builder methods never called |
| 🟡 **P3 — Internal Duplication** | 2 | `is_excluded_url_path()`, `canonicalize_url()` duplicated |
| 🔵 **P4 — Underutilization** | 6 | spider_agent capabilities unused |
| ✅ **Justified** | 8 | Intentional architectural choices with good rationale |

---

## 2. Spider API Coverage — Missing Methods

Spider's `Website` builder exposes **100+ fluent `.with_*()` methods**. The following are never called anywhere in axon_rust. Each entry includes the spider source location and a recommendation for whether axon_rust should adopt it.

### 2.1 Chrome / Rendering APIs

| Method | spider/src/website.rs | Priority | Recommendation |
|--------|----------------------|----------|----------------|
| `with_event_tracker()` | ~:8957 | 🟡 P3 | Useful for tracking DOM changes and detecting when JS execution is complete |
| `with_stealth_advanced()` | ~:9093 | 🟡 P3 | Expose alongside existing `chrome_stealth` config flag for fingerprint customization |
| `with_fingerprint_advanced()` | ~:9233 | 🟡 P3 | Extend binary `chrome_fingerprint: bool` flag to accept optional config struct |
| `with_viewport()` | ~:9248 | 🟡 P3 | Add `chrome_viewport_width/height` to Config for responsive testing |
| `with_wait_for_idle_dom()` | ~:9293 | 🟡 P3 | Complement to existing `with_wait_for_idle_network0()`; faster on some SPAs |
| `with_wait_for_almost_idle_network0()` | ~:9274 | 🔵 P4 | Lenient variant of network-idle; lower false-positive rate on noisy CDNs |
| `with_wait_for_selector()` | ~:9284 | 🟡 P3 | Waits for specific CSS selector before capture; valuable for dynamic content |
| `with_wait_for_delay()` | ~:9302 | 🟡 P3 | Post-load delay distinct from `with_delay()` (request interval); add `chrome_post_load_delay_ms` to Config |
| `with_dismiss_dialogs()` | ~:9373 | 🔵 P4 | Suppresses Chrome alert/confirm popups; useful for poorly-behaved sites |
| `with_emulation()` | ~:9380 | 🔵 P4 | Device emulation (mobile/tablet); add `chrome_emulation_device: Option<String>` |
| `with_timezone_id()` | ~:9392 | 🔵 P4 | Force browser timezone; occasionally needed for localized content |
| `with_evaluate_on_new_document()` | ~:9398 | 🔵 P4 | Inject JS before page execution; useful for disabling analytics/tracking |
| `with_screenshot()` | ~:9415 | 🟡 P3 | Screenshot capture already referenced in code comments but never wired to Config |
| `with_auth_challenge_response()` | ~:9430 | 🔵 P4 | HTTP Basic/Digest/NTLM auth; add `http_auth_user/password` to Config |
| `with_csp_bypass()` | ~:8787 | 🔵 P4 | Disable CSP in Chrome; niche but occasionally needed for JS-heavy SPAs |

**Current code reference:** `crates/crawl/engine.rs:228–275` (Chrome branch of `configure_website()`)

---

### 2.2 HTTP / Network APIs

| Method | spider/src/website.rs | Priority | Recommendation |
|--------|----------------------|----------|----------------|
| `with_whitelist_url()` | ~:8947 | 🟡 P3 | Add `url_whitelist: Vec<String>` to Config; currently only blacklist is exposed |
| `with_default_http_connect_timeout()` | ~:9311 | 🟡 P3 | Separate connect vs read timeouts; add `connect_timeout_ms` alongside `request_timeout_ms` |
| `with_default_http_read_timeout()` | ~:9322 | 🟡 P3 | See above |
| `with_redirect_policy()` | ~:9339 | 🟡 P3 | Always uses spider's `Loose` default; expose `redirect_policy: "strict"\|"loose"` in Config |
| `with_referer()` | ~:9355 | 🔵 P4 | Referer header control; useful for sites that validate referrer chains |
| `with_http2_prior_knowledge()` | ~:9841 | 🔵 P4 | Force HTTP/2; minor performance gain on HTTP/2-only endpoints |
| `with_preserve_host_header()` | ~:8873 | 🔵 P4 | Prevent Host header rewriting; niche but needed for some reverse-proxy configs |
| `with_network_interface()` | ~:9472 | 🔵 P4 | Bind to specific NIC; multi-NIC homelab scenario |
| `with_local_address()` | ~:9478 | 🔵 P4 | Source IP binding; related to above |
| `with_full_resources()` | ~:9367 | 🔵 P4 | Preserve CSS/JS/images in response; enables resource auditing |
| `with_block_assets()` | ~:9484 | 🟡 P3 | Block image/CSS/font fetching; significant crawl speed improvement possible |
| `with_max_page_bytes()` | ~:9502 | 🟡 P3 | Per-page download size limit; prevents runaway on large binary files |
| `with_max_bytes_allowed()` | ~:9508 | 🟡 P3 | Total crawl size cap; important for rate-limiting disk usage |
| `with_cache_policy()` | ~:9102 | 🔵 P4 | HTTP cache control; spider uses cache by default but axon doesn't configure it |

---

### 2.3 State / Coordination APIs

| Method | spider/src/website.rs | Priority | Recommendation |
|--------|----------------------|----------|----------------|
| `with_shared_state()` | ~:9496 | 🔵 P4 | Share cookies/localStorage across pages; useful for authenticated crawls |
| `with_execution_scripts()` | ~:9453 | 🔵 P4 | Web Automation API; run CDP commands during crawl lifecycle |
| `with_automation_scripts()` | ~:9462 | 🔵 P4 | Automation scripts pre-wired to crawl events |
| `with_return_page_links()` | ~:9440 | 🔵 P4 | Include extracted links in real-time page results vs post-crawl `get_links()` |
| `subscribe_guard()` | ~:9765 | 🟡 P3 | Coordinate concurrent subscribe/crawl operations; would make engine.rs subscription safer |

---

### 2.4 Crawl Modes Not Utilized

Spider exposes multiple crawl modes beyond what axon_rust uses:

| Method | Description | Priority |
|--------|-------------|----------|
| `crawl_smart()` | Feature-gated smart crawl with built-in JS detection | 🟡 P3 — overlaps with axon's auto-switch but is officially supported |
| `scrape_page()` | Direct single-page scrape via spider (vs axon's custom HTTP fetch) | 🟡 P3 — `scrape.rs` uses Website for single pages; `scrape_page()` may be more direct |

---

## 3. Custom Reimplementations of Spider Features

### 3.1 Auto-Switch / Thin Page Detection

**axon_rust implementation:**
- `should_fallback_to_chrome()` — `crates/crawl/engine.rs:284–302`
- `try_auto_switch()` — `crates/crawl/engine.rs:304–350`
- Logic: HTTP crawl → measure thin-page ratio (>60%) → retry with Chrome

**Spider official equivalent:**
- `Website::crawl_smart()` — `spider/src/website.rs:4816`
- Feature-gated (`smart` feature), different semantics (budget-based JS detection)

**Assessment:** axon_rust's auto-switch is more configurable and transparent than `crawl_smart()`, but the thin-page ratio threshold (0.60) and coverage minimum (max_pages/10) are hardcoded magic numbers. These belong in Config. See [Section 5](#5-configuration-system-gaps).

---

### 3.2 CDP WebSocket URL Resolution

**axon_rust implementation:**
- `resolve_cdp_ws_url()` — `crates/crawl/engine.rs:128–165`
- Logic: Pre-resolves `ws://` URLs; rewrites Docker hostnames to `127.0.0.1`; falls back to CDP discovery on failure

**Spider official expectation:**
- `with_chrome_connection(Some("http://127.0.0.1:9222/json/version".into()))`
- Spider examples (`examples/chrome_remote.rs:20`) pass an HTTP discovery URL directly

**Assessment:** The hostname rewriting for Docker is legitimate infrastructure glue. However, the fallback to a raw `remote_url` string (engine.rs:245) without validation is risky. The fallback should explicitly validate that the URL ends with `/json/version` or is a valid ws:// endpoint before passing to spider.

---

### 3.3 URL Canonicalization

**axon_rust implementation:**
- `canonicalize_url_for_dedupe()` — `crates/crawl/engine.rs:29–47`
- Removes fragments, strips default ports (80/443), removes trailing slashes

**Spider official:**
- `with_normalize()` — `crates/crawl/engine.rs:218` (already called)
- spider's normalize handles most URL normalization internally

**Assessment:** `canonicalize_url_for_dedupe()` adds port stripping on top of spider's normalization. This is a legitimate extension (spider doesn't guarantee port stripping). The duplication with `content.rs:canonicalize_url()` (which lacks port stripping) is the real problem — see [Section 6](#6-code-duplication-internal).

---

### 3.4 Path Prefix Exclusion

**axon_rust implementation:**
- `is_excluded_url_path()` + `is_path_prefix_excluded()` + `build_exclude_blacklist_patterns()` — `crates/crawl/engine.rs:49–114`
- Semantic path matching with boundary detection to prevent `/developer` matching `/de/`

**Spider official:**
- `with_blacklist_url()` accepts regex patterns — `spider/src/website.rs:8926`
- axon_rust correctly wraps its path logic into regex patterns passed to spider's blacklist (engine.rs:398–402)

**Assessment:** The path exclusion logic is a legitimate extension of spider's blacklist. The `regex_escape()` helper (engine.rs:78–91) ensures safe patterns. This is correct.

---

### 3.5 Sitemap Backfill

**axon_rust implementation:**
- `append_sitemap_backfill()` — references in engine.rs
- `crawl_sitemap_urls()` — separate from main crawl pass

**Spider official:**
- `Website::crawl_sitemap()` — `spider/src/website.rs:4650`
- `Website::persist_links()` — `spider/src/website.rs:1390`

**Assessment:** Engine.rs lines 404–407 show axon_rust *does* use spider's `crawl_sitemap()`. The custom backfill logic adds URL deduplication and subdomain filtering on top. The `with_ignore_sitemap(true)` set unconditionally at line 279 and then overridden at line 404 is a confusing flow that should be simplified.

---

## 4. Incorrect or Suboptimal API Usage

### 4.1 🔴 P1 — Missing `.build()` Call in HTTP Mode

**Location:** `crates/crawl/engine.rs:257–309`

**Issue:** The Chrome/WebDriver branch correctly calls `.build().map_err(...)` before crawling (engine.rs:257–258). The HTTP mode and AutoSwitch modes do NOT call `.build()` before invoking `crawl_raw()`.

Spider examples consistently show `.build().unwrap()` is required after configuration to validate and initialize the Website struct. Without it, domain parsing failures are silent, configuration is not validated, and the internal state may be inconsistent.

**Spider contract:**
```rust
// spider/examples/configuration.rs (standard pattern)
let mut website = Website::new("https://example.com")
    .with_depth(5)
    .build()?;  // <-- REQUIRED
website.crawl_raw().await;
```

**Current axon_rust HTTP path:**
```rust
// engine.rs:309 — no .build() call
let mut website = configure_website(cfg, start_url, mode).await?;
website.crawl_raw().await;  // called without .build()
```

**Fix:** Add `.build()?` at the end of `configure_website()` return, or call it at engine.rs:309 before the crawl.

---

### 4.2 🔴 P1 — WebDriver Branch Incomplete

**Location:** `crates/crawl/engine.rs:259–274`

**Issue:** The WebDriver configuration branch sets up the website with webdriver options but does not validate that the WebDriver server is reachable before proceeding. If the server is down, spider will fail inside the crawl with an opaque error rather than a clear "WebDriver server unreachable" message.

**Recommendation:** Add a connectivity check (`reqwest::get(webdriver_url).await.is_ok()`) before calling `configure_website()` when `cfg.webdriver_url.is_some()`.

---

### 4.3 🟠 P2 — Hardcoded Network Idle Timeout (15 seconds)

**Location:** `crates/crawl/engine.rs:253–254, 272–273`

```rust
website.with_wait_for_idle_network0(WaitForIdleNetwork::new(Some(Duration::from_secs(15))));
```

**Issue:** 15 seconds is hardcoded for all Chrome-mode crawls regardless of target site. Fast CDN-backed sites could use 5s; heavily-loaded SPAs may need 30s.

**Fix:** Add `chrome_network_idle_timeout_secs: u64` to Config (default: 15), expose as CLI flag.

---

### 4.4 🟠 P2 — Auto-Switch Thin Ratio Hardcoded at 0.60

**Location:** `crates/crawl/engine.rs:293`

```rust
if thin_ratio > 0.60 {
```

**Issue:** The 60% threshold is a magic number. Sites with lots of thin navigational pages (e.g., large e-commerce catalogs) may not warrant Chrome even at 70%+ thin pages.

**Fix:** Add `auto_switch_thin_ratio: f64` to Config (default: 0.60), expose as CLI flag.

---

### 4.5 🟠 P2 — Auto-Switch Coverage Minimum Hardcoded

**Location:** `crates/crawl/engine.rs:301`

```rust
summary.markdown_files < (max_pages / 10).max(10)
```

**Issue:** Coverage threshold uses magic division-by-10 and minimum-of-10. For a 1000-page crawl that only finds 5 pages (all with good content), Chrome is NOT triggered — incorrectly.

**Fix:** Add `auto_switch_min_pages: usize` to Config (default: 10) + `auto_switch_coverage_divisor: usize` (default: 10), or use a simpler `auto_switch_min_coverage_pct: f64`.

---

### 4.6 🟠 P2 — Subscribe Buffer Bounds Are Magic Numbers

**Location:** `crates/crawl/engine.rs:378`

```rust
let subscribe_buf = (cfg.max_pages as usize).clamp(4096, 16_384);
```

**Issue:** For `extreme` or `max` performance profiles with very large crawls and uncapped `max_pages=0`, the default clamp to 4096 is insufficient. The collector (`collector.rs:36`) already logs dropped pages on broadcast lag.

**Fix:** Add `crawl_broadcast_buffer_min: usize` (default: 4096) and `crawl_broadcast_buffer_max: usize` (default: 16384) to performance profiles, or scale buffer with concurrency limit.

---

### 4.7 🟡 P3 — Confusing Sitemap Toggle Pattern

**Location:** `crates/crawl/engine.rs:279, 404`

```rust
website.with_ignore_sitemap(true);  // line 279 — always set
// ... later:
if run_sitemap && cfg.discover_sitemaps {
    website.crawl_sitemap().await;   // line 404
}
```

**Issue:** Setting `ignore_sitemap(true)` unconditionally and then calling `crawl_sitemap()` manually is semantically confusing. Spider's `with_ignore_sitemap(true)` prevents spider from auto-crawling the sitemap during the main crawl — the manual `crawl_sitemap()` call is correct, but the flow should be self-documenting.

**Fix:** Add a comment explaining the pattern, or restructure so `with_ignore_sitemap` is only set in the branch where it's needed.

---

### 4.8 🟡 P3 — Chrome Fallback URL Passes Unvalidated String

**Location:** `crates/crawl/engine.rs:240–246`

```rust
let chrome_url = match resolve_cdp_ws_url(remote_url).await {
    Some(ws_url) => ws_url,
    None => cdp_discovery_url(remote_url).unwrap_or_else(|| remote_url.to_string()),  // ← raw fallback
};
website.with_chrome_connection(Some(chrome_url));
```

**Issue:** If both `resolve_cdp_ws_url()` and `cdp_discovery_url()` fail, the raw `remote_url` string is passed to spider as the Chrome connection. This is likely an HTTP host:port string that spider cannot connect to.

**Fix:** Return an error rather than falling back to the raw URL. If no valid CDP endpoint can be resolved, fail fast with a descriptive error.

---

## 5. Configuration System Gaps

The following spider capabilities have no corresponding field in `crates/core/config/types.rs` and no CLI flag. All are exposed via spider's Website builder but axon_rust silently uses spider's defaults.

| Spider Method | Default Used | Suggested Config Field | CLI Flag |
|--------------|-------------|----------------------|----------|
| `with_whitelist_url()` | None (off) | `url_whitelist: Vec<String>` | `--url-whitelist` |
| `with_block_assets()` | Disabled | `block_assets: bool` | `--block-assets` |
| `with_redirect_policy()` | Loose | `redirect_policy: "strict"\|"loose"` | `--redirect-policy` |
| `with_wait_for_selector()` | None | `chrome_wait_for_selector: Option<String>` | `--chrome-wait-for-selector` |
| `with_wait_for_delay()` | None | `chrome_post_load_delay_ms: u64` | `--chrome-post-load-delay-ms` |
| `with_timezone_id()` | System default | `chrome_timezone_id: Option<String>` | `--chrome-timezone-id` |
| `with_max_bytes_allowed()` | Unlimited | `max_page_bytes: Option<usize>` | `--max-page-bytes` |
| `with_auth_challenge_response()` | None | `http_auth_user/password` | `--http-auth-user/password` |
| `with_preserve_host_header()` | Off | `preserve_host_header: bool` | `--preserve-host-header` |
| `with_emulation()` | None | `chrome_emulation_device: Option<String>` | `--chrome-emulation-device` |
| `chrome_network_idle_timeout_secs` | Hardcoded 15 | `chrome_network_idle_timeout_secs: u64` | `--chrome-network-idle-timeout` |
| `auto_switch_thin_ratio` | Hardcoded 0.60 | `auto_switch_thin_ratio: f64` | `--auto-switch-thin-ratio` |
| `auto_switch_min_coverage` | Hardcoded max_pages/10 | `auto_switch_min_pages: usize` | `--auto-switch-min-pages` |
| `crawl_broadcast_buffer_min` | Hardcoded 4096 | `crawl_broadcast_buffer_min: usize` | (profile-only) |

---

## 6. Code Duplication (Internal)

### 6.1 🟡 P3 — `is_excluded_url_path()` Exists in Two Files

**File 1:** `crates/crawl/engine.rs:49–76`
```rust
pub(crate) fn is_excluded_url_path(url: &str, excludes: &[String]) -> bool {
    // ...calls is_path_prefix_excluded() helper
    excludes.iter().any(|prefix| is_path_prefix_excluded(&path, prefix))
}
fn is_path_prefix_excluded(path: &str, prefix: &str) -> bool {
    let boundary = normalized.trim_end_matches('/');
    path == boundary || path.strip_prefix(boundary).is_some_and(|rest| rest.starts_with('/'))
}
```

**File 2:** `crates/core/content.rs:195–218`
```rust
pub fn is_excluded_url_path(url: &str, prefixes: &[String]) -> bool {
    // ...inline logic, slightly different boundary check
    path == p || (path.starts_with(p) && path.as_bytes().get(p.len()) == Some(&b'/'))
}
```

**Divergence:** The engine.rs version allocates `format!("/{prefix}")` per comparison; the content.rs version avoids allocation with byte comparison. The engine.rs version is semantically cleaner (dedicated helper function); content.rs is marginally faster but less readable.

**Fix:** Consolidate to `engine.rs` version (or move to a shared `crates/core/url_utils.rs`). Update all callsites in `crates/cli/commands/` that import from `content.rs`.

---

### 6.2 🟡 P3 — `canonicalize_url()` Has Divergent Implementations

**File 1 (engine.rs:29–47):** Strips fragment, removes default ports (80/443), removes trailing slashes
```rust
pub(crate) fn canonicalize_url_for_dedupe(url: &str) -> Option<String> {
    // strips fragment, default ports, trailing slashes
}
```

**File 2 (content.rs:220–228):** Strips fragment, removes trailing slashes (no port stripping)
```rust
pub fn canonicalize_url(url: &str) -> Option<String> {
    // strips fragment, trailing slashes only
}
```

**Impact:** A URL like `http://example.com:80/path` is treated differently by each version — engine.rs normalizes to `http://example.com/path`, content.rs leaves the port. This inconsistency can cause deduplication mismatches when URLs traverse both paths.

**Fix:** Use `canonicalize_url_for_dedupe()` (engine.rs version, more complete) as the canonical implementation. Remove `canonicalize_url()` from content.rs or make it call the engine version.

---

## 7. spider_agent Underutilization

Spider_agent is a 100,000+ LOC autonomous agent system with 40+ methods. axon_rust uses only 2 capabilities.

### Currently Used

| Capability | Location | Status |
|------------|----------|--------|
| `Agent::builder().with_search_tavily().build()` + `search_with_options()` | `crates/cli/commands/search.rs:50–56` | ✅ Correct |
| `Agent::builder().with_openai_compatible().with_search_tavily().build()` + `research()` | `crates/cli/commands/research.rs:40–56` | ✅ Correct |

### Available But Unused

| Capability | spider_agent | Opportunity |
|------------|-------------|-------------|
| `RemoteMultimodalEngine` (vision extraction) | `spider_agent/src/extract.rs` | 🔵 Could enhance `extract.rs` with vision fallback for complex layouts |
| `Agent::extract()` / `Agent::extract_structured()` | `spider_agent/src/agent.rs` | 🔵 Replaces custom `DeterministicExtractionEngine` entirely |
| `AgentMemory` (persistent state) | `spider_agent/src/memory.rs` | 🔵 Enables multi-step research workflows with context retention |
| `Agent::navigate()`, `click()`, `type_text()` | `spider_agent/src/agent.rs` | 🔵 Browser automation for form-based data extraction |
| `Agent::fetch()` with built-in caching | `spider_agent/src/agent.rs` | 🔵 Alternative to custom `fetch_html()` with caching layer |
| Multiple search providers (Serper, Brave, Bing) | `spider_agent` features | 🔵 Feature-gate alternatives to Tavily for user choice |
| `TimeRange` filtering for search | `spider_agent/src/search.rs` | 🔵 Expose `--search-time-range` flag on `search` command |
| `ResearchOptions::with_depth()` | `spider_agent/src/research.rs` | 🟡 Expose `--research-depth` flag (currently uses default depth) |
| `custom_tool_*()` registry | `spider_agent/src/tools.rs` | 🔵 Not needed now but enables future agent customization |

### Key Note on `extract.rs`

`crates/cli/commands/extract.rs` implements a custom `DeterministicExtractionEngine` instead of using `spider_agent::Agent::extract()`. This is an **intentional architectural choice** (deterministic + lightweight vs. LLM-powered + vision), but it should be documented as such. Spider_agent's extraction is more powerful on complex/visual layouts and is the official first-class API.

---

## 8. Architecture Observations

### 8.1 ✅ Correct: subscribe() Before crawl_raw() Pattern

**Location:** `crates/cli/commands/scrape.rs:115–129`

axon_rust correctly calls `website.subscribe(16)` before `crawl_raw()`, spawning a separate collector task. This is the exact pattern in `spider/examples/chrome.rs:17–23`. Calling subscribe *after* crawl would miss pages due to the broadcast channel's non-buffering behavior.

This demonstrates deep, correct understanding of spider's channel semantics.

---

### 8.2 ✅ Correct: Two-Layer SSRF Defense

**Layer 1:** `validate_url()` — `crates/core/http.rs:43–115`
Blocks private IPs, loopback, `.internal`/`.local` TLDs before any spider invocation.

**Layer 2:** `ssrf_blacklist_patterns()` → `with_blacklist_url()` — `crates/crawl/engine.rs:398–402`
Regex patterns passed to spider's native blacklist, blocking private IP ranges discovered *during* crawl.

This defense-in-depth is correct. Spider has no built-in SSRF guard.

---

### 8.3 ✅ Correct: Readability Disabled

**Location:** `crates/core/content.rs:31`
```rust
readability: false,
main_content: true,
```

Empirically validated: Mozilla Readability scored VitePress/documentation layouts as low-quality and stripped pages to just titles, causing 97% thin rate. With `readability: false`, thin rate dropped to 13% on the same site. This is a correct, evidence-based deviation from spider_transformations' default.

---

### 8.4 ✅ Correct: Batch Uses Direct HTTP

**Location:** `crates/cli/commands/batch.rs`

`batch.rs` uses `reqwest` directly instead of spider instances for batch URL scraping. This is a pragmatic choice: spider adds link extraction and crawl-state overhead that is unnecessary when fetching individual, pre-specified URLs. Direct HTTP is 3–5x lower overhead per URL.

---

### 8.5 🟠 Observation: Direct Config Mutation Pattern Not Utilized

Spider allows direct field mutation on `website.configuration`:
```rust
website.configuration.respect_robots_txt = true;
website.configuration.delay = 15;
```

axon_rust exclusively uses the `.with_*()` fluent builder. This is fine stylistically, but the direct mutation pattern could be used to set configuration from the `Config` struct more efficiently in `configure_website()` — avoiding the per-field `with_*()` call overhead when setting many fields at once.

---

### 8.6 🟡 Observation: `with_retry()` Clamp Is Necessary

**Location:** `crates/crawl/engine.rs:215`
```rust
website.with_retry(cfg.fetch_retries.min(u8::MAX as usize) as u8)
```

This is necessary because spider's `with_retry()` accepts `u8` while axon_rust's Config uses `usize`. Not a bug, but documentation should note this is a spider API limitation.

---

## 9. Intentional Deviations (Justified)

The following are deviations from spider patterns that have been reviewed and found to be **correct architectural decisions**:

| Deviation | Rationale | Status |
|-----------|-----------|--------|
| Custom SSRF guard in `http.rs` | Spider has no SSRF protection; required for security | ✅ Keep |
| `readability: false` in content transforms | Empirically validated: prevents docs from being stripped | ✅ Keep |
| Batch uses raw HTTP not spider | Avoids per-URL crawl overhead; links not needed | ✅ Keep |
| `canonicalize_url_for_dedupe()` extends spider's normalize | Spider doesn't strip default ports; needed for deduplication | ✅ Keep |
| Path prefix boundary detection in `is_path_prefix_excluded()` | Prevents `/developer` from matching `/de/`; spider regex doesn't do this | ✅ Keep |
| Custom extract engine instead of spider_agent | Deterministic + no LLM required for simple schemas; trade-off documented | ✅ Keep (document) |
| `with_tld(false)` hardcoded | Security-by-default; TLD expansion is a security risk | ✅ Keep |
| `with_retry()` clamped to u8 | Spider API limitation; usize → u8 cast is correct | ✅ Keep |

---

## 10. Prioritized Recommendations

### 🔴 Priority 1 — Fix Now (Bugs/Contract Violations)

1. **Add `.build()?` before `crawl_raw()` in HTTP mode** — **Status: FIXED** ✅
   - File: `crates/crawl/engine.rs:309`
   - Action: Call `.build()?` at end of `configure_website()` or at callsite
   - Risk of not fixing: Silent configuration validation failures on invalid domains
   - *Fixed: `.build()` moved to end of `configure_website()` — all render modes now call it.*

2. **Fix Chrome fallback to not pass raw URL to spider** — **Status: FIXED** ✅
   - File: `crates/crawl/engine.rs:244–245`
   - Action: Return `Err(...)` when CDP resolution fails rather than falling back to raw string
   - Risk of not fixing: Chrome connection failures with misleading error messages
   - *Fixed: `cdp_discovery_url(remote_url).ok_or_else(|| format!("Cannot resolve Chrome CDP endpoint..."))` — errors out with actionable message instead of handing raw URL to spider.*

### 🟠 Priority 2 — Config Exposure (Hardcoded Values)

3. **Expose `chrome_network_idle_timeout_secs` in Config** — **Status: FIXED** ✅
   - File: `crates/crawl/engine.rs:253–254`, `crates/core/config/types.rs`
   - Action: Add field (default: 15), expose as `--chrome-network-idle-timeout-secs`
   - *Fixed: `chrome_network_idle_timeout_secs: u64` (default: 15) added to Config; wired via `--chrome-network-idle-timeout`.*

4. **Expose auto-switch thresholds in Config** — **Status: FIXED** ✅
   - File: `crates/crawl/engine.rs:293, 301`, `crates/core/config/types.rs`
   - Action: Add `auto_switch_thin_ratio: f64` (default: 0.60) and `auto_switch_min_pages: usize` (default: 10)
   - *Fixed: Both fields added to Config; exposed as `--auto-switch-thin-ratio` and `--auto-switch-min-pages`. Broadcast buffer min/max also promoted from hardcoded to profile-driven Config fields.*

5. **Add WebDriver pre-flight connectivity check** — **Status: FIXED** ✅
   - File: `crates/crawl/engine.rs:259–274`
   - Action: Validate WebDriver URL is reachable before `configure_website()`
   - *Fixed: 3-second HTTP pre-flight check added; returns `Err(...)` with actionable message if WebDriver unreachable.*

### 🟡 Priority 3 — Code Quality

6. **Deduplicate `is_excluded_url_path()`** — **Status: FIXED** ✅
   - Files: `crates/crawl/engine.rs:49–76`, `crates/core/content.rs:195–218`
   - Action: Consolidate to engine.rs version (or shared `url_utils.rs`); update callsites in `crates/cli/commands/`
   - *Fixed: `engine.rs` now imports `is_excluded_url_path` from `crates::core::content`. Local duplicate removed. `is_path_prefix_excluded` (internal to engine.rs) kept in place.*

7. **Deduplicate `canonicalize_url()`** — **Status: FIXED** ✅
   - Files: `crates/crawl/engine.rs:29–47`, `crates/core/content.rs:220–228`
   - Action: Use `canonicalize_url_for_dedupe()` everywhere; remove content.rs version
   - *Fixed: `canonicalize_url()` in content.rs upgraded to include default port stripping (:80/:443). `canonicalize_url_for_dedupe()` removed from engine.rs. engine.rs imports `canonicalize_url` from content.rs. 4 new tests added to content.rs.*

8. **Clarify sitemap toggle flow** — **Status: DEFERRED** 🔵
   - File: `crates/crawl/engine.rs:279, 404`
   - Action: Add comment or restructure `with_ignore_sitemap` to set conditionally upfront
   - *Not implemented in this sprint — low risk, cosmetic clarity only.*

### 🔵 Priority 4 — Spider API Adoption (Nice to Have)

9. **Expose `with_whitelist_url()`** — allow-list for stricter crawl scope control — **Status: FIXED** ✅
   - *Fixed: `url_whitelist: Vec<String>` added to Config; wired via `--url-whitelist <regex>` (repeatable). Live test confirmed: 30 pages crawled, 0 non-book URLs.*
10. **Expose `with_block_assets()`** — significant crawl speed improvement possible — **Status: FIXED** ✅
    - *Fixed: `block_assets: bool` added to Config; wired via `--block-assets`.*
11. **Expose `with_max_page_bytes()`** — prevent large binary file downloads — **Status: FIXED** ✅
    - *Fixed: `max_page_bytes: Option<u64>` added to Config; wired via `--max-page-bytes`.*
12. **Expose `with_redirect_policy("strict")`** — stricter redirect handling option — **Status: FIXED** ✅
    - *Fixed: `redirect_policy_strict: bool` added to Config; wired via `--redirect-policy-strict`.*
13. **Expose `with_wait_for_selector()`** — wait for DOM element before capture — **Status: FIXED** ✅
    - *Fixed: `chrome_wait_for_selector: Option<String>` added to Config; wired via `--chrome-wait-for-selector`.*
14. **Expose `with_screenshot()`** — screenshot already referenced in comments; wire to Config — **Status: FIXED** ✅
    - *Fixed: `chrome_screenshot: bool` added to Config; wired via `--chrome-screenshot`.*
15. **Expose `with_connect_timeout_ms()` separately from read timeout** — independent tuning — **Status: DEFERRED** 🔵
    - *Not implemented — deferred to future sprint.*

### 🔵 Priority 4 — spider_agent Utilization

16. **Document `extract.rs` trade-off** — Add comment explaining why `DeterministicExtractionEngine` is used instead of `spider_agent::Agent::extract()` — **Status: FIXED** ✅
    - *Fixed: Design note comment added to `crates/cli/commands/extract.rs` explaining deterministic vs LLM-powered trade-off.*
17. **Expose `--research-depth` flag** — map to `ResearchOptions::with_depth()` — **Status: PARTIAL** 🟡
    - *Partial: `research_depth: Option<usize>` field added to Config. `ResearchOptions::with_depth()` not available in pinned spider_agent version. TODO comment added for when spider_agent is upgraded.*
18. **Expose `--search-time-range` flag** — map to `TimeRange` in search options — **Status: FIXED** ✅
    - *Fixed: `search_time_range: Option<String>` added to Config; wired via `--search-time-range`. `TimeRange` enum matched in search.rs. Live test confirmed: 9 results returned, time-filtered.*
19. **Feature-gate alternative search providers** — Serper, Brave, Bing alongside Tavily — **Status: DEFERRED** 🔵
    - *Not implemented — deferred to future sprint.*

---

## Appendix: Files Reviewed

### axon_rust
```
crates/cli/commands/scrape.rs
crates/cli/commands/crawl.rs
crates/cli/commands/map.rs
crates/cli/commands/batch.rs
crates/cli/commands/extract.rs
crates/cli/commands/search.rs
crates/cli/commands/research.rs
crates/cli/commands/ask.rs
crates/cli/commands/query.rs
crates/cli/commands/retrieve.rs
crates/cli/commands/embed.rs
crates/cli/commands/common.rs
crates/crawl/engine.rs
crates/core/http.rs
crates/core/content.rs
crates/core/config/types.rs
crates/core/config/parse/mod.rs
crates/core/logging.rs
crates/core/ui.rs
crates/core/health.rs
crates/jobs/common.rs
crates/jobs/crawl_jobs/mod.rs
crates/jobs/crawl_jobs/runtime/mod.rs
```

### spider
```
spider/src/website.rs         (~11,437 lines — all builder methods)
spider/src/page.rs            (~6,073 lines — Page struct)
spider/src/configuration.rs   (~2,216 lines — Configuration struct)
spider/examples/              (60+ examples, all reviewed)
spider_agent/src/lib.rs
spider_agent/src/agent.rs
spider_agent/src/search.rs
spider_agent/src/research.rs
spider_agent/src/extract.rs
spider_agent/src/memory.rs
spider_agent/src/tools.rs
```
