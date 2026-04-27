# axon map
Last Modified: 2026-04-27

Discover all URLs on a site without scraping page content. Fast by default (seconds via sitemap discovery), with an explicit fallback chain for sites that have no sitemap.

## Synopsis

```bash
axon map <url> [FLAGS]
axon map --start-url <url> [FLAGS]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<url>` | Start URL to map (optional if `--start-url` is set) |

## Flags

All global flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--map-fallback <structure\|crawl>` | `structure` | Fallback when no sitemap is found. `structure`: extract anchors from root page (fast). `crawl`: full Spider.rs crawl (slow, legacy — explicit opt-in). |
| `--max-sitemaps <n>` | `512` | Maximum sitemap documents to process per map operation. |
| `--discover-sitemaps <bool>` | `true` | Enable sitemap discovery (primary URL source). |
| `--sitemap-since-days <n>` | `0` | Only include sitemap URLs with `<lastmod>` within the last N days (0 = no filter). |
| `--include-subdomains <bool>` | `false` | Include subdomains under same parent domain. |
| `--json` | `false` | Print structured payload including all discovered URLs. |

## How it works

`axon map` uses a **sitemap-first** strategy:

1. **Sitemap discovery** (primary): fetches `robots.txt` and default sitemap paths in parallel with seed URL resolution. Checks: `sitemap.xml`, `sitemap_index.xml`, `sitemap-index.xml`, `wp-sitemap.xml`, `sitemap/sitemap-index.xml`.
2. **Bounded structure fallback** (default, when no sitemap parsed): fetches the root page once and extracts anchor hrefs (up to 500 URLs). Fast, no full crawl. Triggered when `parsed_sitemap_documents == 0`.
3. **Full crawl** (opt-in only): set `--map-fallback crawl` to use Spider.rs. This is the legacy behaviour — slower but handles SPAs and complex navigation.

> **Important:** the fallback from sitemap to structure is triggered by `parsed_sitemap_documents == 0`, not by an empty URL list. If a sitemap was found but all URLs were out of scope, `map_source` will be `"sitemap"` and the URL list will be empty — no anchor fallback is applied in this case.

## Examples

```bash
# Default: sitemap-first, bounded-structure fallback
axon map https://example.com/docs

# JSON output for automation
axon map https://example.com --json

# Opt-in to full crawl fallback (legacy, slow)
axon map https://example.com --map-fallback crawl

# Limit sitemap documents processed
axon map https://example.com --max-sitemaps 128

# Disable sitemap discovery entirely (forces bounded-structure or crawl fallback)
axon map https://example.com --discover-sitemaps false
```

## Output

JSON mode returns:

| Field | Type | Description |
|-------|------|-------------|
| `url` | string | The start URL |
| `mapped_urls` | number | Count of discovered URLs in the output |
| `sitemap_urls` | number | Raw `<loc>` count from sitemaps (before dedup/scope filter) |
| `pages_seen` | number | Pages fetched during crawl (`0` in sitemap/structure modes) |
| `thin_pages` | number | Pages below `--min-markdown-chars` (`0` in non-crawl modes) |
| `elapsed_ms` | number | Time taken in milliseconds |
| `map_source` | string | How URLs were discovered: `"sitemap"`, `"bounded-structure"`, or `"crawl"` |
| `warning` | string or null | Non-null when bounded-structure returns fewer than 5 URLs (suggests using `--map-fallback crawl`) |
| `urls` | array | All discovered URLs, sorted and deduplicated |

## Behavior Notes

- `map` validates the URL before any network calls.
- `map` resolves redirects before deriving its scope. Sitemap discovery runs against the original host in parallel with redirect resolution.
- Path-rooted seeds such as `https://example.github.io/project/` stay scoped to that subtree.
- `map` is synchronous (inline) and does not enqueue jobs.
- No Chrome is used in sitemap or bounded-structure modes. Chrome is only used when `--map-fallback crawl` with `--render-mode chrome` or `auto-switch`.
- All discovered URLs are validated and canonicalized before output.
