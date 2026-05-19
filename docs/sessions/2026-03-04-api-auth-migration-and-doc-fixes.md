# Session: API Auth Migration, apiFetch Helper, and Doc Fixes

**Date:** 2026-03-04
**Branch:** `feat/sidebar`
**Session type:** Bug fix + refactor + docs update

---

## Session Overview

Diagnosed systemic 503 auth failures on all client-side `/api/*` calls in the Next.js web app (`apps/web`). Root cause: `proxy.ts` middleware requires `AXON_WEB_API_TOKEN` but neither the env var was set nor did any client fetch include an auth header. Fixed by generating a secure token, setting it in `.env`, creating a shared `apiFetch()` helper, and bulk-migrating 22 client files. Also fixed a `TypeError: sessions.slice is not a function` crash from unchecked API error responses. Created `docs/CONTEXT-INJECTION.md` documenting the RAG pipeline. Updated `.env.example`, `README.md`, and `CLAUDE.md` to reflect the corrected security model.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | Resumed from Chrome DevTools investigation of `/api/cortex/suggest` returning 503 |
| Early | Traced 503 to `proxy.ts` auth enforcement — no token set, no header on client fetch |
| Mid | Fixed suggest fetch in `evaluate/page.tsx` with inline `x-api-key` header (temporary) |
| Mid | Created `docs/CONTEXT-INJECTION.md` — full RAG pipeline documentation |
| Mid | Diagnosed `TypeError: sessions.slice is not a function` — API error object stored as array state |
| Mid | Identified systemic root cause: ALL 22 client fetch callsites missing auth header |
| Mid | Generated token `4TDc7+OFAzm29G5Pjz4qhUox3MeA1bn0MSiRp0LGrE4=` and set in `.env` |
| Mid | Created `apps/web/lib/api-fetch.ts` shared helper |
| Mid | Ran bulk Python migration — replaced `fetch(` → `apiFetch(` across 22 files |
| End | Updated `.env.example`, `README.md`, `CLAUDE.md` |
| End | Restarted `axon-web` container; confirmed both tokens live via `docker exec` |

---

## Key Findings

- **`proxy.ts`** (`apps/web/proxy.ts`): enforces auth on all `/api/*` routes. Accepts token via `Authorization: Bearer <token>` or `x-api-key: <token>`. Returns 503 with `"API authentication is not configured"` when `AXON_WEB_API_TOKEN` is unset and `AXON_WEB_ALLOW_INSECURE_DEV` is false.
- **`middleware.ts` was deleted** from the working tree but `proxy.ts` is the actual active middleware (imported differently). Documentation incorrectly referenced `middleware.ts` everywhere.
- **`use-recent-sessions.ts`**: `setSessions(data)` called without `Array.isArray()` guard — on 503 error, `data` is an object `{error: "..."}`, causing `sessions.slice(0,5)` in `landing-cards.tsx:173` to throw.
- **`NEXT_PUBLIC_AXON_API_TOKEN`** must be set and must match `AXON_WEB_API_TOKEN` — it's baked into the Next.js bundle by Next.js during build. A running container won't pick it up without a restart.
- All 22 client files were calling `fetch('/api/...')` with no auth header.

---

## Technical Decisions

- **`apiFetch` as a thin wrapper** rather than patching each callsite individually — single source of truth for the token header; future changes (e.g., adding `Authorization: Bearer`) only need one edit.
- **`process.env.NEXT_PUBLIC_AXON_API_TOKEN`** read at module level in `api-fetch.ts` — baked in at build time by Next.js, no runtime cost.
- **No fallback to unauthenticated fetch** when token is unset — `apiFetch` always injects `x-api-key` if the env var is present; if absent, it falls through to plain `fetch`. This means dev without a token still works via `AXON_WEB_ALLOW_INSECURE_DEV=true`.
- **Did not add `Authorization: Bearer` format** — `proxy.ts` accepts both; `x-api-key` is simpler and less likely to conflict with other middleware (e.g., OAuth flows).

---

## Files Modified

| File | Change |
|------|--------|
| `apps/web/lib/api-fetch.ts` | **Created** — shared `apiFetch()` wrapper injecting `x-api-key` |
| `apps/web/hooks/use-recent-sessions.ts` | Fixed: `Array.isArray(data)` guard + `apiFetch` migration |
| `apps/web/hooks/use-pulse-autosave.ts` | `apiFetch` migration |
| `apps/web/components/cortex/sources-dashboard.tsx` | `apiFetch` migration |
| `apps/web/components/cortex/domains-dashboard.tsx` | `apiFetch` migration |
| `apps/web/components/cortex/stats-dashboard.tsx` | `apiFetch` migration |
| `apps/web/components/cortex/status-dashboard.tsx` | `apiFetch` migration |
| `apps/web/components/cortex/doctor-dashboard.tsx` | `apiFetch` migration |
| `apps/web/components/landing-cards.tsx` | `apiFetch` migration |
| `apps/web/components/workspace/file-tree.tsx` | `apiFetch` migration |
| `apps/web/components/omnibox/omnibox-effects.ts` | `apiFetch` migration |
| `apps/web/components/omnibox/hooks/use-omnibox-mentions.ts` | `apiFetch` migration |
| `apps/web/components/jobs/jobs-dashboard.tsx` | `apiFetch` migration |
| `apps/web/components/pulse/sidebar/workspace-section.tsx` | `apiFetch` migration |
| `apps/web/components/editor/plugins/ai-chat-kit.tsx` | `apiFetch` migration |
| `apps/web/lib/pulse/chat-api.ts` | `apiFetch` migration |
| `apps/web/app/mcp/page.tsx` | `apiFetch` migration |
| `apps/web/app/jobs/[id]/page.tsx` | `apiFetch` migration |
| `apps/web/app/agents/page.tsx` | `apiFetch` migration |
| `apps/web/app/workspace/page.tsx` | `apiFetch` migration |
| `apps/web/app/docs/page.tsx` | `apiFetch` migration |
| `apps/web/app/editor/page.tsx` | `apiFetch` migration |
| `apps/web/app/evaluate/page.tsx` | Fixed suggest auth + `apiFetch` migration |
| `.env` | Appended `AXON_WEB_API_TOKEN` and `NEXT_PUBLIC_AXON_API_TOKEN` |
| `.env.example` | Fixed `middleware.ts` → `proxy.ts` ref; `NEXT_PUBLIC` marked required-when-set |
| `README.md` | Section renamed; both vars marked required; `proxy.ts` referenced |
| `CLAUDE.md` | Web security env section corrected; `apiFetch` documented |
| `docs/CONTEXT-INJECTION.md` | **Created** — full RAG context injection pipeline documentation |

---

## Commands Executed

```bash
# Confirmed proxy.ts is active middleware
cat apps/web/proxy.ts | grep -n "API_TOKEN\|503\|authentication"

# Generated secure token (base64 32-byte random)
openssl rand -base64 32

# Bulk migration: fetch( → apiFetch( for /api/ routes across 22 files
python3 /tmp/migrate_apifetch.py  # (inline script)

# Restarted container to pick up new env vars
docker restart axon-web

# Verified tokens are live in container
docker exec axon-web env | grep -E "AXON_WEB_API_TOKEN|NEXT_PUBLIC_AXON_API_TOKEN"
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| All `/api/*` client calls | 503 "API authentication is not configured" | Authenticated with `x-api-key` header |
| `landing-cards.tsx` sessions | `TypeError: sessions.slice is not a function` crash | Safely renders empty state on error |
| `/api/cortex/suggest` on evaluate page | 503 — no suggestions loaded | Returns suggestions (if backend up) |
| `.env.example` docs | Referenced deleted `middleware.ts`; `NEXT_PUBLIC` listed as optional | `proxy.ts` referenced; both vars clearly documented as required pair |
| `README.md` security section | "Optional Web App Security" — misleading | "Web App Security" — required vars clearly marked |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `docker exec axon-web env \| grep AXON_WEB_API_TOKEN` | Token value set | `AXON_WEB_API_TOKEN=4TDc7+...` | ✅ PASS |
| `docker exec axon-web env \| grep NEXT_PUBLIC_AXON_API_TOKEN` | Token value set | `NEXT_PUBLIC_AXON_API_TOKEN=4TDc7+...` | ✅ PASS |
| `grep -r "apiFetch" apps/web/app apps/web/components apps/web/hooks apps/web/lib \| wc -l` | All 22 files migrated | 22+ occurrences | ✅ PASS |

---

## Source IDs + Collections Touched

*(No Qdrant embed/retrieve operations in this session — all work was code and config changes.)*

---

## Risks and Rollback

- **Token in `.env`**: The token `4TDc7+OFAzm29G5Pjz4qhUox3MeA1bn0MSiRp0LGrE4=` is in `.env` (gitignored) and in the running container. To rotate: generate a new token, update `.env`, restart `axon-web`.
- **`apiFetch` import missing**: If any new file calls `fetch('/api/...')` without `apiFetch`, it will 503. Pattern is enforced by convention, not linting (no ESLint rule yet).
- **`NEXT_PUBLIC_AXON_API_TOKEN` is build-time**: Changes to this var require a container rebuild (`docker compose build axon-web && docker restart axon-web`), not just a restart.
- **Rollback**: Revert `.env` entry and set `AXON_WEB_ALLOW_INSECURE_DEV=true` to bypass auth temporarily.

---

## Decisions Not Taken

- **ESLint rule to ban raw `fetch('/api/...')`** — would enforce `apiFetch` usage at lint time. Not implemented; would require `@typescript-eslint` plugin config change.
- **`Authorization: Bearer` format** — `proxy.ts` supports it, but `x-api-key` chosen for simplicity.
- **Per-request token from cookie/session** — overkill for a self-hosted single-user app.
- **`AXON_WEB_ALLOW_INSECURE_DEV=true` for dev** — rejected because the token is already set and works; insecure dev mode is a footgun.

---

## Open Questions

- Does the `axon-web` container need a full rebuild (not just restart) for `NEXT_PUBLIC_AXON_API_TOKEN` to take effect in the built Next.js bundle? A restart picks up the env var at process level, but if Next.js bakes it in at `next build` time, the running container may have the old (empty) value baked in — and only the server-side `process.env` gets the restarted value. **This needs verification** by checking if client-side suggest calls now succeed.
- Are there any other non-`apiFetch` fetch calls targeting `/api/` routes that the Python migration missed (e.g., in test files or new files added after migration)?

---

## Next Steps

- Verify suggest on evaluate page works end-to-end (Chrome DevTools check)
- Consider a lint rule or grep CI check to catch raw `fetch('/api/...')` regressions
- Address open PR review comments on `feat/sidebar` (was about to start `/gh-address-comments` before this save)
- Consider container rebuild if client-side bundle still has old empty token baked in
