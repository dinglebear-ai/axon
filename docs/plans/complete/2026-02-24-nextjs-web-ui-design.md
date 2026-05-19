# Next.js Web UI Design — apps/web

**Date:** 2026-02-24
**Branch:** fix-crawl
**Status:** Approved — Canvas-in-React approach

## Goal

Replace the vanilla HTML/CSS/JS frontend at `crates/web/static/` with a Next.js 16 + shadcn/ui app at `apps/web/`. The axum WebSocket backend (`crates/web.rs`, `crates/web/execute.rs`, `crates/web/docker_stats.rs`) stays as-is — the Next.js app connects to it.

## Architecture

```
apps/web/                          # Next.js 16 (App Router, Turbopack)
├── app/
│   ├── layout.tsx                 # Root layout: html/body, fonts, providers, neural canvas
│   ├── page.tsx                   # Dashboard: omnibox + results panel
│   ├── globals.css                # Tailwind v4 CSS-first config + axon custom properties
│   └── providers.tsx              # Client provider tree: WS context, neural intensity
├── components/
│   ├── neural-canvas.tsx          # Port of neural.js — 'use client', useRef canvas, ssr: false
│   ├── omnibox.tsx                # Command input + mode dropdown + inline status
│   ├── results-panel.tsx          # Tabbed results: Content | Stats | Recent
│   ├── docker-stats.tsx           # Real-time container stats grid + log stream
│   ├── ws-indicator.tsx           # Bottom-right connection badge
│   └── ui/                        # shadcn/ui primitives (button, dropdown-menu, tabs, etc.)
├── hooks/
│   ├── use-axon-ws.ts             # WS connection, reconnect backoff, message dispatch
│   └── use-neural-intensity.ts    # Shared ref for canvas ↔ UI coupling
├── lib/
│   ├── ws-protocol.ts             # TS types: WsClientMsg, WsServerMsg, DockerStats, etc.
│   ├── markdown.ts                # Port of parseMarkdown() from app.js
│   └── utils.ts                   # cn() helper from shadcn/ui
├── next.config.ts                 # Rewrites /ws → axum, env vars, output: 'standalone'
├── package.json
├── tsconfig.json
├── biome.json                     # Biome v2 config (replaces ESLint)
└── components.json                # shadcn/ui registry
```

## Data Flow

```
axum WS server (:3333/ws)
    ↓
    ├─ dev: next.config.ts rewrite /ws → ws://localhost:3333/ws
    ├─ prod: NEXT_PUBLIC_AXON_WS_URL env var (direct connect)
    ↓
useAxonWs() hook (context provider)
    ↓
    ├── <NeuralCanvas />     reads: intensity + per-container stats → stimulates neurons
    ├── <Omnibox />          sends: {type:"execute"} / {type:"cancel"}, reads: status
    ├── <ResultsPanel />     reads: output/log/done/error message streams
    └── <DockerStats />      reads: stats messages → renders grid + per-container details
```

## Neural Canvas Strategy: Canvas-in-React

Port `neural.js` (~920 lines) into a single `'use client'` React component:

- `useRef<HTMLCanvasElement>` for the canvas element
- `useEffect` for the animation loop lifecycle (start on mount, cleanup on unmount)
- `useCallback` for the `stimulate(stats)` API — called from the WS provider
- Loaded via `next/dynamic` with `{ ssr: false }` — canvas APIs don't exist server-side
- The animation loop is pure imperative canvas — React never re-renders the hot path
- All classes (Neuron, Axon, Dendrite, etc.) stay as plain JS classes, not React components

### Enhancements (Free Performance)

1. **Motion blur** — Replace `ctx.clearRect()` with `ctx.fillStyle = 'rgba(3,7,18,0.08)'; ctx.fillRect()` for trailing glow effect. One line change, much smoother perceived motion.
2. **Adaptive neuron count** — `Math.min(80, navigator.hardwareConcurrency * 6)` scales to hardware. High-end: 80 neurons. Low-end: 30. Same code, auto-tuned.
3. **LOD by depth** — Neurons with `depth < 0.3` skip spine rendering and use simpler glow (fewer radial gradients per frame). Already have depth field — just extend the draw() method.
4. **Connection LOD** — Skip connection rendering every other frame when `neuronCount > 60`. Connections are the most expensive part (O(n²) distance checks).
5. **GPU compositing** — `will-change: transform` on the canvas element forces GPU layer promotion.

## Color System

CSS custom properties in `globals.css`, referenced by both Tailwind utilities and canvas code:

```css
:root {
  --axon-bg: 3 7 18;                /* #030712 */
  --axon-core: 210 235 255;         /* white-hot center */
  --axon-bright: 50 160 255;        /* electric blue */
  --axon-mid: 15 90 210;            /* medium blue */
  --axon-dim: 8 45 140;             /* deep blue */
  --axon-faint: 4 20 70;            /* barely visible */
  --axon-pink: 255 135 175;         /* #ff87af — accent/firing */
  --axon-text: 232 244 248;         /* #e8f4f8 — primary text */
  --axon-muted: 135 135 175;        /* #8787af — secondary text */
  --axon-surface: 10 18 35;         /* card backgrounds */
}
```

Fonts: DM Sans (body) + DM Mono (code) via `next/font/google`.

## WebSocket Protocol (unchanged)

Client → Server:
- `{type: "execute", mode: string, input: string, flags: object}`
- `{type: "cancel", id: string}`

Server → Client:
- `{type: "output", line: string}` — stdout JSON data
- `{type: "log", line: string}` — stderr progress
- `{type: "done", exit_code: number, elapsed_ms: number}`
- `{type: "error", message: string, elapsed_ms?: number}`
- `{type: "stats", aggregate: {...}, containers: {...}, container_count: number}`

## WS Connection Strategy

Port existing reconnect logic from `app.js`:
- Base backoff: 1s, max: 30s, exponential
- Auto-reconnect on close
- Status indicator: connected / reconnecting / disconnected

Dev: `next.config.ts` rewrites `/ws` to `ws://localhost:3333/ws`
Prod: `NEXT_PUBLIC_AXON_WS_URL` env var for direct connection

## shadcn/ui Components Needed

- `button` — action buttons
- `dropdown-menu` — mode selector
- `tabs` — Content | Stats | Recent
- `input` — omnibox text input
- `scroll-area` — results body scrolling
- `badge` — status indicators

All styled to match bioluminescent palette via CSS custom properties.

## Tooling

- **pnpm** — package manager
- **Tailwind v4** — CSS-first config (no `tailwind.config.js`)
- **Biome v2** — linting + formatting (Next.js 16 supports it natively via `create-next-app`)
- **TypeScript 5.9+** — strict mode
- **Turbopack** — default bundler in Next.js 16

## Environment Variables

```bash
# Dev: proxied via next.config.ts rewrites
# Prod: direct connection to axum backend
NEXT_PUBLIC_AXON_WS_URL=           # e.g., ws://localhost:3333/ws (prod only)
NEXT_PUBLIC_AXON_PORT=3333         # axum serve port (for dev rewrite target)
```

## What Changes in the Rust Codebase

Nothing. The axum WS backend is untouched. The `crates/web/static/` files remain for now as the existing fallback UI. Once the Next.js app reaches feature parity, we can deprecate the static assets and have `axon serve` proxy to the Next.js app instead.

## Key Decisions

1. **SPA mode** — This is a client-heavy dashboard. All pages are `'use client'`. No SSR for the main dashboard. The canvas, WS connection, and Docker stats are all browser-only.
2. **No API routes** — The Next.js app has zero server-side API routes. All data flows through the existing axum WebSocket.
3. **Standalone output** — `output: 'standalone'` for self-hosted deployment (no Vercel).
4. **Biome over ESLint** — Faster, aligns with project standards.
5. **`next/dynamic` for canvas** — `ssr: false` prevents hydration mismatch from canvas APIs.
