# Spider Alignment: Chrome, Caching, Anti-bot, Encoding, and Performance Report

**Date:** 2026-02-19
**Analyst:** chrome-perf-analyst (Haiku 4.5)
**Scope:** READ-ONLY — no files modified
**Methodology:** Read all specified spider examples + spider internals + axon_rust config/engine/Cargo.toml

---

## Table of Contents

1. [Chrome Config Gaps](#1-chrome-config-gaps)
2. [Caching System](#2-caching-system)
3. [Anti-bot / Stealth](#3-anti-bot--stealth)
4. [Concurrent Profiles](#4-concurrent-profiles)
5. [Encoding / Charset](#5-encoding--charset)
6. [WebDriver](#6-webdriver)
7. [Screenshot Capture](#7-screenshot-capture)
8. [Debug Mode](#8-debug-mode)
9. [Performance Benchmarks](#9-performance-benchmarks)
10. [Config Struct Additions](#10-config-struct-additions)

---

## 1. Chrome Config Gaps

### What Spider Exposes

Spider's `Configuration` struct (spider/src/configuration.rs:152–349) has these Chrome-related fields:

```rust
// spider/src/configuration.rs (Chrome fields, all gated on #[cfg(feature = "chrome")])
pub viewport: Option<Viewport>,             // width/height/scale/mobile/landscape/touch
pub stealth_mode: spider_fingerprint::configs::Tier,
pub fingerprint: Fingerprint,               // None / Basic / Advanced
pub chrome_connection_url: Option<String>,  // remote CDP endpoint
pub chrome_intercept: RequestInterceptConfiguration,
pub execution_scripts: Option<ExecutionScripts>,      // per-page JS
pub automation_scripts: Option<AutomationScripts>,    // per-path automation sequences
pub wait_for: Option<WaitFor>,             // WaitForDelay / WaitForIdleNetwork / WaitForSelector
pub screenshot: Option<ScreenShotConfig>,
pub track_events: Option<ChromeEventTracker>,         // requests / responses / automation
pub service_worker_enabled: bool,
pub timezone_id: Option<Box<String>>,
pub locale: Option<Box<String>>,
pub evaluate_on_new_document: Option<Box<String>>,   // custom init script per document
pub dismiss_dialogs: Option<bool>,
pub disable_log: bool,
pub auto_geolocation: bool,
pub bypass_csp: bool,
```

**`Viewport` struct** (chrome_common.rs:162–175):
```rust
pub struct Viewport {
    pub width: u32,
    pub height: u32,
    pub device_scale_factor: Option<f64>,
    pub emulating_mobile: bool,
    pub is_landscape: bool,
    pub has_touch: bool,
}
```

**`RequestInterceptConfiguration`** (chrome_common.rs:949–968):
```rust
pub struct RequestInterceptConfiguration {
    pub enabled: bool,
    pub block_visuals: bool,         // default true
    pub block_stylesheets: bool,     // default true
    pub block_javascript: bool,      // default false
    pub block_analytics: bool,       // default true
    pub block_ads: bool,             // default true
    pub intercept_manager: NetworkInterceptManager,
    pub whitelist_patterns: Option<Vec<String>>,
    pub blacklist_patterns: Option<Vec<String>>,
}
```

**`ChromeEventTracker`** (configuration.rs:68–75):
```rust
pub struct ChromeEventTracker {
    pub responses: bool,
    pub requests: bool,
    pub automation: bool,
}
```

**`WebAutomation`** enum (chrome_web_automation.rs example):
```rust
// Used in automation_scripts map (path -> Vec<WebAutomation>)
WebAutomation::Evaluate(script: String)
WebAutomation::ScrollY(pixels: i32)
WebAutomation::Click(css_selector: String)
WebAutomation::Wait(ms: u64)
WebAutomation::Screenshot { output, full_page, omit_background }
```

**Remote Chrome (chrome_remote.rs / chrome_remote_tls.rs):**
```rust
// HTTP endpoint (DevTools Protocol)
.with_chrome_connection(Some("http://127.0.0.1:9222/json/version".into()))

// WSS endpoint for TLS remote (requires feature chrome_tls_connection)
.with_chrome_connection(Some("wss://your-tls-endpoint".into()))
```

**Sendable mode (chrome_sendable.rs):**
```rust
// Reuse one configured Website instance across multiple URLs without re-init
website.configure_setup().await;
website.crawl_chrome_send(Some(url)).await;
```
This avoids overhead of Chrome session creation per URL — useful for batch operations.

### What axon_rust Currently Uses

From `crates/core/config.rs` and `crates/crawl/engine.rs`:

```rust
// config.rs Config struct — Chrome fields present
pub chrome_remote_url: Option<String>,      // maps to chrome_connection_url — GOOD
pub chrome_proxy: Option<String>,           // PRESENT
pub chrome_user_agent: Option<String>,      // PRESENT
pub chrome_headless: bool,                  // PRESENT
pub chrome_anti_bot: bool,                  // PRESENT — but see §3
pub chrome_intercept: bool,                 // just a bool, not the full struct
pub chrome_stealth: bool,                   // PRESENT
pub chrome_bootstrap: bool,
pub chrome_bootstrap_timeout_ms: u64,
pub chrome_bootstrap_retries: usize,
```

From `engine.rs configure_website()`:
```rust
// Only if RenderMode::Chrome:
website.with_chrome_intercept(RequestInterceptConfiguration::new(false))
       .with_stealth(true);
// That's it — intercept.block_* fields never tuned
// No viewport, no event tracker, no wait_for, no automation_scripts
// No timezone/locale/geolocation, no custom init script
// No fingerprint beyond stealth boolean
```

### Gaps Summary

| Spider Feature | axon_rust Status | Notes |
|---|---|---|
| `viewport` (width/height/mobile/landscape) | **MISSING** | Only `Viewport::default()` (800x600) is used implicitly |
| `fingerprint: Fingerprint` (None/Basic/Advanced) | **MISSING** | stealth=true is set but fingerprint enum never set |
| `wait_for` (delay/idle-network/idle-dom) | **MISSING** | Dynamic pages may be scraped before JS renders |
| `automation_scripts` per path | **MISSING** | No way to scroll, click, or evaluate per URL |
| `execution_scripts` per page | **MISSING** | No per-page JS injection |
| `track_events` (ChromeEventTracker) | **MISSING** | No request/response introspection |
| `evaluate_on_new_document` | **MISSING** | No global init script |
| `timezone_id` / `locale` | **MISSING** | May affect locale-sensitive sites |
| `auto_geolocation` | **MISSING** | Geolocation spoofing |
| `bypass_csp` | **MISSING** | Needed for some CSP-protected sites |
| `dismiss_dialogs` | **MISSING** | Alert dialogs block crawl |
| `disable_log` | **MISSING** | Chrome log noise |
| Sendable mode (`crawl_chrome_send`) | **MISSING** | Reuse Chrome session across batch URLs |
| Remote TLS Chrome (wss://) | config present but only http:// tested | `chrome_tls_connection` feature not in Cargo.toml |
| `intercept.block_javascript` toggle | **MISSING** | Always false, never user-configurable |
| `intercept.whitelist_patterns` | **MISSING** | Fine-grained network filtering not exposed |
| Service worker toggle | **MISSING** | Defaults to enabled in spider |

---

## 2. Caching System

### What Spider Supports

Spider has **four distinct cache modes**, each requiring a different Cargo feature:

| Mode | Feature Flag | Method | How it Works |
|---|---|---|---|
| HTTP disk cache | `cache` or `cache_request` | `.with_caching(true)` | Follows HTTP Cache-Control headers, stores to disk via `cacache`; `CACACHE_MANAGER` is a static you can query directly |
| Chrome+cache hybrid | `cache_chrome_hybrid` | `.with_caching(true)` on Website + Chrome intercept | First pass Chrome, second pass HTTP with cached content returned immediately |
| Remote cache with skip-browser | `chrome_remote_cache` | `.with_caching(true).with_cache_skip_browser(true)` | Returns cached HTML from remote server without launching Chrome at all |
| In-memory (basic) | `cache_request` | Policy via `BasicCachePolicy` | Configurable at config level via `cache_policy: Option<BasicCachePolicy>` |

**Cache example** (cache.rs):
```rust
.with_caching(true)  // enable disk cache
// Access cache directly via static:
spider::website::CACACHE_MANAGER.get(&cache_url).await
// cache_url key format: "GET:https://example.com/page"
```

**Hybrid Chrome example** (cache_chrome_hybrid.rs):
```rust
// First run: Chrome crawl warms cache
website.with_caching(true).with_chrome_intercept(interception);
website.crawl().await;   // Chrome + cache warming

// Second run: HTTP raw, cache serves from disk
website.crawl_raw().await;
```

**Skip-browser example** (cache_remote_skip_browser.rs):
```rust
// Pass 1: warm cache (skip_browser = false)
Website::new(url).with_caching(true).with_cache_skip_browser(false)
// Pass 2: cached return, no browser launch
Website::new(url).with_caching(true).with_cache_skip_browser(true)
// Cached pass duration drops dramatically; bytes_transferred is None on cache hit
```

### What axon_rust Uses

```rust
// config.rs
pub cache: bool,              // PRESENT — maps to with_caching()
pub cache_skip_browser: bool, // PRESENT — maps to with_cache_skip_browser()
```

**BUT**: These are present in the Config struct but `engine.rs configure_website()` does NOT call `.with_caching()` or `.with_cache_skip_browser()` on the Website. The fields exist in Config but are never applied to the crawler.

```rust
// engine.rs configure_website() — missing:
// if cfg.cache { website.with_caching(true); }
// if cfg.cache_skip_browser { website.with_cache_skip_browser(true); }
```

### Gaps Summary

| Feature | axon_rust Status |
|---|---|
| Basic disk cache (cache feature) | Config field present, **never applied in engine.rs** |
| Cache skip-browser | Config field present, **never applied in engine.rs** |
| Chrome-hybrid cache strategy | **MISSING** — no second-pass crawl_raw() |
| Remote cache server integration | **MISSING** — `chrome_remote_cache` feature not in Cargo.toml |
| Cache warm → HTTP fast path | **MISSING** — not implemented as a strategy |
| CACACHE_MANAGER direct access | **MISSING** — no cache introspection in commands |

**Also**: Cargo.toml spider features: `["basic", "chrome", "regex"]`. The `cache` and `chrome_remote_cache` features are not compiled in at all, so even if engine.rs called `.with_caching(true)`, it would be a no-op.

---

## 3. Anti-bot / Stealth

### What Spider Provides

**anti_bots.rs example** demonstrates the full anti-detection stack:

```rust
// 1. Viewport randomization — desktop-class random resolution
let viewport = chrome_viewport::randomize_viewport(&chrome_viewport::DeviceType::Desktop);
// DeviceType variants: Desktop, Mobile, Tablet

// 2. Fine-tuned request interception
let mut interception = RequestInterceptConfiguration::new(true);
interception.block_javascript = false;  // run JS (needed for bot-check pages)
interception.block_stylesheets = false; // allow CSS
interception.block_visuals = false;     // allow images
interception.block_ads = false;         // allow ads
interception.block_analytics = true;   // block analytics

// 3. Realistic wait times
.with_wait_for_delay(Some(WaitForDelay::new(Some(Duration::from_millis(200)))))
.with_wait_for_idle_network(Some(WaitForIdleNetwork::new(Some(Duration::from_millis(2000)))))
.with_wait_for_idle_dom(Some(WaitForSelector::new(
    Some(Duration::from_millis(5000)),
    "body".into(),
)))

// 4. Stealth mode
.with_stealth(true)

// 5. Fingerprint spoofing
.with_fingerprint_advanced(Fingerprint::None)  // or Fingerprint::Basic / Fingerprint::Advanced

// 6. Custom user-agent
.with_user_agent(Some("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) ..."))

// 7. Proxy rotation
.with_proxies(Some(vec!["http://localhost:8888".into()]))

// 8. Block only analytics, allow everything else
interception.block_analytics = true;

// 9. Event tracking for introspection
let mut tracker = ChromeEventTracker::default();
tracker.responses = true;
tracker.requests = true;
```

**Fingerprint enum** (spider_fingerprint crate):
```rust
pub enum Fingerprint {
    None,     // No fingerprint spoofing
    Basic,    // Basic spoofing (canvas, WebGL basic)
    Advanced, // Full spoofing suite
}
```

**Viewport randomization** (`chrome_viewport` feature):
```rust
use spider::features::chrome_viewport;
let viewport = chrome_viewport::randomize_viewport(&chrome_viewport::DeviceType::Desktop);
// Returns a Viewport with randomized but realistic width/height
```

**`real_browser` feature** — enables deeper OS-level fingerprint faking (referenced in anti_bots.rs `--features` comment).

### What axon_rust Uses

```rust
// engine.rs configure_website() for RenderMode::Chrome:
website.with_chrome_intercept(RequestInterceptConfiguration::new(false))
       .with_stealth(true);
// RequestInterceptConfiguration::new(false) = disabled intercept
// with_stealth(true) = stealth mode ON — this is good
// But NO fingerprint, NO viewport randomization, NO wait_for, NO proxy rotation
```

```rust
// config.rs fields that exist but are NOT wired up in engine.rs:
pub chrome_anti_bot: bool,   // field exists, never used to set fingerprint/intercept
pub chrome_proxy: Option<String>,  // field exists, never passed to .with_proxies()
pub chrome_user_agent: Option<String>,  // field exists, never passed to .with_user_agent()
```

### Gaps Summary

| Anti-bot Technique | axon_rust Status |
|---|---|
| Stealth mode | APPLIED in engine.rs — good |
| Viewport randomization | **MISSING** — default 800x600 is detectable |
| Fingerprint spoofing (None/Basic/Advanced) | **MISSING** — fingerprint field never set |
| `real_browser` feature compilation | **MISSING** — not in Cargo.toml features |
| Proxy rotation | Config field exists, **never wired to `.with_proxies()`** |
| Custom user-agent | Config field exists, **never wired to `.with_user_agent()`** |
| `wait_for_delay` (human-like delay) | **MISSING** |
| `wait_for_idle_network` | **MISSING** |
| `wait_for_idle_dom` selector | **MISSING** |
| Fine-grained `block_*` control | **MISSING** — always `new(false)` (disabled) |
| ChromeEventTracker for introspection | **MISSING** |
| Auth challenge response | **MISSING** |

---

## 4. Concurrent Profiles

### What `concurrent_profiles.rs` Demonstrates

This example shows a pattern for **work-stealing across HTTP and Chrome workers simultaneously**:

```rust
// 1. Initial Chrome crawl up to CRAWL_LIMIT pages
website.with_shared_state(true)    // share visited-URL state across instances
       .with_sqlite(true)          // persist visited-URL state to SQLite (disk feature)
       .with_fingerprint(true)     // enable fingerprinting
       .set_disk_persistance(true); // persist links to disk

website.crawl_raw().await;  // first pass: HTTP

// 2. Drain extra discovered links
let extra_links = website.drain_extra_links();

// 3. Round-robin split between HTTP and Chrome workers
let mut set = split_hashset_round_robin(extra_links, 2);
website1.set_extra_links(set.remove(0));  // Chrome worker
website.set_extra_links(set.remove(0));   // HTTP worker
website.set_disk_persistance(false);
website1.set_disk_persistance(false);

// 4. Concurrent HTTP + Chrome pass
tokio::join!(website.crawl_raw(), website1.crawl());
```

Key Spider APIs used:
- `website.with_shared_state(true)` — share visited-URL dedup state across multiple Website instances
- `website.with_sqlite(true)` — requires `disk` feature; persists state to local SQLite
- `website.set_disk_persistance(true/false)` — toggle disk persistence mid-run
- `website.drain_extra_links()` — get discovered but unvisited URLs
- `website.persist_links()` — checkpoint discovered links to disk
- `split_hashset_round_robin(set, n)` — utility from spider::utils to partition URL sets
- `website.setup_database_handler()` / `.generate_pool()` / `.set_pool()` / `.setup_shared_db()` — SQLite shared DB setup

### Applicability to axon_rust Worker Model

axon_rust uses an AMQP job queue (RabbitMQ) for worker coordination — a fundamentally different architecture. The Spider concurrent profiles pattern is designed for in-process parallel crawling, not distributed workers.

**Relevant aspects for axon_rust:**
- `with_shared_state(true)` could help in `run_crawl_once()` to prevent rediscovering already-crawled URLs when running multiple crawl passes (e.g., HTTP → Chrome auto-switch)
- `drain_extra_links()` + round-robin split could replace the current `try_auto_switch()` approach: instead of re-crawling the whole site with Chrome, only Chrome-render the thin/JS-heavy pages
- The SQLite persistence (`disk` feature) is not needed since axon_rust uses Postgres for job state

**Not directly applicable:**
- The multi-proxy, multi-fingerprint concurrent Chrome approach is complex to expose via CLI flags
- The in-process shared state doesn't translate to the AMQP worker model

---

## 5. Encoding / Charset

### What Spider Supports

**`encoding` feature** — explicit charset handling:
```rust
// encoding.rs
res.get_html_encoded("SHIFT_JIS")  // decode HTML bytes with specified charset
```

**`auto_encoding` feature** — automatic charset detection from HTTP headers + HTML meta tags:
```rust
// auto_encoding.rs — same API, but charset detection is automatic
res.get_html()  // returns properly decoded HTML via auto-detected charset
```

Spider's `Page` methods:
- `get_html()` — returns decoded HTML (auto-charset if feature enabled, else UTF-8)
- `get_html_bytes_u8()` — raw bytes
- `get_html_encoded(charset: &str)` — explicit charset decode

### What axon_rust Uses

```rust
// engine.rs crawl_and_collect_map() and run_crawl_once():
let input = TransformInput {
    url: None,
    content: page.get_html_bytes_u8(),  // raw bytes — NO charset handling
    screenshot_bytes: None,
    encoding: None,                      // explicitly None — charset ignored
    selector_config: None,
    ignore_tags: None,
};
```

**axon_rust passes `encoding: None` and `get_html_bytes_u8()` (raw bytes) to spider_transformations.** This is a gap for non-UTF-8 sites (Japanese, Chinese, Korean, legacy European encodings).

### Gaps Summary

| Feature | axon_rust Status |
|---|---|
| `encoding` feature (explicit charset) | **MISSING** — feature not in Cargo.toml |
| `auto_encoding` feature (detect charset) | **MISSING** — feature not in Cargo.toml |
| Charset-aware HTML decode | **MISSING** — always passes `encoding: None` |
| `get_html_encoded(charset)` usage | **MISSING** |

**Impact:** Sites serving Shift-JIS, EUC-JP, GB2312, Latin-1 content will produce garbled markdown. Adding the `auto_encoding` spider feature and passing `encoding: Some(page.charset())` to TransformInput would fix this transparently.

---

## 6. WebDriver

### What Spider Provides

Spider has a complete WebDriver integration (requires `webdriver` and optionally `webdriver_stealth` features):

```rust
// webdriver.rs / webdriver_remote.rs
use spider::features::webdriver_common::{WebDriverBrowser, WebDriverConfig};

let webdriver_config = WebDriverConfig::new()
    .with_server_url("http://localhost:4444")
    .with_browser(WebDriverBrowser::Chrome)  // or WebDriverBrowser::Firefox
    .with_headless(true)
    .with_viewport(1920, 1080)
    .with_timeout(Duration::from_secs(30));

Website::new(url).with_webdriver(webdriver_config).build()
```

**WebDriver vs Chrome DevTools Protocol:**
- WebDriver = Selenium/chromedriver/geckodriver protocol — works with any browser
- CDP (what `.with_chrome_connection()` uses) = Chrome-only, lower-level

**WebDriver screenshot** (webdriver_screenshot.rs):
```rust
use spider::features::webdriver::{
    attempt_navigation, get_page_title, setup_driver_events, take_screenshot,
};
// Direct driver access for custom flows:
website.setup_webdriver().await  // returns Option<WebDriverController>
driver = controller.driver()
attempt_navigation(url, driver, &timeout).await
take_screenshot(driver).await  // returns Vec<u8> PNG
```

**`webdriver_stealth` feature** — auto-configures stealth via `setup_driver_events()`.

### What axon_rust Uses

```rust
// config.rs
pub webdriver_url: Option<String>,  // field exists
```

**But**: Cargo.toml has NO `webdriver` feature in the spider dependency, and `engine.rs` never calls `.with_webdriver()`. The field is dead code.

### Should axon_rust expose WebDriver?

**Recommendation: LOW priority, HIGH complexity.** WebDriver's main use case is multi-browser support (Firefox, Safari via SafariDriver) and Selenium Grid integration. For axon_rust's use case (web crawl → RAG), Chrome CDP mode is superior in performance and stealth. The `webdriver_url` config field should either be wired up properly (with the feature flag) or removed.

---

## 7. Screenshot Capture

### What Spider Provides

**Chrome CDP screenshot** (chrome_screenshot.rs):
```rust
// Method 1: Direct per-page screenshot call
let bytes = page.screenshot(
    true,           // full_page
    true,           // omit_background (PNG transparency)
    CaptureScreenshotFormat::Png,
    Some(75),       // quality (JPEG/WebP)
    Some(output_path),
    None,           // clip viewport
).await;
page.close_page().await;  // REQUIRED after screenshot to release Chrome tab
rxg.inc();                // guard to allow next page (subscribe_guard pattern)
```

**Method 2: Config-based (chrome_screenshot_with_config.rs)**:
```rust
let screenshot_params = ScreenshotParams::new(
    Default::default(),  // clip (ClipViewport)
    Some(true),          // full_page
    Some(true),          // omit_background
);
let screenshot_config = ScreenShotConfig::new(
    screenshot_params,
    true,           // save to disk
    true,           // return bytes
    None,           // output_path (None = use default)
);
Website::new(url).with_screenshot(Some(screenshot_config))
// Access via: page.screenshot_bytes  (Option<Vec<u8>>)
```

**Format options:**
```rust
CaptureScreenshotFormat::Png     // lossless
CaptureScreenshotFormat::Jpeg    // lossy, use quality param
CaptureScreenshotFormat::Webp    // lossy, modern
```

**`chrome_store_page` feature** — required for the `create_output_path()` utility in the example. Not needed if you manage paths yourself.

**`subscribe_guard()`** — A guard channel that must be incremented per processed page when doing Chrome screenshots; prevents the crawl channel from getting too far ahead of screenshot processing.

### What axon_rust Uses

Nothing. No screenshot capability exists.

### Should `cortex screenshot` exist?

**Recommendation: MEDIUM priority, HIGH value for debugging.** A `cortex screenshot <url>` command that:
1. Uses Chrome CDP mode
2. Saves a PNG to the output directory
3. Optionally embeds metadata (URL, timestamp) in EXIF or a sidecar JSON

Would be genuinely useful for debugging thin-page detection, anti-bot verification, and visual confirmation. The Config already has `chrome_remote_url` — screenshot would just need to use it.

**Implementation notes:**
- Requires `chrome` + optionally `chrome_store_page` features
- Must use `subscribe_guard()` pattern to avoid buffer overrun when taking screenshots
- Chrome tab MUST be closed after each screenshot (`page.close_page().await`)
- Full-page screenshots can be large — need to limit concurrent Chrome tabs

---

## 8. Debug Mode

### What Spider's `debug.rs` Exposes

```rust
// debug.rs — spider's entire "debug mode" is env_logger integration
let env = Env::default()
    .filter_or("RUST_LOG", "info")
    .write_style_or("RUST_LOG_STYLE", "always");
env_logger::init_from_env(env);
```

Spider emits structured log entries at DEBUG/INFO/WARN/ERROR via the `log` crate. Setting `RUST_LOG=spider=debug` reveals:
- All HTTP request/response pairs
- CDP command traffic (Chrome mode)
- URL dedup decisions
- Crawl queue state

### What axon_rust Uses

axon_rust uses `tracing` + `tracing-subscriber` (not `env_logger`). The two log frameworks are compatible — `tracing` can forward to `env_logger` via `tracing_log`. Spider's internal `log::*` calls will be captured automatically by axon_rust's tracing subscriber.

However, axon_rust doesn't expose a way to set log level from the CLI (no `--log-level` or `--verbose` flag). Users must set `RUST_LOG` manually.

### Gaps / Recommendations

| Debug Feature | axon_rust Status |
|---|---|
| `RUST_LOG=spider=debug` support | Works via tracing→log bridge but not documented |
| `--verbose` / `--log-level` CLI flag | **MISSING** |
| Chrome event tracker output | **MISSING** (ChromeEventTracker not wired up) |
| Per-request timing output | **MISSING** |
| Crawl progress with bytes transferred | **MISSING** — `page.bytes_transferred` never logged |

**Recommendation:** Add `--log-level <level>` flag that sets `RUST_LOG` programmatically before the tracing subscriber initializes. This is a 5-line change with high diagnostic value.

---

## 9. Performance Benchmarks

### What `remote_multimodal_benchmark.rs` Demonstrates

This is an LLM extraction benchmark (model comparison for `RemoteMultimodalConfigs`), not a network performance benchmark. The benchmark patterns applicable to axon_rust's criterion benchmarks are:

**Measurement pattern:**
```rust
let start = Instant::now();
website.crawl().await;
let duration = start.elapsed();
```

**Results collection:**
```rust
// Access token usage and cost from page.extra_remote_multimodal_data
if let Some(ref ai_data) = page.extra_remote_multimodal_data {
    for result in ai_data {
        let usage = result.usage.as_ref();
        // prompt_tokens, completion_tokens, total_tokens
    }
}
```

**`RemoteMultimodalConfigs` struct** — axon_rust's `extract` command uses an OpenAI-compatible API but not this spider-native struct. Spider can handle vision+LLM extraction natively if the `agent` feature is compiled in.

**Benchmark methodology for axon_rust criterion:**
- Warm-up pass to populate cache
- Multiple timed runs
- Track: pages/second, bytes/second, markdown_chars/page, thin_page_ratio
- Compare HTTP vs Chrome, with/without cache
- axon_rust's existing benchmark at `benches/ask_query_retrieve.rs` benchmarks vector ops, not crawl throughput

### Missing Benchmarks in axon_rust

| Benchmark | Status |
|---|---|
| Crawl throughput (pages/sec, HTTP vs Chrome) | **MISSING** |
| Embed batch throughput (chunks/sec, TEI latency) | **MISSING** |
| Cache warm vs cold crawl comparison | **MISSING** |
| SSRF validation overhead | **MISSING** |

---

## 10. Config Struct Additions

Below is the concrete list of fields to add to axon_rust's `Config` struct, with their types, the spider method they map to, and the Cargo feature dependency.

### Cargo.toml Feature Changes Needed First

```toml
# Current:
spider = { version = "2", default-features = false, features = ["basic", "chrome", "regex"] }

# Recommended additions:
spider = { version = "2", default-features = false, features = [
    "basic",
    "chrome",
    "regex",
    "cache",              # HTTP disk cache (cacache-based)
    "encoding",           # charset detection
    # "chrome_remote_cache",  # remote cache server skip-browser (optional)
    # "real_browser",         # deeper OS-level fingerprint faking (optional, aggressive)
    # "webdriver",            # Selenium WebDriver support (optional)
    # "webdriver_stealth",    # WebDriver stealth mode (optional, requires webdriver)
] }
```

### Config Struct Field Additions

```rust
// === CHROME VIEWPORT ===
/// Viewport width for Chrome renders. None = spider default (800).
/// Spider: .with_viewport(Some(Viewport::new(width, height)))
pub chrome_viewport_width: Option<u32>,

/// Viewport height for Chrome renders. None = spider default (600).
pub chrome_viewport_height: Option<u32>,

/// Emulate mobile device viewport (touch, mobile UA hints).
/// Spider: Viewport { emulating_mobile: true, has_touch: true }
pub chrome_viewport_mobile: bool,

/// Randomize viewport per crawl to avoid fingerprinting.
/// Spider: chrome_viewport::randomize_viewport(&DeviceType::Desktop)
pub chrome_viewport_randomize: bool,

// === CHROME FINGERPRINT ===
/// Fingerprint spoofing level: none | basic | advanced.
/// Spider: .with_fingerprint_advanced(Fingerprint::None/Basic/Advanced)
/// Requires 'chrome' feature.
pub chrome_fingerprint: ChromeFingerprintLevel,  // new enum: None/Basic/Advanced

// === CHROME WAIT-FOR ===
/// Wait for page network to be idle before returning (ms). 0 = disabled.
/// Spider: .with_wait_for_idle_network(Some(WaitForIdleNetwork::new(Some(Duration::from_millis(ms)))))
pub chrome_wait_idle_network_ms: u64,

/// Wait for a CSS selector to appear before returning (ms). Empty = disabled.
/// Spider: .with_wait_for_idle_dom(Some(WaitForSelector::new(Some(timeout), selector)))
pub chrome_wait_selector: Option<String>,
pub chrome_wait_selector_timeout_ms: u64,

/// Fixed delay after page load before returning (ms). 0 = disabled.
/// Spider: .with_wait_for_delay(Some(WaitForDelay::new(Some(Duration::from_millis(ms)))))
pub chrome_wait_delay_ms: u64,

// === CHROME INTERCEPT FINE-TUNING ===
/// Block JavaScript during Chrome crawl (saves bandwidth, may break dynamic pages).
/// Spider: interception.block_javascript = true
/// Default should be false (JS needed for most modern sites).
pub chrome_block_javascript: bool,

/// Block visual resources (images, video) during Chrome crawl.
/// Spider: interception.block_visuals = true (spider default)
/// axon_rust default: true (matches spider default)
pub chrome_block_visuals: bool,

/// Block stylesheets during Chrome crawl.
/// Spider: interception.block_stylesheets = true (spider default)
pub chrome_block_stylesheets: bool,

// === CACHE ===
// NOTE: cache and cache_skip_browser already exist in Config but are never applied in engine.rs
// The fix is to wire them up, not add new fields. But the feature flag must also be added.
// pub cache: bool,             — ALREADY EXISTS, just not applied
// pub cache_skip_browser: bool — ALREADY EXISTS, just not applied

// === ENCODING ===
/// Charset for HTML decode. Empty = auto-detect (requires 'encoding' feature).
/// Spider: page.get_html_encoded(charset) or page.get_html() with auto_encoding
pub html_charset: Option<String>,

// === DEBUG / LOGGING ===
/// Log level override (error|warn|info|debug|trace). Sets RUST_LOG.
/// Spider: enables spider=debug log output
pub log_level: Option<String>,

// === SCREENSHOT ===
/// Take a screenshot of each crawled page (Chrome mode only).
/// Spider: .with_screenshot(Some(ScreenShotConfig::new(...)))
/// Saves to output_dir/screenshots/
pub screenshot: bool,
pub screenshot_full_page: bool,
pub screenshot_format: ScreenshotFormat,  // new enum: Png/Jpeg/Webp

// === WEBDRIVER (low priority) ===
// pub webdriver_url: Option<String>  — ALREADY EXISTS in Config
// Only needs wiring + feature flag if WebDriver support is desired
```

### engine.rs Wiring (apply fields that are already in Config)

Beyond new fields, these **already-existing** Config fields are never applied in `configure_website()`:

```rust
// In configure_website() — missing lines:

// Cache (requires 'cache' feature in Cargo.toml)
if cfg.cache {
    website.with_caching(true);
}
if cfg.cache_skip_browser {
    website.with_cache_skip_browser(true);
}

// Proxy (Config already has chrome_proxy: Option<String>)
if let Some(ref proxy) = cfg.chrome_proxy {
    website.with_proxies(Some(vec![proxy.clone().into()]));
}

// User-agent (Config already has chrome_user_agent: Option<String>)
if let Some(ref ua) = cfg.chrome_user_agent {
    website.with_user_agent(Some(ua.as_str()));
}
```

---

## Summary: Priority Matrix

| Gap | Priority | Effort | Value |
|---|---|---|---|
| Wire `cache`/`cache_skip_browser` in engine.rs | **HIGH** | LOW (2 lines + feature flag) | HIGH — config fields are dead code |
| Wire `chrome_proxy` / `chrome_user_agent` in engine.rs | **HIGH** | LOW (4 lines) | HIGH — config fields are dead code |
| Add `encoding`/`auto_encoding` feature + charset wiring | **HIGH** | LOW (feature + 1 field) | HIGH — garbled output for non-UTF-8 sites |
| `chrome_viewport_width/height/randomize` | **HIGH** | LOW | HIGH — 800x600 viewport is trivially detectable |
| `chrome_wait_idle_network_ms` | **MEDIUM** | LOW | HIGH — dynamic JS pages need this |
| `chrome_fingerprint` level | **MEDIUM** | LOW | HIGH — stealth without fingerprint is partial |
| `--log-level` CLI flag | **MEDIUM** | LOW | MEDIUM — debugging without it requires env var |
| `chrome_block_javascript/visuals/stylesheets` | **MEDIUM** | LOW | MEDIUM — bandwidth savings |
| Screenshot command | **MEDIUM** | MEDIUM | MEDIUM — debugging aid |
| `chrome_wait_selector` | **LOW** | LOW | MEDIUM — niche use case |
| WebDriver wiring | **LOW** | HIGH | LOW — CDP is superior for our use case |
| Remote cache server | **LOW** | HIGH | LOW — external dependency |
| Concurrent profiles (HTTP+Chrome split) | **LOW** | HIGH | MEDIUM — complex in-process pattern |
| Sendable Chrome mode | **LOW** | MEDIUM | LOW — batch already uses AMQP workers |

---

## Appendix: Spider Feature Flag Reference

```
chrome              — CDP-based Chrome crawl (already enabled)
chrome_intercept    — network request interception (currently implicit)
chrome_tls_connection — WSS remote Chrome endpoint
cache               — HTTP disk cache via cacache
cache_request       — in-memory BasicCachePolicy
chrome_remote_cache — remote cache server + skip-browser mode
cache_chrome_hybrid — multi-pass Chrome→HTTP hybrid cache
encoding            — explicit charset decode
auto_encoding       — automatic charset detection from headers/meta
real_browser        — OS-level fingerprint faking (aggressive)
webdriver           — Selenium WebDriver protocol
webdriver_stealth   — WebDriver stealth setup
disk                — SQLite shared state persistence
sitemap             — sitemap.xml integration (basic already covers this)
cron                — cron-based scheduled crawls
regex               — regex blacklist/whitelist (already enabled)
adblock             — ad blocking in Chrome intercept
hedge               — work-stealing hedged requests
search              — web search provider integration
```
