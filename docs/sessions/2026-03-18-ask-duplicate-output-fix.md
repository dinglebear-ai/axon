# Session: Ask Duplicate Output Fix + Simplify Review
**Date:** 2026-03-18
**Branch:** `feat/pulse-shell-and-hybrid-search`

## Session Overview

Investigated and fixed two issues with the `axon ask` command: (1) duplicate output where the LLM answer printed twice, and (2) poor retrieval quality for code-level queries against indexed GitHub repos. Also ran a `/simplify` review on a larger in-progress diff covering `paths.rs` extraction, `elapsed_ms()` reuse, `AcpPromptTurnRequest` Default derive, and other DRY refactors.

## Timeline

1. **User reported duplicate `axon ask` output** — answer and sources displayed twice in terminal
2. **Parallel investigation** — two agents explored (a) the duplicate output root cause and (b) the retrieval quality gap for GitHub repo queries
3. **Root cause identified** — two output paths both firing: streaming tokens to stdout in the service layer AND formatted reprint in the CLI handler
4. **Fix applied** — `stream_to_stdout = false` in `output.rs:15` to stop the service layer from printing tokens directly
5. **Retrieval quality analysis** — identified semantic mismatch, AST chunking boundaries, BM42 short-token filtering, and context limits as compounding factors (architectural, no code fix)
6. **`/simplify` review** — three parallel agents reviewed a larger diff for reuse, quality, and efficiency issues
7. **`help.rs` fix** — reverted an awkward refactoring that introduced a redundant `.filter()` chain

## Key Findings

- **Duplicate output root cause** (`crates/vector/ops/commands/ask/output.rs:15`): `stream_to_stdout = !cfg.json_output` caused the service layer to print LLM tokens to stdout during streaming. The CLI handler in `crates/cli/commands/ask.rs:41-43` then reprinted the collected answer in a formatted `Conversation` block. Both paths fired for non-JSON output.
- **Service layer contract violation**: The `crates/services/CLAUDE.md` contract states "Never print, log, or serialize inside the service function." The streaming-to-stdout behavior violated this.
- **`ask_llm_answer()` already returned `streamed_ok` boolean** (third tuple element) but `ask_payload()` at `crates/vector/ops/commands/ask.rs:35` discarded it with `_`.
- **Retrieval quality for code queries** is limited by: semantic distance between natural-language queries and route definitions, tree-sitter AST chunking that buries routes in large function chunks, BM42 filtering of tokens < 3 chars (drops "api"), and 10-chunk context limit after reranking.

## Technical Decisions

- **Set `stream_to_stdout = false` rather than conditionally skipping CLI reprint**: The service layer should never own presentation. This aligns with the services-first architecture contract. The CLI handler is the single output path.
- **Did not add streaming back to CLI handler**: While losing real-time token streaming is a UX regression for long responses, the fix is correct architecturally. Re-adding streaming requires the CLI to own the SSE stream directly, which is a larger refactor.
- **Reverted `help.rs` refactoring**: The `.map().unwrap_or("axon")` pattern inside an outer `.filter().unwrap_or()` created redundant option wrapping. Restored the original `.and_then().map()` chain which is more direct.

## Files Modified

| File | Change |
|------|--------|
| `crates/vector/ops/commands/ask/output.rs:15` | `stream_to_stdout = false` (was `!cfg.json_output`) |
| `crates/core/config/help.rs:37-47` | Reverted to original `.and_then().map()` chain |

## Behavior Changes (Before/After)

| Behavior | Before | After |
|----------|--------|-------|
| `axon ask "question"` (non-JSON) | Answer printed twice: once during LLM streaming, once in formatted `Conversation` block | Answer printed once in formatted `Conversation` block only |
| `axon ask "question" --json` | Single JSON output (no change) | Single JSON output (no change) |
| Real-time token streaming | Tokens appeared incrementally during LLM response | Full answer appears after LLM completes (UX regression, architecturally correct) |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | Clean compile | `Finished dev profile in 4.44s` | PASS |
| `cargo check 2>&1 \| grep output.rs` | No errors | 0 matches | PASS |

## Risks and Rollback

- **Risk**: Users accustomed to seeing streaming tokens appear incrementally will now see a pause followed by the full answer. For long LLM responses this could feel sluggish.
- **Rollback**: Revert `output.rs:15` to `let stream_to_stdout = !cfg.json_output;` and suppress the CLI reprint in `ask.rs:41-43` instead.
- **Mitigation**: To restore streaming without duplication, the CLI handler could own the SSE stream directly and print tokens as they arrive, then skip the post-stream formatted block.

## Decisions Not Taken

- **Did not fix retrieval quality for code queries**: This is an architectural limitation (semantic gap between NL queries and code patterns, AST chunking granularity, BM42 token filtering). Would require code-aware query expansion, specialized chunking for route definitions, and expanded sparse term vocabulary — a substantial feature effort.
- **Did not add streaming to CLI handler**: Would require `ask.rs` to consume the SSE stream directly rather than going through the service layer. Correct long-term fix but out of scope.
- **Did not cache `axon_data_dir()` env var reads**: Efficiency review confirmed all call sites are on cold paths (initialization, per-command boundaries). Caching unnecessary.

## Open Questions

- Should `axon ask` support a `--stream` flag to opt into real-time token display for interactive use?
- The pre-existing compile errors in `crates/jobs/ingest/process.rs` (9 errors from `PhaseReporter` refactor) are unrelated — are these being addressed on this branch?

## Next Steps

- Consider adding `--stream` flag to `axon ask` for interactive real-time token display
- Address `PhaseReporter` compile errors in `crates/jobs/ingest/process.rs` (separate concern)
- Retrieval quality for code-heavy repos could be improved with code-aware query expansion (future feature)
