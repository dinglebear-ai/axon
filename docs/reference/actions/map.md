# axon map
Last Modified: 2026-04-27

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon map ...` |
| REST | Not inventoried |
| MCP | Not exposed as a dedicated MCP action. |
| Service | `Not inventoried` |

Parity notes: This action page is missing from docs/reference/api-parity.md.
<!-- END GENERATED ACTION SURFACES -->


Discover URLs on a site without scraping page content, crawling the site, or writing crawl output to disk. Discovery is bounded and adapter-owned.

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
| `--include-subdomains <bool>` | `false` | Include subdomains under same parent domain. |
| `--json` | `false` | Print structured payload including all discovered URLs. |

## How it works

`axon map` uses a bounded **sitemap-first** strategy:

1. **Sitemap + llms.txt discovery** (primary): fetches `robots.txt`, the default sitemap paths, and `/llms.txt` in parallel with seed URL resolution. Sitemap paths checked: `sitemap.xml`, `sitemap_index.xml`, `sitemap-index.xml`, `wp-sitemap.xml`, `sitemap/sitemap-index.xml`. When `scrape.discover-llms-txt = true` (default), `/llms.txt` at the site root is parsed for markdown links, host-scoped, and **merged** (deduped) into the sitemap URL set. Disable with `scrape.discover-llms-txt = false`.
2. **Bounded root-anchor discovery** (when neither a sitemap nor an llms.txt yields URLs): fetches the scope root page once and extracts anchor hrefs (up to 500 URLs, or `--max-pages` when set). The "scope root" is the path-anchored root for the requested URL: `axon map https://site/docs` fetches `https://site/docs`, not `https://site/`.

> **Important:** the fallback to bounded-structure is triggered by whether any sitemap **or** llms.txt URL was discovered, not by the URL count. If a sitemap was found but all URLs were out of scope, `map_source` will be `"sitemap"` and the URL list will be empty — no anchor fallback is applied in this case.

## Examples

```bash
# Sitemap-first, then one bounded root-anchor fetch
axon map https://example.com/docs

# JSON output for automation
axon map https://example.com --json

# Sitemap limits and discovery behavior live in config.toml under [scrape].
```

## Output

JSON mode returns:

| Field | Type | Description |
|-------|------|-------------|
| `url` | string | The start URL |
| `mapped_urls` | number | Count of discovered URLs in the output |
| `sitemap_urls` | number | Count of sitemap-discovered URLs after deduplication and in-scope filtering |
| `pages_seen` | number | Always `0`; map does not crawl pages. |
| `thin_pages` | number | Always `0`; map does not extract page content. |
| `elapsed_ms` | number | Time taken in milliseconds |
| `map_source` | string | How URLs were discovered: `"sitemap"`, `"sitemap+llms"` (sitemap merged with `/llms.txt`), `"llms"` (no sitemap, but a curated `/llms.txt`), or `"bounded-structure"` |
| `warning` | string or null | Non-null when bounded root-anchor discovery returns fewer than 5 URLs or cannot fetch the scope root. |
| `urls` | array | All discovered URLs, sorted and deduplicated |

## Behavior Notes

- `map` validates the URL before any network calls.
- `map` resolves redirects before deriving its scope. Sitemap discovery runs against the original host in parallel with redirect resolution.
- Path-rooted seeds such as `https://example.github.io/project/` stay scoped to that subtree.
- `map` is synchronous (inline) and does not enqueue jobs.
- Map never initializes Spider or Chrome and never writes crawl output to disk.
- All discovered URLs are validated and canonicalized before output.
