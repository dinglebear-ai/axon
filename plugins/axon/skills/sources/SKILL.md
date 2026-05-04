---
name: sources
description: Use when the user wants to see all indexed URLs, list what pages are in the knowledge base, check which URLs have been scraped or crawled, audit what's in the vector store, or find out if a specific URL is indexed. Triggers on "list indexed sources", "what URLs are indexed", "show me the sources", "what's in axon", "which pages are stored", "list all indexed URLs", "what did I crawl". Different from domains (grouped by domain) — sources shows individual URLs.
allowed-tools: mcp__plugin_axon_axon__axon
---

# axon-sources

Lists all indexed URLs with chunk counts and indexing timestamps.

## MCP (preferred)

```json
{ "action": "sources" }
```

## CLI fallback

```bash
axon sources
```

## Reading a large list

Grep for a specific domain or path:
```json
{
  "action": "artifacts",
  "subaction": "grep",
  "path": ".cache/axon-mcp/<file>",
  "pattern": "docs.example.com"
}
```

Check whether a specific URL is indexed:
```json
{
  "action": "artifacts",
  "subaction": "grep",
  "path": ".cache/axon-mcp/<file>",
  "pattern": "example.com/specific-page"
}
```

## If a URL is missing

```json
{ "action": "scrape", "url": "https://..." }
```

## sources vs domains vs stats

| Command | Granularity | Best for |
|---------|-------------|----------|
| `sources` | Per URL | "Is this page indexed?" |
| `domains` | Per domain | Coverage audit by site |
| `stats` | Collection total | "How big is my index?" |
