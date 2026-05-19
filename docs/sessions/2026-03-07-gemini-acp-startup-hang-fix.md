# Session: Gemini CLI ACP Startup Hang — Root Cause & Fix

**Date:** 2026-03-07
**Branch:** `feat/services-layer-refactor`
**Duration:** ~2 hours (across context compactions)

## Session Overview

Diagnosed and fixed the Gemini CLI hanging when spawned as an ACP subprocess. The root cause was a filesystem permissions issue (`~/.gemini/` owned by UID 1001 from Docker) that caused `proper-lockfile` to fail silently with exponential retry backoff. After the fix, Gemini CLI successfully completes the ACP handshake with protocol version 1.

## Timeline

1. **Prior session** — Implemented Gemini as third ACP adapter (code in `acp.rs`, `sync_mode.rs`, `types.ts`). Gemini CLI hung on every spawn attempt.
2. **Resumed** — Found 3 zombie gemini processes from prior session; killed them.
3. **Injected debug logging** into `gemini.js:main()` — traced hang to `Promise.all([cleanupCheckpoints(), cleanupToolOutputFiles()])` (line ~215).
4. **Isolated Storage.initialize()** — confirmed it hangs independently.
5. **Traced to `proper-lockfile`** — `ProjectRegistry.getShortId()` calls `lock(registryPath)` which does `mkdir ~/.gemini/projects.json.lock`.
6. **Found EACCES** — `~/.gemini/` owned by UID 1001 (Docker axon user), `mkdir` returns permission denied, `proper-lockfile` retries 100x with 100ms backoff = infinite hang.
7. **Fixed ownership** — `sudo chown jmagar:jmagar ~/.gemini/`
8. **Verified ACP handshake** — Gemini responds with protocol version 1, agent info, capabilities.
9. **Added `GEMINI_FORCE_FILE_STORAGE`** to env allowlist in `acp.rs`.
10. **Cleaned up** — restored original `gemini.js`, removed backup, verified all tests pass.

## Key Findings

- **`~/.gemini/` ownership**: UID 1001 (Docker) not UID 1000 (jmagar). Caused by Docker containers mounting/writing to `~/.gemini/`. `proper-lockfile` v4.1.2 uses `mkdir` for locks — EACCES on parent dir = silent infinite retry. (`projectRegistry.js:98`)
- **Gemini ACP protocolVersion**: Must be a **u16 integer** (e.g., `1`), not a semver string. Zod schema: `z.number().int().gte(0).lte(65535)`. Rust SDK `ProtocolVersion::LATEST = V1 = 1` is correct. (`agent-client-protocol-schema-0.10.8/src/version.rs:26`)
- **Gemini `isInteractive()` in ACP mode**: Returns `true` even with piped stdio. Takes the interactive auth branch, uses `security.auth.selectedType` from `~/.gemini/settings.json` (`oauth-personal`). Does NOT require `GEMINI_API_KEY`. (`gemini.js:270-276`)
- **Gemini startup flow**: `main()` → `patchStdio` → `cleanupCheckpoints` (HANG POINT) → `parseArguments` → `loadCliConfig` → `refreshAuth` → `runDeferredCommand` → sandbox check → `setupTerminalAndTheme` → `initializeApp` → `runZedIntegration` (ACP transport). (`gemini.js:185-452`)
- **Exit code 144**: SIGUSR1 (128+16) propagated from gemini Node.js process to parent process group. Caused cascading shell deaths during debugging.

## Technical Decisions

- **Ownership fix over workaround**: Could have set `GEMINI_CLI_HOME` to a different dir, but fixing the actual ownership is the correct long-term fix since Gemini CLI expects `~/.gemini/`.
- **Added `GEMINI_FORCE_FILE_STORAGE`** to env allowlist: Ensures Gemini skips keychain (libsecret) and uses encrypted file storage when set. Defensive measure for headless environments.
- **Did not add `GEMINI_API_KEY`**: User confirmed OAuth should work without API key ("we shouldnt need an api key if we're authed"). OAuth with cached credentials works correctly.

## Files Modified

| File | Change |
|------|--------|
| `crates/services/acp.rs:100` | Added `GEMINI_FORCE_FILE_STORAGE` to spawn_adapter env allowlist |
| `~/.gemini/` (system) | Fixed ownership from UID 1001 to jmagar:jmagar |
| `memory/gemini-acp.md` (memory) | Created detailed reference for Gemini ACP integration |

## Commands Executed

| Command | Purpose | Result |
|---------|---------|--------|
| `sudo chown jmagar:jmagar ~/.gemini/` | Fix directory ownership | Success |
| `pkill -9 -f "gemini"` | Kill zombie processes | Cleared 3+ hanging processes |
| `node /tmp/test-gemini-acp-final.js` (via nohup) | ACP handshake test | Protocol v1 initialized, agent info returned |
| `cargo check` | Verify compilation | Clean |
| `cargo clippy --lib` | Lint check | Clean |
| `cargo test --lib -- acp` | ACP unit tests | 14 passed |
| `cargo test --test services_acp_spawn_env` | Integration tests | 4 passed |

## Behavior Changes (Before/After)

| Behavior | Before | After |
|----------|--------|-------|
| `gemini --experimental-acp` (piped stdio) | Hangs indefinitely in `cleanupCheckpoints()` | Completes startup in ~4s, accepts ACP JSON-RPC |
| ACP initialize response | None (hang) | `{"protocolVersion":1,"agentInfo":{"name":"gemini-cli","version":"0.32.1"}}` |
| Gemini MCP servers | Never reached | Loads configured servers (swag, neo4j-memory, chrome-devtools, etc.) |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | Clean compilation | `Finished dev profile` | PASS |
| `cargo clippy --lib` | No warnings | `Finished dev profile` | PASS |
| `cargo test --lib -- acp` | 14 tests pass | `14 passed; 0 failed` | PASS |
| `cargo test --test services_acp_spawn_env` | 4 tests pass | `4 passed; 0 failed` | PASS |
| ACP initialize (protocolVersion: 1) | JSON-RPC response | `{"result":{"protocolVersion":1,...}}` | PASS |
| `stat ~/.gemini/` | Owned by jmagar | `jmagar:jmagar 775` | PASS |
| `mkdir ~/.gemini/projects.json.lock` | No EACCES | `mkdir OK` | PASS |

## Risks and Rollback

- **Ownership fix**: Low risk. Only changes `~/.gemini/` top-level dir. Docker containers using UID 1001 may re-create the problem if they write to `~/.gemini/` again. Monitor after Docker rebuilds.
- **`GEMINI_FORCE_FILE_STORAGE` env passthrough**: No risk. Only passed if set in environment. No behavior change unless explicitly configured.

## Decisions Not Taken

- **Setting `GEMINI_API_KEY` as workaround**: Rejected because OAuth works correctly once the permission issue is fixed. API key auth is unnecessary.
- **Changing `GEMINI_CLI_HOME`**: Rejected because it fragments config. Better to fix ownership on the canonical path.
- **Adding PTY allocation for Gemini**: Rejected because ACP protocol uses piped stdio (JSON-RPC), not terminal I/O. Gemini correctly handles `isInteractive()=true` in ACP mode.
- **Patching Gemini's `proper-lockfile` error handling**: Rejected — upstream fix needed. The library should fail fast on EACCES, not retry indefinitely.

## Open Questions

- **Docker UID 1001 writes**: What Docker operation originally set `~/.gemini/` to UID 1001? Need to prevent recurrence. Likely a bind mount from a container running as the `axon` user.
- **Exit code 144 (SIGUSR1)**: Gemini's Node.js process sends SIGUSR1 to the process group during startup. This killed parent shells during debugging. Need to understand why — possibly Node's debugger auto-attach signal.
- **Gemini sandbox mode**: After auth, Gemini checks `!process.env['SANDBOX']` and may try to `readStdin()` (blocking) before entering sandbox. When spawned as ACP subprocess with piped stdin, this could be a second hang point if the sandbox path is taken. Currently not triggered because ACP mode bypasses sandbox.

## Next Steps

1. **End-to-end Pulse UI test** — Send a prompt through the web UI with `agent: "gemini"` and verify full round-trip (prompt → ACP → Gemini → response → UI).
2. **Model selection** — Verify `read_gemini_default_model()` and `append_gemini_model_override()` work with Gemini's `getConfigOptions` / `setConfigOption` ACP methods.
3. **Session persistence** — Test `loadSession` capability (Gemini reports `loadSession: true`).
4. **Docker ownership guard** — Add a healthcheck or startup script that verifies `~/.gemini/` ownership before spawning Gemini.
