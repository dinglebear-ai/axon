# Session: CLAUDE.md Audit and Improvements
Date: 2026-03-16
Branch: feat/pulse-shell-and-hybrid-search

## Session Overview

Ran the `claude-md-management:claude-md-improver` skill against the axon_rust repository. Discovered 12 existing CLAUDE.md files and one critical gap (`crates/services/` had no CLAUDE.md despite being the central architecture boundary). Produced a quality report, then applied three targeted updates: created the missing services CLAUDE.md, updated vector CLAUDE.md with hybrid search documentation, and updated root CLAUDE.md with links and hybrid search notes.

---

## Timeline

1. **Discovery** — Found 14 CLAUDE.md files (12 real; 2 in `node_modules`/`.next`, ignored)
2. **Assessment** — Read all 12 files, assessed each against quality criteria
3. **Gap identified** — `crates/services/` missing CLAUDE.md despite being referenced as architecture boundary in root
4. **Quality report** — Presented file-by-file scores (A/B/C grades) with specific issues
5. **User approved all three updates**
6. **Created** `crates/services/CLAUDE.md` (167 lines)
7. **Updated** `crates/vector/CLAUDE.md` — hybrid search section, module layout fix, VectorMode docs
8. **Updated** `CLAUDE.md` (root) — services crate link, hybrid search architecture note

---

## Key Findings

- `crates/services/CLAUDE.md` was entirely missing. This crate is the mandated contract boundary — all CLI/MCP/web routes call service functions, never raw vector/jobs ops directly. Score: F (0/100).
- Hybrid search (BM42 sparse + dense RRF fusion via `crates/vector/ops/sparse.rs` + `qdrant/hybrid.rs`) introduced in v0.25.0 commit (`7b173bf8`) was undocumented in any CLAUDE.md.
- `crates/vector/CLAUDE.md` module layout was missing `sparse.rs`, `qdrant/hybrid.rs`, and `tei/qdrant_store.rs` — all added in v0.25.0.
- `VectorMode` enum (`Named` vs `Unnamed`) is the mechanism for hybrid vs dense-only query routing — not documented anywhere.
- `crates/cli/CLAUDE.md` and `apps/web/CLAUDE.md` were the strongest files (both A, recently updated 2026-03-15).
- `crates/jobs/CLAUDE.md` is the oldest (2026-02-27) but still accurate for its domain.

---

## Technical Decisions

- **Services CLAUDE.md scope**: Covered architecture contract, typed result pattern, ServiceEvent channel with backpressure note, both ACP code paths (one-shot vs persistent-connection), session cache constants, testing commands, and add-new-service checklist. Excluded ACP mapping internals (too volatile).
- **Hybrid search docs in vector CLAUDE.md**: Included hash collision trade-offs from the `sparse.rs` module docstring — this is non-obvious and operationally important (15% collision rate at 100 terms, deliberate trade-off vs BERT tokenizer dependency).
- **Root CLAUDE.md**: Kept additions minimal (two lines) — a hybrid search summary and a link to the new services CLAUDE.md. The root file is already 582 lines; detail lives in the crate-level files.
- **Did not update `crates/jobs/CLAUDE.md`**: Reviewed and found it still accurate; no stale content, just older date. No changes warranted.

---

## Files Modified

| File | Type | Purpose |
|------|------|---------|
| `crates/services/CLAUDE.md` | Created | Documents services layer contract, ACP modes, ServiceEvent, result types |
| `crates/vector/CLAUDE.md` | Modified | Added hybrid search section, fixed module layout, VectorMode docs, sparse test command |
| `CLAUDE.md` | Modified | Added services crate link + hybrid search note in Architecture section |

---

## Commands Executed

```bash
# Discovery
find /home/jmagar/workspace/axon_rust -name "CLAUDE.md" | head -50
# → 14 files found (2 excluded as node_modules artifacts)

# Gap investigation
ls /home/jmagar/workspace/axon_rust/crates/
ls /home/jmagar/workspace/axon_rust/crates/services/
# → services/ exists with full module tree but no CLAUDE.md

# Hybrid search investigation
grep -r "hybrid" crates --include="*.rs" -l
# → crates/vector/ops/sparse.rs, crates/vector/ops/qdrant.rs, tei/qdrant_store.rs, query.rs, retrieval.rs

grep -n "hybrid\|sparse\|VectorMode" crates/vector/ops/qdrant.rs
# → confirmed hybrid.rs module and qdrant_hybrid_search/qdrant_named_dense_search exports

grep -n "VectorMode" crates/vector/ops/tei/qdrant_store.rs | head -30
# → Named/Unnamed enum, OnceLock cache, ensure_collection() logic confirmed

git log --oneline -10
# → 7b173bf8 feat(web,vector): Pulse shell redesign, AI elements, hybrid search (v0.25.0)
```

---

## Behavior Changes (Before/After)

| Surface | Before | After |
|---------|--------|-------|
| `crates/services/` | No CLAUDE.md — agents had no guidance on the contract boundary | Full CLAUDE.md with contract, patterns, ACP modes, testing |
| `crates/vector/` | No hybrid search docs; module layout missing sparse.rs/hybrid.rs/qdrant_store.rs | Hybrid search section, correct module layout, VectorMode docs |
| Root CLAUDE.md Architecture | No mention of hybrid search or services CLAUDE.md | Brief hybrid note + link to services CLAUDE.md |

---

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| `crates/services/CLAUDE.md` exists | File created | 167 lines, written successfully | ✓ Pass |
| `crates/vector/CLAUDE.md` has hybrid section | Section added | "### Hybrid Search (Dense + Sparse BM42)" present | ✓ Pass |
| Root CLAUDE.md links services | Link added | Line 202: `crates/services/CLAUDE.md` | ✓ Pass |
| `crates/vector/CLAUDE.md` module layout includes sparse.rs | sparse.rs listed | Present in layout | ✓ Pass |
| No node_modules CLAUDE.md files included in assessment | Excluded | 2 path-dep files (react-lite-youtube-embed, .next/standalone) skipped | ✓ Pass |

---

## Source IDs + Collections Touched

*Axon embed performed after session write — see embed status below.*

---

## Risks and Rollback

- **Risk**: Services CLAUDE.md content may drift as the ACP/services crates evolve (new session_cache constants, new service functions). **Mitigation**: Date-stamped `Last Modified` field; run improver periodically.
- **Rollback**: All three files are git-tracked. `git checkout HEAD -- crates/services/CLAUDE.md crates/vector/CLAUDE.md CLAUDE.md` reverts all changes.
- No code changes were made; documentation only. Zero runtime risk.

---

## Decisions Not Taken

- **Did not update `crates/jobs/CLAUDE.md`**: File is accurate, just older. No stale content found to correct.
- **Did not add hybrid search to root CLAUDE.md Gotchas**: Gotchas are for operational traps; hybrid search is an architecture feature, better placed in vector CLAUDE.md.
- **Did not document `VectorMode` cache invalidation** in services CLAUDE.md: The cache is process-scoped and never invalidated at runtime — not a gotcha, just architecture. Documented in vector CLAUDE.md instead.
- **Did not create `crates/acp/CLAUDE.md`**: ACP lives entirely within `crates/services/acp/` and is covered by the new services CLAUDE.md. A separate file would be premature.

---

## Open Questions

- `crates/jobs/CLAUDE.md` doesn't mention `watch.rs` or `events.rs` (now in `crates/services/`). These may have been moved in a refactor — worth confirming whether jobs CLAUDE.md needs a note that these moved out.
- `hybrid_search_candidates` Config field default value — the code says `cfg.hybrid_search_candidates.max(limit)` but the actual default value in `Config::default()` was not confirmed. The vector CLAUDE.md notes it as "at least `limit`" which is accurate from the code but the numeric default is not documented.

---

## Next Steps

- Run `just verify` to confirm no test/lint regressions from doc-only changes (no Rust code changed, so this is a formality)
- Consider running the improver again after the next major feature to keep CLAUDE.md files current
- Follow up on the `hybrid_search_candidates` default value — document the actual number in vector CLAUDE.md when confirmed
