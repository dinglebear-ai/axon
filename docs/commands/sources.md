# axon sources
Last Modified: 2026-03-03

Version: 1.0.0
Last Updated: 20:30:18 | 03/03/2026 EST

List indexed source URLs in the active Qdrant collection with chunk counts.

## Synopsis

```bash
axon sources [FLAGS]
```

## Arguments

None.

## Required Environment Variables

| Variable | Description |
|----------|-------------|
| `QDRANT_URL` | Qdrant base URL. |

`sources` reads Qdrant metadata and does not require Postgres, Redis, or AMQP.

## Flags

All global flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--collection <name>` | `cortex` | Qdrant collection to inspect. |
| `--json` | `false` | Emits a JSON object with `count`, `limit`, `offset`, and `urls` (array of `[url, chunk_count]` pairs). |

## Examples

```bash
# Human-readable list
axon sources

# JSON output
axon sources --json

# Different collection
axon sources --collection docs-local
```

## Tuning Environment Variable

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_SOURCES_FACET_LIMIT` | `100000` | Max URL facets fetched from Qdrant (clamped 1..1,000,000). |

## Notes

- `sources` uses Qdrant facet aggregation on `url`.
- If facet results hit the configured limit, human output prints a truncation hint.
