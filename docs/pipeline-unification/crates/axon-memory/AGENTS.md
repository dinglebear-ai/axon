# axon-memory design-contract maintenance

This directory documents the `axon-memory` boundary: Own authoritative memory lifecycle, recall, decay/review/reinforcement, supersession, and bounded context.
Rust implementations belong in the crate, not in this documentation directory.

## Read together

[Design contract](README.md) · [Current implementation guide](../../../../crates/axon-memory/src/AGENTS.md) · [Crate exports](../../../../crates/axon-memory/src/lib.rs)

[memory](../../../../docs/reference/runtime/memory.md) · [context injection](../../../../docs/guides/context-injection.md) · [source graph](../../../../docs/reference/sources/source-graph.md)

## Review the actual boundary

- SQLite memory records are authoritative; vector points are derived recall indexes. Export correctness must not depend on Qdrant page size or availability.

- Keep lexical/vector/graph recall constraints, visibility, ordering, and source citations intact. Distinguish saved metadata from successfully indexed publication.

For implementation evidence, inspect [store.rs](../../../../crates/axon-memory/src/store.rs), [sqlite.rs](../../../../crates/axon-memory/src/sqlite.rs), [record.rs](../../../../crates/axon-memory/src/record.rs).
Check the manifest and actual callers before describing a dependency or API as
shipped. Distinguish current behavior, intended constraints, and remaining work;
historical phase/cutover prose is not authority to restore removed runtime paths
or to assume that existing databases are empty. Preserve dated outcomes.

## Verification and paired edits

store_tests, shared_pipeline_tests, fixtures_tests and lifecycle/recall sidecars; test export without vectors, enqueue failure recovery, supersession, and bounded context.

When shapes or behavior change, update this contract and its linked live guide.
Regenerate schema projections from owning inputs rather than hand-editing them;
see [documentation validation](../../../development/documentation.md).
Documentation-only edits need link/structural checks, not provider deployment.
