# axon dedupe
Last Modified: 2026-06-01

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon dedupe ...` |
| REST | `POST /v1/dedupe` (Implemented) |
| MCP | Not exposed as a dedicated MCP action. |
| Service | `services::system::dedupe` |

Parity notes: Mutating vector maintenance command; migrate remains CLI-only.
<!-- END GENERATED ACTION SURFACES -->


Remove duplicate vectors in Qdrant by `(url, chunk_index)` key.

## Synopsis

```bash
axon dedupe [FLAGS]
```

## Flags

All global flags apply. Key flags for this command:

| Flag | Default | Description |
|------|---------|-------------|
| `--collection <name>` | `axon` | Qdrant collection to deduplicate. Also settable via `AXON_COLLECTION`. |
| `--json` | `false` | Machine-readable summary. |

## Behavior

`dedupe` scans the full collection and groups points by:

- `payload.url`
- `payload.chunk_index`

For each duplicate group, it keeps the newest record by `payload.scraped_at` and deletes older duplicates.

## Examples

```bash
# Deduplicate default collection
axon dedupe

# Deduplicate a specific collection
axon dedupe --collection docs-rag

# JSON summary
axon dedupe --json
```

## Output

JSON mode returns:

```json
{
  "duplicate_groups": 0,
  "deleted": 0,
  "collection": "axon"
}
```

## Notes

- This command is destructive: duplicate points are deleted immediately.
- There is no dry-run mode.
