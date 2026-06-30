# axon-memory Crate Contract
Last Modified: 2026-06-30

## Purpose

`axon-memory` owns durable user/agent memory records, memory lifecycle, recall,
decay, reinforcement, review, graph links, and context assembly.

## Owns

- `MemoryStore` and memory service primitives
- memory record lifecycle: remember, search, show, link, supersede, review,
  decay, reinforce, archive, and context
- memory graph links to sources, sessions, repos, issues, artifacts, and tools
- memory vector payload conventions and source ledger integration points
- memory-specific tests, fixtures, and retention rules

## Must Not Own

- general source acquisition, source routing, parser registry, or vector store
  implementation
- RAG answer synthesis outside memory context retrieval
- transport command rendering

## Public Modules

```text
lib.rs
store.rs
sqlite.rs
migration.rs
record.rs
link.rs
decay.rs
review.rs
recall.rs
context.rs
graph.rs
testing.rs
```

## Public API

- `MemoryStore`
- `MemoryRecord`
- `MemoryLink`
- `MemoryDecayPolicy`
- `MemoryReviewPolicy`
- `MemoryRecallRequest`
- `MemoryContext`
- `MemoryLifecycleService`
- `FakeMemoryStore`

## Dependencies Allowed

- `axon-api`, `axon-error`, `axon-core`, `axon-ledger`, `axon-graph`,
  `axon-observe`
- SQLite and migration crates

## Dependencies Forbidden

- direct Qdrant client ownership
- LLM provider implementations
- transport crates

## Generated Artifacts

- memory DTO/schema components
- memory database tables in [../../schemas/database-schema.md](../../schemas/database-schema.md)
- memory payload examples in
  [../../sources/metadata-payload.md](../../sources/metadata-payload.md)

## Fixtures And Fakes

- stable memory record fixture
- superseded memory chain fixture
- decay/reinforcement fixture
- memory context fixture
- fake memory store

## Tests

- decay never deletes without policy approval
- supersession preserves old records and graph links
- recall can combine lexical, vector, and graph filters
- memory context output is bounded and source-cited

## Acceptance Criteria

- memory is a first-class source-like durable domain, not a one-off command
- memory lifecycle is observable and linked into SourceGraph

See [../README.md](../README.md) and
[../../runtime/memory-contract.md](../../runtime/memory-contract.md).
