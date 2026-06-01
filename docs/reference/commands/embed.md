# axon embed
Last Modified: 2026-06-01

Embed local content into Qdrant. Input can be a file path, directory path, or URL. In `--json` mode, stdout is a single machine-readable JSON object with no progress chatter mixed in.

## Synopsis

```bash
axon embed [INPUT] [FLAGS]
axon embed <SUBCOMMAND> [ARGS] [FLAGS]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `[INPUT]` | File, directory, URL, or raw text to embed. If omitted, defaults to `<output-dir>/markdown` (i.e. `.cache/axon-rust/output/markdown`). |

## Required Environment Variables

| Variable | Description |
|----------|-------------|
| `TEI_URL` | TEI embeddings base URL. |
| `QDRANT_URL` | Qdrant base URL. |

`embed` writes vectors to Qdrant through TEI embeddings.

## Flags

All global flags apply. Key flags for this command:

| Flag | Default | Description |
|------|---------|-------------|
| `--wait <bool>` | `false` | `false`: enqueue job and return immediately. `true`: run inline and block until embedding completes. |
| `--collection <name>` | `axon` | Qdrant collection to write to. Also settable via `AXON_COLLECTION`. |
| `--json` | `false` | Machine-readable JSON output. |
| `--yes` | `false` | Skip destructive confirmation prompts (used by `embed clear`). |

With `--wait false`, `embed` writes a SQLite job row and exits without draining
the embed table. Use `--wait true` to run the embed operation inline and block
until that operation finishes.

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
# Default mode: start embed job and return job JSON
axon embed ./docs

# Synchronous inline embedding
axon embed ./docs --wait true

# Embed into a specific collection
axon embed ./README.md --wait true --collection docs-local

# Check status
axon embed status 550e8400-e29b-41d4-a716-446655440000

# JSON list output
axon embed list --json

# Server mode with URL/text input
AXON_SERVER_URL=http://127.0.0.1:8001 axon embed https://example.com/docs --json
```

## Notes

- Subcommands and input names can collide. If you need to embed a local path named `status`, pass it as a real path (`./status`) so it is treated as input, not a subcommand.
- In server mode (`AXON_SERVER_URL`), URL and text inputs are submitted to `axon serve`. Host-local paths such as `./README.md` are rejected with a clear error because the server normally runs in a Docker container with a different filesystem. Use `--local` for local file/directory embedding until upload/import support is added.
- `embed clear` is destructive and prompts unless `--yes` is set.
- `--wait false` returns a queued job by default, and jobs stay pending until a worker process (`axon embed worker`) or long-running server process consumes them.
- `--wait true` runs the submitted embed job in-process and blocks until it finishes. In that mode, `axon embed <input> --json` returns a single top-level object such as `{"job_id":"...","status":"completed"}`.
- `axon embed status <job_id> --json` returns a single top-level job object. The stable fields for automation are `id`, `status`, `target`, `collection`, `metrics`, `result_json`, and `config_json`.
- The local source identifier for file embeds is the `target` field. Do not expect a nested `data.url` / `data.collection` envelope from the CLI.
