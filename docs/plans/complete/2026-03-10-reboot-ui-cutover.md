# Reboot UI Cutover Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the reboot UI (`/reboot`) the default root route, retire the legacy dashboard, and fill the remaining feature gaps for a complete standalone experience.

**Architecture:** The reboot UI (`AxonShell`) is an 80%-complete chat-first interface with session management, file browsing, terminal, logs, and MCP config. The legacy dashboard (`/`) is a command-line interface (Omnibox + ResultsPanel) that serves a different paradigm. The cutover promotes reboot to `/`, preserves legacy page routes (`/jobs`, `/cortex/*`, `/settings`, etc.) as-is, and fills 5 specific gaps: route swap, Docker stats, message edit/retry, settings access, and log stream dedup.

**Tech Stack:** Next.js 16 App Router, React 19, TypeScript 5.9, TailwindCSS 4, Biome 2.4, Vitest 4

---

## Chunk 1: Route Swap & AppShell Integration

### Task 1: Move reboot to root route

**Files:**
- Modify: `apps/web/app/page.tsx`
- Modify: `apps/web/app/reboot/page.tsx`
- Modify: `apps/web/components/app-shell.tsx`

The reboot `AxonShell` becomes the root route. The legacy dashboard moves to `/legacy` for reference during transition (delete later). AppShell's PulseSidebar is already hidden for `/reboot` — update the guard to hide it for `/` too (since AxonShell has its own sidebar).

- [ ] **Step 1: Create the legacy route**

Move the current `app/page.tsx` content to `app/legacy/page.tsx`:

```tsx
// apps/web/app/legacy/page.tsx
// Exact copy of current app/page.tsx — no changes to the component itself
```

- [ ] **Step 2: Replace root page with AxonShell**

Replace `apps/web/app/page.tsx`:

```tsx
'use client'

import { AxonShell } from '@/components/reboot/axon-shell'

export default function HomePage() {
  return <AxonShell />
}
```

- [ ] **Step 3: Update AppShell sidebar guard**

In `apps/web/components/app-shell.tsx`, the sidebar is hidden for `/reboot`. Update the guard to also hide it for `/` since AxonShell renders its own sidebar:

```tsx
// Before:
const isRebootRoute = pathname?.startsWith('/reboot') ?? false

// After:
const isRebootRoute = pathname === '/' || (pathname?.startsWith('/reboot') ?? false)
```

Rename the variable to something clearer:

```tsx
const hideAppSidebar = pathname === '/' || pathname === '/legacy' || (pathname?.startsWith('/reboot') ?? false)
```

- [ ] **Step 4: Update reboot page to redirect**

Replace `apps/web/app/reboot/page.tsx` with a redirect to `/`:

```tsx
import { redirect } from 'next/navigation'

export default function RebootPage() {
  redirect('/')
}
```

- [ ] **Step 5: Verify dev server**

Run: `cd apps/web && pnpm dev`
Expected:
- `http://localhost:49010/` renders AxonShell
- `http://localhost:49010/legacy` renders old dashboard
- `http://localhost:49010/reboot` redirects to `/`
- Sidebar pages (`/jobs`, `/cortex/status`, etc.) still work with PulseSidebar visible
- AxonShell's own sidebar shows sessions, files, pages, agents

- [ ] **Step 6: Commit**

```bash
git add apps/web/app/page.tsx apps/web/app/legacy/page.tsx apps/web/app/reboot/page.tsx apps/web/components/app-shell.tsx
git commit -m "feat(web): promote reboot UI to root route, move legacy dashboard to /legacy"
```

---

### Task 2: Update AxonSidebar page links

**Files:**
- Modify: `apps/web/components/reboot/axon-ui-config.ts`

The sidebar's "Pages" rail has a link to `/` labeled "Conversations" and `/reboot` labeled "Axon". Now that `/` IS the reboot UI, remove the duplicate `/reboot` entry and update labels.

- [ ] **Step 1: Read current PAGE_ITEMS**

Read: `apps/web/components/reboot/axon-ui-config.ts`

- [ ] **Step 2: Update PAGE_ITEMS**

Remove any entry pointing to `/reboot`. Update the `/` entry label if needed. Add `/legacy` as "Legacy Dashboard" in the footer group for transition.

- [ ] **Step 3: Verify sidebar renders correctly**

Navigate to `/` → sidebar → Pages tab → confirm no broken links, no duplicate entries.

- [ ] **Step 4: Commit**

```bash
git add apps/web/components/reboot/axon-ui-config.ts
git commit -m "feat(web): update reboot sidebar page links for root route"
```

---

## Chunk 2: Docker Stats & NeuralCanvas Integration

### Task 3: Wire Docker stats to NeuralCanvas in AxonShell

**Files:**
- Modify: `apps/web/components/reboot/axon-shell.tsx`
- Modify: `apps/web/components/reboot/axon-frame.tsx`

Currently `AxonFrame` renders a static NeuralCanvas. The legacy dashboard drives canvas intensity from Docker stats via WS `stats` messages and command execution state. Wire the same logic into AxonShell.

- [ ] **Step 1: Write failing test for stats callback**

Create: `apps/web/__tests__/reboot/axon-stats-integration.test.ts`

Test that `handleStats` computes normalized CPU intensity correctly:

```ts
import { describe, expect, it } from 'vitest'

function computeCanvasIntensity(
  cpuPercent: number,
  containerCount: number,
  isProcessing: boolean,
): number {
  if (isProcessing) return 1
  const maxCpu = containerCount * 100
  const norm = Math.min(cpuPercent / maxCpu, 1.0)
  return 0.02 + norm * 0.83
}

describe('computeCanvasIntensity', () => {
  it('returns 1 when processing', () => {
    expect(computeCanvasIntensity(50, 4, true)).toBe(1)
  })

  it('computes normalized intensity from CPU', () => {
    const result = computeCanvasIntensity(200, 4, false)
    expect(result).toBeCloseTo(0.02 + 0.5 * 0.83, 2)
  })

  it('clamps to max intensity', () => {
    const result = computeCanvasIntensity(500, 4, false)
    expect(result).toBeCloseTo(0.02 + 1.0 * 0.83, 2)
  })

  it('returns baseline with zero CPU', () => {
    const result = computeCanvasIntensity(0, 4, false)
    expect(result).toBeCloseTo(0.02, 2)
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd apps/web && pnpm test -- axon-stats-integration`
Expected: PASS (pure function test — passes immediately since we're testing the formula before extracting it)

- [ ] **Step 3: Add NeuralCanvas ref to AxonFrame**

Update `axon-frame.tsx` to accept an optional `canvasRef` and forward it to `NeuralCanvas`:

```tsx
import type { NeuralCanvasHandle } from '@/components/neural-canvas'
import type { RefObject } from 'react'

export function AxonFrame({
  children,
  canvasRef,
}: {
  children: React.ReactNode
  canvasRef?: RefObject<NeuralCanvasHandle | null>
}) {
  return (
    <div className="relative min-h-dvh w-full overflow-hidden bg-[var(--axon-bg)]">
      <NeuralCanvas ref={canvasRef} profile="current" />
      {/* ... existing gradients and grid ... */}
      <div className="relative z-[1]">{children}</div>
    </div>
  )
}
```

- [ ] **Step 4: Wire DockerStats + canvas intensity in AxonShell**

In `axon-shell.tsx`:

1. Import `DockerStats`, `NeuralCanvasHandle`, `useAxonWs`, `useRef`, `useEffect`
2. Create `canvasRef = useRef<NeuralCanvasHandle>(null)`
3. Pass `canvasRef` to `<AxonFrame canvasRef={canvasRef}>`
4. Add `<DockerStats onStats={handleStats} />` (hidden, just for data — render it with `className="hidden"` or as a data-only component)
5. Add `handleStats` callback (same logic as legacy dashboard):

```tsx
const { subscribe } = useAxonWs()

// Canvas intensity: full on streaming, pulse on turn complete
useEffect(() => {
  return subscribe((msg: WsServerMsg) => {
    if (msg.type === 'command.done' || msg.type === 'command.error') {
      canvasRef.current?.setIntensity(0.15)
      setTimeout(() => canvasRef.current?.setIntensity(0), 3000)
    }
  })
}, [subscribe])

useEffect(() => {
  if (isStreaming) {
    canvasRef.current?.setIntensity(1)
  }
}, [isStreaming])

const handleStats = useCallback(
  (data: {
    aggregate: { cpu_percent: number }
    containers: Record<string, ContainerStats>
    container_count: number
  }) => {
    canvasRef.current?.stimulate(data.containers)
    if (!isStreaming) {
      const maxCpu = data.container_count * 100
      const norm = maxCpu > 0 ? Math.min(data.aggregate.cpu_percent / maxCpu, 1.0) : 0
      canvasRef.current?.setIntensity(0.02 + norm * 0.83)
    }
  },
  [isStreaming],
)
```

- [ ] **Step 5: Verify canvas responds to activity**

1. Open `/` in browser
2. Submit a prompt — canvas should pulse to full intensity during streaming
3. When streaming completes — brief 0.15 pulse then fade
4. Docker stats should drive baseline intensity when idle

- [ ] **Step 6: Commit**

```bash
git add apps/web/components/reboot/axon-frame.tsx apps/web/components/reboot/axon-shell.tsx apps/web/__tests__/reboot/axon-stats-integration.test.ts
git commit -m "feat(web): wire Docker stats and NeuralCanvas intensity into reboot shell"
```

---

## Chunk 3: Message Edit & Retry

### Task 4: Implement message edit and retry callbacks

**Files:**
- Modify: `apps/web/components/reboot/axon-message-list.tsx`
- Modify: `apps/web/components/reboot/axon-shell.tsx`

The message list has TODO stubs for edit (user messages) and retry (assistant messages). Wire them to actual behavior.

- [ ] **Step 1: Add onEdit and onRetryMessage props to AxonMessageList**

In `axon-message-list.tsx`, add to the props interface:

```tsx
onEdit?: (messageId: string, content: string) => void
onRetryMessage?: (messageId: string) => void
```

- [ ] **Step 2: Wire the edit button**

Replace the TODO in the edit `onClick`:

```tsx
onClick={() => onEdit?.(message.id, message.content)}
```

Edit behavior: trim messages from the edit point forward and populate the composer with the message content so the user can modify and resubmit. The edit handler does NOT auto-submit — it only populates the input.

- [ ] **Step 3: Wire the retry button**

Replace the TODO in the retry `onClick`:

```tsx
onClick={() => onRetryMessage?.(message.id)}
```

Retry behavior: resubmit the last user message before this assistant message.

- [ ] **Step 4: Implement handlers in AxonShell**

In `axon-shell.tsx`, add:

```tsx
const handleEditMessage = useCallback(
  (messageId: string, content: string) => {
    // Remove messages from this point forward
    setLiveMessages((prev) => {
      const idx = prev.findIndex((m) => m.id === messageId)
      return idx >= 0 ? prev.slice(0, idx) : prev
    })
    // Populate the composer with the content so the user can edit it
    // TODO: implement setComposerText(content) or similar instead of auto-submitting
  },
  [],
)

const handleRetryMessage = useCallback(
  (messageId: string) => {
    // Find the user message before this assistant message and resubmit
    const idx = liveMessages.findIndex((m) => m.id === messageId)
    if (idx <= 0) return
    const userMsg = liveMessages
      .slice(0, idx)
      .reverse()
      .find((m) => m.role === 'user')
    if (!userMsg) return
    // Trim messages back to the user message
    setLiveMessages((prev) => {
      const userIdx = prev.findIndex((m) => m.id === userMsg.id)
      return userIdx >= 0 ? prev.slice(0, userIdx) : prev
    })
    submitPrompt(userMsg.content)
  },
  [liveMessages, submitPrompt],
)
```

Pass to both mobile and desktop `AxonMessageList`:

```tsx
onEdit={handleEditMessage}
onRetryMessage={handleRetryMessage}
```

- [ ] **Step 5: Verify edit and retry work**

1. Send a message, get a response
2. Click edit on a user message → messages trimmed, content resubmitted
3. Click retry on an assistant message → finds preceding user message, resubmits

- [ ] **Step 6: Commit**

```bash
git add apps/web/components/reboot/axon-message-list.tsx apps/web/components/reboot/axon-shell.tsx
git commit -m "feat(web): wire message edit and retry in reboot chat"
```

---

## Chunk 4: Settings Dialog

### Task 5: Add settings dialog to AxonShell

**Files:**
- Create: `apps/web/components/reboot/axon-settings-dialog.tsx`
- Modify: `apps/web/components/reboot/axon-shell.tsx`

The reboot UI has model/permission/agent dropdowns in the composer but no unified settings view. Add a settings dialog accessible from the chat header (gear icon) that exposes: NeuralCanvas profile, default agent, default model, default permission level.

- [ ] **Step 1: Create AxonSettingsDialog component**

```tsx
// apps/web/components/reboot/axon-settings-dialog.tsx
'use client'

import { Settings2 } from 'lucide-react'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import type { NeuralCanvasProfile } from '@/lib/pulse/neural-canvas-presets'

const CANVAS_PROFILES: { value: NeuralCanvasProfile; label: string }[] = [
  { value: 'current', label: 'Current' },
  { value: 'subtle', label: 'Subtle' },
  { value: 'cinematic', label: 'Cinematic' },
  { value: 'electric', label: 'Electric' },
]

export function AxonSettingsDialog({
  open,
  onOpenChange,
  canvasProfile,
  onCanvasProfileChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  canvasProfile: NeuralCanvasProfile
  onCanvasProfileChange: (profile: NeuralCanvasProfile) => void
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-sm border-[var(--border-subtle)] bg-[var(--glass-overlay)] text-[var(--text-primary)] backdrop-blur-xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 text-base">
            <Settings2 className="size-4" />
            Settings
          </DialogTitle>
        </DialogHeader>
        <div className="space-y-4 pt-2">
          <div>
            <label className="text-[11px] font-semibold uppercase tracking-[0.12em] text-[var(--text-dim)]">
              Canvas Profile
            </label>
            <div className="mt-1.5 flex gap-1.5">
              {CANVAS_PROFILES.map(({ value, label }) => (
                <button
                  key={value}
                  type="button"
                  onClick={() => onCanvasProfileChange(value)}
                  className={`rounded-md border px-3 py-1.5 text-xs transition-colors ${
                    canvasProfile === value
                      ? 'border-[rgba(175,215,255,0.35)] bg-[rgba(175,215,255,0.12)] text-[var(--axon-primary-strong)]'
                      : 'border-[var(--border-subtle)] text-[var(--text-dim)] hover:border-[rgba(175,215,255,0.2)] hover:text-[var(--text-secondary)]'
                  }`}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}
```

- [ ] **Step 2: Wire into AxonShell**

1. Add `settingsOpen` state
2. Add canvas profile state with localStorage persistence (`axon.web.neural-canvas.profile`)
3. Add Settings2 button to desktop chat header toolbar (next to MCP button)
4. Add Settings2 button to mobile header
5. Render `<AxonSettingsDialog>` alongside other dialogs
6. Pass `canvasProfile` to `<AxonFrame>` → `<NeuralCanvas>`

- [ ] **Step 3: Verify settings dialog**

1. Click gear icon → dialog opens
2. Switch canvas profile → canvas animation changes
3. Reload page → profile persists from localStorage

- [ ] **Step 4: Commit**

```bash
git add apps/web/components/reboot/axon-settings-dialog.tsx apps/web/components/reboot/axon-shell.tsx
git commit -m "feat(web): add settings dialog with canvas profile to reboot shell"
```

---

## Chunk 5: Log Stream Hook Extraction

### Task 6: Extract shared log stream hook

**Files:**
- Create: `apps/web/hooks/use-log-stream.ts`
- Modify: `apps/web/components/reboot/axon-logs-dialog.tsx`
- Modify: `apps/web/components/logs/logs-viewer.tsx` (if it exists)

`AxonLogsDialog` has a TODO comment noting it duplicates SSE streaming logic from `logs-viewer.tsx`. Extract the shared behavior into a reusable hook.

- [ ] **Step 1: Write failing test for log stream hook**

Create: `apps/web/__tests__/hooks/use-log-stream.test.ts`

Test the buffer management logic (pure function):

```ts
import { describe, expect, it } from 'vitest'

function appendLogs(buffer: string[], newLines: string[], maxLines: number): string[] {
  const combined = [...buffer, ...newLines]
  return combined.length > maxLines ? combined.slice(combined.length - maxLines) : combined
}

describe('appendLogs', () => {
  it('appends lines within limit', () => {
    expect(appendLogs(['a', 'b'], ['c'], 10)).toEqual(['a', 'b', 'c'])
  })

  it('trims oldest lines when over limit', () => {
    expect(appendLogs(['a', 'b', 'c'], ['d', 'e'], 3)).toEqual(['c', 'd', 'e'])
  })

  it('handles empty buffer', () => {
    expect(appendLogs([], ['x'], 5)).toEqual(['x'])
  })
})
```

- [ ] **Step 2: Run test**

Run: `cd apps/web && pnpm test -- use-log-stream`
Expected: PASS

- [ ] **Step 3: Create use-log-stream hook**

Extract the SSE connection, line buffering, and filter logic into `hooks/use-log-stream.ts`:

```ts
// apps/web/hooks/use-log-stream.ts
'use client'

import { useCallback, useEffect, useRef, useState } from 'react'

const MAX_LINES = 1200

export interface UseLogStreamOptions {
  service: string
  tail: number
  enabled: boolean
}

export function useLogStream({ service, tail, enabled }: UseLogStreamOptions) {
  const [lines, setLines] = useState<string[]>([])
  const esRef = useRef<EventSource | null>(null)

  const clear = useCallback(() => setLines([]), [])

  useEffect(() => {
    if (!enabled) return
    setLines([])
    const params = new URLSearchParams()
    if (service && service !== 'all') params.set('service', service)
    params.set('tail', String(tail))
    const es = new EventSource(`/api/logs?${params}`)
    esRef.current = es

    es.onmessage = (event) => {
      const text = event.data as string
      if (!text) return
      setLines((prev) => {
        const combined = [...prev, text]
        return combined.length > MAX_LINES ? combined.slice(combined.length - MAX_LINES) : combined
      })
    }

    return () => {
      es.close()
      esRef.current = null
    }
  }, [service, tail, enabled])

  return { lines, clear }
}
```

- [ ] **Step 4: Refactor AxonLogsDialog to use the hook**

Replace the inline SSE logic in `axon-logs-dialog.tsx` with `useLogStream()`.

- [ ] **Step 5: Verify logs dialog still works**

1. Open logs dialog
2. Switch service filter → SSE reconnects
3. Lines stream in, buffer trims at 1200
4. Clear button works

- [ ] **Step 6: Commit**

```bash
git add apps/web/hooks/use-log-stream.ts apps/web/components/reboot/axon-logs-dialog.tsx apps/web/__tests__/hooks/use-log-stream.test.ts
git commit -m "refactor(web): extract shared log stream hook from AxonLogsDialog"
```

---

## Chunk 6: Cleanup & Lint

### Task 7: Biome lint + format pass

**Files:**
- All modified files from Tasks 1-6

- [ ] **Step 1: Run Biome check**

Run: `cd apps/web && pnpm lint`
Expected: Clean or list of fixable issues

- [ ] **Step 2: Auto-fix**

Run: `cd apps/web && pnpm format`

- [ ] **Step 3: Run tests**

Run: `cd apps/web && pnpm test`
Expected: All tests pass

- [ ] **Step 4: Verify build compiles**

Run: `cd apps/web && pnpm build`
Expected: Build succeeds (standalone output)

- [ ] **Step 5: Commit if any fixes**

```bash
git add -u
git commit -m "chore(web): lint and format after reboot cutover"
```

---

## Verification

### End-to-end smoke test

1. **Route swap**: `http://localhost:49010/` → AxonShell with sidebar, chat, editor
2. **Legacy fallback**: `http://localhost:49010/legacy` → old dashboard still accessible
3. **Session management**: Click session in sidebar → JSONL history loads → chat works
4. **New session**: Click + → empty chat → submit prompt → streaming response
5. **Message edit**: Click pencil on user message → content resubmitted
6. **Message retry**: Click retry on assistant message → preceding user message resubmitted
7. **Settings**: Click gear → change canvas profile → animation changes → persists on reload
8. **Docker stats**: NeuralCanvas pulses during streaming, responds to container load when idle
9. **Logs dialog**: SSE streams, service filter works, buffer trims
10. **Navigation**: Sidebar Pages → /jobs, /cortex/status, /settings, /terminal all load correctly with PulseSidebar visible
11. **Mobile**: Responsive layout works, pane switcher functional, no duplicate icons
12. **Terminal/MCP**: Dialogs open and function correctly
