# axon watch
Last Modified: 2026-05-31

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | <code>axon watch create</code><br><code>axon watch delete</code><br><code>axon watch exec</code><br><code>axon watch get</code><br><code>axon watch history</code><br><code>axon watch list</code><br><code>axon watch pause</code><br><code>axon watch resume</code><br><code>axon watch status</code><br><code>axon watch update</code> |
| REST | <code>DELETE /v1/watches/{watch_id}</code><br><code>GET /v1/watches</code><br><code>GET /v1/watches/{watch_id}</code><br><code>GET /v1/watches/{watch_id}/history</code><br><code>GET /v1/watches/{watch_id}/status</code><br><code>PATCH /v1/watches/{watch_id}</code><br><code>POST /v1/watches</code><br><code>POST /v1/watches/{watch_id}/exec</code><br><code>POST /v1/watches/{watch_id}/pause</code><br><code>POST /v1/watches/{watch_id}/resume</code> |
| MCP atomic tools | <code>watch_create</code><br><code>watch_delete</code><br><code>watch_exec</code><br><code>watch_get</code><br><code>watch_history</code><br><code>watch_list</code><br><code>watch_pause</code><br><code>watch_resume</code><br><code>watch_status</code><br><code>watch_update</code> |
| Shared service ownership | [axon-services](../../../crates/axon-services/src/lib.rs) and the owning domain crate; see [crate ownership](../../architecture/crate-ownership.md) |

MCP names describe the atomic projection. The legacy `axon` tool uses the corresponding action/subaction selectors; `both` exposes both projections. Discover the running server before calling. [MCP contract](../mcp/tool-schema.md) owns exact schemas and selectors.

Family-level navigation does not imply identical suboperations or request shapes across transports.
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
axon watch <source> --every-seconds <n> [--collection <name>]
axon watch create <source> --every-seconds <n> [--collection <name>]
axon watch list
axon watch get <id>
axon watch status <id-or-source>
axon watch update <id> [--every-seconds <n>] [--collection <name>]
axon watch pause <id>
axon watch resume <id>
axon watch delete <id>
axon watch exec <id-or-source>
axon watch history <id-or-source> [--limit <n>]
```

`axon watch` with no subcommand defaults to `list`.

## Subcommand Details

| Subcommand | Arguments | Description |
|------------|-----------|-------------|
| `create` | `<source>` | Create a new source watch. `axon watch <source>` is equivalent |
| `list` | — | List source watches |
| `get` | `<id>` | Show a source watch |
| `status` | `<id-or-source>` | Show the watch row and latest source job status |
| `update` | `<id>` | Update a source watch schedule or collection |
| `pause` | `<id>` | Disable scheduled execution |
| `resume` | `<id>` | Re-enable scheduled execution |
| `delete` | `<id>` | Delete a source watch and its run history |
| `exec` | `<id-or-source>` | Dispatch one immediate source job for a watch |
| `history` | `<id-or-source>` | List recent source jobs for a watch. Default `--limit 50` |

### create flags

| Flag | Required | Description |
|------|----------|-------------|
| `--every-seconds <n>` | Yes | Run interval in seconds (30–604800) |
| `--collection <name>` | No | Target vector collection for source watch runs |

### Behavior summary

Each tick leases due enabled watches, enqueues a detached unified source job, and
records the resulting job id in `axon_source_watch_runs`. Source-specific scope,
embedding is enabled by default, ledger generations, and vector publishing follow the same
SourceRequest path as `axon <source>`.

## Examples

```bash
# Create a 5-minute change-detection watch
axon watch create https://docs.rs/spider --every-seconds 300

# List watch definitions
axon watch list --json

# Force one immediate run (pass the UUID from list output, or the source)
axon watch exec https://docs.rs/spider --json

# Inspect the current watch row and latest source job
axon watch status https://docs.rs/spider --json

# Inspect recent run history (default: last 50 runs)
axon watch history https://docs.rs/spider --limit 20
```

## Notes

- The stateless `refresh` task type has been replaced by `watch`. The legacy
  refresh scheduling compatibility surface was already removed; use `axon watch`
  directly.
