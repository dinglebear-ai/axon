# Session: Pulse Chat Debugging — No Responses in Web UI

**Date:** 2026-03-06
**Branch:** `feat/services-layer-refactor`
**Status:** In progress — diagnostic logging added, awaiting test with restarted services

## Session Overview

Systematically debugged why the Pulse chat in the Axon web UI returns no responses. Traced the full data flow from frontend POST through WebSocket bridge to ACP adapter subprocess and back. Identified the `agent_client_protocol` v0.10.0 SDK's inability to decode `usage_update` session notifications as a red herring (non-fatal). Added comprehensive diagnostic logging to both Rust backend and Next.js frontend to pinpoint the exact failure point.

## Timeline

1. **Phase 1: Root Cause Investigation** — Traced the full chat data flow:
   - Frontend `POST /api/pulse/chat` → `runAxonCommandWsStream('pulse_chat')` over WS
   - Rust backend `ws_upgrade` → `classify_sync_direct` → `handle_pulse_chat`
   - ACP scaffold spawns `claude-agent-acp` subprocess, exchanges JSON-RPC via stdin/stdout
   - ACP SDK `Client::session_notification` accumulates assistant text, emits `ServiceEvent::AcpBridge`
   - Events mapped via `acp_bridge_event_payload` → WS `command.output.json` → frontend `onJson`

2. **Confirmed services running:** Rust backend on port 49000 (PID 630905), Next.js on port 49010.

3. **Confirmed ACP adapter exists:** `/usr/local/bin/claude-agent-acp` (120 MB binary).

4. **Discovered dotenvy loads `.env` at startup** (`main.rs:26 load_dotenv()`), so `AXON_WEB_API_TOKEN` is active even though it doesn't appear in `/proc/PID/environ`.

5. **Confirmed WS auth works** with URL-encoded token (`4TDc7+OFAzm29G5Pjz4qhUox3MeA1bn0MSiRp0LGrE4=` → `%2B` and `%3D`). Got HTTP 101 Switching Protocols.

6. **User shared ACP decode errors** — `agent_client_protocol::rpc: failed to decode` for `usage_update` variant. Confirmed this is non-fatal (rpc.rs:237-239 logs error and continues loop).

7. **Added diagnostic logging** to both Rust and Next.js sides.

## Key Findings

- **`dotenvy` in `main.rs:26-55`**: The Rust binary loads `.env` via `dotenvy::from_path()` and `dotenvy::dotenv()` at startup. Environment variables set this way are visible to `std::env::var()` but NOT in `/proc/PID/environ`. This caused confusion during token debugging.

- **ACP SDK v0.10.0 missing `usage_update`** (`~/.cargo/registry/src/.../agent-client-protocol-0.10.0/src/rpc.rs:237`): The Claude ACP adapter sends `usage_update` notifications that the SDK's `SessionUpdate` enum doesn't recognize. The error is logged at ERROR level but is non-fatal — the notification is dropped and the IO loop continues. The schema crate `agent-client-protocol-schema` is at v0.11.0 on crates.io, suggesting the fix exists but the main crate hasn't been updated yet.

- **Default console log level is `warn`** (`crates/core/logging.rs:180`): `EnvFilter::new("warn,...")` means `log::info!` and `log::debug!` are suppressed. Diagnostic logging uses `log::warn!` to be visible without RUST_LOG override.

- **`pulse_chat` is a direct-dispatch mode** (`crates/web/execute/sync_mode.rs:54`): It goes through the services layer, not subprocess execution. The ACP adapter is spawned inside `acp_svc::AcpClientScaffold::start_prompt_turn()`.

- **`RequestPermissionOutcome::Cancelled`** (`crates/services/acp.rs:1053`): All tool permission requests are auto-denied. If Claude's response requires tool use, the turn may fail or produce no useful output.

## Technical Decisions

- **Used `log::warn!` instead of `log::info!`** for diagnostic messages so they appear with the default `warn` console filter. These should be reverted to `info` or removed after debugging.

- **Added logging at 5 key points in the Rust pipeline**: pulse_chat entry, adapter resolution, ACP event dispatch (with type + preview), prompt turn completion, and event loop completion.

- **Added logging at 4 key points in the Next.js pipeline**: request received, WS connection open/error/close, onJson events (with type), onDone (with result length and delta count), onError, and catch handler.

## Files Modified

| File | Purpose |
|------|---------|
| `crates/web/execute/sync_mode.rs` | Added `[pulse_chat]` diagnostic logging at warn level for ACP bridge events, adapter resolution, prompt turn lifecycle |
| `apps/web/app/api/pulse/chat/route.ts` | Added `[pulse/chat]` logging for request received, onJson event types, onDone stats, onError, and catch handler |
| `apps/web/lib/axon-ws-exec.ts` | Added `[axon-ws]` logging for WS open, error, and close events |

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| Rust stderr during pulse_chat | No pulse_chat-specific logging | `[pulse_chat]` lines showing adapter, events, completion |
| Next.js console during chat | No chat-specific logging | `[pulse/chat]` and `[axon-ws]` lines showing full event flow |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo build --bin axon` | Compiles | Compiled in 17s | PASS |
| `ss -tuln \| grep 49000` | Listening | PID 630905 on 0.0.0.0:49000 | PASS |
| `curl ... /ws?token=...` | 101 Switching | 101 Switching Protocols | PASS |
| `curl ... /ws` (no token) | 401 | 401 "token required" | PASS (gate active) |
| `which claude-agent-acp` | Found | /usr/local/bin/claude-agent-acp | PASS |

## Risks and Rollback

- **Logging at warn level**: The diagnostic `log::warn!` calls in `sync_mode.rs` will pollute logs for non-debugging usage. Rollback: revert to `log::info!` or remove after root cause is found.
- **Console.log in production paths**: The `console.log` calls in `route.ts` and `axon-ws-exec.ts` should be removed after debugging. They don't affect functionality but add noise.
- **No code logic changes**: Only logging was added. No risk to existing behavior.

## Decisions Not Taken

- **Did not upgrade `agent-client-protocol` to match schema v0.11.0**: The `usage_update` decode failure is non-fatal. Upgrading may fix the error log noise but won't fix the chat response issue.
- **Did not change RUST_LOG to `info`**: Would require restarting the process. Instead used `warn` level for diagnostics.
- **Did not attempt to fix the auto-deny on `RequestPermissionOutcome`**: This could be the root cause if Claude's response involves tool calls, but needs investigation after logging reveals the actual flow.

## Open Questions

1. **Is the ACP prompt turn actually completing?** The `usage_update` events suggest it does, but we haven't confirmed `TurnResult` is emitted and received by the frontend.
2. **Is `RequestPermissionOutcome::Cancelled` causing silent failure?** If every tool permission is denied, Claude may produce no useful output or the turn may loop/fail.
3. **Does the frontend WS connection survive the full prompt turn duration?** Claude responses can take 10-30s. The WS might be timing out or being closed by Next.js middleware.
4. **What does the `axon-ws-exec.ts` `onJson` callback actually receive?** The logging will reveal this on next test.
5. **Is the `command.done` event actually sent after the ACP event loop completes?** `handle_sync_direct` sends it via `send_done_owned`, but only if the event loop returns `Ok(())`.

## Next Steps

1. **Restart `axon serve`** with the new binary: `kill 630905 && ./target/debug/axon serve --port 49000`
2. **Send a chat message** in the Pulse UI and collect logs from both Rust stderr and Next.js console
3. **Analyze logs** to identify where the chain breaks:
   - Rust shows events but Next.js gets nothing → WS transport issue
   - Next.js gets events but no `onDone` → ACP hangs
   - `onDone` with `parserResult=0chars` → assistant text not accumulated
   - Rust shows `event loop failed` → ACP adapter error
4. **Investigate `RequestPermissionOutcome::Cancelled`** — if tool calls are being denied, this may need to be changed to auto-approve for the Pulse chat context
5. **Consider upgrading `agent-client-protocol`** to v0.11.0 schema to eliminate `usage_update` noise
6. **Remove diagnostic logging** after root cause is found and fixed
