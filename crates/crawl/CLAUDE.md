# crates/crawl — Spider.rs Crawl Engine
Last Modified: 2026-03-21

Wraps spider.rs for site crawling with HTTP and Chrome rendering paths.

## Module Layout

```
crawl/
├── engine.rs              # Module root. `run_crawl_once()` (line 96), `run_sitemap_only()` (215), `should_fallback_to_chrome()` (66)
├── engine/
│   ├── runtime.rs         # configure_website()/configure_website_with_crawl_id() — spider Website builder, control-thread wiring, request/identity settings
│   ├── collector.rs       # Crawl collection pipeline
│   ├── collector/         # Collector submodules
│   ├── map.rs             # Map-mode helpers
│   ├── map/               # `crawl_and_collect_map()` lives here (engine/map/strategy.rs)
│   ├── sitemap.rs         # `append_sitemap_backfill()`, sitemap discovery + filtering, `<lastmod>` parsing
│   ├── thin_refetch.rs    # Re-fetch thin pages with Chrome
│   ├── cdp_render.rs      # Chrome DevTools Protocol render path
│   ├── url_utils.rs       # `is_junk_discovered_url`, `derive_auto_whitelist_pattern`, helpers
│   ├── url_utils_proptest.rs
│   ├── waf.rs             # WAF/firewall detection helpers
│   ├── dir_ops.rs         # Output directory helpers
│   └── tests.rs
├── manifest.rs            # Crawl manifest generation + persistence
├── scrape.rs              # Single-URL scrape entrypoint (32K — owns its own Website builder)
├── screenshot.rs          # Screenshot capture
└── chrome_bootstrap.rs    # Chrome runtime bootstrap utilities
```

Top-level keynote: `engine.rs`, `manifest.rs`, `scrape.rs`, `screenshot.rs`, and `chrome_bootstrap.rs` each have **independent** `Website::new()` paths — when adjusting retry/UA/header behavior, keep them in sync.

## Critical Patterns

### crawl_raw() vs crawl()
- `crawl_raw()` — pure HTTP, always available, no Chrome dependency
- `crawl()` — Chrome-aware, requires a running Chrome instance

`engine.rs` calls:
- `crawl_raw()` for `RenderMode::Http`
- `crawl()` for `RenderMode::Chrome` and `RenderMode::AutoSwitch`

If Chrome is unavailable and mode is AutoSwitch, `try_auto_switch()` falls back and keeps the HTTP result.

### configure_website() Chain
Called once per crawl in `engine.rs`. Two entry points:
- `configure_website(cfg, url, mode)` — CLI path (no crawl_id, control thread is a no-op)
- `configure_website_with_crawl_id(cfg, url, mode, Some(id))` — worker path (sets crawl_id for `spider::utils::shutdown()`)

Both are `pub(super)` in `runtime.rs`. Fixed internal calls (do NOT remove):
```rust
website.with_retry(retries as u8)   // clamp to u8 — must not exceed 255
       .with_normalize()             // URL normalization — required for dedup
       .with_tld(false);             // hardcoded — do not change
```
`with_no_control_thread(false)` enables spider's control handler — **do not set to `true`**, it breaks graceful cancel.

`scrape.rs`, `thin_refetch.rs`, and `content/engine.rs` have their **own independent** `Website::new()` paths — keep retry, `custom_headers`, and UA settings in sync when changing `configure_website()` behavior.

### Auto-Switch Logic
`try_auto_switch()` triggers Chrome fallback when:
- >60% of pages are thin (below `--min-markdown-chars`, default 200 chars), **OR**
- total pages crawled is below a minimum coverage threshold

Chrome requires `AXON_CHROME_REMOTE_URL` set. If not set, HTTP result is kept.

### Link Filter (`set_on_link_find`)
`runtime.rs` registers `website.set_on_link_find()` in `apply_request_and_identity_settings()`. It fires on every discovered link **before** the blacklist regex and before any fetch. Two guards run in order:

**1. Junk URL detection** (`is_junk_discovered_url` in `url_utils.rs`):

Heuristics (each sufficient to reject, checked against the full URL then path-only):
- URL length > 2048 characters
- HTML-encoded ampersand (`&amp;`) anywhere in the URL — indicates the link was extracted from raw HTML without entity decoding; the server expects `&`, not `&amp;`, so these always 404
- Encoded HTML tags in URL path (`%3C`/`%3E`)
- Template literal placeholders (`%7B`/`%7D`)
- 3 or more `%20` sequences in the URL path
- JS string concat artifact: `'%20` or `%20'` in path

The `&amp;` check is applied to the full URL (not path-only) because it typically appears in query strings (e.g. `?since=daily&amp;lang=en`).

**2. Auto path-prefix scoping** (`derive_auto_whitelist_pattern` in `url_utils.rs`):

Applied in `configure_website()` **before** the link-find callback. When no explicit `--url-whitelist` is provided and the start URL has ≥2 path segments, a whitelist regex scoping the crawl to that directory subtree is set automatically via `website.with_whitelist_url()`. Single-segment paths (`/docs`) and root paths get no auto-scope. Override by passing `--url-whitelist`.

**3. Cross-domain rejection** (enforced when `include_subdomains = false`, the default):

When `include_subdomains = false`, the start URL's host is extracted once at configuration time and captured in the `set_on_link_find` closure. Any absolute URL whose host does not match (case-insensitive) is dropped immediately.

This prevents scope explosions where a page links out to a different domain (e.g. docs linking to GitHub repos) and spider follows those links across the entire external domain. Spider's own `with_subdomains(false)` only prevents *subdomain* expansion — it does not block following links to completely unrelated domains.

Relative URLs (no `://`) always pass the cross-domain check; they resolve against the base URL at fetch time. Use `--include-subdomains true` to disable host enforcement when intentionally crawling multiple related domains.

Returns `CaseInsensitiveString::default()` to reject; returns the original `(url, html)` pair to allow.

### Mid-Crawl Cancellation (Redis + Spider Control)
Two-layer cancel: Redis for cross-process signaling, spider `control` feature for in-process graceful shutdown.

**Redis layer** — `run_active_crawl_job` in `process.rs` races the crawl future against `poll_cancel_key`:
- Polls Redis key `axon:crawl:cancel:{job_id}` every **3 seconds**
- **Fail-safe:** returns `false` on any Redis error — a Redis outage never false-cancels a crawl
- Cancel a running crawl: `axon crawl cancel <job_id>` (sets the Redis key)

**Spider control layer** — when the Redis cancel key is detected:
1. Calls `spider::utils::shutdown("{crawl_id}{url}")` — signals spider's in-process control thread via `AtomicI8`
2. Spider stops dispatching new pages immediately, drains in-flight requests gracefully
3. The crawl future is **awaited** (not dropped) with a 30s timeout, returning partial results
4. Partial results (`pages_crawled`, `md_created`, `elapsed_ms`) are saved to `result_json` in the DB
5. Job is marked `canceled` (not `failed`) — the `WHERE status='running'` guard prevents racing with natural completion

**crawl_id wiring:** `configure_website_with_crawl_id()` in `runtime.rs` sets `website.with_crawl_id(job_uuid)`. The control target is `"{job_uuid}{start_url}"` — must match spider's `target_id()` format exactly.

**Fallback:** If the 30s drain timeout expires or spider errors during shutdown, the cancel path falls back to the original hard-cancel behavior (no partial results saved).

### readability: false (DO NOT CHANGE)
`build_transform_config()` in `crates/core/content.rs` sets `readability: false`. Changing to `true` causes Mozilla Readability to score VitePress/sidebar docs as low-quality and strip them to just the title — produces ~97% thin pages on most doc sites. `main_content: true` handles structural extraction without the scoring penalty.

### Sitemap Backfill
`append_sitemap_backfill()` runs after the main crawl, discovers URLs from sitemap.xml that the crawler missed, and fetches them individually. Respects `--max-sitemaps` (default 512) and `--include-subdomains`. Safe to skip if `--discover-sitemaps false`.

Use `--sitemap-since-days N` to restrict backfill to URLs whose `<lastmod>` falls within the last N days. Implemented via `extract_loc_with_lastmod()` in `content.rs` which parses `<url>` blocks and extracts `<loc>` + `<lastmod>` pairs. URLs without `<lastmod>` are always included (unknown age = don't filter). Date filtering is skipped entirely for sitemap index entries (`<sitemapindex>`) since those point to child sitemaps, not pages.

### Locale Path Filtering
`--exclude-path-prefix` (and the built-in locale list) treats `/` and `-` as word boundaries. `/ja` blocks both `/ja/docs` and `/ja-jp/docs`. Pass `none` to disable all locale filtering.

## Testing

```bash
cargo test engine         # crawl engine tests (8): auto-switch, sitemap, thin detection
cargo test -- --nocapture # show spider log output during test crawls
```

Engine tests use a live HTTP server (via `httpmock`) — no Docker services required.

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| 97%+ thin pages | `readability: true` stripping docs | Verify `readability: false` in `content.rs` |
| Thin pages on known-good site | JS-rendered SPA | Use `--render-mode chrome` |
| Chrome fallback not triggering | Chrome not reachable | Check `AXON_CHROME_REMOTE_URL`; verify `axon-chrome` is up |
| `NoResponse` errors from `spider::features::chrome` (every ~2s, ×10) | `CHROME_URL` env var contains a Docker hostname (`axon-chrome:6000`) that spider reads raw without normalization. In AutoSwitch/Http modes, axon didn't previously set `chrome_connection_url`, so spider fell back to `CHROM_BASE = CHROME_URL`. | Fixed in `runtime.rs`: `with_chrome_connection` is now called for ALL render modes when `chrome_remote_url` is configured. Also set `CHROME_URL=http://127.0.0.1:6000` (not `axon-chrome:6000`) in `.env` — spider reads this directly, no normalization. |
| Crawl stops at first level | `--max-depth 0` set accidentally | Default is 5; check CLI args |
| Crawling other subdomains instead of target host | `--include-subdomains true` enabled | Default is now `false`; only use `--include-subdomains true` when you intentionally want all `*.parent.com` |
| Crawl explodes to unrelated domains (GitHub, CDN, etc.) | Spider follows cross-domain links by default | Fixed in `set_on_link_find`: cross-domain links are dropped when `include_subdomains = false`. If you see it again, verify the start URL's host is being parsed correctly from `apply_request_and_identity_settings`. |
| External domain links you *want* to follow are being blocked | Cross-domain enforcement too strict for your use case | Pass `--include-subdomains true` to disable host enforcement, then use `--url-whitelist` to scope the crawl precisely. |
| Locale pages being crawled | Default locale filter only blocks known prefixes | Pass `--exclude-path-prefix none` to disable, or add custom prefixes |

## Thin Page Lifecycle
```
fetch page → content.rs transform → check len < min_markdown_chars
    → thin: skip (if --drop-thin-markdown true, default)
    → ok: save to disk + enqueue embed
```
Chrome auto-switch monitors thin rate across the crawl batch, not per-page.
