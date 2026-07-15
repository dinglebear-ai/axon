# Technology Choices -- Axon

## Language and runtime

| Component | Technology | Version | Purpose |
|-----------|-----------|---------|---------|
| Core binary | Rust | 1.94+ (edition 2024) | CLI, MCP server, workers, HTTP server |
| Web panel assets | TypeScript | Node 24+ | Embedded setup/config panel assets |
| Scripts | Bash + Python | -- | Maintenance, testing, analysis |

## Key dependencies

### Rust crates

| Crate | Version | Purpose |
|-------|---------|---------|
| `spider` | 2.x | Web crawling engine (HTTP + Chrome rendering) |
| `spider_agent` | 2.47+ | Tavily search integration |
| `spider_transformations` | 2.x | Content transformation (markdown, readability) |
| `rmcp` | 1.5+ | MCP server framework (stdio + streamable-http) |
| `axum` | 0.8 | HTTP server for web panel, MCP, and first-party action routes |
| `tokio` | 1.x | Async runtime (multi-threaded) |
| `sqlx` | 0.8 | SQLite async driver |
| `reqwest` | 0.13 | HTTP client (rustls, streaming) |
| `clap` | 4.x | CLI argument parsing |
| `serde` / `serde_json` | 1.x | Serialization |
| `text-splitter` | 0.30 | Semantic text chunking (code + markdown) |
| `tree-sitter-*` | various | AST-based code chunking (Rust, Python, JS, TS, Go, Bash) |
| `octocrab` | 0.49 | GitHub API client |
| `bollard` | 0.20 | Docker API client |

### Infrastructure

| Service | Image/Version | Purpose |
|---------|--------------|---------|
| SQLite | (embedded) | Job persistence, metadata storage |
| Qdrant | v1.18.2 | Vector database (dense + sparse search) |
| TEI | ghcr.io/huggingface/text-embeddings-inference:89-1.9 | Text embedding generation |
| Chrome | Custom Dockerfile | Headless browser for JavaScript rendering |

### Web panel assets

| Package | Purpose |
|---------|---------|
| Biome | Linter and formatter |
| npm + package-lock.json | Package manager/build runner |

## Embedding pipeline

### TEI (Text Embeddings Inference)

- Default model: `Qwen/Qwen3-Embedding-0.6B`
- Pooling: `last-token`
- Batch size: up to 128 (auto-splits on 413 Payload Too Large)
- Retry: 5 attempts with exponential backoff (1s, 2s, 4s, 8s + jitter)
- GPU acceleration via NVIDIA (optional; CPU fallback available)

### Text chunking

- `chunk_text()`: 2000 characters with 200-character overlap
- Code files: tree-sitter AST-based chunking (preserves function boundaries)
- Each chunk becomes one Qdrant point with `chunk_text` payload field

## Hybrid vector search

New Qdrant collections use named vectors with two search paths:

| Vector | Type | Source | Purpose |
|--------|------|--------|---------|
| `dense` | Float (dimension matches model) | TEI embedding | Semantic similarity |
| `bm42` | Sparse | Computed locally from chunk text | Keyword matching |

Search uses Reciprocal Rank Fusion (RRF) via Qdrant `/query` API:
1. Dense prefetch: HNSW search (`hnsw_ef=128`)
2. Sparse prefetch: BM42 index search
3. RRF fusion: merge and re-rank results

Legacy unnamed-mode collections fall back to dense-only search. Use `axon migrate` to upgrade.

### Tuning

| Parameter | Default | Description |
|-----------|---------|-------------|
| `AXON_HYBRID_SEARCH` | `true` | Enable hybrid search |
| `AXON_HYBRID_CANDIDATES` | `100` | Prefetch candidates per arm |
| `AXON_ASK_HYBRID_CANDIDATES` | `150` | Ask pipeline window (higher for reranking) |
| `AXON_HNSW_EF_SEARCH` | `128` | HNSW ef for named-mode (32-512) |

## Crawl engine

Spider-based crawling with three render modes:

| Mode | Description |
|------|-------------|
| `http` | Pure HTTP fetch (fastest, no JS) |
| `chrome` | Headless Chrome rendering (JS-heavy sites) |
| `auto-switch` (default) | HTTP first; if >60% thin pages, retry with Chrome |

Key Spider features enabled (see `Cargo.toml` for the full list): `basic`, `chrome`, `regex`, `sitemap`, `adblock`, `chrome_stealth`, `chrome_screenshot`, `chrome_store_page`, `chrome_headless_new`, `chrome_simd`, `simd`, `cache_mem`, `ua_generator`, `headers`, `control`, `hedge`.

Features explicitly NOT enabled (see `docs/reference/spider-feature-flags.md`):
- `firewall`: `spider_firewall`'s build.rs fetches blocklists from `api.github.com` unauthenticated and panics under CI rate limits; SSRF is guarded by `validate_url()` in `src/core/http/ssrf.rs` instead
- `balance`: silently throttles with zero logging
- `glob`: causes budget-aware `is_allowed()` to reject first URL with `with_limit(1)`

## Gemini Headless LLM

LLM synthesis operations (`ask`, `evaluate`, `suggest`, `research`, `extract` fallback, `debug`) use the Gemini CLI headless path through `src/core/llm/`:

- `AXON_HEADLESS_GEMINI_CMD` selects the Gemini CLI command.
- `AXON_HEADLESS_GEMINI_HOME` selects the source HOME for Gemini auth copying.
- `AXON_SYNTHESIS_HEADLESS_GEMINI_MODEL` controls the Gemini synthesis model override; `AXON_HEADLESS_GEMINI_MODEL` remains a legacy alias.
- `AXON_LLM_COMPLETION_CONCURRENCY` caps concurrent completions.
- `AXON_LLM_COMPLETION_TIMEOUT_SECS` caps each completion request.

## Build tooling

| Tool | Purpose |
|------|---------|
| just | Task runner (30+ recipes) |
| lefthook | Git hooks |
| sccache | Compilation cache (auto-detected) |
| mold | Fast linker (auto-detected) |
| cargo-nextest | Parallel test runner |
| cargo-deny | Dependency auditing |
| cargo-llvm-cov | Code coverage |

## See also

- [ARCH.md](arch.md) -- architecture patterns
- [PRE-REQS.md](pre-reqs.md) -- prerequisites
- [../repo/RECIPES.md](../../development/repo/recipes.md) -- Justfile recipes
