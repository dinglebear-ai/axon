---
name: query
description: Use when the user wants to do a semantic vector search over indexed content, find relevant chunks matching a query, search the Qdrant knowledge base, or get raw search results without LLM synthesis. Triggers on "search axon", "query the knowledge base", "find chunks about", "vector search for", "semantic search", "what's indexed about", "find relevant passages". Different from `ask` (which synthesizes an answer) — query returns raw matching chunks with scores.
allowed-tools: mcp__plugin_axon_axon__axon
---

# axon-query

Pure semantic vector search — returns matching chunks from Qdrant with similarity scores, no LLM synthesis.

## MCP (preferred)

```json
{ "action": "query", "query": "embedding pipeline concurrency" }
```

With options:
```json
{
  "action": "query",
  "query": "rate limiting backoff",
  "limit": 20,
  "since": "7d"
}
```

## CLI fallback

```bash
axon query "embedding pipeline" --limit 20 --since 7d
```

## query vs ask

| | `query` | `ask` |
|---|---|---|
| Returns | Raw chunks + scores | Synthesized answer + citations |
| LLM call | No | Yes |
| Use when | Exploring index, debugging | Answering questions |

## Key options

| Option | Default | Notes |
|--------|---------|-------|
| `limit` | 10 | Top chunks to return |
| `since` | — | Filter by indexing date |
| `hybrid_search` | `true` | `false` for dense-only comparison |
| `collection` | `cortex` | Override collection |
