# axon-embedding Agent Instructions

This file is the agent-facing contract for the `axon-embedding` crate docs.

## When Editing

- Keep `EmbeddingProvider`, embedding batches, capabilities, reservations,
  provider health, and embedding fakes here.
- Do not add vector store writes or Qdrant point construction.
- Update `README.md`, `../../runtime/provider-contract.md`, and
  `../../schemas/provider-capability-schema.md` together.
- Treat throughput, cooling, timeout, and saturation behavior as part of the
  provider contract.

## Review Checklist

- Batches preserve input ids and ordering.
- Dimensions/model identity are explicit.
- Fakes are deterministic.
