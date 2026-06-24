# src/crawl — Spider.rs Crawl Engine
Last Modified: 2026-05-16

Wraps spider.rs for site crawling with HTTP and Chrome rendering paths.

## Module Layout

```
crawl/
├── engine.rs              # Module root. `run_crawl_once()` (line 96), `run_sitemap_only()` (215), `should_fallback_to_chrome()` (66)
├── engine/
│   ├── runtime.rs         # configure_website()/configure_website_with_crawl_id() — spider Website builder, control-thread wiring, request/identity settings
│   ├── collector.rs       # Crawl collection pipeline — runs antibot detect + structured-data pass on each page
│   ├── collector/         # Per-page collection submodules
│   │   ├── page.rs            # Per-page handler: calls `detect_challenge` (antibot), structured-data pass, DOM ladder
│   │   ├── manifest.rs        # Crawl manifest accumulator
│   │   ├── manifest_tests.rs  # sidecar tests for manifest.rs
│   │   ├── chrome_tasks.rs    # Per-page Chrome render tasks
│   │   └── util.rs            # Shared collector helpers
│   ├── map.rs             # Map-mode helpers
│   ├── map/               # `crawl_and_collect_map()` lives here (engine/map/strategy.rs)
│   ├── sitemap.rs         # Module root: shared bounded-HTTP fetch helpers (`fetch_text_with_retry`, body caps, `should_retry_status` 52x dead-host classification) + re-exports
│   ├── sitemap/
│   │   ├── discover.rs    # `discover_sitemap_urls()` — robots.txt + seed paths, batched JoinSet fetching, `<sitemapindex>` recursion
│   │   ├── backfill.rs    # `append_sitemap_backfill()`/`append_candidate_backfill()` — fetch missed URLs, convert, append to manifest
│   │   └── filter.rs      # `loc_in_scope()`, `<lastmod>` recency, `is_already_markdown()` passthrough check
│   ├── etag.rs            # Conditional re-crawl (ETag/304): sidecar seed/persist + visited-set-gated reconciliation (axon_rust-hiyf)
│   ├── etag_tests.rs      # sidecar tests for etag.rs
│   ├── thin_refetch.rs    # Re-fetch thin pages with Chrome
│   ├── cdp_render.rs      # Chrome DevTools Protocol render path
│   ├── url_utils.rs       # `is_junk_discovered_url`, `derive_auto_whitelist_pattern`, helpers
│   ├── url_utils_proptest_tests.rs
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

### Per-Page Passes (Collector Pipeline)

`engine/collector/page.rs` runs three passes on every collected HTML page **before** the page is committed to the manifest. Each is independently bounded by a `cfg.*_max_bytes` cap to keep large HTML pages cheap.

| Pass | Function | Cap (env) | Triggered when | Outcome on hit |
|------|----------|-----------|----------------|----------------|
| **Antibot detection** (gc59 / jej7.1) | `core::http::detect_challenge` | `cfg.antibot_max_body_scan_bytes` (env `AXON_ANTIBOT_MAX_BODY_SCAN_BYTES`, default 150 KiB) | 8 WAF signatures matched in scanned prefix | Page is skipped — not embedded, not added to manifest; logged as `antibot.detected` |
| **Structured-data pass** (jej7.2 / xvu9 / d5mb) | JSON-LD + `__NEXT_DATA__` + `__next_f` walkers (1jto, 2dhc) | `cfg.structured_data_max_bytes` (env `AXON_STRUCTURED_DATA_MAX_BYTES`, default 64 KiB) | Page has structured-data island | Output is attached to `ManifestEntry.structured` and flows through to `ScrapedDoc.structured` |
| **DOM retry ladder** (jh32) | `core::content::extract_ladder` | `cfg.ladder_body_multiplier` (env `AXON_LADDER_BODY_MULTIPLIER`) | Page is thin after initial parse and `cfg.ladder_strategy` allows it | Re-parses with progressively richer DOM strategies before triggering Chrome fallback — saves Chrome resources on near-misses |

All three are feature-gated by `cfg.enable_*` toggles introduced in `zehr`; defaults make them on. Antibot detection runs first because there's no point running structured-data/ladder against a challenge page.

### Auto-Switch Logic
`try_auto_switch()` triggers Chrome fallback when:
- >60% of pages are thin (below `--min-markdown-chars`, default 200 chars), **OR**
- total pages crawled is below a minimum coverage threshold

Chrome requires `AXON_CHROME_REMOTE_URL` set. If not set, HTTP result is kept.

### Link Filter (`set_on_link_find`)
`runtime.rs` registers `website.set_on_link_find()` in `apply_request_and_identity_settings()`. It fires on every discovered link **before** the blacklist regex and before any fetch. In-callback guards run in order **junk → media-asset → cross-domain**; auto path-prefix scoping (below) is applied earlier, in `configure_website()`, before the callback:

**1. Junk URL detection** (`is_junk_discovered_url` in `url_utils.rs`):

Heuristics (each sufficient to reject, checked against the full URL then path-only):
- URL length > 2048 characters
- HTML-encoded ampersand (`&amp;`) anywhere in the URL — indicates the link was extracted from raw HTML without entity decoding; the server expects `&`, not `&amp;`, so these always 404
- Encoded HTML tags in URL path (`%3C`/`%3E`)
- Template literal placeholders (`%7B`/`%7D`)
- 3 or more `%20` sequences in the URL path
- JS string concat artifact: `'%20` or `%20'` in path

The `&amp;` check is applied to the full URL (not path-only) because it typically appears in query strings (e.g. `?since=daily&amp;lang=en`).

**1b. Media-asset rejection** (`spider::utils::media_asset::is_media_asset_url`, bead axon_rust-mk95):

Runs immediately after junk detection. Spider's compile-time perfect-hash classifier keys on the URL's file extension and drops images, fonts, audio, video, archives, and PDFs before they are queued, fetched, or embedded. `.html`/`.htm`/extensionless doc routes pass through unaffected. This replaced the previous hand-rolled media heuristics.

**2. Auto path-prefix scoping** (`derive_auto_whitelist_pattern` in `url_utils.rs`):

Applied in `configure_website()` **before** the link-find callback. When no explicit `--url-whitelist` is provided and the start URL has ≥2 path segments, a whitelist regex scoping the crawl to that directory subtree is set automatically via `website.with_whitelist_url()`. Single-segment paths (`/docs`) and root paths get no auto-scope. Override by passing `--url-whitelist`.

**3. Cross-domain rejection** (enforced when `include_subdomains = false`, the default):

When `include_subdomains = false`, the start URL's host is extracted once at configuration time and captured in the `set_on_link_find` closure. Any absolute URL whose host does not match (case-insensitive) is dropped immediately.

This prevents scope explosions where a page links out to a different domain (e.g. docs linking to GitHub repos) and spider follows those links across the entire external domain. Spider's own `with_subdomains(false)` only prevents *subdomain* expansion — it does not block following links to completely unrelated domains.

Relative URLs (no `://`) always pass the cross-domain check; they resolve against the base URL at fetch time. Use `--include-subdomains true` to disable host enforcement when intentionally crawling multiple related domains.

Returns `CaseInsensitiveString::default()` to reject; returns the original `(url, html)` pair to allow.

### Mid-Crawl Cancellation (SQLite + Spider Control)
Cancellation is SQLite-backed plus in-process cancellation tokens. There is no Redis cancel path.

**Job layer**:
- `axon crawl cancel <job_id>` calls the service runtime, which flips the SQLite row to `canceled` through `CancelStore::cancel`.
- The active worker registers a `CancellationToken` for each claimed job; canceling the token lets the runner stop without waiting for a stale-job reclaim.
- Pending jobs are canceled by the row update alone; running jobs also observe the in-memory token.

**Spider control layer**:
1. The crawl runner observes cancellation and calls `spider::utils::shutdown("{job_id}{url}")` for the active Spider control target.
2. Spider stops dispatching new pages and drains in-flight requests where possible.
3. Crawl progress JSON written before cancellation remains on the row, including output paths and counts when available.
4. The job remains `canceled`, not `failed`, unless the runner hits an unrelated execution error before observing cancellation.

**crawl_id wiring:** `configure_website_with_crawl_id()` in `runtime.rs` sets `website.with_crawl_id(job_uuid)`. The control target is `"{job_uuid}{start_url}"` — it must match Spider's `target_id()` format exactly.

### readability: false (DO NOT CHANGE)
`build_transform_config()` in `crates/axon-core/src/content.rs` sets `readability: false`. Changing to `true` causes Mozilla Readability to score VitePress/sidebar docs as low-quality and strip them to just the title — produces ~97% thin pages on most doc sites. `main_content: true` handles structural extraction without the scoring penalty.

### Sitemap Backfill
`append_sitemap_backfill()` runs after the main crawl, discovers URLs from sitemap.xml that the crawler missed, and fetches them individually. Respects `--max-sitemaps` (default 512) and `--include-subdomains`. Safe to skip if `--discover-sitemaps false`.

Use `--sitemap-since-days N` to restrict backfill to URLs whose `<lastmod>` falls within the last N days. Implemented via `extract_loc_with_lastmod()` in `content.rs` which parses `<url>` blocks and extracts `<loc>` + `<lastmod>` pairs. URLs without `<lastmod>` are always included (unknown age = don't filter). Date filtering is skipped entirely for sitemap index entries (`<sitemapindex>`) since those point to child sitemaps, not pages.

### Locale Path Filtering
`--exclude-path-prefix` (and the built-in locale list) treats `/` and `-` as word boundaries. `/ja` blocks both `/ja/docs` and `/ja-jp/docs`. Pass `none` to disable all locale filtering.

### Conditional Re-crawl (ETag/304) — `etag.rs` (bead axon_rust-hiyf)
Opt-in via `--etag-conditional` (independent of `--cache`). Enables spider's `etag_cache` feature and seeds the per-`Website` cache from a persisted `etag.json` sidecar (next to `manifest.jsonl`), so re-crawls send `If-None-Match`/`If-Modified-Since` and unchanged pages return a bodyless `304`.

**The 304 reconciliation gotcha (do NOT regress):** spider's 304 short-circuit returns `Default::default()` *inside* the per-URL fetch task, so a 304'd page **never enters the broadcast** — the collector would otherwise drop it as `Empty` and lose content. `reconcile_unmodified()` re-emits the previous manifest entry (`changed=false`, markdown relinked from `markdown.old`) for the silent skips. The reconcile set is `seeded ∩ previous_manifest − arrived ∩ visited`, where `visited = website.get_links()` canonicalized. **The visited-set gate is load-bearing:** a 304 skip is in `links_visited` (spider scheduled + fetched it); a no-longer-discovered page is not — so deleted pages are excluded rather than resurrected as zombies. `relink_reused_page` refuses symlinked `markdown.old` entries. Wired on the crawl path only; single-page `scrape` is intentionally excluded.

### 52x Dead-host Retry Classification — `should_retry_status` in `sitemap.rs` (bead axon_rust-6i30)
spider 2.51 returns precise synthetic codes: `521` refused, `524` timeout, `525` DNS/NXDOMAIN, `526` host/TLS unreachable (`525` is marked permanent only when proxy + independent local DNS agree). `should_retry_status()` excludes permanent `525`/`526` so the retry budget isn't burned re-resolving dead hosts; transient `52x` and genuine upstream `5xx` stay retryable.

### Per-path Budgets — `with_budget` in `runtime.rs` (bead axon_rust-37zv)
Repeatable `--budget PATH=N` flag caps pages crawled under each path prefix (`*` = all paths), e.g. `--budget /blog=100 --budget '*=1000'`. Parsed into `cfg.path_budgets` (owned `String` keys held on `Config` so the `&str` keys passed to `Website::with_budget` outlive the call). Unset = no budget (current behavior).

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
| `NoResponse` errors from `spider::features::chrome` (every ~2s, ×10) | Spider reads `CHROME_URL` from the environment as `CHROM_BASE` and may use an unresolvable Docker hostname. | Fixed: `runtime.rs` calls `with_chrome_connection` for ALL render modes when `cfg.chrome_remote_url` is set, so spider always uses axon's normalised localhost URL instead of `CHROM_BASE`. Do NOT add a bare `CHROME_URL=…` to `.env` — it is a stale alias deleted by `axon config migrate`; use `AXON_CHROME_REMOTE_URL` only. |
| Crawl stops at first level | `--max-depth 0` set accidentally | Default is 10; check CLI args |
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
