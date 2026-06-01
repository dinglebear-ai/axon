# axon stats
Last Modified: 2026-06-01

Show vector and pipeline statistics for the active collection. Combines Qdrant collection snapshots with job/command metrics derived from the local SQLite jobs database.

## Synopsis

```bash
axon stats [FLAGS]
```

## Arguments

None.

## Required Environment Variables

| Variable | Description |
|----------|-------------|
| `QDRANT_URL` | Qdrant base URL. |

`stats` reads Qdrant collection data and SQLite job metrics.

## Flags

All global flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--collection <name>` | `axon` | Qdrant collection to inspect. Also settable via `AXON_COLLECTION`. |
| `--json` | `false` | Full stats payload as JSON. |

## Examples

```bash
# Human-readable stats panels
axon stats

# JSON payload
axon stats --json

# Different collection
axon stats --collection docs-local
```

## Output Sections

Human output prints five sections:
- `Vector Stats` (collection status, vector counts, docs estimate, dimension/distance, segments, payload schema)
- `Pipeline Stats` (crawl/embed duration metrics, totals, longest/most-chunks jobs)
- `Freshness` (last indexed age, crawl counts over last 24h and 7d)
- `Growth (last 7 days)` (per-day chunk counts as a bar chart; omitted if no data)
- `Command Counts` (per-command invocation counts)

## Notes

- Qdrant stats are required; if Qdrant endpoints fail, the command fails.
- Job/command metrics are best-effort: if metric queries fail, affected fields become `null`/`n/a` while Qdrant stats still print.
