# ACP Initialize Deadlock Debug Log
Date: 2026-03-05
Repo: `axon_rust`

## Problem Summary
`pulse_chat` fails for both ACP agents (`codex-acp`, `claude-agent-acp`).

Observed runtime sequence:
1. `ACP scaffold accepted prompt turn (session_id=<new>)`
2. `ACP runtime: spawning adapter process`
3. `ACP runtime: transport ready, starting initialize`
4. `ACP runtime: initialize request sent`
5. No `initialize` response ever arrives; command hangs until timeout.

This is a deadlock/stall at ACP `initialize` handshake.

## Reproduction Evidence
Direct WS probe to workers from `axon-web` repeatedly shows:
- `command.start`
- `command.output.json` status entries above
- no `command.done`
- timeout reached (20s/120s probes)

Both agents reproduce the same stall point (`codex`, `claude`).

## What Was Debugged and Changed

### 1) Host permission regression fixed
Problem found:
- container init logic recursively `chown`'d mounted host paths, causing host ownership drift.

Actions:
- Restored host ownership to `jmagar:jmagar` on:
  - `/home/jmagar/.claude/projects`
  - `/home/jmagar/.codex/sessions`
  - `/home/jmagar/.gemini`
- Added ACLs so container `uid 1001` can still access files without ownership takeover.
- Patched init scripts to avoid recursive ownership on agent home mounts.

Files changed:
- `docker/s6/cont-init.d/10-load-axon-env`
- `docker/s6/cont-init.d/15-fix-agent-home-ownership`

Result:
- host ownership remains stable after rebuild/restart.
- container can write test files under mounted session dirs.

### 2) `pulse_chat` stream loop correctness fix
Problem found:
- `sync_mode` could break on closed event channel without awaiting prompt task result, potentially hiding task errors.

Action:
- patched `None` branch to await/join `prompt_turn` and propagate errors.

File changed:
- `crates/web/execute/sync_mode.rs`

Result:
- improved error propagation correctness; did not resolve ACP initialize stall.

### 3) Added ACP runtime instrumentation
Actions in ACP service:
- added step logs for:
  - spawn adapter
  - transport ready
  - initialize sent
  - new/load session
  - prompt send/complete
- added IO task completion/failure logs.

File changed:
- `crates/services/acp.rs`

Result:
- isolated failure location to `conn.initialize(...).await`.
- no initialize response observed.

### 4) Mounted missing auth/config into workers
Problem found:
- workers only mounted session dirs, not auth/config files needed by adapters.

Action:
- added worker mounts:
  - `${HOST_HOME}/.claude.json -> /home/axon/.claude.json`
  - `${HOST_HOME}/.codex/auth.json -> /home/axon/.codex/auth.json`
  - `${HOST_HOME}/.codex/config.toml -> /home/axon/.codex/config.toml`

File changed:
- `docker-compose.yaml`

Result:
- resolved explicit codex config permission error previously seen once.
- initialize stall still persists for both agents.

### 5) ACP runtime executor swap attempt
Hypothesis:
- `futures::LocalPool` could be incompatible with ACP client internals expecting Tokio local runtime behavior.

Action:
- switched ACP blocking worker execution from `futures::LocalPool` to Tokio `current_thread + LocalSet`.

File changed:
- `crates/services/acp.rs`

Result:
- no behavioral change; still stalls after `initialize request sent`.

### 6) Initialize payload compatibility tweak
Action:
- added `client_info` (`Implementation::new("axon", <version>)`) to initialize request.

File changed:
- `crates/services/acp.rs`

Result:
- no behavioral change; still stalls at initialize.

### 7) WebSocket proxy resiliency improvement
Problem found:
- transient worker restart windows produced `ECONNREFUSED`/`ENOTFOUND` in web WS bridge.

Action:
- added short connection retry window in WS client bridge before hard fail.

File changed:
- `apps/web/lib/axon-ws-exec.ts`

Verification:
- `pnpm vitest run __tests__/axon-ws-exec.test.ts` passed (5 tests).

Result:
- mitigates restart-race failures; unrelated to ACP initialize deadlock.

## ACP Docs / Research Used
- Queried indexed ACP protocol docs via Axon (`query`, `retrieve`):
  - initialization/session-setup/prompt-turn lifecycle
- Retrieved adapter docs for:
  - `zed-industries/codex-acp`
  - `zed-industries/claude-agent-acp`

Findings:
- lifecycle expectations were already followed (`initialize -> session setup -> prompt`).
- docs did not provide a direct explanation for this deadlock mode.

## Rebuild/Runtime Status
- `axon-workers` rebuilt multiple times during this debugging pass.
- Current state at time of log:
  - `axon-workers`: healthy
  - `axon-web`: healthy

## Current Root-Cause Status
Confirmed:
- ACP adapters spawn.
- Transport setup completes.
- Initialize request is sent.

Unresolved:
- Initialize response never returns (both adapters).

Likely area:
- ACP client/transport interop mismatch at handshake level, not filesystem permissions, not web proxy availability.

## Remaining High-Value Next Steps
1. Capture raw stdio frames (stdin/out) during initialize to verify exact request/response wire behavior.
2. Compare against a known-good minimal ACP client implementation using the same adapter binary and same auth/config mounts.
3. Add explicit initialize timeout around `conn.initialize` with adapter process dump-on-timeout for deterministic diagnostics.
4. Test adapter binaries directly in-container with a tiny standalone ACP client binary/script to separate app integration vs adapter behavior.

## Files Modified During This Debug Cycle
- `apps/web/lib/axon-ws-exec.ts`
- `crates/services/acp.rs`
- `crates/web/execute/sync_mode.rs`
- `docker-compose.yaml`
- `docker/s6/cont-init.d/10-load-axon-env`
- `docker/s6/cont-init.d/15-fix-agent-home-ownership`
- (existing pending edits from parallel work also present in tree)
