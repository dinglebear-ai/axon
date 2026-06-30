# Storage Contract
Last Modified: 2026-06-30

## Contract

Axon storage is divided by ownership. SQLite is for lifecycle, jobs, ledger,
status, graph, memory metadata, watches, and small structured state. Qdrant is
for vector retrieval payloads. The filesystem/object store is for artifacts and
large outputs. Config files are for bootstrap and tuning, not runtime state.

## Design Rules

- Every durable datum has one owning store.
- VectorStore is not the SourceLedger.
- SourceLedger is not the GraphStore.
- ArtifactStore is not a dumping ground for hidden state.
- Cleanup is debt-driven and idempotent.
- Large raw content does not live in SQLite rows or Qdrant payloads.
- Stored secrets are avoided; referenced credentials stay in CredentialProvider.
- Every store has backup/restore and retention expectations.

## Store Registry

| Store | Owns | Default Implementation |
|---|---|---|
| `JobStore` | jobs, attempts, events, heartbeats, config snapshots | SQLite |
| `LedgerStore` | sources, items, manifests, generations, leases, cleanup debt | SQLite |
| document status rows | document/chunk lifecycle and publish state owned by `LedgerStore` | SQLite |
| `GraphStore` | nodes, edges, evidence, merge/conflict state | SQLite |
| `MemoryStore` | memory metadata, decay/reinforcement/review | SQLite + VectorStore |
| `WatchStore` | watch configs, schedules, runs | SQLite |
| `VectorStore` | vector points, searchable payloads | Qdrant |
| `ArtifactStore` | markdown/html/json/screenshots/WARC/tool outputs | filesystem |
| `DocumentCache` | bounded fetched/prepared content cache | filesystem/SQLite index |
| `ConfigStore` | config.toml and env path discovery | files |

## SQLite Ownership

SQLite owns small structured state:

- source rows
- source item manifests
- generation state
- cleanup debt
- job rows/events/heartbeats
- watch schedules/runs
- graph nodes/edges/evidence
- memory metadata and relationships
- artifact metadata index
- provider health snapshots
- config snapshots

SQLite must not store:

- full large page bodies
- screenshots
- WARC bodies
- large tool outputs
- embedding vectors when Qdrant is available
- secrets

## Qdrant Ownership

Qdrant owns:

- dense vectors
- sparse vectors when enabled
- searchable chunk text
- public/redacted retrieval payload fields
- filterable metadata needed for retrieval

Qdrant must not be used as:

- the source ledger
- the only job progress store
- the only cleanup debt tracker
- a secret store
- a large artifact store

Payloads must include enough ids to join back to SQLite stores without heavy
facets as the normal path.

## ArtifactStore Ownership

Artifacts include:

- fetched markdown/html/raw output
- structured extraction results
- screenshots
- WARC archives
- endpoint captures
- large CLI/MCP tool outputs
- reset reports
- prune dry-run reports
- debug bundles

Artifact metadata includes:

| Field | Meaning |
|---|---|
| `artifact_id` | Stable id. |
| `job_id` | Producing job. |
| `source_id` | Source if applicable. |
| `kind` | markdown, screenshot, warc, report, etc. |
| `relative_path` | Store-relative path. |
| `content_type` | MIME type. |
| `byte_count` | Size. |
| `content_hash` | Hash. |
| `visibility` | public/internal/sensitive. |
| `retention_policy` | Cleanup behavior. |

## Cleanup Debt

Target state: all destructive cleanup flows through cleanup debt.

Current implementation: cleanup is still split across direct Qdrant purge,
dedupe, job cleanup/clear/recover, and code-index generation cleanup debt. The
clean-break implementation should migrate those direct paths into this common
debt model.

Debt kinds:

| Kind | Target |
|---|---|
| `vector_delete` | Vector points by selector. |
| `artifact_delete` | Artifact ids/paths. |
| `ledger_prune` | Old source items/generations. |
| `graph_prune` | Orphaned graph evidence/nodes. |
| `memory_prune` | Forgotten/expired memory metadata and vectors. |
| `job_retention` | Old terminal jobs/events. |
| `cache_prune` | Expired cache entries. |

Cleanup debt fields:

- `debt_id`
- `job_id`
- `source_id`
- `generation`
- `kind`
- `selector`
- `status`
- `created_at`
- `attempts`
- `last_error`
- `next_retry_at`
- `completed_at`

## Backup and Restore

Minimum backup set:

- SQLite database
- artifact directory
- config.toml
- `.env` separately through secret backup process
- Qdrant collection snapshot when vectors should be restorable without reindex

Restore must support:

- SQLite + artifacts + reindex vectors
- SQLite + artifacts + Qdrant snapshot
- config-only fresh boot

## Retention

Retention defaults:

| Data | Default |
|---|---|
| source generations | last 2 committed plus active cleanup debt |
| source item manifests | while source exists |
| vector old generations | until cleanup debt succeeds |
| artifacts | source/job policy, default 30 days for transient |
| job events | 14 days, failed 60 days |
| provider health | 7 days |
| memory | memory policy, not job retention |
| graph evidence | while supporting edge/node exists |

## Testing Requirements

Storage tests must prove:

- each store rejects data it does not own
- cleanup debt is idempotent
- vector delete selectors are generation-fenced
- artifact traversal is impossible
- SQLite restore preserves source/job/graph joins
- Qdrant payloads join to ledger/status rows
- retention cleanup does not orphan required evidence
