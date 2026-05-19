# Session: Clippy Lint Fixes + Logs API 2-Minute Response Bug

**Date**: 2026-03-09
**Branch**: `refactor/acp-performance-modern-rust`

---

## Session Overview

Two independent tasks:

1. **Clippy lint fixes** — resolved 3 remaining `-D warnings` errors that existed in the codebase after the v0.13.1 push: two collapsible-if patterns in `crates/mcp/server/artifacts/path.rs` and one too-many-arguments violation in `crates/web/execute/sync_mode/dispatch.rs`.

2. **Logs API 2-minute response bug** — diagnosed and fixed `GET /api/logs?service=all&tail=100` taking 2.1 minutes (compile: 675ms, proxy: 2ms, render: 2.1min). Root cause: `follow: true` in the Dockerode `.logs()` call kept all 7 container streams open indefinitely. Fix: `follow: false`.

---

## Timeline

1. **User reported** `GET /api/logs?service=all&tail=100 200 in 2.1min` timing breakdown
2. **Explored** `apps/web/app/api/logs/route.ts` to identify the root cause
3. **Diagnosed** `follow: true` on line 44 as the cause — equivalent to `docker logs -f`, streams forever
4. **Fixed** `follow: true` → `follow: false` (one character change)
5. **Clippy fixes** (prior task in same session):
   - `path.rs:110` + `path.rs:144`: collapsed `if let Ok(...) { if ... }` into single expression
   - `dispatch.rs:48`: grouped `limit`/`offset`/`max_points` into `QueryPagination` struct to drop below 8-arg limit

---

## Key Findings

- `apps/web/app/api/logs/route.ts:44` — `follow: true` is `docker logs -f`; the stream never emits `end` while containers are running. The response gate (`activeStreams <= 0` at line 111) only closes when ALL 7 streams end — which never happens naturally.
- The 2.1-minute render time exactly matches browser TCP socket timeout (~2 minutes), not an application-level timeout. No timeout is set anywhere in the route.
- `crates/mcp/server/artifacts/path.rs:110,144` — clippy `collapsible_if` lint: `if let Ok(x) = ... { if x.method() { ... } }` can be written as `if expr.map(...).unwrap_or(false) { ... }`.
- `crates/web/execute/sync_mode/dispatch.rs:48` — clippy `too_many_arguments` lint (8/7 limit). `limit`, `offset`, `max_points` are logically related pagination fields — natural grouping into `QueryPagination` struct.

---

## Technical Decisions

### `follow: false` — snapshot semantics
`follow: true` is designed for live log tailing (persistent connection). The `/api/logs` route is a **snapshot** endpoint — it fetches the last N lines and returns. `follow: false` tells Docker to send buffered logs and then close the stream. The `pt.on('end')` handler fires immediately, `activeStreams` reaches zero, and the SSE response closes in milliseconds.

### `QueryPagination` struct for dispatch.rs
The 8-arg limit exists to catch functions that are doing too much. Rather than using `#[allow(clippy::too_many_arguments)]`, grouping the three pagination scalars into a named struct is semantically correct — they always travel together and represent a single concept.

### Collapsible-if: `.map().unwrap_or(false)` over nested if-let
The symlink check pattern `if let Ok(lstat) = ... { if lstat.file_type().is_symlink() }` is a guard — it only rejects on confirmed symlink. Converting to `.map(|m| m.file_type().is_symlink()).unwrap_or(false)` preserves identical semantics: `Err` (no metadata) → `false` → no rejection.

---

## Files Modified

| File | Change |
|------|--------|
| `apps/web/app/api/logs/route.ts` | `follow: true` → `follow: false` on line 44 |
| `crates/mcp/server/artifacts/path.rs` | Collapsed two nested if-let+if patterns (lines 110, 144) |
| `crates/web/execute/sync_mode/dispatch.rs` | Introduced `QueryPagination` struct; updated call site |

---

## Commands Executed

```bash
cargo clippy -- -D warnings 2>&1 | grep "error:|  --> "
# → 3 errors:
#   path.rs:110  — collapsible_if
#   path.rs:144  — collapsible_if
#   dispatch.rs:48 — too_many_arguments (8/7)

cargo clippy -- -D warnings 2>&1 | grep "^error"
# → (empty after fixes) — clean
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| `GET /api/logs?service=all` render time | 2.1 minutes (browser TCP timeout) | ~milliseconds (stream closes after last log line) |
| `GET /api/logs?service=<single>` render time | Same issue, proportionally faster but same root cause | Fixed |
| `cargo clippy -- -D warnings` | 3 errors, fails to compile | 0 errors, clean |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo clippy -- -D warnings` | 0 errors | 0 errors | ✅ |
| Manual timing of `/api/logs` | < 1s | Not re-tested post-fix (local dev) | ⚠️ untested |

---

## Source IDs + Collections Touched

_(Populated after Axon embed)_

---

## Risks and Rollback

- **`follow: false`** — changes the endpoint from a "start streaming live logs" SSE endpoint to a "fetch snapshot" SSE endpoint. If any client expected a persistent live-tail stream from this route, it will no longer receive new log lines after the initial tail. Based on `logs-viewer.tsx` usage (reads until `done`, then disconnects), this is snapshot-only — no regression risk.
- **Rollback**: revert `follow: false` → `follow: true` in `route.ts:44`. One-line revert. No DB or schema changes.

---

## Decisions Not Taken

- **Add a server-side timeout** — `setTimeout(() => { for (const s of logStreams) s.destroy() }, 30_000)` would have been a workaround, not a fix. The real issue is semantic: snapshot vs. live stream. `follow: false` is correct.
- **Add `#[allow(clippy::too_many_arguments)]`** — suppresses the lint without improving the code. The `QueryPagination` struct is better because it documents intent and is reusable.
- **Live log streaming** — if a true live-tail feature is needed in the future, it should be a separate WebSocket endpoint (`/ws/logs`), not an SSE route that buffers all 7 container streams.

---

## Open Questions

- No client-side regression testing was performed on `logs-viewer.tsx` after the `follow: false` change. The component reads until `done` — this should work correctly with a snapshot response, but untested.

---

## Next Steps

- Address 6 GitHub dependabot advisories on main branch (pre-existing, flagged in previous session)
- Consider `ingest_youtube_playlist` function split (89 lines, monolith warns at 80)
