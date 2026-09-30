# sync
Last Modified: 2026-06-01

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | <code>axon sync pending</code> |
| REST | Not exposed in this registry |
| MCP atomic tools | Not exposed in this registry |
| Shared service ownership | [axon-services](../../../crates/axon-services/src/lib.rs) and the owning domain crate; see [crate ownership](../../architecture/crate-ownership.md) |

MCP names describe the atomic projection. The legacy `axon` tool uses the corresponding action/subaction selectors; `both` exposes both projections. Discover the running server before calling. [MCP contract](../mcp/tool-schema.md) owns exact schemas and selectors.

Family-level navigation does not imply identical suboperations or request shapes across transports.
<!-- END GENERATED ACTION SURFACES -->


Reconcile locally produced server-mode artifacts with the server.

> **Status: placeholder.** The `sync` command and its `pending` subcommand are wired into the CLI
> but currently report a no-op result (`0 synced, 0 pending`). The reconciliation logic is not yet
> implemented. This doc describes the intended surface; expect the behavior to change.

## Synopsis

```bash
axon sync <SUBCOMMAND>
```

## Subcommands

| Subcommand | Description |
|------------|-------------|
| `pending` | Show local artifacts waiting to be reconciled with the server. |

## Usage

```bash
# Show pending local artifacts (currently always reports 0)
axon sync pending

# JSON output
axon sync pending --json
```

## Behavior

- `sync pending` currently prints `Sync pending: 0 synced, 0 pending` (or `{"synced":0,"pending":0}` with `--json`).
- Any subcommand other than `pending` is rejected with `unknown sync subcommand`.
- This command exists for the legacy artifact reconciliation workflow. Generic CLI server-mode forwarding was removed in 5.0.0; keep this page aligned with the current direct REST/MCP model before expanding the workflow.

## See also

- [Archived server-mode routing contract](../../archive/server-mode-routing-contract.md)
- [`serve`](serve.md) — run the long-running HTTP server.
