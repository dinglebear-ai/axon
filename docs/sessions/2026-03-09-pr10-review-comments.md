# PR #10 Review Comments — Full Pass
**Date:** 2026-03-09
**Branch:** `refactor/acp-performance-modern-rust`
**PR:** [#10 — refactor(acp): performance/scalability fixes + modern Rust idioms (v0.11.2)](https://github.com/jmagar/axon/pull/10)

---

## Session Overview

Addressed all 114 unresolved CodeRabbit review threads across 67 files in PR #10. Dispatched 14 parallel agents across 3 waves, each owning a non-overlapping domain cluster. All waves completed clean. One wiring gap (`session_guard` mod declaration) was fixed inline after wave 3.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | Re-authenticated `gh` CLI (token was expired) |
| +5min | Fetched 114 unresolved threads from PR #10 via `fetch_comments.py` |
| +10min | Generated `docs/reports/pr10-review-comments-2026-03-09.md` (2,442 lines, 67 files) |
| +15min | Dispatched **Wave 1** — 5 agents, 25 files, 60 comments |
| +60min | Wave 1 complete; dispatched **Wave 2** — 5 agents, 25 files, 33 comments |
| +90min | Wave 2 complete; dispatched **Wave 3** — 4 agents, 17 files, 21 comments |
| +120min | Wave 3 complete; wired `session_guard` mod declaration inline |

---

## Key Findings

- **Critical data-loss bug** in `crates/vector/ops/tei.rs:69` — delete-before-upsert pattern could permanently destroy indexed documents on failed upsert. Fixed with upsert-first + stale-tail cleanup.
- **P1 SSRF gap** in `crates/ingest/youtube.rs:102` — playlist/channel enumeration bypassed the SSRF guard used for single-video ingest. Fixed by adding `validate_url()` before `yt-dlp` spawn.
- **Permanent loading bug** in `apps/web/components/reboot/axon-shell.tsx:175` — empty sessions caused `isLoading` to never settle because it used `messages.length === 0` as a proxy. Fixed via new `loaded: boolean` field in `use-axon-session.ts`.
- **Silent permission hang** in `crates/services/acp/bridge.rs` — blank `session_id` silently inserted a key `("", tool_call_id)` that the WS router could never match, causing a guaranteed 60-second hang.
- **Reddit timestamp bug** in `crates/ingest/reddit/meta.rs:13/23` — `as_u64()` returns `None` for JSON floats; Reddit sends `created_utc` as `1710000000.0`, causing all timestamps to store `0`. Fixed with `as_f64().map(|f| f as u64)`.
- **Test mirrors** — multiple test files (`use-axon-session-retry.test.ts`, `axon-message-list-editor-blocks.test.ts`, `use-axon-acp-editor.test.ts`) contained local copies of production logic that could drift. All updated to import production functions directly.
- **`session_guard.rs`** was restored by Agent C2 but not wired — `mod session_guard;` was missing from `crates/web/execute.rs`. Fixed inline post-wave-3.

---

## Agent Wave Summary

### Wave 1 (5 agents, 25 files)
| Agent | Domain | Files | Highlights |
|-------|--------|-------|-----------|
| A1 | Ingest Rust | `process.rs`, `youtube.rs`, `vtt.rs`, `reddit/meta.rs`, `github/meta.rs` | SSRF fix, Reddit float timestamp, VTT state-machine rewrite |
| A2 | ACP Services | `persistent_conn.rs`, `bridge.rs`, `acp.rs`, `runtime.rs`, `session.rs` | Blank session_id guard, turn-ID tracking, `run_acp_event_loop` extraction |
| A3 | Web Execute | `web.rs`, `acp_adapter.rs`, `types.rs`, `subprocess.rs`, `pulse_chat.rs` | Validation bypass gated, `editor_update` standalone WS message, double JSON fixed |
| A4 | MCP Artifacts | `lifecycle.rs`, `respond.rs`, `handlers_system.rs`, `path.rs`, `shape.rs` | Atomic write (no delete-before-upsert), symlink rejection, viewport validation |
| A5 | Sessions TS | `session-scanner.ts`, `codex-scanner.ts`, `gemini-scanner.ts`, `use-axon-acp.ts`, `use-axon-session.ts` | Limit overflow fix, `loaded` flag, `handleEditorMsg` extracted, concurrency validation |

### Wave 2 (5 agents, 25 files)
| Agent | Domain | Files | Highlights |
|-------|--------|-------|-----------|
| B1 | Reboot components | `axon-editor-artifact.tsx`, `axon-shell.tsx`, `pulse-editor-pane.tsx`, `axon-sidebar.tsx`, `axon-ui-config.ts` | Code-fence masking for `<axon:editor>`, P1 desync fix, `RailModeItem`/`PageItem`/`AgentItem` interfaces |
| B2 | Tests + ai-elements | `use-axon-acp-editor.test.ts`, `axon-message-list-editor-blocks.test.ts`, `use-axon-session-retry.test.ts`, `artifact.tsx`, `diff-kit.tsx` | All test mirrors replaced with production imports; `TooltipProvider` lifted; `diff-kit-utils.ts` created |
| B3 | API routes + utils | `sessions/list/route.ts`, `sessions/[id]/route.ts`, `session-utils.ts`, `gemini-json-parser.ts`, `diff-node-static.tsx` | Triple-scan fixed; `X-Retry-After` removed from 404; `mapWithConcurrency` validation at source |
| B4 | Docs | `commands/ingest.md`, `commands/github.md`, `commands/reddit.md`, `commands/youtube.md`, `ingest/github.md` | Deprecated command pages restored to full format; `yt-dlp` prereq added; cross-links fixed; `docs/ingest/ingest.md` created |
| B5 | Remaining Rust | `oauth_google/tests.rs`, `acp_ws_event_tests.rs`, `vector/ops/tei.rs`, `sync_mode/dispatch.rs`, `acp/mapping.rs` | CRITICAL upsert-first fix in `tei.rs`; `qdrant_delete_stale_tail` added; `dispatch_service` split |

### Wave 3 (4 agents, 17 files)
| Agent | Domain | Files | Highlights |
|-------|--------|-------|-----------|
| C1 | Docs | `ARCHITECTURE.md`, `REBOOT-UI.md`, `MCP-TOOL-SCHEMA.md`, `MCP.md`, `ingest/youtube.md` | `auto-inline` added to `ResponseMode` enum; `path` qualified per-subaction |
| C2 | Web execute Rust | `params.rs`, `session_guard.rs`, `events.rs`, `sync_mode.rs`, `services_acp_security.rs` | `max_points` field fix; `EnvVarGuard` RAII for test env cleanup; `HOME`→`USERPROFILE` fallback |
| C3 | Config + Build | `Justfile`, `config/cli.rs`, `acp/config.rs`, `Cargo.toml` | PID-scoped cleanup in Justfile; `ArgAction::SetTrue` for bool flags; `toml` parse-only feature |
| C4 | CLAUDE.md files | `CLAUDE.md`, `apps/web/CLAUDE.md`, `crates/ingest/CLAUDE.md` | Stale `youtube`/`github`/`reddit` commands replaced with `ingest`; Reddit depth inconsistency fixed |

---

## Files Modified

### New Files Created
- `docs/reports/pr10-review-comments-2026-03-09.md` — full review comments index (2,442 lines)
- `docs/ingest/ingest.md` — stub created for cross-link target
- `docs/ingest/youtube.md` — cross-link corrected (not new but updated)
- `apps/web/components/editor/plugins/diff-kit-utils.ts` — non-client `computeDiff` re-export

### Key Rust Files Modified
- `crates/vector/ops/tei.rs` — CRITICAL: upsert-first pattern
- `crates/vector/ops/qdrant/client.rs` — `qdrant_delete_stale_tail` added
- `crates/ingest/youtube.rs` — SSRF guard on playlist path; bare ID validation reordered
- `crates/ingest/youtube/vtt.rs` — full state-machine VTT parser
- `crates/ingest/reddit/meta.rs` — float timestamp fix
- `crates/ingest/github/meta.rs` — `gh_comment_count` added to PR payload
- `crates/services/acp/bridge.rs` — blank session_id guard; turn-ID tracking; responder map cleanup
- `crates/services/acp/persistent_conn.rs` — established model tracking
- `crates/services/acp/session.rs` — CWD validation
- `crates/services/acp/mapping.rs` — `validate_session_cwd` + `current_value` validation
- `crates/services/acp/config.rs` — stale model slug validation
- `crates/mcp/server/artifacts/respond.rs` — atomic write; auto-inline skips disk write
- `crates/mcp/server/artifacts/path.rs` — symlink rejection
- `crates/mcp/server/artifacts/shape.rs` — `.chars().count()` for Unicode
- `crates/mcp/server/handlers_system.rs` — `response_mode` wired; viewport validation returns Result
- `crates/web.rs` — validation bypass gated to debug/test
- `crates/web/execute/sync_mode/pulse_chat.rs` — `editor_update` as standalone WS message
- `crates/web/execute/sync_mode/subprocess.rs` — double JSON fixed; screenshots on all exit paths
- `crates/web/execute/sync_mode/dispatch.rs` — split into helpers
- `crates/web/execute/sync_mode/acp_adapter.rs` — validation at resolution point
- `crates/web/execute/sync_mode/params.rs` — `max_points` field fix
- `crates/web/execute/session_guard.rs` — HOME/USERPROFILE fallback
- `crates/web/execute/events.rs` — doc clarification for `command.output.json`
- `crates/web/execute.rs` — `mod session_guard` wired
- `crates/jobs/ingest/process.rs` — split into 4 helpers; imports moved; silent SQL errors logged
- `tests/services_acp_security.rs` — `EnvVarGuard` RAII
- `Justfile` — PID-scoped cleanup; proper `wait` loop
- `Cargo.toml` — `toml` parse-only feature
- `crates/core/config/cli.rs` — `ArgAction::SetTrue` for bool flags

### Key TypeScript Files Modified
- `apps/web/hooks/use-axon-session.ts` — `loaded` flag; `fetchSessionWithRetry` exported
- `apps/web/hooks/use-axon-acp.ts` — `handleEditorMsg` extracted and exported
- `apps/web/lib/sessions/session-scanner.ts` — limit overflow fix; concurrency validation wrapper
- `apps/web/lib/sessions/codex-scanner.ts` — capacity cap
- `apps/web/lib/sessions/gemini-scanner.ts` — runtime string guard; capacity cap
- `apps/web/lib/sessions/session-utils.ts` — `mapWithConcurrency` throws on `concurrency <= 0`
- `apps/web/lib/sessions/gemini-json-parser.ts` — `msg.content` string guard
- `apps/web/components/reboot/axon-shell.tsx` — `loaded` flag; mobile pane on editor update
- `apps/web/components/reboot/axon-editor-artifact.tsx` — code-fence masking; timer cleanup
- `apps/web/components/pulse/pulse-editor-pane.tsx` — P1 desync fix; dev-mode warning
- `apps/web/components/reboot/axon-ui-config.ts` — named interfaces; PAGE_ITEMS/AGENT_ITEMS moved here
- `apps/web/components/reboot/axon-sidebar.tsx` — imports from axon-ui-config
- `apps/web/components/ai-elements/artifact.tsx` — TooltipProvider lifted to group
- `apps/web/app/api/sessions/list/route.ts` — triple-scan removed
- `apps/web/app/api/sessions/[id]/route.ts` — X-Retry-After removed from 404
- `apps/web/__tests__/axon-message-list-editor-blocks.test.ts` — imports production parser
- `apps/web/__tests__/use-axon-session-retry.test.ts` — imports production retry function
- `apps/web/__tests__/use-axon-acp-editor.test.ts` — imports production handleEditorMsg

---

## Behavior Changes (Before → After)

| Area | Before | After |
|------|--------|-------|
| Vector re-embed | Delete all points → upsert (data loss on failure) | Upsert → delete stale tail (atomic, safe) |
| YouTube playlist ingest | No SSRF guard on playlist/channel URLs | `validate_url()` called before `yt-dlp` spawn |
| Reddit timestamps | `created_utc` stored as `0` for float values | Correctly parsed via `as_f64()` cast |
| VTT parser | Numeric lines (e.g. `2024`) stripped from transcripts | Kept; NOTE/STYLE/REGION blocks properly filtered |
| Empty session loading | Loading spinner never settles | `loaded: boolean` flag settles after fetch completes |
| ACP permission blank session | 60-second silent hang | Immediate `Cancelled` with warn log |
| `<axon:editor>` in code blocks | Tag in code fences triggers editor actions | Masked during parsing; code content preserved |
| `pulse-editor-pane` on error | `isApplyingExternalUpdateRef` left true (permanent desync) | Reset to false on error, retries on next render |
| `just workers` crash | Bare `wait` returns 0 even if child crashes | PID loop propagates any child failure exit code |
| `--include-source` flag | Requires explicit `true` argument | Bare flag presence sets `true` (standard behavior) |
| Triple session scan in API | `scanSessions` + `scanCodexSessions` + `scanGeminiSessions` | Single `scanSessions` call (deduplication was tripled) |
| `toml` crate features | Full (parse + serialize) | Parse-only (serialize surface removed) |

---

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| `cargo check` post all waves | No errors | 3 unused-fn warnings in `session_guard` (expected) | ✅ |
| `pnpm tsc --noEmit` (TS) | No errors | Clean | ✅ |
| Wave 1 compile | Clean | Clean | ✅ |
| Wave 2 compile | Clean | Clean | ✅ |
| Wave 3 compile | Clean | Clean | ✅ |
| `session_guard` mod wired | Compiles | Compiles with 3 expected warnings | ✅ |

---

## Risks and Rollback

- **`tei.rs` upsert-first** — changed the embed atomicity contract. Rollback: revert `crates/vector/ops/tei.rs` and `crates/vector/ops/qdrant/client.rs`. Risk: low — the new pattern is strictly safer.
- **`Justfile` wait loop** — changed process supervision behavior. Rollback: revert `Justfile`. Risk: low — the fix is strictly more correct.
- **`session_guard.rs`** — module wired but functions unused (3 warnings). These will need callers wired before shipping. Not a regression — file was previously absent.
- **Wave 3 `CLAUDE.md` edits** — removed `github`/`reddit`/`youtube` command rows. If any tooling parses the commands table, it needs updating.

---

## Open Questions

- `session_guard` functions (`projects_dir`, `find_session_file`, `poll_session_file`) are unused — callers need to be wired. Is this for a planned feature?
- `crates/mcp/server/mcp_config.rs` — flagged by C2 as also needing `.exe` stripping for shell blocklist. Out of scope for this session.
- Thread resolution: threads have not yet been marked resolved via `mark_resolved.py` — that step is pending.
- `cargo test` full suite not run — compile verified clean but test regressions not ruled out.

---

## Next Steps

1. **Run `verify_resolution.py`** to check remaining unresolved threads after all the fixes
2. **Mark threads resolved** via `mark_resolved.py` with the thread IDs for all addressed comments
3. **Run `cargo test`** to catch any regressions from the wave changes
4. **Wire `session_guard` callers** — three exported functions are unused
5. **Fix `mcp_config.rs`** `.exe` stripping for shell blocklist (flagged by C2, out of prior scope)
6. **Push branch** and request re-review from CodeRabbit
