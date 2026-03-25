# axon watch
Last Modified: 2026-03-25

Top-level recurring scheduler definitions and run history.

## Synopsis

```bash
axon watch <SUBCOMMAND> [ARGS]
```

## Subcommands

The following subcommands are implemented:

```bash
axon watch create <name> --task-type <type> --every-seconds <n> [--task-payload <json>]
axon watch list
axon watch run-now <id>
axon watch history <id> [--limit <n>]
```

The following subcommands are defined in the CLI schema but return "not yet implemented" errors:

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
| `create` | `<name>` | Create a new watch definition |
| `list` | — | List all watch definitions (up to 200) |
| `run-now` | `<id>` | Dispatch one immediate run for a watch definition (UUID) |
| `history` | `<id>` | List recent runs for a watch definition. Default `--limit 50` |

### create flags

| Flag | Required | Description |
|------|----------|-------------|
| `--task-type <type>` | Yes | Type of task (`refresh` is the only dispatched type) |
| `--every-seconds <n>` | Yes | Run interval in seconds (must be >= 1) |
| `--task-payload <json>` | No | JSON payload for the task. Defaults to `{}` if omitted |

## Task Payloads

Current worker dispatch support is `task_type=refresh`.

Refresh payload shape:

```json
{"urls":["https://example.com/docs","https://example.com/api"]}
```

If `urls` is empty or missing, `run-now` records a run but does not dispatch a downstream refresh job.

## Examples

```bash
# Create a 5-minute refresh watch
axon watch create docs-refresh \
  --task-type refresh \
  --every-seconds 300 \
  --task-payload '{"urls":["https://docs.rs/spider"]}'

# List watch definitions
axon watch list --json

# Force one immediate run (pass the UUID from list output)
axon watch run-now <uuid> --json

# Inspect recent run history (default: last 50 runs)
axon watch history <uuid> --limit 20
```

## Relationship to refresh schedule

`axon refresh schedule ...` remains available as a compatibility interface and is backed by watch definitions with `task_type=refresh`.
