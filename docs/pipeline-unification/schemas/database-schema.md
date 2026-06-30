# Database Schema Contract
Last Modified: 2026-06-30

## Contract

Database schema artifacts are generated from owning store migrations and schema
metadata. Runtime ownership lives in [../runtime/schema-contract.md](../runtime/schema-contract.md).

## Generated Artifacts

```text
docs/reference/runtime/database-schema.json
docs/reference/runtime/database-schema.md
```

Generator:

```bash
cargo xtask schemas database
cargo xtask schemas database --check
```

## Source Inputs

The database schema generator reads:

```text
crates/axon-jobs/src/migrations/**
crates/axon-ledger/src/migrations/**
crates/axon-graph/src/migrations/**
crates/axon-memory/src/migrations/**
crates/axon-core/src/artifact*/**
crates/axon-*/src/schema.rs
crates/axon-*/src/store.rs
```

The generated artifact records these paths in `x-axon.source_inputs`.

## Required Schema Metadata

Every table includes:

- owning crate
- migration introduced
- columns
- primary key
- foreign keys
- unique constraints
- indexes
- retention policy
- reset behavior
- store trait that owns access
- row visibility classification when rows may be exposed through REST/MCP
- test fixture paths
- expected SQLite introspection output

## Root Artifact Shape

```json
{
  "$id": "https://axon.local/schemas/runtime/database-schema.json",
  "x-axon": {
    "owner_crates": ["axon-jobs", "axon-ledger", "axon-graph", "axon-memory"],
    "generated_by": "cargo xtask schemas database"
  },
  "schema_version": 1,
  "generated_at": "2026-06-30T20:20:00Z",
  "tables": [],
  "migrations": [],
  "indexes": [],
  "foreign_keys": [],
  "views": []
}
```

## Table Record Shape

```json
{
  "name": "source_generations",
  "owner_crate": "axon-ledger",
  "introduced_in": "v001_initial.sql",
  "primary_key": ["source_id", "generation"],
  "columns": [
    {
      "name": "source_id",
      "type": "text",
      "nullable": false,
      "references": "sources.source_id"
    }
  ],
  "indexes": [
    {
      "name": "idx_source_generations_status",
      "columns": ["source_id", "status", "created_at"],
      "unique": false
    }
  ],
  "retention": "last_2_committed_plus_active_cleanup_debt",
  "reset_behavior": "drop_and_recreate",
  "store_trait": "LedgerStore",
  "visibility": "internal",
  "fixtures": {
    "valid": "crates/axon-ledger/tests/fixtures/schema/source_generations.valid.json",
    "invalid": "crates/axon-ledger/tests/fixtures/schema/source_generations.invalid.json"
  }
}
```

## Column Record Shape

```json
{
  "name": "source_id",
  "type": "text",
  "nullable": false,
  "default": null,
  "primary_key_position": 1,
  "references": {
    "table": "sources",
    "column": "source_id",
    "on_delete": "cascade",
    "on_update": "cascade"
  },
  "check": null,
  "visibility": "public",
  "redaction": "none",
  "description": "Stable source id."
}
```

Column rules:

- `type` uses SQLite storage classes: `integer`, `real`, `text`, `blob`,
  `numeric`, or generated JSON-text aliases documented in `description`
- JSON columns end with `_json`
- timestamps end with `_at` and store RFC3339 UTC text
- booleans store integer `0`/`1`
- ids use stable prefixes from `schemas/README.md`
- public columns must not contain secrets or raw local-only paths

## Index Record Shape

```json
{
  "name": "idx_jobs_status_priority_created",
  "table": "jobs",
  "columns": ["status", "priority", "created_at"],
  "unique": false,
  "where": "status in ('queued','running','waiting')",
  "purpose": "worker scheduling",
  "required_for": ["JobStore::claim_next"]
}
```

Every non-trivial query in a store trait must name the index that makes it
bounded.

## Migration Record Shape

```json
{
  "id": "v001_initial",
  "owner_crate": "axon-ledger",
  "path": "crates/axon-ledger/src/migrations/v001_initial.sql",
  "checksum": "sha256:...",
  "creates": ["sources", "source_generations"],
  "alters": []
}
```

## Required Table Families

- jobs/events/heartbeats/reservations
- source ledger/manifests/generations/items
- document status
- cleanup debt
- leases
- graph nodes/edges/evidence
- memory records/links/reinforcement/review; decay state lives in
  `memory_records.decay_json`, not a separate table
- watches/runs
- artifacts metadata
- provider health/config snapshots

## Required Tables

Minimum target tables:

| Table | Owner | Primary Key |
|---|---|---|
| `jobs` | `axon-jobs` | `job_id` |
| `job_attempts` | `axon-jobs` | `attempt_id` |
| `job_events` | `axon-jobs` | `(job_id, sequence)` |
| `job_heartbeats` | `axon-jobs` | `job_id` |
| `provider_reservations` | `axon-jobs` | `reservation_id` |
| `sources` | `axon-ledger` | `source_id` |
| `source_generations` | `axon-ledger` | `(source_id, generation)` |
| `source_items` | `axon-ledger` | `(source_id, generation, source_item_key)` |
| `source_manifests` | `axon-ledger` | `(source_id, generation)` |
| `document_status` | `axon-ledger` | `document_id` |
| `cleanup_debt` | `axon-ledger` | `debt_id` |
| `leases` | `axon-ledger` | `lease_key` |
| `graph_nodes` | `axon-graph` | `node_id` |
| `graph_edges` | `axon-graph` | `edge_id` |
| `graph_evidence` | `axon-graph` | `evidence_id` |
| `graph_aliases` | `axon-graph` | `(kind, alias_key)` |
| `graph_conflicts` | `axon-graph` | `conflict_id` |
| `memory_records` | `axon-memory` | `memory_id` |
| `memory_links` | `axon-memory` | `link_id` |
| `memory_reinforcement` | `axon-memory` | `reinforcement_id` |
| `memory_reviews` | `axon-memory` | `review_id` |
| `watches` | `axon-jobs` | `watch_id` |
| `watch_runs` | `axon-jobs` | `run_id` |
| `artifacts` | `axon-core` | `artifact_id` |
| `provider_health` | `axon-observe` | `(provider_id, checked_at)` |
| `config_snapshots` | `axon-jobs` | `config_snapshot_id` |

This table list is canonical. Runtime schema docs, store contracts, reset
plans, and generated migration docs must project this exact list and owner map.
There is no target `memory_decay`, `watch_events`, or `job_config_snapshots`
table; decay lives in `memory_records.decay_json`, watch progress uses
`job_events`, and config snapshots are `config_snapshots`.

## Required Indexes

| Index | Table | Columns | Purpose |
|---|---|---|---|
| `idx_jobs_claim` | `jobs` | `status`, `priority`, `created_at` | Claim queued jobs without full scan. |
| `idx_jobs_source_status` | `jobs` | `source_id`, `status`, `created_at` | Source job history. |
| `idx_job_events_job_sequence` | `job_events` | `job_id`, `sequence` | Event paging/SSE resume. |
| `idx_source_items_changed` | `source_items` | `source_id`, `generation`, `status` | Diff/publish planning. |
| `idx_document_status_source` | `document_status` | `source_id`, `generation`, `status` | Document listing/status. |
| `idx_cleanup_debt_status` | `cleanup_debt` | `status`, `next_retry_at` | Cleanup worker queue. |
| `idx_graph_nodes_kind_key` | `graph_nodes` | `kind`, `stable_key` | Node merge/resolve. |
| `idx_graph_edges_from_kind` | `graph_edges` | `from_node_id`, `kind` | Neighbor lookup. |
| `idx_graph_edges_to_kind` | `graph_edges` | `to_node_id`, `kind` | Reverse lookup. |
| `idx_memory_scope_status` | `memory_records` | `scope_kind`, `scope_value`, `status` | Scoped memory recall. |
| `idx_memory_review` | `memory_reviews` | `status`, `review_after` | Review queue. |
| `idx_watches_due` | `watches` | `enabled`, `next_run_at` | Scheduler due scan. |
| `idx_artifacts_owner` | `artifacts` | `source_id`, `job_id`, `kind` | Artifact lists. |

## Required Column Sets

`jobs`:

- `job_id`
- `job_kind`
- `status`
- `priority`
- `source_id`
- `request_json`
- `auth_snapshot_json`
- `config_snapshot_id`
- `created_at`
- `updated_at`
- `started_at`
- `completed_at`
- `last_error_json`

`sources`:

- `source_id`
- `canonical_uri`
- `source_kind`
- `adapter`
- `default_scope`
- `authority`
- `status`
- `created_at`
- `updated_at`
- `committed_generation`
- `metadata_json`

`source_items`:

- `source_id`
- `generation`
- `source_item_key`
- `canonical_uri`
- `item_kind`
- `content_kind`
- `content_hash`
- `size_bytes`
- `mtime`
- `status`
- `document_id`
- `metadata_json`

`document_status`:

- `document_id`
- `source_id`
- `generation`
- `source_item_key`
- `status`
- `chunk_count`
- `vector_point_count`
- `updated_at`
- `last_error_json`

`cleanup_debt`:

- `debt_id`
- `job_id`
- `source_id`
- `generation`
- `kind`
- `selector_json`
- `status`
- `attempts`
- `last_error_json`
- `next_retry_at`
- `created_at`
- `completed_at`

`memory_records`:

- `memory_id`
- `memory_type`
- `status`
- `body`
- `body_hash`
- `confidence`
- `salience`
- `scope_kind`
- `scope_value`
- `decay_json`
- `visibility`
- `created_at`
- `updated_at`
- `last_reinforced_at`

`graph_nodes`:

- `node_id`
- `kind`
- `stable_key`
- `label`
- `authority`
- `confidence`
- `properties_json`
- `created_at`
- `updated_at`

## Migration Runner Contract

The migration runner records:

- schema version
- migration id
- checksum
- applied timestamp
- owning crate

It runs all store migrations in dependency order:

1. jobs/config snapshots
2. ledger
3. graph
4. memory
5. watches
6. artifacts/provider health

All migrations run with foreign keys enabled.

## SQLite Introspection Contract

`cargo xtask schemas database --check` opens a fresh temporary SQLite database,
runs migrations, and compares generated metadata against:

```sql
PRAGMA foreign_keys;
PRAGMA table_info(<table>);
PRAGMA foreign_key_list(<table>);
PRAGMA index_list(<table>);
PRAGMA index_info(<index>);
PRAGMA integrity_check;
```

The check fails unless:

- `PRAGMA foreign_keys = 1`
- `PRAGMA integrity_check = ok`
- every generated table exists
- every generated column matches SQLite introspection
- every required index exists
- every migration checksum matches the manifest
- migration order is deterministic

## Drift Checks

Fail when:

- migration creates table absent from schema docs
- schema docs list table absent from migrations
- required index is missing
- foreign keys are not represented
- store trait claims table owned by another crate
- schema metadata marks a secret/public field incorrectly
- migration checksum changes without schema regeneration
- SQLite introspection differs from generated schema

## Validation Fixtures

Required fixtures:

```text
crates/axon-jobs/tests/fixtures/schema/jobs.valid.json
crates/axon-ledger/tests/fixtures/schema/source_items.valid.json
crates/axon-graph/tests/fixtures/schema/graph_nodes.valid.json
crates/axon-memory/tests/fixtures/schema/memory_records.valid.json
crates/axon-jobs/tests/fixtures/schema/missing_primary_key.invalid.json
crates/axon-ledger/tests/fixtures/schema/missing_index.invalid.json
crates/axon-core/tests/fixtures/schema/secret_public_column.invalid.json
```

## Acceptance Criteria

- fresh DB migrates to latest
- repeated migration is no-op
- generated schema matches SQLite introspection
- required indexes exist
- foreign key enforcement is enabled
- reset drops/recreates all owned tables
- every store trait query names a supporting index
- generated schema round-trips through SQLite introspection
