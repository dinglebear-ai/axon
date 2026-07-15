# Runtime Storage
Last Modified: 2026-07-15

Runtime storage includes SQLite, Qdrant, artifacts, cache directories, and
configuration files.

## Stores

| Store | Purpose |
|---|---|
| SQLite | jobs, source ledger, graph, memory, observability |
| Qdrant | vector payloads and embeddings |
| artifacts | clean content, screenshots, diagnostics, exports |
| cache | temporary acquisition and preparation data |

## Rule

Storage paths must be configurable through the normal config model and safe to
inspect with doctor and reset dry-runs.
