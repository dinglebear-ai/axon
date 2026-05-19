# Omnibox Upgrade Plan — "Actual Omnibox"

Date: 2026-02-25
Owner: web UI
Scope: `apps/web` (primary), `crates/web.rs` (supporting APIs)

## Goal
Upgrade the Next.js omnibox so it behaves like a real keyboard-first command surface:
- Global shortcut to focus/toggle omnibox
- Keyboard shortcuts to switch modes quickly
- Alt/Tab-style mode switcher
- `@mention` support for files and modes (e.g. `@crawl`)
- Fuzzy local-doc filename search for mentions
- Smart submit behavior: in `crawl` mode without URL, fall back to vector search with progressive collection fan-out

## Current State (from code audit)
- Omnibox target is `apps/web/components/omnibox.tsx`.
- Mode registry is `apps/web/lib/ws-protocol.ts` and command specs in `apps/web/lib/axon-command-map.ts`.
- Command execution is WS-based (`apps/web/hooks/use-axon-ws.ts` + Rust `crates/web/execute/mod.rs`).
- No API in `apps/web/app/api/*` yet; backend support is from Axum routes in `crates/web.rs`.
- `apps/web/components/omnibox.tsx` currently references `optionsOpen`, `setOptionsOpen`, and `activeOptionCount` without state declarations; fix this first to stabilize baseline.

## Product Decisions
1. File mentions use fuzzy filename/path search first, not semantic search.
- Why: fast, deterministic, and predictable for "open file" / "attach file" intent.
- Semantic search remains for query/ask/research execution paths, not for mention tokenization.

2. Mention grammar
- `@<mode>` selects mode (e.g. `@crawl`, `@query`, `@ask`, `@research`)
- `@/<path-or-name>` targets local docs/files (e.g. `@/docs/plans/pulse-session.md`)
- Multiple file mentions allowed; they become an explicit context set for ask/query/research.

3. Crawl smart fallback
- If active mode is `crawl` and input is not URL-like, route to vector query flow.
- Query collections in order: default collection -> `github` -> `sessions` -> `reddit` -> `youtube`.
- Stop early when threshold matches found; continue fan-out when no/low relevance.

## Implementation Plan

### Phase 0 — Stabilize Omnibox Baseline
Files:
- `apps/web/components/omnibox.tsx`

Tasks:
1. Add missing `optionsOpen` / `setOptionsOpen` and `activeOptionCount` state/derived values.
2. Keep existing behavior unchanged after compile fix.

Verification:
- `pnpm --dir apps/web lint`
- `pnpm --dir apps/web build`

### Phase 1 — Keyboard Foundation (Focus + Mode Hotkeys)
Files:
- `apps/web/components/omnibox.tsx`
- `apps/web/lib/ws-protocol.ts`
- `apps/web/app/page.tsx` (if app-level key listener needed)

Tasks:
1. Add global focus/toggle shortcut (recommended: `Ctrl+K` / `Cmd+K`).
2. Add mode cycle shortcuts:
- Next mode: `Alt+.`
- Prev mode: `Alt+,`
3. Add direct mode hotkeys for core modes (example):
- `Alt+1` scrape, `Alt+2` crawl, `Alt+3` query, `Alt+4` ask, `Alt+5` research
4. Ensure shortcuts are ignored while typing in editable contexts, except toggle shortcut.

Verification:
- Unit tests for key handling (Vitest)
- Manual check with active WS connection

### Phase 2 — Alt/Tab Mode Switcher Overlay
Files:
- `apps/web/components/omnibox.tsx`
- `apps/web/components/omnibox-mode-switcher.tsx` (new)
- `apps/web/app/globals.css` (or component-local styling)

Tasks:
1. Implement temporary switcher shown while `Alt` held and `Tab` pressed.
2. Preview target mode in overlay; commit selection on key release.
3. Support reverse cycle with `Shift+Tab`.
4. Keep existing dropdown as secondary mouse path.

Verification:
- Unit tests for overlay state machine
- Manual keyboard-only flow

### Phase 3 — Mention Parser + Suggestion Engine
Files:
- `apps/web/lib/omnibox-mentions.ts` (new)
- `apps/web/components/omnibox.tsx`
- `apps/web/components/omnibox-suggestions.tsx` (new)

Tasks:
1. Implement tokenizer for mention candidates at cursor.
2. Suggestion providers:
- Mode provider from `MODES`
- File provider from local-doc index API (Phase 4)
3. Keyboard navigation in suggestions (`ArrowUp/Down`, `Enter`, `Tab`, `Esc`).
4. Mention chips in input model (render plain text now; chip rendering can be follow-up).

Verification:
- Parser tests with edge cases (multiple mentions, mixed text)
- Selection behavior tests

### Phase 4 — Local Docs Fuzzy Index API (Rust Backend)
Files:
- `crates/web.rs`
- `crates/web/local_docs.rs` (new)

Tasks:
1. Add GET endpoint: `/api/local-docs/search?q=<query>&limit=<n>`.
2. Scope search to local doc roots:
- `docs/`
- optionally `.cache/axon-rust/output/markdown` for crawl artifacts
3. Implement fuzzy matcher (filename + relative path; ranked by match quality).
4. Return safe relative paths + absolute canonical path guard (no traversal).

Verification:
- Rust unit tests for path safety and ranking
- Endpoint smoke test via browser/CLI

### Phase 5 — Mention Actions and Execution Routing
Files:
- `apps/web/components/omnibox.tsx`
- `apps/web/hooks/use-ws-messages.ts`
- `apps/web/components/results-panel.tsx`

Tasks:
1. `@mode` mention sets active mode instantly and removes token.
2. `@/file` mention behavior:
- In editor/workspace mode: open file in PlateJS editor state.
- In ask/query/research: include file contents as explicit context payload.
3. Add pre-submit resolver to expand file mentions into context blobs.
4. Add max context guardrails (size cap + truncation with notice).

Verification:
- UI tests for mention-select-submit
- Manual test opening docs file and ask/query with attached context

### Phase 6 — Smart Crawl Fallback + Collection Fan-Out
Files:
- `apps/web/components/omnibox.tsx`
- `apps/web/lib/omnibox-router.ts` (new)
- `crates/web/execute/mod.rs` (optional if routing server-side)

Tasks:
1. Add URL detector (`http(s)://`, domain-like input).
2. If mode `crawl` and input is non-URL:
- Route to semantic query flow.
- Start with default collection.
- Fan out to `github`, `sessions`, `reddit`, `youtube` if no useful hits.
3. Emit user-visible routing status in omnibox status text/log pane.

Verification:
- Router tests for URL vs non-URL cases
- Integration test with mocked WS responses

### Phase 7 — Editor Integration for File Mentions
Files:
- `apps/web/components/editor/plate-editor.tsx`
- `apps/web/app/editor/page.tsx`
- `apps/web/hooks/use-ws-messages.ts` (or new editor-context provider)

Tasks:
1. Add "open document by path" action exposed to omnibox layer.
2. Parse markdown file to PlateJS value on open.
3. Preserve unsaved-buffer warning when switching documents.

Verification:
- Component tests for open/replace document flow
- Manual open/edit/switch behavior

## Testing Strategy

Add test files:
- `apps/web/__tests__/omnibox-shortcuts.test.ts`
- `apps/web/__tests__/omnibox-switcher.test.ts`
- `apps/web/__tests__/omnibox-mentions.test.ts`
- `apps/web/__tests__/omnibox-router.test.ts`
- `crates/web/local_docs.rs` unit tests

Gate commands:
- `pnpm --dir apps/web test`
- `pnpm --dir apps/web lint`
- `pnpm --dir apps/web build`
- `cargo test -p axon -- crates::web`
- `cargo build --bin axon`

## Rollout Order
1. Phase 0 (compile-safe baseline)
2. Phase 1 (shortcuts)
3. Phase 3 + 4 (mentions + local-doc fuzzy search)
4. Phase 5 (mention actions)
5. Phase 6 (crawl smart fallback)
6. Phase 2 (alt/tab switcher polish)
7. Phase 7 (editor open integration)

## Open Decisions (need your pick before implementation)
1. Toggle shortcut preference:
- `Ctrl/Cmd+K` (recommended)
- `/` when not focused in inputs

2. Alt-tab behavior:
- Classic hold-to-cycle overlay (recommended)
- Tap-to-open palette then arrows/enter

3. File mention source set:
- `docs/**` only (recommended first)
- `docs/**` + crawl markdown cache + sessions exports
