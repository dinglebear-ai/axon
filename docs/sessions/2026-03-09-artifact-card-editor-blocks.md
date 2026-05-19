# Session: Artifact Card for Editor Blocks in Chat

**Date:** 2026-03-09
**Branch:** `refactor/acp-performance-modern-rust`
**Working directory:** `/home/jmagar/workspace/axon_rust`

---

## Session Overview

Implemented a composable **Artifact card** component for the `/reboot` chat UI. When the Axon ACP agent emits `<axon:editor>` XML blocks in its response stream, those blocks previously either leaked as raw XML or were silently stripped. This session replaces that with a rich, clickable artifact card rendered inside the assistant's chat bubble — showing the document title, word count, a text preview, and copy/open-in-editor actions. Clicking the card loads the content into the Plate.js editor and opens the editor pane.

Color scheme was iterated twice based on user feedback: green → pink (secondary) → blue (primary), settling on the primary blue to create visual contrast against the pink assistant bubble.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | Read existing `axon-message-list.tsx`, `axon-shell.tsx`, `tool.tsx` to understand patterns |
| Phase 1 | Created `components/ai-elements/artifact.tsx` — composable Artifact component family |
| Phase 2 | Updated `axon-message-list.tsx`: replaced `stripEditorBlocks` with `parseEditorArtifacts` + `AssistantMessageBody` |
| Phase 3 | Updated `axon-shell.tsx`: passed `onEditorContent={onEditorUpdate}` to both desktop + mobile `AxonMessageList` |
| Phase 4 | Lint revealed 513-line monolith violation — split editor artifact logic into `axon-editor-artifact.tsx` |
| Phase 5 | Added `ArtifactContent` preview section with body text (6 lines desktop / 3 lines mobile) |
| Phase 6 | Color iteration: green → pink secondary → blue primary |
| Verify | Chrome DevTools live testing at `https://axon.tootie.tv/reboot` |

---

## Key Findings

- `axon-message-list.tsx` was ~376 lines before this session; grew to 513 with the artifact code → monolith violation → required split into `axon-editor-artifact.tsx`
- `onEditorUpdate` callback already existed in `axon-shell.tsx:190` with the exact right signature `(content: string, operation: 'replace' | 'append') => void` — no new shell logic needed
- The AI Elements reference at `https://elements.ai-sdk.dev/components/artifact` shows a header + content area pattern; our initial implementation only had the header strip
- Chrome DevTools `fill()` tool bypasses React's synthetic events — must use `nativeInputValueSetter` + `dispatchEvent('input')` to trigger React state updates
- The `/reboot` color scheme: primary `#87afff` (blue) for user messages/borders, secondary `#ff87af` (pink/rose) for Claude bubbles; green was completely off-brand
- Pink-on-pink (artifact inside pink bubble) had poor contrast — blue primary creates the correct visual hierarchy

---

## Technical Decisions

| Decision | Rationale |
|----------|-----------|
| Extract `EditorArtifactCard` + `AssistantMessageBody` into `axon-editor-artifact.tsx` | 513-line monolith violation; natural seam — all editor artifact logic lives together |
| Use `--axon-primary` (blue `#87afff`) for artifact card | Card sits inside pink assistant bubble; blue creates contrast, matches user bubble + tool card language |
| `line-clamp-6` / 600-char preview on desktop, `line-clamp-3` / 280-char on mobile | Desktop has space; mobile needs compact; matches `variant` prop already threading through the component |
| `AssistantMessageBody` sub-component instead of IIFE in render | Cleaner JSX, avoids Turbopack issues with inline logic, matches existing pattern (`ThinkingSection`, `ToolCallCard`) |
| Regex `EDITOR_BLOCK_RE` reused from original `stripEditorBlocks` | Identical match logic; extending to capture content + `op` attribute rather than discard |

---

## Files Modified

| File | Type | Purpose |
|------|------|---------|
| `apps/web/components/ai-elements/artifact.tsx` | **CREATED** | Composable Artifact component family: `Artifact`, `ArtifactHeader`, `ArtifactTitle`, `ArtifactDescription`, `ArtifactActions`, `ArtifactAction`, `ArtifactContent` |
| `apps/web/components/reboot/axon-editor-artifact.tsx` | **CREATED** | Editor artifact logic: `parseEditorArtifacts`, `EditorArtifactCard`, `AssistantMessageBody`, `extractTitle`, `extractPreview` |
| `apps/web/components/reboot/axon-message-list.tsx` | **MODIFIED** | Removed `stripEditorBlocks`; added `onEditorContent` prop; wired `AssistantMessageBody`; removed unused imports |
| `apps/web/components/reboot/axon-shell.tsx` | **MODIFIED** | Added `onEditorContent={onEditorUpdate}` to both desktop (line ~876) and mobile (line ~693) `AxonMessageList` instances |

---

## Commands Executed

```bash
# Lint check (run multiple times)
pnpm lint
# Result: 0 errors in our files; 6 pre-existing a11y warnings in Plate.js UI components

# Line count checks (monolith enforcement)
wc -l apps/web/components/reboot/axon-message-list.tsx apps/web/components/reboot/axon-editor-artifact.tsx
# After split: 366 + 165 = 531 total, both under 500

wc -l apps/web/components/ai-elements/artifact.tsx
# 74 lines

# Format
pnpm format
# Result: No fixes applied (Biome clean)
```

---

## Behavior Changes (Before/After)

| Scenario | Before | After |
|----------|--------|-------|
| Agent emits `<axon:editor op="replace">` block | Block stripped silently; nothing shown in chat | Artifact card rendered with title, word count, text preview |
| Click artifact card | N/A | Loads content into Plate.js editor + opens editor pane |
| Click pencil button | N/A | Same as clicking card |
| Click copy button | N/A | Copies raw markdown to clipboard; button shows ✓ for 2s |
| `op="append"` blocks | Stripped silently | Card shows "appended to editor"; appends to existing editor content |
| Desktop vs mobile | N/A | Desktop: 6-line / 600-char preview; mobile: 3-line / 280-char preview |
| No `<axon:editor>` blocks | Raw `stripEditorBlocks` pass-through | `parseEditorArtifacts` returns displayText unchanged, no cards rendered |

---

## Verification Evidence

| Verification | Expected | Actual | Status |
|---|---|---|---|
| `pnpm lint` | 0 errors | 0 errors (6 pre-existing warnings) | ✅ |
| `wc -l axon-message-list.tsx` | ≤500 lines | 366 lines | ✅ |
| `wc -l axon-editor-artifact.tsx` | ≤500 lines | 165 lines | ✅ |
| `wc -l artifact.tsx` | ≤500 lines | 74 lines | ✅ |
| Chrome: send prompt with `<axon:editor>` block | Artifact card renders | Card renders with title "The Mandalorian Creed", 264/321 words, preview text | ✅ |
| Chrome: click "Open in editor" button | Editor pane opens with content | Editor pane opened, content loaded (verified via screenshot) | ✅ |
| Chrome: console errors | 0 errors | 0 errors (1 pre-existing a11y form field info) | ✅ |
| Color scheme | Blue primary on pink bubble | Blue border/gradient/icon on pink assistant bubble | ✅ |

---

## Source IDs + Collections Touched

*(Axon embed to be completed at end of session — no prior embeds/retrieves this session)*

---

## Risks and Rollback

- **Monolith split risk**: `axon-editor-artifact.tsx` is a new file; if deleted, `axon-message-list.tsx` import breaks. Rollback: revert all 4 changed files via `git checkout`.
- **EDITOR_BLOCK_RE regex**: Same regex as before; if the ACP layer changes the XML tag format, both parse and the old strip logic would have failed identically — no regression risk.
- **`onEditorContent` prop**: Optional (`?`) on `AxonMessageList` — if shell doesn't pass it, cards render but clicks are no-ops. No crash risk.

---

## Decisions Not Taken

| Alternative | Rejected Because |
|-------------|-----------------|
| Keep everything in `axon-message-list.tsx` | 513 lines — hard monolith violation |
| Use an IIFE in render for `AssistantMessageBody` | Turbopack JSX parsing issues; sub-component is cleaner |
| Green accent color | Completely off-brand with `/reboot` color palette |
| Pink (secondary) accent color | Poor contrast — card sits inside pink assistant bubble |
| Show a full markdown renderer in the card | Overkill for a compact card; Plate.js is the renderer — card is a navigation affordance |
| Collapsible content like `Tool` | Artifact is a destination, not a details pane — always-visible preview better UX |

---

## Open Questions

- Should streaming `<axon:editor>` blocks show a "writing..." placeholder card while the block is incomplete, then swap to the final card? Currently, mid-stream raw XML may flash briefly.
- The `copy` action copies raw markdown — should it copy rendered HTML for rich paste targets?
- `ArtifactContent` uses `line-clamp` CSS — verify this truncates correctly on all browsers (Safari).

---

## Next Steps

- Consider a streaming-aware placeholder for in-flight `<axon:editor>` blocks during ACP streaming
- `ArtifactAction` tooltip uses `TooltipProvider` per action — could be hoisted if many actions are added
- The `variant` prop chain (`AxonMessageList` → `AssistantMessageBody` → `EditorArtifactCard`) is clean but verbose; could be a context if more components need it
