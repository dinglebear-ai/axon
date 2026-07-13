# axon-graph Crate Contract
Last Modified: 2026-06-30

## Purpose

`axon-graph` owns the SourceGraph boundary: nodes, edges, evidence, authority,
merge rules, and graph persistence.

## Owns

- `GraphStore` trait and SQLite implementation
- graph node, edge, evidence, confidence, and authority records
- candidate ingestion from parsers/adapters/resolvers/memory/sessions
- merge, upsert, conflict, and provenance rules
- graph query helpers used by retrieval and apps

## Must Not Own

- parsing source files directly
- source acquisition, vector storage, embedding, job scheduling, or transport
  rendering
- graph facts without evidence

## Public Modules

```text
lib.rs
store.rs
sqlite.rs
migration.rs
node.rs
edge.rs
evidence.rs
candidate.rs
authority.rs
merge.rs
testing.rs
```

## Public API

- `GraphStore`
- `SqliteGraphStore`
- `GraphNode`
- `GraphEdge`
- `GraphEvidence`
- `GraphCandidateIngest`
- `AuthorityLink`
- `GraphMergePolicy`
- `GraphQueryRequest` / `GraphQueryResult` — querying is a `GraphStore::query()`
  trait method, not a standalone `GraphQuery` type or `query.rs` module
- `FakeGraphStore`

## Dependencies Allowed

- `axon-api`, `axon-error`, `axon-core`, `axon-observe`
- SQLite and migration crates

## Dependencies Forbidden

- parser implementations as concrete dependencies
- Qdrant/TEI/LLM/provider clients
- transport crates

## Generated Artifacts

- [../../schemas/graph-schema.md](../../schemas/graph-schema.md)
- graph fixture export for docs and tests

## Fixtures And Fakes

- docs-to-repo-to-package authority fixture
- repo dependency graph fixture
- session tool/skill/agent fixture
- conflicting evidence fixture
- fake graph store

## Tests

- graph candidate ingestion is idempotent
- evidence is required for non-manual edges
- merge policy preserves provenance and confidence
- graph queries can filter by source, node kind, edge kind, and generation

## Acceptance Criteria

- repos, docs, packages, sessions, tools, agents, issues, PRs, and artifacts can
  be linked through one graph model
- no graph edge is accepted without source evidence or explicit authority record

See [../README.md](../README.md) and
[../../sources/source-graph.md](../../sources/source-graph.md).
