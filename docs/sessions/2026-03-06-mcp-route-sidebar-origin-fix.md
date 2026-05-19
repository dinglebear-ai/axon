# Session: MCP Route Move, Sidebar Navigation, Origin Fix

**Date:** 2026-03-06
**Branch:** `feat/services-layer-refactor`

## Session Overview

Three related changes to the Axon web UI:
1. Moved the MCP servers page route from `/mcp` to `/settings/mcp`
2. Moved navigation buttons (MCP Servers, Agents, Settings) from the fixed top-right header into the sidebar
3. Fixed Pulse chat 403 "Forbidden origin" — root cause was `AXON_WEB_ALLOWED_ORIGINS` set to `http://localhost:3000` while the app was accessed at `http://dookie:3000`, plus the dev server defaulting to port 3000 instead of the container port 49010

## Timeline

1. **Route move** — Moved `apps/web/app/mcp/` directory to `apps/web/app/settings/mcp/`
2. **Sidebar migration** — Removed 3 header buttons from `app-shell.tsx`, added them as `PAGE_LINKS` entries in `pulse-sidebar.tsx`
3. **Reference updates** — Updated all `/mcp` hrefs in `landing-cards.tsx`, `page-impl-content.tsx`, and test import in `__tests__/mcp-types.test.ts`
4. **Origin debugging** — Found `AXON_WEB_ALLOWED_ORIGINS=http://localhost:3000` in `.env.local`, updated to `http://localhost:49010`, but user still got 403
5. **Root cause** — User accesses app via `http://dookie:3000` (hostname mismatch + port mismatch). `proxy.ts` is loaded as Next.js middleware via Turbopack (confirmed in `.next/dev/server/middleware.js` compiled artifact)
6. **Port fix** — Added `--port 49010` to `dev` and `start` scripts in `package.json` to match container port
7. **Final origin fix** — Set `AXON_WEB_ALLOWED_ORIGINS=http://dookie:49010,http://localhost:49010`

## Key Findings

- **`proxy.ts` IS the Next.js middleware** — No `middleware.ts` file exists, but Turbopack treats `proxy.ts` as middleware because it exports `config` with a `matcher`. Confirmed in `.next/dev/server/middleware.js:4`: `INNER_MIDDLEWARE_MODULE => "[project]/proxy.ts [middleware] (ecmascript)"`
- **Origin check logic** (`proxy.ts:81-115`): When `ALLOWED_ORIGINS` is non-empty, does strict `includes()` match against the normalized browser `Origin` header
- **Middleware manifest is misleading** — `.next/server/middleware-manifest.json` shows `"middleware": {}` even though compiled middleware exists and runs
- **Dev server port** — `pnpm dev` (Next.js) defaults to port 3000; container runs on 49010. These were out of sync.
- **Justfile doesn't need changes** — `just web-dev` and `just dev` call `pnpm dev` which inherits `--port 49010` from `package.json`

## Technical Decisions

- **Route `/settings/mcp`** over `/servers` or `/config/mcp` — user chose `/settings/mcp` to group config pages under settings
- **Both origins in allowlist** — `http://dookie:49010,http://localhost:49010` covers both access patterns
- **Port pinned in package.json** — `--port 49010` in both `dev` and `start` scripts ensures local dev matches container behavior

## Files Modified

| File | Change |
|------|--------|
| `apps/web/app/mcp/` → `apps/web/app/settings/mcp/` | Route directory moved |
| `apps/web/components/app-shell.tsx` | Removed header nav buttons (MCP, Agents, Settings), removed unused imports |
| `apps/web/components/pulse/sidebar/pulse-sidebar.tsx` | Added 3 PAGE_LINKS entries + `Bot`, `Network`, `Settings2` icon imports |
| `apps/web/components/landing-cards.tsx` | Updated `/mcp` → `/settings/mcp` hrefs (auto-formatted by linter) |
| `apps/web/app/settings/page-impl-content.tsx` | Updated `router.push('/mcp')` → `router.push('/settings/mcp')` |
| `apps/web/__tests__/mcp-types.test.ts` | Updated import path `@/app/mcp/mcp-types` → `@/app/settings/mcp/mcp-types` |
| `apps/web/package.json` | Added `--port 49010` to `dev` and `start` scripts |
| `apps/web/.env.local` | Updated `AXON_WEB_ALLOWED_ORIGINS` to `http://dookie:49010,http://localhost:49010` |

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| MCP page URL | `/mcp` | `/settings/mcp` |
| Nav buttons location | Fixed top-right header in `AppShell` | Sidebar `PAGE_LINKS` in `PulseSidebar` |
| Dev server port | 3000 (Next.js default) | 49010 (matches container) |
| Pulse chat origin | 403 Forbidden origin | Should work (pending user restart) |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `vitest run __tests__/mcp-types.test.ts` | 17 tests pass | 17 passed (12ms) | PASS |
| `grep AXON_WEB_ALLOWED_ORIGINS .env.local` | Contains dookie:49010 | `http://dookie:49010,http://localhost:49010` | PASS |
| `grep '"dev"' package.json` | Contains --port 49010 | `"dev": "next dev --port 49010"` | PASS |

## Source IDs + Collections Touched

None — no Qdrant operations in this session.

## Risks and Rollback

- **Low risk** — All changes are UI routing and env config. Rollback: `git checkout -- apps/web/` and restore `.env.local`
- **`.env.local` is gitignored** — The origin fix won't persist across clones; document in `.env.example` if needed
- **Stale `.next/` cache** — If 403 persists after restart, try `rm -rf apps/web/.next` to clear compiled middleware

## Decisions Not Taken

- **Remove `proxy.ts` entirely** — Still needed as middleware for API auth gating; just needed correct origin config
- **Use `AXON_WEB_ALLOW_INSECURE_DEV=true`** — Would bypass origin checks entirely; less secure than fixing the allowlist
- **Wildcard origins** — Could have added `*` support to `isAllowedOrigin()` but not needed; explicit list is safer

## Open Questions

- **`proxy.ts` naming** — It functions as Next.js middleware but isn't named `middleware.ts`. Turbopack picks it up anyway. Is this intentional or accidental? Could break on Next.js upgrades.
- **ECONNRESET errors** — User saw `uncaughtException: Error: aborted` with `ECONNRESET`. Likely caused by 403 rejections but worth monitoring after the fix.
- **Container env sync** — Container's `AXON_WEB_ALLOWED_ORIGINS` may need updating if it still has `http://localhost:3000`

## Next Steps

- Restart `pnpm dev` to pick up port 49010 and new origin config
- Verify Pulse chat no longer returns 403
- Monitor for ECONNRESET errors after fix
- Consider renaming `proxy.ts` → `middleware.ts` for clarity (standard Next.js convention)
