# axon map
Last Modified: 2026-04-27

Discover all URLs on a site without scraping page content. Fast by default (seconds via sitemap discovery), with an explicit fallback chain for sites that have no sitemap.

## Synopsis

```bash
axon map <url> [FLAGS]
axon map <url> [FLAGS]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<url>` | Start URL to map |

## Flags

All global flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--map-fallback <structure\|crawl>` | `structure` | Fallback when no sitemap is parsed. `structure`: extract anchors from the scope root page (fast). `crawl`: full Spider.rs crawl (slow, legacy — explicit opt-in). |
| `--include-subdomains <bool>` | `false` | Include subdomains under same parent domain. |
| `--json` | `false` | Print structured payload including all discovered URLs. |

## How it works

`axon map` uses a **sitemap-first** strategy:

1. **Sitemap + llms.txt discovery** (primary): fetches `robots.txt`, the default sitemap paths, and `/llms.txt` in parallel with seed URL resolution. Sitemap paths checked: `sitemap.xml`, `sitemap_index.xml`, `sitemap-index.xml`, `wp-sitemap.xml`, `sitemap/sitemap-index.xml`. When `scrape.discover-llms-txt = true` (default), `/llms.txt` at the site root is parsed for markdown links, host-scoped, and **merged** (deduped) into the sitemap URL set. Disable with `scrape.discover-llms-txt = false`.
2. **Bounded structure fallback** (default, when neither a sitemap nor an llms.txt yields URLs): fetches the scope root page once and extracts anchor hrefs (up to 500 URLs). Fast, no full crawl. The "scope root" is the path-anchored root for the requested URL — `axon map https://site/docs` fetches `https://site/docs`, not `https://site/`.
3. **Full crawl** (opt-in only): set `--map-fallback crawl` to use Spider.rs. This is the legacy behaviour — slower but handles SPAs and complex navigation.

> **Important:** the fallback to bounded-structure is triggered by whether any sitemap **or** llms.txt URL was discovered, not by the URL count. If a sitemap was found but all URLs were out of scope, `map_source` will be `"sitemap"` and the URL list will be empty — no anchor fallback is applied in this case.

## Examples

```bash
# Default: sitemap-first, bounded-structure fallback
axon map https://example.com/docs

# JSON output for automation
axon map https://example.com --json

# Opt-in to full crawl fallback (legacy, slow)
axon map https://example.com --map-fallback crawl

# Sitemap limits and discovery behavior live in config.toml under [scrape].
```

## Output

JSON mode returns:

| Field | Type | Description |
|-------|------|-------------|
| `url` | string | The start URL |
| `mapped_urls` | number | Count of discovered URLs in the output |
| `sitemap_urls` | number | Count of sitemap-discovered URLs after deduplication and in-scope filtering |
| `pages_seen` | number | Pages fetched during crawl (`0` in sitemap/structure modes) |
| `thin_pages` | number | Pages below `scrape.min-markdown-chars` (`0` in non-crawl modes) |
| `elapsed_ms` | number | Time taken in milliseconds |
| `map_source` | string | How URLs were discovered: `"sitemap"`, `"sitemap+llms"` (sitemap merged with `/llms.txt`), `"llms"` (no sitemap, but a curated `/llms.txt`), `"bounded-structure"`, or `"crawl"` |
| `warning` | string or null | Non-null when bounded-structure returns fewer than 5 URLs or fails to fetch the scope root (suggests using `--map-fallback crawl`) |
| `urls` | array | All discovered URLs, sorted and deduplicated |

## Behavior Notes

- `map` validates the URL before any network calls.
- `map` resolves redirects before deriving its scope. Sitemap discovery runs against the original host in parallel with redirect resolution.
- Path-rooted seeds such as `https://example.github.io/project/` stay scoped to that subtree.
- `map` is synchronous (inline) and does not enqueue jobs.
- No Chrome is used in sitemap or bounded-structure modes. Chrome is only used when `--map-fallback crawl` with `--render-mode chrome` or `auto-switch`.
- All discovered URLs are validated and canonicalized before output.
