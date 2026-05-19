# Session: Editor Tabs + Cmd+K Results Pipeline

**Date:** 2026-03-04
**Branch:** feat/services-layer-refactor

---

## Session Overview

Redesigned the Axon web UI results flow end-to-end. The `ResultsPanel` on the home page was removed for inline-result modes (ask, research, query, retrieve). Instead, all results for these modes are now delivered as tabs in the Plate.js editor at `/editor`. Three trigger paths were unified: Cmd+K inline, Cmd+K background (minimize), and omnibox — all converge on the same pending-tab mechanism.

---

## Timeline

1. **Investigation** — User screenshot showed a "see results panel" toast on `/jobs` page. Investigated where the results panel lived → `apps/web/app/page.tsx` (home page only).
2. **Design decision** — User: "we shouldn't have the results pane on the home page whatsoever — it should be auto loaded in our platejs editor." Tabs system proposed.
3. **Cmd+K decoupling** — Identified the "tricky part": separating dismiss from cancel so background operations continue after UI hides. Built `pending-tab.ts` + subscription survival pattern.
4. **Cmd+K inline rendering** — Added `CmdKOutput` inline result view with `PulseMarkdown` for ask/research/query/retrieve modes.
5. **Tabs system** — Built `use-tabs.ts` + `editor-tab-bar.tsx` + refactored `app/editor/page.tsx`.
6. **Omnibox wiring** — Added JSON capture + navigation logic to `app/page.tsx` for inline modes triggered from the home page omnibox.

---

## Key Findings

- `ResultsPanel` was only on `app/page.tsx:120,167` — not on `/jobs` page at all. The "see results panel" toast was misleading when triggered from `/jobs`.
- `command.output.json` carries payload at `msg.data.data` (nested — not `msg.data`). Confirmed from `ws-protocol.ts`.
- Subscription in `CmdKPalette.tsx` is local (not global context), so it can be kept alive after UI hides by simply not calling `unsubRef.current?.()` in `minimizeToBackground`.
- `PulseMarkdown` component already existed and handles rich markdown rendering — no need for heavy Plate.js in modal.
- `key={activeTabId}` on `PulseEditorPane` forces a clean Plate instance per tab (required — Plate.js has internal document state).
- `WsMessagesContextValue.currentMode` is exposed as state, `currentInput` is not. Worked around by using a local `currentModeRef` in `page.tsx`.

---

## Technical Decisions

**Subscription survival (Cmd+K background):**
`minimizeToBackground` sets `phase = 'idle'` (hides UI) without calling `unsubRef.current?.()`. The component stays mounted (returns `null`), refs/subscription survive. Alternatives: global store (too heavy), re-subscribing on reopen (loses buffered messages).

**`capturedJsonRef` pattern:**
State (`capturedJson`) for rendering, ref (`capturedJsonRef`) for use in async `command.done` callback. Necessary because closure over stale state would miss late-arriving JSON messages.

**`pending-tab.ts` localStorage + StorageEvent:**
Cross-page tab passing via localStorage. Same-page tab passing via synthetic `StorageEvent` (since `storage` events don't fire for same-page writes). One-shot consume pattern prevents double-open.

**`resultToMarkdown` per-mode dispatch:**
Each inline mode has its own rendering strategy: ask/research → prose answer, query → ranked list with links, retrieve → titled doc. Fallback to JSON code block for unknown JSON shapes.

**Tabs localStorage persistence:**
Full markdown content persisted on every `updateTab` call. SSR-safe via `hydrated` flag (state initialized in `useEffect` from localStorage, never from SSR).

**Inline mode detection in `page.tsx`:**
Local `currentModeRef` kept in sync via `useEffect`. Local `capturedJsonRef` reset on `isProcessing` becoming true (new execution). Second `subscribe` call in `page.tsx` alongside the existing canvas subscribe — parallel subscriptions both receive all messages.

---

## Files Modified

| File | Status | Purpose |
|------|--------|---------|
| `apps/web/lib/pending-tab.ts` | **NEW** | Cross-page tab passing via localStorage + StorageEvent |
| `apps/web/lib/result-to-markdown.ts` | **NEW** | Converts captured JSON payloads to markdown per mode |
| `apps/web/hooks/use-tabs.ts` | **NEW** | localStorage-backed tab state (open/close/activate/update) |
| `apps/web/components/editor-tab-bar.tsx` | **NEW** | Tab strip UI component with dirty/error indicators |
| `apps/web/components/cmdk-palette/CmdKPalette.tsx` | **MODIFIED** | Added background mode, capturedJson, navigation on done |
| `apps/web/components/cmdk-palette/CmdKOutput.tsx` | **MODIFIED** | Added inline result rendering, Background/Cancel buttons |
| `apps/web/components/cmdk-palette/cmdk-palette-types.ts` | **MODIFIED** | Added `capturedJson` to `PaletteDialogState` |
| `apps/web/components/cmdk-palette/cmdk-palette-dialog.tsx` | **MODIFIED** | Added `minimizeToBackground`, `handleOpenInEditor` props |
| `apps/web/app/editor/page.tsx` | **REWRITTEN** | Full tabs system: consumePendingTab, onPendingTab, useTabs |
| `apps/web/app/page.tsx` | **MODIFIED** | Added inline mode detection, JSON capture, editor navigation |

---

## Behavior Changes (Before → After)

| Trigger | Before | After |
|---------|--------|-------|
| Cmd+K ask/research while open | Results shown in ResultsPanel (home only) | Results rendered inline in Cmd+K modal via PulseMarkdown |
| Cmd+K dismissed while running | Cancels operation | Operation continues in background; result opens as editor tab on completion |
| Cmd+K result in modal | "See results panel" note | "Open in Editor" button; auto-open on background completion |
| Omnibox ask/research/query/retrieve | ResultsPanel appears on home page | ResultsPanel hidden; auto-navigate to `/editor` with new tab on completion |
| Omnibox crawl/scrape/embed/other | ResultsPanel appears | Unchanged — ResultsPanel still shown for non-inline modes |
| `/editor` route | Single document, no tabs | Multi-tab editor; tabs persisted in localStorage across sessions |
| Editor load with pending tab | No mechanism | `consumePendingTab()` on hydration; `onPendingTab` listener while open |

---

## Key Implementation Details

### `pending-tab.ts`
```typescript
const KEY = 'axon.web.editor.pending-tab'
// setPendingTab: write to localStorage + dispatch synthetic StorageEvent (same-page)
// consumePendingTab: read + remove (one-shot)
// onPendingTab: StorageEvent listener, returns cleanup fn
```

### `CmdKPalette.tsx` — background mode
```typescript
const isBackgroundRef = useRef(false)         // keep subscription alive
const backgroundModeRef = useRef<{mode, input} | null>(null)  // snapshot for title

// minimizeToBackground: sets isBackgroundRef.current = true, phase='idle'
// Does NOT call unsubRef.current?.() — subscription survives component hide

// On command.done with isBackgroundRef.current:
//   resultToMarkdown(mode, capturedJsonRef.current) → setPendingTab → router.push('/editor')
```

### `use-tabs.ts`
```typescript
// EditorTab = { id, title, markdown, docFilename: string | null }
// Keys: axon.web.editor.tabs, axon.web.editor.active-tab
// closeTab: never zero tabs — replaces last tab with blank
// updateTab: persists on every call (including per-keystroke markdown changes)
```

### `app/editor/page.tsx` — tab routing
```typescript
// key={activeTabId} on PulseEditorPane — fresh Plate instance per tab
// scrollStorageKey={`axon.web.editor.scroll.${activeTabId}`} — per-tab scroll
// ?doc= param: checks for existing tab with same docFilename before loading
// savedFilename written back into tab.docFilename via updateTab
```

### `app/page.tsx` — omnibox result routing
```typescript
const INLINE_RESULT_MODES = new Set(['ask', 'research', 'query', 'retrieve'])
// capturedJsonRef: accumulates command.output.json for inline modes only
// Reset on isProcessing → true; consumed on command.done
// ResultsPanel hidden when isInlineMode === true
```

---

## Verification Evidence

| Check | Expected | Status |
|-------|----------|--------|
| TypeScript compile | No errors | Not run (environment check pending) |
| `pending-tab.ts` exports | setPendingTab, consumePendingTab, onPendingTab | Confirmed by file read |
| `resultToMarkdown` modes covered | ask, research, query, retrieve | Confirmed in result-to-markdown.ts |
| `CmdKPalette` subscription survival | `unsubRef.current?.()` NOT called in minimizeToBackground | Confirmed in code |
| Tab persistence | Tabs survive page reload | Not smoke-tested (no browser access) |
| Omnibox → editor navigation | router.push('/editor') on command.done for inline modes | Confirmed in page.tsx:120 |

---

## Risks and Rollback

**Risks:**
- `capturedJsonRef.current = []` reset fires on every `isProcessing → true` transition, including ones not from inline modes. This is intentional (fresh capture per execution) but means switching modes rapidly could theoretically miss a race-condition edge. In practice the WS messages are sequential so this is safe.
- Two `subscribe` calls in `page.tsx` (canvas + JSON capture). Both receive all messages. The JSON capture subscribe only acts on inline modes, so there's no double-processing concern.
- `resultToMarkdown` fallback to JSON code block means any uncovered mode with JSON output would still produce something (not silently drop).

**Rollback:** Revert `apps/web/app/page.tsx` to remove the inline mode effect. `ResultsPanel` visibility returns to `hasResults` only (no `isInlineMode` gate). Editor tabs system is independent and backward-compatible.

---

## Decisions Not Taken

- **Global store for captured JSON** (Zustand/Context): rejected — scope is local to the execution lifecycle, global state adds unnecessary complexity and coupling.
- **Single subscription in WsMessagesProvider for omnibox routing**: rejected — would require adding navigation logic to a data layer, violating separation of concerns.
- **Expose `currentInput` from WsMessagesContext**: rejected — the omnibox input isn't needed for tab title (mode label suffices); avoids context API churn.
- **Keep ResultsPanel for inline modes**: rejected per user requirement — "it should be auto loaded in our platejs editor."
- **Navigate to `/editor` immediately on execution start** (not on done): rejected — we'd navigate before results are available; user would see a blank editor tab.

---

## Open Questions

- Should the tab title include the input text (e.g., "Ask: what is spider.rs?") for omnibox triggers? Currently uses mode label only ("Ask"). CmdK background mode does use `${mode.label}${input ? ': ' + input : ''}` — omnibox doesn't have input at the page level without context changes.
- Should `ResultsPanel` be removed from the file entirely for inline modes, or just hidden? Currently it's conditionally rendered (`!isInlineMode`). If we decide the panel has no future role for inline modes, it can be fully removed to reduce bundle.
- The landing page mobile editor pane (`PulseEditorPane` in `page.tsx:155`) is still present — it's a separate surface from the tabbed `/editor`. Is this intentional or should it also link to `/editor`?

---

## Next Steps

- Smoke-test the three flows in browser: Cmd+K inline, Cmd+K background, omnibox
- Verify `resultToMarkdown` produces reasonable output for each mode (ask answer prose, research report, query ranked list)
- Consider adding input text to omnibox → editor tab title (requires exposing `currentInput` from context or capturing it locally in `page.tsx` via a separate effect watching `startExecution`)
- Run `pnpm build` / TypeScript check to confirm no type errors
