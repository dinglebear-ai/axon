# axon embed
Last Modified: 2026-06-07

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon embed ...` |
| REST | `POST /v1/embed`, `GET /v1/embed`, `GET /v1/embed/{id}`, `POST /v1/embed/{id}/cancel`, `POST /v1/embed/cleanup`, `DELETE /v1/embed`, `POST /v1/embed/recover` (Implemented) |
| MCP | `{ "action": "embed", "subaction": "..." }` (`embed.start`, `embed.status`, `embed.cancel`, `embed.list`, `embed.cleanup`, `embed.clear`, `embed.recover`) |
| Service | `services::embed::{embed_start_with_context,embed_status,embed_list,embed_cancel,embed_cleanup,embed_clear,embed_recover}` |

Parity notes: REST validates local file inputs with the shared server-side embed guard. CLI-only `embed worker` is local process control.
<!-- END GENERATED ACTION SURFACES -->


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
| `--watch` | `false` | Force foreground code-index watch mode for local file/directory inputs. Local paths watch by default. |
| `--no-watch` | `false` | Run one-shot local embedding instead of the default local watch/index flow. |
| `--collection <name>` | `axon` | Qdrant collection to write to. Also settable via `AXON_COLLECTION`. |
| `--fresh <Nd>` | — | CLI-only: create or update a recurring freshness schedule, for example `--fresh 7d`. |
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
# Local path mode: watch/index local changes in the foreground by default
axon embed ./docs

# One-shot local embedding without ongoing watch/index refresh
axon embed ./docs --no-watch

# Synchronous inline embedding
axon embed ./docs --no-watch --wait true

# Explicitly force local code-index refreshes in the foreground
axon embed ./workspace --watch

# Embed one local file without watching
axon embed ./README.md --no-watch --wait true --collection docs-local

# Check status
axon embed status 550e8400-e29b-41d4-a716-446655440000

# JSON list output
axon embed list --json

# URL/text input: enqueue by default when --wait true is not set
axon embed https://example.com/docs --json

# Keep local docs fresh weekly
axon embed ./docs --fresh 7d
```

## Notes

- Subcommands and input names can collide. If you need to embed a local path named `status`, pass it as a real path (`./status`) so it is treated as input, not a subcommand.
- Generic CLI client-to-server forwarding was removed in 5.0.0. `AXON_SERVER_URL` does not route `axon embed` through HTTP; call the `/v1/embed` REST route or MCP HTTP endpoint directly when using `axon serve` as a remote service.
- `embed clear` is destructive and prompts unless `--yes` is set.
- Existing local file and directory inputs enter the foreground local watch/index flow by default, even when `--wait false` is omitted or explicit. URL/free-text inputs return a queued job by default, and jobs stay pending until a worker process (`axon embed worker`) or long-running server process consumes them.
- `--wait true` runs the submitted embed job in-process and blocks until it finishes. In that mode, `axon embed <input> --json` returns a single top-level object such as `{"job_id":"...","status":"completed"}`.
- `--no-watch` restores one-shot local embedding for scripts or ad hoc local files that should not stay attached to the watcher.
- `--watch` is only valid for local file and directory inputs. It runs the local code-search watcher in the foreground, performs the initial refresh for discovered Git checkouts, and uses the existing `axon-code-index` lifecycle tables.
- `--fresh` is CLI-only in v1. It stores a safe replay snapshot and scheduled runs enqueue normal embed jobs through the service layer; REST/MCP freshness management is not exposed yet.
- `axon embed status <job_id> --json` returns a single top-level job object. The stable fields for automation are `id`, `status`, `target`, `collection`, `metrics`, `result_json`, and `config_json`.
- The local source identifier for file embeds is the `target` field. Do not expect a nested `data.url` / `data.collection` envelope from the CLI.
