# Session: MCP Header Icon + Status Indicator Fixes

**Date:** 2026-03-07
**Branch:** feat/services-layer-refactor

---

## Session Overview

Added an MCP Servers popup to the reboot shell header (matching the terminal/logs popup pattern), then diagnosed and fixed three bugs causing the status indicator to show "unknown" instead of the real server state.

---

## Timeline

1. **MCP icon + dialog** — Created `mcp-config.tsx` (MCP logo SVG as React component), `reboot-mcp-dialog.tsx` (full MCP server management in a Dialog), wired button + state into `reboot-shell.tsx` for both mobile and desktop headers.
2. **Screenshot review** — Dialog rendered correctly; server showed "unknown" status dot.
3. **Bug investigation** — Read `status/route.ts`, `mcp-server-card.tsx`, `mcp-types.ts`. Found three independent bugs.
4. **Fixes applied** — Type mismatch in `loadStatus`, HEAD → POST probe, OAuth `auth-required` status.
5. **OAuth Q&A** — Explained that Claude CLI handles OAuth PKCE flow automatically on first connect; UI just needs to surface the "auth required" state clearly.

---

## Key Findings

- **Bug 1 — Type mismatch** (`page.tsx:35`, `reboot-mcp-dialog.tsx`): The status API returns `{ servers: { name: { status: "online" } } }` (objects), but `loadStatus` cast with `as { servers: Record<string, McpServerStatus> }` and passed values directly to `setStatusMap`. At runtime `statusMap[name]` was `{ status: "online" }` (object), not a string. `STATUS_DOT[{ status: "online" }]` → `undefined` → no color. `STATUS_LABEL[...]` → `undefined` → empty text. The `?? 'unknown'` fallback didn't fire because the object is truthy.

- **Bug 2 — Wrong HTTP verb** (`status/route.ts:73`): `HEAD` requests to MCP SSE endpoints hang until the 4 s timeout fires (SSE keeps the connection open), returning `offline` instead of the real status.

- **Bug 3 — Auth headers ignored** (`status/route.ts:133`): `checkHttpServer` only received `name` and `url`; `cfg.headers` (where `Authorization: Bearer <token>` lives) was never forwarded, so authenticated servers always got a bare probe.

- **OAuth architecture**: Axon MCP server is a full OAuth 2.0 Authorization Server (Google upstream). Claude CLI handles PKCE flow automatically on first Pulse connect; tokens stored in `~/.claude/`. A static Bearer token in `cfg.headers` bypasses OAuth entirely.

---

## Technical Decisions

- **POST JSON-RPC ping instead of GET/HEAD**: POST with `{"jsonrpc":"2.0","id":1,"method":"ping"}` returns immediately. GET opens an SSE stream. HEAD is not a standard MCP method. POST is the correct MCP transport verb.
- **`redirect: 'manual'`**: Prevents `fetch` from following OAuth login redirects into long-lived SSE streams; a 3xx is treated as `auth-required`.
- **`auth-required` status (yellow)**: 401/403/3xx = server is reachable but needs auth. More accurate than `offline`. Matches real-world state for OAuth-protected MCP servers.
- **Dialog pattern over navigation**: Used `Dialog`/`DialogContent` (matching `RebootLogsDialog`) rather than `router.push('/settings/mcp')` — keeps the user in context without leaving the reboot shell.
- **Renamed `mcp-icon.tsx` → `mcp-config.tsx`**: User preference for naming.

---

## Files Modified

| File | Change |
|------|--------|
| `apps/web/components/reboot/mcp-config.tsx` | **Created** — MCP logo SVG as `McpIcon` React component (`currentColor` strokes) |
| `apps/web/components/reboot/reboot-mcp-dialog.tsx` | **Created** — Full MCP server management dialog (load/add/edit/delete servers, status display) |
| `apps/web/components/reboot/reboot-shell.tsx` | **Modified** — Added `mcpOpen` state, `McpIcon` button in both mobile + desktop headers, `RebootMcpDialog` at render bottom |
| `apps/web/app/settings/mcp/mcp-types.ts:93` | **Modified** — Added `'auth-required'` to `McpServerStatus` union |
| `apps/web/app/settings/mcp/mcp-server-card.tsx:9-21` | **Modified** — Added `auth-required` to `STATUS_DOT` (yellow glow) and `STATUS_LABEL`, updated status text color logic |
| `apps/web/app/api/mcp/status/route.ts:22,73-91,133` | **Modified** — Added `auth-required` to `ServerStatus`; rewrote `checkHttpServer` to POST JSON-RPC ping with configured headers; pass `cfg.headers` at call site |
| `apps/web/app/settings/mcp/page.tsx:35-36` | **Fixed** — `loadStatus` now extracts `.status` from `{ status, error }` objects |
| `apps/web/components/reboot/reboot-mcp-dialog.tsx` | **Fixed** — Same `loadStatus` type extraction fix applied |

---

## Behavior Changes (Before → After)

| Surface | Before | After |
|---------|--------|-------|
| Reboot shell header | No MCP button | MCP icon button (both mobile + desktop) opens MCP Servers dialog |
| MCP status dot (OAuth server) | Always "unknown" (type bug) or "offline" (HEAD timeout) | Yellow "auth required" dot when unauthenticated; green "online" when Bearer token present |
| MCP status dot (any HTTP server) | HEAD request hung 4 s → `offline` | POST JSON-RPC ping → resolves in <1 s |
| Auth headers in status probe | Ignored | Forwarded from `cfg.headers` to the probe request |
| `/settings/mcp` page status | Same type bug — status never rendered | Fixed (same `loadStatus` extraction fix) |

---

## Verification Evidence

| Check | Expected | Observed | Status |
|-------|----------|----------|--------|
| Dialog renders | MCP Servers popup visible | Confirmed via screenshot | ✅ |
| `mcp-config.tsx` SVG paths | MCP logo paths match source SVG | Paths copied verbatim from `~/Downloads/Model_Context_Protocol_logo.svg` | ✅ |
| Type extraction fix | `statusMap[name]` is a string | `Object.fromEntries(...map(([k,v]) => [k, v.status]))` | ✅ (code review) |
| POST ping verb | No SSE hang | `method: 'POST'` + `redirect: 'manual'` | ✅ (code review) |
| `cfg.headers` forwarded | Auth headers reach probe | `checkHttpServer(name, cfg.url, cfg.headers)` | ✅ (code review) |
| End-to-end status for `https://axon.tootie.tv/mcp` | Yellow "auth required" (no token) or green "online" (with token) | Not live-tested | ⚠️ needs runtime verify |

---

## Source IDs + Collections Touched

None — no Axon embed/crawl/query operations performed in this session.

---

## Risks and Rollback

- **POST to MCP endpoint**: Sending a JSON-RPC `ping` is safe (read-only, standard MCP method). If a non-MCP HTTP server is configured, a POST with JSON body is harmless — it will return 4xx which maps to `offline` or `auth-required` correctly.
- **Rollback**: All changes are in the UI layer. Revert `status/route.ts` to HEAD if POST causes issues. The type fix in `loadStatus` is a pure correctness fix with no risk.

---

## Decisions Not Taken

- **Navigate to `/settings/mcp`** instead of dialog — would leave the reboot shell; dialog keeps context.
- **Use iframe to embed the MCP page** — complex auth, stale layout; reusing components directly is cleaner.
- **Implement OAuth flow in the web UI** — Claude CLI already handles it; duplicating the PKCE flow in the browser adds complexity with no benefit for this use case.
- **Show a "click to authenticate" button** for `auth-required` state — deferred; user needs to trigger it via Pulse (Claude CLI), not the web UI directly.

---

## Open Questions

- Does the axon MCP server at `https://axon.tootie.tv/mcp` respond to an unauthenticated `POST` with a JSON-RPC `ping` with a clean 401 (not a redirect or hang)? Needs runtime verification to confirm the yellow dot appears correctly.
- Should the dialog re-poll status periodically (e.g. every 30 s) so the dot updates after OAuth completes in Claude CLI?
- Is there a way to surface the OAuth login URL in the UI so users can trigger the flow from the dialog rather than waiting for Pulse to do it?

---

## Next Steps

- [ ] Live-test status indicator against `https://axon.tootie.tv/mcp` after dev server restarts
- [ ] Consider periodic status re-poll in `RebootMcpDialog` (30 s interval while dialog is open)
- [ ] Consider adding "Open OAuth Login" button for `auth-required` servers — fetch `/.well-known/oauth-authorization-server`, build auth URL, open in new tab
