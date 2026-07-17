# axon status
Last Modified: 2026-06-01

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon status ...` |
| REST | Not inventoried |
| MCP | Not exposed as a dedicated MCP action. |
| Service | `Not inventoried` |

Parity notes: This action page is missing from docs/reference/api-parity.md.
<!-- END GENERATED ACTION SURFACES -->


Show local job state across source, extract, watch, and prune queues.

## Synopsis

```bash
axon status [FLAGS]
```

## Flags

All global flags apply. Key flags for this command:

| Flag | Default | Description |
|------|---------|-------------|
| `--json` | `false` | Print machine-readable JSON status payload. |
| `--reclaimed` | `false` | Show only watchdog-reclaimed stale-running failures. |
| `--active` | `false` | Show only active jobs (pending/running). |
| `--recent` | `false` | Show active + completed jobs (hide failed/canceled). |
| `--watch` | `false` | Live-update mode: redraw the status view on an interval (human output only; ignored with `--json`/`--quiet`). |

## Output

Human output prints grouped sections and status breakdowns for:

- Sources
- Extracts
- Watches
- Prunes

JSON output shape:

```json
{
  "source_jobs": [...],
  "extract_jobs": [...],
  "watch_jobs": [...],
  "prune_jobs": [...],
  "totals": {
    "source": 0,
    "extract": 0,
    "watch": 0,
    "prune": 0
  }
}
```

## Examples

```bash
# Human summary
axon status

# JSON payload
axon status --json

# Only watchdog-reclaimed stale jobs
axon status --reclaimed --json

# JSON status
axon status --json
```

## Notes

- `status` loads up to 20 recent jobs per queue family.
- Generic CLI client-to-server forwarding was removed in 5.0.0. `AXON_SERVER_URL` does not route `axon status` through HTTP; call the `/v1/status` REST route or MCP HTTP endpoint directly when using `axon serve` as a remote service.
- By default, watchdog-reclaimed failures are hidden. `--reclaimed` flips to reclaimed-only mode.
- `--active` and `--recent` apply across canonical job kinds in the unified store.
- This command is read-only and does not enqueue or mutate jobs.
