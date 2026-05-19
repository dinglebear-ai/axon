# Session: Performance Review and Fixes
Date: 2026-03-22
Branch: feat/pulse-shell-and-hybrid-search

## Session Overview

Complete performance review and fix cycle on the `axon_rust` codebase. Dispatched 8 parallel performance-only reviewers (scoped to performance/optimization issues only), then 8 parallel `systems-programming:rust-pro` fix agents to address all surfaced issues, followed by a simplify pass.

**Final state:** `cargo check` clean, 1505 tests passing, 0 clippy warnings.

## Timeline

1. **Review phase** — 8 parallel agents loaded `/rust-best-practices`, `/rust-async-patterns`, `/rust-code-review` skills, scoped to performance only. Found ~127 issues across all crates.
2. **Fix phase** — 8 parallel `systems-programming:rust-pro` agents addressed all surfaced issues (83 files, 1826 insertions, 2348 deletions net).
3. **Integration** — Reverted worktree-1 (TEI) and worktree-3 (RAG) changes that were incompatible with the current feature branch API surface. Applied remaining 6 fix agents directly.
4. **Compilation repair** — Fixed 2 stable-API violations introduced by fix agents.
5. **Simplify pass** — Code quality cleanup: removed duplicate helpers, unified query text resolution, cleaned up unnecessary comments.

## Key Findings

### Critical Performance Issues Fixed

| Issue | Location | Impact |
|-------|----------|--------|
| `std::fs` blocking calls in async hot loop | `crates/crawl/engine/collector.rs:54` | Blocks tokio thread pool |
| N+1 embed per Reddit post | `crates/ingest/reddit.rs` | O(n) sequential embed calls |
| Sequential Neo4j writes per chunk | `crates/jobs/graph/worker.rs` | 350+ HTTP round-trips/doc → 4 queries |
| `std::env::var()` on every search request | `crates/vector/ops/qdrant/utils.rs` | Acquires global env lock per query |
| Full 7M-point payload fetch for dedupe | `crates/vector/ops/qdrant/client.rs` | ~14GB transfer vs ~500MB selective |
| Taxonomy JSON parsed per graph job | `crates/jobs/graph/taxonomy.rs` | Repeated JSON parse + HashMap build |
| Config cloned per job (30+ heap allocs) | `crates/jobs/worker_lane.rs` | Per-job allocation vs Arc clone |
| Sequential Qdrant stats requests | `crates/vector/ops/stats/qdrant_fetch.rs` | 3 sequential HTTP calls → parallel |
| DDL `ensure_schema` on every command | `lib.rs` | Unnecessary DB round-trip |
| O(n²) link dedup | `crates/core/content.rs` | Vec::contains in loop |
| Clone-all-then-sort for ranking | `crates/vector/ops/ranking.rs` | ~600KB/query for 150 candidates |
| CPU-bound `to_markdown` on async executor | `crates/core/content/engine.rs` | Starves async I/O tasks |
| Watchdog sequential per-job UPDATEs | `crates/jobs/common/watchdog.rs` | N queries → 1 batch query |
| Stats broadcast with zero subscribers | `crates/web/docker_stats.rs` | Wasted JSON build + broadcast |
| `SPAWN_LOCKS` DashMap unbounded growth | `crates/web/execute/sync_mode/pulse_chat/connection.rs` | Memory leak |
| Sitemapindex scan of full XML body | `crates/crawl/engine/sitemap.rs` | Multi-MB scan → 512-byte probe |
| Sequential `reflink_or_copy` | `crates/crawl/engine/dir_ops.rs` | Sequential → parallel JoinSet |
| Scrape fallback client created per-call | `crates/crawl/scrape.rs` | Repeated TLS handshake setup |
| PgPool created per-scrape | `crates/services/scrape.rs` | Connection pool thrash |

## Technical Decisions

### Arc<Config> Pattern
Changed `ProcessFn` from `Fn(Config, ...)` to `Fn(Arc<Config>, ...)` in `worker_lane.rs`. Each lane creates the Arc once; all job dispatches use `Arc::clone` (~30 heap allocations → 1 atomic increment per job). Required fixing Sessions branch which mutated config fields — must use `(*cfg).clone()` (deref Arc→Config) not `arc.clone()` (clones the Arc pointer).

### OnceLock stable workaround
`OnceLock::get_or_try_init` is nightly-only. Stable pattern for fallible initialization:
```rust
let client = if let Some(c) = STATIC.get() { c } else {
    let built = build_client(cfg)?;
    let _ = STATIC.set(built);
    STATIC.get().expect("just initialized")
};
```
Race condition acceptable since client construction is idempotent.

### Worktree isolation hazard
`isolation: worktree` spawns agents from `main`, not the current feature branch. Worktree-1 (TEI) and worktree-3 (RAG) were reverted because they referenced APIs that no longer exist in `feat/pulse-shell-and-hybrid-search` (`chunk_markdown`, `prepend_query_instruction` export path, `EmbedSummary.docs_failed`).

### Qdrant selective payload
Added `qdrant_scroll_pages_selective()` with configurable `with_payload` (list of field names). Dedupe and detailed_domains now fetch only `url`, `chunk_index`, `scraped_at` — ~27x reduction in data transfer on the 7M-point cortex collection.

### Neo4j UNWIND batch queries
Replaced sequential per-chunk `CREATE` queries with `UNWIND $chunks AS chunk MERGE ... SET c += chunk.props`. 350+ round-trips per document → 4 queries total (chunks, entities, relations, document node).

### Index-based ranking
Clone-all-then-sort (150 candidates × 4 Strings + 2 HashSets = ~600KB/query) replaced with index-based scoring: score on `(usize, f64)` pairs, sort indices, clone only selected top-K at the end.

## Files Modified

| File | Change |
|------|--------|
| `crates/crawl/scrape.rs` | Static `OnceLock` for fallback reqwest client |
| `crates/crawl/engine/collector.rs` | `tokio::fs::try_exists`, throttled progress emit |
| `crates/crawl/engine/sitemap.rs` | 512-byte sitemapindex probe |
| `crates/crawl/engine/dir_ops.rs` | Parallel `JoinSet::spawn_blocking` for file copies |
| `crates/core/content.rs` | `HashSet` for O(1) link dedup |
| `crates/core/content/engine.rs` | `spawn_blocking` for CPU-bound `to_markdown` |
| `crates/jobs/common/watchdog.rs` | Batch UPDATE with `ANY($1::uuid[])` |
| `crates/jobs/worker_lane.rs` | `Arc<Config>`, stale sweep only on lane 0 |
| `crates/jobs/graph/taxonomy.rs` | `LazyLock<Arc<Taxonomy>>` for parsed taxonomy |
| `crates/jobs/graph/worker.rs` | Neo4j UNWIND batch writes |
| `crates/jobs/ingest/process.rs` | `Arc<Config>` + `(*cfg).clone()` for sessions mutation |
| `crates/ingest/reddit.rs` | Batch embed via mpsc channel, 50-post batches |
| `crates/vector/ops/qdrant/client.rs` | `qdrant_scroll_pages_selective()` |
| `crates/vector/ops/qdrant/utils.rs` | `LazyLock` for env-var HNSW params |
| `crates/vector/ops/stats/qdrant_fetch.rs` | `tokio::join!` for parallel stats |
| `crates/vector/ops/ranking.rs` | Index-based scoring, clone only top-K |
| `crates/vector/ops/commands.rs` | `as_deref()` pattern, removed unnecessary clone |
| `crates/web/docker_stats.rs` | Skip build/broadcast when receiver_count == 0 |
| `crates/web/execute/sync_mode/pulse_chat/connection.rs` | TTL eviction for SPAWN_LOCKS DashMap |
| `crates/services/scrape.rs` | Shared telemetry pool, DDL OnceLock |
| `crates/cli/commands.rs` | Shared `resolve_input_text()` helper |
| `crates/cli/commands/research.rs` | Restored event channel comment |
| `lib.rs` | DDL OnceLock, `record_command_run` takes `(pg_url, command)` |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | 0 errors | 0 errors | ✅ |
| `cargo test --lib` | All pass | 1505 passing | ✅ |
| `cargo clippy` | 0 warnings | 0 warnings | ✅ |
| `cargo fmt --check` | Clean | Clean | ✅ |

## Risks and Rollback

- **Arc<Config> change** is the most invasive — affects all job dispatch paths. If a new branch mutates Config fields in process functions, will hit borrow-checker errors. Fix: `(*cfg).clone()` before mutation.
- **OnceLock race on scrape client** — two threads may both build the client; the loser's result is dropped. Idempotent construction makes this safe, but if client construction has side effects (e.g., opens a socket), this could be a concern.
- **Worktree isolation** — any future subagent work with `isolation: worktree` must be verified against the current branch's API before applying.
- Rollback: `git revert` the relevant commits, or `git checkout HEAD -- <files>` for individual regressions.

## Decisions Not Taken

- **SQL deduplication in `export/query.rs`** — 4 functions each duplicating ~95% of SQL (if-else for WHERE clause). Left as-is because sqlx macro constraints make dynamic query building complex and the performance gain was already captured by pushing filters to SQL.
- **Worktree-1 TEI changes** — Reverted. Agent worked on `main` branch with different `input.rs` API (`chunk_markdown` fn, `prepend_query_instruction` export). Would require forward-porting.
- **Worktree-3 RAG changes** — Reverted. Same incompatibility (`VectorMode` import path, `EmbedSummary.docs_failed` field).

## Open Questions

- Is the 5-minute TTL for `SPAWN_LOCKS` DashMap appropriate? Depends on longest expected Pulse chat session without new spawns.
- Neo4j UNWIND batch size — currently unbounded per document. Should cap at ~500 chunks to avoid large Bolt payloads.
- Reddit ingest batch size of 50 posts — should this be configurable via env var?

## Next Steps

- Monitor `SPAWN_LOCKS` DashMap size in production to validate TTL eviction effectiveness.
- Consider making Reddit ingest batch size configurable (`AXON_REDDIT_EMBED_BATCH_SIZE`).
- Neo4j batch size cap for very long documents.
- Forward-port TEI and RAG optimizations from worktree agents (requires adapting to current branch API).
