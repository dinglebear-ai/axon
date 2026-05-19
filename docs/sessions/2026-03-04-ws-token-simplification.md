# Session: WS Token Gate Simplification

**Date:** 2026-03-04
**Branch:** feat/sidebar
**Continued from:** `docs/sessions/2026-03-04-ws-oauth-gate-v2.md`

---

## Session Overview

Resolved the open question left by the previous session: the Next.js web UI was not passing any token to the `/ws` endpoint, which would have caused 401s against the newly-implemented Rust WS OAuth gate. After investigating, the OAuth/Redis path was identified as unnecessary complexity — one static token (`AXON_WEB_API_TOKEN`) already covers both `/api/*` and `/ws` for the frontend. The Redis-backed MCP OAuth path was removed from the WS gate entirely, leaving a clean two-state model (gate on / gate off).

---

## Timeline

1. **Session start** — user asked "wasnt there something we had to make a decision about"; identified the pending frontend WS token integration from the previous session
2. **Read `use-axon-ws.ts`** — confirmed WS URL constructed at line 56 as `${proto}//${host}/ws` with no `?token=` param
3. **First pass (over-engineered)** — added `static_api_token` AND `oauth_redis` dual-path to the Rust WS gate; appended `NEXT_PUBLIC_AXON_API_TOKEN` as `?token=` in the frontend hook
4. **User question** — "dont we already have an api key setup for this?" — confirmed `AXON_WEB_API_TOKEN` already existed for `/api/*`
5. **Architecture diagram** — mapped out the three separate auth systems; user identified the confusion
6. **User decision** — "so we can just have one token for frontend and /ws right? then just leave oauth for mcp" — remove Redis/OAuth path from WS gate
7. **Simplification** — stripped `oauth_redis`, `oauth_prefix`, `BearerTokenRecord`, `unix_now_secs`, `validate_bearer_token` from `crates/web.rs`; WS gate is now a single `if let Some(ref expected) = state.api_token` check
8. **Docs pass** — updated `docs/SERVE.md`, `docs/SECURITY.md`, `apps/web/CLAUDE.md`, root `CLAUDE.md`

---

## Key Findings

- **`use-axon-ws.ts:56`** — WS URL built without any token; would 401 against an active gate
- **`proxy.ts` is not Next.js middleware** — no `export default`, no `middleware.ts` file exists; the `/api/*` auth logic in `proxy.ts` is callable but not automatically wired as Next.js Edge middleware
- **`/ws` bypasses Next.js middleware entirely** — it is a raw TCP rewrite in `next.config.ts:73`; even if `middleware.ts` existed, it would not intercept WS upgrade requests
- **MCP OAuth tokens (`atk_`) and `AXON_WEB_API_TOKEN` were two independent secrets** for the same surface (`/ws`); consolidating to one removes the ambiguity entirely
- **`redis::AsyncCommands` import** removed from `crates/web.rs` — no longer needed once the Redis path was dropped

---

## Technical Decisions

### One token for all frontend surfaces (adopted)
`AXON_WEB_API_TOKEN` already existed and was enforced for `/api/*`. Reusing it for `/ws` gives a single secret that covers both surfaces. No new env vars, no Redis dependency in the WS path.

### Remove MCP OAuth path from WS gate (adopted)
MCP clients (`atk_` tokens) already have the MCP tool API (`/mcp`). There is no documented use case for an MCP client driving the WS command bridge directly. Keeping the Redis validation path added async latency and coupling between the WS gate and the MCP OAuth subsystem.

### Two-path gate (rejected)
First implementation accepted both `AXON_WEB_API_TOKEN` (fast path) and `atk_` Redis tokens (slow path). Rejected because: (a) MCP clients don't need WS access, (b) created confusion about which token to use, (c) added Redis I/O on the hot WS upgrade path.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/web.rs` | Removed `oauth_redis`, `oauth_prefix`, `BearerTokenRecord`, `unix_now_secs`, `validate_bearer_token`, `redis::AsyncCommands` import. `AppState.static_api_token` → `AppState.api_token`. `ws_upgrade` gate is now 8 lines: check `api_token`, compare `?token=`, 401 on mismatch. |
| `apps/web/hooks/use-axon-ws.ts` | `connect()` at line 54: appends `?token=${encodeURIComponent(NEXT_PUBLIC_AXON_API_TOKEN)}` to WS URL when env var is present. |
| `docs/SERVE.md` | Added "WebSocket Authentication" section (gate activation, token flow diagram, env var table, shell WS loopback restriction). |
| `docs/SECURITY.md` | Added "WebSocket Authentication Gate" to Security Controls; updated Residual Risk #2 (was "no built-in WS auth" — now describes gate-disabled risk); expanded Source Map. |
| `apps/web/CLAUDE.md` | Updated WS client note + added "WS Auth Gate" gotcha section. |
| `CLAUDE.md` (root) | Updated "Web App Security Env" section — now explains one-token model covering both `/api/*` and `/ws`. |

---

## Commands Executed

| Command | Result |
|---------|--------|
| `cargo check --bin axon` (after initial two-path implementation) | `Finished` 0 errors |
| `cargo check --bin axon` (after simplification) | `Finished` 0 errors |

---

## Behavior Changes (Before / After)

| Surface | Before | After |
|---------|--------|-------|
| `/ws` upgrade (no token) | Accepted (gate disabled — no Redis configured) | Accepted if `AXON_WEB_API_TOKEN` unset; 401 if set |
| `/ws` upgrade (`AXON_WEB_API_TOKEN` token) | Not checked | Accepted |
| `/ws` upgrade (MCP `atk_` token) | Accepted (Redis path) | Rejected — MCP OAuth tokens no longer accepted by WS gate |
| Browser WS URL | `wss://<host>/ws` (no token) | `wss://<host>/ws?token=<NEXT_PUBLIC_AXON_API_TOKEN>` when env var set |
| `AppState` fields | `oauth_redis`, `oauth_prefix`, `static_api_token` | `api_token` only |
| Redis connection at startup | Attempted when `GOOGLE_OAUTH_REDIS_URL` or `AXON_REDIS_URL` set | Not attempted (no Redis in WS path) |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` (post-simplification) | 0 errors, 0 warnings | `Finished dev profile` | ✅ |

---

## Source IDs + Collections Touched

None — no Axon embed/retrieve operations were performed for web searches or doc indexing during this session.

---

## Risks and Rollback

**Risk 1**: MCP OAuth clients that previously could connect to `/ws` via `atk_` tokens can no longer do so. This was an undocumented capability with no known users — risk is low.

**Risk 2**: `NEXT_PUBLIC_AXON_API_TOKEN` is embedded in the browser bundle (Next.js `NEXT_PUBLIC_*` convention). Any user who can load the page can read it from the bundle. This is acceptable for a self-hosted homelab tool but is not suitable for multi-tenant deployments.

**Rollback**: Revert `crates/web.rs` to restore `AppState` with `oauth_redis`/`oauth_prefix`/`static_api_token` fields and the two-path `ws_upgrade` check. Revert `use-axon-ws.ts` to remove the `?token=` append.

---

## Decisions Not Taken

| Alternative | Why Rejected |
|-------------|-------------|
| Keep two-path gate (static + Redis OAuth) | MCP clients don't need WS access; added Redis latency; confused auth model |
| New `NEXT_PUBLIC_AXON_WS_TOKEN` env var | Unnecessary — `NEXT_PUBLIC_AXON_API_TOKEN` already exists and serves the same purpose |
| Implement real Next.js middleware (`middleware.ts`) for `/api/*` | Out of scope; `proxy.ts` auth logic exists and works when called; `/ws` bypass is unrelated |
| Constant-time compare for WS token check | `timingSafeEqual` is used in `proxy.ts` for `/api/*`; WS token is in a query param (already visible in logs/network tab), so timing safety is marginal benefit vs added complexity |

---

## Open Questions

1. **`proxy.ts` middleware wiring** — `proxy.ts` exports a named `proxy` function (not `export default`) and has `config.matcher` but no `middleware.ts` file exists. It is unclear whether `/api/*` auth is actually enforced automatically or requires each route to call `proxy(req)` explicitly. The `logs/route.ts` comment says "Auth is enforced by middleware.ts" but no such file was found.
2. **`NEXT_PUBLIC_AXON_API_TOKEN` in bundle** — the token is publicly readable from the browser bundle. For homelab use this is acceptable; worth documenting as a known limitation.

---

## Next Steps

1. **Clarify `proxy.ts` wiring** — determine whether each `/api/*` route calls `proxy(req)` or whether there is some other mechanism ensuring auth is enforced
2. **Create PR** from `feat/sidebar` → `main` with description of WS gate simplification
3. **Test end-to-end** — confirm browser WS connects successfully with `NEXT_PUBLIC_AXON_API_TOKEN` set
