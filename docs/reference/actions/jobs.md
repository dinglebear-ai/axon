# axon jobs
Last Modified: 2026-08-02

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon jobs ...` |
| REST | See docs/reference/rest/routes.md |
| MCP | `{ "action": "jobs" }` |
| Service | `Shared domain/service implementation` |
<!-- END GENERATED ACTION SURFACES -->


Request cancellation for a unified durable job

## Commands

| Command | Summary |
|---|---|
| `axon jobs cancel` | Request cancellation for a unified durable job |
| `axon jobs cleanup` | Remove old terminal unified durable jobs |
| `axon jobs clear` | Clear terminal unified durable job rows; active jobs require cancel/recover first |
| `axon jobs events` | Show one job's event page |
| `axon jobs get` | Show one unified durable job |
| `axon jobs list` | List unified durable jobs |
| `axon jobs recover` | Recover stale unified durable jobs |
| `axon jobs retry` | Retry a unified durable job |
| `axon jobs stream` | Fetch an event page for stream consumers |
| `axon jobs worker` | Run a standalone worker process for the unified durable queue |

## Help

```bash
axon jobs --help
```

The generated [CLI registry](../cli/commands.md) is authoritative for the full command inventory.
