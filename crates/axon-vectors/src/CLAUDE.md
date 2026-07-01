# axon-vectors Agent Instructions

This file is the agent-facing contract for the `axon-vectors` crate docs.

## When Editing

- Keep `VectorStore`, Qdrant implementation, collection specs, point batches,
  payloads, filters, indexes, and vector query primitives here.
- Do not add embedding generation, source acquisition, chunking, retrieval
  synthesis, or transport rendering.
- Update `../../../docs/pipeline-unification/crates/axon-vectors/README.md`, `../../../docs/pipeline-unification/runtime/storage-contract.md`, and
  `../../../docs/pipeline-unification/schemas/vector-payload-schema.md` together.
- Preserve replaceability of Qdrant behind `VectorStore`.

## Review Checklist

- Payloads include required metadata fields.
- Collection specs validate dimensions and vector names.
- Delete filters are source/generation-safe.
