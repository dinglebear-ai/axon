# Terminal WebSocket Fix + Transparency Debug Session
**Date:** 2026-03-07
**Branch:** feat/services-layer-refactor

---

## Session Overview

Debugged and fully fixed the Reboot UI terminal component. The terminal was permanently stuck on "RECONNECTING..." due to a 403 from the Rust WebSocket server. Additionally resolved terminal background transparency issues and verified full end-to-end PTY operation via Chrome DevTools MCP.

---

## Timeline

1. **Continued from prior session** — prior work had added `process.loadEnvFile()` to `shell-server.mjs` and changed `use-shell-session.ts` to bypass the Turbopack WS proxy by connecting directly to port 49000 (Rust serve).
2. **Opened Chrome DevTools MCP** — navigated to `http://dookie:49010/reboot`, opened terminal drawer.
3. **Identified 403 root cause** — console showed `ws://dookie:49000/ws/shell?token=...` failing with `Unexpected response code: 403`.
4. **Traced to `shell_ws_upgrade` in `crates/web.rs`** — handler only allowed loopback IPs, rejected all remote browser connections.
5. **Fixed Rust handler** — added token-based auth path for non-loopback connections (same `AXON_WEB_API_TOKEN` credential used by `/ws`).
6. **Rebuilt and restarted `axon serve`** — killed old PID 2612150, started new binary on port 49000.
7. **Confirmed CONNECTED status** — terminal showed green "CONNECTED" dot and shell prompt `jmagar@dookie ~ `.
8. **Investigated background transparency** — walked DOM ancestry, confirmed all xterm elements have `rgba(0,0,0,0)` computed bg. Dark appearance is the glassmorphism dialog container (`rgba(3,7,18,0.22)`) over the dark page background (`#030817`) — correct and intentional.
9. **Verified end-to-end via WS injection** — intercepted WebSocket constructor, sent `echo DEVTOOLS_TEST\r` directly through the captured `__shellWs`, observed output in terminal.
10. **Confirmed `ls /tmp | head -5`** — live PTY output returned, new prompt appeared.

---

## Key Findings

| Finding | Location | Detail |
|---------|----------|--------|
| `shell_ws_upgrade` loopback-only restriction | `crates/web.rs:198` | Only checked `addr.ip().is_loopback()`. Browser at `dookie` is NOT loopback → 403. |
| `TerminalLoadingPlaceholder` dark bg | `terminal-emulator-wrapper.tsx:52` | `style={{ background: '#030712' }}` — visible while xterm.js bundle loads. Not the steady-state issue. |
| All xterm elements transparent | Chrome DevTools eval | `rgba(0,0,0,0)` on `.xterm`, `.xterm-viewport`, `.xterm-screen`, `canvas`. CSS injection working. |
| Overview ruler canvas is 8px wide | Chrome DevTools eval | `canvas[style]` with `width: 8px` is the ruler, not a WebGL renderer canvas. DOM renderer active. |
| Dialog background is intentional glassmorphism | Chrome DevTools eval | `rgba(3,7,18,0.22)` with `backdrop-blur-2xl` — correct look for glass panel on dark bg. |
| `everOpened` state resets on navigation | `reboot-terminal-dialog.tsx:16` | `useState(false)` — after page nav, `RebootTerminalPane` doesn't mount until first open. Expected. |
| Turbopack does not proxy WS upgrades | Observed behavior | Next.js 15 Turbopack dev server does not forward WebSocket upgrade headers for custom `rewrites()` paths. |

---

## Technical Decisions

**Token auth over IP restriction for `/ws/shell`**
The prior loopback-only guard worked for localhost access but broke remote browser connections. Added the same `AXON_WEB_API_TOKEN` check used by `/ws`. Loopback still works without a token (trusted-network pattern). Remote without token configured → 403 (safe default — shell is sensitive).

**Direct port 49000 connection in dev mode**
`use-shell-session.ts` detects `loc.port === '49010'` and connects to `hostname:49000` instead of going through the Next.js rewrite. This bypasses the Turbopack WS proxy limitation without needing env var changes.

**DOM renderer over WebGL for transparency**
WebGL addon's canvas `clear()` operation writes a solid color regardless of CSS `background: transparent`. Skipping WebGL when `allowTransparency: true` forces the DOM renderer which respects transparent backgrounds. CSS injection (`axon-terminal-scrollbar` style tag) ensures all xterm elements are transparent.

**Kept glassmorphism dialog bg**
The dark appearance of the empty terminal area is the `rgba(3,7,18,0.22)` dialog container over `#030817` page background — intentional glass effect, not a bug. No changes needed.

---

## Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `crates/web.rs` | `shell_ws_upgrade` now accepts `Query(params)`, `State(state)` — added token auth for non-loopback | Fix 403 for remote browser connections |
| `apps/web/hooks/use-shell-session.ts` | (prior session) Port heuristic: `loc.port === '49010'` → connect to `:49000` | Bypass Turbopack WS proxy limitation |
| `apps/web/shell-server.mjs` | (prior session) Added `process.loadEnvFile()` at top | Load `AXON_WEB_API_TOKEN` env var |
| `apps/web/components/terminal/terminal-emulator.tsx` | (prior session) CSS injection + skip WebGL when `allowTransparency: true` | Terminal background transparency |

---

## Commands Executed

```bash
# Verify fix compiles
cargo check --bin axon
# → Finished dev profile in 4.87s

# Build new binary
cargo build --bin axon
# → Finished dev profile in 53.81s

# Find old PID
fuser 49000/tcp
# → 2612150

# Kill old server, start new
kill 2612150
source .env && AXON_SERVE_HOST=0.0.0.0 ./target/debug/axon serve --port 49000 > /tmp/axon-serve.log 2>&1 &

# Verify listening
ss -lnp | grep 49000
# → tcp LISTEN 0 4096 0.0.0.0:49000 ... axon,pid=2615321
```

**Chrome DevTools verification:**
```js
// WS interception
window.WebSocket = function(url, protocols) { ... if (url.includes('/ws/shell')) window.__shellWs = ws; }

// Send test command
window.__shellWs.send(JSON.stringify({ type: 'input', data: 'echo DEVTOOLS_TEST\r' }))
// → Terminal showed: echo DEVTOOLS_TEST / DEVTOOLS_TEST / new prompt

window.__shellWs.send(JSON.stringify({ type: 'input', data: 'ls /tmp | head -5\r' }))
// → axon-mcp, claude-1000, com.google.Chrome.Lz455c, e52014366924ae3ba787fa70e347f723, f6b834d1925a61043ea7ad48dcbef906
```

---

## Behavior Changes (Before / After)

| Aspect | Before | After |
|--------|--------|-------|
| Terminal WS connection | 403 Forbidden from Rust server for all browser connections | Accepts connections with valid `AXON_WEB_API_TOKEN` |
| Status indicator | Permanently "RECONNECTING..." | Green "CONNECTED" dot |
| Shell prompt | Never appeared | `jmagar@dookie ~ ` visible immediately |
| PTY I/O | No input/output | Full bidirectional — commands execute, output returns |
| Terminal background | Dark (placeholder or WebGL canvas) | Transparent xterm + glassmorphism dialog container |

---

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| `cargo check --bin axon` | Clean compile | `Finished dev profile in 4.87s` | ✅ |
| `cargo build --bin axon` | Build success | `Finished dev profile in 53.81s` | ✅ |
| Port 49000 listening | axon process bound | `tcp LISTEN ... axon,pid=2615321` | ✅ |
| Terminal status indicator | "CONNECTED" | Green dot + "CONNECTED" text | ✅ |
| Shell prompt in xterm rows | Prompt present | `jmagar@dookie ~ ` in `.xterm-rows` | ✅ |
| `echo DEVTOOLS_TEST` round-trip | Output visible | "DEVTOOLS_TEST" returned in terminal | ✅ |
| `ls /tmp \| head -5` | Live PTY output | 5 `/tmp` entries returned | ✅ |
| xterm background transparency | `rgba(0,0,0,0)` | All xterm elements transparent | ✅ |
| CSS injection present | Style tag exists | `document.getElementById('axon-terminal-scrollbar')` → found | ✅ |
| No WebGL canvas | DOM renderer active | `canvasCount` in `.xterm-screen` = 0 | ✅ |

---

## Risks and Rollback

**Token auth added to `/ws/shell`:**
- Risk: Low. Same token already guarding `/ws`. Loopback connections still work without token (no regression for container-internal use).
- Rollback: Revert `crates/web.rs:shell_ws_upgrade` to the original loopback-only version (git diff).

**`axon serve` restart:**
- The new binary is running as a background process started from terminal. It will stop on shell exit or server reboot.
- Production deployment (Docker) must rebuild the image to pick up the `crates/web.rs` change.

---

## Decisions Not Taken

| Alternative | Rejected Because |
|-------------|-----------------|
| Use `shell-server.mjs` (port 49011) via Next.js proxy | Turbopack doesn't proxy WS upgrades for custom `rewrites()` paths — confirmed experimentally |
| IP allowlist instead of token | Fragile with DHCP/VPN — token auth is portable and already infrastructure |
| WebSocket proxy middleware in Next.js custom server | Would require ejecting from Turbopack dev server — too invasive |
| Resize viewport to desktop (1024px+) for testing | Mobile viewport (780px) actually confirmed mobile layout works; didn't need to change |

---

## Open Questions

- `reboot-shell.tsx` was modified externally (linter/user) to add `RebootMcpDialog` and `McpIcon` — this new MCP dialog component has not been reviewed in this session.
- The `use-shell-session.ts` port heuristic (`loc.port === '49010'`) will break if the Next.js dev port changes. A `NEXT_PUBLIC_AXON_WS_URL` env var override is the proper long-term fix.
- The `axon serve` process started in this session is ephemeral (background process). Needs to be added to the `just dev` workflow or s6 service definition for persistence.
- `press_key('Enter')` via Chrome DevTools MCP navigated the page to `about:blank` — root cause unknown (possibly triggered a form submit or navigation in the page's event listeners).

---

## Next Steps

1. Add `axon serve` to the `just dev` recipe so it starts alongside workers and Next.js.
2. Rebuild Docker image to ship `crates/web.rs` token auth fix to the container environment.
3. Review the new `RebootMcpDialog` component added by the concurrent linter changes.
4. Consider making terminal dialog draggable/resizable (user requested in prior session, linter reverted the implementation).
5. Add `NEXT_PUBLIC_AXON_WS_URL` to `.env.example` as the canonical override for WS base URL.
