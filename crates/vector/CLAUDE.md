# crates/vector — Embeddings & Vector Search
Last Modified: 2026-03-16

TEI embedding + Qdrant vector store ops. Supports both dense-only and hybrid (dense + sparse BM42) search depending on collection type.

## Module Layout

```
vector/ops/
├── commands/        # ask/, ask.rs, evaluate.rs, query.rs, streaming.rs, suggest.rs
├── input.rs         # module root: chunk_text(), url_lookup_candidates()
├── input/
│   ├── classify.rs  # classify_file_type(), language_name(), is_test_path()
│   └── code.rs      # chunk_code() — tree-sitter AST-aware code chunking
├── qdrant/          # client.rs, commands.rs, types.rs, utils.rs
│   └── hybrid.rs    # qdrant_hybrid_search(), qdrant_named_dense_search() — hybrid/named-mode search
├── ranking.rs       # BM25-style reranking module root
├── ranking/         # snippet.rs (helpers used by ranking.rs)
├── sparse.rs        # compute_sparse_vector(), SparseVector — BM42-style sparse vectors
├── stats/           # display.rs, pg.rs, qdrant_fetch.rs
├── tei.rs           # tei_embed(), PreparedDoc, EmbedSummary, embed_prepared_docs()
├── tei/
│   ├── tei_manifest.rs
│   └── qdrant_store.rs  # ensure_collection(), VectorMode detection, named vs unnamed collection management
└── source_display.rs
```

## Critical Patterns

### LazyLock HTTP Client
`static HTTP_CLIENT: LazyLock<reqwest::Client>` in `ops/tei.rs` — use this, never `reqwest::Client::new()` per call. New clients per call exhaust sockets and ignore connection pooling.

### TEI Batch Size / 413 Handling
`tei_embed()` auto-splits batches on HTTP 413 (Payload Too Large). Controlled by `TEI_MAX_CLIENT_BATCH_SIZE` env var (default: 64, max: 128). Do not manually split batches before calling `tei_embed()` — it handles this internally.

### TEI 429 / Rate Limiting
On 429 or 503, `tei_embed()` retries up to **5 times** with exponential backoff starting at 1s (1, 2, 4, 8, 16s) + jitter. Override with `TEI_MAX_RETRIES` env var. The default is tuned so worst-case retry budget (~181s) fits inside the 300s doc timeout.

### Pipeline Resilience
`run_embed_pipeline()` in `tei/pipeline.rs` processes docs concurrently with per-doc timeouts. Individual doc failures (TEI timeout, transport error) are **logged and skipped** — they do not abort the remaining batch. `EmbedSummary.docs_failed` reports how many docs failed. The pipeline uses **upsert-first** (deterministic UUID v5 point IDs overwrite existing) then **stale-tail cleanup** after successful upsert — no data is deleted until the replacement is safely stored.

### ensure_collection() — GET First + VectorMode Detection
`ensure_collection()` in `tei/qdrant_store.rs` does **GET first, PUT only on 404**. Safe to call on every embed — no 409 Conflict on existing collections.

It also detects (and caches) the collection's **VectorMode**:

| State | Action | Result |
|-------|--------|--------|
| Collection doesn't exist | Create with named `dense` + `bm42` sparse | `VectorMode::Named` |
| Collection exists with named `dense` | Ensure `bm42` sparse index exists; PATCH if missing | `VectorMode::Named` |
| Collection exists with unnamed vector | No changes | `VectorMode::Unnamed` |

`VectorMode` is cached in a process-wide `OnceLock<RwLock<HashMap>>`. Cache is populated on first embed and reused on all subsequent embeds and queries — no repeated Qdrant introspection calls. `RwLock` allows unlimited concurrent readers; the rare write (first-time population per collection) briefly takes an exclusive lock.

### Scroll vs Facet — Performance Critical
| Use case | Function | Cost |
|----------|----------|------|
| Aggregate (count URLs, list domains) | `qdrant_url_facets()` via `/facet` POST | O(1) |
| Iterate all points | `qdrant_scroll_pages()` (streaming, callback) | O(n) — use sparingly |
| **Never** use | `qdrant_scroll_all()` | O(n) — loads everything into memory |

Any new command that needs URL counts/dedup **must** use `qdrant_url_facets`. A full scroll on a 2M+ point collection takes 60-80 seconds.

### Code Chunking (tree-sitter)
`chunk_code()` in `input/code.rs` splits source code at AST boundaries (functions, structs, classes) using tree-sitter grammars. Returns `Option<Vec<String>>` — `None` means no grammar for the extension, caller should fall back to `chunk_text()`. Supported: Rust, Python, JavaScript, TypeScript/TSX, Go, Bash. Chunk range: 500–2000 chars. GitHub ingest builds `PreparedDoc` with code chunks and embeds via `embed_prepared_docs`.

`classify_file_type()` in `input/classify.rs` tags files as `test`/`config`/`doc`/`source` for metadata enrichment. Pure function, no I/O.

### Hybrid Search (Dense + Sparse BM42)

New collections are created with **named vectors** (`dense` + `bm42` sparse). For these collections, query commands use hybrid search instead of dense-only search:

1. **Dense embedding**: TEI encodes the query into a float32 vector
2. **Sparse vector**: `compute_sparse_vector()` in `sparse.rs` computes BM42-style TF weights (FNV-1a hash, `SPARSE_DIM=30_522` buckets, stopword filtering, min-length 3 chars)
3. **Fusion**: Qdrant `/query` endpoint receives two `prefetch` arms (dense + sparse) and fuses with **Reciprocal Rank Fusion (RRF)**

```rust
// sparse.rs — compute sparse vector for a text
let sv: SparseVector = compute_sparse_vector(text);
// sv.to_json() → { "indices": [...], "values": [...] }

// qdrant/hybrid.rs — issue hybrid query
qdrant_hybrid_search(cfg, &dense_vec, &sparse_vec, limit).await?
```

**Fallback:** When a collection is `VectorMode::Unnamed` (legacy dense-only), the query falls back to standard cosine search via the regular `/points/search` endpoint — no sparse vector is computed.

**Hash collisions:** With 30,522 buckets, ~15% collision rate for 100 unique terms, ~48% for 200. Qdrant's IDF weighting mitigates impact — high-IDF terms are unlikely to collide. This is a deliberate trade-off vs. requiring the BERT tokenizer vocabulary.

**Config:** `cfg.hybrid_search_candidates` controls the prefetch window size for each arm before RRF fusion (default: at least `limit`).

### Ranking Pipeline
`ranking.rs` applies BM25-style scoring on top of Qdrant cosine/hybrid results. `ranking/snippet.rs` extracts and highlights matching text fragments. Used by `ask` and `query` commands. Do not bypass ranking in new retrieval commands — it significantly improves answer quality.

### Collection Naming
Default collection: `cortex` (set via `AXON_COLLECTION` or `--collection`). The legacy `firecrawl` alias resolves to `cortex` — GET returns 200, `ensure_collection()` exits early. Do not hardcode `cortex` in new code; always read from `cfg.collection`.

## Testing

```bash
cargo test tei            # TEI embed, batch-split, 413/429 retry logic (uses httpmock)
cargo test ranking        # BM25 ranking pipeline + snippet extraction
cargo test qdrant         # Qdrant client, scroll, facet, ensure_collection
cargo test chunk_text     # text chunking (7 tests, no services needed)
cargo test chunk_code     # tree-sitter AST code chunking (23 tests)
cargo test classify_file  # file type classification (46 tests)
cargo test sparse         # sparse vector computation, stopwords, hash stability (9 tests)
cargo test -- --nocapture # show request/response debug output
```

All TEI, Qdrant, and sparse tests run without live services (`httpmock` for network calls; sparse tests are pure computation).

## Key Env Vars (Vector Tuning)

| Var | Default | Effect |
|-----|---------|--------|
| `TEI_MAX_CLIENT_BATCH_SIZE` | 64 (max 128) | Batch size before auto-split on 413 |
| `AXON_COLLECTION` | `cortex` | Qdrant collection name |
| `AXON_SOURCES_FACET_LIMIT` | 100,000 | Max URLs returned by `sources` command via facet |
| `AXON_SUGGEST_INDEX_LIMIT` | 50,000 | Max URLs fetched for dedup in `suggest` command |
| `hybrid_search_candidates` (Config field) | `≥ limit` | Prefetch window per arm before RRF fusion (set via CLI `--hybrid-search-candidates`) |

## TEI Service (External — steamy-wsl)

TEI runs on `steamy-wsl` (RTX 4070), not localhost. Reachable via `jakenet` (Tailscale).

```
TEI_URL=http://steamy-wsl:52000
```

### Model: Qwen/Qwen3-Embedding-0.6B
- **Pooling**: `last-token` (not mean pooling — relevant if comparing to other models)
- **dtype**: float16 (GPU-optimized)
- **Max client batch size**: 128 — matches `TEI_MAX_CLIENT_BATCH_SIZE` CLI cap
- **Max batch tokens**: 163,840 — large budget; unlikely to hit in practice
- **Auto-truncate**: enabled — chunks exceeding the model's max sequence length are **silently truncated**, not rejected. Long chunks lose their tail without error.

### Query Instruction (Asymmetric Encoding)
`--default-prompt` has been **removed** from the TEI Docker config. The instruction is now applied in Rust at query time only.

- **`QUERY_INSTRUCTION`** constant in `crates/vector/ops/tei/tei_client.rs` — single source of truth
- Prepended by `query.rs`, `ask/context/retrieval.rs`, and `evaluate/scoring.rs` before calling `tei_embed`
- Document embeds (`pipeline.rs`) do **not** get the prefix — raw text only
- This is correct per the Qwen3-Embedding spec: queries need the instruction, documents must not have it

**If you switch models:** check whether the new model is asymmetric (instruction-aware). If not, remove `QUERY_INSTRUCTION` from the three query callers.

### Connectivity
- TEI is on `jakenet` (external Docker network, Tailscale-accessible)
- It is **never** on `127.0.0.1` — `axon doctor` will fail on TEI if run without Tailscale connectivity
- The `axon` Docker workers inside docker-compose reach it via `TEI_URL` env var (must be set in `.env`)

## Adding a New Vector Command
1. Add to `vector/ops/commands/` (one file per command)
2. Re-export from `ops/commands.rs`
3. Add `CommandKind::*` variant to `crates/core/config.rs`
4. Call `ensure_collection(&cfg).await?` before any Qdrant write
5. Prefer `tei_embed_batch()` over `tei_embed()` for multiple texts
