---
name: map
description: Use when the user wants to discover all URLs on a site without fetching content, preview what pages exist before crawling, get a sitemap of a site, or explore the URL structure of a domain. Triggers on "map this site", "what pages exist at", "list all URLs on", "discover URLs", "show me the site structure", "find all pages before crawling". Fast and non-destructive — does not embed anything.
allowed-tools: mcp__plugin_axon_axon__axon
---

# axon-map

Discovers all URLs on a site without fetching page content — sitemap first, anchor extraction fallback.

## MCP (preferred)

```json
{ "action": "map", "url": "https://docs.example.com" }
```

## CLI fallback

```bash
axon map https://docs.example.com
```

Does **not** embed anything — purely discovery.

## Typical workflow

```json
// 1. Map to see what's available
{ "action": "map", "url": "https://docs.example.com" }

// 2. Crawl only the sections you need
{ "action": "crawl", "urls": ["https://docs.example.com/guides"], "max_pages": 100 }
```

## Reading output

```json
{ "action": "artifacts", "subaction": "head",   "path": ".cache/axon-mcp/<file>", "limit": 50 }
{ "action": "artifacts", "subaction": "grep",   "path": ".cache/axon-mcp/<file>", "pattern": "/api/" }
```
