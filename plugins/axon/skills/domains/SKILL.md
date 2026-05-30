---
name: domains
description: Use when the user wants to see which domains have indexed content, get a summary grouped by domain, check how many pages from each site are stored, or audit coverage by website. Triggers on "list indexed domains", "which domains are in axon", "how many pages from each site", "domain breakdown", "show domains", "what sites are indexed". Similar to sources but grouped by domain — use domains for a high-level view, sources for individual URL details.
allowed-tools: mcp__plugin_axon_axon__axon
---

# axon-domains

Lists indexed domains with URL count and chunk totals — a high-level view of what's in the knowledge base.

## MCP (preferred)

```json
{ "action": "domains" }
```

## CLI fallback

```bash
axon domains
```

## What it shows

- Domain name (e.g., `docs.example.com`)
- Number of indexed URLs from that domain
- Total chunk count across those URLs
- Last indexed timestamp

## domains vs sources vs stats

| Command | Granularity | Best for |
|---------|-------------|----------|
| `domains` | Per domain | "Do I have X site indexed?" |
| `sources` | Per URL | "Is this specific page stored?" |
| `stats` | Collection total | "How big is my index?" |

## Identifying gaps

Low URL count on a crawled domain? The crawl may have been capped or many pages were thin. Re-crawl with higher `max_pages`:
```json
{
  "action": "crawl",
  "urls": ["https://docs.example.com"],
  "max_pages": 500,
  "max_depth": 4
}
```
