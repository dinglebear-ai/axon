# Cortex Mission Control Makeover Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the current tabbed Cortex pane with a single, high-signal Mission Control dashboard that unifies health, throughput, queue state, and corpus intelligence in one view.

**Architecture:** Introduce one aggregated backend route (`/api/cortex/overview`) that composes data from existing Cortex routes plus jobs. Render a new Tailwind-first “Mission Control” pane using composable sections (hero, live health strip, queue radar, corpus map, and action rail) with progressive loading and resilient fallback states. Keep existing API routes and legacy widgets during migration, then switch `AxonCortexPane` to the new layout and retire the old tab shell.

**Tech Stack:** Next.js 16, React 19, TypeScript, Tailwind CSS v4, `lucide-react`, existing `apiFetch`, Vitest + Testing Library.

---

## Design Direction (Locked)

- **Aesthetic:** “Midnight Command Deck” (editorial + tactical instrumentation)
- **Typography:** Keep current app fonts for consistency, but introduce explicit display hierarchy and tighter mono metric styling.
- **Color System:** Preserve existing token family (`--axon-*`, `--surface-*`) and extend with mission-control specific semantic tokens in `globals.css`.
- **Layout:** Single scroll narrative; no top-level Cortex tabs. Sections are stacked with asymmetric two-column composition on desktop and progressive collapse on mobile.
- **Motion:** Use purposeful staggered reveal + subtle pulse states only where data freshness matters.
- **Information Priority:**
  1) “Can I trust the system right now?”
  2) “What is moving or stuck?”
  3) “Where is corpus growth concentrated?”
  4) “What should I do next?”

References: `@frontend-design`, `@styling-with-tailwind`

---

### Task 1: Add Aggregated Cortex Overview API Contract

**Files:**
- Create: `apps/web/app/api/cortex/overview/route.ts`
- Create: `apps/web/lib/cortex/overview-normalize.ts`
- Create: `apps/web/__tests__/api/cortex-overview-route.test.ts`
- Modify: `apps/web/lib/result-types.ts`

**Step 1: Write the failing test**

```ts
// apps/web/__tests__/api/cortex-overview-route.test.ts
import { describe, expect, it, vi } from 'vitest'

vi.mock('@/lib/axon-ws-exec', () => ({
  runAxonCommandWs: vi.fn(),
}))

describe('GET /api/cortex/overview', () => {
  it('returns unified payload with health, queue, corpus, and jobs slices', async () => {
    const mod = await import('@/app/api/cortex/overview/route')
    const res = await mod.GET()
    expect(res.status).toBe(200)
    const body = await res.json()
    expect(body.ok).toBe(true)
    expect(body.data).toHaveProperty('health')
    expect(body.data).toHaveProperty('queue')
    expect(body.data).toHaveProperty('corpus')
    expect(body.data).toHaveProperty('jobs')
  })
})
```

**Step 2: Run test to verify it fails**

Run: `pnpm --dir apps/web test apps/web/__tests__/api/cortex-overview-route.test.ts`
Expected: FAIL with module/file not found for `/api/cortex/overview/route`.

**Step 3: Write minimal implementation**

```ts
// apps/web/app/api/cortex/overview/route.ts
import { NextResponse } from 'next/server'
import { runAxonCommandWs } from '@/lib/axon-ws-exec'
import { apiError } from '@/lib/server/api-error'
import { getJobsPgPool } from '@/lib/server/pg-pool'
import { toCortexOverview } from '@/lib/cortex/overview-normalize'

export const dynamic = 'force-dynamic'

export async function GET() {
  try {
    const [status, doctor, stats] = await Promise.all([
      runAxonCommandWs('status', 30_000),
      runAxonCommandWs('doctor', 30_000),
      runAxonCommandWs('stats', 30_000),
    ])

    const jobsRows = await getJobsPgPool().query(
      `SELECT id, status, created_at, started_at, finished_at
       FROM axon_crawl_jobs
       ORDER BY created_at DESC
       LIMIT 50`,
    )

    return NextResponse.json({
      ok: true,
      data: toCortexOverview({ status, doctor, stats, jobsRows: jobsRows.rows }),
    })
  } catch (err) {
    console.error('[cortex/overview] failed', err)
    return apiError(500, 'Failed to build Cortex overview', { code: 'cortex_overview' })
  }
}
```

**Step 4: Run test to verify it passes**

Run: `pnpm --dir apps/web test apps/web/__tests__/api/cortex-overview-route.test.ts`
Expected: PASS

**Step 5: Commit**

```bash
git add apps/web/app/api/cortex/overview/route.ts apps/web/lib/cortex/overview-normalize.ts apps/web/lib/result-types.ts apps/web/__tests__/api/cortex-overview-route.test.ts
git commit -m "feat(web): add cortex overview aggregation route"
```

---

### Task 2: Create Mission Control Data Model + Normalizers

**Files:**
- Create: `apps/web/lib/cortex/mission-control-model.ts`
- Modify: `apps/web/lib/cortex/overview-normalize.ts`
- Create: `apps/web/__tests__/cortex-mission-control-model.test.ts`

**Step 1: Write the failing test**

```ts
// apps/web/__tests__/cortex-mission-control-model.test.ts
import { describe, expect, it } from 'vitest'
import { buildMissionControlModel } from '@/lib/cortex/mission-control-model'

it('derives top KPIs and queue pressure correctly', () => {
  const model = buildMissionControlModel({
    health: { allOk: false, unhealthyServices: 2 },
    queue: { running: 4, pending: 11, failed: 3, completed: 97 },
    corpus: { points: 1000, vectors: 1000, domains: 12, topDomains: [] },
    jobs: [],
  })

  expect(model.kpis.queuePressure).toBe('high')
  expect(model.kpis.reliability).toBe('degraded')
})
```

**Step 2: Run test to verify it fails**

Run: `pnpm --dir apps/web test apps/web/__tests__/cortex-mission-control-model.test.ts`
Expected: FAIL because model file/export does not exist.

**Step 3: Write minimal implementation**

```ts
// apps/web/lib/cortex/mission-control-model.ts
export type QueuePressure = 'low' | 'medium' | 'high'
export type Reliability = 'healthy' | 'degraded'

export function buildMissionControlModel(input: any) {
  const pending = Number(input?.queue?.pending ?? 0)
  const running = Number(input?.queue?.running ?? 0)
  const unhealthy = Number(input?.health?.unhealthyServices ?? 0)

  const queuePressure: QueuePressure = pending > 10 ? 'high' : pending > 3 ? 'medium' : 'low'
  const reliability: Reliability = unhealthy > 0 ? 'degraded' : 'healthy'

  return {
    kpis: {
      queuePressure,
      reliability,
      activeWork: pending + running,
    },
    sections: input,
  }
}
```

**Step 4: Run test to verify it passes**

Run: `pnpm --dir apps/web test apps/web/__tests__/cortex-mission-control-model.test.ts`
Expected: PASS

**Step 5: Commit**

```bash
git add apps/web/lib/cortex/mission-control-model.ts apps/web/lib/cortex/overview-normalize.ts apps/web/__tests__/cortex-mission-control-model.test.ts
git commit -m "feat(web): add mission-control view model normalizer"
```

---

### Task 3: Build New Mission Control Layout Component

**Files:**
- Create: `apps/web/components/cortex/mission-control-pane.tsx`
- Create: `apps/web/components/cortex/mission-control/hero-kpis.tsx`
- Create: `apps/web/components/cortex/mission-control/health-strip.tsx`
- Create: `apps/web/components/cortex/mission-control/queue-radar.tsx`
- Create: `apps/web/components/cortex/mission-control/corpus-map.tsx`
- Create: `apps/web/components/cortex/mission-control/action-rail.tsx`
- Create: `apps/web/__tests__/cortex-mission-control-pane.test.tsx`

**Step 1: Write the failing test**

```tsx
// apps/web/__tests__/cortex-mission-control-pane.test.tsx
import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { MissionControlPane } from '@/components/cortex/mission-control-pane'

describe('MissionControlPane', () => {
  it('renders core sections', () => {
    render(<MissionControlPane />)
    expect(screen.getByText(/Mission Control/i)).toBeInTheDocument()
    expect(screen.getByText(/System Health/i)).toBeInTheDocument()
    expect(screen.getByText(/Queue Pressure/i)).toBeInTheDocument()
    expect(screen.getByText(/Corpus Map/i)).toBeInTheDocument()
  })
})
```

**Step 2: Run test to verify it fails**

Run: `pnpm --dir apps/web test apps/web/__tests__/cortex-mission-control-pane.test.tsx`
Expected: FAIL because component does not exist.

**Step 3: Write minimal implementation**

```tsx
// apps/web/components/cortex/mission-control-pane.tsx
'use client'

import { HeroKpis } from './mission-control/hero-kpis'
import { HealthStrip } from './mission-control/health-strip'
import { QueueRadar } from './mission-control/queue-radar'
import { CorpusMap } from './mission-control/corpus-map'
import { ActionRail } from './mission-control/action-rail'

export function MissionControlPane() {
  return (
    <section className="axon-mission-control mx-auto w-full max-w-7xl p-4 md:p-6">
      <header className="mb-4">
        <h1 className="text-2xl font-semibold tracking-tight text-[var(--text-primary)]">
          Mission Control
        </h1>
      </header>

      <div className="grid gap-4 lg:grid-cols-[1.2fr_0.8fr]">
        <div className="space-y-4">
          <HeroKpis />
          <HealthStrip />
          <QueueRadar />
          <CorpusMap />
        </div>
        <ActionRail />
      </div>
    </section>
  )
}
```

**Step 4: Run test to verify it passes**

Run: `pnpm --dir apps/web test apps/web/__tests__/cortex-mission-control-pane.test.tsx`
Expected: PASS

**Step 5: Commit**

```bash
git add apps/web/components/cortex/mission-control-pane.tsx apps/web/components/cortex/mission-control/*.tsx apps/web/__tests__/cortex-mission-control-pane.test.tsx
git commit -m "feat(web): introduce cortex mission control layout"
```

---

### Task 4: Replace Tabbed Cortex Shell With Mission Control

**Files:**
- Modify: `apps/web/components/shell/axon-cortex-pane.tsx`
- Create: `apps/web/__tests__/shell/axon-cortex-pane-redesign.test.tsx`

**Step 1: Write the failing test**

```tsx
// apps/web/__tests__/shell/axon-cortex-pane-redesign.test.tsx
import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { AxonCortexPane } from '@/components/shell/axon-cortex-pane'

it('renders mission control and no legacy tab bar', () => {
  render(<AxonCortexPane />)
  expect(screen.getByText(/Mission Control/i)).toBeInTheDocument()
  expect(screen.queryByRole('button', { name: /Status/i })).toBeNull()
  expect(screen.queryByRole('button', { name: /Doctor/i })).toBeNull()
})
```

**Step 2: Run test to verify it fails**

Run: `pnpm --dir apps/web test apps/web/__tests__/shell/axon-cortex-pane-redesign.test.tsx`
Expected: FAIL because old tab buttons still render.

**Step 3: Write minimal implementation**

```tsx
// apps/web/components/shell/axon-cortex-pane.tsx
'use client'

import { memo } from 'react'
import { MissionControlPane } from '@/components/cortex/mission-control-pane'

export const AxonCortexPane = memo(function AxonCortexPane() {
  return (
    <div className="flex h-full flex-col overflow-auto">
      <MissionControlPane />
    </div>
  )
})
```

**Step 4: Run test to verify it passes**

Run: `pnpm --dir apps/web test apps/web/__tests__/shell/axon-cortex-pane-redesign.test.tsx`
Expected: PASS

**Step 5: Commit**

```bash
git add apps/web/components/shell/axon-cortex-pane.tsx apps/web/__tests__/shell/axon-cortex-pane-redesign.test.tsx
git commit -m "refactor(web): replace cortex tabs with mission control pane"
```

---

### Task 5: Tailwind Styling Pass + Token Additions

**Files:**
- Modify: `apps/web/app/globals.css`
- Modify: `apps/web/app/density-high.css`
- Modify: `apps/web/components/cortex/mission-control-pane.tsx`
- Modify: `apps/web/components/cortex/mission-control/*.tsx`

**Step 1: Write the failing test (visual contract snapshot)**

```tsx
// apps/web/__tests__/cortex-mission-control-pane.test.tsx
it('matches mission-control desktop shell class contract', () => {
  const { container } = render(<MissionControlPane />)
  expect(container.querySelector('.axon-mission-control')).toBeTruthy()
  expect(container.querySelector('.axon-mission-grid')).toBeTruthy()
})
```

**Step 2: Run test to verify it fails**

Run: `pnpm --dir apps/web test apps/web/__tests__/cortex-mission-control-pane.test.tsx`
Expected: FAIL until new class hooks are added.

**Step 3: Write minimal implementation**

```css
/* apps/web/app/globals.css */
:root {
  --mc-hero-bg: linear-gradient(135deg, rgba(18, 34, 68, 0.88), rgba(10, 18, 35, 0.9));
  --mc-accent-cyan: #66d9ff;
  --mc-accent-gold: #ffd166;
  --mc-accent-coral: #ff8a7a;
}

.axon-mission-control {
  container-type: inline-size;
}

.axon-mission-card {
  @apply rounded-2xl border p-4 md:p-5;
  border-color: var(--border-subtle);
  background: var(--surface-base);
  box-shadow: var(--shadow-md);
}
```

```tsx
// mission-control-pane.tsx (class hooks)
<div className="axon-mission-control ...">
  <div className="axon-mission-grid grid gap-4 lg:grid-cols-[1.2fr_0.8fr]">
```

**Step 4: Run tests + lint to verify**

Run: `pnpm --dir apps/web test apps/web/__tests__/cortex-mission-control-pane.test.tsx`
Expected: PASS

Run: `pnpm --dir apps/web lint`
Expected: PASS (no Biome errors)

**Step 5: Commit**

```bash
git add apps/web/app/globals.css apps/web/app/density-high.css apps/web/components/cortex/mission-control-pane.tsx apps/web/components/cortex/mission-control/*.tsx apps/web/__tests__/cortex-mission-control-pane.test.tsx
git commit -m "style(web): apply mission control visual system and responsive density tuning"
```

---

### Task 6: Integrate Live Data + Action Model (Unified UX)

**Files:**
- Modify: `apps/web/components/cortex/mission-control-pane.tsx`
- Modify: `apps/web/components/cortex/mission-control/action-rail.tsx`
- Modify: `apps/web/components/cortex/mission-control/queue-radar.tsx`
- Modify: `apps/web/components/cortex/mission-control/corpus-map.tsx`
- Create: `apps/web/__tests__/cortex-mission-control-data.test.tsx`

**Step 1: Write the failing test**

```tsx
// apps/web/__tests__/cortex-mission-control-data.test.tsx
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

vi.mock('@/lib/api-fetch', () => ({
  apiFetch: vi.fn(async () => ({
    json: async () => ({
      ok: true,
      data: {
        health: { allOk: true, unhealthyServices: 0 },
        queue: { running: 2, pending: 1, failed: 0, completed: 10 },
        corpus: { vectors: 500, topDomains: [{ domain: 'docs.rs', vectors: 220 }] },
        jobs: [],
      },
    }),
  })),
}))

it('renders live kpis from overview payload', async () => {
  render(<MissionControlPane />)
  expect(await screen.findByText(/docs.rs/i)).toBeInTheDocument()
  expect(await screen.findByText(/500/i)).toBeInTheDocument()
})
```

**Step 2: Run test to verify it fails**

Run: `pnpm --dir apps/web test apps/web/__tests__/cortex-mission-control-data.test.tsx`
Expected: FAIL until component fetches `/api/cortex/overview`.

**Step 3: Write minimal implementation**

```tsx
// mission-control-pane.tsx (core load path)
const [state, setState] = useState<{ loading: boolean; error: string | null; data: any }>({
  loading: true,
  error: null,
  data: null,
})

useEffect(() => {
  const controller = new AbortController()
  void apiFetch('/api/cortex/overview', { signal: controller.signal })
    .then((r) => r.json())
    .then((json) => {
      if (!json.ok) throw new Error(json.error ?? 'Overview failed')
      setState({ loading: false, error: null, data: json.data })
    })
    .catch((err) => {
      if (err?.name === 'AbortError') return
      setState({ loading: false, error: String(err), data: null })
    })

  return () => controller.abort()
}, [])
```

**Step 4: Run test to verify it passes**

Run: `pnpm --dir apps/web test apps/web/__tests__/cortex-mission-control-data.test.tsx`
Expected: PASS

Run: `pnpm --dir apps/web test apps/web/__tests__/cortex-mission-control-pane.test.tsx apps/web/__tests__/shell/axon-cortex-pane-redesign.test.tsx`
Expected: PASS

**Step 5: Commit**

```bash
git add apps/web/components/cortex/mission-control-pane.tsx apps/web/components/cortex/mission-control/action-rail.tsx apps/web/components/cortex/mission-control/queue-radar.tsx apps/web/components/cortex/mission-control/corpus-map.tsx apps/web/__tests__/cortex-mission-control-data.test.tsx
git commit -m "feat(web): wire mission control to unified cortex overview data"
```

---

### Task 7: Remove Legacy Cortex Tab Widgets (Post-Switch Cleanup)

**Files:**
- Delete: `apps/web/components/cortex/status-dashboard.tsx`
- Delete: `apps/web/components/cortex/doctor-dashboard.tsx`
- Delete: `apps/web/components/cortex/stats-dashboard.tsx`
- Delete: `apps/web/components/cortex/sources-dashboard.tsx`
- Delete: `apps/web/components/cortex/domains-dashboard.tsx`
- Evaluate delete/retain: `apps/web/components/jobs/jobs-dashboard.tsx` (retain only if reused outside Cortex)
- Modify: `apps/web/README.md`

**Step 1: Write failing safety test**

```ts
// apps/web/__tests__/shell/axon-cortex-pane-redesign.test.tsx
it('does not import legacy cortex tab widgets', async () => {
  const source = await import('fs').then((fs) =>
    fs.readFileSync('apps/web/components/shell/axon-cortex-pane.tsx', 'utf8'),
  )
  expect(source).not.toMatch('status-dashboard')
  expect(source).not.toMatch('doctor-dashboard')
})
```

**Step 2: Run test to verify current state**

Run: `pnpm --dir apps/web test apps/web/__tests__/shell/axon-cortex-pane-redesign.test.tsx`
Expected: PASS before deletion, still PASS after deletion.

**Step 3: Remove legacy components and dead imports**

- Delete files only after confirming no remaining imports via:

```bash
rg -n "status-dashboard|doctor-dashboard|stats-dashboard|sources-dashboard|domains-dashboard" apps/web
```

**Step 4: Run verification**

Run: `pnpm --dir apps/web test`
Expected: PASS

Run: `pnpm --dir apps/web build`
Expected: PASS

**Step 5: Commit**

```bash
git add -A apps/web/components/cortex apps/web/components/shell/axon-cortex-pane.tsx apps/web/README.md
git commit -m "refactor(web): retire legacy cortex tab dashboards after mission control launch"
```

---

### Task 8: Final QA, Performance, and Accessibility Gate

**Files:**
- Modify as needed from findings in prior tasks.
- Create: `docs/reports/2026-03-12-cortex-mission-control-qa.md`

**Step 1: Run full verification suite**

Run: `pnpm --dir apps/web lint`
Expected: PASS

Run: `pnpm --dir apps/web test`
Expected: PASS

Run: `pnpm --dir apps/web build`
Expected: PASS

**Step 2: Manual viewport checks**

- 320px width: no horizontal overflow, action rail collapses below main.
- 768px width: two-column sections collapse gracefully.
- 1440px width: max width respected, no dead whitespace bands.

**Step 3: Keyboard and focus checks**

- Tab through quick actions and filters; visible focus ring on all controls.
- Ensure status chips and charts have screen-reader labels where needed.

**Step 4: Capture QA report**

Document pass/fail and any deferred follow-ups in:
`docs/reports/2026-03-12-cortex-mission-control-qa.md`

**Step 5: Commit**

```bash
git add docs/reports/2026-03-12-cortex-mission-control-qa.md apps/web
git commit -m "chore(web): finalize cortex mission control QA and accessibility checks"
```

---

## Implementation Notes

- Keep existing `/api/cortex/status`, `/doctor`, `/stats`, `/sources`, `/domains` routes initially for backwards compatibility; deprecate in a separate cleanup PR if external consumers exist.
- Prefer deriving “jobs by type/status” from `/api/jobs` query path or shared SQL helpers to avoid duplicating brittle SQL logic.
- If build size grows materially from visualization code, lazy-load heavy sections with `next/dynamic`.

## Verification Checklist (Done Before Merge)

- `pnpm --dir apps/web lint`
- `pnpm --dir apps/web test`
- `pnpm --dir apps/web build`
- Manual test in shell right-pane (`Cortex`) on desktop + mobile widths
- Confirm no regression in adjacent right-pane modes (`MCP`, `Logs`, `Terminal`)

