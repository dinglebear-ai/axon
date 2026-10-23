---
title: "Ask, Query, Retrieve, And Search"
created: 2026-07-15
updated: 2026-07-30
---

# Ask, Query, Retrieve, And Search
Last Modified: 2026-07-15

These commands have intentionally separate responsibilities.

## Commands

| Command | Purpose |
|---|---|
| `search` | external web discovery, optionally followed by indexing |
| `query` | vector and metadata retrieval from indexed content |
| `retrieve` | fetch stored chunks for a known source or URL |
| `ask` | retrieval plus LLM synthesis with citations |

## Guidance

Use `search` when the answer may not be indexed yet. Use `query` when you want
ranked evidence. Use `retrieve` when you already know the source identity. Use
`ask` when you want a synthesized answer grounded in indexed context.

The commands must not bypass source identity, freshness, vector filters, or
authorization rules.

## Query score interpretation

Ordinary query output labels its provider value as `score`. Dense search returns
similarity and hybrid search returns a fusion score. Structured hits identify
this with `score_kind` (`dense_similarity`, `hybrid_fusion`, or `code_search`);
neither is an answer
probability. The serialized `rerank_score` field is retained for compatibility
and currently mirrors the retrieval score in this path. Its presence does not
mean a separate reranking model ran.
