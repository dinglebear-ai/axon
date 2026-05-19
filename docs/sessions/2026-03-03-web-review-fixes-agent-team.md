# Web App 27-Issue Fix + Review: Agent Team Operation
**Date:** 2026-03-03 | **Branch:** `feat/sidebar`

## Session Overview

Deployed a 3-phase agent team operation to fix 27 review findings across `apps/web/`. Phase 1: 4 parallel implementation agents fixed all 27 issues. Phase 2: 2 code reviewers (CodeRabbit + feature-dev) independently analyzed all changes and found 11 additional issues. Phase 3: 3 fixer agents addressed all 11 review findings. Total: 37 fixes, 618 tests passing, 68 files changed.

## Timeline

1. **Team creation + task decomposition** — Created `web-fixes` team with 5 tasks, strict file ownership boundaries to prevent conflicts
2. **Phase 1 — 4 implementation agents** (parallel):
   - `api-routes-agent`: 9 issues (SC-11, CQ-14/15/17/18, AR-5/6/7/8) — API routes security, Zod validation, error standardization, refactoring
   - `pulse-components-agent`: 7 issues (PF-11/13/14/16, AR-3/4/9) — scroll batching, virtualization, memoization, occlusion detection, workspace split, type unification
   - `hooks-editor-agent`: 5 issues (CQ-12/13/16/19/20) — omnibox split, editor type casts, stale TODOs, useTimedNotice extraction, closure fix
   - `infra-tests-agent`: 4 issues (PF-12/15/17, CQ-21) — logLines cap, HTTP caching, replayCache eviction, 74 new tests
3. **Phase 2 — 2 code reviewers** (parallel):
   - `coderabbit-reviewer`: Found 3 critical, 3 high, 5 medium, 7 low findings
   - `feature-dev-reviewer`: Found 2 bugs, 2 security gaps, 2 notes; confirmed 8+ refactors correct
   - `superpowers-reviewer`: Unresponsive — went idle repeatedly without delivering findings
4. **Phase 3 — 3 fixer agents** (parallel):
   - `security-fixer`: IPv6 SSRF hardening, auth bypass fix, 23 tests
   - `api-fixer`: PG pool confirmed correct, Enter key guard, jobs status validation, 22 tests
   - `hooks-fixer`: 5 hook dependency/cleanup fixes

## Key Findings

- **PG Pool singleton was already correct** — CodeRabbit flagged it as critical but api-fixer confirmed the globalThis singleton pattern was already in place for both dev and production
- **SSRF IPv6-mapped IPv4 bypass was real** — `::ffff:127.0.0.1` bypassed all hostname/IP checks; required full IPv6 parser with mapped-address detection
- **Auth bypass on empty token** — `?? ''` vs `|| null` caused empty `AXON_WEB_API_TOKEN` to fall through to insecure dev path
- **`chat` object in dep arrays** — React anti-pattern where `usePulseChat` returns new object literal every render, causing all downstream effects/callbacks to re-run
- **DNS rebinding is a known limitation** — documented in url-validation.ts, cannot be solved at URL-parse time without DNS resolution step

## Technical Decisions

- **IPv6 parsing**: Built custom `parseIpv6()` + `isBlockedIpv6()` rather than using a library — keeps the validation self-contained and avoids new dependencies
- **Omnibox split**: 3 sub-hooks (mentions, execution, keyboard) composed in main hook — preserves exact public API shape
- **PulseWorkspace split**: Behavioral wiring extracted to `usePulseWorkspaceBehavior` hook, component reduced from 540 to ~220 lines
- **Type unification (AR-9)**: `lib/pulse/types.ts` is canonical source, `ws-messages/types.ts` re-exports via type aliases
- **Replay cache eviction**: Running byte total maintained at module level (O(1) checks) instead of iterating all entries (O(n))
- **@tanstack/react-virtual**: Already in deps but unused — adopted for pulse-chat-pane with `measureElement` for actual heights, threshold at 120 messages
- **NeuralCanvas occlusion**: IntersectionObserver pauses animation loop when `intersectionRatio < 0.01`

## Files Modified

### New Files Created
| File | Purpose |
|------|---------|
| `lib/server/api-error.ts` | Shared `apiError()` + `makeErrorId()` utility |
| `lib/server/url-validation.ts` | SSRF validation with IPv4/IPv6 blocking |
| `hooks/use-timed-notice.ts` | Extracted setTimeout pattern for notices |
| `hooks/use-pulse-workspace.ts` | Behavioral wiring from PulseWorkspace |
| `components/omnibox/hooks/use-omnibox-mentions.ts` | Mention suggestions sub-hook |
| `components/omnibox/hooks/use-omnibox-execution.ts` | Command execution sub-hook |
| `components/omnibox/hooks/use-omnibox-keyboard.ts` | Keyboard handling sub-hook |
| `__tests__/url-validation.test.ts` | 23 SSRF validation tests |
| `__tests__/ws-messages-handlers.test.ts` | 19 dispatcher tests |
| `__tests__/replay-cache-eviction.test.ts` | 8 eviction tests |
| `__tests__/workspace-persistence.test.ts` | 36 persistence tests |
| `__tests__/axon-ws-logic.test.ts` | 11 WebSocket logic tests |
| `__tests__/use-timed-notice.test.ts` | Timer hook tests |
| `__tests__/pg-pool.test.ts` | PG pool singleton tests |
| `__tests__/jobs-route.test.ts` | Jobs route validation tests |
| `__tests__/pulse-op-confirmation.test.ts` | Enter key guard tests |

### Key Modified Files
| File | Changes |
|------|---------|
| `middleware.ts` | Auth bypass fix (`?? ''` → `|| null`) |
| `next.config.ts` | HTTP caching headers for cortex routes |
| `app/api/pulse/chat/route.ts` | Zod validation, apiError(), refactored to helpers |
| `app/api/pulse/source/route.ts` | SSRF validation before subprocess |
| `app/api/ai/command/route.ts` | Zod schema, model ID constants |
| `app/api/jobs/route.ts` | Status filter validation, apiError() |
| `components/pulse/pulse-chat-pane.tsx` | @tanstack/react-virtual, scroll batching |
| `components/pulse/pulse-workspace.tsx` | Layout-only after behavioral extraction |
| `components/pulse/pulse-editor-pane.tsx` | Memoized markdownToPlateNodes |
| `components/pulse/pulse-op-confirmation.tsx` | Enter key guard |
| `components/app-shell.tsx` | Removed crawl state prop drilling |
| `components/neural-canvas-core.tsx` | IntersectionObserver occlusion |
| `hooks/use-pulse-chat.ts` | useTimedNotice, configRef pattern |
| `hooks/use-ws-messages.ts` | Context split, useMemo deps fix |
| `hooks/ws-messages/handlers.ts` | logLines cap via pushCapped |
| `hooks/ws-messages/runtime.ts` | MAX_LOG_LINES constant |
| `components/omnibox/omnibox-hooks.ts` | Composition of 3 sub-hooks |
| `components/editor/use-chat.ts` | Stale closure fix, TODO removal |
| `app/api/pulse/chat/replay-cache.ts` | Running byte total eviction |

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| SSRF | User URLs passed to axon subprocess unchecked | Private IPs, IPv6 ULA/link-local/multicast/mapped blocked |
| Auth | Empty API token fell through to insecure dev path | Empty/whitespace tokens treated as unset |
| Jobs API | Invalid status filter returned all rows (200) | Returns 400 with error details |
| Enter key | Global capture during confirmation dialogs | Skips when input/textarea/contenteditable focused |
| Scroll | 3 setState + sync localStorage per event | Batched updates, 150ms debounced persistence |
| Virtualization | Custom fixed-height spacers | @tanstack/react-virtual with measured heights |
| NeuralCanvas | Animated under opaque overlays | Paused when fully occluded |
| Error format | 4 different shapes across API routes | Unified `apiError()` format |
| Omnibox hook | 24 state vars, 474 lines monolith | 3 focused sub-hooks composed together |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `npx vitest run` | All tests pass | 618 pass, 0 fail (51 files) | PASS |
| SSRF IPv6-mapped test | Block `::ffff:127.0.0.1` | Blocked | PASS |
| SSRF ULA test | Block `fc00::1` | Blocked | PASS |
| Auth empty token | Reject empty API token | Returns 401 | PASS |
| Jobs invalid filter | Return 400 | Returns 400 with `invalid_status_filter` | PASS |

## Risks and Rollback

- **DNS rebinding**: Known unmitigated SSRF vector — documented but not fixable at URL-parse time. Would require DNS resolution step before subprocess spawn.
- **Omnibox split**: Public API preserved but internal state flow changed — regression risk if sub-hooks have ordering dependencies
- **PulseWorkspace split**: Behavioral hook extracted — if any component relies on PulseWorkspace internal state that wasn't exposed, it will break
- **Rollback**: `git stash` or `git checkout -- apps/web/` to revert all changes

## Decisions Not Taken

- **DNS resolution for SSRF**: Would fully prevent DNS rebinding but adds latency, complexity, and async DNS dependency — documented as known gap instead
- **Full LRU cache for replayCache**: Simple oldest-entry eviction chosen over proper LRU — 64-entry cap makes the difference negligible
- **Nonce-based CSP**: Would replace `unsafe-inline` but requires Next.js middleware integration — out of scope for this batch

## Open Questions

- `superpowers-reviewer` agent was completely unresponsive across 4 shutdown requests — may indicate an agent type issue worth investigating
- Default permission level changed to `bypass-permissions` in `lib/pulse/types.ts:54` — is this intentional for dev, or should it be `accept-edits` for safety?
- `TagDef` and `TaggedItem` types added to sidebar types but never consumed — dead code or future prep?
- LogsViewer `estimateSize: 20` uses fixed estimate — may need `measureElement` if log lines wrap

## Next Steps

- Verify the `superpowers-reviewer` zombie eventually terminates or investigate the agent type
- Consider nonce-based CSP as a follow-up security improvement
- Review the `bypass-permissions` default with the user
- Clean up any remaining dead types if confirmed unused
