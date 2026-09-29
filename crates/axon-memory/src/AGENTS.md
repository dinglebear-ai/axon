# axon-memory

Own authoritative memory lifecycle, recall, decay/review/reinforcement, supersession, and bounded context.

## Read before changing

[memory](../../../docs/reference/runtime/memory.md) · [context injection](../../../docs/guides/context-injection.md) · [source graph](../../../docs/reference/sources/source-graph.md)

## Implementation map

[Crate exports](lib.rs) and [manifest](../Cargo.toml); focused entry points:
[store.rs](store.rs) · [sqlite.rs](sqlite.rs) · [record.rs](record.rs) · [recall.rs](recall.rs) · [context.rs](context.rs) · [decay.rs](decay.rs) · [review.rs](review.rs) · [vector.rs](vector.rs) · [graph.rs](graph.rs)

## Change requirements

- SQLite memory records are authoritative; vector points are derived recall indexes. Export correctness must not depend on Qdrant page size or availability.

- Preserve history and links during supersession, contradictions, review, and decay. Do not turn decay into unapproved deletion or expose uncited/unbounded context.

- The memory:// projection uses the shared source pipeline via adapters/services. Lifecycle changes must not directly publish generation-0 vectors or production graph copies. Preserve durable source-sync recovery and pending-publication diagnostics.

- Keep lexical/vector/graph recall constraints, visibility, ordering, and source citations intact. Distinguish saved metadata from successfully indexed publication.

## Verification for code changes

store_tests, shared_pipeline_tests, fixtures_tests and lifecycle/recall sidecars; test export without vectors, enqueue failure recovery, supersession, and bounded context.

Use focused `cargo test -p axon-memory` targets. For contract changes, follow
[generated-contract validation](../../../docs/development/documentation.md);
update the linked references and affected transport consumers together.
