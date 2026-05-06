---
name: retrieve
description: Use when the user wants to fetch all stored chunks for a specific URL from Qdrant, get everything indexed from a particular page, or see what was stored for a specific source. Triggers on "retrieve from axon", "get the indexed content for this URL", "fetch stored chunks for", "what did axon store from", "show me what's indexed at". Different from query (keyword search) — retrieve fetches by exact URL.
allowed-tools: mcp__plugin_axon_axon__axon
---

# axon-retrieve

Fetches all Qdrant chunks stored for a specific URL.

## MCP (preferred)

```json
{ "action": "retrieve", "url": "https://example.com/docs/article" }
```

## CLI fallback

```bash
axon retrieve https://example.com/docs/article
```

## What it returns

All chunks for the URL: chunk text, chunk index, indexing timestamp.

## Common uses

- **Verify indexing**: if `ask` isn't finding content from a specific page, retrieve directly to confirm it's stored and the chunks look right
- **Inspect chunking**: see exactly how a document was split

## If content is missing or stale

```json
{ "action": "scrape", "url": "https://example.com/docs/article" }
```

Re-scraping overwrites existing chunks for that URL.

## Reading large results

```json
{ "action": "artifacts", "subaction": "grep", "path": ".cache/axon-mcp/<file>", "pattern": "rate limit" }
```
