# axon-ledger Crate Contract
Last Modified: 2026-06-30

## Purpose

`axon-ledger` owns durable source accounting: manifests, diffs, generations,
items, jobs linkage, document status, leases, and cleanup debt.

## Owns

- `LedgerStore` boundary and SQLite implementation
- source records, source items, manifest snapshots, and manifest diffs
- generation creation, commit, publish readiness, and failed generation state
- document status rows for prepared/embedded/published/cleaned states
- leases, cleanup debt, and reset-on-empty-DB behavior

## Must Not Own

- source acquisition, parsing, chunking, embedding, vector store writes, graph
  parsing, or transport output
- cleanup execution logic beyond recording debt and transactions
- provider rate limiting

## Public Modules

```text
lib.rs
store.rs
sqlite.rs
migration.rs
source.rs
item.rs
manifest.rs
diff.rs
generation.rs
document_status.rs
lease.rs
cleanup_debt.rs
transaction.rs
testing.rs
```

## Public API

`axon-ledger` implements the `LedgerStore` trait and its SQLite backend; the
DTOs it reads/writes are `axon-api` types, not ledger-owned re-declarations
(repo-wide DTO-ownership rule — every wire/domain shape lives in `axon-api`,
per `dto-contract.md`). This crate's own module files stay marker/impl-only
for the shapes below:

- `LedgerStore` (trait), `SqliteLedgerStore` (impl) — owned here
- `axon_api::source::SourceSummary` (not `SourceRecord`)
- `axon_api::source::SourceItemStatus` (not `SourceItemRecord`)
- `axon_api::source::SourceManifest`
- `axon_api::source::SourceManifestDiff`
- `axon_api::source::SourceGeneration`
- `axon_api::source::DocumentStatus`
- `axon_api::source::LeaseGuard` (not `LedgerLease`)
- `axon_api::source::CleanupDebt`

There is no `LedgerTransaction` DTO — commit/rollback across the manifest,
generation, and cleanup-debt tables is internal SQLite transaction handling
in `transaction.rs`, not a wire-visible type.

## Dependencies Allowed

- `axon-api`, `axon-error`, `axon-core`, `axon-observe`
- SQLite and migration crates

## Dependencies Forbidden

- Qdrant/TEI/LLM/provider clients
- source adapter concrete implementations
- transport crates
- service-layer orchestration cycles

## Generated Artifacts

- ledger database schema in [../../schemas/database-schema.md](../../schemas/database-schema.md)
- manifest/diff DTO schema components

## Fixtures And Fakes

- in-memory/fake ledger store
- SQLite temp-db fixture
- generation commit/rollback fixture
- cleanup debt fixture
- lease conflict fixture

## Tests

- manifest diffs classify added, changed, removed, and unchanged items
- generation commit is transactional
- failed generations are not visible as published source state
- cleanup debt is durable and idempotent
- leases expire and can be safely reclaimed

## Acceptance Criteria

- mutable and refreshable sources are ledger-owned
- vector cleanup is driven from cleanup debt, not ad hoc scroll queries
- implementation assumes empty DB for this clean break and does not carry legacy
  migration baggage

See [../README.md](../README.md) and
[../../runtime/ledger-contract.md](../../runtime/ledger-contract.md).
