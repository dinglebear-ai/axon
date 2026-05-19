# Session: Next.js Plate.js Content Viewer + WS Decoupling

**Date:** 2026-02-24 13:30 EST
**Branch:** fix-crawl
**Duration:** ~3 hours (continued from prior session)

## Session Overview

Decoupled the omnibox and results panel components in the Next.js web UI, replaced `dangerouslySetInnerHTML` markdown rendering with a read-only Plate.js editor, and rewired the backend to send scraped markdown files over WebSocket instead of streaming JSON stdout. Also added themed scrollbars, a copy-to-clipboard button, and markdown normalization for scraper output artifacts.

## Timeline

1. **Context restoration** — Continued from prior session that built the Next.js dashboard (`apps/web/`). Plan existed at `drifting-booping-pike.md`.
2. **WS message store** — Created `use-ws-messages.ts` hook + `WsMessagesContext` to decouple omnibox from results panel.
3. **Plate.js content viewer** — Created `content-viewer.tsx` wrapping a read-only Plate editor using `markdownToPlateNodes()`.
4. **Component decoupling** — Rewrote `omnibox.tsx` (removed forwardRef/props), `providers.tsx` (added WsMessagesProvider), `page.tsx` (removed message routing state).
5. **Type fix** — `Descendant[]` vs `Value` mismatch in Plate's `usePlateEditor` required `as any` cast in both `content-viewer.tsx` and `plate-editor.tsx`.
6. **Backend architecture pivot** — User corrected approach: CLI saves clean markdown to `.cache/axon-rust/output/scrape-markdown/`. Rewrote `execute.rs` to drop `--json`, read output files after command completion, send `file_content` WS message.
7. **Debug logging** — Added `[web]` prefixed log messages to execute.rs to diagnose file_content delivery (initially files weren't rendering).
8. **Markdown normalization** — Scraper outputs `## [\n](#anchor)Text` for heading anchors. Added `normalizeMarkdown()` in `lib/markdown.ts` to fix these before Plate deserialization. Initial regex missed trailing space after `##` — fixed.
9. **Themed scrollbars** — Added CSS outside `@layer base` for specificity over Tailwind resets.
10. **Copy button** — Added inside `content-viewer.tsx` with proper clipboard SVG icon (Lucide-style stroke, not filled).

## Key Findings

- `execute.rs:62-66` — `output_dir()` uses relative path `.cache/axon-rust/output` which works because subprocess inherits server's cwd
- `content.rs:84` — `url_to_filename()` produces `{idx:04}-{sanitized_host_path}.md` pattern
- Plate v52 `usePlateEditor` value option expects `Value` (TElement[]) but `deserializeMd` returns `Descendant[]` — requires `as any` cast since Value isn't exported from platejs
- `remarkMdx` in the markdown deserializer config can be aggressive about parsing JSX-like content
- Scrollbar CSS inside `@layer base` gets overridden by Tailwind — must be unlayered for specificity

## Technical Decisions

- **File-based content over stdout parsing** — CLI already saves clean markdown to disk. Reading the output file is simpler, cleaner, and avoids JSON parsing/filtering. Backend reads newest `.md` file after command exits 0.
- **`as any` for Plate Value type** — No clean type-safe cast available. `Value` isn't exported from platejs v52. `deserializeMd` only produces element nodes in practice, so the cast is safe.
- **Normalize markdown pre-Plate** — Fix scraper artifacts (multiline heading anchors) before deserialization rather than post-processing Plate nodes. Regex is simpler and more maintainable.
- **Copy button inside ContentViewer** — Positioned absolutely top-right with frosted glass background. Copies raw markdown, not rendered text.
- **Debug log lines kept in execute.rs** — `[web]` prefixed lines visible in Stats tab help diagnose file delivery issues. Can be removed later.

## Files Modified

| File | Action | Purpose |
|------|--------|---------|
| `crates/web/execute.rs` | Modified | Drop --json, add file_content WS message, debug logging |
| `apps/web/lib/ws-protocol.ts` | Modified | Added `file_content` message type |
| `apps/web/hooks/use-ws-messages.ts` | Created | Shared WS message store with context |
| `apps/web/components/content-viewer.tsx` | Rewritten | Read-only Plate editor + copy button |
| `apps/web/components/results-panel.tsx` | Modified | Log lines in Stats tab, removed copy button |
| `apps/web/components/omnibox.tsx` | Modified | Decoupled from props, uses context |
| `apps/web/app/providers.tsx` | Modified | Added WsMessagesProvider |
| `apps/web/app/page.tsx` | Modified | Removed message routing state |
| `apps/web/components/editor/plate-editor.tsx` | Modified | `as any` cast for Value type |
| `apps/web/lib/markdown.ts` | Modified | Added `normalizeMarkdown()` for scraper artifacts |
| `apps/web/app/globals.css` | Modified | Themed scrollbars outside @layer |

## Commands Executed

| Command | Result |
|---------|--------|
| `cargo check` | 0 errors (execute.rs changes) |
| `pnpm build` (apps/web) | 3 static pages, 0 errors |
| `ls -lt .cache/axon-rust/output/scrape-markdown/` | Confirmed output files exist |

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| Content tab | `dangerouslySetInnerHTML` with hand-rolled markdown parser | Read-only Plate.js editor via `@platejs/markdown` deserializer |
| Data source | Streaming JSON stdout from subprocess | Reading output `.md` file from disk after command exits |
| Log lines | Mixed into Content tab | Isolated in Stats tab with LogViewer component |
| Component coupling | Omnibox/ResultsPanel connected via page.tsx props | Independent via WsMessagesContext |
| Scrollbars | Default browser chrome | Thin 6px translucent themed scrollbar |
| Copy | Not available | Copy button in content viewer copies raw markdown |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | 0 errors | `Finished dev profile` | PASS |
| `pnpm build` | Compiles | 3 static pages generated | PASS |
| Manual: scrape platejs.org | Content renders in Plate | Content renders with headings, lists, links | PASS |
| Manual: Stats tab | Shows log lines | Shows `[web]` debug lines + CLI progress | PASS |
| Manual: Copy button | Copies markdown | Copies raw markdown to clipboard | PASS |

## Source IDs + Collections Touched

None — no Axon embed/retrieve operations in this session.

## Risks and Rollback

- **`as any` casts** — Two locations (`content-viewer.tsx:53`, `plate-editor.tsx:30`). If Plate releases a breaking change to Value type, these will silently break. Monitor on Plate upgrades.
- **Relative output path** — `execute.rs:65` uses `.cache/axon-rust/output`. If server cwd differs from subprocess cwd, file lookup fails. Mitigated: subprocess inherits server cwd.
- **Debug logging in production** — `[web]` log lines in execute.rs add noise to Stats tab. Should be gated behind a flag or removed before release.
- **Rollback:** `git checkout fix-crawl~1 -- crates/web/execute.rs apps/web/`

## Decisions Not Taken

- **Stream stdout for content** — Rejected by user. CLI saves clean markdown files; parsing stdout is unnecessary complexity.
- **`--json` flag on subprocess** — Removed. JSON output was being mixed into content rendering. File-based approach is cleaner.
- **remarkMdx removal** — Considered removing from markdown deserializer config to prevent potential content eating. Kept because no issues observed with current scraper output.
- **Value type assertion via Parameters<>** — Over-engineered TypeScript gymnastics to avoid `as any`. Rejected for simplicity.

## Open Questions

- Should `[web]` debug log lines in `execute.rs` be removed or gated behind `cfg(debug_assertions)`?
- Does `remarkMdx` in the markdown deserializer cause issues with scraped content containing JSX-like patterns?
- Should `normalizeMarkdown()` handle additional scraper artifacts beyond heading anchors?
- The `output` message type in `ws-protocol.ts` is still defined but no longer sent — should it be removed?

## Next Steps

- Remove debug `[web]` log lines from `execute.rs` or gate behind debug builds
- Test with more scrape targets to validate markdown normalization coverage
- Consider adding support for non-file-producing modes (query, ask, etc.) to render output in ContentViewer
- Clean up unused `output` message type from ws-protocol.ts
- `/quick-push` the changes
