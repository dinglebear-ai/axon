# axon-retrieval Agent Instructions

This file is the agent-facing contract for the `axon-retrieval` crate docs.

## When Editing

- Keep retrieval planning, filters, hybrid ranking, context bundles, citations,
  graph/memory joins, and result explanation here.
- Do not implement LLM final synthesis, vector store internals, embedding
  providers, or transport rendering.
- Update `../../../docs/pipeline-unification/crates/axon-retrieval/README.md`, `../../../docs/pipeline-unification/foundation/boundary-map.md`, and retrieval DTO
  schema docs together.
- Keep `ask`, `query`, `search`, and `retrieve` semantics aligned with surface
  contracts.

## Review Checklist

- Generation/source filters are preserved.
- Citations map to stored chunks and source metadata.
- Ranking is deterministic under fakes.
