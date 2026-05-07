# crates/vector
Last Modified: 2026-03-03

Embedding, vector storage, retrieval, and RAG operations.

## Purpose
- Convert content into embeddings (TEI).
- Store and query vectors in Qdrant.
- Implement retrieval and answer generation flows (`query`, `retrieve`, `ask`, `evaluate`, `suggest`).

## Responsibilities
- Input chunking and embedding batch execution.
- Qdrant collection management and point upsert/search.
- Retrieval ranking, source display shaping, and answer context assembly.
- RAG evaluation and suggestion workflows.

## Key Files
- `ops.rs`: vector operations module root.
- `ops/input.rs`: text/input handling for embed flows.
- `ops/tei.rs` + `ops/tei/tei_manifest.rs`: embedding client and manifest behavior.
- `ops/qdrant.rs` + `ops/qdrant/*`: Qdrant client operations and payload types.
- `ops/ranking.rs` + `ops/ranking/snippet.rs`: ranking/summarization helpers.
- `ops/commands/query.rs`: semantic query command path.
- `ops/commands/ask.rs` + `ops/commands/ask/context.rs`: RAG answer flow and context assembly.
- `ops/commands/evaluate.rs`: answer-vs-baseline evaluation logic.
- `ops/commands/suggest.rs`: complementary source suggestion flow.

## Integration Points
- `crates/cli/commands/*` invoke these operations for interactive and batch usage.
- `crates/jobs/embed` and ingest workflows rely on vector upsert paths.
- Depends on `TEI_URL` and `QDRANT_URL` runtime config from `crates/core/config`.

## Notes
- Retry/splitting behavior for TEI overload and payload limits is handled in embedding paths and should remain conservative for stability.
- Keep retrieval/ranking logic deterministic where possible to reduce answer drift.

## Related Docs
- [Repository README](../../README.md)
- [Architecture](../../docs/ARCHITECTURE.md)
- [Docs Index](../../docs/README.md)
