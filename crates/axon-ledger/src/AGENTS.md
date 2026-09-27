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
`DocumentStatus`/`CleanupDebt`/etc. live in `axon-api`, not here. The obsolete
marker modules for those DTO names have been removed; live ledger behavior is
organized by backend operation under `sqlite/` and `store/`. Do not add
acquisition/embedding/vector behavior here.

## Module map
| File | Owns |
|---|---|
| `store.rs` | `LedgerStore` trait — the durable boundary all callers use |
| `sqlite.rs` | `SqliteLedgerStore` — the only concrete implementation |
| `sqlite/source.rs` | durable source records and source detail projection |
| `sqlite/manifest.rs` | normalized manifests and deterministic manifest diffs |
| `sqlite/generation.rs` | create → commit → publish and failed-generation state |
| `sqlite/document.rs` | prepared/embedded/published/cleaned document status |
| `sqlite/lease.rs` | refresh/watch lease persistence |
| `cleanup_debt.rs` | `CleanupDebt` — **recorded here, executed by `axon-prune`** |
| `migration.rs` | forward-only SQLite schema (no legacy migration baggage) |
| `store/fake.rs` | in-memory fake used by focused contract tests |

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
