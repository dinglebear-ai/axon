# axon-graph — Agent Guide

`axon-graph` owns the **SourceGraph boundary**: nodes, edges, evidence,
confidence, authority, merge rules, and graph persistence. It ingests
evidence-backed `GraphCandidate` values produced by parsers, adapters, the
resolver, sessions, and memory, and links repos, docs, packages, sessions, tools,
agents, issues, PRs, and artifacts through one graph model. Full contract
(owns / API / deps / tests):
[../../../docs/pipeline-unification/crates/axon-graph/README.md](../../../docs/pipeline-unification/crates/axon-graph/README.md)
· behavior spec:
[../../../docs/pipeline-unification/sources/source-graph.md](../../../docs/pipeline-unification/sources/source-graph.md).

## Status — PR0 skeleton
Modules below are **markers only**. Real implementation lands in **Phase 7**
(the `axon-graph` boundary and SQLite implementation, after `axon-parse`). Do not
add source fetching, embedding, vector retrieval, or ledger lifecycle here.

## Module map
| File | Owns |
|---|---|
| `store.rs` | `GraphStore` trait — the durable boundary all callers use |
| `sqlite.rs` | `SqliteGraphStore` — the only concrete implementation |
| `migration.rs` | forward-only SQLite graph schema |
| `node.rs` / `edge.rs` | `GraphNode`, `GraphEdge` records + node/edge kinds |
| `evidence.rs` | `GraphEvidence` — required provenance for non-manual edges |
| `candidate.rs` | `GraphCandidateIngest` — idempotent candidate ingestion |
| `authority.rs` | `AuthorityLink` — explicit authority records (docs→repo→package) |
| `merge.rs` | `GraphMergePolicy` — merge/upsert/conflict/provenance rules |
| `query.rs` | `GraphQuery` — retrieval/app query helpers |
| `testing.rs` | `FakeGraphStore` + graph fixtures (authority, dep graph, session, conflict) |

## Boundary — keep OUT of this crate
- Parsing source files directly — consume `GraphCandidate` from `axon-parse`, adapters, resolver, sessions, memory.
- Source acquisition, vector storage/retrieval, embedding, job scheduling, transport rendering.
- Source ledger lifecycle; graph facts without evidence.

## Dependencies
- **Allowed:** `axon-api`, `axon-error`, `axon-core`, `axon-observe`, `axon-parse` types, `axon-ledger` types, SQLite + migration crates.
- **Forbidden:** parser implementations as concrete deps, Qdrant/TEI/LLM/provider clients, transport crates. Enforced by `cargo xtask check-layering`.

## Invariants (review checklist)
- Graph candidate ingestion is **idempotent** — re-ingesting the same candidate is safe.
- **Evidence is required for every non-manual edge** (or an explicit authority record).
- Merge policy **preserves provenance and confidence**; conflict handling is explicit.
- Node/edge kinds stay **aligned with the graph contract**.
- Graph queries can filter by **source, node kind, edge kind, and generation**.

## DTO ownership
Wire DTOs (`GraphNode`, `GraphEdge`, `GraphEvidence`, `GraphCandidateIngest`,
`AuthorityLink`, `GraphMergePolicy`, `GraphQuery`, …) are defined in
**`axon-api`**; this crate stores and returns them — it does not redefine
transport-facing shapes.

## Keep in sync when shapes change
`README.md` (crate contract) · `sources/source-graph.md` ·
`schemas/graph-schema.md` (node/edge/evidence schema) · the graph DTO components
in `axon-api`.
