# axon status
Last Modified: 2026-03-20

Show local job state across crawl, extract, embed, ingest, refresh, and graph queues.

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

## Output

Human output prints grouped sections and status breakdowns for:

- Crawls
- Refresh
- Embeds
- Ingests
- Extracts
- Graph

JSON output shape:

```json
{
  "local_crawl_jobs": [...],
  "local_extract_jobs": [...],
  "local_embed_jobs": [...],
  "local_ingest_jobs": [...],
  "local_refresh_jobs": [...],
  "local_graph_jobs": [...]
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
```

## Notes

- `status` loads up to 20 recent jobs per queue family.
- By default, watchdog-reclaimed failures are hidden. `--reclaimed` flips to reclaimed-only mode.
- `--active` and `--recent` apply to graph jobs as well as other job families.
- This command is read-only and does not enqueue or mutate jobs.
