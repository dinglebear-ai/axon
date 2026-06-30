# axon-embedding Crate Contract
Last Modified: 2026-06-30

## Purpose

`axon-embedding` owns embedding provider boundaries, batch formation, provider
capabilities, reservations, throughput limits, and embedding fakes.

## Owns

- `EmbeddingProvider` trait and provider implementations
- embedding request batching and response normalization
- provider capability discovery, dimensions, model identity, and vector names
- throughput reservations, cooling, retries, timeout classification
- fake embedding provider for tests

## Must Not Own

- source acquisition, document chunking, vector store upserts, retrieval ranking,
  job scheduling, or CLI/MCP/REST rendering
- Qdrant point construction
- LLM chat/completion behavior

## Public Modules

```text
lib.rs
provider.rs
batch.rs
capability.rs
reservation.rs
tei.rs
openai_compat.rs
fake.rs
testing.rs
```

## Public API

- `EmbeddingProvider`
- `EmbeddingBatch`
- `EmbeddingInput`
- `EmbeddingOutput`
- `EmbeddingVector`
- `EmbeddingCapability`
- `EmbeddingReservation`
- `EmbeddingProviderHealth`
- `FakeEmbeddingProvider`

## Dependencies Allowed

- `axon-api`, `axon-error`, `axon-core`, `axon-observe`
- HTTP clients for provider implementations

## Dependencies Forbidden

- `axon-vectors`, `axon-retrieval`, `axon-services`, transport crates
- Qdrant clients
- LLM provider clients unless they are also embedding APIs behind this trait

## Generated Artifacts

- provider capability schema in
  [../../schemas/provider-capability-schema.md](../../schemas/provider-capability-schema.md)
- embedding config schema references

## Fixtures And Fakes

- deterministic fake vectors by input id
- provider saturation fixture
- provider outage fixture
- mixed-dimension rejection fixture

## Tests

- batches preserve input order and ids
- provider dimensions match vector store collection requirements
- reservations prevent overload and expose wait/cooling reasons
- fake provider is deterministic across test runs

## Acceptance Criteria

- all embedding throughput knobs converge on this boundary
- callers receive embeddings only; they do not know TEI/OpenAI internals
- provider failure can degrade or retry without corrupting document status

See [../README.md](../README.md) and
[../../runtime/provider-contract.md](../../runtime/provider-contract.md).
