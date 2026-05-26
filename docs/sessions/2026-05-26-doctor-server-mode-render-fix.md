---
date: 2026-05-26 01:03:22 EST
repo: git@github.com:jmagar/axon.git
branch: main
head: f3a1b1c3f9bb0b7fe4ecba43ab91bf2e1e17a9a8
session id: 29c4e0eb-2900-4adc-822e-64c5cb080a77
working directory: /home/jmagar/workspace/axon_rust
---

# axon doctor server-mode render fix

## User Request

`axon doctor` displayed all services as failed (`✗ tei failed unreachable`, `✗ qdrant failed n/a`) despite the server being healthy. The user noted this had not been fixed yet and suspected URL normalization was the cause.

## Session Overview

Diagnosed and fixed a rendering bug in server-mode doctor output. When `AXON_SERVER_URL` is set, the CLI routes `axon doctor` to `/v1/doctor` on the running server, which returns `Json<DoctorResult>` — a `{"payload": {...}}` wrapper. The `render_doctor` function was passing this wrapper directly to `render_doctor_report_human`, which then read `report["services"]["tei"]["ok"]` as null (the real data is at `payload["services"]`). The fix was a one-line pattern already used by `render_stats`. A pre-existing clippy error (`prepared::MAX_PREPARED_SESSION_DOCS` — unnecessary qualification) in `sessions_tests.rs` also blocked the push and was resolved in the same session.

## Sequence of Events

1. **User reported doctor failures.** `axon doctor` showed all services failed. User believed URL normalization was broken.
2. **Compared `--json` vs human output.** `axon doctor --json` showed all services healthy; human output showed failures. Same data, different rendering path — confirming the bug was in the renderer.
3. **Identified `AXON_SERVER_URL` routing.** The environment had `AXON_SERVER_URL=http://127.0.0.1:8001` set, causing `axon doctor` to route via `run_server_mode_command` → `/v1/doctor` GET → `render_server_result` → `render_doctor`.
4. **Found the missing unwrap.** `/v1/doctor` handler returns `Json<DoctorResult>` which serializes as `{"payload": {...}}`. `render_doctor` passed this directly to `render_doctor_report_human`, but `render_stats` (same file, line 113) already had the correct pattern: `let payload = result.get("payload").unwrap_or(result);`.
5. **Applied the fix.** Added the same `payload` unwrap to `render_doctor` in `src/cli/server_mode/render.rs`.
6. **Rebuilt and verified.** `axon doctor` now shows all services ✓.
7. **Push blocked by clippy.** Pre-existing unused-qualification lint in `src/ingest/sessions_tests.rs` (three uses of `prepared::MAX_PREPARED_SESSION_DOCS`) blocked the lefthook pre-push hook.
8. **Fixed clippy error.** Removed the unnecessary `prepared::` prefix from the three references.
9. **Committed and pushed.** Both fixes landed on `main`.
10. **Session resumed from compaction.** The continuation session verified clippy was clean and git was already up to date; no further action required.

## Key Findings

- `render_doctor` at `src/cli/server_mode/render.rs:118` was missing the `result.get("payload").unwrap_or(result)` unwrap that `render_stats` at line 112 already had.
- `/v1/doctor` handler at `src/web/server/handlers/discovery.rs` returns `Json<DoctorResult>` — axum serializes this as `{"payload": {...}}`, adding a layer that all server-mode renderers must unwrap.
- `render_doctor_report_human` reads `report["services"]["tei"]["ok"]` — when `report` is the outer wrapper, `report["services"]` is null and returns false, causing every service to display as "failed".
- The JSON output path (`--json`) was unaffected because `render_server_result` takes the early return for `cfg.json_output` before calling `render_doctor`.
- `src/ingest/sessions_tests.rs` lines 98, 99, 102 had `prepared::MAX_PREPARED_SESSION_DOCS` where the const is already in scope via `use super::*;` — unnecessary qualification that clippy caught with `-D unused-qualifications`.

## Technical Decisions

- **Reused the existing pattern from `render_stats`.** The `result.get("payload").unwrap_or(result)` idiom was already established at line 112. Consistency mattered more than anything clever.
- **Did not change the server response format.** The `{"payload": {...}}` wrapper is the correct Axum serialization of `DoctorResult` and is relied on by MCP/API callers. The fix belongs in the CLI renderer, not the server.
- **Fixed clippy in the same push.** The lint was pre-existing but blocked the hook. Rather than skip hooks (`--no-verify`), fixed the underlying issue.

## Files Changed

| Status | Path | Purpose | Evidence |
|--------|------|---------|---------|
| modified | `src/cli/server_mode/render.rs` | Add `payload` unwrap in `render_doctor` | commit b85c7b58 |
| modified | `src/ingest/sessions_tests.rs` | Remove unnecessary `prepared::` qualification (3 occurrences) | commit 4c3ce0df |

## Beads Activity

No bead activity observed. This was a targeted bug fix with no associated beads created or closed.

## Repository Maintenance

**Plans:** No plan files were moved. This fix did not correspond to an active plan document.

**Beads:** Checked open issues — `axon_rust-psnq` (async prepared session ingest epic) is open; not directly related to this fix. No other beads were directly relevant.

**Worktrees/branches:** Only `main` in use. No stale worktrees detected. `origin/claude/new-session-C3INm` (b119ea55) remains on remote — leftover from prior session automation, not merged into main; left alone as it may contain unmerged artifacts from its session.

**Stale docs:** No documentation was contradicted by this fix.

**Dirty files:** `src/vector/ops/qdrant/client/retrieve.rs` and `src/vector/ops/qdrant/client/retrieve_tests.rs` are modified but uncommitted — these appear to be in-progress work from a prior session, not part of this fix. Left as-is.

## Tools and Skills Used

- **Shell commands (`rtk cargo clippy`, `rtk git push`, `rtk git log`):** Used to verify clippy cleanliness and push status after resuming from compaction. No issues encountered.
- **File tools (Read):** Used to inspect `sessions_tests.rs` and `server_mode/render.rs` during diagnosis in the compacted portion of the session.
- **Bash:** Git log, clippy, push verification.

## Commands Executed

| Command | Result |
|---------|--------|
| `cargo clippy` | No issues found |
| `rtk git push` | ok (up-to-date) |
| `rtk git log --oneline -5` | Confirmed b85c7b58 and 4c3ce0df on remote |

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| `axon doctor` (human output, with `AXON_SERVER_URL` set) | All services shown as `✗ failed` despite server being healthy | All services shown as `✓ completed` matching actual health |
| `axon doctor --json` | Correct (unaffected — takes early JSON return path) | Unchanged |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `axon doctor` (after fix, server running) | All services ✓ | All services ✓ — sqlite, tei, qdrant, chrome, gemini_headless + all pipelines | pass |
| `cargo clippy` | No errors | No issues found | pass |

## Next Steps

- **Uncommitted changes in `retrieve.rs` / `retrieve_tests.rs`**: These dirty files should be reviewed and either committed or stashed before starting new work.
- **`axon_rust-psnq`** (async prepared session ingest epic): Still open; the sessions server-mode work it covers may benefit from auditing whether other server-mode renderers have the same missing-payload-unwrap pattern.
- **Audit other `render_*` functions** in `src/cli/server_mode/render.rs` for missing `result.get("payload").unwrap_or(result)` calls — `render_stats` and `render_doctor` have it; check `render_scrape`, `render_sources`, `render_domains`, `render_map`, `render_query`, `render_retrieve`, `render_ask`, `render_evaluate`, `render_suggest`, `render_search`, `render_research`, `render_screenshot`.
