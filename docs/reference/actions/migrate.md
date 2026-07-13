# migrate
Last Modified: 2026-06-01

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon migrate ...` |
| REST | Not inventoried |
| MCP | Not exposed as a dedicated MCP action. |
| Service | `Not inventoried` |

Parity notes: This action page is missing from docs/reference/api-parity.md.
<!-- END GENERATED ACTION SURFACES -->


Migrate an unnamed-vector Qdrant collection to named-mode, enabling hybrid RRF search (dense + BM42 sparse vectors).

## Synopsis

```bash
axon migrate --from <source> --to <destination>
```

## Flags

| Flag | Required | Description |
|------|----------|-------------|
| `--from <name>` | Yes | Source collection name. Must use the legacy unnamed dense-vector schema. |
| `--to <name>` | Yes | Destination collection name. Created automatically if it does not exist. |

## Usage

```bash
# Migrate the default collection to a new named-mode collection
axon migrate --from cortex --to cortex_v2
```

After migration, update `~/.axon/.env` to point to the new collection:

```bash
AXON_COLLECTION=cortex_v2
```

## Behavior

- Scrolls all points from the source collection using the Qdrant `/points/scroll` API.
- Computes BM42 sparse vectors locally from `chunk_text` payload fields (no TEI calls).
- Upserts named-mode points (dense + BM42 sparse) to the destination collection.
- The destination collection is created automatically if it does not exist. If it already exists as a named collection, migration is idempotent — existing points are re-upserted with fresh sparse vectors.
- The source collection must use the unnamed-vector schema (`"vectors": {"size": N}`). Named collections are rejected with a clear error.
- Progress is logged every 100 pages (~25,600 points).
- The source collection is not modified or deleted.

## Notes

- Migration is a one-time operation. New collections created after migration are already in named-mode and do not require this command.
- At 2.57M points: expect roughly 1–2 hours. At 7M+ points: plan for longer.
- After migration, hybrid RRF search (dense + BM42) is active on the new collection. The old collection remains available for rollback — simply revert `AXON_COLLECTION` in `~/.axon/.env`.
