---
name: crawl
description: Use when the user wants to crawl an entire website, documentation site, or multiple pages from a domain; index a whole docs section; or follow links deeply across a site. Triggers on "crawl this site", "index the whole docs", "crawl all pages under", "spider this URL", "index the entire", "grab all pages from". Prefer over scrape when breadth matters — multiple pages across a site.
allowed-tools: mcp__plugin_axon_axon__axon
---

# axon-crawl

Crawls a site recursively, scraping and embedding all discovered pages.

## MCP (preferred)

```json
{ "action": "crawl", "urls": ["https://docs.example.com"] }
```

With limits (always set for large sites):
```json
{
  "action": "crawl",
  "urls": ["https://docs.example.com"],
  "max_pages": 200,
  "max_depth": 3,
  "include_subdomains": false
}
```

## Lifecycle subactions

```json
{ "action": "crawl", "subaction": "status",  "job_id": "<uuid>" }
{ "action": "crawl", "subaction": "cancel",  "job_id": "<uuid>" }
{ "action": "crawl", "subaction": "list",    "limit": 10 }
{ "action": "crawl", "subaction": "cleanup" }
{ "action": "crawl", "subaction": "recover" }
```

## CLI fallback

```bash
axon crawl https://docs.example.com --max-pages 200 --max-depth 3 --wait true
```

## Key options

| Option | Default | Notes |
|--------|---------|-------|
| `max_pages` | 0 (uncapped) | Always set for unknown sites |
| `max_depth` | 5 | Link depth from start URL |
| `include_subdomains` | `false` | Include `*.example.com` |
| `render_mode` | `auto_switch` | `http`, `chrome`, `auto_switch` |

Crawl is async — returns a `job_id` immediately. Poll `subaction: "status"` until complete.
