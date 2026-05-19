# PlateJS Plugin Audit + Find-Replace & Diff Integration

**Date:** 2026-03-09
**Branch:** refactor/acp-performance-modern-rust

---

## Session Overview

Conducted a full audit of PlateJS plugins installed in `apps/web`, answered questions about the code editor capabilities, and added `@platejs/find-replace` and `@platejs/diff` support. Discovered that alignment, horizontal-rule, and font were all already set up — only two packages needed actual installation and wiring.

---

## Timeline

1. **PlateJS code editor question** — Explained that `@platejs/code-block` is a styled syntax-highlighted block, not a full code editor (no Monaco/LSP). For full code editing, a void node wrapping Monaco would be needed.
2. **Confirmed code block is set up** — `code-block-base-kit.tsx`, `code-block-node.tsx`, `code-block-node-static.tsx` all present.
3. **Plugin gap analysis** — Surveyed all 27 installed `@platejs/*` packages against the full ecosystem. Identified find-replace, diff, align, font, horizontal-rule as notable gaps.
4. **User requested 4 plugins** — find-replace, diff, horizontal-rule, font.
5. **Pre-installation audit** — Checked existing files before touching anything. Found:
   - `align-kit.tsx` + `align-base-kit.tsx` — already fully wired via `@platejs/basic-styles`
   - `HorizontalRulePlugin` — already in `basic-blocks-kit.tsx:87` from `@platejs/basic-nodes`
   - `font-base-kit.tsx` — already has `BaseFontColorPlugin`, `BaseFontBackgroundColorPlugin`, `BaseFontSizePlugin`, `BaseFontFamilyPlugin` from `@platejs/basic-styles`
   - `font-color-toolbar-button.tsx` — custom UI component already present
6. **Installed packages** — `pnpm add @platejs/find-replace @platejs/diff`
7. **Inspected actual exports** — Critically, `@platejs/diff` exports no `DiffPlugin` — it's a pure utility (`computeDiff()`). Built a thin wrapper plugin manually.
8. **Created all files** — 8 new files, 3 files updated.
9. **Type-checked** — `tsc --noEmit` clean.
10. **Explained diff architecture** — Walked through how `computeDiff` works and how the plugin renders its output.

---

## Key Findings

- `@platejs/find-replace` exports only `FindReplacePlugin` (a `SlatePlugin` with key `search_highlight`), `FindReplaceConfig`, and `decorateFindReplace`. No `/react` subpath, no `Base` prefix.
- `@platejs/diff` is a **pure utility library** — exports `computeDiff()` and type helpers only. No `DiffPlugin`. The diff document output has `{ diff: true, diffOperation: { type: 'insert' | 'delete' | 'update' } }` on text nodes.
- Alignment was in `@platejs/basic-styles` as `TextAlignPlugin` — already wired in `align-kit.tsx:3`.
- Font was in `@platejs/basic-styles` as `BaseFontColorPlugin` etc — already wired in `font-base-kit.tsx`.
- Horizontal rule was in `@platejs/basic-nodes` as `HorizontalRulePlugin` — already wired in `basic-blocks-kit.tsx:87`.
- `FindReplacePlugin` key is `'search_highlight'` — Plate uses this key to trigger the `SearchHighlightLeaf` when `leaf.search_highlight === true`.
- To trigger a search: `editor.setOption(FindReplacePlugin, 'search', searchText)`.

---

## Technical Decisions

- **Thin wrapper for diff** — Since `@platejs/diff` has no plugin, created `DiffTextPlugin = createPlatePlugin({ key: 'diff', node: { isLeaf: true } }).withComponent(DiffLeaf)`. This registers a leaf renderer that fires when `leaf.diff === true`, which is the exact property `computeDiff()` sets.
- **Re-exported `computeDiff` from diff-kit** — Makes usage ergonomic: `import { computeDiff, DiffKit } from '@/components/editor/plugins/diff-kit'` covers both the computation and the rendering registration.
- **Both base and react variants for find-replace** — `BaseFindReplaceKit` uses `SearchHighlightLeafStatic` (SlateLeaf from `platejs/static`) for SSR; `FindReplaceKit` uses `SearchHighlightLeaf` (PlateLeaf from `platejs/react`) for the interactive editor.
- **`as="mark"` for search highlight** — Semantically correct HTML element for search highlights; matches browser native find behavior.
- **Diff plugin included in main CopilotKit** — No impact on normal editing (nothing produces `diff: true` during normal use). Having it registered means a diff document can be loaded into the main editor if needed.
- **3rd party plugin question** — Answered that the ecosystem is almost entirely first-party. Major reasons: complex plugin API contract and frequent major rewrites. The `DiffTextPlugin` wrapper is exactly the standard pattern for missing plugins.

---

## Files Modified

### Created
| File | Purpose |
|------|---------|
| `apps/web/components/ui/search-highlight-node.tsx` | Interactive leaf for `search_highlight` decoration (yellow `<mark>`) |
| `apps/web/components/ui/search-highlight-node-static.tsx` | Static/SSR version of search highlight leaf |
| `apps/web/components/ui/diff-node.tsx` | Interactive leaf for diff marks (green/red/yellow by op type) |
| `apps/web/components/ui/diff-node-static.tsx` | Static/SSR version of diff leaf |
| `apps/web/components/editor/plugins/find-replace-kit.tsx` | React editor kit wrapping `FindReplacePlugin` |
| `apps/web/components/editor/plugins/find-replace-base-kit.tsx` | Base (static) kit wrapping `FindReplacePlugin` with static leaf |
| `apps/web/components/editor/plugins/diff-kit.tsx` | Thin wrapper plugin + re-exports `computeDiff` |
| `apps/web/components/editor/plugins/diff-base-kit.tsx` | Base (static) diff kit with `createSlatePlugin` |

### Modified
| File | Change |
|------|--------|
| `apps/web/components/editor/editor-base-kit.tsx` | Added `BaseFindReplaceKit` and `BaseDiffKit` to `BaseEditorKit` |
| `apps/web/components/editor/plugins/copilot-kit.tsx` | Added `FindReplaceKit` and `DiffKit` to `CopilotKit` |
| `apps/web/next.config.ts` | Added `'@platejs/find-replace'` and `'@platejs/diff'` to `transpilePackages` |
| `apps/web/package.json` | Added `@platejs/find-replace` and `@platejs/diff` as dependencies |

---

## Commands Executed

```bash
# Install packages
cd apps/web && pnpm add @platejs/find-replace @platejs/diff
# → Done in 4s

# Verify installed
ls node_modules/@platejs/find-replace/  # README.md dist package.json
ls node_modules/@platejs/diff/          # README.md dist package.json

# Check actual exports (critical step before writing code)
grep 'export' node_modules/@platejs/find-replace/dist/index.d.ts
# → export { FindReplaceConfig, FindReplacePlugin, decorateFindReplace }
grep 'export' node_modules/@platejs/diff/dist/index.d.ts
# → export { ComputeDiffOptions, DiffDeletion, DiffInsertion, DiffOperation, DiffProps, DiffUpdate, computeDiff, ... }
# NOTE: No DiffPlugin — pure utility library

# Type check
npx tsc --noEmit
# → clean (no output)
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| Find-replace | No `search_highlight` leaf registered — decorator would produce unrendered marks | `FindReplaceKit` + `SearchHighlightLeaf` registered; calling `editor.setOption(FindReplacePlugin, 'search', text)` now highlights matches with yellow `<mark>` elements |
| Diff rendering | No `diff` leaf registered — `computeDiff()` output rendered as plain text | `DiffKit` registered; diff documents show green (insert) / red strikethrough (delete) / yellow (update) marks |
| Static/SSR | Same gaps | `BaseFindReplaceKit` + `BaseDiffKit` registered in `BaseEditorKit` |
| `next.config.ts` transpile | Missing both packages | Both added to `transpilePackages` — no standalone build failures |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `pnpm add @platejs/find-replace @platejs/diff` | Both installed | Done in 4s | ✅ |
| `ls node_modules/@platejs/find-replace/` | dist/index.d.ts present | Present | ✅ |
| `grep 'export' find-replace/dist/index.d.ts` | FindReplacePlugin exported | Confirmed | ✅ |
| `grep 'export' diff/dist/index.d.ts` | computeDiff exported, no DiffPlugin | Confirmed | ✅ |
| `npx tsc --noEmit` | No type errors | Clean output | ✅ |

---

## Risks and Rollback

- **Low risk** — all changes are additive. No existing plugins modified. Normal editing unaffected.
- **Rollback**: `git restore apps/web/package.json apps/web/next.config.ts apps/web/components/editor/editor-base-kit.tsx apps/web/components/editor/plugins/copilot-kit.tsx && git clean -f apps/web/components/ui/search-highlight-node*.tsx apps/web/components/ui/diff-node*.tsx apps/web/components/editor/plugins/find-replace-*.tsx apps/web/components/editor/plugins/diff-*.tsx`

---

## Decisions Not Taken

- **`@platejs/font` installation** — Not needed; font color/size/family/background are all already in `@platejs/basic-styles` and wired in `font-base-kit.tsx`.
- **`@platejs/horizontal-rule` installation** — Not needed; `HorizontalRulePlugin` from `@platejs/basic-nodes` already registered in `basic-blocks-kit.tsx`.
- **`@platejs/align` installation** — Not needed; `TextAlignPlugin` from `@platejs/basic-styles` already wired in `align-kit.tsx`.
- **Importing from `@platejs/find-replace/react`** — No such subpath exists; package has a single entry point.
- **Using `DiffPlugin` from `@platejs/diff`** — Does not exist. `@platejs/diff` is purely a utility; a thin wrapper plugin was created instead.

---

## Open Questions

- The find-replace plugin has no built-in UI (search bar, replace input, keyboard shortcut). The `search_highlight` leaf is wired but a toolbar/panel component needs to be built to actually trigger `editor.setOption(FindReplacePlugin, 'search', text)`.
- `DiffKit` being in the main `CopilotKit` means any document with `diff: true` nodes will render with diff styling — this is intentional but could be surprising if diff documents are ever loaded into the interactive editor unintentionally.
- `BaseDiffKit` uses `createSlatePlugin` from `platejs` — should verify this resolves correctly for SSR (no `/react` import needed, but the import path should be checked against other base kits in the project).

---

## Next Steps

- Build a find-replace toolbar panel component that calls `editor.setOption(FindReplacePlugin, 'search', text)` and optionally `editor.tf.replaceText(...)`.
- Create a `DiffViewer` component that takes `(before: Descendant[], after: Descendant[])` props, calls `computeDiff`, and renders the result in a read-only Plate editor with `DiffKit`.
- Wire find-replace keyboard shortcut (`Mod+F`) as a toolbar/panel toggle.
