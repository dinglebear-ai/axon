# Parallel Security + Quality Fixes — Frontend Review Remediation
**Date:** 2026-02-27
**Branch:** `feat/crawl-download-pack`
**Plan:** `refactored-whistling-popcorn.md`

---

## Session Overview

Implemented a 7-agent parallel remediation of 8 Critical and 15+ High issues flagged by four code-review agents across the recently shipped frontend (settings redesign, MCP config/agents pages, PlateJS theming). All 7 agents ran simultaneously with zero file-scope overlap. After agent completion, fixed 10 additional issues exposed by TypeScript compilation and test runs. Final state: 27/27 test files pass, 231/231 tests pass, TSC clean, Biome lint clean (9 infos only).

---

## Timeline

| Time | Activity |
|------|----------|
| Start | Verified `react-error-boundary` NOT installed → Agent 4 creates custom ErrorBoundary |
| T+0 | Created 7 task records; launched all 7 agents simultaneously |
| T+2m | All 7 agents reported complete |
| T+2m | `tsc --noEmit` revealed: Zod v4 `.max()` on record, Vitest v4 `vi.fn` generic syntax, 3 pre-existing test failures |
| T+15m | Fixed 10 follow-up issues across 8 files; all tests green |

---

## Key Findings

- **Zod v4 breaking change**: `z.record().max()` does not exist in Zod v4 — use `.refine(obj => Object.keys(obj).length <= N)` instead (`route.ts:19`)
- **Vitest v4 breaking change**: `vi.fn<[Args], Return>()` syntax removed — use `vi.fn<(...args: Args) => Return>()` or untyped `vi.fn()`
- **IPv6 hostname brackets**: `new URL('http://[::1]').hostname` returns `[::1]` WITH brackets — must strip with `.replace(/^\[|\]$/g, '')` before blocklist check (`status/route.ts:45`)
- **`process.env.HOME` in ALLOWED_DIR_ROOTS**: Too broad in non-container environments — `../../etc/passwd` from a deep CWD resolves under HOME, passing the allowlist (`claude-stream-types.ts:76`)
- **Zod command regex allows `..`**: `/^[/a-zA-Z0-9._-]+$/` allows dots and slashes, so `../../bin/bash` passes — needed negative lookahead `/^(?!.*\.\.)([/a-zA-Z0-9._-]+)$/`
- **Pre-existing test staleness**: 3 test files referenced removed props (`mobilePane`, `onMobilePaneChange`, `isDesktop`) and missing `PulseChatRequest` fields (5 fields with `.default()` are required in the inferred OUTPUT type)
- **`PulseToolbar` uses `useRouter`**: Causes "invariant: app router must be mounted" when rendered with `renderToStaticMarkup` — requires `vi.mock('next/navigation', ...)` in smoke test
- **Session IDs in streaming test**: Test used `'sess-abc123'` / `'sess-resume-xyz'` — non-hex, rejected by the new `sessionId` regex → updated to valid UUID-format hex IDs

---

## Technical Decisions

- **Removed `process.env.HOME` from ALLOWED_DIR_ROOTS**: Container-specific value (`/home/node`) kept; host dev value excluded to prevent path-traversal bypass in tests and non-container deployments. Comment added explaining the rationale.
- **`z.record().refine()` over custom wrapper**: Zod v4 idiomatic approach; avoids wrapping with a custom `.superRefine()` which is more complex.
- **Bracket stripping for IPv6**: Applied to both production code AND the test's inline copy of the function — ensures test and production logic stay in sync.
- **`vi.fn()` without generics for fs mocks**: Consistent with `readFile` (already untyped); avoids Vitest v4 type-parameter syntax churn; `mockResolvedValue(undefined)` works without constraint.
- **`BASE_REQUEST_EXTRAS` constant in pulse-rag test**: Shared spread for 5 missing fields across 5 test objects — DRY and makes future schema additions to `PulseChatRequest` easier to track.
- **`vi.mock('next/navigation')` in smoke test**: Least invasive fix; test already renders with `renderToStaticMarkup` (no router context available).

---

## Files Modified

### Agent Work (parallel batch)

| File | Agent | Change |
|------|-------|--------|
| `app/api/mcp/route.ts` | A1 | Zod schemas replace manual type guard; CSRF `X-Pulse-Request` header check on PUT/DELETE |
| `app/api/mcp/status/route.ts` | A1 | SSRF `validateStatusUrl()` + path-sep guard on `checkStdioServer` |
| `__tests__/mcp/route.test.ts` | A1 | CSRF header in all PUT/DELETE tests; new security tests |
| `app/api/pulse/chat/claude-stream-types.ts` | A2 | `validateAddDir()`, tool identifier regex, `PULSE_SKIP_PERMISSIONS` gate |
| `lib/pulse/types.ts` | A2 | `sessionId` regex from `min(1).max(256)` to `^[0-9a-f-]{8,64}$` |
| `app/api/pulse/chat/replay-cache.ts` | A2 | `setInterval` prune every 60s |
| `app/api/pulse/chat/route.ts` | A2 | stderr 16KB cap |
| `__tests__/pulse/build-claude-args.test.ts` | A2 | Path traversal, skip-perms gate, sessionId format tests |
| `lib/sessions/session-scanner.ts` | A3 | Path traversal bounds check; `cleanProjectName` ternary fix |
| `lib/sessions/claude-jsonl-parser.ts` | A3 | 512KB line cap; null-byte stripping |
| `__tests__/sessions/scanner.test.ts` | A3 | **NEW** — 9 tests for scanner |
| `__tests__/sessions/parser.test.ts` | A3 | **NEW** — 12 tests for JSONL parser |
| `app/mcp/page.tsx` | A4 | AbortController, `X-Pulse-Request` header, functional `setConfig` updater, `key={editTarget}` on McpServerForm, ErrorBoundary |
| `app/mcp/components.tsx` | A4 | `KvPair.id` stable key; `jsonEditedManuallyRef` JSON tab sync guard |
| `app/agents/page.tsx` | A4 | AbortController; ErrorBoundary |
| `app/api/agents/route.ts` | A4 | `parseAgentsOutput` extracted to `lib/agents/parser.ts` |
| `lib/agents/parser.ts` | A4 | **NEW** — extracted `parseAgentsOutput` + `Agent` type |
| `components/ui/error-boundary.tsx` | A4 | **NEW** — class ErrorBoundary component |
| `hooks/use-ws-messages.ts` | A5 | `workspaceMode` deferred hydration; `pulseModel`/`pulsePermissionLevel` localStorage persistence |
| `hooks/use-pulse-settings.ts` | A5 | Removed stale TODO comment |
| `components/pulse/pulse-workspace.tsx` | A6 | `handleNewSession` calls `handleCancelPrompt()` first |
| `components/pulse/pulse-chat-pane.tsx` | A6 | `copyStatuses` Map keyed by message ID |
| `app/settings/page.tsx` | A7 | Two-step reset confirmation with 5s auto-cancel |
| `components/service-worker.tsx` | A7 | SW `onupdatefound` + `onstatechange` update lifecycle |
| `app/page.tsx` | A7 | Deleted `_CANVAS_PROFILE_LABELS` and `_handleCanvasProfileChange` dead code |

### Post-Agent Fixes (TypeScript/test cleanup)

| File | Fix |
|------|-----|
| `app/api/mcp/route.ts` | Zod v4: `.max(50)` → `.refine(obj => Object.keys(obj).length <= 50)` |
| `app/api/mcp/route.ts` | Command regex: added `(?!.*\.\.)` negative lookahead to block `../` traversal |
| `app/api/mcp/status/route.ts` | IPv6 bracket stripping: `.replace(/^\[|\]$/g, '')` before blocklist check |
| `app/api/pulse/chat/claude-stream-types.ts` | Removed `process.env.HOME` from ALLOWED_DIR_ROOTS |
| `__tests__/mcp/route.test.ts` | Vitest v4: dropped generic params from `vi.fn`; bracket-strip in inline `validateStatusUrl` |
| `__tests__/pulse-chat-route-streaming.test.ts` | Updated session IDs to valid hex UUID format |
| `__tests__/pulse-rag.test.ts` | Added `BASE_REQUEST_EXTRAS` spread with 5 missing required fields |
| `__tests__/pulse-chat-pane-layout.test.ts` | Replaced `mobilePane`/`onMobilePaneChange`/`isDesktop` with `sourcesExpanded`/`onSourcesExpandedChange`; updated snapshots |
| `__tests__/pulse-ui-smoke.test.ts` | Same prop replacement; added `vi.mock('next/navigation')`; removed stale `'Pulse Chat'`/`'1 src'` assertions |

---

## Commands Executed

```bash
# Pre-flight
cat apps/web/package.json | grep react-error-boundary   # → not installed
npx tsc --noEmit                                         # → errors found, fixed iteratively
pnpm lint                                                # → 9 infos, no errors
pnpm test                                                # → 231/231 pass

# Snapshot update
npx vitest run -u                                        # → 2 snapshots updated

# Diagnostic
node -e "console.log(new URL('http://[::1]/sse').hostname)"  # → [::1] (with brackets)
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| MCP PUT/DELETE | No CSRF protection | Requires `X-Pulse-Request: 1` header; 403 otherwise |
| MCP PUT body | Manual `typeof` guard | Zod v4 schema — command allowlist, arg caps, env key regex |
| MCP status URL | Raw `fetch()` to any URL | SSRF guard: rejects private IPs, localhost, non-http(s), IPv6 loopback |
| MCP stdio command | No validation | Rejects commands with path separators before `which` |
| `addDir` CLI arg | Passed raw to Claude | Validated against `/home/node`, `/tmp`, `/workspace` roots |
| `allowedTools`/`disallowedTools` | Raw passthrough | Filtered to `^[a-zA-Z][a-zA-Z0-9_*(),:]*$` per entry |
| `--dangerously-skip-permissions` | Always injected | Gated by `PULSE_SKIP_PERMISSIONS !== 'false'` |
| `sessionId` | `min(1).max(256)` | `^[0-9a-f-]{8,64}$` (hex/UUID only) |
| Session JSONL lines | Unbounded | 512KB max; null bytes stripped |
| Session path traversal | Not checked | `startsWith(root + sep)` guard on every `path.join` |
| `replayCache` | Only pruned on requests | Also pruned every 60s via `setInterval` |
| stderr accumulation | Unbounded | Capped at 16KB |
| `workspaceMode` init | `localStorage` in render | Safe default `'pulse'`; read from storage in `useEffect` |
| `pulseModel`/`pulsePermissionLevel` | Transient (reset on reload) | Persisted to `axon.web.pulse-model` / `axon.web.pulse-permission` |
| New session | Started without cancelling in-flight | Calls `handleCancelPrompt()` before clearing history |
| Copy button | Single shared `copyStatus` state | Per-message `Map<messageId, status>` |
| Reset Settings | Single click resets immediately | Two-click with 5s auto-cancel window |
| Service Worker | Installed but no update handling | Reloads page when new SW installed while one is active |
| `McpServerForm` tab sync | JSON overwritten on any tab switch | Only regenerated when switching TO json AND not manually edited |
| `KvEditor` keys | Index-based (unstable) | `crypto.randomUUID()` stable IDs |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `npx tsc --noEmit` | 0 errors | 0 errors | ✅ |
| `pnpm lint` | No errors | 9 infos only | ✅ |
| `pnpm test` | 231 pass | 231/231 pass, 27/27 files | ✅ |
| `npx vitest run -u` | Snapshots updated | 2 snapshots updated | ✅ |

---

## Deferred Items (Out of Scope for This PR)

| Item | Reason |
|------|--------|
| CSS variable name swap (`--axon-accent-blue` ↔ `--axon-accent-pink`) | Requires touching ALL .tsx files simultaneously; separate PR |
| Session ID 12-char SHA collision | Low probability at current scale |
| Split pane `aria-valuenow={0}` | Minor accessibility polish; separate ticket |
| `use-split-pane.ts` duplicate pointermove listeners | Performance micro-optimization |
| `dotenv` custom parser in `server-env.ts` | Correctness risk, not security hole; separate PR |

---

## Risks and Rollback

- **CSRF header requirement**: MCP page PUT/DELETE now requires `X-Pulse-Request: 1`. Agent 4 added this to the client-side fetch calls. If the header is missing from any client path not covered by Agent 4, those calls will get 403s. Rollback: remove the header check in `mcp/route.ts` PUT/DELETE handlers.
- **`sessionId` regex tightening**: Any existing Claude session with a non-hex ID (e.g. from older CLI versions using `sess-` prefix) will be rejected. Rollback: revert `types.ts:38` to `z.string().min(1).max(256).optional()`.
- **`PULSE_SKIP_PERMISSIONS` gate**: If `PULSE_SKIP_PERMISSIONS=false` is accidentally set in the container env, Claude CLI won't get `--dangerously-skip-permissions` and will prompt for permissions on every action, breaking the Pulse flow. Default is unchanged (flag is always added unless explicitly disabled).
- **`workspaceMode` hydration change**: Brief flash of `'pulse'` default before localStorage value loads. Acceptable UX trade-off vs SSR hydration mismatch error.

---

## Open Questions

- Should `ALLOWED_DIR_ROOTS` be configurable via env var (`PULSE_ALLOWED_DIRS`) for self-hosters who mount volumes at non-standard paths?
- The `sessionId` regex update may break session resume for users on older Claude CLI versions that use `sess-XXXXXXXX` format — needs validation against actual CLI output formats.
- The service worker reload-on-update (7b) will interrupt active Pulse sessions on SW update. Should this be gated behind a toast notification instead of immediate reload?

---

## Next Steps

- Open PR on `feat/crawl-download-pack` → `main` with this remediation
- Verify MCP CRUD still works end-to-end in the UI (PUT/DELETE with `X-Pulse-Request` header)
- Validate `sessionId` resume flow works with actual Claude CLI session IDs
- Add `ALLOWED_DIR_ROOTS` to `.env.example` as an optional override
