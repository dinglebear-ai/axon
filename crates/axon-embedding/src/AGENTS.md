# axon-embedding — Agent Guide

`axon-embedding` owns the **embedding provider boundary**: the
`EmbeddingProvider` trait, batch formation, provider capabilities, throughput
reservations/cooling, and provider clients (TEI, OpenAI-compatible). It returns
embeddings only — it never persists them. Full contract (owns / API / deps /
tests):
[../../../docs/pipeline-unification/crates/axon-embedding/README.md](../../../docs/pipeline-unification/crates/axon-embedding/README.md)
· behavior spec:
[../../../docs/pipeline-unification/runtime/provider-contract.md](../../../docs/pipeline-unification/runtime/provider-contract.md).

## Status — live crate, Phase 7 landed
`TeiEmbeddingProvider` (real TEI HTTP client + identity derivation) and
`FakeEmbeddingProvider` are real and tested, not markers. Do not add
vector-store writes, Qdrant point construction, or LLM chat behavior here.

## Module map
| File | Owns |
|---|---|
| `provider.rs` | `EmbeddingProvider` trait — the durable boundary all callers use |
| `batch.rs` | `EmbeddingBatch`, `EmbeddingInput`/`EmbeddingOutput`/`EmbeddingVector`; order-preserving batch formation + response normalization |
| `capability.rs` | `EmbeddingCapability` — dimensions, model identity, vector names |
| `reservation.rs` | `EmbeddingReservation` — throughput reservations, cooling, retries, timeout classification |
| `tei.rs` | TEI provider client |
| `openai_compat.rs` | OpenAI-compatible provider client |
| `fake.rs` / `testing.rs` | `FakeEmbeddingProvider` + saturation/outage/mixed-dimension fixtures |

## Boundary — keep OUT of this crate
- Source acquisition, document chunking, vector-store upserts, retrieval ranking, job scheduling, CLI/MCP/REST rendering.
- Qdrant point construction.
- LLM chat/completion behavior.

## Dependencies
- **Allowed:** `axon-api`, `axon-error`, `axon-core`, `axon-observe`, HTTP clients for provider impls.
- **Forbidden:** `axon-vectors`, `axon-retrieval`, `axon-services`, transport crates, Qdrant clients, LLM provider clients (unless the API is also an embedding API behind this trait). Enforced by `cargo xtask check-layering`.

## Invariants (review checklist)
- Batches **preserve input order and ids**.
- Provider dimensions/model identity are **explicit** and match vector-store collection requirements.
- Reservations **prevent overload** and expose wait/cooling reasons.
- Provider failure can **degrade or retry** without corrupting document status.
- All embedding throughput knobs **converge on this boundary**; callers never know TEI/OpenAI internals.

## DTO ownership
Serializable wire shapes (`EmbeddingCapability`, `EmbeddingProviderHealth`, batch
DTOs) are defined in **`axon-api`**; this crate produces and returns them — it
does not redefine transport-facing shapes.

## Keep in sync when shapes change
`README.md` (crate contract) · `runtime/provider-contract.md` ·
`schemas/provider-capability-schema.md` · the embedding capability/health DTO
components in `axon-api`.
