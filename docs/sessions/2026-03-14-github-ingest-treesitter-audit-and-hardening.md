# GitHub Ingest + Tree-sitter Audit & Hardening
**Date:** 2026-03-14
**Branch:** main
**Session type:** Parallel agent audit → implementation → module refactor

---

## Session Overview

Comprehensive audit of the GitHub repo ingestion pipeline and tree-sitter AST chunking subsystem, followed by two rounds of parallel agent implementation. The first round addressed 9 identified issues across chunking quality, Qdrant indexing, metadata schema, and memory safety. The second round split three oversized/allowlisted modules into proper submodules and consolidated a redundant Qdrant payload field.

**Net result:** 13 Rust files modified or created, 1295 tests passing, 0 clippy warnings, monolith allowlist cleared, and the GitHub ingest pipeline materially improved in safety, observability, and query performance.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | Dispatched 3 parallel Explore agents to audit tree-sitter chunking logic, GitHub ingest integration, and embed-to-Qdrant pipeline |
| +5 min | Agent 2 (GitHub ingest) returned — no binary file guard, no file size limit, all docs buffered before embedding |
| +8 min | Agent 3 (embed pipeline) returned — `gh_chunking_method` only on GitHub path, no universal field, 3 missing Qdrant indexes |
| +12 min | Agent 1 (tree-sitter) returned — 7 languages, no AST overlap, redundant filter in `tei.rs`, `.expect()` noted |
| Round 1 | Dispatched 4 parallel `rust-pro` agents to implement all 9 recommendations (non-overlapping file ownership) |
| Round 1 done | All 4 returned clean; 1295 tests pass; `files.rs` at 511L (over limit), allowlist stale, field redundancy introduced |
| Round 2 | Dispatched 3 parallel `rust-pro` agents: split `files.rs`, split `tei.rs`, consolidate `gh_chunking_method` |
| Done | Full workspace: `cargo check` clean, `cargo clippy` 0 warnings, 1295 tests pass |

---

## Key Findings (Audit Phase)

**Tree-sitter chunking (`crates/vector/ops/input/code.rs`):**
- 7 languages supported (Rust, Python, JS, JSX, TS, TSX, Go, Bash); no Java/C/C#/Kotlin
- `ChunkConfig::new(500..2000)` — no overlap, unlike prose which has 200-char overlap
- Redundant `.filter(|c| !c.is_empty())` on `Option` return in `tei.rs:199`
- `.expect("valid language")` on `CodeSplitter::new()` — safe but non-idiomatic

**GitHub ingest (`crates/ingest/github/files.rs`):**
- No file size cap — 50MB+ files fully loaded into memory before tree-sitter parse
- All `PreparedDoc`s buffered into single `Vec` before any are submitted to TEI
- Non-UTF8/binary files gracefully skipped via `read_to_string` error (not a bug, just implicit)
- `is_indexable_source_path()` missing: `.ipynb`, `.proto`, `.adoc`, `.zig`, `.ex`, `.sql`, `.tf`, `.nix`, `.r`, `.lua`
- Missing excluded dirs: `build/`, `vendor/`, `.gradle/`, `.terraform/`, `.next/`, `venv/`, `.pytest_cache/`

**Qdrant/embed pipeline (`crates/vector/ops/tei/`):**
- No Qdrant keyword indexes on `gh_file_language`, `source_type`, `chunking_method` — full collection scans on 2M+ points
- `gh_chunking_method` only populated for GitHub ingest path; web-crawled code had no chunking metadata
- Redundant `source_command` field in every Qdrant point (duplicated `source_type`)
- No `line_number_range` stored — couldn't link chunks back to GitHub source view
- No thin-page filter on ingest sources (intentional design difference from crawl path)

---

## Technical Decisions

**Batched flushing over true streaming (Issue #2):** `embed_prepared_docs` accepts `Vec<PreparedDoc>`. Rather than refactoring its signature (which would cascade through MCP/web handlers), implemented `collect_and_embed_batched()` that flushes every `EMBED_BATCH_SIZE=50` docs. Bounds peak memory without API churn.

**`gh_chunking_method` → universal `chunking_method`:** After Round 1 introduced both `gh_chunking_method` (GitHub-specific, in `meta.rs`) and `chunking_method` (universal, in `tei.rs`), Round 2 consolidated to the universal field. The GitHub-specific field was removed; the Qdrant index was updated to target `chunking_method`. This means all chunk types (crawl, ingest, embed-file) now carry the same field name.

**`pipeline.rs` not split:** At 228 lines with a single cohesive concern (concurrent point upsert loop), splitting payload construction out would have fragmented readability for no structural benefit. Left intact; removed from allowlist since it's under the 500-line limit.

**No new tree-sitter grammar crates:** Checked `Cargo.toml` and `Cargo.lock` — only the 7 already-wired grammars are available. Added TODO comments listing 8 candidates (Java, C, C++, C#, Ruby, Kotlin, Swift, Scala, TOML) for future addition.

**AST overlap implemented:** `text_splitter::ChunkConfig::with_overlap(200)` is supported. Added 200-char overlap to code chunks to match prose behavior. Fixes split function bodies losing signature context in the second chunk.

---

## Files Modified

| File | Action | Purpose |
|------|--------|---------|
| `crates/ingest/github/files.rs` | Modified + split | 511→364L; added size cap, batched flushing, line numbers |
| `crates/ingest/github/files/line_range.rs` | **Created** | `line_range_for_chunk()` + 3 tests (47L) |
| `crates/ingest/github/files/batch.rs` | **Created** | `collect_and_embed_batched()`, `flush_batch()`, consts (107L) |
| `crates/ingest/github/meta.rs` | Modified | Removed `gh_chunking_method` field (31→32 `gh_*` keys, net +1 for line numbers) |
| `crates/ingest/github.rs` | Modified | Expanded `is_indexable_source_path()` + `is_indexable_doc_path()` + excluded dirs |
| `crates/vector/ops/tei.rs` | Modified + split | 266→123L; extracted `code_embed.rs` and `text_embed.rs` |
| `crates/vector/ops/tei/code_embed.rs` | **Created** | `embed_code_with_metadata()` with `chunking_method` tracking (46L) |
| `crates/vector/ops/tei/text_embed.rs` | **Created** | Text/prepared-doc embed entry points (114L) |
| `crates/vector/ops/tei/pipeline.rs` | Modified | Removed `source_command` field from Qdrant payload |
| `crates/vector/ops/tei/qdrant_store.rs` | Modified | Added 3 keyword indexes; changed `gh_chunking_method` → `chunking_method` index |
| `crates/vector/ops/input/code.rs` | Modified | Added `with_overlap(200)` to `ChunkConfig`; added TODO comments for missing grammars |
| `.monolith-allowlist` | Modified | Removed stale `tei.rs` and `pipeline.rs` entries |

---

## Commands Executed

```bash
# Post-Round-1 verification
cargo check        # → Finished (clean)
cargo clippy       # → 0 warnings
cargo test --lib   # → 1295 passed, 0 failed

# Post-Round-2 verification
cargo check        # → Finished (clean)
cargo clippy       # → 0 warnings
cargo test --lib   # → 1295 passed, 0 failed

# Line count confirmation
wc -l crates/ingest/github/files.rs \
      crates/vector/ops/tei.rs \
      crates/vector/ops/tei/pipeline.rs
# → 364 / 123 / 228 (all under 500)
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| Large file safety | No limit — 50MB+ files loaded into memory | 5MB cap (`MAX_FILE_BYTES`); oversized files skipped with `log_warn` |
| Embed memory | All docs buffered before TEI submission | Flushed every 50 docs (`EMBED_BATCH_SIZE`) |
| Qdrant code chunk metadata | No `chunking_method` on web-crawled code | All points carry `chunking_method: "tree-sitter" \| "prose"` |
| Qdrant field indexes | `url`, `domain` only | +`source_type`, +`chunking_method`, +`gh_file_language` |
| Redundant field | `source_command` duplicated `source_type` on every point | `source_command` removed |
| Line numbers | No chunk-to-source linkage | `gh_line_start`, `gh_line_end` per chunk; URL fragment `#L{n}-L{n}` |
| AST chunk context | No overlap between chunks | 200-char overlap matches prose behavior |
| Extension support | 23 whitelisted extensions | +`.ipynb`, `.proto`, `.zig`, `.ex`, `.exs`, `.erl`, `.r`, `.R`, `.lua`, `.sql`, `.tf`, `.nix`, `.kts`, `.gradle`, `.adoc` |
| Excluded dirs | `target/`, `node_modules/`, `dist/` | +`build/`, `vendor/`, `.gradle/`, `.terraform/`, `.next/`, `.nuxt/`, `venv/`, `.venv/`, `env/`, `.pytest_cache/`, `.mypy_cache/`, `coverage/` |
| `tei.rs` size | 266 lines (in allowlist) | 123 lines; clean submodule split |
| `files.rs` size | 511 lines (over limit) | 364 lines; two submodules extracted |
| Monolith allowlist | 2 stale entries (no expiry) | Empty (0 entries) |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` (post Round 1) | Clean | Clean | ✅ |
| `cargo clippy` (post Round 1) | 0 warnings | 0 warnings | ✅ |
| `cargo test --lib` (post Round 1) | All pass | 1295 passed, 0 failed | ✅ |
| `cargo check` (post Round 2) | Clean | Clean | ✅ |
| `cargo clippy` (post Round 2) | 0 warnings | 0 warnings | ✅ |
| `cargo test --lib` (post Round 2) | All pass | 1295 passed, 0 failed | ✅ |
| `wc -l files.rs` | ≤500 | 364 | ✅ |
| `wc -l tei.rs` | ≤500 | 123 | ✅ |
| `.monolith-allowlist` entries | 0 | 0 | ✅ |

---

## Risks and Rollback

**Qdrant schema divergence:** Existing 2M+ points in the `cortex` collection do not have `chunking_method` or `gh_line_start`/`gh_line_end`. New points will have them. Queries filtering on these fields will only match new/re-embedded content. Not a bug — gradual population as content is re-ingested.

**`source_command` removal:** Existing points retain the field; new points omit it. Any query code that filters on `source_command` would silently miss new points. Confirmed via grep: no query/filter code references `source_command` — removal is safe.

**5MB file cap:** Legitimately large source files (generated code, embedded data) will be skipped. `log_warn` makes skips visible. Threshold is a constant (`MAX_FILE_BYTES` in `files/batch.rs`) — adjustable without API changes.

**Rollback:** All changes are on `main`. `git revert` on the relevant commits restores prior state. No schema migrations required — Qdrant collections are append/upsert-only.

---

## Decisions Not Taken

- **Adding new tree-sitter grammar crates:** No `tree-sitter-java`, `tree-sitter-c`, etc. in `Cargo.toml` or `Cargo.lock`. Adding speculatively risks compile failures and binary size bloat. TODO comments added in `code.rs` listing 8 candidates.
- **Splitting `pipeline.rs`:** At 228 lines with one cohesive concern, splitting would reduce clarity. Left intact; removed from allowlist.
- **True streaming to TEI:** Would require changing `embed_prepared_docs` signature, cascading through MCP/web handlers. Batched flushing (50 docs) achieves the memory-bounding goal without the API churn.
- **Thin-page filter on ingest:** Ingest sources (GitHub files, Reddit, YouTube) intentionally bypass the crawl-path 200-char thin-page filter. Source files like a 50-char `mod.rs` are legitimately short and should be indexed.

---

## Open Questions

- Should the 5MB file size cap be configurable via CLI flag (`--max-file-bytes`)? Currently hardcoded as `MAX_FILE_BYTES = 5 * 1024 * 1024`.
- Should `gh_line_start`/`gh_line_end` be Qdrant-indexed? Currently unindexed — range queries on line numbers would be full scans.
- When should tree-sitter grammar crates (Java, C, C++) be added? Requires Cargo.toml changes and binary size increase.
- Do existing Qdrant points need a re-embed sweep to backfill `chunking_method` and line number fields?

---

## Next Steps

1. Monitor `log_warn` output for skipped large files during real GitHub ingests to validate the 5MB threshold
2. Re-ingest key repos to populate `chunking_method`, `gh_line_start`, `gh_line_end` on existing chunks
3. Add tree-sitter grammar crates for Java, C, C++ when binary size budget permits
4. Consider `--max-file-bytes` CLI flag if the hardcoded 5MB cap proves too restrictive in practice
5. Run `just precommit` (monolith check + full verify) before next PR
