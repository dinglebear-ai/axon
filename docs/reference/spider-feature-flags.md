# Spider.rs Feature Flags

Last Modified: 2026-08-02

Axon's production web acquisition engine lives in
`crates/axon-adapters/src/web_engine/`. The authoritative feature declarations
are the Cargo manifests, especially `crates/axon-adapters/Cargo.toml`.

This page documents the Spider features Axon explicitly enables and the current
code paths that consume them. It does not attempt to mirror Spider's entire
upstream feature catalog.

## Runtime owner

`axon-adapters` owns Spider-based web acquisition:

```toml
spider = { version = "2", default-features = false, features = [
  "basic",
  "chrome",
  "chrome_intercept",
  "regex",
  "sitemap",
  "adblock",
  "chrome_stealth",
  "chrome_screenshot",
  "chrome_store_page",
  "chrome_headless_new",
  "chrome_simd",
  "simd",
  "inline-more",
  "cache_mem",
  "ua_generator",
  "headers",
  "time",
  "control",
  "hedge",
  "etag_cache",
  "warc",
] }
spider_transformations = "2"
spider_agent = { version = "2.47.89", default-features = false,
                 features = ["search_tavily"] }
```

`axon-core`, `axon-jobs`, and `axon-cli` also declare a compatible Spider
feature set for shared types, supporting boundaries, and tests. The production
crawl engine and its configuration remain owned by `axon-adapters`.

## Enabled Spider features

| Feature | Axon use |
|---|---|
| `basic` | Core Spider website/crawl primitives. |
| `chrome` | Chrome/CDP rendering for `render_mode=chrome` and auto-switch escalation. |
| `chrome_intercept` | Request interception used by adapter-owned browser acquisition. Enabled only in `axon-adapters`. |
| `regex` | Pattern filtering used by Spider URL and crawl configuration. |
| `sitemap` | Sitemap discovery and bounded sitemap backfill. |
| `adblock` | Browser request filtering for unwanted resource traffic. |
| `chrome_stealth` | Chrome stealth configuration where supported upstream. |
| `chrome_screenshot` | Screenshot capture through Spider Chrome types. |
| `chrome_store_page` | Retains rendered page content for collection and transformation. |
| `chrome_headless_new` | Uses the current Chrome headless mode. |
| `chrome_simd`, `simd` | SIMD-enabled upstream processing paths. |
| `inline-more` | Upstream inline crawl support used by the selected Spider build. |
| `cache_mem` | In-memory crawl cache support. Axon still requires explicit request-level cache opt-in. |
| `ua_generator` | User-agent generation support. Axon applies its own configured user-agent policy. |
| `headers` | Request and response header support, including conditional validators. |
| `time` | Upstream time-related crawl metadata and controls. |
| `control` | Crawl control identifiers used for cancellation and runtime coordination. |
| `hedge` | Hedged request configuration in the web-engine runtime. |
| `etag_cache` | Conditional ETag/Last-Modified recrawl support. |
| `warc` | WARC 1.1 output for HTTP and Chrome acquisition. |

The `basic` meta-feature may activate additional upstream internals. Axon only
claims behavioral support for paths exercised by its own runtime and tests.

## Current implementation map

| Behavior | Current source |
|---|---|
| Website construction and feature wiring | `crates/axon-adapters/src/web_engine/engine/runtime.rs` |
| HTTP/Chrome collection | `crates/axon-adapters/src/web_engine/engine/collector.rs` |
| CDP rendering | `crates/axon-adapters/src/web_engine/engine/cdp_render.rs` |
| Sitemap discovery/backfill | `crates/axon-adapters/src/web_engine/engine/sitemap/` |
| URL filtering and normalization | `crates/axon-adapters/src/web_engine/engine/url_utils.rs` |
| ETag reconciliation | `crates/axon-adapters/src/web_engine/engine/etag.rs` |
| Adaptive concurrency | `crates/axon-adapters/src/web_engine/engine/adaptive.rs` |
| WARC projection | `crates/axon-adapters/src/web/warc.rs` |
| Screenshot command path | `crates/axon-adapters/src/web_engine/screenshot.rs` |
| HTML transformation | `crates/axon-core/src/content/` and `spider_transformations` |
| Tavily search client | `crates/axon-adapters/src/providers/tavily_search.rs` |

## Request-level controls

Spider compile-time features make capabilities available. Runtime use is still
controlled by the unified source request and Axon configuration:

```bash
axon source https://docs.example.com --scope site --render-mode http
axon source https://docs.example.com --scope site --render-mode chrome
axon source https://docs.example.com --scope site --render-mode auto-switch
axon source https://docs.example.com --scope site --cache true --etag-conditional
axon source https://docs.example.com --scope site --warc /tmp/docs.warc.gz
```

Other important controls include `--max-pages`, `--max-depth`,
`--exclude-path-prefix`, `--budget`, `--block-assets`,
`--chrome-wait-for-selector`, `--root-selector`, and
`--exclude-selector`.

The installed binary's `axon source --help` output is authoritative for
current CLI options.

## Security boundary

Spider feature availability does not replace Axon's network security policy.
Seed URLs, discovered URLs, redirects, and resolved addresses remain subject to
shared SSRF checks in `crates/axon-core/src/http/ssrf.rs`. The custom
`SsrfBlockingResolver` validates addresses at connect time to reduce DNS
rebinding risk.

The Spider `firewall` feature is not enabled. Axon's SSRF boundary is explicit,
tested, and independent of upstream build-time blocklist fetching.

## Related references

- [Source Pipeline](../architecture/source-pipeline.md)
- [Security](../operations/security.md)
- [Cargo Features](cargo-features.md)
- [Adapter Scopes](sources/adapter-scopes.md)
