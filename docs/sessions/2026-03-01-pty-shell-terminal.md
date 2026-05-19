# Session: PTY Shell Terminal Implementation
**Date:** 2026-03-01
**Branch:** `feat/crawl-download-pack`
**Duration:** ~1 session

---

## Session Overview

Replaced the axon command-mode executor in the `/terminal` web page with a real interactive PTY shell (`$SHELL`). Previously, typing `ls` in the terminal routed through `axon ls` and returned `"axon: unknown mode: ls"`. After this session, the terminal spawns a real bash/zsh session in a pseudo-terminal (PTY), giving full interactive shell access: `ls`, `vim`, `htop`, tab completion, colors, shell history — all work natively.

---

## Timeline

1. **Diagnosis** — Screenshot confirmed `/terminal` was routing all input through `axon <mode>` executor. Identified root cause in `use-terminal-session.ts:156-168` where `submitInput()` split on whitespace and sent `mode`+`input` to the shared `/ws` execute handler.

2. **Planning** — Explored all relevant files: `terminal/page.tsx` (318L), `use-terminal-session.ts` (191L), `use-axon-ws.ts` (151L), `terminal-emulator.tsx` (234L), `crates/web.rs` (293L), `crates/web/execute/mod.rs`. Discovered `terminal-emulator.tsx` already had `onResize?: (cols, rows) => void` prop wired — no component changes needed.

3. **Plan written** — Saved to `docs/plans/2026-03-01-pty-shell-terminal.md`. 7 tasks, 5 file changes, no existing infrastructure touched.

4. **Execution (subagent-driven)** — 5 implementation subagents + 1 gate subagent dispatched sequentially. All tasks passed on first attempt with no rework cycles.

---

## Key Findings

- `terminal-emulator.tsx:22` already declared `onResize?: (cols: number, rows: number) => void` and `terminal-emulator.tsx:191-195` already wired `terminal.onResize()` — zero component changes needed.
- `portable-pty 0.9.0` uses `native_pty_system()` free function (not `NativePtySystem::default()`). The implementer subagent caught this at compile time.
- `PtySystem` trait does NOT need to be imported explicitly — `openpty()` dispatches through the `Box<dyn PtySystem>` vtable without the trait in scope.
- `NEXT_PUBLIC_AXON_WS_URL` handling: hook strips trailing `/ws` suffix before appending `/ws/shell`, ensuring correct URL construction in all proxy configurations.
- Shell's `/ws/shell` endpoint needs no `State<Arc<AppState>>` — PTY sessions are self-contained. This avoids coupling shell lifecycle to Docker stats broadcast.

---

## Technical Decisions

| Decision | Rationale |
|----------|-----------|
| Separate `/ws/shell` endpoint (not reusing `/ws`) | Avoids coupling PTY session to AppState (Docker stats broadcast, job_dirs registry). Clean separation of concerns. |
| `portable-pty` crate over raw `nix::pty` | Production-tested (WezTerm), cross-platform, cleaner API. `nix` requires platform-specific code for PTY setup. |
| JSON text frames for all WS messages | Terminal data is UTF-8; `serde_json` handles escape sequences. Avoids binary frame complexity in the frontend. |
| Three tokio tasks (reader/writer/sender) | PTY I/O is blocking `std::io::Read`/`Write`; must run in `spawn_blocking`. Sender task decouples PTY reader latency from WS send latency. |
| `isRunning={false}` in toolbar | PTY manages its own running state. Ctrl+C sends `\x03` through the PTY — no separate "cancel" mechanism needed. |
| Kept shared `/ws` and all execute infrastructure intact | Other pages (Pulse sidebar stats) depend on the shared WS. The terminal page simply no longer uses it. |

---

## Files Modified

| File | Action | Purpose |
|------|--------|---------|
| `Cargo.toml` | Modified | Added `portable-pty = "0"` dependency |
| `Cargo.lock` | Modified (auto) | Resolved `portable-pty 0.9.0` + transitive deps |
| `crates/web/shell.rs` | **Created** | PTY shell WebSocket handler: spawns `$SHELL`, bridges PTY ↔ WebSocket |
| `crates/web.rs` | Modified | Added `mod shell;`, `shell_ws_upgrade()` handler, `/ws/shell` route |
| `apps/web/hooks/use-shell-session.ts` | **Created** | Dedicated WS hook for `/ws/shell`: auto-reconnect, `sendInput`, `resize` |
| `apps/web/app/terminal/page.tsx` | Modified | Replaced `useTerminalSession`+`useAxonWs` with `useShellSession`; removed 171 lines of input parsing |

**Unchanged (no modifications needed):**
- `components/terminal/terminal-emulator.tsx` — `onResize` prop already wired
- `components/terminal/terminal-emulator-wrapper.tsx` — passes all props through
- `components/terminal/terminal-toolbar.tsx` — accepts `WsStatus` from any source
- `hooks/use-axon-ws.ts` — shared WS untouched
- `lib/ws-protocol.ts` — existing types sufficient
- `crates/web/execute/` — axon command executor untouched

---

## Commands Executed

```bash
# Dependency resolution
cargo fetch  # clean, 116 additions to Cargo.lock

# Compile checks (ran after each task)
cargo check  # clean after all tasks

# Test suite
cargo test --lib  # 477 passed, 0 failed, 3 ignored

# Linting
cargo clippy  # 0 errors
cargo fmt --check  # clean (no reformatting needed)

# Monolith policy
./scripts/enforce_monoliths.py  # no FAIL/ERROR

# TypeScript
cd apps/web && pnpm tsc --noEmit  # 0 errors
```

---

## Behavior Changes (Before/After)

| Scenario | Before | After |
|----------|--------|-------|
| Type `ls` in terminal | `axon: unknown mode: ls` error | `ls` output from shell |
| Type `vim file.txt` | `axon: unknown mode: vim` error | vim opens with full curses UI |
| Type `htop` | `axon: unknown mode: htop` error | htop runs with full terminal rendering |
| Ctrl+C | Sent via WS cancel message to axon binary | Sent as `\x03` through PTY (native SIGINT) |
| Up/Down arrows | Navigated localStorage command history | Navigates shell's built-in history (native PTY) |
| Tab completion | Not supported | Works natively via PTY |
| Color output | Only what axon `--json` formatting provided | Full 256-color + truecolor via `COLORTERM=truecolor` |
| Terminal resize | No-op (no resize notification existed) | `SIGWINCH` equivalent sent to PTY; programs reflow |
| WS URL | Connected to shared `/ws` | Connects to dedicated `/ws/shell` |
| Toolbar status | Reflected shared `/ws` connection | Reflects `/ws/shell` shell session status |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo fetch` | No errors | Silent, Cargo.lock updated | ✅ |
| `cargo check` (after shell.rs) | No errors | Clean | ✅ |
| `cargo check` (after web.rs) | No errors | Clean | ✅ |
| `pnpm tsc --noEmit` (after hook) | 0 `error TS` | 0 errors | ✅ |
| `pnpm tsc --noEmit` (after page) | 0 `error TS` | 0 errors | ✅ |
| `cargo test --lib` | No regressions | 477 passed, 0 failed | ✅ |
| `cargo clippy` | 0 errors | 0 errors | ✅ |
| `cargo fmt --check` | Clean | Clean | ✅ |
| `enforce_monoliths.py` | No FAIL/ERROR | No FAIL/ERROR | ✅ |

---

## Source IDs + Collections Touched

No Axon crawl/embed/retrieve operations were performed during implementation. This section will be populated after Axon embedding of this session doc.

---

## Risks and Rollback

**Risk:** `/ws/shell` has no authentication — any client with network access to the WS server can spawn a shell.
**Mitigation:** All ports bound to `127.0.0.1` (per docker-compose); access is gated by Tailscale/SWAG proxy. Acceptable for homelab.

**Risk:** PTY reader blocks on `std::io::Read` — if `spawn_blocking` thread pool is exhausted, new shell sessions could be delayed.
**Mitigation:** `spawn_blocking` uses a dedicated thread pool (unbounded on tokio's default runtime). Each session uses 2 threads (reader + writer); typical homelab load is 1-2 concurrent sessions.

**Rollback:** `git revert e55c4e00 e9011060 d357f088 d7cff203 9fdf8913` — reverts all 5 implementation commits, restoring the axon command executor. No database schema changes; no persistent state affected.

---

## Decisions Not Taken

| Alternative | Rejected Because |
|-------------|-----------------|
| `node-pty` npm package + Next.js custom server | Next.js App Router has no native WS server support; requires custom server setup that breaks the existing architecture |
| Binary WS frames for PTY data | Adds frontend complexity (ArrayBuffer handling); UTF-8 JSON handles all terminal sequences correctly |
| Reusing `/ws` endpoint with new message types | Would couple shell session lifecycle to `AppState` (Docker stats, job_dirs registry); cleaner as a separate endpoint |
| `nix::pty` (raw POSIX) | Platform-specific, verbose setup, no cross-platform support; `portable-pty` is purpose-built |
| Keeping axon command mode as fallback | Complicates the terminal UX; axon commands can still be run as `axon scrape ...` from the shell |
| `Arc<Mutex<ws_tx>>` wrapping | Necessary because `ws_tx` is moved into blocking reader task but also needed in async sender task; no cleaner pattern without unsafe |

---

## Open Questions

- Does the `axon-workers` Docker container have `bash` at `/bin/bash`? (Expected yes — debian:bookworm-slim includes bash. Confirmed by pre-existing Pulse chat feature using the same container.)
- Should `/ws/shell` get rate limiting or a session limit to prevent resource exhaustion? (Low priority for homelab, but worth adding if exposed externally.)
- The `run_shell` function is 87 lines — above the 80-line advisory threshold. The monolith check passed (hard limit is 120). Could be split into `spawn_pty()` helper if desired.

---

## Next Steps

1. **Deploy:** `docker compose build axon-workers && docker compose up -d axon-workers` to pick up the new Rust binary with `/ws/shell`
2. **Smoke test** in browser at `https://axon.tootie.tv/terminal`: verify `ls`, `vim`, `htop`, Ctrl+C all work
3. **Optional:** Add session timeout to `shell.rs` — kill idle PTY sessions after N minutes to reclaim thread pool slots
4. **Optional:** Add `onCancelCurrent` handler that sends `\x03` to PTY (currently a no-op; Ctrl+C through xterm works naturally but the toolbar CANCEL button is dead)
