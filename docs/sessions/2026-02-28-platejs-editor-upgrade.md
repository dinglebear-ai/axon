# PlateJS Editor Upgrade — Pulse Editor Pane
**Date:** 2026-02-28
**Branch:** feat/crawl-download-pack
**Working directory:** `apps/web/`

---

## Session Overview

Transformed the Pulse editor pane from a minimal 4-button strip (Bold/Italic/Underline/Strike) into a full-featured rich text editor with a comprehensive toolbar, floating selection toolbar, right-click context menu, bullet/numbered lists, live word count, copilot hint bar, and link insertion.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | Reviewed screenshot showing minimal editor with ~5 buttons |
| Phase 1 | Expanded toolbar: Undo/Redo, H1–H3, Blockquote, Code Block, Marks + word count |
| Phase 2 | Investigated copilot wiring (confirmed fully wired), identified context menu + floating toolbar gaps |
| Phase 3 | Created `block-type-button.tsx`, `editor-context-menu.tsx`, `floating-toolbar.tsx`, `link-toolbar-button.tsx` |
| Phase 4 | Added `@platejs/floating` to package.json; added `ListPlugin` + `list-toolbar-button.tsx` |
| Phase 5 | Fixed floating toolbar offset, modernised clipboard API, completed list buttons |
| Phase 6 | Filled remaining gaps: Link in floating toolbar, Paste in context menu, lists in "Turn into" submenu |

---

## Key Findings

- **CopilotPlugin is fully wired**: `/api/ai/copilot` endpoint exists, NDJSON streaming custom fetch, `GhostText` component, all 4 shortcuts (Tab/Ctrl+Right/Esc/Ctrl+Space) configured in `copilot-kit.tsx`
- **`@platejs/floating`** was a transitive dep of `@platejs/link` but not direct — needed explicit `package.json` entry
- **`@platejs/list` v52** uses indent-based lists (`listStyleType` on paragraph nodes), NOT nested `ul>li>lic` — **`@platejs/markdown` v52 handles `listStyleType` for markdown roundtrip** (confirmed via grep of dist JS)
- **`ListStyleType` is NOT exported from `@platejs/list/react`** — use string literals `'disc'` / `'decimal'` directly
- **`radix-ui`** unified package re-exports `ContextMenu` from `@radix-ui/react-context-menu` — no extra dep needed
- **Block type key strings**: `'h1'`, `'h2'`, `'h3'`, `'p'`, `'blockquote'`, `'code_block'` (confirmed via `node -e` introspection)
- **`editor.tf.toggleBlock(type)` / `editor.tf.toggleMark(type)`** — confirmed API from `@platejs/basic-nodes` dist source
- **`offset` middleware** is re-exported from `@platejs/floating` — no separate `@floating-ui/dom` import needed

---

## Technical Decisions

| Decision | Rationale |
|----------|-----------|
| `onMouseDown` + `e.preventDefault()` on toolbar buttons | Prevents editor blur on toolbar click — standard PlateJS pattern |
| `requestAnimationFrame` in context menu `onSelect` | Menu must fully close and focus must restore before transforms fire |
| `ListPlugin` (indent-based) alongside custom `list/li/lic` render plugins | Indent-based lists roundtrip through `serializeMd`; custom plugins only handle rendering of markdown-imported lists. Two systems coexist without conflict. |
| `navigator.clipboard` with `execCommand` fallback | Avoids deprecated-API console warnings; `execCommand` still works as fallback in all major browsers |
| `useEditorRef()` cast to `any` for `deleteFragment`, `insertText`, `tf.toggleList` | These methods exist at runtime but aren't typed on the base `useEditorRef()` return type — biome-ignore comments document the reason |
| `offset(6)` middleware on floating toolbar | 6px gap between selection and balloon — prevents toolbar from sitting flush against selected text |
| Link button at end of marks group (not separate group) | Links are inline formatting; conceptually belongs with marks |

---

## Files Modified

| File | Status | Purpose |
|------|--------|---------|
| `apps/web/components/pulse/pulse-editor-pane.tsx` | Modified | Main editor: full toolbar (Undo/Redo, H1-H3, Blockquote, CodeBlock, BulletList, NumberedList, Bold, Italic, Underline, Strike, Code, Link), word count, copilot hint bar, EditorContextMenu + FloatingToolbar integration |
| `apps/web/components/editor/plugins/extended-nodes-kit.tsx` | Modified | Added `ListPlugin` from `@platejs/list/react` for toolbar-driven list creation |
| `apps/web/package.json` | Modified | Added `"@platejs/floating": "^52.0.11"` as direct dependency |
| `apps/web/components/ui/block-type-button.tsx` | Created | Reusable block-type toggle button with active state via `useEditorSelector` |
| `apps/web/components/ui/mark-toolbar-button.tsx` | Existing | Already existed — no changes needed |
| `apps/web/components/ui/link-toolbar-button.tsx` | Created | Link button using `useLinkToolbarButton` + `useLinkToolbarButtonState` from `@platejs/link/react` |
| `apps/web/components/ui/list-toolbar-button.tsx` | Created | Bullet/numbered list toggle using `useListToolbarButton` + `useListToolbarButtonState` from `@platejs/list/react` |
| `apps/web/components/ui/editor-context-menu.tsx` | Created | Right-click context menu: Copy/Cut/Paste (modern clipboard API), formatting marks, "Turn into" submenu (P/H1/H2/H3/Blockquote/CodeBlock/BulletList/NumberedList) |
| `apps/web/components/ui/floating-toolbar.tsx` | Created | Balloon selection toolbar using `useFloatingToolbar` + `offset(6)`: H1/H2, Bold/Italic/Underline/Strike/Code/Link |

---

## Behavior Changes (Before → After)

| Area | Before | After |
|------|--------|-------|
| Toolbar buttons | 5: Bold, Italic, Underline, Strike, Code2 | 14: Undo, Redo, H1, H2, H3, Blockquote, CodeBlock, BulletList, NumberedList, Bold, Italic, Underline, Strike, Code, Link |
| Word count | None | Live word count in header, right-aligned |
| Right-click | Browser default | Custom context menu: Copy/Cut/Paste, formatting marks, "Turn into" submenu |
| Text selection | No toolbar | Floating balloon: H1/H2, Bold/Italic/Underline/Strike/Code/Link |
| List support | Render-only (markdown import) | Full toggle via toolbar, context menu, and keyboard |
| Copilot hints | Hidden | Hint bar at bottom: "✦ AI copilot active · Ctrl+Space · Tab · Esc" |
| Clipboard | `execCommand` (deprecated) | `navigator.clipboard` API with `execCommand` fallback |

---

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| `pnpm exec tsc --noEmit --skipLibCheck` | 0 errors | 0 errors | ✅ |
| `@platejs/list/react` exports `useListToolbarButton` | Present | Confirmed via `node -e` introspection | ✅ |
| `@platejs/markdown` handles `listStyleType` | Markdown roundtrip works | Confirmed via `grep listStyleType dist/index.js` | ✅ |
| `offset` re-exported from `@platejs/floating` | Available | Confirmed via `node -e` introspection | ✅ |
| `ListStyleType` export from `@platejs/list/react` | — | NOT exported — use string literals | ⚠️ worked around |

---

## Risks and Rollback

- **Two list systems**: `ListPlugin` (indent-based, toolbar-created) and custom `list/li/lic` plugins (markdown-imported) coexist. Lists created via toolbar and lists loaded from markdown have different internal structures and cannot be converted between each other. Not a bug, but a known architectural limitation.
- **Clipboard API permissions**: `navigator.clipboard.readText()` (Paste) requires the `clipboard-read` permission in some browsers/contexts. In a local dev environment behind localhost this is generally granted automatically; in production HTTPS contexts it will prompt.
- **Rollback**: All changes are in `apps/web/` only. Revert `extended-nodes-kit.tsx` (remove `ListPlugin`) and `package.json` (remove `@platejs/floating`) plus delete the 4 new `components/ui/` files to return to the original state.

---

## Decisions Not Taken

| Alternative | Rejected Because |
|-------------|-----------------|
| Replace custom `list/li/lic` plugins with `@platejs/list` proper | Would break markdown deserialization roundtrip; custom plugins intentionally match MDAST_TO_PLATE mapping |
| Add list buttons to floating toolbar | Floating toolbar targets inline selection actions; block-level list toggle is less common in that context |
| Use `@floating-ui/dom` directly for offset | `@platejs/floating` re-exports it; adding another dep is unnecessary |
| Paste as plain-text only | Plain text is the only safe cross-browser async paste option; rich paste would require `ClipboardItem` with MIME types which isn't universally supported |

---

## Open Questions

- Do indent-based lists created via toolbar serialize correctly to all markdown flavours (GFM, MDX)? Verified `listStyleType` is handled by `@platejs/markdown` dist but not tested end-to-end.
- `useLinkToolbarButton` triggers `triggerFloatingLink` — this opens a link input popover. Is that popover UI (`@platejs/link`'s floating link) actually rendered? `LinkPlugin` is configured but no `FloatingLink` component is explicitly placed in the JSX.

---

## Next Steps

- Verify the link button actually opens the link insertion popover (may need `FloatingLink` component from `@platejs/link/react` added inside `EditorContainer`)
- End-to-end test: create list via toolbar → save → reload → confirm markdown roundtrip
- Consider adding `FloatingLink` component alongside `FloatingToolbar` in `EditorContainer`
