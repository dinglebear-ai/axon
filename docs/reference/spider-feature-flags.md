# Spider.rs Feature Flags

> **Webclaw port capabilities and feature flags** (baseline TLS fingerprinting,
> optional quickjs and social-verticals) are documented in
> [`docs/reference/cargo-features.md`](cargo-features.md) — that file also covers runtime env-var gates.

**Total feature entries tracked in this inventory: 89 (includes the `basic` meta-feature; +6 rows vs. the previous count, added to track spider 2.52.0's `__basic` force-enabled set — see "Transitively Enabled via `basic` → `__basic`" below)**
**Flags enabled in Axon: 33 (spider) + 2 (spider_agent) + spider_transformations (no flags) — spider's 33 = 20 explicitly declared in `crates/axon-crawl/Cargo.toml` + 13 transitively force-enabled via `basic` → `__basic` as of spider 2.52.0 (2 of the 13 — `rate_limit`, `request_coalesce` — compile but have no call sites upstream, i.e. dead code today)**

---

## Active Dependency Declarations

```toml
# crates/axon-crawl/Cargo.toml — axon is a multi-crate Cargo workspace (see root
# CLAUDE.md "Workspace layout (Rust crates)"); the spider dependency is declared in
# the axon-crawl crate, not the root Cargo.toml.
spider = { version = "2", default-features = false, features = [
    "basic", "chrome", "regex", "sitemap", "adblock",
    "chrome_stealth", "chrome_screenshot", "chrome_store_page",
    "chrome_headless_new", "chrome_simd",
    "simd", "inline-more", "cache_mem",
    "ua_generator", "headers", "time", "control",
    "hedge", "etag_cache",
] }

spider_agent       = { version = "2.47.89", default-features = false, features = ["search_tavily", "openai"] }

spider_transformations = "2"  # no feature flags — full crate used as-is
```

> **`firewall` is intentionally NOT enabled.** `spider_firewall`'s build.rs fetches
> blocklists from `api.github.com` unauthenticated and panics when GitHub
> rate-limits the CI runner; it does not read `GITHUB_TOKEN`, so external auth is
> impossible. `validate_url()` in `src/core/http/ssrf.rs` remains the primary SSRF
> guard. See the Cargo.toml comment block and the root `CLAUDE.md` "Spider feature
> flags" section.

**No `#[cfg(feature = "...")]` gates exist anywhere in the local source tree.** All conditional compilation is internal to the spider crates. Feature selection happens entirely at the Cargo.toml level.

---

## Transitively Enabled via `basic` → `__basic` (spider 2.52.0)

> **New in spider 2.52.0.** Spider restructured its `basic` meta-feature so that
> `basic = ["__basic", "basic_tls"]`, and `__basic` itself is defined as: `sync`,
> `cookies`, `ua_generator` (already declared), `encoding`,
> `string_interner_buffer_backend`, `balance`, `real_browser`, `disk_native_tls`,
> `time` (already declared), `adaptive_concurrency`, `priority_frontier`,
> `dns_cache`, `rate_limit`, `request_coalesce`, `auto_throttle`, `etag_cache`
> (already declared), `warc` (already declared). Of the 13 not already in
> `crates/axon-crawl/Cargo.toml`'s explicit list, none were opted into by
> axon-crawl — they ride along with `basic`. Verified against spider 2.52.0's own
> `Cargo.toml` `[features]` table and `cargo tree -p axon-crawl -e features -i
> spider`. See the root `CLAUDE.md` "Spider feature flags with observable
> behavior" section for the CLAUDE.md-side summary.

| Flag | Behaviorally significant? | Notes |
|------|---------------------------|-------|
| `balance` | Not exercised | Previously documented here as "NOT enabled." As of spider 2.52.0 it IS compiled in via `__basic`, but axon still doesn't rely on its throttling — concurrency stays governed by our performance profiles. |
| `cookies` | **Yes** | Spider now attaches a persistent cookie jar per crawl by default. Real, previously-undocumented behavior change — revisit whether axon wants cookie persistence across a crawl. Independently also pulled in by the already-declared `chrome` feature. |
| `real_browser` | **Yes, conditionally** | Changes spider's local-Chrome `CHROME_ARGS` (drops `--no-sandbox`). Only matters on the fallback path where spider launches its own Chrome process — i.e. when `AXON_CHROME_REMOTE_URL` is unset. Production always sets `AXON_CHROME_REMOTE_URL`, so this doesn't bite there. |
| `rate_limit` | No — dead upstream | Compiles `src/utils/rate_limiter.rs` (per-domain token bucket) in spider 2.52.0, but nothing else in the crate calls into it (`rate_limiter::` has no external call sites). Track on future spider bumps in case upstream wires it up. |
| `request_coalesce` | No — dead upstream | Compiles `src/utils/coalesce.rs` (in-flight request dedup) in spider 2.52.0, but nothing else in the crate calls into it (`coalesce::` has no external call sites). Implies `sync`. Track on future spider bumps. |
| `sync` | Not exercised | Also already implied independently by the already-declared `warc` feature. No axon call site. |
| `encoding` | Not exercised | No axon call site; internal to spider's charset handling. |
| `disk_native_tls` | Not exercised | Implies `disk` (`dep:sqlx`) and the `sqlx` crate's own `runtime-tokio-native-tls` feature. axon uses SQLite jobs + Qdrant, not spider's disk-backed cache — the `sqlx` optional dependency is now compiled into the binary but unused by axon code. |
| `priority_frontier` | Not exercised | No axon call site. |
| `dns_cache` | Not exercised | No axon call site. |
| `string_interner_buffer_backend` | Not exercised | Internal string-interning backend selection; no axon call site. |
| `auto_throttle` | Not exercised | Implies `time` (already declared). No axon call site or config wiring. |
| `adaptive_concurrency` | **Yes, opt-in** | Already documented separately in the section below and in root `CLAUDE.md` — axon gates it behind `[workers.adaptive-concurrency] enabled = true`; controller logic stays in `src/crawl/engine/adaptive.rs`. |

`ua_generator`, `time`, `etag_cache`, and `warc` are also members of `__basic`, but
axon already declares them explicitly in `crates/axon-crawl/Cargo.toml` — no
change for those four.

---

## Flags In Use

### spider crate — 20 explicitly declared flags (+ `adaptive_concurrency` via `basic`, 21 rows below)

| Flag | Category | Where Used in Source |
|------|----------|----------------------|
| `basic` | Core | Meta-feature — enables core crawl engine. Used everywhere spider is imported (`src/crawl/engine/`, `src/crawl/engine/collector.rs`, etc.) |
| `regex` | Core | URL blacklist/whitelist pattern matching. Powers `--exclude-path-prefix` and `--url-whitelist` flags in crawl config |
| `sitemap` | Core | `append_sitemap_backfill()` in `src/crawl/engine/`. Drives `--discover-sitemaps` and `--sitemap-since-days` flags before sync inline embed or async dependent embed handoff |
| `simd` | Core | SIMD-accelerated JSON/text parsing. Performance optimization — no direct call site; implicit via spider internals |
| `inline-more` | Core | Aggressive function inlining in spider internals for runtime perf |
| `ua_generator` | Core | Random realistic User-Agent generation per request |
| `headers` | Core | Custom HTTP header injection for crawl requests |
| `time` | Core | Timing/duration tracking for crawl operations |
| `control` | Core | Runtime crawl control — pause/resume/shutdown; crawl cancellation sends Spider shutdown for the active target |
| `hedge` | Core | Hedged duplicate HTTP request for resilience — races a second request after the default 3s delay. Doubles HTTP traffic for pages that take >3s. Used in `src/crawl/engine/runtime.rs` via `HedgeConfig::default()`. |
| `adaptive_concurrency` | Core | Included by Spider's `basic` meta-feature. Axon opts into it only when `[workers.adaptive-concurrency] enabled = true`, keeping controller logic in `src/crawl/engine/adaptive.rs` and attaching the semaphore in `src/crawl/engine.rs`. |
| `chrome` | Chrome / Browser | `RenderMode::Chrome` and `RenderMode::AutoSwitch` paths in `src/crawl/engine/runtime.rs`. Imports `spider::features::chrome_common::{RequestInterceptConfiguration, ScreenShotConfig, ScreenshotParams, WaitForSelector}` |
| `chrome_stealth` | Chrome / Browser | Passed to `spider::website::Website` in `configure_website()` in `src/crawl/engine/`. Enables headless detection evasion |
| `chrome_screenshot` | Chrome / Browser | `ScreenshotParams` usage in `src/crawl/engine/runtime.rs`. Powers screenshot capture during crawls |
| `chrome_store_page` | Chrome / Browser | Retains page object for conditional post-render actions (screenshots, metadata) |
| `chrome_headless_new` | Chrome / Browser | `--headless=new` mode — better DOM fidelity, fewer detection fingerprints |
| `chrome_simd` | Chrome / Browser | SIMD-optimized CDP message parsing for Chrome communication |
| `adblock` | Chrome / Browser | Implicit ad/tracker request filtering during crawl. No local toggle — always active when chrome features are in use |
| `cache_mem` | Caching | In-memory page/request deduplication during crawls. No local call site; spider uses it internally for request memoization |
| `etag_cache` | Caching | Conditional re-crawl. `--etag-conditional` seeds the per-`Website` ETag cache from `etag.json`; spider sends `If-None-Match`/`If-Modified-Since` and skips the body on `304`. Wired in `src/crawl/engine/runtime.rs`; cross-run reconciliation in `src/crawl/engine/etag.rs` (bead axon_rust-hiyf) |
| `warc` | Output | WARC 1.1 archive output. `--warc <path>` calls `website.configuration.with_warc(WarcConfig { .. })` in `src/crawl/engine/runtime.rs` so spider writes every fetched page as a WARC response record. HTTP and Chrome paths both archive. Pulls in `sync` + `headers` (already enabled). |


### spider_agent crate — 2 flags enabled

| Flag | Category | Where Used in Source |
|------|----------|----------------------|
| `search_tavily` | Search | `src/cli/commands/search.rs:4` — `use spider_agent::{Agent, SearchOptions, TimeRange}` (Tavily web search command) · `src/cli/commands/research.rs:4` — same imports · `src/mcp/server/common.rs:9` — `use spider_agent::TimeRange` (MCP TimeRange type) |
| `openai` | AI / LLM | `src/cli/commands/research.rs:4` — `Agent::builder().with_openai_compatible().with_search_tavily(key).build()?.research(ResearchOptions)` — LLM synthesis for the `research` command |

### spider_transformations crate — no feature flags

Used in two files for HTML→Markdown content transformation:
- `src/crawl/engine/collector.rs:6` — `use spider_transformations::transformation::content::{TransformInput, transform_content_input}`
- `src/core/content.rs:14` — `use spider_transformations::transformation::content::{...}`

---

## Full Flag Inventory (all 89, includes `basic` meta-feature)

`✅` = enabled in Axon · `—` = not used

### Core (34)

| Flag | Status | Notes |
|------|--------|-------|
| `ua_generator` | ✅ | Random realistic User-Agent generation per request |
| `regex` | ✅ | URL blacklist/whitelist filtering |

| `glob` | — | NOT enabled — glob URL patterns change `crawl_establish` to use a budget-aware `is_allowed()` check that immediately returns `BudgetExceeded` for the first URL with `with_limit(1)`, producing 0 pages from Chrome crawls. axon does not use URL glob patterns. Do NOT add this flag. See CLAUDE.md gotchas. |

| `fs` | — | Project uses SQLite jobs plus Qdrant vector storage, not spider disk FS |
| `sitemap` | ✅ | Sitemap discovery + backfill |
| `time` | ✅ | Timing/duration tracking for crawl operations |
| `encoding` | ✅ via `__basic` | Transitively enabled by spider 2.52.0's `basic` → `__basic`. No axon call site — see "Transitively Enabled" above. |
| `serde` | — | Project uses its own serde deps directly |
| `sync` | ✅ via `__basic` | Transitively enabled by spider 2.52.0's `basic` → `__basic` (also already implied independently by the already-declared `warc` feature). No axon call site. |
| `control` | ✅ | Runtime crawl control — pause/resume/shutdown. Crawl cancellation sends Spider shutdown for the active crawl target before returning canceled |
| `adaptive_concurrency` | ✅ via `basic` | Opt-in runtime crawl concurrency. TOML-only in Axon; 429, 5xx, and broadcast lag reduce target. No arbitrary decrease-factor or sync-interval knobs until Spider honors them. |
| `full_resources` | — | |
| `cookies` | ✅ via `__basic` | Transitively enabled by spider 2.52.0's `basic` → `__basic` (also independently implied by the already-declared `chrome` feature). Spider now attaches a persistent cookie jar per crawl by default — a real, previously-undocumented behavior change; see "Transitively Enabled" above. |
| `spoof` | — | `chrome_stealth` covers bot-evasion needs |
| `headers` | ✅ | Custom HTTP header injection for crawl requests |
| `balance` | ✅ via `__basic` | Previously "NOT enabled" here. As of spider 2.52.0 it IS transitively compiled in via `basic` → `__basic`, but axon still doesn't rely on it — we manage concurrency ourselves via performance profiles. Silent concurrency throttling with no logging if it were ever wired up. |
| `cron` | — | |
| `tracing` | — | Project uses `tracing` crate directly, not via spider |
| `cowboy` | — | Full concurrency with no throttle — dangerous, prefer `balance` |
| `llm_json` | — | Lenient JSON parsing for LLM output quirks |
| `page_error_status_details` | — | |
| `extra_information` | — | |
| `cmd` | — | tokio process support within spider (axon has its own) |
| `io_uring` | — | |
| `rate_limit` | ✅ via `__basic` (dead upstream) | Transitively enabled by spider 2.52.0's `basic` → `__basic`. Compiles `src/utils/rate_limiter.rs` (per-domain token bucket), but nothing else in spider 2.52.0 calls into it — dead code today. Track on future spider bumps. |
| `request_coalesce` | ✅ via `__basic` (dead upstream) | Transitively enabled by spider 2.52.0's `basic` → `__basic`. Compiles `src/utils/coalesce.rs` (in-flight request dedup), but nothing else in spider 2.52.0 calls into it — dead code today. Implies `sync`. Track on future spider bumps. |
| `priority_frontier` | ✅ via `__basic` | Transitively enabled by spider 2.52.0's `basic` → `__basic`. No axon call site. |
| `dns_cache` | ✅ via `__basic` | Transitively enabled by spider 2.52.0's `basic` → `__basic`. No axon call site. |
| `string_interner_buffer_backend` | ✅ via `__basic` | Transitively enabled by spider 2.52.0's `basic` → `__basic`. Internal string-interning backend selection; no axon call site. |
| `auto_throttle` | ✅ via `__basic` | Transitively enabled by spider 2.52.0's `basic` → `__basic` (implies `time`, already declared). No axon call site or config wiring. |
| `simd` | ✅ | SIMD-accelerated text/JSON parsing |
| `inline-more` | ✅ | Aggressive function inlining in spider internals for runtime perf |

| `hedge` | ✅ | Hedged duplicate HTTP request for resilience — races a second request after the default 3s delay. Doubles HTTP traffic for pages that take >3s. Used in `src/crawl/engine/runtime.rs` via `HedgeConfig::default()`. |

| `warc` | ✅ | WARC 1.1 archive output (`--warc <path>`). Writes every fetched page as a WARC response record via `website.configuration.with_warc()`. Implies `sync` + `headers`. |


### Storage (3)

| Flag | Status | Notes |
|------|--------|-------|
| `disk` | ✅ via `__basic` (`disk_native_tls`) | Transitively enabled by spider 2.52.0's `basic` → `__basic` (via `disk_native_tls`, which lists `disk` as a prerequisite). Project still uses SQLite jobs plus Qdrant vector storage, not spider's disk cache — the code is compiled in but unused. |
| `disk_native_tls` | ✅ via `__basic` | Transitively enabled by spider 2.52.0's `basic` → `__basic`. Also enables the `sqlx` crate's own `runtime-tokio-native-tls` feature. Unused by axon. |
| `disk_aws` | — | Not part of `__basic`; still requires its own explicit opt-in |

### Caching (7)

| Flag | Status | Notes |
|------|--------|-------|
| `cache` | — | |
| `cache_mem` | ✅ | In-memory request deduplication during crawls |
| `etag_cache` | ✅ | Conditional re-crawl (`--etag-conditional`): seeds ETag cache from `etag.json`, 304-skips reconciled in `etag.rs` (bead axon_rust-hiyf) |
| `cache_openai` | — | |
| `cache_gemini` | — | |
| `cache_chrome_hybrid` | — | |
| `cache_chrome_hybrid_mem` | — | |

### Chrome / Browser (17)

| Flag | Status | Notes |
|------|--------|-------|
| `chrome` | ✅ | Chrome headless rendering — required for `RenderMode::Chrome` and `RenderMode::AutoSwitch` |
| `chrome_headed` | — | |
| `chrome_cpu` | — | |
| `chrome_stealth` | ✅ | Headless bot-detection evasion in `configure_website()` |
| `chrome_store_page` | ✅ | Retains page object for conditional post-render actions (screenshots, metadata) |
| `chrome_screenshot` | ✅ | Screenshot capture via `ScreenshotParams` in crawl engine |
| `chrome_intercept` | — | |
| `chrome_headless_new` | ✅ | `--headless=new` mode — better DOM fidelity, fewer detection fingerprints |
| `chrome_simd` | ✅ | SIMD-optimized CDP message parsing for Chrome communication |
| `chrome_tls_connection` | — | |
| `chrome_serde_stacker` | — | |
| `chrome_remote_cache` | — | |
| `chrome_remote_cache_disk` | — | |
| `chrome_remote_cache_mem` | — | |
| `adblock` | ✅ | Implicit ad/tracker blocking during Chrome renders |
| remote local policy API | ✅ via `chrome` | `chrome.remote-local-policy` pushes Spider/Chromey's local interception policy to capable remote Chrome engines for Chrome-rendered crawls only. Generic CDP proxies may reject it; standalone `axon screenshot` is not wired in this release. |
| `real_browser` | ✅ via `__basic` | Transitively enabled by spider 2.52.0's `basic` → `__basic`. Changes spider's local-Chrome `CHROME_ARGS` (drops `--no-sandbox`) on the fallback path where spider launches its own Chrome process — only relevant when `AXON_CHROME_REMOTE_URL` is unset; production always sets it. |
| `smart` | — | Project implements its own `auto-switch` logic in `engine.rs` |

### WebDriver (7)

| Flag | Status | Notes |
|------|--------|-------|
| `webdriver` | — | |
| `webdriver_headed` | — | |
| `webdriver_stealth` | — | |
| `webdriver_chrome` | — | |
| `webdriver_firefox` | — | |
| `webdriver_edge` | — | |
| `webdriver_screenshot` | — | |

### AI / LLM (2)

| Flag | Status | Notes |
|------|--------|-------|
| `openai` | ✅ (spider_agent) | LLM synthesis for `research` command — `with_openai_compatible()` in `research.rs` |
| `gemini` | — | |

### Spider Cloud (1)

| Flag | Status | Notes |
|------|--------|-------|
| `spider_cloud` | — | Self-hosted only |

### Agent (12)

| Flag | Status | Notes |
|------|--------|-------|
| `agent` | — | `spider_agent` crate used directly instead of via spider feature flag |
| `agent_openai` | — | |
| `agent_chrome` | — | |
| `agent_webdriver` | — | |
| `agent_skills` | — | |
| `agent_skills_s3` | — | |
| `agent_fs` | — | |
| `agent_search_serper` | — | |
| `agent_search_brave` | — | |
| `agent_search_bing` | — | |
| `agent_search_tavily` | ✅ (spider_agent) | Tavily search in `search.rs`, `research.rs`, `mcp/server/common.rs` |
| `agent_full` | — | |

### Firewall (1)

| Flag | Status | Notes |
|------|--------|-------|
| `firewall` | — | **NOT enabled.** `spider_firewall`'s build.rs fetches blocklists from `api.github.com` unauthenticated and panics when GitHub rate-limits CI; it doesn't read `GITHUB_TOKEN`. `validate_url()` in `src/core/http/ssrf.rs` remains the primary SSRF guard. Re-enable when upstream supports an auth knob. (See root `CLAUDE.md` → Spider feature flags.) |

### Search (5)

| Flag | Status | Notes |
|------|--------|-------|
| `search` | — | |
| `search_serper` | — | |
| `search_brave` | — | |
| `search_bing` | — | |
| `search_tavily` | — | Tavily access is via `spider_agent`, not the `spider` search feature |

---

## Summary

| Category | Total | Enabled |
|----------|-------|---------|

| Core | 34 | 21 — 11 previously enabled (`regex`, `sitemap`, `simd`, `inline-more`, `ua_generator`, `headers`, `hedge`, `time`, `control`, `adaptive_concurrency`, `warc`; `basic` itself is the meta-feature, not a separate row) plus 10 newly force-enabled by spider 2.52.0's `basic` → `__basic` (`balance`, `cookies`, `encoding`, `sync`, `rate_limit`, `request_coalesce`, `priority_frontier`, `dns_cache`, `string_interner_buffer_backend`, `auto_throttle`) — `glob` is still NOT enabled |
| Storage | 3 | 2 (`disk`, `disk_native_tls`) newly force-enabled via `basic` → `__basic` (spider 2.52.0) — unused by axon, which stores jobs in SQLite and vectors in Qdrant; `disk_aws` still NOT enabled |
| Caching | 7 | 2 (`cache_mem`, `etag_cache`) |
| Chrome / Browser | 17 | 8 (`chrome`, `chrome_stealth`, `chrome_screenshot`, `chrome_store_page`, `chrome_headless_new`, `chrome_simd`, `adblock`, `real_browser`) — `real_browser` newly force-enabled via `basic` → `__basic` (spider 2.52.0) |
| Firewall | 1 | 0 (`firewall` NOT enabled — build.rs rate-limit panic) |
| WebDriver | 7 | 0 |
| AI / LLM | 2 | 1 via spider_agent (`openai`) |
| Spider Cloud | 1 | 0 |
| Agent | 12 | 1 via spider_agent (`search_tavily`) |
| Search | 5 | 0 |
| **Total** | **89** | **33 spider + 2 spider_agent = 35** |

> `basic` is a meta-feature on the `spider` crate; as of spider 2.52.0 it expands to `["__basic", "basic_tls"]` and force-enables 13 features axon never declared (`__basic`'s own list, minus the 4 axon already declares explicitly — see "Transitively Enabled via `basic` → `__basic`" above). The project still uses `default-features = false` on all spider crates, so anything beyond `basic`'s own transitive closure remains excluded.
