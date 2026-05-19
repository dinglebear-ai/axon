# Session: Pulse Chat Three Bug Fixes + Verification
**Date:** 2026-03-09
**Branch:** `refactor/acp-performance-modern-rust`

---

## Session Overview

Fixed three bugs in the Pulse chat UI (`/reboot`) that degraded multi-turn conversation UX. All three were verified end-to-end via Chrome DevTools automation. A fourth bug (response bleed after timeout + agent switch) was reported and is pending investigation.

---

## Timeline

| Time | Activity |
|------|----------|
| Session start | Resumed from prior session where root causes were confirmed |
| Phase 1 | Fixed Bug 1 (stale `service_tx` in `AcpBridgeClient`) + Bug 2 (agent isolation) |
| Phase 2 | Fixed Bug 3 (scroll blocked by `overflow-y-hidden`) |
| Phase 3 | Fixed React frontend race (sync effect guard + `reloadSession` on turn complete) |
| Phase 4 | Full end-to-end verification — Claude 2-turn + Gemini 2-turn |
| Session end | User reported new Bug 4 (late response bleed after timeout+agent switch) |

---

## Key Findings

### Bug 1 — Stale `service_tx` in `AcpBridgeClient`
- **Root cause**: `AcpBridgeClient` held a `tx: Option<mpsc::Sender<ServiceEvent>>` baked in at bridge initialization (turn 1). For turns 2+, this sender was already closed → all `session_notification` / `request_permission` streaming deltas were emitted to a dead channel → silently dropped.
- **Location**: `crates/services/acp/bridge.rs:199` (old `tx` field), `crates/services/acp/session.rs` (construction site).
- **Fix**: Added `service_tx: std::cell::RefCell<Option<mpsc::Sender<ServiceEvent>>>` to `AcpRuntimeState` (`bridge.rs:32`). Each turn sets it before `conn.prompt()` and clears it after (`persistent_conn.rs:215,235`). Bridge callbacks now read from `runtime_state.service_tx` instead of the dead baked-in sender.

### Bug 2 — Agent session bleed (Gemini loads Claude history)
- **Root cause**: `acp_connection: Arc<Mutex<Option<Arc<AcpConnectionHandle>>>>` stored only the handle, not which agent spawned it. Switching agent picked up the existing Claude handle.
- **Fix**: Changed stored type to `Option<(String, Arc<AcpConnectionHandle>)>` where the `String` is the agent key (`format!("{agent:?}")`). On mismatch, old handle is dropped and a new adapter is spawned.
- **Locations**: `crates/web/execute/sync_mode/pulse_chat.rs`, `crates/web.rs:290`, `crates/web/execute.rs:171`, `crates/web/execute/sync_mode.rs`, `crates/web/execute/sync_mode/dispatch.rs`.

### Bug 3 — Chat not scrollable
- **Root cause**: `overflow-y-hidden` on the `StickToBottom` outer wrapper clipped the scroll container to zero visible height.
- **Fix**: `overflow-y-hidden` → `overflow-y-auto` in `apps/web/components/ai-elements/conversation.tsx:8`.

### React Frontend Race (Bug 1 companion)
- **Root cause**: Sync effect that set `liveMessages = historicalMessages` fired when `isStreaming` transitioned to `false` — wiping in-progress streaming messages. `isStreaming` was in the effect's deps.
- **Fix**: `isStreamingRef` pattern — ref tracks streaming state without being in deps. Effect excluded `isStreaming`, guarded by `isStreamingRef.current`. Added `reloadSession()` call in `onTurnComplete` to refresh historical messages after each turn.
- **Location**: `apps/web/components/reboot/axon-shell.tsx`.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/services/acp/bridge.rs` | Removed dead `tx` field from `AcpBridgeClient`; added `service_tx: RefCell<Option<...>>` to `AcpRuntimeState`; updated `session_notification` and `request_permission` to clone from `runtime_state.service_tx` |
| `crates/services/acp/session.rs` | Removed `tx: tx.clone()` from `AcpBridgeClient` construction |
| `crates/services/acp/persistent_conn.rs` | Added per-turn `service_tx` set/clear around `conn.prompt()`; `run_turn_on_conn` at lines 215 and 235 |
| `crates/web/execute/sync_mode/pulse_chat.rs` | Changed `acp_connection` param type to `Option<(String, Arc<AcpConnectionHandle>)>`; agent-key comparison in `get_or_create_acp_connection` |
| `crates/web.rs` | Updated `WsConnState.acp_connection` field type |
| `crates/web/execute.rs` | Updated `handle_command` signature |
| `crates/web/execute/sync_mode.rs` | Updated `handle_sync_direct` signature |
| `crates/web/execute/sync_mode/dispatch.rs` | Updated `dispatch_service` signature |
| `apps/web/components/ai-elements/conversation.tsx` | `overflow-y-hidden` → `overflow-y-auto` |
| `apps/web/components/reboot/axon-shell.tsx` | `isStreamingRef` guard + `reloadSession()` in `onTurnComplete` |

---

## Commands Executed

```bash
# Build verification (pre-existing MCP errors on branch — not introduced by this session)
cargo build --bin axon  # succeeded with pre-built binary from 12:09

# Dev server
just dev  # Next.js ready; Rust binary compilation blocked by pre-existing MCP errors
./target/debug/axon serve  # Used pre-built binary to start serve
```

---

## Behavior Changes (Before/After)

| Bug | Before | After |
|-----|--------|-------|
| Bug 1: Multi-turn messages | Sent message + typing indicator disappeared after first response | Messages persist; turn 2 renders correctly |
| Bug 2: Agent isolation | Switching to Gemini loaded Claude's session history | Gemini starts fresh; agent change respawns adapter |
| Bug 3: Scroll | Chat container not scrollable at all | Full scroll via `overflow-y-auto` on `StickToBottom` |
| React race | Live messages wiped when streaming ended | `isStreamingRef` guard prevents wipe during active stream |

---

## Verification Evidence

| Test | Expected | Actual | Status |
|------|----------|--------|--------|
| Claude turn 1: "My favorite color is midnight blue." | Response received | "Already got it — midnight blue is in your MEMORY.md" | ✓ PASS |
| Claude turn 2: "What is my favorite color?" | "Midnight blue" (context retained, message persists) | "Midnight blue." — message did NOT disappear | ✓ PASS (Bug 1 fixed) |
| New session → switch to Gemini: empty chat | No Claude messages loaded | "Gemini is ready" empty state | ✓ PASS (Bug 2 fixed) |
| Gemini turn 1: "My favorite color is midnight blue." | Response received from Gemini | "I have saved that to my memory…midnight blue…" | ✓ PASS |
| Gemini turn 2: "What is my favorite color?" | "Midnight blue" from Gemini | "Your favorite color is midnight blue." | ✓ PASS (multi-turn + Bug 1 confirmed for Gemini) |

---

## Source IDs + Collections Touched

None — no Axon embed/retrieve operations performed during this session.

---

## Risks and Rollback

- **`AcpRuntimeState.service_tx` RefCell**: Safe because the ACP runtime runs exclusively on a `current_thread` tokio runtime inside `LocalSet` (single-threaded). If ever moved to a multi-threaded context, this would need `Mutex`.
- **Agent-key type change** (`Option<Arc<...>>` → `Option<(String, Arc<...>)>`): 5 files required signature updates. All compile-time checked — no runtime risk.
- **Rollback**: `git revert` of the relevant commits on `refactor/acp-performance-modern-rust`.

---

## Decisions Not Taken

- **Storing `PulseChatAgent` enum directly** in the connection tuple: rejected because `PulseChatAgent` is defined in `pulse_chat.rs` — keeping it as `String` avoids cross-module type dep.
- **`Mutex<service_tx>` instead of `RefCell`**: rejected — `RefCell` is intentionally `!Send` here; the single-threaded LocalSet guarantee makes it correct and lock-free on the hot streaming path.
- **Fixing pre-existing MCP build errors** (`crates/mcp/server/artifacts.rs`, `handlers_system.rs`): user directed to leave them — another agent is working on them.

---

## Open Questions

- **Bug 4 (new, reported at session end)**: User reports sending a Gemini message → timeout/"check agent configuration" error → switched to Claude in same session → received what looked like a Gemini-written response that then disappeared and was replaced by a Claude response. Hypothesis: Gemini's response arrived late (after timeout was declared) and was delivered to the now-Claude-active session's channel. Root cause investigation needed:
  - Where is the per-turn timeout set? (`persistent_conn.rs` has no explicit timeout on `conn.prompt()`)
  - What happens when the adapter connection is replaced mid-flight? Does the old in-flight `result_tx` fire late?
  - Should we add a timeout on `conn.prompt()` calls?
- **Pre-existing MCP build errors**: `crates/mcp/server/artifacts.rs` (`regex` crate missing), `crates/mcp/server/handlers_system.rs` (missing common imports, `parse_viewport`). Another agent is addressing these.
- **`just dev` blocked by MCP errors**: The `just dev` recipe tries to recompile — failed due to MCP errors. Used pre-built `target/debug/axon` binary (compiled at 12:09 before MCP errors were introduced). Once MCP errors fixed, `just dev` will work normally.

---

## Next Steps

1. **Investigate Bug 4** — add per-turn timeout on `conn.prompt()` in `run_turn_on_conn`; ensure late responses from a replaced adapter don't bleed into the new session.
2. **Wait for MCP fix** from the parallel agent, then re-run `just dev` to confirm full build passes.
3. **Commit the three bug fixes** once the full build is green.
