---
name: stats
description: Use when the user wants to see Qdrant collection statistics, check how many points or vectors are indexed, see collection size, or get an overview of the vector store. Triggers on "axon stats", "how many vectors", "collection size", "how many points in Qdrant", "vector store stats", "how much is indexed", "collection stats". Different from sources (URL list) and status (job queue).
allowed-tools: mcp__plugin_axon_axon__axon
---

# axon-stats

Qdrant collection statistics — point count, vector count, memory usage, collection config.

## MCP (preferred)

```json
{ "action": "stats" }
```

Override collection:
```json
{ "action": "stats", "collection": "my_collection" }
```

## CLI fallback

```bash
axon stats
```

## What it shows

- Total point count (each chunk = one point)
- Named vector counts (`dense`, `bm42` sparse)
- Collection segment count and memory usage
- Collection schema (named vs unnamed vectors)

## Related

```json
{ "action": "sources" }   // which URLs are indexed
{ "action": "domains" }   // which domains have content
{ "action": "doctor" }    // verify Qdrant is reachable
```
