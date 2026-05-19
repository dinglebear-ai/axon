# Web UI Command Parity — Full 4-Phase Implementation

**Date:** 2026-02-24
**Branch:** `fix-crawl`
**Plan:** `/home/jmagar/.claude/plans/linear-coalescing-plum.md`

## Session Overview

Implemented complete web UI command support for the Axon dashboard across 4 phases using subagent-driven development. Before this work, only `scrape` and `crawl` rendered output in the web UI — all other commands ran blind because `execute.rs` silently drained stdout. After this work, all 20+ commands stream output to the frontend with typed renderers.

## Timeline

1. **Phase A — Core Unlock** (commits `9009d80`, `94a7951`, `2ca4446`, `24446bf`)
   - Fixed the root cause: `while let Ok(Some(_)) = lines.next_line().await {}` silently drained stdout
   - Split `execute.rs` into module directory (`execute/mod.rs`, `files.rs`, `polling.rs`)
   - Added `stdout_json`, `stdout_line`, `command_start` WS message types
   - Expanded `ALLOWED_FLAGS` whitelist from ~10 to 30 entries
   - Built `RawRenderer` fallback component
   - Added `--json` injection for all sync commands (except `search`/`research` which lack `--json` support)

2. **Phase B — Typed Renderers** (commits `1312841`, `639b208`)
   - Created `result-types.ts` (207L) — TypeScript interfaces for every command's JSON output
   - Created `result-normalizers.ts` (156L) — per-mode shape validators with raw fallback
   - Built 4 renderers: `table-renderer.tsx` (532L), `cards-renderer.tsx` (109L), `report-renderer.tsx` (390L), `status-renderer.tsx` (163L)
   - Renderer dispatch via `renderIntent` field in `AXON_COMMAND_SPECS`

3. **Phase C — Job Lifecycle + Mode Completeness** (commits `4f5411d`, `8fdbc2e`)
   - Built `job-lifecycle-renderer.tsx` (350L) — enqueue confirmation, auto-poll status, cancel button
   - Added all missing modes to omnibox picker, grouped by `AxonCommandCategory`
   - Fixed cancel routing, `PHASE_META` record, removed ingest from job lifecycle

4. **Phase D — Polish** (commits `e4e09ce`, `2922f53`)
   - Built `command-options-panel.tsx` (192L) — collapsible per-mode flag controls
   - Added helper selectors: `getCommandSpec()`, `isAsyncMode()`, `isNoInputMode()`, `getCommandsByCategory()`
   - Populated `target` field in recent runs, added `rendererUsed`
   - Review fixes: Rules of Hooks compliance, `depth` flag in whitelist, exhaustive deps

## Key Findings

- **Root cause of blind commands:** `execute.rs:318` had `while let Ok(Some(_)) = lines.next_line().await {}` — silently consuming all stdout from sync commands
- **`--json` not universal:** `search` and `research` commands have no `--json` support — must stream raw text
- **Rules of Hooks violation:** `useCallback` appeared after conditional `return null` guards in `CommandOptionsPanel` — React would crash on render order changes
- **Silent flag dropping:** Backend `ALLOWED_FLAGS` whitelist silently drops any flag not listed — `depth` (reddit) was missing
- **Stale closure:** `optionValues` missing from `executeCommand` deps array in `omnibox.tsx` — Biome caught this

## Technical Decisions

- **Module directory for execute.rs:** Split into `mod.rs` + `files.rs` + `polling.rs` to stay under monolith policy (500L limit)
- **Renderer dispatch via spec:** `renderIntent` field in `AXON_COMMAND_SPECS` drives which React component renders, rather than hardcoded switch on mode name
- **Raw fallback everywhere:** Every renderer falls back to `<RawRenderer>` if normalization fails — no command can ever be blank
- **Capped stdout arrays:** `MAX_STDOUT_LINES = 10000`, `MAX_STDOUT_JSON = 5000` — prevents memory issues on long-running commands
- **Shared `CopyButton`:** Extracted from duplicate implementations across renderers into `results/shared.ts`

## Files Modified

### Rust Backend
| File | Purpose |
|------|---------|
| `crates/web/execute/mod.rs` | Stdout streaming, `--json` injection, flag whitelist expansion |
| `crates/web/execute/files.rs` | File-based output handlers (scrape, screenshot) |
| `crates/web/execute/polling.rs` | Crawl/job progress polling |

### Frontend — Core
| File | Purpose |
|------|---------|
| `apps/web/lib/ws-protocol.ts` | Added `stdout_json`, `stdout_line`, `command_start` message types |
| `apps/web/hooks/use-ws-messages.ts` | Stdout accumulation, `commandMode` tracking, capped arrays |
| `apps/web/lib/axon-command-map.ts` | Full command spec with `renderIntent`, `commandOptions`, categories |
| `apps/web/lib/result-types.ts` | TypeScript interfaces for all command JSON outputs |
| `apps/web/lib/result-normalizers.ts` | Per-mode shape validators |

### Frontend — Renderers
| File | Purpose |
|------|---------|
| `apps/web/components/results/raw-renderer.tsx` | Fallback: scrollable `<pre>` + pretty JSON |
| `apps/web/components/results/table-renderer.tsx` | `sources`, `domains`, `map`, `status`, `retrieve`, `suggest` |
| `apps/web/components/results/cards-renderer.tsx` | `query`, `search` |
| `apps/web/components/results/report-renderer.tsx` | `ask`, `evaluate`, `research`, `debug`, `doctor` |
| `apps/web/components/results/status-renderer.tsx` | `stats`, `dedupe`, `embed` |
| `apps/web/components/results/job-lifecycle-renderer.tsx` | Async jobs: `extract`, `embed`, `github`, `reddit`, `youtube` |
| `apps/web/components/results-panel.tsx` | Renderer dispatch chain |

### Frontend — UI
| File | Purpose |
|------|---------|
| `apps/web/components/omnibox.tsx` | All modes grouped by category, flag serialization, job ops |
| `apps/web/components/command-options-panel.tsx` | Collapsible per-mode option controls |

## Behavior Changes (Before/After)

| Command | Before | After |
|---------|--------|-------|
| `doctor` | Blank output | JSON service grid with color-coded status |
| `query "test"` | Blank output | Score cards with URL, snippet, relevance |
| `sources` | Blank output | Sortable URL table with chunk counts |
| `stats` | Blank output | Collection stats dashboard |
| `ask "question"` | Blank output | Report with answer, diagnostics, timing |
| `search`, `research` | Blank output | Raw text streamed line-by-line |
| `github owner/repo` | Blank output | Job lifecycle: enqueue → poll → cancel |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --lib` | 0 errors | 0 errors | PASS |
| `pnpm lint` | ≤2 errors (pre-existing) | 2 errors (pre-existing) | PASS |
| `pnpm build` | Clean build | Clean build | PASS |
| Pre-commit hooks | All pass | All pass (monolith, rustfmt, clippy) | PASS |

## Risks and Rollback

- **Low risk:** All changes are additive — existing scrape/crawl rendering untouched
- **Rollback:** `git revert 9009d80..2922f53` reverts all 10 commits cleanly
- **Frontend-only for renderers:** Backend stdout streaming is the only Rust change; reverting just frontend has no backend impact

## Decisions Not Taken

- **Did not add result caching (D4):** Deferred — live snapshots (`doctor`, `stats`) shouldn't be cached, and file-based history adds complexity without clear UX benefit
- **Did not fix number input snap-back:** Minor UX issue with partial input in `CommandOptionsPanel` — not blocking
- **Did not add `enumValues` to `AxonOptionSpec`:** Current regex parsing of `notes` field works; explicit enum arrays would be cleaner but requires touching all option specs

## Open Questions

- Pre-existing lint errors in `crawl-file-explorer.tsx:273` and `results-panel.tsx:252` — not from this work but should be addressed
- `parseEnumValues` regex fragility — works for current patterns but may break with new note formats
- Checkbox double-fire risk with `<button>` inside `<label>` in `CommandOptionsPanel` — not observed but theoretically possible

## Next Steps

- Push branch and create PR against `main`
- Address remaining minor review findings (accessibility, optimization) in follow-up

## Commit Log

```
2922f53 fix(web): Phase D review fixes — hooks order, depth flag, exhaustive deps
e4e09ce feat(web): Phase D polish — helper selectors, recent run targets, command options panel
8fdbc2e fix(web): Phase C review fixes — cancel routing, PHASE_META, ingest removal
4f5411d feat(web): job lifecycle renderer + all modes grouped by category
639b208 fix(web): Phase B review fixes — renderIntent routing + deduplicate utils
1312841 feat(web): typed renderers + result normalizer pipeline (Phase B)
24446bf fix(web): extract shared CopyButton, cap stdout arrays, explicit switch cases
2ca4446 feat(web): add stdout streaming protocol types, hook state, and raw renderer
94a7951 refactor(web): split execute.rs into module directory + add poll timeout
9009d80 feat(web): stream stdout from sync commands + expand flag whitelist
```
