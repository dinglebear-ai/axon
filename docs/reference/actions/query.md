# axon query
Last Modified: 2026-06-01

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon query ...` |
| REST | `POST /v1/query` (Implemented) |
| MCP | `{ "action": "query" }` |
| Service | `services::query::query` |

Parity notes: Uses canonical request DTO and typed query service result.
<!-- END GENERATED ACTION SURFACES -->


Semantic vector search against the local Qdrant collection. The command embeds the query with TEI, searches Qdrant, reranks candidates, and returns diversified results with snippets.

## Synopsis

```bash
axon query <text> [FLAGS]
axon query --query "<text>" [FLAGS]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<text>` | Query text (positional, or via `--query`). |

## Required Environment Variables

| Variable | Description |
|----------|-------------|
| `TEI_URL` | TEI embeddings base URL. |
| `QDRANT_URL` | Qdrant base URL. |

`query` searches Qdrant through TEI embeddings.

## Flags

All global flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--query <text>` | — | Query text (alternative to positional argument). |
| `--limit <n>` | `10` | Number of query results to return. |
| `--collection <name>` | `axon` | Qdrant collection to search. Also settable via `AXON_COLLECTION`. |
| `--diagnostics` | `false` | Adds per-result debug fields in human output (`vector_score`, full URL). |
| `--since <date>` | — | Filter results to content indexed on or after this date. Accepts `7d`, `30d`, `1w`, `YYYY-MM-DD`, or RFC3339. |
| `--before <date>` | — | Filter results to content indexed on or before this date. Same formats as `--since`. |
| `--no-hybrid-search` | `false` | Disable hybrid (dense + BM42 sparse + RRF) retrieval; force dense-only. Overrides `AXON_HYBRID_SEARCH=true`. |
| `--json` | `false` | Machine-readable output (one JSON object per result line). |

Note: `query` runs synchronously and does not enqueue jobs.

## Examples

```bash
# Basic query
axon query "embedding pipeline"

# Using --query
axon query --query "tokio worker lane reconnect"

# Limit results
axon query "qdrant payload schema" --limit 5

# Diagnostics
axon query "ranking heuristics" --diagnostics
```

## Notes

- Result ranking uses rerank score for final ordering and diversity selection.
- `--wait` has no effect for `query` (command is inline).
