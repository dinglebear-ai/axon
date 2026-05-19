# Session: Unified Embed Pipeline

**Date:** 2026-03-14
**Branch:** `feat/web-integration-review-fixes`
**Duration:** Multi-session (continued from prior context)

---

## Session Overview

Consolidated all ingest sources (GitHub files, GitHub issues/PRs/wiki/metadata, Reddit, YouTube, Sessions) onto the crawl pipeline (`run_embed_pipeline`). Deleted the broken batch machinery (`EmbedDocument`, `PreparedBatchDocument`, `embed_documents_batch`, `embed_documents_in_batches`, `embed_pipeline.rs`). All ingest paths now use one `PreparedDoc` per logical unit with all chunks in `Vec<String>`, processed concurrently via `FuturesUnordered`.

Also added Qdrant resource limits to prevent CPU spikes from HNSW index rebuilds.

---

## Timeline

1. **Qdrant resource limits** — Added `cpus: '4.0'` to `axon-qdrant` in `docker-compose.yaml`; added `max_optimization_threads: 2` to `docker/qdrant/production.yaml`
2. **Discovery** — Audited three embedding paths: crawl pipeline (`run_embed_pipeline`), batch path (`embed_documents_batch`), and batch wrapper (`embed_documents_in_batches`); confirmed batch path is broken (413 risk, double-chunking bug)
3. **Design** — Decided to consolidate on crawl pipeline; wrote implementation plan at `docs/superpowers/plans/2026-03-14-unified-embed-pipeline.md` (5 tasks + verify checkpoint + spec review loop)
4. **Task 1** — Extended `PreparedDoc` with 4 new fields; added `embed_prepared_docs` entry point; updated pipeline payload builder
5. **Task 2** — Migrated `github/files.rs` to one `PreparedDoc` per file (reverted bad Task-3-from-prior-session pre-chunking)
6. **Task 2b** — Migrated `github.rs`, `github/issues.rs`, `github/wiki.rs`; deleted `embed_github_docs`
7. **Task 3** — Migrated `reddit.rs`, `youtube.rs`; deleted `embed_reddit_documents`, `embed_youtube_documents`; fixed `Send` bound in `pipeline.rs`
8. **Task 4** — Migrated `sessions.rs`; simplified `embed_session_text` from 3-layer wrapper to direct `PreparedDoc`
9. **Task 5** — Deleted all dead code: 7 items from `tei.rs`, updated `ops.rs` re-exports, deleted `embed_pipeline.rs`, removed `pub mod embed_pipeline` from `ingest.rs`

---

## Key Findings

- **Root cause of 413 errors**: `embed_documents_batch` collected ALL chunks from ALL files into one giant TEI call. 500 files × 5 chunks/file = 2500 chunks in one HTTP call → 413 Payload Too Large
- **Double-chunking bug**: A prior commit made `read_file_embed_doc` pre-chunk into one `EmbedDocument` per chunk; `prepare_batch_document` inside `embed_documents_batch` ALSO chunked — net effect was a wasteful no-op (each pre-chunk re-chunked into exactly 1 chunk)
- **Three undiscovered callers**: Initial plan missed `github.rs`, `github/issues.rs`, `github/wiki.rs` — all using `EmbedDocument` + `embed_documents_in_batches`. Caught by spec reviewer subagent before Task 5 deletion
- **`Send` bound fix**: `FuturesUnordered` inside `pipeline::run_embed_pipeline` held `Box<dyn Error>` (`!Send`) values across await points. Fixed in Task 3 by changing internal helpers to `Box<dyn Error + Send + Sync>`, converting at boundaries. Public API retains `Box<dyn Error>`
- **Correct architecture**: One `PreparedDoc` per file → one TEI call with K chunks → buffered Qdrant upserts. GPU utilization good (multiple chunks per call), no 413 risk (chunks bounded per file), correct stale-tail cleanup per URL

---

## Technical Decisions

| Decision | Rationale | Alternative Rejected |
|----------|-----------|----------------------|
| One PreparedDoc per file (not per chunk) | TEI is most efficient with multiple chunks per call; one PreparedDoc per chunk = N single-chunk TEI calls | One PreparedDoc per chunk — O(N files × chunks) TEI calls |
| `FuturesUnordered` concurrency | Already used by crawl pipeline, battle-tested; `AXON_EMBED_DOC_CONCURRENCY` controls parallelism | Sequential embed — too slow for large repos |
| Delete batch path entirely | Dead code after migration; nothing should resurrect it | Deprecate but keep — increases maintenance burden with no benefit |
| `pub(crate)` visibility on PreparedDoc fields | Allows `pipeline.rs` to access fields directly without getter methods | Getter methods — unnecessary boilerplate for crate-internal type |
| `content_type: &'static str` | Zero-cost string for two known values ("markdown", "text") | `String` — heap allocation for compile-time constant |

---

## Files Modified

| File | Change |
|------|--------|
| `docker/qdrant/production.yaml` | Added `max_optimization_threads: 2` |
| `docker-compose.yaml` | Added `cpus: '4.0'` to axon-qdrant service |
| `crates/vector/ops/tei.rs` | Extended `PreparedDoc`, added `embed_prepared_docs`, deleted 7 batch items |
| `crates/vector/ops/tei/pipeline.rs` | Updated payload builder to use PreparedDoc fields; fixed `Send` bound |
| `crates/vector/ops/tei/prepare.rs` | Set crawl defaults on PreparedDoc struct literal |
| `crates/vector/ops/tei/tests.rs` | Added `prepared_doc_with_ingest_metadata_compiles` test |
| `crates/vector/ops.rs` | Removed batch re-exports; added `pub(crate) use tei::{PreparedDoc, embed_prepared_docs}` |
| `crates/ingest/github/files.rs` | One PreparedDoc per file; deleted `embed_collected_docs` |
| `crates/ingest/github.rs` | Repo metadata → PreparedDoc |
| `crates/ingest/github/issues.rs` | Issues + PRs → PreparedDoc; deleted `embed_github_docs` |
| `crates/ingest/github/wiki.rs` | Wiki pages → PreparedDoc |
| `crates/ingest/reddit.rs` | Posts + threads → PreparedDoc; deleted `embed_reddit_documents` |
| `crates/ingest/youtube.rs` | Transcript + description → PreparedDoc; deleted `embed_youtube_documents` |
| `crates/ingest/sessions.rs` | `embed_session_text` → direct PreparedDoc |
| `crates/ingest.rs` | Removed `pub mod embed_pipeline` |
| `docs/superpowers/plans/2026-03-14-unified-embed-pipeline.md` | Implementation plan (written + executed) |

**Deleted:**
- `crates/ingest/embed_pipeline.rs` — `embed_documents_in_batches` wrapper (64 lines)

---

## Commands Executed

```bash
# Verification after all tasks
cargo test --lib
# → 1286 passed; 0 failed; 5 ignored

# Dead code check (pre-deletion gate)
grep -rn "EmbedDocument|embed_documents_in_batches|embed_documents_batch" crates --include="*.rs" | grep -v "tei.rs|ops.rs"
# → Only embed_pipeline.rs itself (the file being deleted) — all callers clean

# Final dead code confirmation (post-deletion)
grep -rn "embed_documents_batch|embed_documents_in_batches|EmbedDocument|PreparedBatchDocument" crates --include="*.rs"
# → 0 results

# embed_prepared_docs callers
grep -rn "embed_prepared_docs" crates --include="*.rs"
# → 8 results: github/files.rs, github.rs, github/issues.rs, github/wiki.rs,
#               reddit.rs, youtube.rs, sessions.rs (callers) + tei.rs (def) + ops.rs (re-export)
```

---

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| GitHub file embedding | All file chunks batched into one giant TEI call → 413 on large repos | One TEI call per file with all that file's chunks; concurrent via FuturesUnordered |
| GitHub issues/PRs | `embed_github_docs` → `embed_documents_in_batches` → one TEI call for all issues | One PreparedDoc per issue/PR → concurrent TEI |
| Reddit posts | `embed_reddit_documents` → batch path | Direct `PreparedDoc` + `embed_prepared_docs` per post |
| YouTube transcripts | `embed_youtube_documents` → batch path | Direct `Vec<PreparedDoc>` per video |
| Sessions | `embed_session_text` → 3-layer wrapper → batch path | Direct single `PreparedDoc` |
| Qdrant payload | `source_command: "embed"`, `content_type: "markdown"` hardcoded for all sources | Uses `PreparedDoc.source_type` and `PreparedDoc.content_type`; ingest sources get correct values |
| Qdrant resource usage | HNSW rebuilds used all available CPU threads | Capped at 2 optimization threads; container CPU hard-limited to 4.0 cores |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo test --lib` | 0 failures | 1286 passed, 0 failed | ✅ |
| `cargo clippy -- -D warnings` | 0 errors | 0 errors | ✅ |
| `./scripts/enforce_monoliths.py` | 0 FAIL/ERROR | 0 violations | ✅ |
| `grep -rn "EmbedDocument\|embed_documents_batch"` | 0 results | 0 results | ✅ |
| `grep -rn "embed_prepared_docs"` | 9 results (7 callers + def + re-export) | 9 results | ✅ |
| lefthook pre-commit on Task 5 commit | all hooks pass | all hooks pass | ✅ |

---

## Source IDs + Collections Touched

No live Axon embed/retrieve operations were performed during this session (all work was code modification and compilation). The session markdown itself is embedded below.

---

## Risks and Rollback

**Risk:** `Send` bound fix in `pipeline.rs` (Task 3 side-effect) changed internal error types from `Box<dyn Error>` to `Box<dyn Error + Send + Sync>`. If any downstream consumer of pipeline errors relies on downcasting to a specific error type, it would break silently.

**Rollback:** `git revert 89c4011d 8d22e7f5 2a7c93b0 aa2bce2b 99dfb55d 95add431` — reverts all 6 pipeline unification commits in order. The old batch path was self-contained; reverting restores it completely.

**Risk (low):** The prior bad commit (`1a78dc82` — pre-chunking per chunk) is superseded but still in git history. It is not reverted, only superseded. If someone cherry-picks it onto another branch, it will re-introduce the double-chunking bug.

---

## Decisions Not Taken

- **Keep `embed_documents_batch` as deprecated with warning**: Rejected — dead code with no callers is a maintenance liability. Hard delete is cleaner.
- **One PreparedDoc per chunk**: Rejected — would result in N single-chunk TEI HTTP calls per file. One PreparedDoc per file with all chunks = one batched TEI call. Much more GPU-efficient.
- **Expose `embed_prepared_docs` as `pub` (not just `pub(crate)`)**: Rejected — external callers should use the CLI/MCP interface, not raw pipeline functions.
- **Keep double `source_command` / `source_type` fields in Qdrant payload**: Both are kept in the payload builder for backward compatibility with existing Qdrant points that have `source_command`. The crawl path sets `source_type: "embed"` for both fields.

---

## Open Questions

- `source_command` field in Qdrant payload is now set to `doc.source_type` (same value as `source_type`) for ingest sources. Existing crawl points have `source_command: "embed"`. Is `source_command` used in any queries? If not, it could be dropped.
- The `1a78dc82` commit (bad pre-chunking) is still in git history on this branch. Should it be squashed before merging to main?
- `embed_text_with_metadata`, `embed_text_with_extra_payload`, `embed_code_with_metadata` are kept — still used by the refresh worker (`crates/jobs/refresh/url_processor.rs`). These could eventually be unified with the pipeline path, but that's a separate refactor.

---

## Next Steps

- Run a live ingest of a large GitHub repo to verify no 413 errors and correct Qdrant payload metadata
- Verify refresh worker still functions correctly with `embed_text_with_metadata` (not part of this change, but worth a smoke test)
- Consider squashing `1a78dc82` (bad pre-chunking commit) before merging branch to main
- Update `crates/ingest/CLAUDE.md` — the "Embedding Pattern" table still references the old `embed_text_with_metadata` / `embed_text_with_extra_payload` / `embed_code_with_metadata` functions as the primary ingest paths. Should be updated to reflect `embed_prepared_docs` as the canonical path
