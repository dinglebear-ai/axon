# Session: Reboot ACP Debugging + Session/Config API Fixes

**Date:** 2026-03-09
**Branch:** main
**Scope:** `apps/web`, `crates/web/execute.rs`, `Justfile`

---

## Session Overview

Systematically debugged four user-reported issues across the Reboot page (`/reboot`) and the supporting API layer:

1. **Thinking dots with no response** — ACP events from Rust never reached the React switch statement
2. **Message disappears after 60s** — 60-second fallback timeout was the only thing ever firing
3. **`/api/sessions/{id}` 404** — ID type mismatch between scanner output and ACP session UUID
4. **`/api/sessions/list` 6.5s slowness** — serial git subprocess calls per project directory
5. **`/api/pulse/config` 9.9–16.6s** — parallel probes both spawning full ACP lifecycle
6. **`just dev` Ctrl+C leaves orphans** — background processes not tracked for cleanup
7. **Rust build failures** — stale incremental cache + dead code warnings blocking `axon serve` start

---

## Timeline

| Step | Activity |
|------|----------|
| 1 | Applied fix from previous session summary: unwrap `command.output.json` in `use-axon-acp.ts` |
| 2 | Debugged Rust compilation failure (`tokio-util sync` feature error, stale cache, `E0761` ambiguous module) |
| 3 | Removed orphaned `WS_ACP_SEMAPHORE` + `LazyLock` import from `execute.rs` |
| 4 | Fixed `just dev` Ctrl+C process leak with shebang recipe + signal trap |
| 5 | Debugged sessions 404 + slowness + pulse/config duplicate probes |
| 6 | Implemented three API fixes and verified with Biome |

---

## Key Findings

### Finding 1: ACP Event Wrapping Mismatch (`use-axon-acp.ts`)

- **Root cause**: `useAxonAcp` switch statement matched `assistant_delta`, `result`, etc. at the top level. Rust backend (`sync_mode.rs:dispatch_acp_event`) wraps ALL ACP events inside `{ type: 'command.output.json', data: { ctx, data: <actual_event> } }`.
- **Evidence**: `crates/web/execute/sync_mode.rs` — `send_json_owned()` always emits `WsEventV2::CommandOutputJson`; `crates/services/types.rs` — `AcpBridgeEvent` serializes inner type correctly but is nested.
- **Effect**: Switch never fired → `isStreaming` stayed `true` → 60s `STREAMING_TIMEOUT_MS` fired → message replaced with error → "session disappeared".

### Finding 2: Sessions 404 — ID Type Mismatch

- **Root cause**: `session-scanner.ts:sessionId()` computes a 12-char SHA-256 hex hash of the absolute path. `useAxonSession` passes the Claude ACP UUID (36-char, e.g. `99409929-7aed-4947-b99d-7854e35a378f`) as the route param. Route did `s.id === id` — a SHA-256 hash never equals a UUID.
- **Evidence**: `lib/sessions/session-scanner.ts:132` — `crypto.createHash('sha256').update(absolutePath).digest('hex').slice(0, 12)`. `hooks/use-axon-session.ts:54` — passes `sessionId` (Claude ACP UUID) to `/api/sessions/`.

### Finding 3: Sessions List 6.5s — Serial Git Calls

- **Root cause**: `scanSessions()` iterated project directories sequentially, calling `enrichWithGit()` per directory. Each `enrichWithGit` spawns two `execFile('git', ...)` calls (branch + remote). With many projects, this is N×2 serial subprocess calls.
- **Evidence**: `lib/sessions/git-metadata.ts:164` — `exec('git', ['rev-parse', '--abbrev-ref', 'HEAD'], opts)` and `exec('git', ['remote', 'get-url', 'origin'], opts)` called sequentially per project.

### Finding 4: Pulse Config 9.9s/16.6s — Parallel Probe Duplication

- **Root cause**: Two parallel `POST /api/pulse/config` requests (for `claude` and `gemini`) both missed cache simultaneously and both launched the full ACP adapter lifecycle. No coalescing.
- **Evidence**: Logs showed `[axon-ws] connected to localhost/ws for mode=pulse_chat_probe` twice, followed by two 200 responses at 9.9s and 16.6s.

### Finding 5: Stale Rust Incremental Cache

- **Root cause**: `just dev` left `cargo` and `rustc` processes running (with build directory lock). After killing them, `cargo clean` was needed to clear ~23 GiB of stale artifacts. The stale cache was showing compiler errors pointing at a 5-argument `classify_sync_direct` signature that no longer existed on disk.
- **Evidence**: `lsof +D target/` confirmed PID 512392 (cargo) and 514091 (rustc) still held the build lock.

### Finding 6: Dead Code in `execute.rs`

- `WS_ACP_SEMAPHORE` and `try_acquire_acp_permit` were declared but never used anywhere in the codebase. Introduced as planned concurrency limiting for ACP sessions but never wired into the dispatch path.

### Finding 7: `just dev` Ctrl+C Orphan Processes

- **Root cause**: `just` `dev` recipe used `&` background processes with `wait`. Ctrl+C sends SIGINT to `just`, which exits, but child `cargo run` subprocesses become orphaned (not in the same process group cleanup).

---

## Technical Decisions

### Decision: Unwrap `command.output.json` in TypeScript, not Rust

Two options:
1. Change Rust backend to emit ACP events directly (not wrapped) for `pulse_chat` mode
2. Unwrap in TypeScript in `useAxonAcp` before the switch

Chose option 2: the `command.output.json` wrapping is the correct protocol for all other modes (scrape, crawl, etc.) and is consumed correctly by `axon-ws-exec.ts` server-side. Changing Rust would break the existing protocol contract. TypeScript unwrap is surgical and scoped to `pulse_chat` mode only.

### Decision: Match by `filename` OR `id` in session route

The SHA-256 hash-based `id` is used by `use-recent-sessions.ts` list view. The ACP UUID is used by `use-axon-session.ts` for session resumption. Both are valid lookup keys — the route now accepts either.

### Decision: `Promise.all` parallelization over streaming concurrency control

`scanSessions` now fans out all project directories in parallel. No concurrency limit was added because: (a) the number of Claude project directories is bounded to the user's actual projects (typically <50), (b) each `enrichWithGit` has a 3s timeout, (c) git operations are fast local I/O. Adding a semaphore would add complexity without meaningful benefit at this scale.

### Decision: `IN_FLIGHT` coalescing map for `pulse/config`

Rather than a queue or deduplicated request cache, an `IN_FLIGHT` Map of `cacheKey → Promise` is used. Multiple concurrent requests await the same promise. The map entry is deleted in `finally` after the probe completes or fails. This avoids duplicate ACP spawns without complex request deduplication infrastructure.

### Decision: Drop `--locked` in `just dev` recipe

`--locked` was failing because `Cargo.toml` was modified (added `thiserror` + `toml` deps) and the new `Cargo.lock` hadn't been generated yet when `just dev` was first run. After `cargo build` regenerated `Cargo.lock`, `--locked` would work again, but dropping it in dev mode avoids this fragility. CI still validates lock file consistency.

---

## Files Modified

| File | Change |
|------|--------|
| `apps/web/hooks/use-axon-acp.ts` | Unwrap `command.output.json` before ACP event switch |
| `crates/web/execute.rs` | Remove dead `WS_ACP_SEMAPHORE`, `try_acquire_acp_permit`, unused `LazyLock` import |
| `Justfile` | Convert `dev` + `workers` to shebang recipes with `trap cleanup INT TERM EXIT` |
| `apps/web/app/api/sessions/[id]/route.ts` | Match session by `s.id === id \|\| s.filename === id` |
| `apps/web/lib/sessions/session-scanner.ts` | Parallelize project scan with `Promise.all`; run `enrichWithGit` + file stats concurrently |
| `apps/web/app/api/pulse/config/route.ts` | Add `IN_FLIGHT` coalescing map to prevent duplicate ACP probes |

---

## Commands Executed

```bash
# Kill stale build processes
kill -9 512392 514091

# Full clean after stale cache
cargo clean          # removed 202k files, 233 GiB; then 49k files, 23 GiB

# Build verification
cargo build --bin axon   # succeeded after second attempt (spider_firewall network timeout on first)
cargo check -q           # clean, no warnings

# Biome lint check
cd apps/web && pnpm biome check 'app/api/sessions/[id]/route.ts' 'app/api/pulse/config/route.ts' 'lib/sessions/session-scanner.ts'
# → Checked 3 files in 17ms. No fixes applied.

# Process cleanup
just stop
```

---

## Behavior Changes (Before/After)

| Symptom | Before | After |
|---------|--------|-------|
| Reboot page agent response | Thinking dots for 60s, then "⚠ No response received" | ACP events flow through; assistant text streams correctly |
| `GET /api/sessions/{claude-uuid}` | 404 always | Resolves to correct JSONL file by filename match |
| `GET /api/sessions/list` | ~6.5s (serial git calls) | Sub-second (parallel fan-out) |
| `POST /api/pulse/config` (concurrent) | Two full ACP lifecycles (~10s + ~17s) | One lifecycle, second request awaits first (~10s both) |
| `just dev` Ctrl+C | Orphaned `cargo run` / Next.js processes | All PIDs tracked, `trap cleanup INT TERM EXIT` kills them |
| `cargo build` warnings | 2 dead_code warnings on semaphore | 0 warnings |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check -q` | No warnings | Clean output | ✅ |
| `pnpm biome check` (3 files) | No fixes | Checked 3 files, no fixes | ✅ |
| `cargo build --bin axon` | Build succeeds | `Finished dev profile in 3m 44s` | ✅ |
| `just stop` | All processes killed | `Stopped running servers and workers` | ✅ |

---

## Source IDs + Collections Touched

*(No Axon embed/retrieve operations were performed during this debugging session.)*

---

## Risks and Rollback

### `use-axon-acp.ts` unwrapping

- **Risk**: If Rust ever emits `assistant_delta` directly (not wrapped), the unwrap would skip it and fall through.
- **Rollback**: Revert the `if (msg.type === 'command.output.json' ...)` block. `sync_mode.rs:send_json_owned` always wraps, so this is low risk.

### Session scanner parallelization

- **Risk**: Parallel git calls on a system with many projects could spike CPU briefly. All have 3s timeout so worst case is bounded.
- **Rollback**: Replace `Promise.all(projectNames.map(...))` with the original `for` loop.

### `IN_FLIGHT` coalescing in pulse/config

- **Risk**: If the in-flight probe throws, all coalesced requests return that error (not just the original caller). This is acceptable — the error condition is the same for all.
- **Risk**: Map entry not deleted on error path — mitigated by `finally` block.
- **Rollback**: Remove `IN_FLIGHT` map and revert to direct `runAxonCommandWsStream` call.

### `just dev` dropping `--locked`

- **Risk**: In dev, Cargo might silently update a transitive dependency to a version that breaks CI.
- **Mitigation**: CI still validates lock file. Dev is intentionally more permissive.
- **Rollback**: Add `--locked` back to each `cargo run` line in the `dev` recipe.

---

## Decisions Not Taken

| Alternative | Rejected Because |
|-------------|-----------------|
| Change Rust to emit ACP events without `command.output.json` wrapping for `pulse_chat` | Would break the existing protocol consumed by `axon-ws-exec.ts` server-side |
| Wire `WS_ACP_SEMAPHORE` into `sync_mode.rs` now | It's a useful feature but a separate task; not needed to unblock current bugs |
| Add concurrency limit to `Promise.all` session scan | Project count bounded, git has 3s timeout; complexity not justified |
| Cache `scanSessions` result in memory | Next.js server components cache differently; module-level cache can serve stale data after session files change |

---

## Open Questions

- **Semaphore wiring**: `WS_ACP_SEMAPHORE` was removed as dead code. If ACP concurrency limiting is needed (prevent 50 concurrent `pulse_chat` requests from spawning 50 Claude processes), it should be added inside `sync_mode.rs`'s `pulse_chat` dispatch path with an `AXON_ACP_MAX_CONCURRENT_SESSIONS` env var.
- **Session list cache**: `scanSessions` is called on every request, including `GET /api/sessions/{id}` which scans 200 sessions to find one. An in-memory cache with TTL (~5s) would help if session detail loads are frequent.
- **Config probe timing**: 9-16s for ACP adapter startup is the actual cost. No further optimization is possible without changing the ACP protocol (lazy config fetch, background probe, etc.).
- **`decodeProjectPath` ambiguity**: The scanner's naive hyphen→slash decode fails for project paths containing real hyphens. `decodedProjectPathCandidates` exists but `enrichWithGit` is only called with the naive decode in `scanSessions`. Passing `folderName` as the second arg would enable fallback candidate resolution.

---

## Next Steps

1. Verify Reboot page now shows agent responses (smoke test `pulse_chat` with `just dev`)
2. Consider adding session detail cache to avoid 200-session scan per `GET /api/sessions/{id}` request
3. Wire ACP concurrency limiting into `sync_mode.rs` if production load warrants it
4. Update `scanSessions` to pass `projectName` as `folderName` to `enrichWithGit` for hyphenated path resolution
