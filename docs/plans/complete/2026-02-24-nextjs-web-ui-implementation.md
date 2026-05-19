# Next.js Web UI Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build the Axon dashboard at `apps/web/` — omnibox command input, Plate editor for content rendering, neural canvas background, Docker stats, all powered by the existing axum WebSocket backend.

**Architecture:** SPA dashboard with `'use client'` throughout. A single WS connection (via React context) dispatches messages to isolated components: Omnibox (input/execute), ResultsPanel (tabbed output with Plate editor for content), NeuralCanvas (imperative canvas animation driven by Docker stats), and DockerStats (container metrics grid). No SSR, no API routes — all data flows through the axum WS at `:3333/ws`.

**Tech Stack:** Next.js 16.1.6, React 19.2.4, Plate.js 52.x, shadcn/ui, Tailwind CSS v4, Biome v2, TypeScript 5.9

**Reference files (existing vanilla UI to port):**
- `crates/web/static/neural.js` — canvas animation (~917 lines)
- `crates/web/static/app.js` — WS connection, message handling, UI logic (~938 lines)
- `crates/web/static/style.css` — bioluminescent dark theme (~1115 lines)
- `crates/web/static/index.html` — DOM structure, mode options (~161 lines)
- `crates/web/execute.rs` — ALLOWED_MODES + ALLOWED_FLAGS whitelist

---

## Task 1: Install remaining shadcn/ui primitives

**Files:**
- Create: `apps/web/components/ui/button.tsx` (via CLI)
- Create: `apps/web/components/ui/input.tsx` (via CLI)
- Create: `apps/web/components/ui/tabs.tsx` (via CLI)
- Create: `apps/web/components/ui/scroll-area.tsx` (via CLI)
- Create: `apps/web/components/ui/badge.tsx` (via CLI)

**Step 1: Install components via shadcn CLI**

```bash
cd apps/web
pnpm dlx shadcn@latest add button input tabs scroll-area badge
```

**Step 2: Verify build**

```bash
pnpm build
```

Expected: passes with all 3 routes (/, /_not-found, /editor)

**Step 3: Commit**

```bash
git add apps/web/components/ui/button.tsx apps/web/components/ui/input.tsx \
  apps/web/components/ui/tabs.tsx apps/web/components/ui/scroll-area.tsx \
  apps/web/components/ui/badge.tsx apps/web/package.json apps/web/pnpm-lock.yaml
git commit -m "feat(web): add shadcn button, input, tabs, scroll-area, badge"
```

---

## Task 2: WS protocol types

**Files:**
- Create: `apps/web/lib/ws-protocol.ts`

**Step 1: Create type definitions**

These types must match exactly what `crates/web/execute.rs` sends/receives.

```typescript
// Client → Server
export type WsClientMsg =
  | { type: 'execute'; mode: string; input: string; flags: Record<string, string | boolean> }
  | { type: 'cancel'; id: string }

// Server → Client
export type WsServerMsg =
  | { type: 'output'; line: string }
  | { type: 'log'; line: string }
  | { type: 'done'; exit_code: number; elapsed_ms: number }
  | { type: 'error'; message: string; elapsed_ms?: number; stderr?: string }
  | { type: 'stats'; aggregate: AggregateStats; containers: Record<string, ContainerStats>; container_count: number }

export interface AggregateStats {
  cpu_percent: number
  avg_memory_percent: number
  total_net_io_rate: number
}

export interface ContainerStats {
  cpu_percent: number
  memory_usage_mb: number
  memory_limit_mb: number
  net_rx_rate: number
  net_tx_rate: number
}

export type WsStatus = 'connected' | 'reconnecting' | 'disconnected'

// Mode definitions — must match ALLOWED_MODES in crates/web/execute.rs
export const MODES = [
  { id: 'scrape', label: 'Scrape', icon: 'M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z' },
  { id: 'crawl', label: 'Crawl', icon: 'M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9m-9 9a9 9 0 019-9' },
  { id: 'map', label: 'Map', icon: 'M9 20l-5.447-2.724A1 1 0 013 16.382V5.618a1 1 0 011.447-.894L9 7m0 13l6-3m-6 3V7m6 10l4.553 2.276A1 1 0 0021 18.382V7.618a1 1 0 00-.553-.894L15 4m0 13V4m0 0L9 7' },
  { id: 'extract', label: 'Extract', icon: 'M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M9 19l3 3m0 0l3-3m-3 3V10' },
  { id: 'embed', label: 'Embed', icon: 'M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10' },
  { id: 'query', label: 'Query', icon: 'M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z' },
  { id: 'ask', label: 'Ask', icon: 'M8 10h.01M12 10h.01M16 10h.01M9 16H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-5l-5 5v-5z' },
  { id: 'research', label: 'Research', icon: 'M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253' },
  { id: 'search', label: 'Search', icon: 'M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0zM10 7v3m0 0v3m0-3h3m-3 0H7' },
  { id: 'evaluate', label: 'Evaluate', icon: 'M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-6 9l2 2 4-4' },
  { id: 'doctor', label: 'Doctor', icon: 'M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z' },
  { id: 'sources', label: 'Sources', icon: 'M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4' },
  { id: 'domains', label: 'Domains', icon: 'M3.055 11H5a2 2 0 012 2v1a2 2 0 002 2 2 2 0 012 2v2.945M8 3.935V5.5A2.5 2.5 0 0010.5 8h.5a2 2 0 012 2 2 2 0 104 0 2 2 0 012-2h1.064M15 20.488V18a2 2 0 012-2h3.064' },
  { id: 'stats', label: 'Stats', icon: 'M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z' },
  { id: 'status', label: 'Status', icon: 'M13 10V3L4 14h7v7l9-11h-7z' },
  { id: 'suggest', label: 'Suggest', icon: 'M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z' },
] as const

export type ModeId = (typeof MODES)[number]['id']

// Modes that auto-execute without input
export const NO_INPUT_MODES = new Set<ModeId>([
  'stats', 'status', 'doctor', 'domains', 'sources', 'suggest', 'debug', 'sessions',
])
```

**Step 2: Verify build**

```bash
pnpm build
```

**Step 3: Commit**

```bash
git add apps/web/lib/ws-protocol.ts
git commit -m "feat(web): add WS protocol types matching axum backend"
```

---

## Task 3: WS connection hook

**Files:**
- Create: `apps/web/hooks/use-axon-ws.ts`
- Create: `apps/web/app/providers.tsx`

**Step 1: Create the WS hook**

Port the reconnect logic from `app.js` lines 166-241. Exponential backoff (1s base, 30s max), auto-reconnect on close, status tracking.

```typescript
// apps/web/hooks/use-axon-ws.ts
'use client'

import { createContext, useCallback, useContext, useEffect, useRef, useState } from 'react'
import type { WsClientMsg, WsServerMsg, WsStatus } from '@/lib/ws-protocol'

const BASE_BACKOFF = 1000
const MAX_BACKOFF = 30000

interface AxonWsContextValue {
  status: WsStatus
  statusLabel: string
  send: (msg: WsClientMsg) => void
  subscribe: (handler: (msg: WsServerMsg) => void) => () => void
}

export const AxonWsContext = createContext<AxonWsContextValue | null>(null)

export function useAxonWs() {
  const ctx = useContext(AxonWsContext)
  if (!ctx) throw new Error('useAxonWs must be used within AxonWsProvider')
  return ctx
}

export function useAxonWsProvider() {
  const [status, setStatus] = useState<WsStatus>('disconnected')
  const [statusLabel, setStatusLabel] = useState('DISCONNECTED')
  const wsRef = useRef<WebSocket | null>(null)
  const attemptsRef = useRef(0)
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const handlersRef = useRef(new Set<(msg: WsServerMsg) => void>())

  const connect = useCallback(() => {
    if (wsRef.current?.readyState === WebSocket.CONNECTING || wsRef.current?.readyState === WebSocket.OPEN) return

    const proto = globalThis.location?.protocol === 'https:' ? 'wss:' : 'ws:'
    const envUrl = process.env.NEXT_PUBLIC_AXON_WS_URL
    const wsUrl = envUrl || `${proto}//${globalThis.location?.host}/ws`

    try {
      const ws = new WebSocket(wsUrl)
      wsRef.current = ws

      ws.onopen = () => {
        attemptsRef.current = 0
        setStatus('connected')
        setStatusLabel('CONNECTED')
      }

      ws.onmessage = (event) => {
        try {
          const msg: WsServerMsg = JSON.parse(event.data)
          for (const handler of handlersRef.current) handler(msg)
        } catch { /* malformed */ }
      }

      ws.onclose = () => {
        setStatus('reconnecting')
        scheduleReconnect()
      }

      ws.onerror = () => { /* onclose fires after */ }
    } catch {
      scheduleReconnect()
    }
  }, [])

  const scheduleReconnect = useCallback(() => {
    if (timerRef.current) return
    const delay = Math.min(BASE_BACKOFF * 2 ** attemptsRef.current, MAX_BACKOFF)
    attemptsRef.current++
    setStatusLabel(`RETRY ${Math.round(delay / 1000)}s`)
    timerRef.current = setTimeout(() => {
      timerRef.current = null
      connect()
    }, delay)
  }, [connect])

  useEffect(() => {
    connect()
    return () => {
      wsRef.current?.close()
      if (timerRef.current) clearTimeout(timerRef.current)
    }
  }, [connect])

  const send = useCallback((msg: WsClientMsg) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify(msg))
    }
  }, [])

  const subscribe = useCallback((handler: (msg: WsServerMsg) => void) => {
    handlersRef.current.add(handler)
    return () => { handlersRef.current.delete(handler) }
  }, [])

  const updateStatusLabel = useCallback((label: string) => {
    setStatusLabel(label)
  }, [])

  return { status, statusLabel, send, subscribe, updateStatusLabel }
}
```

**Step 2: Create the providers wrapper**

```typescript
// apps/web/app/providers.tsx
'use client'

import type { ReactNode } from 'react'
import { AxonWsContext, useAxonWsProvider } from '@/hooks/use-axon-ws'
import { TooltipProvider } from '@/components/ui/tooltip'

export function Providers({ children }: { children: ReactNode }) {
  const ws = useAxonWsProvider()
  return (
    <AxonWsContext value={ws}>
      <TooltipProvider>
        {children}
      </TooltipProvider>
    </AxonWsContext>
  )
}
```

**Step 3: Verify build**

```bash
pnpm build
```

**Step 4: Commit**

```bash
git add apps/web/hooks/use-axon-ws.ts apps/web/app/providers.tsx
git commit -m "feat(web): add WS connection hook with exponential backoff"
```

---

## Task 4: Bioluminescent theme + layout

**Files:**
- Modify: `apps/web/app/globals.css`
- Modify: `apps/web/app/layout.tsx`

**Step 1: Replace globals.css with axon bioluminescent theme**

Keep the existing shadcn/Plate CSS imports and theme bindings, but replace the `:root` / `.dark` color values with our bioluminescent palette. Since this is a dark-only dashboard, set `.dark` as the default.

Add the axon custom properties that the canvas code references, plus the font overrides for DM Sans / DM Mono.

**Step 2: Update layout.tsx**

- Replace Geist fonts with DM Sans + DM Mono via `next/font/google`
- Add `className="dark"` to `<html>` — forces dark mode
- Wrap `{children}` in `<Providers>`
- Add metadata: title "Axon", description "Neural RAG Pipeline"

**Step 3: Verify build**

```bash
pnpm build
```

**Step 4: Commit**

```bash
git add apps/web/app/globals.css apps/web/app/layout.tsx
git commit -m "feat(web): bioluminescent dark theme + DM Sans/Mono fonts"
```

---

## Task 5: WS indicator component

**Files:**
- Create: `apps/web/components/ws-indicator.tsx`

**Step 1: Create the component**

Small fixed-position badge (bottom-right) showing WS connection state. Uses the `useAxonWs()` hook for status. Three states: connected (green dot), reconnecting (yellow pulse), disconnected (red).

Port behavior from `app.js` lines 188-191 (`setWsStatus`). The stats handler also updates the label to show live container count + CPU (line 640-643).

**Step 2: Verify build**

```bash
pnpm build
```

**Step 3: Commit**

```bash
git add apps/web/components/ws-indicator.tsx
git commit -m "feat(web): WS connection indicator badge"
```

---

## Task 6: Omnibox component

**Files:**
- Create: `apps/web/components/omnibox.tsx`

**Step 1: Create the omnibox**

This is a pure shadcn/ui component — NO Plate editor. Composition:
- `<Input>` for URL/query text
- `<DropdownMenu>` for mode selector (16 modes from `MODES` constant)
- Execute button (click mode label or press Enter)
- Inline status area (spinner during processing, elapsed time on done)

Port behavior from `app.js`:
- Mode selection (lines 131-149): click option → set mode, auto-execute for NO_INPUT_MODES
- Execute (lines 650-712): send `{type: "execute"}` over WS, fire neural intensity to 1
- Cancel: on re-click during processing, send `{type: "cancel"}`
- Keyboard: Enter = execute, Escape = close dropdown

The component receives `send` from `useAxonWs()` and exposes `onExecute`/`onDone` callbacks so the parent can coordinate with ResultsPanel.

**Step 2: Verify build**

```bash
pnpm build
```

**Step 3: Commit**

```bash
git add apps/web/components/omnibox.tsx
git commit -m "feat(web): omnibox command input with mode selector"
```

---

## Task 7: Results panel with Plate editor for content

**Files:**
- Create: `apps/web/components/results-panel.tsx`
- Modify: `apps/web/components/editor/plate-editor.tsx`

**Step 1: Create the results panel**

Tabbed layout using shadcn `<Tabs>`:
- **Content** tab: renders command output via Plate editor (read-only)
- **Stats** tab: Docker stats grid + command options key-value table
- **Recent** tab: history table of recent runs

The Content tab hosts a `<PlateEditor>` instance that receives output lines and renders them as rich Plate nodes. When a command starts, the editor value is cleared. As `output` messages arrive:
- JSON with `markdown` field → parse to Plate nodes
- JSON with `answer` field → parse to Plate nodes
- JSON with `rank`/`snippet` → render as result card nodes
- Plain text → render as paragraph nodes
- Config objects → route to Stats tab

Port the output handling logic from `app.js` lines 284-444 (`handleOutput`, `renderJsonOutput`).

**Step 2: Modify PlateEditor to accept external value**

Update `plate-editor.tsx` to accept a `value` prop and `readOnly` prop, removing the hardcoded demo content.

**Step 3: Verify build**

```bash
pnpm build
```

**Step 4: Commit**

```bash
git add apps/web/components/results-panel.tsx apps/web/components/editor/plate-editor.tsx
git commit -m "feat(web): results panel with Plate editor content rendering"
```

---

## Task 8: Neural canvas component

**Files:**
- Create: `apps/web/components/neural-canvas.tsx`

**Step 1: Port neural.js to a React component**

Port `crates/web/static/neural.js` (~917 lines) into a `'use client'` component:
- `useRef<HTMLCanvasElement>` for the canvas element
- `useEffect` for animation loop lifecycle (requestAnimationFrame on mount, cancel on unmount)
- `useImperativeHandle` to expose `stimulate(stats)` and `setIntensity(target)` APIs
- Load via `next/dynamic` with `{ ssr: false }` in the layout

All classes (SimplexDrift, Dendrite, Axon, Neuron, SynapticConnection, ActionPotential, BackgroundParticle) stay as plain JS classes inside the file — NOT React components. The animation loop is pure imperative canvas code.

Apply the performance enhancements from the design doc:
1. Motion blur: `ctx.fillStyle = 'rgba(3,7,18,0.08)'; ctx.fillRect()` instead of `clearRect()`
2. Adaptive neuron count: `Math.min(80, navigator.hardwareConcurrency * 6)`
3. LOD by depth: skip spines for `depth < 0.3`
4. Connection LOD: skip every other frame when `neuronCount > 60`
5. GPU compositing: `will-change: transform` on canvas element

**Step 2: Verify build**

```bash
pnpm build
```

**Step 3: Commit**

```bash
git add apps/web/components/neural-canvas.tsx
git commit -m "feat(web): port neural canvas with performance enhancements"
```

---

## Task 9: Docker stats component

**Files:**
- Create: `apps/web/components/docker-stats.tsx`

**Step 1: Create the Docker stats renderer**

Subscribes to `stats` messages via `useAxonWs().subscribe()`. Renders:
- Aggregate stats grid: container count, total CPU, avg memory, net I/O rate
- Per-container detail rows: CPU%, MEM (used/limit), NET (tx/rx rates)

Port `renderStatsPane` from `app.js` lines 724-755.

Also bridges stats → neural canvas: maps aggregate CPU to neural intensity (lines 600-607), and per-container CPU/net to individual neuron stimulation (lines 610-637).

**Step 2: Verify build**

```bash
pnpm build
```

**Step 3: Commit**

```bash
git add apps/web/components/docker-stats.tsx
git commit -m "feat(web): Docker stats grid with neural canvas bridge"
```

---

## Task 10: Dashboard page assembly

**Files:**
- Modify: `apps/web/app/page.tsx`
- Modify: `apps/web/app/layout.tsx`

**Step 1: Wire up the dashboard**

Replace the default Next.js page with the full dashboard layout:

```
<NeuralCanvas />          (fixed background, z-0)
<WsIndicator />           (fixed bottom-right, z-10)
<div className="logo">    (fixed top-left, z-10)
<main>                    (relative, z-1, centered container)
  <div className="card">
    <Omnibox />
    <ResultsPanel />
  </div>
</main>
```

Wire the message flow:
1. `useAxonWs().subscribe()` in page.tsx dispatches messages to child components
2. Omnibox `onExecute` → clears results, fires neural intensity
3. `output`/`log`/`done`/`error` → ResultsPanel
4. `stats` → DockerStats + NeuralCanvas
5. `done`/`error` → decay neural intensity, update omnibox status

**Step 2: Update layout.tsx to use dynamic import for NeuralCanvas**

```typescript
import dynamic from 'next/dynamic'
const NeuralCanvas = dynamic(() => import('@/components/neural-canvas'), { ssr: false })
```

**Step 3: Verify build**

```bash
pnpm build
```

**Step 4: Verify dev server connects to axum backend**

```bash
# Terminal 1: start axum backend
cd /home/jmagar/workspace/axon_rust && ./scripts/axon serve

# Terminal 2: start Next.js dev
cd apps/web && pnpm dev
```

Open `http://localhost:3000` — verify:
- Neural canvas animates
- WS indicator shows CONNECTED
- Mode selector works
- Execute a command (e.g., `stats`) and see results

**Step 5: Commit**

```bash
git add apps/web/app/page.tsx apps/web/app/layout.tsx
git commit -m "feat(web): assemble dashboard with all components"
```

---

## Task 11: next.config.ts — WS proxy rewrite

**Files:**
- Modify: `apps/web/next.config.ts`

**Step 1: Add WS rewrite for dev mode**

```typescript
import path from 'node:path'
import type { NextConfig } from 'next'

const axonPort = process.env.NEXT_PUBLIC_AXON_PORT || '3333'

const nextConfig: NextConfig = {
  output: 'standalone',
  turbopack: {
    root: path.resolve(__dirname, '..'),
  },
  async rewrites() {
    return [
      {
        source: '/ws',
        destination: `http://localhost:${axonPort}/ws`,
      },
    ]
  },
}

export default nextConfig
```

**Step 2: Verify build**

```bash
pnpm build
```

**Step 3: Commit**

```bash
git add apps/web/next.config.ts
git commit -m "feat(web): add WS proxy rewrite + standalone output"
```

---

## Task 12: Markdown parser utility

**Files:**
- Create: `apps/web/lib/markdown.ts`

**Step 1: Port parseMarkdown from app.js**

Port `parseMarkdown()` from `app.js` lines 807-926. This is the custom markdown→HTML parser used for rendering output snippets. It handles: headings, code blocks, tables, blockquotes, lists, inline formatting (bold, italic, code, links), horizontal rules.

We need this because command output arrives as raw markdown strings that get fed into the Plate editor as HTML, then converted to Plate nodes.

Alternatively, since we have `@platejs/markdown` installed, we can use `deserializeMd()` to convert markdown strings directly to Plate nodes — which would be cleaner than the custom parser. Investigate which approach works better.

**Step 2: Verify build**

```bash
pnpm build
```

**Step 3: Commit**

```bash
git add apps/web/lib/markdown.ts
git commit -m "feat(web): add markdown parsing utility"
```

---

## Task 13: Final polish + cleanup

**Files:**
- Delete: `apps/web/app/page.tsx` (the default Next.js page — replaced in Task 10)
- Modify: `apps/web/app/editor/page.tsx` (optional: keep as a standalone editor demo route)
- Run: `pnpm lint` (Biome check)

**Step 1: Run Biome lint + format**

```bash
cd apps/web
pnpm lint
pnpm format
```

**Step 2: Final build verification**

```bash
pnpm build
```

Expected: clean build, routes: `/`, `/editor`, `/_not-found`

**Step 3: Commit**

```bash
git add -A apps/web/
git commit -m "chore(web): lint, format, cleanup"
```

---

## Dependency Graph

```
Task 1 (shadcn primitives) ──┐
Task 2 (WS types) ───────────┤
                              ├─→ Task 3 (WS hook + providers)
                              │       │
Task 4 (theme + layout) ─────┤       ├─→ Task 5 (WS indicator)
                              │       ├─→ Task 6 (Omnibox)
                              │       ├─→ Task 7 (Results panel + Plate)
                              │       ├─→ Task 8 (Neural canvas)
                              │       └─→ Task 9 (Docker stats)
                              │               │
                              └───────────────┴─→ Task 10 (Dashboard assembly)
                                                      │
                                                  Task 11 (WS proxy config)
                                                      │
                                                  Task 12 (Markdown parser)
                                                      │
                                                  Task 13 (Polish + cleanup)
```

Tasks 1, 2, 4 are independent and can run in parallel.
Tasks 5-9 depend on Task 3 (WS hook) but are independent of each other — can run in parallel.
Tasks 10-13 are sequential.
