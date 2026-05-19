# Session: Stats Bug Fixes + Suggest Performance Overhaul

**Date:** 2026-02-19
**Branch:** `perf/command-performance-fixes`
**Duration:** Single session

---

## Session Overview

Two parallel tracks:

1. **`axon stats` debugging** — four distinct bugs causing null values, missing command counts, and missing color were identified via systematic root-cause analysis and fixed.
2. **`axon suggest` performance** — a thorough code review identified critical O(N) bottlenecks; all were fixed, reducing Qdrant traffic by ~66× and replacing a 151K-iteration CPU loop with a single HTTP call.

---

## Timeline

| Phase | Activity |
|-------|----------|
| 1 | Read `stats.rs` and `status.rs`; fetched live Qdrant API response to confirm field names |
| 2 | Queried Postgres DB directly to reproduce nulls; confirmed `SUM(bigint)→numeric` type mismatch |
| 3 | Fixed all four stats bugs; verified live `axon stats` output |
| 4 | Added `suggest` and `evaluate` to Command Counts (user request) |
| 5 | Read all `suggest` dependencies: `suggest.rs`, `client.rs`, `input.rs`, `utils.rs`, `commands.rs` |
| 6 | Wrote thorough performance review identifying 7 issues |
| 7 | Implemented all performance fixes; verified clippy + tests |

---

## Key Findings

### Stats Bugs

1. **`Vectors: null`** — Qdrant 1.13.1 dropped `vectors_count` from the collection info API response. Code read `info["result"]["vectors_count"]` directly; field absent → `null`. "Indexed Vectors" already used a correct fallback. (`stats.rs:97`)

2. **`Total Chunks: null` / `Total Docs: null`** — PostgreSQL `SUM(bigint)` returns `numeric`, not `bigint`. `sqlx::query_scalar::<_, Option<i64>>` fails to decode `numeric` → error swallowed by `.ok()` → `None`. Confirmed: `SELECT pg_typeof(SUM(...))` returned `numeric`; direct psql query returned 223,200. (`stats.rs:352-357`)

3. **Missing `Embeds` count** — `embed_count` field never existed in `PostgresMetrics` struct. (`stats.rs:45-65`)

4. **No blue/accent color** — `accent()` function never imported or called in `stats.rs`. All numeric values printed as unstyled plain text.

### Suggest Performance

5. **Full 2.5M-point scroll** — `qdrant_indexed_urls` used `qdrant_scroll_pages` (page size 256, no filter) fetching all points including duplicate chunks. With 2.5M points at 256/page: **~9,966 sequential HTTP requests**. (`client.rs:62-80`)

6. **151K `Url::parse()` calls** — `base_url_counts` loop called `qdrant::base_url()` (which calls `Url::parse()`) for every indexed URL. `qdrant_domain_facets()` — a single API call — already existed in the codebase and returns the same data. (`suggest.rs:136-143`)

7. **~600K allocations in `indexed_lookup` build** — `url_lookup_candidates` called for all 151K indexed URLs, each running `normalize_url()` + creating 4 String variants with an internal per-call `HashSet`. (`suggest.rs:119-124`)

8. **Sort 151K strings to use 500** — `indexed_urls.sort()` on 151K entries before truncating to 500 for the LLM prompt. (`client.rs:77-78`)

9. **Duplicate `env_usize_clamped`** — identical function defined in both `suggest.rs` and `qdrant/utils.rs`. (`suggest.rs:10-17`)

---

## Technical Decisions

### `qdrant_scroll_pages` not modified
Three callers exist (`qdrant_indexed_urls`, `run_sources_native`, `run_domains_native`). Changing the signature would force updates to all three. Since only `qdrant_indexed_urls` needed the optimization, it was rewritten with its own loop rather than making `qdrant_scroll_pages` generic. Avoids accidental behavior changes in `sources` and `domains` commands.

### Use `qdrant_domain_facets` for `suggest` domain counts
The facets API returns domain→count in a single HTTP call. The previous approach computed the same data by iterating all 151K URLs and calling `Url::parse()` per URL. The only difference: facets return `hostname` (e.g. `docs.rust-lang.org`) while the old code built `https://hostname`. The LLM prompt label was updated to "INDEXED_BASE_URLS_WITH_PAGE_COUNTS" — the LLM handles bare hostnames equivalently.

### Concurrent Qdrant calls in `build_suggest_prompt_context`
`qdrant_indexed_urls` and `qdrant_domain_facets` are now launched with `spider::tokio::try_join!`. Both are read-only Qdrant requests with no dependency between them. Consistent with existing `try_join!` patterns in `status.rs`.

### Simplified `indexed_lookup` construction
Stored URLs are already normalized (went through `normalize_url` when indexed). Calling `normalize_url` again on retrieval is redundant. Replaced with a direct 2-variant insert (with/without trailing slash), eliminating the per-call `HashSet` and `normalize_url`.

### Removed the `./sort()` from `qdrant_indexed_urls`
Sort was only used to make `existing_url_context` deterministic for the LLM. The LLM doesn't need sorted URLs; removing the sort is O(N) saved for free.

---

## Files Modified

| File | Purpose |
|------|---------|
| `crates/vector/ops/stats.rs` | Fixed 4 stats bugs; added `evaluate`/`suggest`/`embed` counts; added `accent()` color to all values |
| `crates/vector/ops/qdrant/client.rs` | Rewrote `qdrant_indexed_urls` with `chunk_index=0` filter, url-only payload, page size 1000; removed unused `payload_url` import |
| `crates/vector/ops/qdrant/mod.rs` | Re-exported `qdrant_domain_facets` and `env_usize_clamped` as `pub(crate)` |
| `crates/vector/ops/commands/suggest.rs` | Removed duplicate `env_usize_clamped`; concurrent Qdrant calls; `qdrant_domain_facets` for domain counts; simplified `indexed_lookup`; removed `HashMap`/`std::env` imports |

---

## Commands Executed

```bash
# Confirmed Qdrant API response shape (no vectors_count field)
curl -s http://127.0.0.1:53333/collections/cortex | python3 -m json.tool

# Confirmed SUM(bigint) returns numeric in PostgreSQL
docker exec axon-postgres psql -U axon -d axon \
  -c "SELECT pg_typeof(SUM(COALESCE((result_json->>'chunks_embedded')::bigint, 0))) FROM axon_embed_jobs WHERE status='completed';"
# → numeric

# Confirmed data is actually there (not truly null)
docker exec axon-postgres psql -U axon -d axon \
  -c "SELECT SUM(COALESCE((result_json->>'chunks_embedded')::bigint, 0)) FROM axon_embed_jobs WHERE status='completed';"
# → 223200

# Confirmed all command_runs entries
docker exec axon-postgres psql -U axon -d axon \
  -c "SELECT command, COUNT(*) FROM axon_command_runs GROUP BY command ORDER BY COUNT(*) DESC;"
# crawl(168), status(92), embed(73), retrieve(25), ask(9), evaluate(7), ...

# Verified fixes
./scripts/axon stats
cargo clippy --bin axon
cargo test suggest
```

---

## Behavior Changes (Before/After)

### `axon stats` output

| Field | Before | After |
|-------|--------|-------|
| `Vectors:` | `null` (field removed from Qdrant API) | Row removed entirely |
| `Total Chunks:` | `null` (type mismatch) | `223210` |
| `Total Docs:` | `null` (type mismatch) | `57238` |
| Command Counts | Missing Embeds, Evaluates, Suggests | `Embeds: 237`, `Evaluates: 7`, `Suggests: 1` |
| All numeric values | Unstyled plain text | Colored with `accent()` (lavender/blue) |
| Usage note | Stale "tracked from this release onward" text | Removed |

### `axon suggest` performance

| Metric | Before | After |
|--------|--------|-------|
| Qdrant HTTP requests (URL fetch) | ~9,966 (256/page, all 2.5M points) | ~152 (1000/page, 152K chunk_index=0 points) |
| Payload size per page | All 3 fields (url, domain, source_command) | url only |
| Domain count computation | 151K × `Url::parse()` + HashMap loop | 1 HTTP request (`/facet`) |
| Concurrency | Sequential: URLs then domains | Concurrent: `try_join!` |
| `indexed_lookup` allocations | ~600K (4 variants × `normalize_url` per URL) | ~304K (2 variants, no normalize) |
| Sort | `O(N log N)` on 151K strings | Removed |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `./scripts/axon stats` | No nulls, Embeds/Evaluates/Suggests visible | `Total Chunks: 223210`, `Embeds: 237`, `Evaluates: 7`, `Suggests: 1` | ✅ |
| `cargo clippy --bin axon` | 0 warnings | 0 warnings | ✅ |
| `cargo test suggest` | 3/3 passing | `3 passed; 0 failed` | ✅ |
| `cargo check --bin axon` | Compiles clean | `Finished` with no errors | ✅ |
| Engine tests pre/post | Pre-existing failures unchanged | Same 2 failures on base commit confirmed | ✅ |

---

## Risks and Rollback

**Stats fixes are low risk** — SQL cast changes are additive (`::bigint` is a safe no-op when the SUM already fits in bigint). Removing the `Vectors:` row is display-only with no downstream effect.

**Suggest performance fix risk** — The `chunk_index=0` filter assumes every indexed document has a `chunk_index=0` point. If a document was upserted with chunks starting at index 1 (non-standard), it would be invisible to `qdrant_indexed_urls` and excluded from suggest filtering. This matches existing behavior (the field was already used for Docs estimate in stats). The `qdrant_domain_facets` path was already used in the `domains` command with no issues.

**Rollback:** `git checkout crates/vector/ops/` reverts all four files.

---

## Decisions Not Taken

- **Generalize `qdrant_scroll_pages`** to accept filter/payload params — rejected because it would touch all three callers (`qdrant_indexed_urls`, `run_sources_native`, `run_domains_native`) for a change only needed by one. Inlining is simpler and more explicit.
- **Increase page size in `qdrant_scroll_pages` globally** — rejected to avoid unintended memory impact on `sources` and `domains` commands which build in-memory maps over the full collection.
- **Remove `Vectors:` row vs. repurpose it** — could have shown `indexed_vectors_count` again (already shown on the next line). Removing is cleaner; the duplication added no value.
- **Keep `indexed_urls.sort()`** — the LLM prompt doesn't need sorted URL context. Rejected retaining it.
- **Use `chunk_index=0` filter in `qdrant_scroll_pages` for `run_sources_native`** — `sources` command correctly counts per-URL chunk totals; it needs all chunks, not just first ones.

---

## Open Questions

- The 2 pre-existing `engine/tests.rs` failures (`test_no_fallback_uncapped_*`) in `should_fallback_to_chrome` — root cause not investigated this session. Likely related to an earlier change in the branch.
- `Total Docs: 57,238` vs `Docs (est): 151,945` — the embed job `docs_embedded` field tracks only async-pipeline embeds; inline embeds during crawl don't contribute. Is this gap expected? Worth documenting in CLAUDE.md or the stats help text.
- `Base URLs: 121` counts distinct seed URLs in `axon_crawl_jobs` (not unique domains or all indexed URLs). Consider renaming to "Seed URLs" for clarity.

---

## Next Steps

- Investigate and fix the 2 pre-existing `should_fallback_to_chrome` test failures
- Consider adding `--explain` output to `axon stats` showing what each metric measures
- Consider whether `sources` command could also benefit from `chunk_index=0` filter when only unique-URL listing is needed (it currently counts chunks per URL, so all points needed)
