---
name: ask
description: Use when the user wants to ask a question and get an LLM-synthesized answer grounded in indexed documents, do RAG over previously crawled or embedded content, get cited answers from the knowledge base, or find information that was previously indexed. Triggers on "ask axon", "what does the documentation say about", "according to what I've indexed", "RAG query", "use axon to answer", or any question where the user wants grounded answers from indexed content rather than hallucination.
allowed-tools: mcp__plugin_axon_axon__axon
---

# axon-ask

RAG query: retrieves relevant chunks from Qdrant then synthesizes an answer with citations.

## MCP (preferred)

```json
{ "action": "ask", "query": "How does axon handle Chrome auto-switching?" }
```

With temporal filter:
```json
{ "action": "ask", "query": "latest rust async patterns", "since": "7d" }
```

With diagnostics (shows retrieval scores):
```json
{ "action": "ask", "query": "embedding pipeline", "diagnostics": true }
```

## CLI fallback

```bash
axon ask "how does axon handle Chrome auto-switching?" --since 7d --diagnostics
```

## Key options

| Option | Default | Notes |
|--------|---------|-------|
| `since` | — | `7d`, `30d`, `YYYY-MM-DD` — filters by **indexing date** |
| `before` | — | Upper date bound |
| `diagnostics` | `false` | Show retrieval scores and sources |
| `hybrid_search` | `true` | `false` for dense-only comparison |
| `graph` | `false` | Deprecated compatibility option; graph retrieval is not available in the current runtime |
| `collection` | `cortex` | Override collection per-request |

## When results are poor

1. Add `diagnostics: true` — check retrieval scores
2. Try `hybrid_search: false` to compare approaches
3. Check `{ "action": "sources" }` to verify content is indexed
