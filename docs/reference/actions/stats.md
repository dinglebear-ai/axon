# axon stats
Last Modified: 2026-06-13

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | <code>axon stats</code> |
| REST | <code>GET /v1/stats</code> |
| MCP atomic tools | Not exposed in this registry |
| Shared service ownership | [axon-services](../../../crates/axon-services/src/lib.rs) and the owning domain crate; see [crate ownership](../../architecture/crate-ownership.md) |

MCP names describe the atomic projection. The legacy `axon` tool uses the corresponding action/subaction selectors; `both` exposes both projections. Discover the running server before calling. [MCP contract](../mcp/tool-schema.md) owns exact schemas and selectors.

Family-level navigation does not imply identical suboperations or request shapes across transports.
<!-- END GENERATED ACTION SURFACES -->


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
- `Vector Stats` (collection status, vector counts, docs estimate, average chunks per doc, sampled average chunk/doc token estimates, dimension/distance, segments, payload schema)
- `Pipeline Stats` (source pipeline duration metrics, totals, longest/most-chunks jobs)
- `Freshness` (last indexed age and source indexing counts over last 24h and 7d)
- `Growth (last 7 days)` (per-day chunk counts as a bar chart; omitted if no data)
- `Command Counts` (per-command invocation counts)

## Notes

- Qdrant stats are required; if Qdrant endpoints fail, the command fails.
- Job/command metrics are best-effort: if metric queries fail, affected fields become `null`/`n/a` while Qdrant stats still print.
- Average token estimates prefer a sampled Qdrant scan over indexed chunks and documents. If that sample fails, stats falls back to SQLite job metrics when available.
