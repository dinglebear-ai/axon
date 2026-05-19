# Session: Web Test Coverage Expansion

**Date:** 2026-03-03
**Branch:** `feat/sidebar`
**Duration:** ~30 minutes

## Session Overview

Expanded `apps/web` Vitest test coverage from 248 tests (27 files) to 405 tests (35 files) by adding 8 new test files covering pure-logic modules, stream parsers, caching, prompt utilities, and WebSocket message runtime transforms.

## Timeline

1. Assessed current test state: 27 test files, 248 tests, all passing
2. Identified 81 source files in coverage scope (`lib/`, `app/api/`, `hooks/`, `components/`)
3. Classified ~48 untested files by testability (pure-logic, api-route, hook, types-only, thin-wrapper)
4. Selected 8 highest-ROI pure-logic files for coverage
5. Read all 8 source files + their type dependencies
6. Wrote all 8 test files
7. Fixed `terminal-history.test.ts` — needed `window` + `localStorage` stubs for Vitest node environment (TerminalHistory checks `typeof window === 'undefined'` for SSR safety)
8. Final run: 35 test files, 405 tests, all passing

## Key Findings

- **TerminalHistory SSR guard**: `typeof window === 'undefined'` check in `load()` means Vitest node environment treats it as SSR by default — must stub both `window` and `localStorage` globals (`terminal-history.ts:78`)
- **stream-parser.ts** mutates state in-place with tool_use dedup by ID, thinking block coalescence, and result truncation to 600 chars — all verified
- **replay-cache.ts** uses module-level `Map` singleton with `setInterval` TTL pruning — only runs server-side (`typeof window === 'undefined'` guard)
- **ai/command/utils.ts** has editor-dependent functions (`addSelection`, `getMarkdownWithSelection`, `isSelectionInTable`) that need SlateEditor mocks — skipped, tested only pure functions

## Technical Decisions

- **Pure-logic first**: Targeted files with zero or minimal mocking needs for maximum coverage per test-writing effort
- **No `@vitest/coverage-v8`**: Coverage dep missing from `package.json` — tests run fine without it, coverage report unavailable. Did not add it (not requested).
- **`vi.stubGlobal` over jsdom**: Kept `environment: 'node'` (matching existing vitest.config.ts) and selectively stubbed `window`/`localStorage` rather than switching to jsdom
- **Module-level `vi.resetModules()`** in `server-env.test.ts`: Required because `ensureRepoRootEnvLoaded` has a module-level `rootEnvLoaded` guard that makes it idempotent — each test needs a fresh module import

## Files Modified

| File | Action | Purpose |
|------|--------|---------|
| `__tests__/structured-text.test.ts` | Created | 27 tests for formatStructuredText + summarizeStructuredValue |
| `__tests__/download-urls.test.ts` | Created | 7 tests for URL encoder helpers |
| `__tests__/terminal-history.test.ts` | Created | 18 tests for command history class |
| `__tests__/stream-parser.test.ts` | Created | 22 tests for Claude NDJSON stream parser |
| `__tests__/replay-cache.test.ts` | Created | 10 tests for replay cache TTL + key computation |
| `__tests__/ai-command-utils.test.ts` | Created | 27 tests for prompt formatting utilities |
| `__tests__/ws-messages-runtime.test.ts` | Created | 22 tests for WS message transforms + reducer |
| `__tests__/server-env.test.ts` | Created | 9 tests for .env parser + idempotent loading |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `npx vitest run` (before) | 248 tests, 27 files | 248 passed, 27 files | PASS |
| `npx vitest run` (after) | 405 tests, 35 files | 405 passed, 35 files | PASS |
| Duration | <5s | 2.56s | PASS |

## Behavior Changes (Before/After)

No runtime behavior changes — test-only additions.

## Risks and Rollback

- **Risk**: None — test files only, no source changes
- **Rollback**: `git checkout -- __tests__/` removes all new test files

## Decisions Not Taken

- **jsdom environment**: Could have switched vitest.config.ts to jsdom for localStorage/window support, but existing tests rely on node environment — would be a larger change
- **Hook testing**: React hooks (`use-debounce`, `use-pulse-chat`, etc.) need `@testing-library/react` or similar — deferred to a follow-up session
- **Coverage report**: `@vitest/coverage-v8` not installed — adding it is a separate concern

## Open Questions

- Should `@vitest/coverage-v8` be added to track coverage % formally?
- Should hook tests be next priority, or API route handler tests?

## Next Steps

- Add `@vitest/coverage-v8` and establish baseline coverage percentage
- Test API route handlers (cortex/stats, cortex/domains, pulse/save, pulse/doc)
- Test React hooks with `@testing-library/react` (use-debounce, use-pulse-chat, use-axon-ws)
- Test `hooks/ws-messages/handlers.ts` — complex message dispatch logic
