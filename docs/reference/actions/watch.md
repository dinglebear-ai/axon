# axon watch
Last Modified: 2026-05-31

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon watch ...` |
| REST | Not inventoried |
| MCP | Not exposed as a dedicated MCP action. |
| Service | `Not inventoried` |

Parity notes: This action page is missing from docs/reference/api-parity.md.
<!-- END GENERATED ACTION SURFACES -->


Top-level recurring source scheduler definitions and run history. A watch is a
source-request-backed recurring run: each due tick enqueues one canonical source
job, records the job in watch history, and lets the unified source pipeline own
acquire/prepare/embed/publish behavior.

## Synopsis

```bash
axon watch <SUBCOMMAND> [ARGS]
```

## Subcommands

The following subcommands are implemented:

```bash
axon watch create <name> --task-type <type> --every-seconds <n> [--task-payload <json>]
axon watch list
axon watch exec <id>
axon watch history <id> [--limit <n>]
```

Additional source-watch subcommands are implemented for direct inspection and
lifecycle control:

```bash
axon watch get <id>
axon watch update <id> [--every-seconds <n>]
axon watch pause <id>
axon watch resume <id>
axon watch delete <id>
axon watch artifacts <run_id> [--limit <n>]
```

`axon watch` with no subcommand defaults to `list`.

## Subcommand Details

| Subcommand | Arguments | Description |
|------------|-----------|-------------|
| `create` | `<source>` | Create a new source watch |
| `list` | — | List source watches |
| `exec` | `<id>` | Dispatch one immediate source job for a watch |
| `history` | `<id>` | List recent source jobs for a watch. Default `--limit 50` |

### create flags

| Flag | Required | Description |
|------|----------|-------------|
| `--task-type <type>` | Yes | Type of task (`watch` is the only supported type) |
| `--every-seconds <n>` | Yes | Run interval in seconds (30–604800) |
| `--task-payload <json>` | No | Compatibility payload. Only `{"urls":["<source>"]}` is accepted; richer legacy watch fields are rejected. Defaults to `{}` if omitted. |

When `--task-payload` is omitted or `{}`, the positional `<name>` is treated as
the canonical source selector. When `--task-payload` is supplied, it must contain
exactly one URL/source in `urls`; multiple URLs and legacy fields such as
`max_depth`, `ignore_patterns`, `change_threshold_words`, or `summarize` are
rejected instead of silently ignored.

## Task Payloads

The only supported `task_type` is `watch`.

Compatibility payload shape:

```json
{
  "urls": ["https://docs.example.com/guide/intro"]
}
```

| Field | Default | Description |
|-------|---------|-------------|
| `urls` | — | Exactly one URL or source selector for this watch. |

### Behavior summary

Each tick leases due enabled watches, enqueues a detached unified source job, and
records the resulting job id in `axon_source_watch_runs`. Source-specific scope,
embedding defaults, ledger generations, and vector publishing follow the same
SourceRequest path as `axon <source>`.

## Examples

```bash
# Create a 5-minute change-detection watch
axon watch create docs-watch \
  --task-type watch \
  --every-seconds 300 \
  --task-payload '{"urls":["https://docs.rs/spider"]}'

# List watch definitions
axon watch list --json

# Force one immediate run (pass the UUID from list output)
axon watch exec <uuid> --json

# Inspect recent run history (default: last 50 runs)
axon watch history <uuid> --limit 20
```

## Notes

- The stateless `refresh` task type has been replaced by `watch`. The legacy
  `axon refresh schedule ...` compatibility surface was already removed; use
  `axon watch` directly.
