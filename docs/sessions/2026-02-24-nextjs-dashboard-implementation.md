# Next.js Dashboard Implementation

**Date:** 2026-02-24
**Branch:** `fix-crawl`
**Commits:** `b4ac497..0fec6d7` (6 commits, +9,932 lines across 22 files)

## Session Overview

Built the full Axon dashboard web UI at `apps/web/` using Next.js 16, React 19, Plate.js, and shadcn/ui. Executed a 13-task implementation plan via subagent-driven development — parallelizing independent tasks for throughput while sequencing dependency chains.

## Timeline

1. **Task 1** — Installed shadcn/ui primitives: button, input, tabs, scroll-area, badge (`b4ac497`)
2. **Tasks 2, 4, 11 (parallel)** — WS protocol types, bioluminescent theme, WS proxy config (`7259337`)
3. **Task 3 (sequential gateway)** — WS connection hook with exponential backoff + providers (`a5829b1`)
4. **Tasks 5, 6, 7, 8, 9, 12 (parallel)** — All dashboard components + markdown parser (`8a9a184`)
5. **Task 10** — Dashboard page assembly wiring all components (`d5a8722`)
6. **Task 13** — Biome v2.4 migration, lint/format fixes, hook circular dep fix (`0fec6d7`)

## Key Findings

- **Biome v2.4 breaking change:** `files.ignore` renamed to `files.includes` with negated glob patterns (`!**/*.css`). Migration tool (`biome migrate`) doesn't catch this — manual fix required.
- **Biome + Tailwind v4 incompatibility:** Biome cannot parse `@custom-variant`, `@theme`, `@apply` — CSS files must be excluded from Biome entirely.
- **Circular useCallback dependency:** `connect` ↔ `scheduleReconnect` mutual dependency resolved via `connectRef` pattern — scheduleReconnect uses `connectRef.current()` instead of direct `connect` reference.
- **`NO_INPUT_MODES` type widening:** `Set<ModeId | string>` defeats type safety. Fixed with `as const` tuple: `new Set([...] as const)`.
- **Neural canvas is inherently monolithic:** 1081 lines of imperative canvas code with tightly coupled animation classes — cannot be meaningfully split. Added to `.monolith-allowlist`.

## Technical Decisions

| Decision | Rationale |
|----------|-----------|
| `connectRef` for WS reconnect | Breaks circular useCallback dependency without suppressing lint |
| Dark-only theme (oklch) | Matches existing bioluminescent canvas aesthetic; single palette simplifies |
| `next/dynamic` for NeuralCanvas | Canvas uses `requestAnimationFrame` — SSR would throw |
| Inline `simpleMarkdownToHtml()` | Avoids heavy deps for simple WS output rendering; Plate handles full editor |
| `forwardRef` + `useImperativeHandle` on Omnibox/Canvas | Parent needs imperative control (handleDone, setIntensity) |
| Biome over ESLint | Already configured in project; v2.4 is type-aware without TSC dep |

## Files Modified

### New Files (18)
| File | Purpose | Lines |
|------|---------|-------|
| `apps/web/lib/ws-protocol.ts` | WS message types, mode definitions, ModeId type | 124 |
| `apps/web/hooks/use-axon-ws.ts` | WS connection hook with exponential backoff | 116 |
| `apps/web/app/providers.tsx` | Context wrapper (WS + Tooltip) | 14 |
| `apps/web/components/ws-indicator.tsx` | Connection status badge | 22 |
| `apps/web/components/omnibox.tsx` | Command input with 16 modes | 221 |
| `apps/web/components/results-panel.tsx` | Tabbed output (Content/Stats/Recent) | 428 |
| `apps/web/components/neural-canvas.tsx` | Bioluminescent canvas animation | 1180 |
| `apps/web/components/docker-stats.tsx` | Live container metrics grid | 102 |
| `apps/web/lib/markdown.ts` | Plate.js markdown deserializer | 41 |
| `apps/web/biome.json` | Biome v2.4 config with Tailwind CSS exclusion | 57 |
| `apps/web/next.config.ts` | Standalone output + WS proxy rewrite | 21 |
| `apps/web/components/ui/{button,input,tabs,scroll-area,badge}.tsx` | shadcn/ui primitives | ~266 |

### Modified Files (4)
| File | Change |
|------|--------|
| `apps/web/app/globals.css` | Replaced gray palette with oklch bioluminescent theme |
| `apps/web/app/layout.tsx` | DM Sans/Mono fonts, dark mode, Providers wrapper |
| `apps/web/app/page.tsx` | Full dashboard assembly with all components wired |
| `.monolith-allowlist` | Added `apps/web/components/neural-canvas.tsx` |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `pnpm biome check .` | 0 errors | `Checked 54 files. No fixes applied.` | PASS |
| `pnpm build` | Clean build | `Compiled successfully in 5.1s`, 3 static pages | PASS |
| `git diff --stat HEAD~6..HEAD` | All 22 files staged | 22 files, +9932 insertions | PASS |
| lefthook pre-commit | Monolith pass | `Monolith policy check passed` | PASS |

## Behavior Changes

| Before | After |
|--------|-------|
| `apps/web/` had skeleton Next.js with Plate editor page only | Full dashboard: omnibox, results panel, neural canvas, Docker stats, WS indicator |
| No WS connection to axum backend | Auto-connecting WS with exponential backoff (1s–30s) |
| No command execution UI | 16 command modes with run/cancel via WS |
| Static page only | Live Docker container stats via WS broadcast |

## Risks and Rollback

- **Low risk:** All changes are additive within `apps/web/` — no Rust code modified
- **Rollback:** `git revert 0fec6d7..b4ac497` or `git reset --hard 891449b`
- **Runtime dependency:** Dashboard requires axum `serve` command running for WS — gracefully shows "DISCONNECTED" when unavailable

## Decisions Not Taken

- **Did not use React Server Components** — Dashboard is fully client-side (WS, canvas, imperative refs)
- **Did not split neural-canvas.tsx** — Imperative canvas classes are tightly coupled; splitting would create artificial boundaries
- **Did not add tests** — Plan scope was UI implementation only; E2E tests are a follow-up

## Open Questions

- `axon-ws-bridge.py` and `axon-interface.html` in repo root are superseded — pending deletion
- E2E tests for WS reconnection and command execution flows not yet written
- `apps/web/app/editor/` page exists from prior setup — unclear if it should stay or be removed

## Next Steps

1. Delete superseded files (`axon-ws-bridge.py`, `axon-interface.html`)
2. Add E2E tests (Playwright) for WS connection, command execution, Docker stats
3. Wire `serve` command to serve both axum API and Next.js in production
4. Consider code-splitting neural-canvas.tsx if performance becomes an issue
