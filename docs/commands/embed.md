# axon embed
Last Modified: 2026-03-25

Embed local content into Qdrant. Input can be a file path, directory path, or URL. By default this command enqueues an async embed job and returns a job ID.

## Synopsis

```bash
axon embed [INPUT] [FLAGS]
axon embed <SUBCOMMAND> [ARGS] [FLAGS]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `[INPUT]` | File, directory, or URL to embed. If omitted, defaults to `.cache/axon-rust/output/markdown`. |

## Required Environment Variables

| Variable | Description |
|----------|-------------|
| `AXON_PG_URL` | Required by global config parsing (all commands). |
| `AXON_REDIS_URL` | Required by global config parsing (all commands). |
| `AXON_AMQP_URL` | Required by global config parsing (all commands). |
| `TEI_URL` | TEI embeddings base URL. |
| `QDRANT_URL` | Qdrant base URL. |

## Flags

All global flags apply. Key flags for this command:

| Flag | Default | Description |
|------|---------|-------------|
| `--wait <bool>` | `false` | `false`: enqueue job and return immediately. `true`: run inline and block until embedding completes. |
| `--collection <name>` | `cortex` | Qdrant collection to write to. |
| `--json` | `false` | Machine-readable JSON output. |
| `--yes` | `false` | Skip destructive confirmation prompts (used by `embed clear`). |

Note: `embed` does not use `--limit`.

## Job Subcommands

```bash
axon embed status <job_id>   # show one embed job
axon embed cancel <job_id>   # cancel pending/running embed job
axon embed errors <job_id>   # show stored error_text for job
axon embed list              # list recent embed jobs
axon embed cleanup           # remove failed/canceled embed jobs
axon embed clear             # delete all embed jobs and purge queue (confirmation)
axon embed recover           # reclaim stale/interrupted embed jobs
axon embed worker            # run embed worker inline
```

## Examples

```bash
# Async (default): enqueue and return job ID
axon embed ./docs

# Synchronous inline embedding
axon embed ./docs --wait true

# Embed into a specific collection
axon embed ./README.md --wait true --collection docs-local

# Check status
axon embed status 550e8400-e29b-41d4-a716-446655440000

# JSON list output
axon embed list --json
```

## Notes

- Subcommands and input names can collide. If you need to embed a local path named `status`, pass it as a real path (`./status`) so it is treated as input, not a subcommand.
- `embed clear` is destructive and prompts unless `--yes` is set.
- Without workers running, async jobs stay pending until a worker (`axon embed worker` or `axon-workers`) consumes them.
