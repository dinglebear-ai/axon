# Runtime Storage

Last Modified: 2026-07-19

Runtime storage is split three ways: SQLite (lifecycle, jobs, ledger, status,
graph, memory metadata, watches, small structured state), Qdrant (vector
retrieval payloads), and filesystem/object store (artifacts, large outputs).
Config files are for bootstrap, not runtime state.

> Contract source:
> [`docs/pipeline-unification/runtime/storage-contract.md`](../../pipeline-unification/runtime/storage-contract.md).
> On-disk layout: `~/.axon/` (flat — see [configuration guide](../../guides/configuration.md)).

## Store registry

| Store | Backend | Owns |
|---|---|---|
| `JobStore` | SQLite | jobs, attempts, stages, events, heartbeats, artifacts refs, config snapshots |
| `LedgerStore` | SQLite | sources, items, manifests, generations, leases, cleanup debt, document_status |
| `GraphStore` | SQLite | nodes, edges, evidence, merge-conflict |
| `MemoryStore` | SQLite + VectorStore | memory records, decay, reinforcement, review |
| `WatchStore` | SQLite | watches, watch runs |
| `VectorStore` | Qdrant | dense + sparse vectors, chunk text, retrieval payloads |
| `ArtifactStore` | filesystem | fetched markdown/html/raw, screenshots, WARC, tool outputs, reports |
| `DocumentCache` | filesystem + SQLite index | prepared-doc cache |
| `ConfigStore` | files | config.toml, .env |

SQLite binds all the store pools together so `jobs.source_id` can FK to
`sources(source_id)`.

## What each tier must NOT hold

- **SQLite must NOT store:** full large page bodies, screenshots, WARC bodies,
  large tool outputs, embedding vectors (when Qdrant available), secrets.
- **Qdrant must NOT be:** the source ledger, the only job-progress store, the
  only cleanup-debt tracker, a secret store, or a large-artifact store.
  Payloads must include enough ids to join back to SQLite.
- **Artifacts** are referenced by id from SQLite, never inlined.

## Local filesystem layout (`~/.axon/`)

```text
~/.axon/
├── .env, config.toml
├── jobs.db (+ -wal/-shm)
├── output/           prepared docs, vectors, manifests
├── logs/axon.log     rotated
├── artifacts/        fetched content, screenshots, WARC, tool outputs
├── screenshots/
├── chrome-diagnostics/
└── tei/              TEI model + cache (compose bind)
```

`AXON_DATA_DIR` defaults to `~/.axon`. Under Incus this is `/mnt/axon-data`
(mapped from `~/.axon-incus` on the host).

## ArtifactStore

Kinds: fetched markdown/html/raw output, structured extraction results,
screenshots, WARC archives, endpoint captures, large CLI/MCP tool outputs,
reset reports, prune dry-run reports, debug bundles.

Metadata: `artifact_id`, `job_id`, `source_id`, `kind`, `relative_path`,
`content_type`, `byte_count`, `content_hash`, `visibility`, `retention_policy`.
ArtifactStore must canonicalize paths, reject traversal, reject symlink root
escape, set safe content type, record hash + byte count, classify visibility,
and enforce retention.

## Cleanup debt (drives cleanup)

All destructive source cleanup flows through reviewed prune plans and cleanup
debt. Job-lifecycle retention is a jobs-runtime op, **not** a source-data
deletion path.

Debt kinds (7): `vector_delete`, `artifact_delete`, `ledger_prune`,
`graph_prune`, `memory_prune`, `job_retention`, `cache_prune`. Debt fields:
`debt_id`, `job_id`, `source_id`, `generation`, `kind`, `selector`, `status`,
`created_at`, `attempts`, `last_error`, `next_retry_at`, `completed_at`.

See [pruning.md](pruning.md) for execution; `axon-ledger` records debt,
`axon-prune` executes it.

## Retention defaults

| Data | Default |
|---|---|
| source generations | last 2 committed + active cleanup debt |
| source item manifests | while source exists |
| vector old generations | until cleanup debt succeeds |
| artifacts | source/job policy (default 30d transient) |
| job events | 14d (failed: 60d) |
| provider health | 7d |
| memory | memory policy (not job retention) |
| graph evidence | while supporting edge/node exists |

## Backup / restore

Minimum backup set = SQLite DB + artifact dir + `config.toml` + `.env`
(separate secret process) + Qdrant collection snapshot (if vectors must be
restorable without reindex). Restore modes: SQLite + artifacts + reindex
vectors; SQLite + artifacts + Qdrant snapshot; config-only fresh boot.

## Rule

Storage paths must be configurable through the normal config model and safe to
inspect with `axon doctor` and `axon reset plan` dry-runs.

If the storage layout changes, update this file and the configuration guide in
the same PR.
