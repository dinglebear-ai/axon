# src/vector — Embeddings & Vector Search
Last Modified: 2026-05-09

TEI embedding + Qdrant vector store ops. Supports both dense-only and hybrid (dense + sparse BM42) search depending on collection type.

## Module Layout

```
vector/
├── ops.rs           # Crate-level module root re-exporting ops/*
└── ops/
    ├── commands.rs / commands/
    │   ├── ask.rs               # Module root for the ask path
    │   ├── ask/
    │   │   ├── context.rs / context/{build,heuristics,query_rewrite,retrieval,tests}.rs
    │   │   ├── normalize.rs
    │   │   ├── output.rs
    │   │   └── tests.rs
    │   ├── evaluate.rs / evaluate/{display,scoring,streaming}.rs (+ streaming/tests.rs)
    │   ├── query.rs
    │   ├── retrieval.rs
    │   ├── streaming.rs / streaming/{test_support,tests}.rs
    │   └── suggest.rs
    ├── input.rs / input/{classify,code}.rs    # chunk_text(), chunk_code(), classify_file_type(), language_name(), is_test_path()
    ├── input_proptest.rs                       # Property-based chunk_text tests
    ├── qdrant.rs / qdrant/
    │   ├── client.rs            # HTTP/Qdrant client wiring
    │   ├── commands.rs / commands/{dedupe,dispatch,facets,retrieve}.rs
    │   ├── filter.rs            # Qdrant filter builder
    │   ├── hybrid.rs            # qdrant_hybrid_search(), qdrant_named_dense_search()
    │   ├── search.rs            # Standard cosine search path
    │   ├── tests.rs
    │   ├── types.rs             # Qdrant types
    │   └── utils.rs             # Shared helpers
    ├── ranking.rs / ranking/snippet.rs
    ├── ranking_test.rs
    ├── sparse.rs               # compute_sparse_vector(), SparseVector — BM42-style sparse vectors
    ├── source_display.rs
    ├── stats.rs / stats/{display,pg,qdrant_fetch}.rs
    └── tei.rs / tei/
        ├── pipeline.rs          # run_embed_pipeline() — concurrent doc embedding with timeouts
        ├── prepare.rs           # PreparedDoc helpers
        ├── qdrant_store.rs / qdrant_store/tests.rs   # ensure_collection(), VectorMode cache
        ├── tei_client.rs        # tei_embed(), QUERY_INSTRUCTION, retry/backoff, batch sizing
        ├── tei_manifest.rs
        ├── tests.rs
        └── text_embed.rs        # embed_prepared_docs() entry point
```

The diagram is intentionally complete — every named function in this CLAUDE.md should be locatable from the listing above.

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

**Local directory embed** (`axon embed <dir>`, `tei/prepare.rs`) reuses the same code/prose split: it recurses the tree, prunes junk dirs and binary files via `input/select.rs`, and routes local source files (by extension) through `chunk_code` (tagged `content_type = "text"`) while markdown/docs stay on `chunk_markdown` (tagged `"markdown"`). `code::chunk_code` runs inside `spawn_blocking` there because tree-sitter parsing is CPU-bound. Crawl-output dirs (http manifest URLs) stay on prose chunking.

`classify_file_type()` in `input/classify.rs` tags files as `test`/`config`/`doc`/`source` for metadata enrichment. Pure function, no I/O.

### Hybrid Search (Dense + Sparse BM42)

New collections are created with **named vectors** (`dense` + `bm42` sparse). For these collections, query commands use hybrid search instead of dense-only search:

1. **Dense embedding**: TEI encodes the query into a float32 vector
2. **Sparse vector**: `compute_sparse_vector()` in `sparse.rs` computes BM42-style TF weights (FNV-1a hash, `SPARSE_DIM=65_536` buckets, stopword filtering, min-length 3 chars)
3. **Fusion**: Qdrant `/query` endpoint receives two `prefetch` arms (dense + sparse) and fuses with **Reciprocal Rank Fusion (RRF)**

```rust
// sparse.rs — compute sparse vector for a text
let sv: SparseVector = compute_sparse_vector(text);
// SparseVector implements Serialize: serde_json emits { "indices": [...], "values": [...] }

// qdrant/hybrid.rs — issue hybrid query
qdrant_hybrid_search(cfg, &dense_vec, &sparse_vec, limit).await?
```

**Fallback:** When a collection is `VectorMode::Unnamed` (legacy dense-only), the query falls back to standard cosine search via the regular `/points/search` endpoint — no sparse vector is computed.

**Hash collisions:** With 65,536 buckets, ~7% collision rate for 100 unique terms, ~26% for 200 — roughly half the collision rate of the old 30,522-bucket scheme. Qdrant's IDF weighting mitigates remaining impact — high-IDF terms are unlikely to collide. This is a deliberate trade-off vs. requiring the BERT tokenizer vocabulary.

**Config:** `AXON_HYBRID_CANDIDATES` env var (default: `100`) controls the normal query prefetch window size per arm before RRF fusion. `AXON_ASK_HYBRID_CANDIDATES` defaults to `150` for the `ask` pipeline to preserve a wider recall window before LLM synthesis.

### Ranking Pipeline
`ranking.rs` applies BM25-style scoring on top of Qdrant cosine/hybrid results. `ranking/snippet.rs` extracts and highlights matching text fragments. Used by `ask` and `query` commands. Do not bypass ranking in new retrieval commands — it significantly improves answer quality.

The ask reranker has two score-scale contracts:

| Collection vectors | `cfg.hybrid_search_enabled` | Sparse query | Effective scoring | Rerank + threshold | Adaptive full-doc skip gate |
|--------------------|-----------------------------|--------------|-------------------|--------------------|------------------------------|
| Named (`dense` + `bm42`) | `true` | non-empty | RRF rank-fusion score + mode-safe lexical/doc/authority boosts | Applied, but no cosine threshold | Rank-based: every top-K score must be `>= P75` of all reranked candidates (only when `[ask.adaptive] fulldoc-skip-enabled = true`) |
| Named (`dense` + `bm42`) | `true` | empty | Cosine via `named_dense` | Applied | Score-floor: every top-K `rerank_score >= ask_min_relevance_score + ask_fulldoc_skip_score_delta` (only when enabled) |
| Named (`dense` + `bm42`) | `false` | any | Cosine via `named_dense` | Applied | Score-floor (same as above; only when enabled) |
| Unnamed legacy vector | any | n/a | Cosine via `/points/search` | Applied | Score-floor (same as above; only when enabled) |

On RRF mode, Qdrant's fusion order remains the base ranking signal, then `score_and_filter_candidates` applies small lexical URL/chunk, docs-path, phrase, configured-authority, and docs-like URL product-token boosts before context selection. `AXON_ASK_MIN_RELEVANCE_SCORE` is still skipped on RRF because that threshold is calibrated to cosine `[0, 1]`, not rank-fusion output; the topical-overlap gate still applies and rejects generic-only matches when a query includes a salient product/library token. Diagnostics expose `authority_ratio` as the effective max of configured-authority and product-authority matches, plus `configured_authority_ratio` and `product_authority_ratio` separately. Run `axon evaluate --no-hybrid-search` to compare RRF against dense-only behavior on Named collections.

### Retrieval Tuning Rule

Do not tune retrieval from a single query. Before changing scoring, token policy,
authority handling, or context selection, run the tracked retrieval fixture sweep.
Classify every miss first:

- ranking bug: relevant candidates exist but score/filter order is wrong
- selection bug: relevant candidates rank well but do not enter context
- corpus-health gap: expected source is not indexed or indexed too thinly
- fixture mismatch: the fixture expectation does not match indexed content

Hard-coded product/domain allowlists are not allowed in code. User-configured
authoritative domains are allowed through config.

The **adaptive full-doc fetch skip gate** (bd axon_rust-30y) elides `fetch_full_docs(...)` when the reranked top-K already covers >= `ask_fulldoc_skip_min_urls` unique URLs, >= `ask_fulldoc_skip_min_chars` chunk-text bytes, and every score satisfies the mode-specific floor above. The gate defaults to **disabled** because `ask` is normally a Gemini-backed one-shot synthesis path with a large context window, so recall is more valuable than minimizing context assembly. It can be enabled with `[ask.adaptive] fulldoc-skip-enabled = true` in `~/.axon/config.toml` after `axon evaluate` proves no quality regression on the target corpus. The decision is exposed in ask diagnostics as `full_doc_fetch_skipped: bool` and `full_doc_fetch_skip_reason: "ok_skip" | "disabled" | "insufficient_urls" | "insufficient_chars" | "low_top_scores" | "empty_top_k"`. The cosine `score_delta` knob is intentionally ignored on the RRF row because rank-fusion output is unitless — the rank-based gate uses P75 across the full reranked set instead.

### Collection Naming
Default collection: `axon` (set via `AXON_COLLECTION` or `--collection`). Do not hardcode the collection name in new code; always read from `cfg.collection`.

The dispatch entry validates `cfg.collection` against `[A-Za-z0-9_.-]{1,255}` with no leading/trailing dot and no `..`. The validator is a path-injection guard — Qdrant URLs interpolate the collection name without percent-encoding, so a malicious value like `../etc/passwd` would otherwise escape the path.

### Dual-Embedding for Ask

The `ask` retrieval path embeds the question in two forms when they differ meaningfully:

1. **NL form** — the raw question, with `QUERY_INSTRUCTION` prepended (asymmetric encoding).
2. **Keyword form** — the question reduced to its non-stopword tokens joined with spaces. Document-shaped, so it does **not** get the query instruction (see Query Instruction section).

Both vectors are produced in a **single TEI batch call**, then dispatched to Qdrant **in parallel** via `tokio::join!` (sequential dispatch burned ~2-3s/ask before bd axon_rust-d71.3). Results are merged by `(url, chunk-prefix)` deduplication.

This is opt-in by query shape: only kicks in when the keyword form has 3+ tokens and differs from the trimmed NL question. Short / single-keyword / already-keyword-shaped queries skip the secondary dispatch entirely.

### Operational Caveats

A few sharp edges worth knowing before debugging retrieval:

- **VectorMode cache is process-local.** `LazyLock<RwLock<HashMap>>` in `tei/qdrant_store.rs` is populated on first embed/query. `Named` cache hits remain authoritative. Cached legacy `Unnamed` hits are revalidated whenever hybrid search is enabled, so worker processes self-heal after `axon migrate cortex cortex_v2` on their next embed/query instead of silently staying dense-only. (bd axon_rust-d71.2)
- **Empty sparse vector → silent dense-only fallback.** `compute_sparse_vector` returns empty for non-ASCII / all-stopword / very-short queries (every term < 3 chars). `dispatch_vector_search` routes to named-dense in that case. The fallback now logs a `tracing::warn!` with a query character profile, so it's visible at default INFO level (bd axon_rust-d71.9).
- **`ask_min_relevance_score` is calibrated to cosine.** The threshold is intentionally skipped on the RRF code path — Qdrant's RRF fusion handles ordering, and topical-overlap is the only loose-quality gate that survives. Named collections still apply the threshold when hybrid search is disabled or the sparse query is empty. Run `axon evaluate --no-hybrid-search` for A/B comparison against dense-only behavior (bd axon_rust-d71.1 / d71.12).
- **`compute_sparse_vector` returns `SparseVector::default()` (empty `indices`/`values`) for empty/non-indexable input.** Callers must check `sv.is_empty()` before issuing a hybrid query — Qdrant rejects empty sparse arms.
- **Payload schema versioning.** Existing pre-`axon_rust-lu6a` points are implicit schema version `1`; new upserts carry the current `payload_schema_version = 4` (see `qdrant::PAYLOAD_SCHEMA_VERSION` in `ops/qdrant/utils.rs`; the value has since advanced 2 → 3 → 4 as vertical/GitHub fields were promoted to indexed top-level keys). Default retrieval applies no version filter — backward-compatible with all existing points. Opt-in callers use `VectorSearchRequest::with_payload_schema_version_min(Some(N))` to scope to vertical-aware fields. New keyword `extractor_name` payload field is OPTIONAL — generic crawl/embed paths leave it absent rather than writing a placeholder. `axon sources --by-schema-version` produces per-version chunk counts via collection scroll.

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
| `AXON_COLLECTION` | `axon` | Qdrant collection name. Validated at dispatch: `[A-Za-z0-9_.-]`, 1–255 chars, no leading/trailing dot, no `..`. |
| `AXON_HYBRID_SEARCH` | `true` | Master switch for hybrid RRF search on Named collections. `false` forces dense-only on every query (used by `axon evaluate --no-hybrid-search` for A/B comparison). |
| `AXON_HYBRID_CANDIDATES` | `100` | Prefetch window per arm (dense + sparse) before RRF fusion for `query`. Maps to `cfg.hybrid_search_candidates`. |
| `AXON_SOURCES_FACET_LIMIT` | 100,000 | Max URLs returned by `sources` command via facet |
| `AXON_SUGGEST_INDEX_LIMIT` | 50,000 | Max URLs fetched for dedup in `suggest` command |
| `AXON_ASK_HYBRID_CANDIDATES` | `150` | Prefetch window per arm before RRF fusion for `ask`; overrides `cfg.hybrid_search_candidates` for the ask path only. |
| `AXON_ASK_MIN_RELEVANCE_SCORE` | `0.45` | Minimum reranker score to include a candidate on cosine paths. Intentionally skipped as a threshold on the RRF path, though RRF still receives lexical/docs/authority rerank boosts — see Ranking Pipeline above. |

**Retrieval input caps:** `dispatch_vector_search` rejects queries longer than 64 KiB (CWE-770). Queries are validated before reaching `compute_sparse_vector` or TEI.

## TEI Service (External — steamy-wsl)

TEI runs on `steamy-wsl` (RTX 4070), not localhost. Reachable via Tailscale.

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

- **`QUERY_INSTRUCTION`** constant in `src/vector/ops/tei/tei_client.rs` — single source of truth
- Prepended by `query.rs`, `ask/context/retrieval.rs` (NL question only), and `evaluate/scoring.rs` before calling `tei_embed`
- Dual-embedding for ask: when the keyword form differs from the NL form, both are embedded in a single TEI batch. The **NL form gets `QUERY_INSTRUCTION`; the keyword form does not** — keyword tokens are document-shaped, so prefixing them would push the vector into query space and defeat the dual-embedding pass (D-C2 / bd axon_rust-d71.5).
- Document embeds (`pipeline.rs`) do **not** get the prefix — raw text only
- This is correct per the Qwen3-Embedding spec: queries need the instruction, documents must not have it

**If you switch models:** check whether the new model is asymmetric (instruction-aware). If not, remove `QUERY_INSTRUCTION` from the three query callers.

### Connectivity
- TEI is on the `axon` Docker network (Tailscale-accessible)
- It is **never** on `127.0.0.1` — `axon doctor` will fail on TEI if run without Tailscale connectivity
- The `axon` Docker service reaches TEI through the container-internal `TEI_URL` env var.

## Adding a New Vector Command
1. Add to `vector/ops/commands/` (one file per command)
2. Re-export from `ops/commands.rs`
3. Add `CommandKind::*` variant to `src/core/config.rs`
4. Call `ensure_collection(&cfg).await?` before any Qdrant write
5. Prefer `tei_embed_batch()` over `tei_embed()` for multiple texts
