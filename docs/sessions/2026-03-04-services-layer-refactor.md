# Services Layer Refactor — Session Log
**Date:** 2026-03-04
**Branch:** `feat/services-layer-refactor`
**Session Duration:** Multi-context (resumed from prior session)

---

## Session Overview

Completed the full Services Layer Refactor plan (`docs/plans/2026-03-03-services-layer-plan.md`). The `crates/services/` module is now the single source of business logic; CLI, MCP, and WS are all thin transport adapters. All 6 tasks across 5 waves executed, verified, and committed.

---

## Timeline

| Time | Activity |
|------|----------|
| Session start | Resume mid-Wave 3 — two quality fixes pending in `handlers_query.rs` |
| Wave 3 | Fixed `retrieve` chunk_count bug + `research` error class; QUALITY_PASS |
| Wave 3 | Added `map_retrieve_result_stores_chunk_count_inside_chunks_element` test |
| Wave 4 | Task 5.1: Web config plumbing + override mapper via subagent |
| Wave 4 | Task 5.2–5.3: Pre-existing commits found; fixed resulting compile errors + test failures |
| Wave 5 | Task 6.1: Dead code cleanup — deleted `polling.rs`, removed `#[allow(dead_code)]` |
| End | User invoked `save-to-md` + `quick-push` |

---

## Key Findings

- **`chunk_count` data contract**: `map_retrieve_result` stores chunk count at `chunks[0]["chunk_count"]`, not `chunks.len()` (which is always 0 or 1). `crates/mcp/server/handlers_query.rs` was reading `result.chunks.len()` — always wrong.
- **`!Send` constraint blocks ingest direct dispatch**: `github`/`reddit`/`youtube` service functions use `Box<dyn Error>` without `+ Send` in sub-futures, making them `!Send`. They must stay on the subprocess fallback path.
- **`ASYNC_MODES` narrowed**: Was `["crawl","extract","embed","github","reddit","youtube"]` → now `["crawl","extract","embed"]`. New `ASYNC_SUBPROCESS_MODES = ["github","reddit","youtube"]`.
- **`polling.rs` deleted**: The entire job-polling loop module (274 lines) was removed. Fire-and-forget replaces polling for async modes.
- **No REST API exists**: Three transport surfaces (MCP stdio, WS bridge, CLI). Services layer is the foundation — a REST API would be another thin shim, same pattern.

---

## Technical Decisions

- **`Box::pin(async move {...})` wrappers** in `async_mode.rs`: Service functions take `&Config` references that span `.await` points. Wrapping in `Box::pin` with `Arc<Config>` captured by value achieves `Send + 'static` without lifetime parameters leaking to the outer `handle_command` future.
- **`unreachable!()` over `return Err()`** in `overrides.rs`: The `render_mode` guard validates against `VALID_RENDER_MODES` before the match arm. `Some(_) => unreachable!(...)` is more accurate than silently returning an error for dead code.
- **`internal_error` not `invalid_params`** for runtime failures: MCP spec distinguishes "bad input" (invalid_params) from "service failure" (internal_error). Research handler was misclassifying runtime `Err` as `invalid_params`.
- **Fire-and-forget semantics**: `handle_async_command` enqueues via services, emits job ID to browser, and returns immediately. No polling loop survives. Cancel support remains via `crawl_job_id` mutex.

---

## Files Modified

| File | Action | Purpose |
|------|--------|---------|
| `crates/mcp/server/handlers_query.rs` | Modified | Fix `chunk_count` extraction; fix `research` error class |
| `crates/web/execute/async_mode.rs` | Rewritten | Fire-and-forget via direct service calls (no subprocess) |
| `crates/web/execute/constants.rs` | Modified | Narrow `ASYNC_MODES`; add `ASYNC_SUBPROCESS_MODES`, `NO_JSON_MODES` |
| `crates/web/execute.rs` | Modified | Three-path routing doc; pub re-exports for integration tests |
| `crates/web/execute/overrides.rs` | Created | `ws_request_to_overrides()` — WS flags → `WsConfigOverrides` |
| `crates/web/execute/context.rs` | Modified | Remove `#[allow(dead_code)]` from `cfg` field |
| `crates/web/execute/polling.rs` | **Deleted** | Entire polling loop removed (274 lines) |
| `crates/web/execute/tests/ws_event_v2_tests.rs` | Modified | Remove test referencing deleted `polling` module |
| `crates/web/execute/tests/ws_protocol_tests.rs` | Modified | Narrow async modes test; add subprocess async modes test |
| `tests/web_ws_async_fire_and_forget.rs` | Modified | Rename + update `async_modes_contains_lifecycle_commands` |
| `tests/mcp_contract_parity.rs` | Modified | Add `map_retrieve_result_stores_chunk_count_inside_chunks_element` test |

---

## Commands Executed

```bash
cargo test --lib                    # 971 tests passing, 0 failures
cargo clippy                        # 0 warnings
cargo fmt --check                   # clean
just verify                         # full pre-PR gate passed
```

---

## Behavior Changes (Before/After)

| Behavior | Before | After |
|----------|--------|-------|
| `crawl`/`extract`/`embed` WS dispatch | Subprocess (`axon` binary) | Direct service enqueue (no subprocess) |
| `github`/`reddit`/`youtube` WS dispatch | Classified as `ASYNC_MODES` | Subprocess fallback (correctly classified) |
| `retrieve` chunk_count in MCP | Always 0 or 1 (wrong) | Actual Qdrant point count (correct) |
| `research` MCP error class | `invalid_params` on runtime failure | `internal_error` on runtime failure |
| Dead code `polling.rs` | 274-line module existed | Deleted |
| `ExecCommandContext.cfg` | `#[allow(dead_code)]` suppressed lint | Actively used by `sync_mode.rs` |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo test --lib` | 971 pass, 0 fail | 971 pass, 0 fail | ✅ PASS |
| `cargo clippy` | 0 warnings | 0 warnings | ✅ PASS |
| `cargo fmt --check` | clean | clean | ✅ PASS |
| `just verify` | full gate pass | passed | ✅ PASS |

---

## Commit Log (Wave Summary)

```
4e5144a3 chore(web): remove dead code from services layer refactor
14b62d49 feat(web): fire-and-forget async dispatch and cancel via services
476ad35b feat(web): replace sync subprocess execution with direct service dispatch
fe83d0a9 fix(web): replace dead Some(other) arm with unreachable! in render_mode match
ed2bd90d refactor(web): plumb base Config and ws override mapping for direct service dispatch
dae2b0b1 test(mcp): pin map_retrieve_result data contract — chunk_count in wrapper element
e93df53e fix(mcp): correct retrieve chunk_count and research error class
fb485043 fix(mcp): preserve sources wire contract — urls remains string[] in MCP response
03996f72 fix(mcp): use option mapper helpers in system and query handlers
38f0a53d refactor(mcp): rewire handlers to use services layer
d146571f refactor(mcp): add request-to-service option mappers
e4f81653 fix(services): address quality review issues from Wave 2
```

---

## Errors and Fixes

### Compile Error: `polling` module not found
- **Error**: `error[E0433]: failed to resolve: could not find 'polling' in 'super'`
- **Location**: `crates/web/execute/tests/ws_event_v2_tests.rs`
- **Cause**: `mod polling;` removed from `execute.rs` but test still referenced `super::polling::poll_messages_for_status`
- **Fix**: Removed `async_polling_dual_emits_legacy_and_v2_status_progress` test, replaced with explanatory comment

### Test Failure: `build_args_skips_wait_flag_for_async_modes`
- **Cause**: Test included `github/reddit/youtube` as modes that should suppress `--wait`. After Task 5.3, these moved to `ASYNC_SUBPROCESS_MODES`.
- **Fix**: Narrowed test to `crawl/extract/embed`; added `build_args_allows_wait_flag_for_subprocess_async_modes`

### Test Failure: `async_modes_contains_lifecycle_commands` (integration test)
- **Same cause** as above — expected `github/reddit/youtube` in `ASYNC_MODES`
- **Fix**: Renamed test to `async_modes_contains_direct_enqueue_commands`; updated assertions

---

## Risks and Rollback

- **`polling.rs` deletion is permanent**: Async job status is now tracked via the job ID emitted at enqueue time. The browser polls `/api/jobs/[id]` (Next.js route) rather than the WS bridge. No rollback path without re-implementing polling.
- **`!Send` constraint is a known blocker**: `github`/`reddit`/`youtube` can't be moved to direct dispatch until the upstream service functions are made `Send`. Tracked in code via `ASYNC_SUBPROCESS_MODES` constant.

---

## Decisions Not Taken

- **REST API**: Discussed but not in plan scope. Would be another thin shim over `crates/services/` — same pattern as MCP/WS/CLI. Deferred.
- **Fixing `!Send` for ingest services**: Would require either making `Box<dyn Error>` into `Box<dyn Error + Send>` throughout the ingest call chain, or wrapping in `spawn_blocking`. Deferred — subprocess fallback is correct and safe.

---

## Open Questions

- REST API endpoint design — user expressed interest but did not explicitly request it
- `github`/`reddit`/`youtube` `!Send` resolution timeline — upstream service refactor needed

---

## Next Steps

- `quick-push` — commit + push all changes on `feat/services-layer-refactor`
- Open PR once pushed
- Consider REST API as follow-up scope if user requests
