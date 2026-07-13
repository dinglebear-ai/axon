# sync
Last Modified: 2026-06-01

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon sync ...` |
| REST | Not inventoried |
| MCP | Not exposed as a dedicated MCP action. |
| Service | `Not inventoried` |

Parity notes: This action page is missing from docs/reference/api-parity.md.
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
