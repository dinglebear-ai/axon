# axon-embedding

Own embedding provider calls, batching, capabilities, reservations, cooldown, and cache behavior.

## Read before changing

[adding provider](../../../docs/development/adding-provider.md) · [providers](../../../docs/reference/runtime/providers.md) · [provider capabilities](../../../docs/reference/runtime/provider-capabilities.md) · [pipeline performance boundaries](../../../docs/guides/pipeline-performance-boundaries.md)

## Implementation map

[Crate exports](lib.rs) and [manifest](../Cargo.toml); focused entry points:
[provider.rs](provider.rs) · [batch.rs](batch.rs) · [tei.rs](tei.rs) · [openai_compat.rs](openai_compat.rs) · [cache.rs](cache.rs) · [reservation.rs](reservation.rs) · [capability.rs](capability.rs)

## Change requirements

- Preserve input IDs/order and validate model identity, dimensions, response cardinality, and vector validity. Do not let source adapters implement TEI/OpenAI retry policy.

- CachedEmbeddingProvider is real behavior, not a stateless-only contract. Cache storage is injected; the durable implementation is in axon-jobs/embedding_cache_store.rs. Keep cache identity tied to model/input and bound store operations.

- Respect batch/concurrency/in-flight limits and provider reservations. Oversized request splitting, cooldown, and partial failures need bounded retries with useful machine-readable reasons.

- Embedding output is not a Qdrant publication receipt. Keep vector point construction, collection management, and vector persistence outside this crate.

## Verification for code changes

provider_tests, tei_client_tests, reservation_compat_tests, capability_tests, and cache sidecars; include mixed hits/misses, timeout, dimension mismatch, and retry exhaustion.

Use focused `cargo test -p axon-embedding` targets. For contract changes, follow
[generated-contract validation](../../../docs/development/documentation.md);
update the linked references and affected transport consumers together.
