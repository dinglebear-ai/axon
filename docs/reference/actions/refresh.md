# axon refresh
Last Modified: 2026-06-08

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon refresh ...` |
| REST | Missing |
| MCP | Not exposed as a dedicated MCP action. |
| Service | `services::refresh::{plan_refresh,execute_refresh}` |

Parity notes: Re-enqueues crawl/ingest jobs for indexed origins; CLI-only, gated behind an interactive confirmation.
<!-- END GENERATED ACTION SURFACES -->


Re-enqueue crawl/ingest jobs for previously indexed origins — a full docs refresh.

## Synopsis

```bash
axon refresh [FILTER] [FLAGS]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `FILTER` | Optional. A `source_type` (`crawl`/`embed`/`scrape`/`github`/`gitlab`/`gitea`/`git`/`reddit`/`youtube`) or a `seed_url` substring (e.g. a domain). Omit to refresh every indexed origin. |

## Flags

All global flags apply. Key flags for this command:

| Flag | Default | Description |
|------|---------|-------------|
| `--yes` | `false` | Skip the confirmation prompt before re-enqueuing. Also auto-confirmed when stdout is not a TTY. |
| `--json` | `false` | Machine-readable summary. |
| `--collection <name>` | `axon` | Qdrant collection to read origins from. Also settable via `AXON_COLLECTION`. |

## Behavior

Each indexed chunk carries a `seed_url` payload field — the crawl start URL or ingest
target that originated it (see the [`seed_url` origin tracking](../qdrant-payload-schema.md)
note). `refresh`:

1. Facets the collection on `seed_url`, scoped per `source_type`.
2. Classifies each distinct origin:
   - web URL (`http(s)://…`) → **crawl** (re-enqueue a crawl job seeded from the URL)
   - ingest target (github/gitlab/gitea/git/reddit/youtube) → **ingest** (re-enqueue via `classify_target`)
   - sessions or non-URL origin → **skip** (not re-runnable from an origin marker)
3. Prints the plan and confirms (`confirm_destructive`, respects `--yes` / non-TTY).
4. Enqueue-only — never blocks on `--wait`. Per-origin failures are collected and reported, not fatal.

Only content indexed with a `seed_url` participates. Chunks indexed before origin tracking
shipped (payload schema < 5) carry no `seed_url` and are invisible to the facet — re-crawl or
re-ingest them once to backfill the marker.

## Examples

```bash
# Plan + refresh every indexed origin (prompts for confirmation)
axon refresh

# Only re-ingest GitHub origins
axon refresh github

# Only re-crawl origins under a domain
axon refresh docs.rs

# Non-interactive, machine-readable
axon refresh --yes --json
```

## Output

JSON mode returns:

```json
{
  "crawl_enqueued": 12,
  "ingest_enqueued": 3,
  "skipped": 1,
  "failures": []
}
```

## Notes

- Facet breadth is bounded by `AXON_REFRESH_FACET_LIMIT` (default `10000`).
- Re-enqueued crawl/ingest jobs re-stamp `seed_url`, so refreshed content stays consistent.
- The crawl queue cap (`AXON_MAX_PENDING_CRAWL_JOBS`) still applies; origins beyond the cap are reported as failures rather than silently dropped.
