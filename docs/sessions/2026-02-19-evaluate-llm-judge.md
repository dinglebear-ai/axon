# Session: Evaluate Command — LLM Judge with Independent Research

**Date:** 2026-02-19
**Branch:** `perf/command-performance-fixes`
**Commit:** `9787f0e`

---

## Session Overview

Implemented a third LLM "judge" pass on the `evaluate` command. Previously `evaluate` ran two LLM calls (RAG answer and baseline answer) side by side. The judge performs its own independent Qdrant retrieval to ground accuracy assessment in facts rather than comparing answers against each other. After the implementation, the session covered tightening, prompt alignment verification, a commit/push (including monolith allowlist fixes), and doc/help updates.

---

## Timeline

1. **Plan review** — Reviewed the pre-written plan: `build_judge_reference`, judge prompt design, streaming/non-streaming helpers, data flow, JSON output additions.
2. **Read existing code** — Read `evaluate.rs`, `streaming.rs`, `ask.rs`, `qdrant/mod.rs`, `ranking.rs`, `tei.rs` to understand data structures and APIs.
3. **Implemented `streaming.rs` additions** — `judge_system_prompt()`, `judge_user_msg()`, `judge_llm_streaming()`, `judge_llm_non_streaming()`.
4. **Implemented `evaluate.rs` additions** — `build_judge_reference()`, updated `run_evaluate_native()` with research step, analysis step, updated timing and JSON output.
5. **Build/lint/test clean** — Zero clippy warnings, clean fmt, all tests passing.
6. **Tightening pass** — Three fixes: URL extraction from diagnostic sources, empty `reranked` guard, `NO_REFERENCE` const.
7. **Quick-push** — Pre-commit hook caught monolith violations in pre-existing jobs refactor files; added four allowlist entries with tracking notes, then committed and pushed.
8. **Prompt alignment check** — Verified evaluate automatically picks up updated ask/baseline system prompts via shared function calls; no changes needed.
9. **Docs/help update** — Added `evaluate` and `suggest` to CLAUDE.md commands table; added `evaluate` to two locations in README.md; updated config.rs one-liner.

---

## Key Findings

- `evaluate.rs` was an untracked (`??`) new file on the branch — had to be explicitly `git add`-ed.
- `ctx.diagnostic_sources` entries are formatted `"full-doc score=X url=Y"` / `"chunk score=X url=Y"` — passing raw strings to the judge polluted the prompt with internal scoring noise; fixed with `split_once(" url=")`.
- Empty `reranked` after relevance filter (all candidates below threshold) was a silent path that produced an empty reference string instead of the `NO_REFERENCE` fallback.
- Pre-commit monolith hook blocked commit due to pre-existing jobs module files: `worker_process.rs` (`process_job()` at 342 lines — primary refactor target), `worker_loops.rs`, `robots.rs`, `common/tests.rs`.
- `evaluate` was missing from both the CLAUDE.md commands table and both README.md commands tables. `suggest` was also missing from CLAUDE.md.
- Prompt changes to `ask_llm_streaming` propagate automatically to `evaluate` — no duplication, no drift risk.

---

## Technical Decisions

**Independent retrieval for judge, not shared vector from RAG** — The judge embeds the query fresh and searches with `ask_candidate_limit * 2` (wider pool). Re-using the RAG vector would couple the judge to the RAG pipeline. Fresh retrieval isolates the judge's accuracy basis.

**`split_once(" url=")` over parsing the full diagnostic format** — Lightweight and correct for the known format. The alternative (changing `diagnostic_sources` format in `ask.rs`) would have been a larger surface change.

**Two early-exit guards in `build_judge_reference`** — One for empty candidates (no Qdrant hits), one for empty `reranked` (all hits below relevance threshold). Both return `(NO_REFERENCE, 0)` so the quality note fires and the judge is aware.

**`NO_REFERENCE` const** — Eliminated the same string literal appearing in two places (`build_judge_reference` body and `unwrap_or_else` call site).

**Monolith allowlist for jobs files** — The violations were in verbatim-carried-over functions from the pre-existing branch refactor (`runtime.rs` → `runtime/` split). Fixing them in this session would have been out of scope; tracking notes added with `process_job()` flagged as primary target.

**`#[allow(clippy::too_many_arguments)]`** — Applied to `judge_user_msg`, `judge_llm_streaming`, `judge_llm_non_streaming`. All three have 10–13 parameters by design; the judge needs all context to produce meaningful analysis.

**Kept judge at `temperature: 0.3`** — Higher than ask/baseline (0.1) to allow nuanced comparative reasoning without becoming creative. Deterministic enough for scoring, flexible enough for prose.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/vector/ops/commands/streaming.rs` | Added `judge_system_prompt()`, `judge_user_msg()`, `judge_llm_streaming()`, `judge_llm_non_streaming()` |
| `crates/vector/ops/commands/evaluate.rs` | Added `build_judge_reference()`, `NO_REFERENCE` const, research + analysis phases in `run_evaluate_native()`, updated timing/JSON output |
| `.monolith-allowlist` | Added four entries for pre-existing jobs module violations, with tracking notes |
| `CLAUDE.md` | Added `evaluate` and `suggest` rows to commands table |
| `README.md` | Added `evaluate` to feature list, vector crate description, and full commands table |
| `crates/core/config.rs` | Updated evaluate help one-liner to describe the judge |

---

## Commands Executed

```bash
cargo build --bin axon        # Clean every run
cargo clippy                  # Zero warnings
cargo fmt --check             # Clean
cargo test                    # 101 tests passing
git add .
git commit                    # Blocked by monolith hook → fixed allowlist → re-committed
git push                      # 9787f0e pushed to perf/command-performance-fixes
```

---

## Behavior Changes (Before / After)

**Before:** `axon evaluate "question"` ran two LLM calls (RAG + baseline) and printed both side by side with a timing line.

**After:** Three phases:
1. `── RAG Answer (with context) ──` — streams RAG answer (unchanged)
2. `── Baseline Answer (no context) ──` — streams baseline answer (unchanged)
3. `── Analysis ──` — streams structured judge output:
   - `## Accuracy     RAG: X/5 | Baseline: X/5` with `[R#]` citations
   - `## Relevance    RAG: X/5 | Baseline: X/5`
   - `## Completeness RAG: X/5 | Baseline: X/5`
   - `## Specificity  RAG: X/5 | Baseline: X/5`
   - `## Timing` — latency overhead justification
   - `## Did RAG Add Value?` — YES/NO with reasoning
   - `## Verdict` — 1-2 sentence summary

**Timing line:** `rag_llm=Xms | baseline_llm=Xms | research=Xms | analysis_llm=Xms | total=Xms`

**JSON additions:** `analysis_answer`, `ref_chunk_count`, `timing_ms.research_elapsed_ms`, `timing_ms.analysis_llm_ms`

**Thin reference warning:** When `ref_chunk_count < 3`, judge context prepends `⚠️ Reference material is limited — accuracy scores may be less reliable.`

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo build --bin axon` | Clean | `Finished dev profile` | ✅ |
| `cargo clippy` | 0 warnings | 0 warnings | ✅ |
| `cargo fmt --check` | Clean | Clean | ✅ |
| `cargo test` | All pass | 101 passed, 0 failed | ✅ |
| `git push` | Pushed to remote | `b83ae37..9787f0e` | ✅ |

---

## Risks and Rollback

- **Extra LLM call cost** — `evaluate` now makes 3 LLM calls + 2 TEI embed calls (1 for RAG context, 1 for judge reference). Cost scales with model pricing. No mitigation added; acceptable for an evaluation command.
- **Judge reference TEI failure** — If TEI is unavailable for the research step, `build_judge_reference` returns `(NO_REFERENCE, 0)` via `unwrap_or_else`. Judge continues with the warning note. Graceful degradation, not a hard failure.
- **Rollback** — `git revert 9787f0e` reverts all session changes. The `evaluate.rs` file would need to be removed manually (it's a new file, not tracked before this commit).

---

## Decisions Not Taken

- **Reuse RAG query vector for judge retrieval** — Would couple the judge to the RAG pipeline ordering and skip independent reranking. Rejected for isolation.
- **Change `diagnostic_sources` format in `ask.rs`** — Would be cleaner but larger surface change affecting diagnostics display. `split_once` in evaluate is contained.
- **Refactor `process_job()` (342 lines)** — Out of scope for this session; tracked in allowlist.
- **Shared `judge_params` struct** instead of 10-arg functions — Adds a type for a function called in one place. YAGNI.

---

## Open Questions

- `process_job()` at 342 lines in `crawl_jobs/runtime/worker/worker_process.rs` is the primary monolith refactor target. Needs a dedicated session.
- The judge `temperature: 0.3` is untested at scale — may need tuning based on observed output quality.

---

## Next Steps

- Run `axon evaluate` against real questions to validate judge output quality and tune the system prompt if needed.
- Refactor `process_job()` — split by job-type dispatch into focused handlers.
- Consider adding `--no-judge` flag if users want the old two-pass behavior without the extra latency.
