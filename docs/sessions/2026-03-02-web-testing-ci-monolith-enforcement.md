# Session: Web Testing CI Gate + Monolith Enforcement Hardening
**Date:** 2026-03-02
**Branch:** `feat/sidebar`

## Session Overview

Investigated the `apps/web/` testing situation, fixed two bugs (one production-impacting streaming bug), added CI enforcement for web tests/lint, added coverage config, and hardened the monolith allowlist to prevent exception abuse over actual file splitting.

## Timeline

1. **Explored web test landscape** — Discovered 27 test files, 248 tests, Vitest 4, node environment. No CI enforcement, no coverage tracking.
2. **Found and fixed streaming route bug** — `route.ts:272` had a `closed = true` at the top of the child `close` handler that prevented all subsequent `emit()` and `safeClose()` calls. Streams never closed for clients. Production bug.
3. **Fixed stale pane switcher test** — Component uses `aria-label="Chat pane"` but test expected `"Show chat pane"`.
4. **Added coverage config** — `v8` provider in `vitest.config.ts`, `test:coverage` script in `package.json`.
5. **Added `web-lint-test` CI job** — pnpm install + biome lint + vitest run, gates PRs.
6. **Reviewed existing hooks** — Found `ts-lint.py` (Biome auto-fix on Edit/Write) and `cargo-check.py` (fmt + check on .rs edits) already in place.
7. **Hardened monolith enforcement** — Blocked `.monolith-allowlist` edits via PreToolUse hook, added expiry date enforcement (no date = violation, expired = violation, >7 days out = violation).
8. **Extracted inline hook to script** — Replaced brittle one-liner in settings.json with `monolith-allowlist-guard.py`.

## Key Findings

- **Production streaming bug** (`app/api/pulse/chat/route.ts:272`): The `closed` boolean served double duty as both a stream-controller guard and a child-event dedup flag. Setting it `true` inside `child.on('close')` silently killed all subsequent `emit()` calls and `safeClose()`. Clients would wait forever for response body.
- **Test runtime impact**: Streaming tests went from 5000ms timeout → 650ms after the fix.
- **Biome PostToolUse hook** (`ts-lint.py`): runs `biome check --write` on every TS/JS edit. This caused a race condition during multi-step edits — declaring `let childHandled` was auto-fixed to `const` before the assignments were added in a subsequent edit.
- **Existing hook coverage**: Rust has `cargo-check.py` (fmt + check), TS/JS has `ts-lint.py` (biome), Python has `ruff-fix.py` + `ty-check.py`, Go has `gofmt-check.py`. Full language coverage already in place.
- **Monolith allowlist was unguarded**: Any agent could add entries instead of splitting files. No expiry enforcement meant entries could persist indefinitely.

## Technical Decisions

| Decision | Rationale |
|----------|-----------|
| Separate `childHandled` flag vs fixing `closed` | `closed` guards the ReadableStream controller; `childHandled` guards child event dedup. Different concerns need different flags. Same pattern as Node.js `destroyed` vs `ended`. |
| `v8` coverage provider (not `istanbul`) | Built into Vitest 4, zero additional deps. `istanbul` would require `@vitest/coverage-istanbul`. |
| PreToolUse block on allowlist + expiry enforcement | Defense in depth: hook prevents Claude from editing, expiry prevents manual entries from rotting. |
| Script file over inline one-liner | Inline Python in JSON is brittle (escaping, readability, debugging). Proper script is testable and maintainable. |
| 7-day max expiry window | Tight enough to force action, long enough for a sprint. Configurable via `--allowlist-expiry-days`. |

## Files Modified

| File | Purpose |
|------|---------|
| `apps/web/app/api/pulse/chat/route.ts` | Fixed streaming bug: added `childHandled` flag, changed dedup guards in `error`/`close` handlers |
| `apps/web/__tests__/pulse-mobile-pane-switcher.test.ts` | Updated stale assertion (`"Show chat pane"` → `aria-label="Chat pane"`) |
| `apps/web/vitest.config.ts` | Added v8 coverage config targeting lib/api/hooks/components |
| `apps/web/package.json` | Added `test:coverage` script |
| `.github/workflows/ci.yml` | Added `web-lint-test` job (pnpm + biome + vitest) |
| `~/.claude/hooks/monolith-allowlist-guard.py` | New: PreToolUse hook blocking `.monolith-allowlist` and `enforce_monoliths.py` edits |
| `~/.claude/settings.json` | Replaced inline one-liner with script reference |
| `scripts/enforce_monoliths_helpers.py` | Added `AllowlistEntry`, `load_allowlist_entries()`, `check_allowlist_expiry()`, `EXPIRES_RE`, `DEFAULT_ALLOWLIST_EXPIRY_DAYS` |
| `scripts/enforce_monoliths_impl.py` | Wired expiry check into `main()`, added `--allowlist-expiry-days` flag, updated violation message |

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| Pulse chat streaming | Stream never closed after child process exited; clients waited until timeout/disconnect | Stream properly emits final events and closes via `controller.close()` |
| Streaming test suite | 6/8 tests timeout at 5s (31.5s total) | 8/8 pass in 793ms |
| Web tests in CI | Not run — test failures invisible on PRs | `web-lint-test` job gates PRs with biome lint + vitest |
| Coverage tracking | None | `pnpm test:coverage` available; v8 provider on lib/api/hooks/components |
| Monolith allowlist | Unguarded — any agent could add exceptions | PreToolUse hook blocks edits; entries require `# expires: YYYY-MM-DD` within 7 days |
| Monolith violation message | "Add temporary exceptions to .monolith-allowlist" | "Split the file — do not add allowlist exceptions." |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `pnpm test` (all web tests) | 248 pass, 0 fail | 248 pass, 0 fail, 2.17s | PASS |
| `pnpm lint` | Exit 0, warnings only | Exit 0, 17 warnings (pre-existing a11y) | PASS |
| `enforce_monoliths.py --self-test` | self-test passed | self-test passed | PASS |
| Entry without date → enforcer | Violation | `has no expiry date` violation | PASS |
| Expired entry → enforcer | Violation | `expired 29 day(s) ago` violation | PASS |
| Valid future entry → enforcer | Pass | Monolith policy check passed | PASS |
| Hook on `.monolith-allowlist` edit | Exit 2, blocked | Exit 2, "Split the file" stderr | PASS |
| Hook on normal `.rs` file | Exit 0 | Exit 0 | PASS |

## Risks and Rollback

- **Streaming fix**: Low risk — the previous behavior was strictly broken (streams never closing). Rollback: revert `childHandled` flag and restore `closed = true` in handlers (restores the bug).
- **CI job**: Low risk — additive. If it causes issues, comment out the `web-lint-test` job in `ci.yml`.
- **Allowlist guard hook**: Remove `monolith-allowlist-guard.py` entry from `settings.json` PreToolUse hooks to restore unguarded allowlist.
- **Expiry enforcement**: Set `--allowlist-expiry-days 9999` to effectively disable, or remove `check_allowlist_expiry()` call from `enforce_monoliths_impl.py`.

## Decisions Not Taken

- **jsdom environment for component tests** — Higher effort, requires new dependency (`@vitest/environment-jsdom`), most tests are logic-focused. Deferred.
- **Playwright E2E** — High effort to set up browser automation infra. Not "lowest effort, highest gain."
- **Blocking `enforce_monoliths_helpers.py` edits** — Initially included but reverted. The helpers need to be editable to add new enforcement features (like the expiry check itself).
- **Expiry dates on commented-out (resolved) entries** — No active allowlist entries exist currently. The format only applies to active (uncommented) entries.

## Open Questions

- The streaming bug may have caused production issues (Pulse chat hanging until client timeout). Worth checking if users experienced this.
- Should the `web-lint-test` CI job also run `pnpm build` to catch Next.js build errors? Currently only lint + test.
- The 7-day expiry window is a starting value. May need adjustment based on real usage patterns.

## Next Steps

- Consider adding `pnpm build` to the CI job for build-time type checking
- Run `pnpm test:coverage` to baseline current coverage and identify gaps
- Consider Playwright setup for critical Pulse chat E2E flows (future session)
- Monitor Pulse chat streaming behavior post-fix to confirm production impact
