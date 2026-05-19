# Spider Pattern Audit — axon_rust CLI Commands
**Date:** 2026-02-22
**Branch:** perf/command-performance-fixes
**Method:** 4-agent parallel investigation (spider-mapper × core-cmd-auditor × crawl-cmd-auditor × ingest-cmd-auditor)
**Spider ref:** `~/workspace/spider` (examples/, spider/src/, spider_cli/src/)

---

## TL;DR

| Severity | Count | Summary |
|----------|-------|---------|
| 🔴 **BROKEN** | 2 | sync_crawl.rs won't compile — 2 undefined function references |
| 🟠 **CRITICAL** | 1 | engine.rs calls `persist_links()` — undocumented internal Spider API |
| 🟡 **OPPORTUNITY** | 3 | scrape.rs bypasses Spider; map.rs duplicates Spider APIs; extract.rs reinvented Spider's LLM extraction |
| 🟢 **GAP (low)** | 4 | Missing `with_budget()`, `crawl_smart()`, `subscribe_guard()`, per-page metadata |
| ✅ **ALIGNED** | 11 | All remaining commands correct or outside Spider's scope |

---

## Part 1 — Spider API Inventory (Reference)

Spider exposes ~80 `with_*()` builder methods on `Website` and ~60 `Configuration` fields. Key surfaces audited:

### Core Configuration (selection — full list in spider/src/configuration.rs)

| Field / Builder | Type | Default | Purpose |
|-----------------|------|---------|---------|
| `with_depth(usize)` | usize | 25 | Max crawl depth |
| `with_delay(u64)` | u64 ms | 0 | Polite crawl delay |
| `with_limit(u32)` | u32 | — | Global page cap (shorthand for `budget("*", n)`) |
| `with_budget(HashMap<&str, u32>)` | map | None | **Per-path** page caps (`"/api" => 50, "*" => 500`) |
| `with_concurrency_limit(Option<usize>)` | option | CPU-based | Concurrent request cap |
| `with_subdomains(bool)` | bool | false | Include subdomain crawling |
| `with_tld(bool)` | bool | false | Include all TLDs |
| `with_respect_robots_txt(bool)` | bool | false | Honor robots.txt |
| `with_blacklist_url(Option<Vec<T>>)` | vec | None | Skip URLs (literal or regex) |
| `with_whitelist_url(Option<Vec<T>>)` | vec | None | Allow-only URLs |
| `with_request_timeout(Option<Duration>)` | option | 120s | Per-request timeout |
| `with_retry(u8)` | u8 | 0 | Retry on failure |
| `with_proxies(Option<Vec<String>>)` | vec | None | Proxy list (auto-rotates) |
| `with_headers(Option<HeaderMap>)` | option | None | Custom HTTP headers |
| `with_user_agent(Option<&str>)` | option | auto | Custom User-Agent |
| `with_normalize(bool)` | bool | false | Dedup trailing-slash URLs |
| `with_block_assets(bool)` | bool | true | Block non-HTML resources |
| `with_return_page_links(bool)` | bool | false | Include per-page links in subscription |
| `with_ignore_sitemap(bool)` | bool | false | Skip sitemap discovery |
| `with_sitemap(Option<&str>)` | option | None | Custom sitemap.xml URL |
| `with_chrome_connection(Option<String>)` | option | None | Remote CDP URL |
| `with_stealth(bool)` | bool | false | Anti-bot fingerprint evasion |
| `with_wait_for_idle_network0(...)` | struct | None | Chrome: wait for network idle |
| `with_wait_for_selector(...)` | struct | None | Chrome: wait for CSS selector |
| `with_webdriver_config(...)` | option | None | Selenium/WebDriver |
| `with_screenshot(...)` | option | None | Screenshot capture config |

### Crawl Methods

```rust
website.crawl().await          // HTTP or Chrome based on config
website.crawl_raw().await      // Always HTTP (never Chrome)
website.crawl_smart().await    // Internal thin-page fallback (auto HTTP→Chrome)
website.scrape().await         // Collect page content (HTTP)
website.scrape_raw().await     // Always HTTP scrape
website.crawl_sitemap().await  // Discover + crawl sitemap URLs only
```

### Subscription Pattern

```rust
// Stream pages as they're crawled (non-blocking)
let mut rx = website.subscribe(buffer_size)?;
tokio::spawn(async move {
    while let Ok(page) = rx.recv().await {
        process(page.get_url(), page.get_html());
    }
});
website.crawl().await;
website.unsubscribe();
```

### Page API

```rust
page.get_url()         -> &str
page.get_url_parsed()  -> Option<&url::Url>
page.get_html()        -> &str
page.get_html_bytes()  -> &[u8]
page.get_content()     -> &str        // extracted main content
page.get_title()       -> &Option<String>
page.get_description() -> &Option<String>
page.get_links()       -> &Vec<String>
page.status_code       // HTTP status (StatusCode)
page.headers           // Response headers (feature: "headers")
page.duration_elapsed  // Load time (feature: "time")
```

---

## Part 2 — Command-by-Command Audit

### `scrape.rs`

**What we do:** Manual HTTP fetch via `reqwest::Client` → `to_markdown()` transformation. Spider is not used.

**What Spider provides:**
```rust
// Spider scrape pattern (examples/scrape.rs)
let mut website = Website::new(url);
website.configuration.user_agent = Some(Box::new("agent".into()));
website.configuration.request_timeout = Some(Box::new(Duration::from_secs(30)));
website.scrape().await;
for page in website.get_pages().unwrap().iter() {
    println!("{}", page.get_html());  // or page.get_content()
}
```

**Gaps:**

| What Spider Has | What We Use | Impact |
|-----------------|-------------|--------|
| Built-in retry (`retry: u8`) | Manual retry loop | Low — ours works |
| Proxy rotation (`proxies: Vec<RequestProxy>`) | Single proxy option | Low — single proxy sufficient |
| Charset/encoding auto-detection | `to_markdown()` raw | Medium — broken encoding on some sites |
| HTTP status code on page | Discarded | Medium — can't distinguish 404 vs content |
| Cache support (`with_caching(true)`) | None | Low — would help repeated scrapes |
| User-Agent via config | Passed via builder | ✓ Actually aligned |

**Spider's `with_limit(1).scrape()` alternative:** For single-URL scrape, `Website::new(url).with_limit(1).scrape().await` then `get_pages()[0]` would give us full Spider infrastructure for free (retry, proxy, encoding, headers). Tradeoff: less control over the HTTP layer.

**Verdict:** 🟡 OPPORTUNITY — scrape.rs bypasses Spider entirely. Current impl works but misses encoding auto-detection and status code capture. Consider switching to `website.scrape()` for single-URL case.

---

### `map.rs`

**What we do:** `crawl_and_collect_map()` from our engine + manual sitemap discovery via `discover_sitemap_urls_with_robots()` + dedup + Chrome fallback if HTTP finds zero pages.

**Spider native equivalent:**
```rust
// Spider already provides this (examples/sitemap_only.rs)
website.crawl_sitemap().await;
let links = website.get_all_links_visited().await;

// Or combined: crawl + sitemap
website.crawl().await;
website.crawl_sitemap().await;
let all_links = website.get_all_links_visited().await;
```

**What `crawl_and_collect_map()` does that Spider doesn't:**
- Render-mode switching (HTTP → Chrome fallback on thin pages)
- Thin-page filtering with our custom threshold
- Progress reporting to AMQP/Postgres

**Gaps:**
- We duplicate sitemap URL discovery logic that Spider handles natively
- Our robots.txt Sitemap: directive parsing (in audit/backfill.rs) supplements Spider correctly — Spider doesn't parse `Sitemap:` lines from robots.txt

**Verdict:** 🟡 OPPORTUNITY — map.rs reimplements URL discovery Spider provides. The render-mode switching logic is our value-add. Consider whether `crawl_sitemap()` + `get_all_links_visited()` can replace the inner loop in `crawl_and_collect_map()`.

---

### `search.rs`

**Spider API:** Spider has no web search integration. Search is explicitly outside Spider's scope.

**What we do:** Manual DuckDuckGo HTML scrape + regex link extraction. Correct approach.

**Note:** Spider's `SearchConfig` struct (feature: "search") integrates with Serper/Brave/Bing/Tavily for Spider Cloud users only — not applicable to our self-hosted stack.

**Verdict:** ✅ ALIGNED — custom implementation is correct. No spider patterns to adopt.

---

### `probe.rs` / `doctor.rs` / `debug.rs`

**Spider API:** None. Spider is a web crawler; infrastructure health checks are out of scope.

**What we do:** Custom HTTP probes against each service endpoint. Correct.

**Verdict:** ✅ ALIGNED — outside Spider's scope entirely.

---

### `crawl.rs`

**What we do:** CLI entry point — parses args, builds Config, enqueues AMQP job or runs sync.

**Spider usage:** Indirect (delegates to engine.rs).

**Verdict:** ✅ ALIGNED — command dispatch layer, not responsible for spider patterns directly.

---

### `crawl/sync_crawl.rs` 🔴 BROKEN

**Compiler errors (lines 54, 108):**
```
error[E0425]: cannot find function `run_sitemap_only_crawl` in this scope
error[E0425]: cannot find function `append_sitemap_backfill` in this scope
```

**Root cause analysis:**

`run_sitemap_only_crawl` (line 54):
- Called as a local function but never defined in this file
- Was likely a wrapper that got deleted during refactoring
- The actual function exists in `crates/crawl/engine.rs` as `run_sitemap_only()`
- **Fix:** Import and call `engine::run_sitemap_only()` directly, or add a local wrapper

`append_sitemap_backfill` (line 108):
- Called as `super::audit::append_sitemap_backfill()`
- The audit module only exports `append_robots_backfill` (see `audit/mod.rs:8`)
- Name mismatch from rename during refactoring — the function is `append_robots_backfill`
- **Fix:** Change call site to `super::audit::append_robots_backfill()`

**Spider relevance:** `crawl_sitemap()` is Spider's native sitemap crawl method. The `run_sitemap_only_crawl()` wrapper maps to this. Fix the reference; the underlying Spider call is correct.

**Minimal fix:**
```rust
// Line 54: replace
return run_sitemap_only_crawl(cfg, start_url).await;
// with:
return crate::crates::crawl::engine::run_sitemap_only(cfg, start_url, &cfg.output_dir).await;

// Line 108: replace
super::audit::append_sitemap_backfill(...)
// with:
super::audit::append_robots_backfill(...)
```

**Verdict:** 🔴 BROKEN — 2 undefined function references. Two-line fix.

---

### `crawl/audit/` (backfill, manifest_audit, sitemap)

**Purpose:** Supplements Spider's crawl with:
1. `backfill.rs` — parses `Sitemap:` directives from robots.txt (Spider doesn't do this)
2. `manifest_audit.rs` — compares discovered URLs against a baseline snapshot
3. `sitemap.rs` — sitemap.xml URL discovery helpers
4. `mod.rs` — module exports (`append_robots_backfill` — note: NOT `append_sitemap_backfill`)

**Spider relevance:** Spider discovers `/sitemap.xml` by convention but does NOT parse `Sitemap:` lines from robots.txt. This audit module correctly fills that gap.

**Verdict:** ✅ ALIGNED — Spider-compliant. Fills a real gap in Spider's robots.txt handling.

---

### `engine.rs` — Core Crawl Engine 🟠 CRITICAL

This is where we build the `Website` object and run crawls. Most important file.

#### What we correctly use

```rust
Website::new()
.with_depth()              ✓
.with_subdomains()         ✓
.with_limit()              ✓
.with_respect_robots_txt() ✓
.with_concurrency_limit()  ✓
.with_delay()              ✓
.with_shared_queue()       ✓
.with_blacklist_url()      ✓ (SSRF protection + excludes)
.with_request_timeout()    ✓
.with_chrome_intercept()   ✓
.with_stealth()            ✓
.with_fingerprint()        ✓
.with_chrome_connection()  ✓
.with_wait_for_idle_network0() ✓
.with_webdriver()          ✓
.with_ignore_sitemap()     ✓
.subscribe()               ✓
.crawl_raw()               ✓
.crawl()                   ✓
.unsubscribe()             ✓
.crawl_sitemap()           ✓
.persist_links()           ⚠️  (see critical issue below)
.get_links()               ✓
.get_all_links_visited()   ✓
```

#### 🟠 CRITICAL: `persist_links()` is undocumented internal API

**Our code (engine.rs ~line 160):**
```rust
website.crawl_sitemap().await;
website.persist_links();   // ← not in public docs
```

**The intent:** Carry sitemap-discovered URLs into the main crawl phase.

**The Spider-intended pattern:** Call `crawl_sitemap()` first, then `crawl()`. Spider should automatically merge sitemap links into the main crawl — `persist_links()` should not be needed.

**Risk:** If Spider's internal API changes, this silently breaks. Spider has no stability guarantee on `persist_links()`.

**Fix:** Remove `persist_links()` call. Verify that calling `crawl_sitemap()` then `crawl_raw()` correctly propagates discovered URLs without manual persistence. If not, file an issue with Spider.

```rust
// Before
website.crawl_sitemap().await;
website.persist_links();
website.crawl_raw().await;

// After (try first; verify behavior)
website.crawl_sitemap().await;
website.crawl_raw().await;
```

#### Missing: `with_budget()` for per-path page limits

**Spider example (examples/budget.rs):**
```rust
.with_budget(Some(spider::hashbrown::HashMap::from([
    ("*", 500),       // global cap
    ("/blog", 100),   // /blog/* capped at 100
    ("/api", 20),     // /api/* capped at 20
])))
```

**Our code:** Only `with_limit(cfg.max_pages)` — flat global cap. No per-path budgeting.

**Impact:** Low — most crawls don't need path-specific limits. Worth adding to CLI as `--budget "/path:N,/other:M"`.

#### Missing: `crawl_smart()` — but we reimplemented it

**Spider provides:**
```rust
website.crawl_smart().await;  // internal HTTP→Chrome fallback
```

**Our approach:** Manual thin-page detection + retry in `sync_crawl.rs` / `engine.rs`. Functionally equivalent but more explicit. Our implementation gives finer control over fallback threshold.

**Impact:** Low — our reimplementation is defensible. Spider's internal version is less configurable.

#### Missing: `subscribe_guard()` — but broadcast buffer compensates

Spider provides `subscribe_guard()` to prevent page loss on high-throughput crawls by blocking until all subscribed pages are consumed. We use a large broadcast buffer (4096–16k) instead.

**Impact:** None in practice — our buffer size exceeds typical crawl rates.

**Verdict:** 🟠 CRITICAL on `persist_links()`. Everything else GOOD.

---

### `batch.rs`

**Spider API:** Spider has no multi-URL batch crawling. Each `Website` instance is one entry point.

**What we do:** `tokio::task::JoinSet` + semaphore for concurrent multi-URL fetching. Correct.

**Verdict:** ✅ ALIGNED — our batch implementation fills a real gap in Spider.

---

### `extract.rs`

**What we do:** Custom `DeterministicExtractionEngine` + LLM fallback via OpenAI API.

**What Spider provides:**
```rust
// Spider example (examples/openai.rs, openai_multi.rs)
let config = GPTConfigs::new("gpt-4", "Extract key information...", 512);
website.with_openai(Some(config)).crawl().await;
// Each page automatically gets LLM processing during crawl
```

**Gap:** We reinvented extraction. Spider has built-in LLM-during-crawl integration that fires per-page during the crawl subscription loop. Our approach runs extraction as a post-crawl step.

**Spider's advantage:** LLM extraction happens inline during crawl — no separate pass. Supports `openai_multi` (multiple prompts per page), `gemini`, `remote_multimodal`.

**Our advantage:** More control over chunking strategy, TEI vs OpenAI, Qdrant schema, and extraction failure handling.

**Verdict:** 🟡 OPPORTUNITY — significant gap vs Spider's native LLM integration. Worth reviewing `examples/openai.rs` and `examples/openai_multi.rs` to determine if Spider's extraction can replace or augment our engine. Not a blocker — our implementation works.

---

### `embed.rs`

**Spider API:** Spider has no embedding/vector-store integration. This is our responsibility.

**Verdict:** ✅ ALIGNED — outside Spider's scope.

---

### `github.rs` / `reddit.rs` / `youtube.rs` / `ingest_common.rs`

**Spider API relevance:**
- `github.rs` — Uses Octocrab (GitHub API). Spider not applicable.
- `reddit.rs` — Uses OAuth + Reddit API. Spider not applicable. RSS fallback possible via `examples/rss.rs` pattern but not needed.
- `youtube.rs` — Uses `yt-dlp` subprocess. Spider not applicable.
- `ingest_common.rs` — AMQP/Postgres job routing. App-level, outside Spider.

**Verdict:** ✅ ALIGNED — all four are application-layer commands using appropriate non-Spider patterns.

---

### `sessions.rs`

**Purpose:** Ingest Claude/Codex/Gemini session history into the vector store.

**Spider API:** Not applicable — this is a data pipeline, not a web crawler.

**Verdict:** ✅ ALIGNED.

---

### `status.rs`

**Purpose:** Dashboard showing active/pending/completed job queue state.

**Spider API:** Spider provides `website.get_status() -> CrawlStatus` and `website.get_crawl_id()` for in-flight crawl state. We use these correctly where needed. The broader job queue status (AMQP, Postgres) is application-layer.

**Verdict:** ✅ ALIGNED.

---

### `common.rs` (URL parsing utilities)

**What we do:**
- `parse_urls()` — validates + normalizes URL list
- `expand_url_glob_seed()` — expands `{a,b,c}` and `*` glob patterns into URL lists

**Spider API:**
- `spider::url::Url` — we use this correctly for URL parsing
- Spider examples show `url_glob.rs` / `url_glob_subdomains.rs` using Spider's built-in glob expansion

**Spider's url_glob pattern:**
```rust
// examples/url_glob.rs
let website = Website::new("https://rsseau.fr/[0-9].xml");  // Spider handles glob natively
website.crawl().await;
```

**Gap:** Spider supports URL glob natively in `Website::new()` — you pass the glob pattern directly and Spider expands it. We preexpand globs in `common.rs` before passing to Spider.

**Impact:** Low — both approaches work. Spider's native glob may support edge cases ours doesn't.

**Verdict:** ✅ MOSTLY ALIGNED. Minor: Spider has native URL glob that we preempt with our own expansion.

---

### `mod.rs` (CLI dispatch)

**Spider CLI comparison:**

| Pattern | spider_cli approach | Our approach |
|---------|--------------------|--------------|
| Arg structure | Flat `Options` struct | Nested substructs (`CrawlArgs`, `BatchArgs`) |
| Command dispatch | Match on `clap::Subcommand` | Same |
| Global flags | Mixed with subcommand flags | Centralized in `GlobalArgs` |
| Config mapping | Direct field assignment | `Config::from_cli_args()` builder |

**Our approach is superior** — nested substructs give better type safety and CLI help organization. spider_cli's flat struct becomes unwieldy at scale.

**Verdict:** ✅ ALIGNED (and arguably better than spider_cli's approach).

---

### `http.rs` (SSRF Defense)

**What we do:** `validate_url()` blocks private IPs, loopback, link-local before any request.

**Spider's approach:** `with_blacklist_url()` is the crawl-time filter. No pre-request SSRF validation.

**Our approach:** Defense-in-depth — validates at HTTP client level before any Spider call reaches the network. The Spider blacklist and our SSRF validator are complementary layers.

**Verdict:** ✅ ALIGNED — our SSRF validation is a security addition, not a duplication.

---

## Part 3 — Configuration Coverage Matrix

Full mapping of our CLI flags → Spider `with_*()` methods:

| Our CLI Flag | Spider Method | Status |
|-------------|---------------|--------|
| `--max-pages` | `with_limit()` | ✓ Used |
| `--max-depth` | `with_depth()` | ✓ Used |
| `--include-subdomains` | `with_subdomains()` | ✓ Used |
| `--respect-robots` | `with_respect_robots_txt()` | ✓ Used |
| `--delay-ms` | `with_delay()` | ✓ Used |
| `--crawl-concurrency-limit` | `with_concurrency_limit()` | ✓ Used |
| `--request-timeout-ms` | `with_request_timeout()` | ✓ Used |
| `--discover-sitemaps` | `with_ignore_sitemap(!bool)` | ✓ Used (inverted) |
| `--render-mode chrome` | `with_chrome_connection()` | ✓ Used |
| `--render-mode http` | `crawl_raw()` | ✓ Used |
| `--exclude-path-prefix` | `with_blacklist_url()` | ✓ Used |
| (SSRF guard) | `with_blacklist_url()` | ✓ Used (internal) |
| `--fetch-retries` | `with_retry(u8)` | ✗ **NOT EXPOSED** |
| `--proxy` (partial) | `with_proxies(Vec<String>)` | ✗ **NOT USED in engine** |
| — | `with_budget(HashMap)` | ✗ **MISSING** |
| — | `with_tld(bool)` | ✗ **MISSING** |
| — | `with_normalize(bool)` | ✗ **MISSING** |
| — | `with_block_assets(bool)` | ✗ **MISSING** |
| — | `with_headers(HeaderMap)` | ✗ **MISSING** |
| — | `with_proxies(Vec<String>)` | ✗ **MISSING in crawl** |
| — | `with_return_page_links(bool)` | ✗ **MISSING** (low impact) |
| — | `with_redirect_policy(Policy)` | ✗ **MISSING** (low impact) |
| — | `with_redirect_limit(usize)` | ✗ **MISSING** (low impact) |
| — | `with_http2_prior_knowledge(bool)` | ✗ **MISSING** (low impact) |
| — | `with_danger_accept_invalid_certs(bool)` | ✗ **MISSING** (low impact) |
| — | `with_caching(bool)` | ✗ **MISSING** (feature-gated) |
| — | `crawl_smart()` | ✗ **MISSING** (reimplemented) |
| — | `subscribe_guard()` | ✗ **MISSING** (buffer compensates) |

---

## Part 4 — Prioritized Action List

### 🔴 P0 — BROKEN (fix before anything else)

**1. Fix `sync_crawl.rs` compiler errors**

File: `crates/cli/commands/crawl/sync_crawl.rs`

```rust
// Line ~54: undefined `run_sitemap_only_crawl`
// Fix: use engine's public function
use crate::crawl::engine::run_sitemap_only;
// Replace call: run_sitemap_only_crawl(cfg, start_url)
// With: run_sitemap_only(cfg, start_url, &cfg.output_dir)

// Line ~108: undefined `append_sitemap_backfill`
// Fix: correct the name — function is append_robots_backfill
// Replace: super::audit::append_sitemap_backfill(...)
// With: super::audit::append_robots_backfill(...)
```

---

### 🟠 P1 — CRITICAL (fix soon, production risk)

**2. Remove `persist_links()` internal API call**

File: `crates/crawl/engine.rs`

```rust
// Current (undocumented internal API):
website.crawl_sitemap().await;
website.persist_links();     // ← REMOVE THIS
website.crawl_raw().await;

// Correct (Spider-intended pattern):
website.crawl_sitemap().await;
website.crawl_raw().await;   // Spider merges sitemap links automatically
```

Test: After removing `persist_links()`, verify that a crawl with `--discover-sitemaps true` still correctly backfills sitemap URLs. If not, open an issue with Spider — this is their bug.

---

### 🟡 P2 — OPPORTUNITY (high value, evaluate carefully)

**3. Evaluate Spider's native LLM extraction for `extract.rs`**

Spider examples to read: `examples/openai.rs`, `examples/openai_multi.rs`, `examples/remote_multimodal.rs`

Spider's inline LLM extraction:
```rust
let config = GPTConfigs::new("model", "Extract structured data: ...", 512);
let mut website = Website::new(url).with_openai(Some(config)).build()?;
let mut rx = website.subscribe(16)?;
website.crawl().await;
// Each page in rx has LLM-processed content attached
```

**Decision criteria:**
- If our extraction schema maps cleanly to Spider's GPTConfigs prompting → migrate
- If we need custom chunking, TEI embeddings (not OpenAI), or complex schema validation → keep ours
- If we need per-crawl extraction (not post-hoc) → Spider's inline approach wins

**4. Evaluate `scrape.rs` Spider migration**

Consider replacing manual HTTP + `to_markdown()` with:
```rust
let mut website = Website::new(url)
    .with_limit(1)
    .with_request_timeout(Some(Duration::from_millis(cfg.request_timeout_ms)))
    .with_retry(cfg.fetch_retries as u8)
    .build()?;
website.scrape().await;
let page = website.get_pages().unwrap().first()?;
// page.get_html() / page.get_content() / page.status_code
```

**Win:** Gets encoding detection, retry, status code, headers for free.
**Risk:** Less control over response handling; adds Spider overhead for single-URL case.

**5. Evaluate `map.rs` simplification via Spider APIs**

Replace inner loop in `crawl_and_collect_map()` with:
```rust
website.crawl().await;
website.crawl_sitemap().await;
let links = website.get_all_links_visited().await;
```

Keep our Chrome fallback logic on top. The render-mode switching and thin-page detection are our value-add and should remain.

---

### 🟢 P3 — LOW PRIORITY (nice to have)

**6. Add `with_budget()` support**

Expose via CLI: `--budget "/path:N,*:M"` → parse into `HashMap<&str, u32>` → pass to `with_budget()`.

**7. Add `--fetch-retries` wiring to Spider**

We parse `cfg.fetch_retries` but don't call `with_retry(cfg.fetch_retries as u8)` in engine.rs. Wire it up.

**8. Add `--proxy` support to crawl engine**

Proxy is parsed but not passed to Spider's `with_proxies()` in the crawl path (only in scrape path). Wire to engine.

**9. Expose `with_normalize(bool)`**

Deduplicates trailing-slash variants. Useful for canonical URL handling.

**10. Expose `with_tld(bool)`**

For crawls that need to span all country-code TLDs of a domain.

---

### ✅ NO ACTION NEEDED

| Command | Reason |
|---------|--------|
| search.rs | Spider has no search integration — custom DuckDuckGo scrape is correct |
| probe.rs / doctor.rs / debug.rs | Infrastructure health is outside Spider's scope |
| batch.rs | Spider has no multi-URL batch — our JoinSet approach is correct |
| embed.rs | Spider has no embedding/vector-store — our TEI+Qdrant pipeline is correct |
| github.rs | Octocrab API — Spider not applicable |
| reddit.rs | Reddit OAuth API — Spider not applicable |
| youtube.rs | yt-dlp subprocess — Spider not applicable |
| ingest_common.rs | AMQP/Postgres job routing — application layer |
| sessions.rs | Data pipeline — not a web crawl |
| status.rs | Job queue dashboard — application layer |
| http.rs (SSRF) | Defense-in-depth — complements Spider's blacklist |
| audit/ module | Correctly fills Spider's robots.txt Sitemap: gap |
| common.rs | Spider's native glob exists but our preexpansion is equivalent |

---

## Part 5 — Spider Features We Don't Use (and Shouldn't Need)

Features in Spider that are out of scope for our self-hosted stack:

| Feature | Spider Capability | Why We Skip |
|---------|-------------------|-------------|
| `spider_cloud` | SaaS proxy/unblocker integration | Self-hosted only — no cloud |
| `cron` feature | Scheduled recurring crawls | We use AMQP job queues |
| `gemini` feature | Google Gemini LLM automation | OpenAI-compatible only |
| `cookies` feature | Cookie jar management | Not needed currently |
| `disk` feature | Disk-based state sharing | In-memory + DB is sufficient |
| `hedge` feature | Hedged requests for latency | Performance profile handles this |
| `remote_multimodal` | Vision+text LLM | Out of scope |
| Spider Cloud search | Serper/Brave/Bing/Tavily | DuckDuckGo HTML is sufficient |
| `ClientRotator` | Round-robin proxy rotation | Single proxy sufficient currently |

---

## Summary Scorecard

```
crates/cli/commands/
├── scrape.rs           🟡 OPPORTUNITY  — bypasses Spider; missing encoding/status
├── map.rs              🟡 OPPORTUNITY  — duplicates crawl_sitemap() + get_all_links_visited()
├── search.rs           ✅ ALIGNED      — outside Spider's scope
├── probe.rs            ✅ ALIGNED      — outside Spider's scope
├── doctor.rs           ✅ ALIGNED      — outside Spider's scope
├── debug.rs            ✅ ALIGNED      — outside Spider's scope
├── crawl.rs            ✅ ALIGNED      — dispatch layer
├── crawl/
│   ├── sync_crawl.rs   🔴 BROKEN       — 2 undefined function references
│   └── audit/          ✅ ALIGNED      — correctly fills Spider gaps
├── batch.rs            ✅ ALIGNED      — fills Spider gap (no native batch)
├── extract.rs          🟡 OPPORTUNITY  — reinvented Spider's LLM extraction
├── embed.rs            ✅ ALIGNED      — outside Spider's scope
├── github.rs           ✅ ALIGNED      — API-driven, Spider not applicable
├── reddit.rs           ✅ ALIGNED      — API-driven, Spider not applicable
├── youtube.rs          ✅ ALIGNED      — subprocess-driven, Spider not applicable
├── ingest_common.rs    ✅ ALIGNED      — application layer
├── sessions.rs         ✅ ALIGNED      — data pipeline
├── status.rs           ✅ ALIGNED      — job queue dashboard
├── common.rs           ✅ ALIGNED      — minor glob preexpansion vs Spider native
└── mod.rs              ✅ ALIGNED      — better structure than spider_cli

crates/crawl/
└── engine.rs           🟠 CRITICAL     — persist_links() is internal API; also missing with_budget()
```
