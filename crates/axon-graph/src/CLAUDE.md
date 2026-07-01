# axon-graph Agent Instructions

This file is the agent-facing contract for the `axon-graph` crate docs.

## When Editing

- Keep SourceGraph storage, nodes, edges, evidence, authority, merge policy, and
  graph query helpers here.
- Do not parse source files directly; consume graph candidates from parsers,
  adapters, resolver, sessions, and memory.
- Update `../../../docs/pipeline-unification/crates/axon-graph/README.md`, `../../../docs/pipeline-unification/sources/source-graph.md`, and
  `../../../docs/pipeline-unification/schemas/graph-schema.md` together.
- Require evidence or explicit authority for every edge.

## Review Checklist

- Node/edge kinds remain aligned with the graph contract.
- Upserts are idempotent and preserve provenance.
- Conflict handling is explicit.
