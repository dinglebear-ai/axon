# Right Pane Switching Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace dialog-modal rendering of Terminal/Logs/MCP/Settings with inline right-pane content that shares the Editor's slot, controlled by a single `rightPane` state variable.

**Architecture:** Single `rightPane` state replaces 5 booleans (`editorOpen`, `terminalOpen`, `logsOpen`, `mcpOpen`, `settingsOpen`). Clicking a header icon toggles the right pane between that content and `null` (collapsed). Three new pane components are extracted from existing dialog components (stripping `<Dialog>` wrappers). The existing `AxonTerminalPane` already works as a standalone pane. Mobile layout keeps dialog modals for now (separate concern).

**Tech Stack:** React 19, Next.js 16, TypeScript 5.9, TailwindCSS 4, shadcn/ui

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `components/reboot/axon-shell.tsx` | Modify | Replace 5 boolean states with `rightPane`, update header icons, render pane content inline, remove dialog imports/renders, update localStorage persistence |
| `components/reboot/axon-logs-pane.tsx` | Create | Standalone logs pane extracted from `axon-logs-dialog.tsx` (no Dialog wrapper) |
| `components/reboot/axon-mcp-pane.tsx` | Create | Standalone MCP pane extracted from `axon-mcp-dialog.tsx` (no Dialog wrapper) |
| `components/reboot/axon-settings-pane.tsx` | Create | Standalone settings pane extracted from `axon-settings-dialog.tsx` (no Dialog wrapper) |

**Unchanged:** `axon-terminal-pane.tsx` (already standalone), `axon-logs-dialog.tsx` / `axon-mcp-dialog.tsx` / `axon-settings-dialog.tsx` / `axon-terminal-dialog.tsx` (kept for potential mobile use).

---

## Chunk 1: Extract Pane Components

### Task 1: Create AxonLogsPane

Extract the inner content from `axon-logs-dialog.tsx` into a standalone pane component that renders without any `<Dialog>` wrapper.

**Files:**
- Create: `apps/web/components/reboot/axon-logs-pane.tsx`
- Reference: `apps/web/components/reboot/axon-logs-dialog.tsx` (lines 22-195)

- [ ] **Step 1: Create `axon-logs-pane.tsx`**

The component is the inner content of `AxonLogsDialog` (lines 29-191) wrapped in a plain `<div>` instead of `<Dialog>/<DialogContent>`. Key differences from the dialog version:
- No `open`/`onOpenChange` props — always mounted when visible
- `useLogStream` gets `enabled: true` (always active when mounted)
- Root element: `<div className="flex h-full flex-col">` instead of Dialog wrappers
- Keep the header with title + icon (repurposed from `DialogHeader`)
- Keep toolbar, virtual log list, and footer stats bar

```tsx
'use client'

import { useVirtualizer } from '@tanstack/react-virtual'
import { ScrollText } from 'lucide-react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { LogLine } from '@/components/logs/log-line'
import {
  type IndividualService,
  LogsToolbar,
  SERVICES,
  type ServiceName,
  TAIL_OPTIONS,
  type TailLines,
} from '@/components/logs/logs-toolbar'
import { useLogStream } from '@/hooks/use-log-stream'

const MAX_LINES = 1200
const LOGS_SERVICE_KEY = 'axon.web.logs.service'
const DEFAULT_SERVICE: ServiceName = 'all'

export function AxonLogsPane() {
  const [service, setService] = useState<ServiceName>(DEFAULT_SERVICE)
  const [tailLines, setTailLines] = useState<TailLines>(TAIL_OPTIONS[1])
  const [filter, setFilter] = useState('')
  const [autoScroll, setAutoScroll] = useState(true)
  const [compact, setCompact] = useState(true)
  const [wrapLines, setWrapLines] = useState(false)

  const {
    lines,
    isConnected,
    clear: clearLines,
  } = useLogStream({
    service,
    tail: tailLines,
    enabled: true,
  })

  const scrollAreaRef = useRef<HTMLDivElement>(null)
  const autoScrollRef = useRef(autoScroll)

  useEffect(() => {
    autoScrollRef.current = autoScroll
  }, [autoScroll])

  useEffect(() => {
    try {
      const saved = window.localStorage.getItem(LOGS_SERVICE_KEY)
      if (saved && (SERVICES.includes(saved as IndividualService) || saved === 'all')) {
        setService(saved as ServiceName)
      }
    } catch {
      /* ignore */
    }
  }, [])

  const handleScroll = useCallback(() => {
    const el = scrollAreaRef.current
    if (!el) return
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40
    if (!atBottom) setAutoScroll(false)
  }, [])

  const filteredLines = useMemo(() => {
    if (!filter.trim()) return lines
    const lower = filter.toLowerCase()
    return lines.filter((l) => l.text.toLowerCase().includes(lower))
  }, [lines, filter])

  const rowVirtualizer = useVirtualizer({
    count: filteredLines.length,
    getScrollElement: () => scrollAreaRef.current,
    estimateSize: () => (wrapLines ? 36 : 20),
    overscan: 30,
  })

  useEffect(() => {
    if (autoScrollRef.current && filteredLines.length > 0) {
      rowVirtualizer.scrollToIndex(filteredLines.length - 1)
    }
  }, [filteredLines, rowVirtualizer])

  function handleServiceChange(s: ServiceName) {
    setService(s)
    try {
      window.localStorage.setItem(LOGS_SERVICE_KEY, s)
    } catch {
      /* ignore */
    }
  }

  function handleAutoScrollToggle() {
    const next = !autoScroll
    setAutoScroll(next)
    if (next && filteredLines.length > 0) {
      rowVirtualizer.scrollToIndex(filteredLines.length - 1)
    }
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-[var(--border-subtle)] px-4 py-3">
        <ScrollText className="size-4 text-[var(--axon-primary-strong)]" />
        <span className="text-[14px] font-semibold text-[var(--text-primary)]">Docker Logs</span>
      </div>

      <div className="shrink-0 border-b border-[var(--border-subtle)] px-4 py-2.5">
        <LogsToolbar
          service={service}
          tailLines={tailLines}
          filter={filter}
          autoScroll={autoScroll}
          compact={compact}
          wrapLines={wrapLines}
          isConnected={isConnected}
          onServiceChange={handleServiceChange}
          onTailChange={setTailLines}
          onFilterChange={setFilter}
          onAutoScrollToggle={handleAutoScrollToggle}
          onCompactToggle={() => setCompact((prev) => !prev)}
          onWrapToggle={() => setWrapLines((prev) => !prev)}
          onClear={clearLines}
        />
      </div>

      <div
        ref={scrollAreaRef}
        onScroll={handleScroll}
        className="min-h-0 flex-1 overflow-y-auto px-4 py-2 font-mono text-xs"
        style={{ background: 'rgba(3,7,18,0.6)' }}
        role="log"
        aria-live="polite"
        aria-label={
          service === 'all' ? 'Log output for all services' : `Log output for ${service}`
        }
      >
        {filteredLines.length === 0 && (
          <div className="flex h-32 items-center justify-center">
            <p className="text-[11px] text-[var(--text-dim)]">
              {isConnected ? 'Waiting for log output\u2026' : 'Connecting\u2026'}
            </p>
          </div>
        )}
        <div style={{ height: `${rowVirtualizer.getTotalSize()}px`, position: 'relative' }}>
          {rowVirtualizer.getVirtualItems().map((virtualRow) => {
            const entry = filteredLines[virtualRow.index]
            return (
              <div
                key={virtualRow.key}
                style={{
                  position: 'absolute',
                  top: 0,
                  transform: `translateY(${virtualRow.start}px)`,
                  width: '100%',
                }}
              >
                <LogLine entry={entry} compact={compact} wrapLines={wrapLines} />
              </div>
            )
          })}
        </div>
      </div>

      <div className="flex shrink-0 items-center gap-3 border-t border-[var(--border-subtle)] px-4 py-2">
        <span className="text-[10px] text-[var(--text-dim)]">
          {filteredLines.length.toLocaleString()} line{filteredLines.length !== 1 ? 's' : ''}
          {filter ? ' (filtered)' : ''}
        </span>
        <span className="text-[10px] text-[var(--text-dim)]">
          Snapshot {tailLines} then live stream{' '}
          {service === 'all' ? '(all services)' : `(${service})`}
        </span>
        {lines.length >= MAX_LINES && (
          <span className="text-[10px] text-[var(--axon-warning)]">
            Buffer capped at {MAX_LINES.toLocaleString()} lines
          </span>
        )}
      </div>
    </div>
  )
}
```

- [ ] **Step 2: Verify file created**

Run: `ls -la apps/web/components/reboot/axon-logs-pane.tsx`
Expected: File exists

- [ ] **Step 3: Commit**

```bash
git add apps/web/components/reboot/axon-logs-pane.tsx
git commit -m "feat(web): extract AxonLogsPane from dialog wrapper"
```

---

### Task 2: Create AxonMcpPane

Extract the MCP server management content from `axon-mcp-dialog.tsx` into a standalone pane.

**Files:**
- Create: `apps/web/components/reboot/axon-mcp-pane.tsx`
- Reference: `apps/web/components/reboot/axon-mcp-dialog.tsx` (lines 20-256 inner content)

- [ ] **Step 1: Create `axon-mcp-pane.tsx`**

The component reuses `DeleteConfirmModal`, `EmptyState`, and the `McpDialogContent` logic from the dialog file. Key differences:
- No `<Dialog>/<DialogContent>` wrapper
- Root: `<div className="flex h-full flex-col">`
- Header with MCP icon + title (matching pane header style from AxonLogsPane)
- All inner state and API calls remain identical

```tsx
'use client'

import { Network, Plus, Trash2 } from 'lucide-react'
import { useCallback, useEffect, useState } from 'react'
import {
  configToForm,
  EMPTY_FORM,
  type FormState,
  type McpConfig,
  McpServerCard,
  type McpServerConfig,
  McpServerForm,
  type McpServerStatus,
} from '@/app/settings/mcp/components'
import { ErrorBoundary } from '@/components/ui/error-boundary'
import { apiFetch } from '@/lib/api-fetch'
import { McpIcon } from './mcp-config'

function DeleteConfirmModal({
  name,
  onConfirm,
  onCancel,
}: {
  name: string
  onConfirm: () => void
  onCancel: () => void
}) {
  return (
    <div className="absolute inset-0 z-10 flex items-center justify-center bg-[rgba(3,7,18,0.75)] backdrop-blur-sm">
      <div className="w-full max-w-sm rounded-xl border border-[var(--border-standard)] bg-[var(--surface-base)] p-5 shadow-[var(--shadow-xl)]">
        <div className="mb-1 flex items-center gap-2">
          <Trash2 className="size-4 text-[var(--axon-secondary)]" />
          <h3 className="text-sm font-semibold text-[var(--text-primary)]">
            Delete &ldquo;{name}&rdquo;?
          </h3>
        </div>
        <p className="mb-4 text-xs text-[var(--text-muted)]">
          This MCP server configuration will be permanently removed.
        </p>
        <div className="flex justify-end gap-2">
          <button
            type="button"
            onClick={onCancel}
            className="rounded-md border border-[var(--border-subtle)] bg-transparent px-3 py-1.5 text-xs text-[var(--text-secondary)] transition-colors hover:bg-[var(--surface-float)]"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={onConfirm}
            className="rounded-md border border-[var(--border-accent)] bg-[rgba(255,135,175,0.15)] px-3 py-1.5 text-xs text-[var(--axon-secondary)] transition-colors hover:bg-[rgba(255,135,175,0.25)]"
          >
            Delete
          </button>
        </div>
      </div>
    </div>
  )
}

function EmptyState({ onAdd }: { onAdd: () => void }) {
  return (
    <div className="flex min-h-[200px] flex-col items-center justify-center gap-4 rounded-xl border border-dashed border-[var(--border-subtle)] bg-[var(--surface-float)] p-8 text-center">
      <Network className="size-8 text-[var(--axon-primary)]" />
      <div className="space-y-1">
        <p className="text-sm font-semibold text-[var(--text-primary)]">
          No MCP servers configured
        </p>
        <p className="text-xs text-[var(--text-muted)]">
          MCP servers extend Claude&apos;s capabilities with external tools, APIs, and data sources.
        </p>
      </div>
      <button
        type="button"
        onClick={onAdd}
        className="flex items-center gap-1.5 rounded-lg border border-[var(--border-standard)] bg-[rgba(135,175,255,0.15)] px-4 py-2 text-[12px] font-semibold text-[var(--axon-primary)] transition-colors hover:bg-[rgba(135,175,255,0.25)]"
      >
        <Plus className="size-3.5" />
        Add your first server
      </button>
    </div>
  )
}

function McpPaneContent() {
  const [config, setConfig] = useState<McpConfig>({ mcpServers: {} })
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [formOpen, setFormOpen] = useState(false)
  const [editTarget, setEditTarget] = useState<string | null>(null)
  const [deleteModal, setDeleteModal] = useState<string | null>(null)
  const [statusMap, setStatusMap] = useState<Record<string, McpServerStatus>>({})

  const loadStatus = useCallback(async (signal?: AbortSignal) => {
    try {
      const res = await apiFetch('/api/mcp/status', { signal })
      if (!res.ok) return
      const data = (await res.json()) as {
        servers: Record<string, { status: McpServerStatus; error?: string }>
      }
      setStatusMap(Object.fromEntries(Object.entries(data.servers).map(([k, v]) => [k, v.status])))
    } catch (err) {
      void err
    }
  }, [])

  const loadConfig = useCallback(
    async (signal?: AbortSignal) => {
      try {
        const res = await apiFetch('/api/mcp', { signal })
        if (!res.ok) throw new Error(`HTTP ${res.status}`)
        const data = (await res.json()) as McpConfig
        setConfig(data)
        setError('')
        setStatusMap(
          Object.fromEntries(
            Object.keys(data.mcpServers).map((k) => [k, 'checking' as McpServerStatus]),
          ),
        )
        void loadStatus(signal)
      } catch (err) {
        if (err instanceof Error && err.name === 'AbortError') return
        setError(err instanceof Error ? err.message : 'Failed to load')
      } finally {
        setLoading(false)
      }
    },
    [loadStatus],
  )

  useEffect(() => {
    const controller = new AbortController()
    void loadConfig(controller.signal)
    return () => controller.abort()
  }, [loadConfig])

  async function saveServer(name: string, cfg: McpServerConfig) {
    const previousConfig: McpConfig = config
    const mergedConfig: McpConfig = { mcpServers: { ...config.mcpServers, [name]: cfg } }
    setConfig(mergedConfig)
    setFormOpen(false)
    setEditTarget(null)
    const res = await apiFetch('/api/mcp', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json', 'X-Pulse-Request': '1' },
      body: JSON.stringify(mergedConfig),
    })
    if (!res.ok) {
      setConfig(previousConfig)
      setError('Save failed')
    }
  }

  async function deleteServer(name: string) {
    const res = await apiFetch('/api/mcp', {
      method: 'DELETE',
      headers: { 'Content-Type': 'application/json', 'X-Pulse-Request': '1' },
      body: JSON.stringify({ name }),
    })
    if (!res.ok) {
      setError('Delete failed')
      return
    }
    const controller = new AbortController()
    await loadConfig(controller.signal)
  }

  const servers = Object.entries(config.mcpServers)
  const existingNames = servers.map(([n]) => n)
  const formInitial: FormState =
    editTarget && config.mcpServers[editTarget]
      ? configToForm(editTarget, config.mcpServers[editTarget])
      : EMPTY_FORM

  function openAdd() {
    setEditTarget(null)
    setFormOpen(true)
  }

  return (
    <div
      className="relative flex min-h-0 flex-1 flex-col overflow-hidden"
    >
      <div className="flex shrink-0 items-center justify-between border-b border-[var(--border-subtle)] px-4 py-2.5">
        {error ? <span className="text-xs text-red-400">{error}</span> : <span />}
        <button
          type="button"
          onClick={openAdd}
          className="flex items-center gap-1.5 rounded-lg border border-[rgba(175,215,255,0.18)] bg-[rgba(175,215,255,0.07)] px-3 py-1.5 text-[12px] font-semibold text-[var(--axon-primary-strong)] transition-colors hover:bg-[rgba(175,215,255,0.13)]"
        >
          <Plus className="size-3.5" />
          Add Server
        </button>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4">
        {formOpen && (
          <div className="mb-6">
            <McpServerForm
              key={editTarget ?? '__new__'}
              initial={formInitial}
              existingNames={existingNames}
              isEditing={editTarget !== null}
              onSave={saveServer}
              onCancel={() => {
                setFormOpen(false)
                setEditTarget(null)
              }}
            />
          </div>
        )}

        {loading ? (
          <div className="flex items-center justify-center py-20">
            <div className="size-6 animate-spin rounded-full border-2 border-[rgba(175,215,255,0.2)] border-t-[var(--axon-primary-strong)]" />
          </div>
        ) : servers.length === 0 && !formOpen ? (
          <EmptyState onAdd={openAdd} />
        ) : (
          <div className="space-y-2">
            {servers.map(([name, cfg]) => (
              <McpServerCard
                key={name}
                name={name}
                cfg={cfg}
                status={statusMap[name] ?? 'unknown'}
                onEdit={() => {
                  setEditTarget(name)
                  setFormOpen(true)
                }}
                onDelete={() => setDeleteModal(name)}
              />
            ))}
          </div>
        )}
      </div>

      {deleteModal && (
        <DeleteConfirmModal
          name={deleteModal}
          onConfirm={() => {
            void deleteServer(deleteModal)
            setDeleteModal(null)
          }}
          onCancel={() => setDeleteModal(null)}
        />
      )}
    </div>
  )
}

export function AxonMcpPane() {
  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-[var(--border-subtle)] px-4 py-3">
        <McpIcon className="size-4 text-[var(--axon-primary-strong)]" />
        <span className="text-[14px] font-semibold text-[var(--text-primary)]">MCP Servers</span>
      </div>
      <ErrorBoundary>
        <McpPaneContent />
      </ErrorBoundary>
    </div>
  )
}
```

- [ ] **Step 2: Verify file created**

Run: `ls -la apps/web/components/reboot/axon-mcp-pane.tsx`
Expected: File exists

- [ ] **Step 3: Commit**

```bash
git add apps/web/components/reboot/axon-mcp-pane.tsx
git commit -m "feat(web): extract AxonMcpPane from dialog wrapper"
```

---

### Task 3: Create AxonSettingsPane

Extract settings content from `axon-settings-dialog.tsx` into a standalone pane.

**Files:**
- Create: `apps/web/components/reboot/axon-settings-pane.tsx`
- Reference: `apps/web/components/reboot/axon-settings-dialog.tsx` (lines 7-61)

- [ ] **Step 1: Create `axon-settings-pane.tsx`**

Simple extraction — remove Dialog wrapper, keep canvas profile selector.

```tsx
'use client'

import { Settings2 } from 'lucide-react'
import type { NeuralCanvasProfile } from '@/lib/pulse/neural-canvas-presets'

const CANVAS_PROFILES: { value: NeuralCanvasProfile; label: string }[] = [
  { value: 'current', label: 'Current' },
  { value: 'subtle', label: 'Subtle' },
  { value: 'cinematic', label: 'Cinematic' },
  { value: 'electric', label: 'Electric' },
  { value: 'zen', label: 'Zen' },
]

export function AxonSettingsPane({
  canvasProfile,
  onCanvasProfileChange,
}: {
  canvasProfile: NeuralCanvasProfile
  onCanvasProfileChange: (profile: NeuralCanvasProfile) => void
}) {
  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-[var(--border-subtle)] px-4 py-3">
        <Settings2 className="size-4 text-[var(--axon-primary-strong)]" />
        <span className="text-[14px] font-semibold text-[var(--text-primary)]">Settings</span>
      </div>
      <div className="flex-1 overflow-y-auto px-4 py-4">
        <div>
          <span className="text-[11px] font-semibold uppercase tracking-[0.12em] text-[var(--text-dim)]">
            Canvas Profile
          </span>
          <div className="mt-1.5 flex flex-wrap gap-1.5">
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
    </div>
  )
}
```

- [ ] **Step 2: Verify file created**

Run: `ls -la apps/web/components/reboot/axon-settings-pane.tsx`
Expected: File exists

- [ ] **Step 3: Commit**

```bash
git add apps/web/components/reboot/axon-settings-pane.tsx
git commit -m "feat(web): extract AxonSettingsPane from dialog wrapper"
```

---

## Chunk 2: Wire Pane Switching in AxonShell

### Task 4: Replace boolean states with `rightPane` state

Replace the 5 boolean state variables with a single `rightPane` discriminated union. Update localStorage persistence to save/restore the pane selection.

**Files:**
- Modify: `apps/web/components/reboot/axon-shell.tsx`

- [ ] **Step 1: Add `RightPane` type and replace state variables**

At the top of the file (after the storage key constants, around line 63), replace the `EDITOR_OPEN_STORAGE_KEY` with a new key:

```ts
// REMOVE this line:
const EDITOR_OPEN_STORAGE_KEY = 'axon.web.reboot.editor-open'

// ADD this line:
const RIGHT_PANE_STORAGE_KEY = 'axon.web.reboot.right-pane'
```

Add the type (after the `AxonMobilePane` type on line 57):

```ts
type RightPane = 'editor' | 'terminal' | 'logs' | 'mcp' | 'settings' | null
const VALID_RIGHT_PANES = new Set<string>(['editor', 'terminal', 'logs', 'mcp', 'settings'])
```

Replace the 5 boolean state variables (lines 161-166):

```ts
// REMOVE these 5 lines:
const [editorOpen, setEditorOpen] = useState(true)
const [terminalOpen, setTerminalOpen] = useState(false)
const [logsOpen, setLogsOpen] = useState(false)
const [mcpOpen, setMcpOpen] = useState(false)
const [settingsOpen, setSettingsOpen] = useState(false)

// ADD this 1 line:
const [rightPane, setRightPane] = useState<RightPane>('editor')
```

Derive `editorOpen` for any code that still references it (e.g., the `persistSidebarOpen` / `persistChatOpen` guards):

```ts
const editorOpen = rightPane !== null
```

- [ ] **Step 2: Update localStorage restore (mount effect)**

In the mount effect (around line 337-339), replace:

```ts
// REMOVE:
setEditorOpen(readStoredBool(EDITOR_OPEN_STORAGE_KEY, true))

// ADD:
const storedPane = getStorageItem(RIGHT_PANE_STORAGE_KEY)
if (storedPane && VALID_RIGHT_PANES.has(storedPane)) {
  setRightPane(storedPane as RightPane)
} else {
  setRightPane('editor')
}
```

- [ ] **Step 3: Replace `persistEditorOpen` with `persistRightPane`**

Replace the `persistEditorOpen` callback (lines 543-554):

```ts
// REMOVE the entire persistEditorOpen callback

// ADD:
const persistRightPane = useCallback(
  (pane: RightPane) => {
    if (pane === null && !sidebarOpen && !chatOpen) return
    setRightPane(pane)
    try {
      window.localStorage.setItem(RIGHT_PANE_STORAGE_KEY, pane ?? '')
    } catch {
      /* ignore */
    }
  },
  [sidebarOpen, chatOpen],
)
```

Update `persistSidebarOpen` and `persistChatOpen` — replace references to `editorOpen` in their guards with `rightPane !== null`:

```ts
// In persistSidebarOpen (line 519):
// CHANGE: if (!open && !chatOpen && !editorOpen) return
// TO:     if (!open && !chatOpen && rightPane === null) return

// In persistChatOpen (line 532):
// CHANGE: if (!open && !sidebarOpen && !editorOpen) return
// TO:     if (!open && !sidebarOpen && rightPane === null) return
```

Also update their dependency arrays to reference `rightPane` instead of `editorOpen`.

- [ ] **Step 4: Verify the state changes compile**

Run: `cd apps/web && pnpm exec tsc --noEmit 2>&1 | head -30`
Expected: Errors only from missing pane component imports (not yet wired) or reference errors we'll fix next

- [ ] **Step 5: Commit**

```bash
git add apps/web/components/reboot/axon-shell.tsx
git commit -m "refactor(web): replace 5 boolean states with single rightPane state"
```

---

### Task 5: Update header icon click handlers

Wire each header icon to toggle `rightPane` instead of individual boolean states. Highlight the active icon.

**Files:**
- Modify: `apps/web/components/reboot/axon-shell.tsx` (lines 921-987)

- [ ] **Step 1: Update desktop header icon handlers**

Replace the icon button section (lines 921-987) with toggle handlers:

**Terminal button** (line 922-933):
```tsx
<Button
  type="button"
  variant="ghost"
  size="icon-sm"
  className={
    rightPane === 'terminal' ? 'text-[var(--axon-primary)]' : 'text-[var(--text-secondary)]'
  }
  onClick={() => persistRightPane(rightPane === 'terminal' ? null : 'terminal')}
>
  <TerminalSquare className="size-4" />
  <span className="sr-only">Toggle terminal</span>
</Button>
```

**Logs button** (line 934-943):
```tsx
<Button
  type="button"
  variant="ghost"
  size="icon-sm"
  className={
    rightPane === 'logs' ? 'text-[var(--axon-primary)]' : 'text-[var(--text-secondary)]'
  }
  onClick={() => persistRightPane(rightPane === 'logs' ? null : 'logs')}
>
  <ScrollText className="size-4" />
  <span className="sr-only">Toggle logs</span>
</Button>
```

**MCP button** (line 944-953):
```tsx
<Button
  type="button"
  variant="ghost"
  size="icon-sm"
  className={
    rightPane === 'mcp' ? 'text-[var(--axon-primary)]' : 'text-[var(--text-secondary)]'
  }
  onClick={() => persistRightPane(rightPane === 'mcp' ? null : 'mcp')}
>
  <McpIcon className="size-4" />
  <span className="sr-only">Toggle MCP servers</span>
</Button>
```

**Settings button** (line 954-963):
```tsx
<Button
  type="button"
  variant="ghost"
  size="icon-sm"
  className={
    rightPane === 'settings' ? 'text-[var(--axon-primary)]' : 'text-[var(--text-secondary)]'
  }
  onClick={() => persistRightPane(rightPane === 'settings' ? null : 'settings')}
>
  <Settings2 className="size-4" />
  <span className="sr-only">Toggle settings</span>
</Button>
```

**Editor button** (line 976-987) — replace `persistEditorOpen(!editorOpen)`:
```tsx
<Button
  type="button"
  variant="ghost"
  size="icon-sm"
  className={
    rightPane === 'editor' ? 'text-[var(--axon-primary)]' : 'text-[var(--text-secondary)]'
  }
  onClick={() => persistRightPane(rightPane === 'editor' ? null : 'editor')}
>
  <PanelRight className="size-4" />
  <span className="sr-only">Toggle editor</span>
</Button>
```

- [ ] **Step 2: Update mobile header icon handlers**

In the mobile header (lines 732-774), update the terminal/logs/mcp buttons to use `persistRightPane` instead of individual setters. Mobile keeps the same toggle pattern but routes through the same state:

```tsx
// Terminal (line 735): setTerminalOpen((current) => !current)
// → persistRightPane(rightPane === 'terminal' ? null : 'terminal')

// Logs (line 748): setLogsOpen(true)
// → persistRightPane(rightPane === 'logs' ? null : 'logs')

// MCP (line 756): setMcpOpen(true)
// → persistRightPane(rightPane === 'mcp' ? null : 'mcp')
```

Update the mobile terminal button's `aria-pressed` and className conditions from `terminalOpen` to `rightPane === 'terminal'`.

- [ ] **Step 3: Verify no references to old setters remain**

Run: `grep -n 'setTerminalOpen\|setLogsOpen\|setMcpOpen\|setSettingsOpen\|setEditorOpen\|persistEditorOpen' apps/web/components/reboot/axon-shell.tsx`
Expected: No matches (all replaced with `persistRightPane` or `setRightPane`)

- [ ] **Step 4: Commit**

```bash
git add apps/web/components/reboot/axon-shell.tsx
git commit -m "feat(web): wire header icons to rightPane toggle"
```

---

### Task 6: Render right pane content inline and remove dialogs

Replace the Editor-only right pane rendering with a multi-content pane. Remove dialog component renders.

**Files:**
- Modify: `apps/web/components/reboot/axon-shell.tsx` (lines 1030-1055)

- [ ] **Step 1: Update imports**

At the top of `axon-shell.tsx`, replace dialog imports with pane imports:

```ts
// REMOVE these 4 imports:
import { AxonLogsDialog } from './axon-logs-dialog'
import { AxonMcpDialog } from './axon-mcp-dialog'
import { AxonSettingsDialog } from './axon-settings-dialog'
import { AxonTerminalDialog } from './axon-terminal-dialog'

// ADD these 3 imports (terminal pane already imported if used elsewhere, check first):
import { AxonLogsPane } from './axon-logs-pane'
import { AxonMcpPane } from './axon-mcp-pane'
import { AxonSettingsPane } from './axon-settings-pane'
import { AxonTerminalPane } from './axon-terminal-pane'
```

- [ ] **Step 2: Replace right pane rendering (lines 1030-1044)**

Replace the Editor-only conditional with multi-pane rendering:

```tsx
{/* Right pane */}
{rightPane ? (
  <aside
    className={`h-full min-h-0 overflow-hidden bg-[var(--glass-editor)] animate-fade-in ${transitionClass}`}
    style={{ flex: '1 1 0%', minWidth: PANE_WIDTH_MIN }}
  >
    {rightPane === 'editor' && (
      <PulseEditorPane
        markdown={editorMarkdown}
        onMarkdownChange={setEditorMarkdown}
        scrollStorageKey="axon.web.reboot.editor-scroll"
      />
    )}
    {rightPane === 'terminal' && <AxonTerminalPane />}
    {rightPane === 'logs' && <AxonLogsPane />}
    {rightPane === 'mcp' && <AxonMcpPane />}
    {rightPane === 'settings' && (
      <AxonSettingsPane
        canvasProfile={canvasProfile}
        onCanvasProfileChange={handleCanvasProfileChange}
      />
    )}
  </aside>
) : (
  <AxonPaneHandle label="Editor" side="right" onClick={() => persistRightPane('editor')} />
)}
```

- [ ] **Step 3: Remove dialog renders (lines 1047-1055)**

Delete these lines entirely:

```tsx
// DELETE all of these:
<AxonLogsDialog open={logsOpen} onOpenChange={setLogsOpen} />
<AxonMcpDialog open={mcpOpen} onOpenChange={setMcpOpen} />
<AxonTerminalDialog open={terminalOpen} onOpenChange={setTerminalOpen} />
<AxonSettingsDialog
  open={settingsOpen}
  onOpenChange={setSettingsOpen}
  canvasProfile={canvasProfile}
  onCanvasProfileChange={handleCanvasProfileChange}
/>
```

- [ ] **Step 4: Update the resize handle condition (line 1022)**

The resize divider currently shows when `chatOpen && editorOpen`. Change to show when `chatOpen && rightPane !== null`:

```tsx
// CHANGE:
{chatOpen && editorOpen ? (
// TO:
{chatOpen && rightPane !== null ? (
```

- [ ] **Step 5: Verify build compiles**

Run: `cd apps/web && pnpm exec tsc --noEmit 2>&1 | head -30`
Expected: No errors

- [ ] **Step 6: Run lint**

Run: `cd apps/web && pnpm lint 2>&1 | tail -20`
Expected: No new errors

- [ ] **Step 7: Commit**

```bash
git add apps/web/components/reboot/axon-shell.tsx
git commit -m "feat(web): render Terminal/Logs/MCP/Settings inline in right pane"
```

---

## Chunk 3: Final Verification

### Task 7: Build and lint verification

**Files:**
- All modified files

- [ ] **Step 1: Run TypeScript type check**

Run: `cd apps/web && pnpm exec tsc --noEmit`
Expected: Clean (0 errors)

- [ ] **Step 2: Run Biome lint**

Run: `cd apps/web && pnpm lint`
Expected: Clean

- [ ] **Step 3: Run build**

Run: `cd apps/web && pnpm build 2>&1 | tail -20`
Expected: Build succeeds

- [ ] **Step 4: Commit any lint fixes**

```bash
git add -A
git commit -m "chore(web): lint fixes for right pane switching"
```

---

## Verification Checklist

1. Editor shows in right pane by default
2. Click Terminal icon → right pane switches to terminal
3. Click Logs icon → right pane switches to logs
4. Click MCP icon → right pane switches to MCP config
5. Click Settings icon → right pane switches to settings
6. Click the active icon again → right pane closes (collapse handle shows)
7. Click collapse handle → re-opens editor (default)
8. Editor content persists when switching away and back
9. Terminal session persists when switching away and back (component stays mounted via CSS hidden class, not conditional rendering)
10. Active icon is highlighted with `--axon-primary` color
11. localStorage saves `rightPane` selection and restores on reload
12. `pnpm lint` and `pnpm build` pass
