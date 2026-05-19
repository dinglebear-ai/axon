# 3-Panel Collapsible Layout Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the current `DesktopViewMode` enum + swap-panes pattern with three independently collapsible panels (Sidebar, Chat, Editor), where Chat is left of Editor by default, and each panel collapses to a 28px chevron strip.

**Architecture:** Two booleans (`showChat`, `showEditor`) replace the `DesktopViewMode = 'chat' | 'editor' | 'both'` enum and `DesktopPaneOrder = 'editor-first' | 'chat-first'` type. The workspace layout is fixed-order `[Chat | handle | Editor]` — the old swap-panes button is removed. Collapsed panels shrink to a 28px-wide strip showing a directional chevron button. The drag handle between Chat and Editor has dual behavior: pointer drag ≥ 4px → resize split; pointer click < 4px → toggle Editor.

**Tech Stack:** React (hooks, refs), Tailwind CSS, Next.js App Router, localStorage for persistence, lucide-react icons.

---

## Task 1: Update workspace-persistence.ts types

**Files:**
- Modify: `apps/web/lib/pulse/workspace-persistence.ts`

Replace `DesktopViewMode` / `DesktopPaneOrder` types and persisted fields with `showChat: boolean` / `showEditor: boolean`. The old types are exported and referenced in several places — we update them here first so TypeScript errors guide the remaining tasks.

**Step 1: Edit workspace-persistence.ts**

Replace the `DesktopViewMode` / `DesktopPaneOrder` type exports and the `PersistedPulseWorkspaceState` fields, and update `parsePersistedWorkspaceState` + `buildPersistedPayload`:

```typescript
// REMOVE these two lines:
// export type DesktopViewMode = 'chat' | 'editor' | 'both'
// export type DesktopPaneOrder = 'editor-first' | 'chat-first'

// In PersistedPulseWorkspaceState, REPLACE:
//   desktopViewMode: DesktopViewMode
//   desktopPaneOrder: DesktopPaneOrder
// WITH:
  showChat: boolean
  showEditor: boolean
```

In `parsePersistedWorkspaceState`, replace the `desktopViewMode`/`desktopPaneOrder` parse block with migration logic:

```typescript
// Migration: if old desktopViewMode is present, derive showChat/showEditor from it.
// New fields take priority.
const showChat =
  typeof parsed.showChat === 'boolean'
    ? parsed.showChat
    : (parsed as Record<string, unknown>).desktopViewMode !== 'editor'  // old: 'chat' or 'both' → showChat true
const showEditor =
  typeof parsed.showEditor === 'boolean'
    ? parsed.showEditor
    : (parsed as Record<string, unknown>).desktopViewMode !== 'chat'    // old: 'editor' or 'both' → showEditor true
```

Full replacement for the return object block:

```typescript
return {
  permissionLevel,
  model,
  documentMarkdown: parsed.documentMarkdown,
  chatHistory: Array.isArray(parsed.chatHistory) ? parsed.chatHistory.slice(-250) : [],
  documentTitle: parsed.documentTitle,
  currentDocFilename:
    typeof parsed.currentDocFilename === 'string' ? parsed.currentDocFilename : null,
  chatSessionId: typeof parsed.chatSessionId === 'string' ? parsed.chatSessionId : null,
  indexedSources: Array.isArray(parsed.indexedSources) ? parsed.indexedSources.slice(-50) : [],
  activeThreadSources: Array.isArray(parsed.activeThreadSources)
    ? parsed.activeThreadSources.slice(-50)
    : [],
  desktopSplitPercent: clampSplit(parseSplit(parsed.desktopSplitPercent, 62), 20, 80),
  mobileSplitPercent: clampSplit(parseSplit(parsed.mobileSplitPercent, 56), 35, 70),
  lastResponseLatencyMs:
    typeof parsed.lastResponseLatencyMs === 'number' ? parsed.lastResponseLatencyMs : null,
  lastResponseModel:
    parsed.lastResponseModel === 'sonnet' ||
    parsed.lastResponseModel === 'opus' ||
    parsed.lastResponseModel === 'haiku'
      ? parsed.lastResponseModel
      : null,
  showChat,
  showEditor,
  savedAt: typeof parsed.savedAt === 'number' ? parsed.savedAt : Date.now(),
}
```

Note: The split clamp is expanded from `42-74` to `20-80` — needed when only one panel is open and the other is a narrow strip.

**Step 2: Verify TypeScript errors**

Run:
```bash
cd /home/jmagar/workspace/axon_rust/apps/web && pnpm tsc --noEmit 2>&1 | head -40
```

Expected: Errors in `use-split-pane.ts`, `use-pulse-persistence.ts`, `pulse-workspace.tsx`, `pulse-toolbar.tsx` (these reference old types). No errors in `workspace-persistence.ts` itself.

**Step 3: Commit**

```bash
cd /home/jmagar/workspace/axon_rust
git add apps/web/lib/pulse/workspace-persistence.ts
git commit -m "feat(web): replace DesktopViewMode/DesktopPaneOrder with showChat/showEditor booleans"
```

---

## Task 2: Rewrite use-split-pane.ts for 3-panel layout

**Files:**
- Modify: `apps/web/hooks/use-split-pane.ts`

Remove `desktopViewMode` / `desktopPaneOrder` state. Add `showChat` / `showEditor` boolean state. Update the drag handle `stopDrag` to detect click (total pointer movement < 4px) and toggle `showEditor`.

**Step 1: Update use-split-pane.ts**

Full replacement of the file:

```typescript
'use client'

import { useCallback, useEffect, useRef, useState } from 'react'

const DESKTOP_SPLIT_STORAGE_KEY = 'axon.web.pulse.editor-split.desktop'
const MOBILE_SPLIT_STORAGE_KEY = 'axon.web.pulse.editor-split.mobile'
const SHOW_CHAT_STORAGE_KEY = 'axon.web.pulse.show-chat'
const SHOW_EDITOR_STORAGE_KEY = 'axon.web.pulse.show-editor'
export const MOBILE_PANE_STORAGE_KEY = 'axon.web.pulse.mobile-pane'

export function useSplitPane() {
  const [desktopSplitPercent, setDesktopSplitPercent] = useState(50)
  const [mobileSplitPercent, setMobileSplitPercent] = useState(56)
  const [isDesktop, setIsDesktop] = useState(false)
  const [mobilePane, setMobilePane] = useState<'chat' | 'editor'>('chat')
  const [showChat, setShowChat] = useState(true)
  const [showEditor, setShowEditor] = useState(true)

  const desktopSplitPercentRef = useRef(50)
  const mobileSplitPercentRef = useRef(56)
  const dragStartRef = useRef<{ pointerX: number; startPercent: number } | null>(null)
  const verticalDragStartRef = useRef<{ pointerY: number; startPercent: number } | null>(null)
  const splitContainerRef = useRef<HTMLDivElement>(null)
  const splitHandleRef = useRef<HTMLDivElement>(null)
  const showChatRef = useRef(true)
  const showEditorRef = useRef(true)

  // Keep refs in sync with state
  useEffect(() => { desktopSplitPercentRef.current = desktopSplitPercent }, [desktopSplitPercent])
  useEffect(() => { mobileSplitPercentRef.current = mobileSplitPercent }, [mobileSplitPercent])
  useEffect(() => { showChatRef.current = showChat }, [showChat])
  useEffect(() => { showEditorRef.current = showEditor }, [showEditor])

  // Storage restore effect
  useEffect(() => {
    try {
      const desktop = window.localStorage.getItem(DESKTOP_SPLIT_STORAGE_KEY)
      const mobile = window.localStorage.getItem(MOBILE_SPLIT_STORAGE_KEY)
      const parsedDesktop = Number(desktop)
      const parsedMobile = Number(mobile)
      if (Number.isFinite(parsedDesktop) && parsedDesktop >= 20 && parsedDesktop <= 80) {
        setDesktopSplitPercent(parsedDesktop)
      }
      if (Number.isFinite(parsedMobile) && parsedMobile >= 35 && parsedMobile <= 70) {
        setMobileSplitPercent(parsedMobile)
      }
      const pane = window.localStorage.getItem(MOBILE_PANE_STORAGE_KEY)
      if (pane === 'chat' || pane === 'editor') setMobilePane(pane)
      const storedShowChat = window.localStorage.getItem(SHOW_CHAT_STORAGE_KEY)
      const storedShowEditor = window.localStorage.getItem(SHOW_EDITOR_STORAGE_KEY)
      if (storedShowChat === 'false') setShowChat(false)
      if (storedShowEditor === 'false') setShowEditor(false)
    } catch {
      // Ignore storage errors.
    }
  }, [])

  // Media query effect
  useEffect(() => {
    const media = window.matchMedia('(min-width: 1024px)')
    const update = () => setIsDesktop(media.matches)
    update()
    media.addEventListener('change', update)
    return () => media.removeEventListener('change', update)
  }, [])

  // Horizontal drag effect — click (< 4px) toggles editor; drag (>= 4px) resizes
  useEffect(() => {
    function onPointerMove(event: PointerEvent) {
      const start = dragStartRef.current
      const container = splitContainerRef.current
      if (!start || !container) return
      const rect = container.getBoundingClientRect()
      if (rect.width <= 0) return
      const deltaPx = event.clientX - start.pointerX
      const deltaPercent = (deltaPx / rect.width) * 100
      const next = Math.max(20, Math.min(80, start.startPercent + deltaPercent))
      setDesktopSplitPercent(next)
    }

    function stopDrag(event: PointerEvent) {
      const start = dragStartRef.current
      if (!start) return
      const totalMovement = Math.abs(event.clientX - start.pointerX)
      dragStartRef.current = null
      splitHandleRef.current?.classList.remove('bg-[rgba(175,215,255,0.15)]')
      if (totalMovement < 4) {
        // Click — toggle the editor panel
        const next = !showEditorRef.current
        setShowEditor(next)
        try {
          window.localStorage.setItem(SHOW_EDITOR_STORAGE_KEY, String(next))
        } catch { /* ignore */ }
        return
      }
      // Drag — persist the new split position
      try {
        window.localStorage.setItem(
          DESKTOP_SPLIT_STORAGE_KEY,
          String(desktopSplitPercentRef.current),
        )
      } catch { /* ignore */ }
    }

    window.addEventListener('pointermove', onPointerMove)
    window.addEventListener('pointerup', stopDrag)
    return () => {
      window.removeEventListener('pointermove', onPointerMove)
      window.removeEventListener('pointerup', stopDrag)
    }
  }, [])

  // Vertical drag effect (mobile)
  useEffect(() => {
    function onPointerMove(event: PointerEvent) {
      const start = verticalDragStartRef.current
      const container = splitContainerRef.current
      if (!start || !container) return
      const rect = container.getBoundingClientRect()
      if (rect.height <= 0) return
      const deltaPx = event.clientY - start.pointerY
      const deltaPercent = (deltaPx / rect.height) * 100
      const next = Math.max(35, Math.min(70, start.startPercent + deltaPercent))
      setMobileSplitPercent(next)
    }

    function stopVerticalDrag() {
      if (!verticalDragStartRef.current) return
      verticalDragStartRef.current = null
      try {
        window.localStorage.setItem(MOBILE_SPLIT_STORAGE_KEY, String(mobileSplitPercentRef.current))
      } catch { /* ignore */ }
    }

    window.addEventListener('pointermove', onPointerMove)
    window.addEventListener('pointerup', stopVerticalDrag)
    return () => {
      window.removeEventListener('pointermove', onPointerMove)
      window.removeEventListener('pointerup', stopVerticalDrag)
    }
  }, [])

  const persistMobilePane = useCallback((pane: 'chat' | 'editor') => {
    setMobilePane(pane)
    try {
      window.localStorage.setItem(MOBILE_PANE_STORAGE_KEY, pane)
    } catch { /* ignore */ }
  }, [])

  const toggleChat = useCallback((next?: boolean) => {
    setShowChat((prev) => {
      const value = next ?? !prev
      try {
        window.localStorage.setItem(SHOW_CHAT_STORAGE_KEY, String(value))
      } catch { /* ignore */ }
      return value
    })
  }, [])

  const toggleEditor = useCallback((next?: boolean) => {
    setShowEditor((prev) => {
      const value = next ?? !prev
      try {
        window.localStorage.setItem(SHOW_EDITOR_STORAGE_KEY, String(value))
      } catch { /* ignore */ }
      return value
    })
  }, [])

  return {
    desktopSplitPercent,
    setDesktopSplitPercent,
    mobileSplitPercent,
    setMobileSplitPercent,
    isDesktop,
    mobilePane,
    setMobilePane: persistMobilePane,
    showChat,
    setShowChat,
    toggleChat,
    showEditor,
    setShowEditor,
    toggleEditor,
    splitContainerRef,
    splitHandleRef,
    dragStartRef,
    verticalDragStartRef,
    desktopSplitPercentRef,
    mobileSplitPercentRef,
  }
}
```

**Step 2: Verify no TypeScript errors in the hook**

```bash
cd /home/jmagar/workspace/axon_rust/apps/web && pnpm tsc --noEmit 2>&1 | grep "use-split-pane"
```

Expected: No errors in `use-split-pane.ts`.

**Step 3: Commit**

```bash
cd /home/jmagar/workspace/axon_rust
git add apps/web/hooks/use-split-pane.ts
git commit -m "feat(web): rewrite use-split-pane for 3-panel chevron layout"
```

---

## Task 3: Update use-pulse-persistence.ts

**Files:**
- Modify: `apps/web/hooks/use-pulse-persistence.ts`

Replace `desktopViewMode`/`desktopPaneOrder` with `showChat`/`showEditor` throughout.

**Step 1: Update UsePulsePersistenceInput interface**

Replace the four lines referencing old types:
```typescript
// REMOVE:
//   desktopViewMode: DesktopViewMode
//   desktopPaneOrder: DesktopPaneOrder
//   setDesktopViewMode: (v: DesktopViewMode) => void
//   setDesktopPaneOrder: (v: DesktopPaneOrder) => void

// ADD:
  showChat: boolean
  showEditor: boolean
  setShowChat: (v: boolean) => void
  setShowEditor: (v: boolean) => void
```

**Step 2: Update import**

Remove `DesktopViewMode` and `DesktopPaneOrder` from the import in `workspace-persistence`:
```typescript
import type {
  ChatMessage,
} from '@/lib/pulse/workspace-persistence'
```

**Step 3: Update function body**

In the destructuring and function body, replace `desktopViewMode`/`desktopPaneOrder` with `showChat`/`showEditor`:

- Destructure: `showChat, showEditor, setShowChat, setShowEditor`
- Hydration: `setShowChat(restored.showChat)` and `setShowEditor(restored.showEditor)` (remove old setters)
- `persistWorkspaceState` payload: `showChat, showEditor` (remove old fields)
- `useCallback` dep array: `showChat, showEditor` (remove old deps)

**Step 4: Verify**

```bash
cd /home/jmagar/workspace/axon_rust/apps/web && pnpm tsc --noEmit 2>&1 | grep "use-pulse-persistence"
```

Expected: No errors.

**Step 5: Commit**

```bash
cd /home/jmagar/workspace/axon_rust
git add apps/web/hooks/use-pulse-persistence.ts
git commit -m "feat(web): update use-pulse-persistence for showChat/showEditor"
```

---

## Task 4: Update pulse-toolbar.tsx — remove view-mode buttons

**Files:**
- Modify: `apps/web/components/pulse/pulse-toolbar.tsx`

Remove: `desktopViewMode`, `onDesktopViewModeChange`, `desktopPaneOrder`, `onSwapPanes` props + the three view-mode buttons (MessageSquare, Columns2, FileText) + the ArrowLeftRight swap button. Keep: title input, New session, MCP/Agents/Settings nav buttons.

**Step 1: Strip old props and icons**

Remove from imports: `ArrowLeftRight`, `Columns2`, `FileText`, `MessageSquare`.

Remove from `PulseToolbarProps`:
```typescript
// REMOVE:
//   desktopViewMode?: DesktopViewMode
//   onDesktopViewModeChange?: (mode: DesktopViewMode) => void
//   desktopPaneOrder?: DesktopPaneOrder
//   onSwapPanes?: () => void
```

Remove from destructuring: `desktopViewMode`, `onDesktopViewModeChange`, `desktopPaneOrder`, `onSwapPanes`.

**Step 2: Remove the view-mode button group from the JSX**

Delete the three buttons (chat only, both, editor only) and the swap button + separator that precedes it. Leave only: `onNewSession` section + separator + MCP/Agents/Settings buttons.

**Step 3: Remove the local `DesktopViewMode` / `DesktopPaneOrder` type aliases** (lines 16-17 in current file — these are local redeclarations, not imported):
```typescript
// REMOVE:
// type DesktopViewMode = 'chat' | 'editor' | 'both'
// type DesktopPaneOrder = 'editor-first' | 'chat-first'
```

**Step 4: Verify**

```bash
cd /home/jmagar/workspace/axon_rust/apps/web && pnpm tsc --noEmit 2>&1 | grep "pulse-toolbar"
```

Expected: No errors.

**Step 5: Commit**

```bash
cd /home/jmagar/workspace/axon_rust
git add apps/web/components/pulse/pulse-toolbar.tsx
git commit -m "feat(web): remove view-mode toggle buttons from PulseToolbar"
```

---

## Task 5: Rewrite pulse-workspace.tsx layout

**Files:**
- Modify: `apps/web/components/pulse/pulse-workspace.tsx`

This is the main layout change. Replace the `desktopViewMode`/`desktopPaneOrder`-based visibility logic with `showChat`/`showEditor`. Implement:
- Fixed order: `[Chat | handle | Editor]` (chat always left)
- Collapsed panels = 28px strip with directional chevron
- Drag handle dual behavior (wire `dragStartRef.current` + collapse buttons)
- Remove `setDesktopViewMode`, `setDesktopPaneOrder`, `onSwapPanes` from persistence call and toolbar props

**Step 1: Update useSplitPane destructuring**

Replace old fields in the destructuring:
```typescript
const {
  desktopSplitPercent,
  setDesktopSplitPercent,
  mobileSplitPercent,
  setMobileSplitPercent,
  isDesktop,
  mobilePane,
  setMobilePane,
  showChat,
  setShowChat,
  toggleChat,
  showEditor,
  setShowEditor,
  toggleEditor,
  splitContainerRef,
  splitHandleRef,
  dragStartRef,
  verticalDragStartRef,
} = useSplitPane()
```

**Step 2: Update usePulsePersistence call**

Replace `desktopViewMode`/`desktopPaneOrder`/`setDesktopViewMode`/`setDesktopPaneOrder` with `showChat`/`showEditor`/`setShowChat`/`setShowEditor`.

**Step 3: Update PulseToolbar usage**

Remove `desktopViewMode`, `onDesktopViewModeChange`, `desktopPaneOrder`, `onSwapPanes` from the `<PulseToolbar>` props.

**Step 4: Replace the desktop split container JSX**

Replace the current `<div ref={splitContainerRef} ...>` contents with the new 3-panel layout:

```tsx
<div
  ref={splitContainerRef}
  className="flex h-full min-w-0 flex-1 flex-col gap-1.5 p-1.5 lg:flex-row lg:gap-0"
>
  {/* ── Chat panel ── */}
  <div
    className={`group/chat relative flex h-full flex-col overflow-hidden rounded-xl bg-[rgba(10,18,35,0.52)] transition-all duration-200 ${
      isDesktop
        ? showChat
          ? 'lg:flex-1'
          : 'lg:w-7 lg:flex-none'
        : mobilePane === 'chat'
          ? 'flex'
          : 'hidden'
    }`}
  >
    {isDesktop && !showChat ? (
      /* Collapsed chat strip */
      <button
        type="button"
        onClick={() => toggleChat(true)}
        aria-label="Expand chat"
        title="Expand chat"
        className="flex h-full w-7 flex-col items-center justify-center text-[var(--text-dim)] transition-colors hover:text-[var(--axon-primary)]"
      >
        <ChevronRight className="size-4" />
      </button>
    ) : (
      <>
        <PulseChatPane
          messages={chatHistory}
          isLoading={isChatLoading}
          streamingPhase={streamPhase}
          liveToolUses={liveToolUses}
          onCancelRequest={handleCancelPrompt}
          indexedSources={indexedSources}
          activeThreadSources={activeThreadSources}
          onRemoveSource={(url) =>
            setActiveThreadSources((prev) => prev.filter((u) => u !== url))
          }
          onRetry={(prompt) => void handlePrompt(prompt)}
          sourcesExpanded={sourcesExpanded}
          onSourcesExpandedChange={setSourcesExpanded}
          requestNotice={requestNotice}
        />
        {pendingOps && pendingValidation && (
          <div className="p-3">
            <PulseOpConfirmation
              operations={pendingOps}
              validation={pendingValidation}
              onConfirm={() => {
                applyOperations(pendingOps)
                setPendingOps(null)
                setPendingValidation(null)
              }}
              onReject={() => {
                setPendingOps(null)
                setPendingValidation(null)
              }}
            />
          </div>
        )}
        {/* Collapse chat button — right inner edge, desktop only */}
        {isDesktop && (
          <button
            type="button"
            onClick={() => toggleChat(false)}
            aria-label="Collapse chat"
            title="Collapse chat"
            className="absolute right-0 top-1/2 z-10 flex h-10 w-4 -translate-y-1/2 items-center justify-center rounded-l border border-r-0 border-[var(--border-subtle)] bg-[rgba(10,18,35,0.72)] text-[var(--text-dim)] opacity-0 transition-opacity hover:text-[var(--axon-primary)] group-hover/chat:opacity-100"
          >
            <ChevronLeft className="size-3" />
          </button>
        )}
      </>
    )}
  </div>

  {/* ── Drag handle (desktop, both panels open) ── */}
  {isDesktop && (
    <div
      ref={splitHandleRef}
      role="separator"
      aria-label="Resize chat/editor — drag or click to toggle editor"
      aria-orientation="vertical"
      aria-valuenow={Math.round(desktopSplitPercent)}
      aria-valuemin={20}
      aria-valuemax={80}
      className={`group mx-0.5 hidden w-2 cursor-col-resize items-center justify-center rounded-sm transition-colors hover:bg-[var(--border-subtle)] ${
        showChat && showEditor ? 'lg:flex' : 'lg:hidden'
      }`}
      onPointerDown={(event) => {
        dragStartRef.current = {
          pointerX: event.clientX,
          startPercent: desktopSplitPercent,
        }
        splitHandleRef.current?.classList.add('bg-[rgba(175,215,255,0.15)]')
      }}
    >
      <div className="flex flex-col gap-0.5 opacity-30 transition-opacity group-hover:opacity-70">
        {[0, 1, 2, 3, 4].map((i) => (
          <div key={i} className="size-0.5 rounded-full bg-[var(--text-muted)]" />
        ))}
      </div>
    </div>
  )}

  {/* ── Editor panel ── */}
  <div
    className={`group/editor relative flex h-full flex-col overflow-hidden rounded-xl bg-[rgba(10,18,35,0.5)] transition-all duration-200 ${
      isDesktop
        ? showEditor
          ? 'lg:flex-none'
          : 'lg:w-7 lg:flex-none'
        : mobilePane === 'editor'
          ? 'flex'
          : 'hidden'
    }`}
    style={
      isDesktop && showEditor
        ? { flexBasis: `${100 - desktopSplitPercent}%` }
        : undefined
    }
  >
    {isDesktop && !showEditor ? (
      /* Collapsed editor strip */
      <button
        type="button"
        onClick={() => toggleEditor(true)}
        aria-label="Expand editor"
        title="Expand editor"
        className="flex h-full w-7 flex-col items-center justify-center text-[var(--text-dim)] transition-colors hover:text-[var(--axon-primary)]"
      >
        <ChevronLeft className="size-4" />
      </button>
    ) : (
      <>
        {/* Collapse editor button — left inner edge, desktop only */}
        {isDesktop && (
          <button
            type="button"
            onClick={() => toggleEditor(false)}
            aria-label="Collapse editor"
            title="Collapse editor"
            className="absolute left-0 top-1/2 z-10 flex h-10 w-4 -translate-y-1/2 items-center justify-center rounded-r border border-l-0 border-[var(--border-subtle)] bg-[rgba(10,18,35,0.72)] text-[var(--text-dim)] opacity-0 transition-opacity hover:text-[var(--axon-primary)] group-hover/editor:opacity-100"
          >
            <ChevronRight className="size-3" />
          </button>
        )}
        <PulseEditorPane
          markdown={documentMarkdown}
          onMarkdownChange={setDocumentMarkdown}
          scrollStorageKey="axon.web.pulse.editor-scroll"
        />
      </>
    )}
  </div>
</div>
```

**Note on collapse button hover:** The `group-hover/chat:opacity-100` and `group-hover/editor:opacity-100` patterns require adding `group/chat` and `group/editor` classes to the respective panel `<div>` wrappers. Add `group/chat` to the chat panel div and `group/editor` to the editor panel div.

**Note on flexBasis:** The chat panel uses `flex-1` (fills remaining space) and the editor gets `flexBasis: (100 - desktopSplitPercent)%`. Dragging the handle right increases `desktopSplitPercent`, which shrinks the editor's explicit basis and lets chat expand via flex-1. When either panel is collapsed (28px wide), it is `flex-none` with explicit `w-7` so the remaining panel fills available space naturally.

**Step 5: Add ChevronLeft/ChevronRight to lucide imports**

These icons should already be in scope from lucide-react if not already imported. Add them to the import line.

**Step 6: Verify**

```bash
cd /home/jmagar/workspace/axon_rust/apps/web && pnpm tsc --noEmit 2>&1 | grep "pulse-workspace"
```

Expected: No errors.

**Step 7: Commit**

```bash
cd /home/jmagar/workspace/axon_rust
git add apps/web/components/pulse/pulse-workspace.tsx
git commit -m "feat(web): 3-panel collapsible layout — chat left, editor right, chevron strips"
```

---

## Task 6: Visual verification

**Step 1: Open the workspace in browser**

Navigate to `https://axon.tootie.tv` (the running container dev server).

Check:
- [ ] Chat is on the LEFT of the editor by default
- [ ] Both panels are open on first load
- [ ] Drag handle between Chat and Editor is visible
- [ ] Dragging the handle resizes the split (both panels resize)
- [ ] Clicking the handle (< 4px movement) collapses the Editor
- [ ] When Editor is collapsed: 28px strip with `ChevronLeft` appears on the right
- [ ] Clicking the collapsed editor strip expands the editor
- [ ] When Chat is collapsed: 28px strip with `ChevronRight` appears on the left
- [ ] Hovering either panel shows the collapse chevron on its inner edge
- [ ] Panel visibility persists after page refresh
- [ ] Mobile: pane switcher still works (chat / editor toggle)

**Step 2: Fix any visual issues found**

Common issues to watch for:
- Collapse button not visible on hover (check Tailwind group variant class names)
- Split percentages inverted (chat gets editor's width — check which panel uses `desktopSplitPercent` vs `100 - desktopSplitPercent`)
- Transition flicker (check `transition-all duration-200` is on the right elements)

---

## Verification Checklist

```
[ ] pnpm tsc --noEmit exits 0
[ ] Chat is LEFT of editor (conversation-first)
[ ] Both panels start open
[ ] Drag handle resizes split when dragged ≥ 4px
[ ] Drag handle click (< 4px) collapses editor
[ ] Collapsed panel = 28px strip with directional chevron
[ ] Clicking collapsed strip expands the panel
[ ] Hover on open panel reveals inner-edge collapse button
[ ] Panel open/closed state persists to localStorage
[ ] Old desktopViewMode in persisted state migrates cleanly (chat→showChat=true/showEditor=false, etc.)
[ ] No TypeScript errors
[ ] Mobile pane switcher unaffected
[ ] PulseToolbar no longer shows view-mode buttons
```
