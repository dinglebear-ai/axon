# axon query
Last Modified: 2026-03-03

Version: 1.0.0
Last Updated: 20:30:18 | 03/03/2026 EST

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

`query` searches Qdrant and does not require Postgres, Redis, or AMQP.

## Flags

All global flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--query <text>` | — | Query text (alternative to positional argument). |
| `--limit <n>` | `10` | Number of query results to return. |
| `--collection <name>` | `cortex` | Qdrant collection to search. |
| `--diagnostics` | `false` | Adds per-result debug fields in human output (`vector_score`, full URL). |
| `--since <date>` | — | Filter results to content indexed on or after this date. Accepts `7d`, `30d`, `1w`, `YYYY-MM-DD`, or RFC3339. |
| `--before <date>` | — | Filter results to content indexed on or before this date. Same formats as `--since`. |
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
