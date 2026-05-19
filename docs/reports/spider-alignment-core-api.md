# Spider API Alignment: Core API Gap Analysis

**Analyst:** core-api-agent
**Date:** 2026-02-19
**Scope:** Spider examples vs. axon_rust crawl/config/sitemap/subscribe patterns
**Files read:**
- Spider examples: `example.rs`, `scrape.rs`, `crawl_extract.rs`, `configuration.rs`, `advanced_configuration.rs`, `depth.rs`, `budget.rs`, `blacklist.rs`, `whitelist.rs`, `url_glob.rs`, `url_glob_subdomains.rs`, `sitemap.rs`, `sitemap_only.rs`, `sitemap_quality_audit.rs`, `queue.rs`, `callback.rs`, `subscribe.rs`, `subscribe_download.rs`, `subscribe_multiple.rs`, `real_world.rs`, `smart.rs`, `loop.rs`, `cron.rs`, `rss.rs`
- axon_rust: `crates/crawl/engine.rs`, `crates/core/config.rs`, `crates/jobs/crawl_jobs.rs`

---

## Table of Contents

1. [Missing Config Fields](#1-missing-config-fields)
2. [Suboptimal Patterns](#2-suboptimal-patterns)
3. [Missing Capabilities](#3-missing-capabilities)
4. [Direct Adoptions](#4-direct-adoptions)
5. [Optimization Opportunities](#5-optimization-opportunities)

---

## 1. Missing Config Fields

### 1.1 `with_tld` — TLD crawl expansion (inconsistently wired)

**Spider API usage (configuration.rs:16-17):**
```rust
let config = Configuration::new()
    .with_tld(true)
```

**axon_rust current (engine.rs:138-139):**
```rust
website.with_depth(cfg.max_depth);
website.with_subdomains(cfg.include_subdomains);
// Include root-domain siblings when crawling from a subdomain
website.with_tld(cfg.include_subdomains);  // ← always mirrors include_subdomains
```

**Gap:** `with_tld` is hardwired to mirror `include_subdomains`. Spider treats these as independent settings. TLD crawl (e.g. `example.co.uk → example.com`) and subdomain crawl are orthogonal features. Axon has no `--tld` flag. Users cannot enable subdomain crawling without also enabling TLD expansion, or vice versa. Config struct has no `tld` field at all.

**Impact:** Medium — affects users crawling from subdomains where TLD expansion is undesirable.

---

### 1.2 `with_redirect_limit` — redirect hop limit

**Spider API usage (advanced_configuration.rs:23):**
```rust
let config = Configuration::new()
    .with_redirect_limit(3)
```

**axon_rust:** No equivalent field in `Config` struct or `GlobalArgs`. Spider defaults to following all redirects; axon has no way to cap redirect chains.

**Impact:** Low-medium — matters for sites that use redirect chains as anti-bot measures.

---

### 1.3 `with_external_domains` — allowed off-domain crawl targets

**Spider API usage (configuration.rs:18-20):**
```rust
.with_external_domains(Some(
    Vec::from(["http://loto.rsseau.fr/"].map(|d| d.to_string())).into_iter(),
))
```

**axon_rust:** No equivalent. `include_subdomains` + `with_tld` cover same-root expansion, but there is no mechanism to whitelist arbitrary external domains that should be followed during a crawl.

**Impact:** Medium — needed for crawling micro-frontend or cross-domain documentation sites.

---

### 1.4 `with_whitelist_url` — allow-only URL filter

**Spider API usage (whitelist.rs:11):**
```rust
website.with_whitelist_url(Some(vec!["/books".into()]));
```

**axon_rust:** Only has `--exclude-path-prefix` (blacklist). No whitelist/allow-only mode. To crawl only `/docs/*` on a large site, users must enumerate every other top-level path as an exclusion, which is impractical.

**Impact:** High — a whitelist is semantically distinct from a blacklist and essential for scoped crawls.

---

### 1.5 `with_budget` — per-path page budget map

**Spider API usage (budget.rs:12-17):**
```rust
let mut website = Website::new("https://rsseau.fr/en")
    .with_budget(Some(spider::hashbrown::HashMap::from([
        ("*", 15),
        ("en", 11),
        ("fr", 3),
    ])))
    .with_limit(15)
```

**axon_rust:** Only has `--max-pages` (global limit). No per-path budget allocation. `with_budget` is a `budget` Cargo feature in spider and would require feature flag enabling, but it is not surfaced at all.

**Impact:** Medium — useful for crawling multi-section sites with uneven content density.

---

### 1.6 `with_user_agent` — custom user agent string

**Spider API usage (example.rs:19):**
```rust
website.configuration.user_agent = Some(Box::new("SpiderBot".into()));
```

**axon_rust:** Has `chrome_user_agent` for Chrome mode only. No HTTP-mode user agent override. Non-Chrome crawls always use spider's default `spider/x.y.z` user agent.

**Impact:** Medium — many sites block non-browser user agents; needed for polite crawling identification.

---

### 1.7 `cron_str` — native cron scheduling

**Spider API usage (cron.rs:10):**
```rust
website.configuration.cron_str = "1/5 * * * * *".into();
let mut runner = run_cron(website).await;
```

**axon_rust:** Has `--cron-every-seconds` (simple interval loop) and `--cron-max-runs`. Does NOT use spider's native `run_cron()` or `cron_str` field. axon implements its own time-based loop instead of delegating to spider's scheduler, which supports full cron expression syntax.

**Impact:** Low-medium — full cron syntax (`0 */6 * * *`) is more expressive than `--cron-every-seconds`.

---

### 1.8 `with_proxies` — proxy list

**Spider API usage (subscribe_multiple.rs:16, real_world.rs:47 — commented):**
```rust
// website2.with_proxies(Some(vec!["http://myproxy.com"]));
// .with_proxies(Some(vec!["http://localhost:8888".into()]))
```

**axon_rust:** Has `chrome_proxy` for Chrome mode. No HTTP-mode proxy support. Config struct contains only `chrome_proxy`.

**Impact:** Medium — proxy rotation is standard for high-volume crawling.

---

### 1.9 `with_sitemap` — custom sitemap path

**Spider API usage (sitemap.rs:17):**
```rust
.with_sitemap(Some("/sitemap/sitemap-0.xml"));
```

**axon_rust (`engine.rs:311-313`):**
```rust
queue.push_back(format!("{scheme}://{host}/sitemap.xml"));
queue.push_back(format!("{scheme}://{host}/sitemap_index.xml"));
queue.push_back(format!("{scheme}://{host}/sitemap-index.xml"));
```

**Gap:** axon hardcodes three well-known sitemap paths. Spider's `with_sitemap()` allows specifying a non-standard sitemap path. axon has no `--sitemap-path` flag.

**Impact:** Medium — sites with custom sitemap paths (e.g. `/sitemaps/main.xml`) are missed.

---

### 1.10 `with_ignore_sitemap` — decouple sitemap from base crawl

**Spider API usage (sitemap.rs:16):**
```rust
.with_ignore_sitemap(true) // ignore running the sitemap on base crawl/scrape methods
```

**axon_rust:** Uses `--discover-sitemaps` bool, which controls axon's own post-crawl backfill logic. This is NOT the same as spider's `with_ignore_sitemap`. axon does NOT pass `with_ignore_sitemap` to the spider `Website` object, meaning spider's internal sitemap fetch runs in addition to axon's custom backfill — potentially doing double work.

**Impact:** Medium — may cause duplicate sitemap fetches during every crawl.

---

### 1.11 `persist_links` — link persistence across crawl phases

**Spider API usage (sitemap.rs:24):**
```rust
website.crawl_sitemap().await;
website.persist_links();  // persist links to next crawl
website.crawl().await;
```

**axon_rust:** Does not use `persist_links()`. The sitemap backfill runs separately after the main crawl completes, using its own reqwest client (not spider's `crawl_sitemap()` method). Links discovered via sitemap do not feed back into spider's link graph.

**Impact:** Medium — spider's integrated approach ensures sitemap links are de-duplicated and depth-tracked properly.

---

## 2. Suboptimal Patterns

### 2.1 Subscribe buffer capacity: 4096 is likely too small for large crawls

**axon_rust (engine.rs:237):**
```rust
let mut rx = website.subscribe(4096).ok_or("subscribe failed")?;
```

**Spider examples (subscribe_download.rs:28):**
```rust
let mut rx2 = website.subscribe(888).unwrap();
```
**(blacklist.rs:14, subscribe.rs:11):**
```rust
website.subscribe(0)  // 0 = use broadcast default capacity
```

**Gap:** Spider uses `0` (default) or small values for interactive examples, and larger values (888) only for download-heavy workloads. axon uses 4096 uniformly. The `subscribe()` argument is the broadcast channel capacity. A capacity of 4096 will cause `RecvError::Lagged` drops when the crawl produces pages faster than the consumer processes them, which axon already handles but silently discards pages. For very large crawls (>10k pages), the consumer loop will lag and miss pages. Spider's crawl_raw() in loop.rs uses `subscribe(0)` which uses the channel's default.

**Impact:** Medium — pages may be silently dropped under high-throughput crawls.

---

### 2.2 Chrome mode calls `crawl()` instead of `crawl_raw()` for HTTP

**axon_rust (engine.rs:288-291):**
```rust
match mode {
    RenderMode::Http => website.crawl_raw().await,
    RenderMode::Chrome | RenderMode::AutoSwitch => website.crawl().await,
}
```

**Spider examples (loop.rs:39, crawl_extract.rs:145):**
```rust
website.crawl_raw().await;  // always use crawl_raw() when chrome is not needed
```

**Spider docs note:** `crawl()` with Chrome feature compiled in expects a Chrome connection. When Chrome is unavailable, it may silently fall back or error. `crawl_raw()` is pure HTTP and always works. For `AutoSwitch` mode when Chrome bootstrap fails, calling `website.crawl()` may behave unexpectedly rather than falling back gracefully to HTTP.

**Impact:** Medium — AutoSwitch may not degrade cleanly when Chrome is unavailable.

---

### 2.3 `CrawlJobConfig` re-implements `Config` fields redundantly

**axon_rust (crawl_jobs.rs:38-65):**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CrawlJobConfig {
    max_pages: u32,
    max_depth: usize,
    include_subdomains: bool,
    // ... 17 more fields mirroring Config
}
```

This is a parallel struct that must be kept in sync with `Config` manually. When new config fields are added (e.g. `with_tld`, `with_redirect_limit`), they must be added to both `Config` AND `CrawlJobConfig`. The spider examples show simple pattern: build `Configuration` once, clone it for reuse (advanced_configuration.rs:19, 33).

**Impact:** Maintenance — every new Spider config field requires updating two structs.

---

### 2.4 Sitemap implemented as custom reqwest client instead of spider's `crawl_sitemap()`

**axon_rust (engine.rs:302-412):** `crawl_sitemap_urls()` is ~110 lines of custom XML parsing + concurrent HTTP fetching.

**Spider native approach (sitemap.rs:19-27):**
```rust
website.crawl_sitemap().await;
website.persist_links();
website.crawl().await;
```

**Gap:** axon reimplements sitemap discovery from scratch with manual XML parsing (`extract_loc_values`), manual concurrency (`JoinSet`), and manual redirect handling. Spider's `crawl_sitemap()` handles all of this natively plus respects `robots.txt`, handles `<sitemapindex>` recursion, and integrates link results into the crawler's dedup set. axon's implementation cannot benefit from spider's link dedup or sitemap protocol improvements.

**Impact:** High — maintenance burden + likely missing edge cases spider's native implementation handles.

---

### 2.5 AutoSwitch triggers a full second crawl with no shared state

**axon_rust (engine.rs:565-600):**
```rust
pub async fn try_auto_switch(...) -> Result<...> {
    // if threshold exceeded, start entirely new crawl from scratch
    match crawl_and_collect_map(cfg, start_url, RenderMode::Chrome).await {
```

**Spider approach:** `crawl_smart()` (smart.rs:18) handles this internally with intelligence about render mode selection.

**Gap:** When AutoSwitch falls back to Chrome, it discards all HTTP crawl results and runs a full Chrome crawl from page 0. There is no partial reuse. The `crawl_smart()` API in spider may do this more efficiently.

**Impact:** Medium — wasted work on AutoSwitch triggers.

---

### 2.6 `on_link_find` callback not used

**Spider API usage (callback.rs:13-17):**
```rust
website.set_on_link_find(move |s, ss| {
    println!("link target: {:?} - {some}", s);
    (s.as_ref().replacen("/fr/", "", 1).into(), ss)
});
```

**axon_rust:** Does not use `set_on_link_find`. Link filtering is done post-crawl via `is_excluded_url_path()` which runs AFTER pages are fetched. Using `on_link_find` to rewrite or reject links BEFORE they are queued would:
1. Prevent fetching excluded pages entirely (not just filtering output)
2. Enable URL normalization before fetching (e.g. strip tracking parameters)
3. Allow dynamic link transformation

**Impact:** Medium — currently excluded pages are still fetched and processed, just discarded.

---

### 2.7 Dynamic queue injection (`queue.rs` pattern) not used

**Spider API usage (queue.rs:13-36):**
```rust
let q = website.queue(100).unwrap();
// In subscriber callback:
let _ = q.send(url.into());  // inject new URLs mid-crawl
g.inc();  // signal guard for backpressure
```

**axon_rust:** The AMQP job queue handles job-level queuing, but axon does not use spider's native URL injection queue. URL seeds are fixed at crawl start. There is no mechanism to inject additional URLs during a crawl based on discovered content.

**Impact:** Low — advanced use case, but enables dynamic seed expansion.

---

## 3. Missing Capabilities

### 3.1 `crawl_smart()` — adaptive render mode selection

**Spider API (smart.rs:18):**
```rust
website.crawl_smart().await;
```
**real_world.rs:61:**
```rust
website.crawl_smart().await;
```

axon has its own `try_auto_switch()` logic but does NOT use spider's `crawl_smart()`. Spider's smart mode likely integrates render decisions more tightly with the crawl loop itself.

---

### 3.2 Native cron scheduling via `run_cron()`

**Spider API (cron.rs:20-24):**
```rust
website.configuration.cron_str = "1/5 * * * * *".into();
let mut runner = run_cron(website).await;
tokio::time::sleep(tokio::time::Duration::from_secs(20)).await;
let _ = tokio::join!(runner.stop(), join_handle);
```

axon uses `--cron-every-seconds` with a manual `tokio::time::sleep` loop. Spider's `run_cron()` supports full cron syntax and provides a runner handle for graceful stop.

---

### 3.3 Multi-website parallel crawl with shared subscriber

**Spider API (subscribe_multiple.rs:14-43):**
```rust
let mut website2 = website.clone();
website2.set_url_only("https://example.com?target=2");
let (mut stdout, _, _) = tokio::join!(sub, c1, c2);
```

axon has no way to crawl multiple sites simultaneously under a single CLI invocation while sharing an output subscriber. Each crawl is sequential or handled by separate AMQP workers.

---

### 3.4 Per-page raw bytes download (subscribe_download pattern)

**Spider API (subscribe_download.rs:51-56):**
```rust
if let Some(b) = page.get_bytes() {
    file.write_all(b).await.unwrap_or_default();
}
```

axon converts every page to markdown via `transform_content_input`. There is no raw bytes/HTML download mode for asset preservation.

---

### 3.5 URL glob patterns for targeted multi-URL seeds

**Spider API (url_glob.rs:10-13):**
```rust
let mut website: Website = Website::new(
    "https://rsseau.fr/blog/{lazy-load-components,gnu-parallel,migrate-from-jekyll-to-gatsby}",
);
```
**(url_glob_subdomains.rs:10-12):**
```rust
Website::new("https://{www,docs}.a11ywatch.com")
    .with_subdomains(true)
```

axon has `--url-glob` flag in config but this is only used for URL seed expansion before creating the `Website` object. The glob is processed by axon, not passed to spider's native glob handling. The subdomain glob pattern (`https://{www,docs}.example.com`) specifically requires spider's native glob support to work correctly with `with_subdomains(true)`.

---

### 3.6 Chrome event tracking (request/response telemetry)

**Spider API (real_world.rs:23-25):**
```rust
let mut tracker = ChromeEventTracker::default();
tracker.responses = true;
tracker.requests = true;
// .with_event_tracker(Some(tracker))
```

axon does not expose `ChromeEventTracker`. No way to record or report Chrome network events per page.

---

### 3.7 Chrome fingerprint spoofing and stealth beyond basic stealth flag

**Spider API (real_world.rs:44-45):**
```rust
.with_fingerprint(true)
.with_return_page_links(true)
```
```rust
.with_headers(Some(HeaderMap::from_iter([(
    REFERER,
    HeaderValue::from_static(spider::spider_fingerprint::spoof_referrer()),
)])))
```

axon exposes `--chrome-stealth` and `--chrome-anti-bot` but not `--chrome-fingerprint`, `--chrome-return-page-links`, or custom header injection.

---

### 3.8 `subscribe_guard` — backpressure mechanism

**Spider API (queue.rs:12, real_world.rs:52-56):**
```rust
let mut g = website.subscribe_guard().unwrap();
g.guard(true);
// In subscriber:
g.inc();  // signals guard to allow next page
```

axon subscriber loops have no backpressure. If the embed/write pipeline is slower than the crawl, the broadcast channel fills and pages are dropped (handled by `RecvError::Lagged` branch but pages are lost).

---

### 3.9 `with_block_assets` and `with_wait_for_idle_network0`

**Spider API (real_world.rs:41-34):**
```rust
.with_block_assets(true)
.with_wait_for_idle_network0(Some(WaitForIdleNetwork::new(Some(Duration::from_millis(3000)))))
.with_wait_for_idle_dom(Some(WaitForSelector::new(
    Some(Duration::from_millis(100)),
    "body".into(),
)))
```

axon has no `--block-assets` flag or DOM-idle wait configuration. These are Chrome-mode features that significantly affect JS-rendered page quality.

---

### 3.10 `with_chrome_connection` — explicit Chrome endpoint

**Spider API (real_world.rs:48):**
```rust
.with_chrome_connection(Some("http://127.0.0.1:9222/json/version".into()))
```

axon has `chrome_remote_url` in config but whether it is passed to `with_chrome_connection` is not verified in engine.rs. The `configure_website()` function does not call `with_chrome_connection()`.

**engine.rs configure_website (lines 169-176):**
```rust
if matches!(mode, RenderMode::Chrome) {
    website
        .with_chrome_intercept(RequestInterceptConfiguration::new(false))
        .with_stealth(true);
    // ← chrome_remote_url from cfg is NEVER passed here
    website = website.build()...
}
```

**Impact:** High — `AXON_CHROME_REMOTE_URL` env var is parsed but never applied to the Website object. Chrome connections always use spider's default Chrome discovery, ignoring the configured remote URL.

---

### 3.11 RSS feed crawling

**Spider API (rss.rs:10-13):**
```rust
Website::new("https://a11ywatch.com/rss")
    .with_limit(5)
    .build()
```

Spider can crawl RSS feeds natively (URLs pointing to RSS/Atom XML). axon has no specific RSS support and does not document this capability.

---

## 4. Direct Adoptions

### 4.1 Native `crawl_sitemap()` + `persist_links()` pattern

**Copy verbatim from sitemap.rs:**
```rust
// Phase 1: crawl sitemap
website.crawl_sitemap().await;
// Persist discovered links for next crawl phase
website.persist_links();
// Phase 2: regular crawl with sitemap seeds extended
website.crawl().await;
```

Replace `crawl_sitemap_urls()` in engine.rs. Eliminates ~110 lines of custom XML parsing.

---

### 4.2 `set_on_link_find` for pre-fetch path filtering

**Copy from callback.rs:**
```rust
website.set_on_link_find(move |s, ss| {
    // Reject excluded paths before fetching
    if is_excluded_url_path(s.as_ref(), &exclude_path_prefix) {
        return (String::new().into(), ss);  // empty URL = skip
    }
    (s, ss)
});
```

Move `is_excluded_url_path` filtering from post-fetch subscriber to pre-fetch link callback. Prevents fetching excluded pages entirely.

---

### 4.3 `subscribe(0)` for default broadcast capacity

**From subscribe.rs:**
```rust
let mut rx2 = website.subscribe(0).unwrap();
```

Replace `website.subscribe(4096)` with `website.subscribe(0)` to use spider's tuned default capacity, avoiding hardcoded buffer that may be too small or too large.

---

### 4.4 `with_whitelist_url` as `--whitelist-path-prefix` flag

**From whitelist.rs:**
```rust
website.with_whitelist_url(Some(vec!["/books".into()]));
```

Add `--whitelist-path-prefix` as complement to `--exclude-path-prefix`. Wire into `configure_website()` alongside the existing blacklist.

---

### 4.5 `with_external_domains` as `--external-domains` flag

**From configuration.rs:**
```rust
.with_external_domains(Some(
    Vec::from(["http://loto.rsseau.fr/"].map(|d| d.to_string())).into_iter(),
))
```

Add `--external-domains <url,...>` global flag. Wire into `configure_website()`.

---

### 4.6 `run_cron()` replacing manual sleep loop

**From cron.rs:**
```rust
website.configuration.cron_str = "1/5 * * * * *".into();
let mut runner = run_cron(website).await;
let _ = tokio::join!(runner.stop(), join_handle);
```

Replace axon's manual `--cron-every-seconds` sleep loop with `website.configuration.cron_str` + `run_cron()`. Expose as `--cron-expr "0 */6 * * *"` flag.

---

### 4.7 `RemoteMultimodalConfigs` for native LLM extraction

**From crawl_extract.rs:85-95:**
```rust
let mut mm = RemoteMultimodalConfigs::new(&api_url, &model);
mm.api_key = Some(api_key);
mm.cfg.extra_ai_data = true;
mm.cfg.include_html = true;
mm.cfg.include_url = true;
mm.cfg.max_rounds = 1;
mm.cfg.request_json_object = true;
mm.cfg.extraction_prompt = Some(format!("{prompt}..."));

let mut website = Website::new(&url)
    .with_limit(limit)
    .with_remote_multimodal(Some(mm))
    .build()?;
```

axon's `extract` command uses a custom `remote_extract.rs` module that makes its own OpenAI HTTP calls per page. Spider's `RemoteMultimodalConfigs` provides native per-page LLM extraction integrated into the crawl loop with subscribe support. Result is available at `page.extra_remote_multimodal_data`. This is the pattern used in `sitemap_quality_audit.rs` and `crawl_extract.rs`.

---

## 5. Optimization Opportunities

### 5.1 Fix: `chrome_remote_url` is parsed but never applied

**Current (engine.rs configure_website, lines 169-176):**
```rust
if matches!(mode, RenderMode::Chrome) {
    website
        .with_chrome_intercept(RequestInterceptConfiguration::new(false))
        .with_stealth(true);
    // BUG: cfg.chrome_remote_url is never passed here
    website = website.build()...
}
```

**Fix:**
```rust
if matches!(mode, RenderMode::Chrome) {
    website
        .with_chrome_intercept(RequestInterceptConfiguration::new(false))
        .with_stealth(true);
    if let Some(ref url) = cfg.chrome_remote_url {
        website.with_chrome_connection(Some(url.clone()));
    }
    website = website.build()...
}
```

This is a functional bug: the `AXON_CHROME_REMOTE_URL` env var is documented, parsed, stored in `Config`, but silently ignored when building the `Website` object.

---

### 5.2 Fix: `with_ignore_sitemap(true)` not passed to spider — double sitemap fetching

**Current:** axon calls `append_sitemap_backfill()` after crawl, AND spider internally fetches sitemaps during `crawl()` / `crawl_raw()` unless explicitly disabled.

**Fix in configure_website():**
```rust
// When axon manages sitemap backfill separately, tell spider not to auto-fetch
if cfg.discover_sitemaps {
    website.with_ignore_sitemap(true);  // axon handles sitemap via append_sitemap_backfill
}
```

---

### 5.3 Add `--tld <bool>` as independent flag

**Current:** `with_tld` mirrors `include_subdomains` (engine.rs:139):
```rust
website.with_tld(cfg.include_subdomains);
```

**Fix:** Add `--tld` to `GlobalArgs` and `Config`. Allow independent control:
```rust
website.with_tld(cfg.tld);
```

---

### 5.4 Replace custom sitemap parsing with `website.crawl_sitemap()`

**Current `crawl_sitemap_urls()` (engine.rs:302-412):** 110 lines of custom XML parsing, JoinSet concurrency, redirect handling, scope filtering.

**Spider native (sitemap.rs):**
```rust
website.crawl_sitemap().await;
website.persist_links();
website.crawl().await;
```

Benefits:
- Spider handles `<sitemapindex>` recursion natively
- Spider respects `robots.txt` for sitemap URLs
- Links integrate into spider's dedup set
- Eliminates `extract_loc_values()` custom XML parser
- Eliminates custom `JoinSet` concurrency management

---

### 5.5 Add `set_on_link_find` pre-fetch filtering

**Current flow:** pages are fetched → received via subscribe → checked in `is_excluded_url_path()` → discarded if excluded. The page was already fetched.

**Optimized flow:**
```rust
let exclude_path_prefix = cfg.exclude_path_prefix.clone();
website.set_on_link_find(move |s, ss| {
    if is_excluded_url_path(s.as_ref(), &exclude_path_prefix) {
        return ("".into(), ss);
    }
    (s, ss)
});
```

Prevents HTTP requests for excluded paths. For a crawl where 30 locale paths are excluded, this avoids fetching potentially thousands of pages.

---

### 5.6 Use `crawl_smart()` instead of manual AutoSwitch logic

**Current (engine.rs:565-600):** `try_auto_switch()` is ~35 lines that detects thin-page ratio and relaunches a full Chrome crawl, discarding all HTTP results.

**Alternative:** Replace with `website.crawl_smart().await` which handles render mode selection internally. Requires investigating what `crawl_smart()` does vs. axon's threshold logic, but the spider implementation likely handles progressive enhancement without full restart.

---

### 5.7 Add `subscribe_guard` for subscriber backpressure

**Current:** `RecvError::Lagged` is caught and continues (engine.rs:250-251):
```rust
Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
```

This silently drops pages. For embed-heavy workloads, the subscriber (embedding pipeline) can lag behind the crawler.

**Fix using spider's guard:**
```rust
let mut g = website.subscribe_guard().unwrap();
// In subscriber loop, after processing each page:
g.inc();  // signal crawler to proceed
```

---

### 5.8 Add `with_user_agent` for HTTP-mode crawls

**Current:** HTTP crawls use spider's default user agent. No CLI flag affects HTTP-mode UA.

**Fix in configure_website() and GlobalArgs:**
```rust
// Add to GlobalArgs:
#[arg(global = true, long, env = "AXON_USER_AGENT")]
user_agent: Option<String>,

// In configure_website():
if let Some(ref ua) = cfg.user_agent {
    website.with_user_agent(Some(ua.as_str()));
}
```

---

### 5.9 Add `with_redirect_limit` to prevent redirect loops

**Fix in configure_website():**
```rust
// Add to Config and GlobalArgs:
#[arg(global = true, long, default_value_t = 5)]
redirect_limit: u8,

// In configure_website():
website.with_redirect_limit(cfg.redirect_limit);
```

---

### 5.10 Use `Configuration::new()` builder pattern for reusable config

**Current (engine.rs):** `configure_website()` mutates a `Website` object field by field using chained `.with_*()` calls on the website.

**Spider pattern (advanced_configuration.rs:18-28):**
```rust
let config = Configuration::new()
    .with_user_agent(Some("SpiderBot"))
    .with_blacklist_url(...)
    .with_subdomains(false)
    .with_redirect_limit(3)
    .with_respect_robots_txt(true)
    .build();

// Reuse across multiple Website instances:
Website::new(url).with_config(config.to_owned()).build()
```

axon's `configure_website()` creates a new `Website` per call and re-applies all settings. The `Configuration` builder pattern allows creating one config struct and reusing it — relevant for batch crawls and the parallel multi-website pattern.

---

## Summary Table

| Gap | Severity | Type | Fix Effort |
|-----|----------|------|------------|
| `chrome_remote_url` never applied (bug) | **Critical** | Bug | Low |
| Double sitemap fetch (`with_ignore_sitemap` missing) | High | Bug | Low |
| `with_whitelist_url` not exposed | High | Missing feature | Medium |
| Native `crawl_sitemap()` not used (reimplemented) | High | Suboptimal | High |
| `RemoteMultimodalConfigs` for extract command | High | Missing capability | High |
| `with_external_domains` not exposed | Medium | Missing feature | Low |
| `with_tld` hardwired to `include_subdomains` | Medium | Config gap | Low |
| `set_on_link_find` not used (pre-fetch filtering) | Medium | Optimization | Medium |
| `with_user_agent` HTTP mode missing | Medium | Config gap | Low |
| `with_redirect_limit` not exposed | Medium | Config gap | Low |
| `subscribe_guard` backpressure absent | Medium | Missing capability | Medium |
| `crawl_smart()` not used | Medium | Missing capability | Medium |
| Custom sitemap XML parser vs native | Medium | Maintenance | High |
| `with_budget` (per-path limits) | Medium | Missing feature | Medium |
| `with_block_assets` + wait-for-idle | Medium | Missing capability | Medium |
| `with_chrome_connection` not wired | Medium | Bug | Low |
| `run_cron()` not used (manual loop) | Low-Med | Suboptimal | Medium |
| `subscribe(0)` vs hardcoded 4096 | Low-Med | Optimization | Low |
| `subscribe_guard` page drops | Low-Med | Bug | Medium |
| `with_proxies` HTTP mode | Low | Missing feature | Low |
| `ChromeEventTracker` not exposed | Low | Missing capability | Low |
| URL glob native support | Low | Optimization | Low |
| RSS native support | Low | Missing capability | Low |
| `Configuration` builder reuse pattern | Low | Optimization | Low |
