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
- `axon doctor` must detect incompatible non-empty stores and recommend
  wiping/reinitializing before unified workers start.
- `axon reset` is required local/admin tooling for the clean-slate cutover.

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

## Fate of Existing State

| State | Cutover Fate |
|---|---|
| OAuth/JWT tokens | Invalidated by auth config version/audience/signing-key change; users re-auth. |
| Static bearer token | Preserved only if it is rewritten to the target key name; otherwise startup reports the stale key. |
| Memory records | Dropped with old stores; recreate through `axon memory remember` or session/source reindex. |
| Running jobs | Not resumed; reset/preflight marks old job DB incompatible and requires reset before unified workers start. |
| Pending jobs | Dropped with old job tables; caller resubmits canonical source jobs. |
| Partial source generations | Dropped; only new `SourceLedger` generations created after cutover are searchable. |
| Old watches | Dropped unless recreated through `axon watch <source>`. |
| Old artifacts | Deleted by reset unless user explicitly moves them outside Axon's artifact root before reset. |
| Qdrant vectors | Deleted/recreated; all content is reindexed into the target payload shape. |

Re-auth guidance:

- HTTP clients must fetch new OAuth/bearer credentials after cutover.
- MCP clients must refresh saved server config if endpoint/auth env names
  changed.
- Android/web/Palette clients must discard cached session tokens when the server
  reports a new auth config version.

## Config Cutover

Existing `.env` and `config.toml` files are user-authored input, not indexed
data. They are not migrated automatically, but Axon must make stale config
obvious and recoverable.

Required behavior:

- normal startup validates config before starting workers
- unknown removed keys fail startup with file path, key path, and target
  replacement when known
- `axon preflight --config` reports stale/removed keys, missing required runtime
  URLs/secrets, and config-vs-env placement mistakes
- `axon setup config rewrite --dry-run` prints the desired end-state `.env` and
  `config.toml` edits without writing
- `axon setup config rewrite` rewrites only after explicit confirmation and
  preserves unknown comments where practical
- config rewrite never migrates indexed data, Qdrant payloads, job rows, or
  ledger rows

Known config replacements live in
[surface-removal-contract.md](surface-removal-contract.md#removed-config-keys).
`axon preflight --config`, setup rewrite, and schema validation must use that same
registry.

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
- old watch table migration
- old memory schema migration
- old source list import
- route tombstones for a public deprecation period
- compatibility aliases
- dual-write old/new stores

## Required Cutover Checks

Before declaring the refactor complete:

- `axon preflight --config` reports config placement/staleness accurately
- preflight inventories SQLite, Qdrant, artifacts, config, and generated schemas
- incompatible non-empty stores block unified workers until reset or explicit
  developer override
- `axon reset --dry-run` prints exact stores, paths, collections, row counts,
  artifact counts, and generated reset receipt path
- `axon reset --yes` deletes selected local stores, recreates fresh schema, and
  writes a reset receipt artifact
- SQLite integrity checks pass after reset
- Qdrant collection shape matches the target vector payload schema after reset
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

## Required Reset Tooling

Add an explicit reset command for local development and cutover:

```text
axon reset --stores jobs,ledger,graph,memory,vectors,artifacts
```

Reset must be admin/destructive, require confirmation unless `--yes`, and print
exactly what it will delete. This is not migration; it is intentional local
state destruction.

Reset result shape:

```json
{
  "job_id": "job_...",
  "reset_id": "reset_...",
  "stores": ["jobs", "ledger", "graph", "memory", "vectors", "artifacts"],
  "dry_run": false,
  "deleted": {
    "sqlite_tables": 42,
    "qdrant_collections": ["axon"],
    "artifact_files": 120
  },
  "created": {
    "sqlite_schema_version": 1,
    "qdrant_collections": ["axon"]
  },
  "receipt_artifact_id": "art_...",
  "warnings": []
}
```
