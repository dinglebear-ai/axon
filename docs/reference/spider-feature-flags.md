# Spider.rs Feature Flags

> **Webclaw port feature flags** (tls-fingerprinting, quickjs, social-verticals) are documented in
> [`docs/reference/cargo-features.md`](cargo-features.md) — that file also covers runtime env-var gates.

**Total feature entries tracked in this inventory: 79 (includes `basic` meta-feature)**
**Flags enabled in axon_rust: 19 (spider) + 2 (spider_agent) + spider_transformations (no flags)**

---

## Active Dependency Declarations

```toml
# Root Cargo.toml (single-crate workspace — only one Cargo.toml)
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

## Flags In Use

### spider crate — 19 flags enabled

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
| `chrome` | Chrome / Browser | `RenderMode::Chrome` and `RenderMode::AutoSwitch` paths in `src/crawl/engine/runtime.rs`. Imports `spider::features::chrome_common::{RequestInterceptConfiguration, ScreenShotConfig, ScreenshotParams, WaitForSelector}` |
| `chrome_stealth` | Chrome / Browser | Passed to `spider::website::Website` in `configure_website()` in `src/crawl/engine/`. Enables headless detection evasion |
| `chrome_screenshot` | Chrome / Browser | `ScreenshotParams` usage in `src/crawl/engine/runtime.rs`. Powers screenshot capture during crawls |
| `chrome_store_page` | Chrome / Browser | Retains page object for conditional post-render actions (screenshots, metadata) |
| `chrome_headless_new` | Chrome / Browser | `--headless=new` mode — better DOM fidelity, fewer detection fingerprints |
| `chrome_simd` | Chrome / Browser | SIMD-optimized CDP message parsing for Chrome communication |
| `adblock` | Chrome / Browser | Implicit ad/tracker request filtering during crawl. No local toggle — always active when chrome features are in use |
| `cache_mem` | Caching | In-memory page/request deduplication during crawls. No local call site; spider uses it internally for request memoization |
| `etag_cache` | Caching | Conditional re-crawl. `--etag-conditional` seeds the per-`Website` ETag cache from `etag.json`; spider sends `If-None-Match`/`If-Modified-Since` and skips the body on `304`. Wired in `src/crawl/engine/runtime.rs`; cross-run reconciliation in `src/crawl/engine/etag.rs` (bead axon_rust-hiyf) |


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

## Full Flag Inventory (all 79, includes `basic` meta-feature)

`✅` = enabled in axon_rust · `—` = not used

### Core (25)

| Flag | Status | Notes |
|------|--------|-------|
| `ua_generator` | ✅ | Random realistic User-Agent generation per request |
| `regex` | ✅ | URL blacklist/whitelist filtering |

| `glob` | — | NOT enabled — glob URL patterns change `crawl_establish` to use a budget-aware `is_allowed()` check that immediately returns `BudgetExceeded` for the first URL with `with_limit(1)`, producing 0 pages from Chrome crawls. axon does not use URL glob patterns. Do NOT add this flag. See CLAUDE.md gotchas. |

| `fs` | — | Project uses SQLite jobs plus Qdrant vector storage, not spider disk FS |
| `sitemap` | ✅ | Sitemap discovery + backfill |
| `time` | ✅ | Timing/duration tracking for crawl operations |
| `encoding` | — | |
| `serde` | — | Project uses its own serde deps directly |
| `sync` | — | |
| `control` | ✅ | Runtime crawl control — pause/resume/shutdown. Crawl cancellation sends Spider shutdown for the active crawl target before returning canceled |
| `full_resources` | — | |
| `cookies` | — | |
| `spoof` | — | `chrome_stealth` covers bot-evasion needs |
| `headers` | ✅ | Custom HTTP header injection for crawl requests |
| `balance` | — | Silent concurrency throttling with no logging — we manage concurrency ourselves via performance profiles |
| `cron` | — | |
| `tracing` | — | Project uses `tracing` crate directly, not via spider |
| `cowboy` | — | Full concurrency with no throttle — dangerous, prefer `balance` |
| `llm_json` | — | Lenient JSON parsing for LLM output quirks |
| `page_error_status_details` | — | |
| `extra_information` | — | |
| `cmd` | — | tokio process support within spider (axon has its own) |
| `io_uring` | — | |
| `simd` | ✅ | SIMD-accelerated text/JSON parsing |
| `inline-more` | ✅ | Aggressive function inlining in spider internals for runtime perf |

| `hedge` | ✅ | Hedged duplicate HTTP request for resilience — races a second request after the default 3s delay. Doubles HTTP traffic for pages that take >3s. Used in `src/crawl/engine/runtime.rs` via `HedgeConfig::default()`. |


### Storage (3)

| Flag | Status | Notes |
|------|--------|-------|
| `disk` | — | Project uses SQLite jobs plus Qdrant vector storage, not spider disk cache |
| `disk_native_tls` | — | |
| `disk_aws` | — | |

### Caching (6)

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
| `real_browser` | — | |
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

| Core | 25 | 10 (`basic`, `regex`, `sitemap`, `simd`, `inline-more`, `ua_generator`, `headers`, `hedge`, `time`, `control`) — `glob` is NOT enabled |

| Storage | 3 | 0 |
| Caching | 6 | 2 (`cache_mem`, `etag_cache`) |
| Chrome / Browser | 17 | 7 (`chrome`, `chrome_stealth`, `chrome_screenshot`, `chrome_store_page`, `chrome_headless_new`, `chrome_simd`, `adblock`) |
| Firewall | 1 | 0 (`firewall` NOT enabled — build.rs rate-limit panic) |
| WebDriver | 7 | 0 |
| AI / LLM | 2 | 1 via spider_agent (`openai`) |
| Spider Cloud | 1 | 0 |
| Agent | 12 | 1 via spider_agent (`search_tavily`) |
| Search | 5 | 0 |
| **Total** | **79** | **19 spider + 2 spider_agent = 21** |

> `basic` is a meta-feature enabled on the `spider` crate that bundles core crawl behavior. The project uses `default-features = false` on all spider crates, so only explicitly listed features are compiled in.
