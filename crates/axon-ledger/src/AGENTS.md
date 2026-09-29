# axon-ledger

Own durable source records, manifest diffs, generations, document status, leases, retention, and cleanup debt.

## Read before changing

[ledger](../../../docs/reference/runtime/ledger.md) · [pruning](../../../docs/reference/runtime/pruning.md) · [database schema](../../../docs/reference/runtime/database-schema.md) · [vector payload](../../../docs/reference/sources/vector-payload.md)

## Implementation map

[Crate exports](lib.rs) and [manifest](../Cargo.toml); focused entry points:
[store.rs](store.rs) · [sqlite.rs](sqlite.rs) · [sqlite/](sqlite/) · [listing.rs](listing.rs) · [validation.rs](validation.rs) · [cleanup_debt.rs](cleanup_debt.rs) · [migration.rs](migration.rs)

## Change requirements

- Production databases already exist. Remove fresh-database assumptions: append migrations and test upgrades, restart/recovery, and existing generations.

- Keep publication transactional and generation-fenced. Failed or partially prepared generations must not replace committed searchable state.

- Diff deterministic item identities/hashes into added/modified/removed/unchanged; absence in an incomplete source snapshot must not become deletion.

- Preserve retention while unresolved cleanup debt references older generations. Leases expire/reclaim safely; debt remains durable and idempotent until actual cleanup is reconciled.

- Source accounting does not acquire, embed, write Qdrant points, or execute cleanup itself. Report source/generation and conflict context on rejected transitions.

## Verification for code changes

store_tests, sqlite_tests and generation/lease/debt sidecars, including stale leases, replay, failed publication, partial snapshots, and existing-schema upgrades.

Use focused `cargo test -p axon-ledger` targets. For contract changes, follow
[generated-contract validation](../../../docs/development/documentation.md);
update the linked references and affected transport consumers together.
