# axon-ledger — Agent Guide

`axon-ledger` is the SQLite-backed **system of record for source accounting**:
source records, items, manifests + manifest diffs, generations, document status,
leases, and cleanup debt. It answers "what sources exist, what is in each
generation, and what is safe to search." Full contract (owns / API / deps / tests):
[../../../docs/pipeline-unification/crates/axon-ledger/README.md](../../../docs/pipeline-unification/crates/axon-ledger/README.md)
· behavior spec:
[../../../docs/pipeline-unification/runtime/ledger-contract.md](../../../docs/pipeline-unification/runtime/ledger-contract.md).

## Status — live crate, Phase 6 landed
`LedgerStore` (trait) and `SqliteLedgerStore` (`sqlite.rs`) are real and tested:
source upsert, generation create/commit/publish with failed-generation state,
manifest diffing, document status tracking, leases, and cleanup debt recording.
Per the DTO ownership rule, `SourceRecord`/`SourceManifest`/`SourceGeneration`/
`DocumentStatus`/`CleanupDebt`/etc. live in `axon-api`, not here — `source.rs`,
`item.rs`, `manifest.rs`, `diff.rs`, `generation.rs`, `document_status.rs`,
`lease.rs`, `cleanup_debt.rs`, and `transaction.rs` remain marker files for that
reason, not because the functionality is unimplemented. Do not add
acquisition/embedding/vector behavior here.

## Module map
| File | Owns |
|---|---|
| `store.rs` | `LedgerStore` trait — the durable boundary all callers use |
| `sqlite.rs` | `SqliteLedgerStore` — the only concrete implementation |
| `source.rs` / `item.rs` | `SourceRecord`, `SourceItemRecord` |
| `manifest.rs` / `diff.rs` | `SourceManifest`, `SourceManifestDiff` (added/changed/removed/unchanged) |
| `generation.rs` | `SourceGeneration` — create → commit → publish, and failed-generation state |
| `document_status.rs` | `DocumentStatus` (prepared/embedded/published/cleaned) |
| `lease.rs` | `LedgerLease` — refresh/watch leases |
| `cleanup_debt.rs` | `CleanupDebt` — **recorded here, executed by `axon-prune`** |
| `transaction.rs` | `LedgerTransaction` — the atomic commit unit |
| `migration.rs` | forward-only SQLite schema (no legacy migration baggage) |
| `testing.rs` | in-memory fake store + SQLite temp-db fixtures |

## Boundary — keep OUT of this crate
- Source acquisition, parsing, chunking, embedding, vector writes, graph parsing, transport output.
- Cleanup **execution** — this crate records `CleanupDebt` and owns the transaction; `axon-prune` runs it.
- Provider rate limiting / cooling.

## Dependencies
- **Allowed:** `axon-api`, `axon-error`, `axon-core`, `axon-observe`, SQLite + migration crates.
- **Forbidden:** Qdrant/TEI/LLM/provider clients, concrete source adapters, transport crates (`axon-cli`/`axon-mcp`/`axon-web`), service-layer cycles. Enforced by `cargo xtask check-layering`.

## Invariants (review checklist)
- Generation **commit/publish is transactional** — a partially-written generation is never visible to search.
- **Failed generations never become searchable state.**
- Manifest diffs deterministically classify **added / changed / removed / unchanged**.
- `CleanupDebt` is **durable and idempotent** — re-running a cleanup is safe.
- Leases **expire and can be safely reclaimed** — no permanent locks.
- **Empty-DB clean break** — assume a fresh schema; vector cleanup is driven from cleanup debt, never ad-hoc Qdrant scroll queries.

## DTO ownership
Wire DTOs (`SourceManifest`, `SourceManifestDiff`, `SourceGeneration`,
`DocumentStatus`, `CleanupDebt`, …) are defined in **`axon-api`**; this crate
stores and returns them — it does not redefine transport-facing shapes.

## Keep in sync when shapes change
`README.md` (crate contract) · `runtime/ledger-contract.md` ·
`schemas/database-schema.md` (ledger tables) · the ledger DTO components in `axon-api`.
