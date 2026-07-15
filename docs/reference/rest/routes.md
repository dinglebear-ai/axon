# REST Routes
Last Modified: 2026-07-15

This page summarizes REST route ownership. Generated OpenAPI remains the
machine-readable source of truth.

## Route Families

| Family | Purpose |
|---|---|
| sources | source acquisition, status, listing |
| query and ask | retrieval and synthesis |
| jobs | durable job status and control |
| watches | watch lifecycle |
| prune and reset | destructive operations |
| config and system | runtime inspection and diagnostics |

## Removed Route Rule

Removed pre-unification routes must not be mounted as aliases. Compatibility
belongs in migration data handling, not public HTTP routes.
