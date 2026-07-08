# Adding a Vector Store Backend

`axon-vectors` owns vector storage: the `VectorStore` trait, the Qdrant
implementation, point-batch construction, collection/index management, and
payload writes/search/delete. It stores vectors; it never generates them —
see [`adding-provider.md`](adding-provider.md) for the embedding-provider
side of that boundary.

See also: crate guide `crates/axon-vectors/src/CLAUDE.md`, behavior contract
`docs/pipeline-unification/runtime/storage-contract.md`, ledger reference
[`docs/reference/runtime/ledger.md`](../reference/runtime/ledger.md) (vector
stores implement the generation-commit side of the ledger's publish model).

## The trait

`crates/axon-vectors/src/store.rs`:

```rust
#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn ensure_collection(&self, spec: CollectionSpec) -> Result<()>;
    async fn upsert(&self, batch: VectorPointBatch) -> Result<VectorStoreWriteResult>;
    async fn mark_generation_committed(
        &self,
        collection: String,
        source_id: SourceId,
        generation: SourceGenerationId,
    ) -> Result<VectorStoreWriteResult>;
    async fn mark_unchanged_items_committed(
        &self,
        collection: String,
        source_id: SourceId,
        previous_generation: SourceGenerationId,
        committed_generation: SourceGenerationId,
        source_item_keys: Vec<SourceItemKey>,
    ) -> Result<VectorStoreWriteResult>;
    async fn delete(&self, selector: VectorDeleteSelector) -> Result<VectorStoreDeleteResult>;
    async fn search(&self, request: VectorSearchRequest) -> Result<VectorSearchResult>;
    async fn capabilities(&self) -> Result<ProviderCapability>;
}
```

The reference implementation is `crates/axon-vectors/src/qdrant.rs` plus
`crates/axon-vectors/src/qdrant/{http,convert,store_impl,search,commit}.rs`
— a live, non-stub `VectorStore` over the Qdrant REST API (via `reqwest`).
Read it end to end before writing a new backend; every invariant below is
demonstrated there in practice.

## Generation-aware semantics are the hard part

Unlike a plain upsert/search store, `VectorStore` is generation-aware because
it backs the source ledger's publish model:

- **`mark_generation_committed`** flips a generation's points to visible in
  place once the ledger commits that generation — this is the moment new
  content becomes searchable.
- **`mark_unchanged_items_committed`** carries forward points for source
  items that didn't change between generations, **without mutating the
  previous committed generation's points**. Old committed searches must
  remain valid until the new generation's publish is durable — the Qdrant
  implementation achieves this by staging new-generation visibility on
  copied/re-tagged points rather than editing the old generation's points in
  place. Get this wrong and a search mid-publish can either see a torn
  generation or lose visibility of unchanged content.

## Collection lifecycle

- **`ensure_collection` is idempotent.** GET-then-PUT-on-404 — never a blind
  PUT — so calling it on every embed operation is safe and never produces a
  409 Conflict on an already-existing collection.
- `crates/axon-vectors/src/collection.rs` provides
  `normalize_collection_spec` (dedupes/sorts payload indexes and aliases,
  ensures required retrieval payload indexes are present) and
  `validate_collection_spec` (non-empty collection name, non-empty dense
  vector name, nonzero dimensions, non-empty sparse vector name if a sparse
  vector is configured) — run new specs through both before creating a
  collection.
- `check_collection_drift` detects when an already-existing collection's
  vector configuration (`dense`/`sparse`) disagrees with what's being
  requested, and errors rather than silently reinterpreting an existing
  collection.
- Named dense + sparse hybrid vectors (Qdrant's `dense` + `bm42` RRF fusion)
  are the current collection shape for new collections — see
  `crates/axon-vectors/src/CLAUDE.md` and `crates/axon-vectors/src/sparse.rs`
  before assuming a single dense-only vector shape.

## Writes go through validated point batches

`crates/axon-vectors/src/point.rs`'s `VectorPointBatchBuilder` is the only
sanctioned way to construct `VectorPointBatch` values — it validates
dimensions match the collection spec, rejects duplicate/unexpected/missing
embedding-chunk correlations (`DuplicateChunkId`, `UnexpectedEmbeddingChunk`,
`MissingEmbeddingChunk`, `DimensionMismatch`, `InvalidDenseVector`,
`EmbeddingBatchMismatch`), and computes stable, deterministic point IDs
(`build_helpers::stable_point_id`). **All vector writes go through validated
point batches** — do not construct raw Qdrant points bypassing this builder.

## Deletes must be scoped safely

`VectorDeleteSelector` filters must match source id, generation, and cleanup
debt selectors **safely** — never construct an over-broad delete filter that
could remove points outside the intended scope. This is the sharp edge:
`axon-vectors` has no dependency on `axon-ledger` (that edge is forbidden —
see Boundary below), so a new store implementation cannot query the ledger
directly to double-check scope; it must trust the `VectorDeleteSelector`
passed in and implement it precisely.

## Credential handling

Strip credentials from the configured endpoint before surfacing any error —
only an opaque `endpoint = "configured"` marker should leak into error
output. Look at the Qdrant implementation's error-path redaction for the
exact pattern before adding a new backend that talks to an authenticated
endpoint.

## Boundary

- No embedding generation, source acquisition, chunking, ledger generation
  commits, or RAG synthesis in this crate.
- No provider throughput decisions beyond store-side backpressure errors —
  that's the embedding provider's job.
- Allowed dependencies: `axon-api`, `axon-error`, `axon-core`,
  `axon-observe`, `axon-embedding` **types** (not implementations), Qdrant
  client + serde/schema crates.
- Forbidden dependencies: embedding provider implementations, source
  adapters, parser implementations, job runtime, transport crates, LLM
  providers, and — critically — `axon-ledger` (the `axon-vectors ->
  axon-ledger` edge is forbidden; cleanup is driven from cleanup debt
  recorded in the ledger and executed by `axon-prune`, never queried ad hoc
  from here). Enforced by `cargo xtask check-layering`.

## Testing

```bash
cargo test -p axon-vectors
```

`crates/axon-vectors/src/testing.rs` exposes `FakeVectorStore` with
deterministic search ordering plus outage/partial/slow fixtures, so
downstream crates (`axon-retrieval`, `axon-services`) can exercise
vector-store-dependent logic without a live Qdrant instance.
