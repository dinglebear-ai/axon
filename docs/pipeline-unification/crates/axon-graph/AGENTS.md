# axon-graph design-contract maintenance

This directory documents the `axon-graph` boundary: Persist and query evidence-backed SourceGraph nodes, edges, authority, merges, and conflicts.
Rust implementations belong in the crate, not in this documentation directory.

## Read together

[Design contract](README.md) · [Current implementation guide](../../../../crates/axon-graph/src/AGENTS.md) · [Crate exports](../../../../crates/axon-graph/src/lib.rs)

[source graph](../../../../docs/reference/sources/source-graph.md) · [graph](../../../../docs/reference/sources/graph.md) · [database schema](../../../../docs/reference/runtime/database-schema.md)

## Review the actual boundary

- Consume GraphCandidate facts; source parsing and acquisition stay outside graph persistence. Do not invent relationships without evidence or explicit authority.

- Append migrations and test existing-state upgrades; graph cleanup must not destroy current-generation evidence when older generations expire.

For implementation evidence, inspect [store.rs](../../../../crates/axon-graph/src/store.rs), [sqlite.rs](../../../../crates/axon-graph/src/sqlite.rs), [candidate.rs](../../../../crates/axon-graph/src/candidate.rs).
Check the manifest and actual callers before describing a dependency or API as
shipped. Distinguish current behavior, intended constraints, and remaining work;
historical phase/cutover prose is not authority to restore removed runtime paths
or to assume that existing databases are empty. Preserve dated outcomes.

## Verification and paired edits

store_tests, fixture_tests, schema_fixture_tests and SQLite/merge sidecars; exercise conflicting evidence, duplicate candidates, source filtering, and generation cleanup.

When shapes or behavior change, update this contract and its linked live guide.
Regenerate schema projections from owning inputs rather than hand-editing them;
see [documentation validation](../../../development/documentation.md).
Documentation-only edits need link/structural checks, not provider deployment.
