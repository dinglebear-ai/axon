# Adding a Provider (Embedding or Vector Store)

"Provider" covers two distinct trait boundaries that are easy to conflate:
**embedding providers** (`axon-embedding`, turn text into vectors) and
**vector stores** (`axon-vectors`, persist/search vectors). This guide covers
both — pick the section for the boundary you're extending.

See also: crate guides `crates/axon-embedding/src/CLAUDE.md` and
`crates/axon-vectors/src/CLAUDE.md`; behavior contracts
`docs/pipeline-unification/runtime/provider-contract.md` and
`docs/pipeline-unification/runtime/storage-contract.md`.

**Rule of thumb:** embedding providers generate vectors and never persist
them; vector stores persist/search vectors and never generate them. Neither
crate may depend on the other's concrete implementation — `axon-vectors` may
depend on `axon-embedding` **types** only (enforced by
`cargo xtask check-layering`).

## Adding an embedding provider

`crates/axon-embedding/src/provider.rs` defines the trait:

```rust
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, batch: EmbeddingBatch) -> Result<EmbeddingResult>;
    async fn capabilities(&self) -> Result<ProviderCapability>;
}
```

Real implementations to read before writing a new one:
`crates/axon-embedding/src/tei.rs` (TEI — the real reqwest-backed `/embed`
client, with 413 batch-splitting and 429/5xx retry with backoff) and
`crates/axon-embedding/src/openai_compat.rs` (OpenAI-compatible embeddings
endpoint).

**Steps:**

1. Implement `EmbeddingProvider` in a new module (e.g.
   `crates/axon-embedding/src/your_provider.rs`). Use `crates/axon-embedding/
   src/batch.rs`'s `validate_batch` to validate the incoming
   `EmbeddingBatch` before dispatching.
2. **Preserve input order and ids.** `EmbeddingBatch`/`EmbeddingResult` are
   order-preserving — callers correlate outputs to inputs positionally; do
   not reorder or drop items silently.
3. Report explicit, accurate dimensions and model identity in
   `capabilities()` (`ProviderCapability`) — this is what the vector-store
   collection sizing and payload stamping (`embedding_model`,
   `embedding_dimensions`) depend on. See `TeiEmbeddingProvider::
   derive_embedding_identity` in `tei.rs` for the pattern of probing the live
   provider rather than hardcoding a constant.
4. Route provider failures through retry/degradation, not silent corruption
   of document status — TEI's retry/backoff/413-split behavior in `tei.rs`
   is the reference implementation.
5. Use `crates/axon-embedding/src/reservation.rs`'s throughput reservation
   pattern if your provider needs overload/cooling protection; all embedding
   throughput knobs should converge on this boundary so callers never need
   to know provider-specific internals.
6. Add tests as a sidecar `_tests.rs` file. `crates/axon-embedding/src/
   testing.rs` exposes `FakeEmbeddingProvider` plus saturation/outage/mixed-
   dimension fixtures for downstream consumers.

**Boundary:** no source acquisition, document chunking, vector-store
upserts, retrieval ranking, job scheduling, transport rendering, or Qdrant
point construction in this crate. Allowed dependencies: `axon-api`,
`axon-error`, `axon-core`, `axon-observe`, HTTP clients for provider
implementations. Forbidden: `axon-vectors`, `axon-retrieval`,
`axon-services`, transport crates, Qdrant clients, LLM provider clients
(unless the same API is also an embedding API behind this trait).

## Adding a vector store

Vector stores persist and search embeddings; they are a distinct boundary
from embedding providers (`axon-vectors`, not `axon-embedding`) with their
own generation-aware invariants. See
[`adding-vector-store.md`](adding-vector-store.md) for the full guide.

## Testing

```bash
cargo test -p axon-embedding
cargo test -p axon-vectors
```
