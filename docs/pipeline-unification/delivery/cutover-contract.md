# Clean-Slate Cutover Contract
Last Modified: 2026-06-30

## Contract

The pipeline unification assumes an empty database and a full reindex after the
refactor lands. There is no requirement to migrate existing local data,
backfill old Qdrant payloads, preserve old job rows, tombstone old data, or
prune old data carefully.

Existing local Axon data may be wiped before or during the cutover.

## Design Rules

- Prefer a fresh schema over compatibility migrations.
- Prefer reindexing over payload backfill.
- Prefer deleting old command/action/route code over tombstoning.
- Do not build read-only legacy views.
- Do not preserve old job-family tables for compatibility.
- Do not write migration jobs for current local data.
- Do not write vector-payload backfill code for the old payload shape.
- Do not keep legacy crate/module names solely to read old state.
- `axon doctor` may detect stale old data and recommend wiping/reinitializing.

## Empty-Store Assumption

Implementation may assume:

| Store | Cutover Behavior |
|---|---|
| SQLite jobs DB | Can be recreated from fresh migrations/schema. |
| Source/code index tables | Can be dropped/replaced by unified ledger tables. |
| Watch tables | Can be dropped/replaced by unified watch/source tables. |
| Memory tables | Can be dropped/replaced by target memory/graph schema. |
| Qdrant collections | Can be deleted/recreated and repopulated. |
| Artifact output | Can be deleted unless explicitly retained by the user outside Axon. |
| Config files | Must be preserved or rewritten intentionally; these are user input, not indexed data. |

Config is the only durable local state that needs careful handling. `.env` and
`config.toml` should be validated and rewritten by explicit user action or clear
setup tooling, not silently discarded.

## Config Cutover

Existing `.env` and `config.toml` files are user-authored input, not indexed
data. They are not migrated automatically, but Axon must make stale config
obvious and recoverable.

Required behavior:

- normal startup validates config before starting workers
- unknown removed keys fail startup with file path, key path, and target
  replacement when known
- `axon doctor --config` reports stale/removed keys, missing required runtime
  URLs/secrets, and config-vs-env placement mistakes
- `axon setup config rewrite --dry-run` prints the desired end-state `.env` and
  `config.toml` edits without writing
- `axon setup config rewrite` rewrites only after explicit confirmation and
  preserves unknown comments where practical
- config rewrite never migrates indexed data, Qdrant payloads, job rows, or
  ledger rows

## Removed Surfaces

Removed CLI commands, MCP actions, REST routes, DTO fields, config keys, and
old help entries should be deleted from normal user-facing schemas.

During the implementation PR, tests may assert either:

- the old surface is absent from parser/router/schema, or
- the old surface fails immediately during parsing with a developer-facing
  message while the deletion is still in progress.

There is no requirement to keep a runtime tombstone window.

## Reindex Plan

After cutover, rebuilding knowledge means running canonical source jobs:

```text
axon <source> --wait
axon <source> --watch
axon watch <source>
axon memory remember "..."
```

SourceLedger, SourceGraph, DocumentStatus, VectorStore payloads, and artifacts
are rebuilt from canonical source inputs.

## What Not To Build

Do not build:

- old Qdrant payload backfill
- legacy job-row migration
- legacy code-index table migration
- legacy watch table migration
- old memory schema migration
- old source list import
- route tombstones for a public deprecation period
- compatibility aliases
- dual-write old/new stores

## Required Cutover Checks

Before declaring the refactor complete:

- fresh SQLite schema initializes
- fresh Qdrant collection initializes
- `axon doctor` reports empty/fresh stores clearly
- old CLI commands are absent or fail before side effects
- old MCP actions are absent from schema
- old REST routes are absent from OpenAPI/router
- canonical source job can index a local repo
- canonical source job can index a web/docs source
- canonical ask/query can retrieve from the new payload shape
- provider backpressure works during fresh reindex

## Optional Reset Tooling

It is acceptable to add an explicit reset command for local development:

```text
axon reset --stores jobs,ledger,graph,memory,vectors,artifacts
```

Reset must be admin/destructive, require confirmation unless `--yes`, and print
exactly what it will delete. This is not migration; it is intentional local
state destruction.
