# Database Schema and Migration Contract
Last Modified: 2026-06-30

## Contract

SQLite schema is a product contract. `axon-ledger`, `axon-jobs`, `axon-graph`,
`axon-memory`, `axon-prune`, and supporting stores own their tables explicitly.

This refactor assumes empty stores at cutover. Migrations are still required for
forward development after the new schema exists, but there is no requirement to
migrate old Axon data into the new model.

## Schema Ownership

The canonical target table list is the `Required Tables` registry in
[../schemas/database-schema.md](../schemas/database-schema.md). This ownership
table is a projection of that registry; drift checks must compare them exactly.

| Owner | Tables |
|---|---|
| `axon-jobs` | `jobs`, `job_attempts`, `job_events`, `job_heartbeats`, `provider_reservations`, `watches`, `watch_runs`, `config_snapshots` |
| `axon-ledger` | `sources`, `source_generations`, `source_items`, `source_manifests`, `document_status`, `cleanup_debt`, `leases` |
| `axon-graph` | `graph_nodes`, `graph_edges`, `graph_evidence`, `graph_aliases`, `graph_conflicts` |
| `axon-memory` | `memory_records`, `memory_links`, `memory_reinforcement`, `memory_reviews` |
| `axon-core` / artifact store | `artifacts` when filesystem metadata index is SQLite-backed |
| `axon-observe` | `provider_health` |

Schema rules:

- Memory decay state is stored in `memory_records.decay_json`; there is no
  separate `memory_decay` table in the target schema.
- Watch progress is represented by `job_events` for `job_kind=watch`; there is
  no separate `watch_events` table.
- Config snapshots are owned by `axon-jobs` as `config_snapshots`; there is no
  `job_config_snapshots` table.

## Migration Rules

- every schema change is a numbered migration
- migrations are idempotent within SQLite transaction semantics
- migrations never silently drop data outside explicit reset paths
- empty-store bootstrap and upgraded-store migration both use the same migration
  runner
- schema version is recorded in SQLite
- failed migration leaves the previous schema usable or the database marked
  unusable with a clear error

## Empty Cutover Rule

For the pipeline unification cutover:

- old local data may be wiped
- old code-search/code-index tables do not need migration
- old vector payloads do not need backfill
- old job rows do not need migration
- implementation may provide `axon reset --all` to initialize clean stores

## Required Indexes

Minimum indexes:

- `sources(source_canonical_uri)`
- `sources(source_kind, source_canonical_uri)`
- `source_items(source_id, source_item_key, generation)`
- `source_items(source_id, item_canonical_uri)`
- `source_generations(source_id, status, created_at)`
- `document_status(source_id, generation, source_item_key)`
- `cleanup_debt(status, next_retry_at)`
- `leases(lease_key, expires_at)`
- `jobs(status, priority, created_at)`
- `job_events(job_id, sequence)`
- `graph_nodes(kind, stable_key)`
- `graph_edges(kind, from_node_id, to_node_id)`
- `memory_records(status, importance, next_review_at)`
- `watches(enabled, next_run_at)`

## Foreign Keys and Integrity

- SQLite foreign keys are enabled on every connection
- source item rows reference source/generation rows
- document status references source item identity
- graph evidence references source ids/items when applicable
- cleanup debt references source/job ids when applicable
- job events reference job id
- deletes are explicit and driven by reset/prune policy

## Schema Review Checklist

Every new table requires:

- owning crate
- purpose
- primary key
- uniqueness constraints
- indexes
- retention policy
- reset behavior
- fake-store representation
- test fixture

## Testing Requirements

- fresh database migrates to latest
- repeated migration is no-op
- migration failure is reported with migration id
- foreign keys are enforced
- indexes support expected query plans
- reset drops/recreates all owned tables
