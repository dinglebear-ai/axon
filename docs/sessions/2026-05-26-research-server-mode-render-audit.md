---
date: 2026-05-26 23:20:09 EST
repo: git@github.com:jmagar/axon.git
branch: feat/openai-compat-palette-polish
head: 358d00cc
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust [feat/openai-compat-palette-polish]
pr: "#139 feat: add OpenAI-compatible backend and palette polish (https://github.com/jmagar/axon/pull/139)"
---

# Research server-mode render audit and payload-unwrap fix

## User Request

Running `axon research "what causes zsh segfaults?"` failed with:
```
Error: Error("missing field `query`", line: 0, column: 0)
```

Diagnose and fix the error.

## Session Overview

Traced a serde deserialization mismatch in the server-mode render path for the `research` command. The `/v1/research` endpoint returns `Json<ResearchResult>` (a wrapper with a `payload` field), but `render_research` in `src/cli/server_mode/render.rs` was attempting to deserialize the raw response body directly as `ResearchPayload` (the inner type). Applied the same `.get("payload").unwrap_or(result)` unwrap pattern used by the preceding `render_doctor` fix. Then audited every other render function in the file and confirmed no further mismatches.

## Sequence of Events

1. **Reproduced the error.** User ran `axon research "what causes zsh segfaults?"` against a running `axon serve` instance (AXON_SERVER_URL set), triggering the server-mode dispatch path.
2. **Traced the dispatch chain.** `Research` is in `is_server_mode_rest_command()` → routed to `/v1/research` → response passed to `render_research`.
3. **Identified the bug.** `render_research` at `render.rs:401` did `serde_json::from_value(result.clone())?` expecting `ResearchPayload` fields at the top level, but the server returns `{"payload": {...}}` (a `ResearchResult` envelope).
4. **Located the precedent.** Commit `b85c7b58` applied the identical fix pattern (`result.get("payload").unwrap_or(result)`) to `render_doctor` for the same class of bug.
5. **Applied the fix.** Added the `inner` unwrap line to `render_research`, making it consistent with `render_stats` and `render_doctor`.
6. **Verified compilation.** `cargo check` passed with no errors or warnings.
7. **Audited all other renderers.** Dispatched a subagent to cross-reference every render function against the HTTP handler return types. Confirmed that `StatsResult`, `DoctorResult`, `StatusResult`, and `ResearchResult` are the only types with a `payload` wrapper; all four are now handled correctly.

## Key Findings

- `src/cli/server_mode/render.rs:400-404` — `render_research` was deserializing the full `ResearchResult` envelope as `ResearchPayload`, missing the `payload` indirection.
- `src/web/server/handlers/exploration.rs:201-214` — `/v1/research` returns `Json<services::types::ResearchResult>`, which serializes as `{"payload": {"query": "...", ...}}`.
- `src/services/types/service.rs:952-968` — `ResearchResult` wraps `ResearchPayload` in a `payload` field; `ResearchPayload` has `query`, `limit`, `offset`, `search_results`, `extractions`, `summary`, `summary_source`, `usage`, `timing_ms`.
- Four result types use the `payload` wrapper pattern: `StatsResult` (line 101), `DoctorResult` (line 106), `StatusResult` (line 125), `ResearchResult` (line 965). All four now have correct unwrap logic in their renderers.
- All other result types (`ScrapeResult`, `SummarizeResult`, `MapResult`, `QueryResult`, `RetrieveResult`, `AskResult`, `EvaluateResult`, `SuggestResult`) are flat structs — their renderers correctly deserialize `result` directly.
- `ScrapeResult` has a field named `payload` (line 802) but it holds extractor metadata, not a wrapper envelope; the render correctly deserializes the full `ScrapeResult`.

## Technical Decisions

- **Chose `.get("payload").unwrap_or(result)` over `from_value::<ResearchResult>` + `.payload`.** The `unwrap_or(result)` fallback keeps the renderer tolerant when `result` is already a bare `ResearchPayload` (e.g., from a test fixture or a future wire-format change), whereas deserializing as `ResearchResult` would hard-fail on the bare form.
- **Kept the fix minimal.** One `let inner` binding, no other changes to the function or its callers. Consistent with the identical pattern in `render_stats` and `render_doctor`.
- **Did not add a `#[serde(flatten)]` or envelope-stripping layer at the HTTP boundary.** The server-side shape is the stable JSON contract consumed by web clients; changing it would be a broader breaking change.

## Files Changed

| Status | Path | Purpose | Evidence |
|--------|------|---------|----------|
| modified | `src/cli/server_mode/render.rs` | Add `inner` unwrap for `render_research` to handle `ResearchResult` envelope | `Edit` tool applied; `cargo check` passed |

## Beads Activity

No bead activity observed. The fix was a targeted bug repair surfaced by a runtime error, not tracked as a pre-existing open issue.

## Repository Maintenance

### Plans
Reviewed `docs/plans/` (excluding `complete/`). Thirteen active plan files remain. None are demonstrably complete based on this session's scope. No plan files moved.

### Beads
`bd list --status=open` shows 13 open issues. None are directly related to this server-mode render bug. No bead state changed.

### Worktrees and Branches
Three worktrees registered:
- `/home/jmagar/workspace/axon_rust` (current, `feat/openai-compat-palette-polish`, 27 commits ahead of origin — not yet pushed)
- `/home/jmagar/workspace/axon_rust/.worktrees/axon-android-app` (`feat/axon-android-app`, clean)
- `/home/jmagar/workspace/axon_rust/.worktrees/palette-streamdown-streaming` (`work/palette-streamdown-streaming`, at origin parity)

No worktrees or branches removed. All three are active or unmerged.

### Stale Docs
No documentation contradicted or made stale by this session's changes. The fix is a single-line render correction with no impact on public API docs, CLAUDE.md, or architecture guides.

### Transparency
- **Dirty files at session end (not related to this session's change):** `apps/palette-tauri/src/lib/axonClient.test.ts`, `apps/palette-tauri/src/lib/axonClient.ts`, `src/jobs/config_snapshot.rs` — pre-existing from the `feat/openai-compat-palette-polish` branch work.
- **Untracked:** `.broadcastr/` — not repo-relevant, not staged.
- **Branch not pushed.** The branch is 27 commits ahead of `origin/feat/openai-compat-palette-polish`. Pushing the full branch is out of scope for this session; only the session log is pushed.

## Tools and Skills Used

- **File tools (Read, Grep, Glob, Edit, Write):** Traced the bug through source files and applied the fix. No issues encountered.
- **Bash:** `cargo check`, `git diff`, `git log`, `git show`, `git status`, `bd list`. All commands succeeded.
- **Agent (Explore subagent):** Dispatched to cross-reference all render functions against HTTP handler return types. Returned accurate analysis with no mismatches beyond the one already fixed.
- **save-to-md skill:** Invoked to produce this session artifact.

## Commands Executed

| Command | Result |
|---------|--------|
| `rtk cargo check` | Clean compile — `Finished dev profile in 26.67s` |
| `rtk git diff src/cli/server_mode/render.rs` | Empty (fix matches HEAD) |
| `rtk git show HEAD:src/cli/server_mode/render.rs` | Confirmed fixed version in HEAD |
| `bd list --status=open` | 13 open issues, none related to this fix |
| `rtk git log --oneline -15` | Identified preceding `b85c7b58 fix(doctor): unwrap server-mode payload` |

## Errors Encountered

| Error | Root Cause | Resolution |
|-------|-----------|------------|
| `Error("missing field 'query'", line: 0, column: 0)` | `render_research` deserialized `ResearchResult` envelope (`{"payload":{...}}`) as `ResearchPayload` directly, finding no `query` at the top level | Added `let inner = result.get("payload").unwrap_or(result);` before the `from_value` call |

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| `axon research <query>` (server mode) | Failed with serde deserialization error — `missing field 'query'` | Correctly renders the human-readable research summary from the server response |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `rtk cargo check` | Clean compile | `Finished dev profile in 26.67s` | pass |
| `git show HEAD:render.rs` shows `inner = result.get("payload")` | Fixed version in HEAD | Fixed version confirmed | pass |

## Risks and Rollback

Low risk — the `unwrap_or(result)` fallback is additive and tolerant. If `payload` is absent the renderer falls through to the original behaviour; if `payload` is present it unwraps correctly. Rollback by reverting the one-line addition in `render_research`.

## Decisions Not Taken

- **Changing the server response envelope.** The `/v1/research` handler returning `Json<ResearchResult>` is the stable wire contract consumed by web clients and MCP callers. Stripping the envelope server-side would silently break all non-CLI consumers.
- **Centralising envelope-unwrapping in `render_server_result`.** Each command has different wrapping conventions; a blanket `get("payload")` at the dispatch level would silently succeed for commands whose result has no such field while breaking those whose payload field is semantically meaningful (e.g. `ScrapeResult.payload` is extractor metadata).

## Open Questions

- The branch is 27 commits ahead of `origin/feat/openai-compat-palette-polish`. Several of these are actively dirty (palette, config_snapshot). When are these changes intended to be pushed as a PR update?

## Next Steps

1. **Push the branch.** `git push` to update `origin/feat/openai-compat-palette-polish` with all 27 pending commits including this render fix.
2. **Commit dirty files** (palette/axonClient, config_snapshot) if they are ready — or stash if they are work-in-progress.
3. **Verify `axon research` end-to-end** with a live `axon serve` instance after the binary is rebuilt from HEAD.
4. **Consider a regression test** in `src/cli/server_mode/` that asserts `render_research` tolerates the `{"payload": {...}}` envelope shape — the class of bug recurred for doctor and research; a unit test would prevent a third occurrence.
