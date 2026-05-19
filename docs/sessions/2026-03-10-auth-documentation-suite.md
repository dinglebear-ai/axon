# Session: Auth Documentation Suite
**Date:** 2026-03-10
**Branch:** `refactor/acp-performance-modern-rust`

## Session Overview

Created comprehensive auth documentation for all five Axon authentication methods in `docs/auth/`. Debugged Tailscale serve routing issues (stale config pointing at dead port, Next.js WS rewrite stripping headers), fixed tailscale serve route configuration, and documented every auth surface end-to-end.

## Timeline

1. **Tailscale header debugging** — User reported `AXON_REQUIRE_DUAL_AUTH=true but no valid Tailscale header from 127.0.0.1`. Investigated `crates/web/tailscale_auth.rs`, `crates/web.rs`, and tailscale serve config.
2. **Root cause identified** — Two issues: (a) `tailscale serve` was pointing at dead port 18789, not the active servers; (b) Next.js WS rewrite (`/ws → localhost:49000`) creates a new upstream connection that strips Tailscale-injected headers.
3. **Tailscale serve reconfigured** — Set up correct route map: `/` → Next.js (49010), `/ws` → Rust (49000), `/ws/shell` → shell server (49011), `/download` and `/output` → Rust (49000).
4. **Verified auth end-to-end** — curl tests confirmed 101 WS upgrade via both direct and tailscale serve paths. Token URL-encoding (`+` → `%2B`) verified.
5. **Created `docs/auth/` directory** with five documents:
   - `README.md` — Method overview, decision tree, hardening options, source file map
   - `TAILSCALE.md` — Route map, setup commands, verification, troubleshooting
   - `API-TOKEN.md` — Token delivery (WS/API/download/shell), dual-auth, env vars
   - `SSH-KEY.md` — Challenge-response flow, client examples, security model
   - `MCP-OAUTH.md` — Google OAuth broker, endpoints, TTLs, MCP client config
6. **Gap audit** — Sweep found two missing surfaces: shell WebSocket (`/ws/shell`) and download/output endpoints. Updated README.md and API-TOKEN.md to cover them.
7. **README.md updated** — Added one-line pointer from the main README's "Web App Security" section to `docs/auth/README.md`.

## Key Findings

- **Tailscale serve injects headers ONLY for traffic proxied through it** — raw tailnet IP connections get no headers (`tailscale_auth.rs:1-18`)
- **Next.js WS rewrite strips Tailscale headers** — the rewrite creates a new upstream connection, losing all injected headers. Solution: route `/ws` directly to Rust from tailscale serve, bypassing Next.js (`next.config.ts` rewrites)
- **`tailscale serve` prefix matching** — more specific paths take precedence (`/ws/shell` over `/ws`). Removing one route can drop others (known tailscale behavior)
- **Shell server (`apps/web/shell-server.mjs:32`)** has independent token auth (`AXON_SHELL_WS_TOKEN` → `AXON_WEB_API_TOKEN` fallback) with no Tailscale/SSH key support
- **Download routes (`crates/web/download.rs:30-52`)** use the full auth stack (SSH key → TS/token via `check_auth()`)
- **`/proc/PID/environ`** does not reflect runtime `std::env::set_var()` calls from `dotenvy` — misleading when debugging env var availability

## Technical Decisions

| Decision | Rationale |
|----------|-----------|
| Route `/ws` directly to Rust (49000) from tailscale serve | Bypasses Next.js rewrite that strips Tailscale headers |
| Document shell token as a distinct (5th) auth method | Separate server, separate token, no Tailscale/SSH — functionally independent |
| Keep `AXON_WEB_ALLOW_QUERY_TOKEN` out of `.env.example` | Security-sensitive debug flag, disabled by default — intentional omission |
| Add hardening options table to README.md | `AXON_TAILSCALE_STRICT` and `AXON_TAILSCALE_ALLOWED_USERS` were only in TAILSCALE.md, not visible in the overview |

## Files Modified

| File | Action | Purpose |
|------|--------|---------|
| `docs/auth/README.md` | Created | Auth method overview, decision tree, hardening options, source file map |
| `docs/auth/TAILSCALE.md` | Created | Tailscale serve setup, route map, verification, troubleshooting |
| `docs/auth/API-TOKEN.md` | Created | Static token auth for WS, `/api/*`, download, shell WebSocket |
| `docs/auth/SSH-KEY.md` | Created | SSH key challenge-response flow, client examples, security model |
| `docs/auth/MCP-OAUTH.md` | Created | Google OAuth broker for MCP clients, endpoints, TTLs, config |
| `README.md` | Modified | Added one-line link to `docs/auth/README.md` in Web App Security section |

## Behavior Changes (Before/After)

| Surface | Before | After |
|---------|--------|-------|
| Auth documentation | Scattered across `docs/MCP.md`, `docs/SECURITY.md`, source comments | Centralized in `docs/auth/` with per-method docs |
| README.md auth section | Env var table only, no link to detailed docs | Links to `docs/auth/README.md` |
| Shell WebSocket auth | Undocumented | Fully documented in `API-TOKEN.md#shell-websocket-wsshell` |
| Download/output endpoint auth | Not mentioned in auth docs | Documented in `API-TOKEN.md` and README source table |

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| `docs/auth/` file count | 5 files | README.md, TAILSCALE.md, API-TOKEN.md, SSH-KEY.md, MCP-OAUTH.md | PASS |
| README.md links to docs/auth/ | Present | Line 204: `[docs/auth/README.md]` | PASS |
| All 5 auth methods in README.md table | 5 rows | Tailscale, API token, SSH key, Shell token, MCP OAuth | PASS |
| All 7 source files in README.md table | 7 rows | tailscale_auth.rs, ssh_auth.rs, web.rs, download.rs, proxy.ts, shell-server.mjs, oauth_google/ | PASS |
| Agent sweep for undocumented auth | No gaps | Only minor polish (strict/allowlist in overview — fixed) | PASS |

## Risks and Rollback

- **Low risk** — documentation-only changes, no code modifications
- **Rollback**: `git checkout -- docs/auth/ README.md`
- **No behavior changes** to any auth mechanism — these docs describe existing code

## Decisions Not Taken

| Alternative | Why rejected |
|-------------|-------------|
| Consolidate all auth into a single doc | Five distinct methods with different surfaces — separate docs are more navigable |
| Move MCP OAuth content out of `docs/MCP.md` | MCP.md still has the authoritative OAuth endpoint list; `docs/auth/MCP-OAUTH.md` references it rather than duplicating |
| Add `AXON_WEB_ALLOW_QUERY_TOKEN` to `.env.example` | Security-sensitive debug flag disabled by default — intentional omission to avoid accidental enablement |

## Open Questions

- Should `docs/MCP.md` OAuth section be trimmed now that `docs/auth/MCP-OAUTH.md` exists, or keep both as complementary?
- Should the shell server support Tailscale header auth in the future (would require routing `/ws/shell` through tailscale serve → shell server directly)?

## Next Steps

- None blocking — auth documentation is complete and verified
