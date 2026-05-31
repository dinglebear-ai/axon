---
name: search
description: Use when the user wants to search the web via Tavily and index the results, find recent information on a topic and store it, or combine live web search with automatic crawling. Triggers on "search the web for", "find recent articles about", "search and index", "Tavily search", or when the user wants to pull fresh web content into axon. Different from `query` — this searches the live web, not already-indexed content.
allowed-tools: mcp__plugin_axon_axon__axon
---

# axon-search

Tavily web search that auto-queues crawl jobs for the top results, adding them to Qdrant.

## MCP (preferred)

```json
{ "action": "search", "query": "rust async patterns 2025" }
```

With time range:
```json
{
  "action": "search",
  "query": "kubernetes ingress comparison",
  "search_time_range": "month"
}
```

`search_time_range` ∈ `day | week | month | year`

## CLI fallback

```bash
axon search "rust async patterns"
axon research "current state of Rust async"   # search + LLM synthesis, no indexing
```

## What happens

1. Tavily returns top results for the query
2. axon auto-queues crawl jobs for each result URL
3. Results are embedded into Qdrant as crawls complete

## After searching

```json
{ "action": "ask", "query": "rust async patterns", "since": "1d" }
```

Requires `TAVILY_API_KEY`. Run `{ "action": "doctor" }` if search fails.
