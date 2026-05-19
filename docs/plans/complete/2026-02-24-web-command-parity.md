# Web UI Command Parity Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make every Axon CLI command render meaningful output in the web UI — not just scrape/crawl.

**Architecture:** The backend executor stops silently draining stdout and instead streams it line-by-line as `stdout_json` or `stdout_line` messages. The frontend receives these, normalizes them by `renderIntent` from the existing `AXON_COMMAND_SPECS`, and dispatches to one of five renderer components. Scrape/crawl behavior is unchanged.

**Tech Stack:** Rust (axum/tokio) for backend WS changes, React 19 + TypeScript + TailwindCSS v4 for frontend renderers.

---

## Situation Summary

| What | Current State | Target State |
|------|--------------|--------------|
| Backend stdout | Silently drained (line 206-210 of `execute.rs`) | Streamed as `stdout_line` / `stdout_json` messages |
| Frontend output types | `output` exists in types but is never sent or handled | `stdout_line` + `stdout_json` handled, accumulated, dispatched to renderers |
| Mode picker | 17 modes in `MODES` array | All 24 backend-allowed modes (add `retrieve`, `debug`, `dedupe`, `screenshot`, `github`, `reddit`, `youtube`) |
| Flags | Always `flags: {}` — no UI | High-value flags exposed per-mode via command spec |
| Cancel | Sends local counter, not job UUID | Sends actual job ID from backend `done`/`error` response |
| Recent runs | `target` always empty, `lines` always 0 | Populated from execution context |
| `axon-command-map.ts` | Dead code — not imported anywhere | Single source of truth for command dispatch |
| Backend `--json` flag | Never passed | Always passed for non-file-producing modes |
| Allowed flags | 10 hardcoded | Expanded to cover command-specific options |

## Phase A: Core Unlock (Backend stdout streaming + frontend state)

This is the single highest-leverage change. Once stdout flows to the browser, every `--json` command becomes visible.

---

### Task 1: Stream stdout instead of draining it

**Files:**
- Modify: `crates/web/execute.rs:200-210`

**What:** Replace the silent stdout drain with a line-by-line stream that sends each line as a WS message. Attempt JSON parse on each line — if it succeeds, send `stdout_json`; otherwise send `stdout_line`.

**Step 1: Replace stdout drain with streaming**

In `execute.rs`, replace the stdout task (lines 205-210):

```rust
// OLD: Drain stdout silently (prevent pipe buffer deadlock)
let stdout_task = tokio::spawn(async move {
    let Some(stdout) = stdout else { return };
    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(_)) = lines.next_line().await {}
});
```

With:

```rust
let stdout_tx = tx.clone();
let stdout_task = tokio::spawn(async move {
    let Some(stdout) = stdout else { return };
    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(raw)) = lines.next_line().await {
        let clean = strip_ansi(&raw);
        if clean.trim().is_empty() {
            continue;
        }
        // Try JSON parse — send typed message if valid
        let msg = if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&clean) {
            json!({"type": "stdout_json", "data": parsed})
        } else {
            json!({"type": "stdout_line", "line": clean})
        };
        if stdout_tx.send(msg.to_string()).await.is_err() {
            break;
        }
    }
});
```

**Step 2: Force `--json` for non-file-producing modes**

After the `--wait true` injection (line 148-151), add:

```rust
// Force JSON output for commands that don't produce files.
// Scrape and crawl write output files that we read post-process;
// all other commands should emit structured JSON to stdout.
const FILE_PRODUCING_MODES: &[&str] = &["scrape", "crawl", "screenshot"];
if !FILE_PRODUCING_MODES.contains(&mode) {
    args.push("--json".to_string());
}
```

**Step 3: Verify it compiles**

Run: `cargo check --lib 2>&1 | tail -20`
Expected: clean compile (no new fields, no type changes)

**Step 4: Commit**

```bash
git add crates/web/execute.rs
git commit -m "feat(web): stream stdout as stdout_json/stdout_line instead of draining"
```

---

### Task 2: Expand ALLOWED_FLAGS for command-specific options

**Files:**
- Modify: `crates/web/execute.rs:37-48`

**What:** Add the command-specific flags from `AXON_COMMAND_OPTIONS` + high-value global flags that the UI will need.

**Step 1: Expand the flag whitelist**

Replace the `ALLOWED_FLAGS` const:

```rust
const ALLOWED_FLAGS: &[(&str, &str)] = &[
    // Global flags (high-value subset for web UI)
    ("max_pages",           "--max-pages"),
    ("max_depth",           "--max-depth"),
    ("limit",               "--limit"),
    ("collection",          "--collection"),
    ("format",              "--format"),
    ("render_mode",         "--render-mode"),
    ("include_subdomains",  "--include-subdomains"),
    ("discover_sitemaps",   "--discover-sitemaps"),
    ("embed",               "--embed"),
    ("diagnostics",         "--diagnostics"),
    ("delay_ms",            "--delay-ms"),
    ("performance_profile", "--performance-profile"),
    ("respect_robots",      "--respect-robots"),
    ("research_depth",      "--research-depth"),
    ("search_time_range",   "--search-time-range"),
    // Command-specific flags
    ("include_source",      "--include-source"),       // github
    ("sort",                "--sort"),                  // reddit
    ("time",                "--time"),                  // reddit
    ("max_posts",           "--max-posts"),             // reddit
    ("min_score",           "--min-score"),             // reddit
    ("depth",               "--depth"),                 // reddit
    ("scrape_links",        "--scrape-links"),          // reddit
    ("claude",              "--claude"),                // sessions
    ("codex",               "--codex"),                 // sessions
    ("gemini",              "--gemini"),                // sessions
    ("project",             "--project"),               // sessions
];
```

**Step 2: Verify it compiles**

Run: `cargo check --lib 2>&1 | tail -5`

**Step 3: Commit**

```bash
git add crates/web/execute.rs
git commit -m "feat(web): expand allowed flags for command-specific options"
```

---

### Task 3: Add `stdout_json` and `stdout_line` to frontend WS types

**Files:**
- Modify: `apps/web/lib/ws-protocol.ts:8-20`

**What:** Add the two new server message types to the `WsServerMsg` union so TypeScript knows about them.

**Step 1: Update the WsServerMsg type**

Replace lines 8-20:

```typescript
// Server → Client
export type WsServerMsg =
  | { type: 'output'; line: string }
  | { type: 'stdout_json'; data: Record<string, unknown> }
  | { type: 'stdout_line'; line: string }
  | { type: 'log'; line: string }
  | { type: 'file_content'; path: string; content: string }
  | { type: 'crawl_files'; files: CrawlFile[]; output_dir: string }
  | { type: 'done'; exit_code: number; elapsed_ms: number }
  | { type: 'error'; message: string; elapsed_ms?: number; stderr?: string }
  | {
      type: 'stats'
      aggregate: AggregateStats
      containers: Record<string, ContainerStats>
      container_count: number
    }
```

**Step 2: Add missing modes to the MODES array**

Add entries for `retrieve`, `debug`, `dedupe`, `screenshot`, `github`, `reddit`, `youtube` to the `MODES` array (after the existing entries, before `] as const`):

```typescript
  {
    id: 'github',
    label: 'GitHub',
    icon: 'M12 2C6.477 2 2 6.484 2 12.017c0 4.425 2.865 8.18 6.839 9.504.5.092.682-.217.682-.483 0-.237-.008-.868-.013-1.703-2.782.605-3.369-1.343-3.369-1.343-.454-1.158-1.11-1.466-1.11-1.466-.908-.62.069-.608.069-.608 1.003.07 1.531 1.032 1.531 1.032.892 1.53 2.341 1.088 2.91.832.092-.647.35-1.088.636-1.338-2.22-.253-4.555-1.113-4.555-4.951 0-1.093.39-1.988 1.029-2.688-.103-.253-.446-1.272.098-2.65 0 0 .84-.27 2.75 1.026A9.564 9.564 0 0112 6.844c.85.004 1.705.115 2.504.337 1.909-1.296 2.747-1.027 2.747-1.027.546 1.379.202 2.398.1 2.651.64.7 1.028 1.595 1.028 2.688 0 3.848-2.339 4.695-4.566 4.943.359.309.678.92.678 1.855 0 1.338-.012 2.419-.012 2.747 0 .268.18.58.688.482A10.019 10.019 0 0022 12.017C22 6.484 17.522 2 12 2z',
  },
  {
    id: 'reddit',
    label: 'Reddit',
    icon: 'M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm5.8 11.33c.02.16.03.33.03.5 0 2.55-2.97 4.63-6.63 4.63-3.65 0-6.62-2.07-6.62-4.63 0-.17.01-.34.03-.5A1.98 1.98 0 013.5 11c0-1.1.9-2 2-2 .53 0 1.01.21 1.37.55C8.37 8.55 10.1 8 12 8c1.9 0 3.63.55 5.13 1.55.36-.34.84-.55 1.37-.55 1.1 0 2 .9 2 2 0 .84-.52 1.56-1.25 1.85l-.45-.02zM9.5 12c-.83 0-1.5.67-1.5 1.5S8.67 15 9.5 15s1.5-.67 1.5-1.5S10.33 12 9.5 12zm5 0c-.83 0-1.5.67-1.5 1.5s.67 1.5 1.5 1.5 1.5-.67 1.5-1.5-.67-1.5-1.5-1.5zm-5.26 4.44c-.14-.14-.14-.37 0-.51.14-.14.37-.14.51 0 .67.67 1.65 1.07 2.75 1.07s2.08-.4 2.75-1.07c.14-.14.37-.14.51 0 .14.14.14.37 0 .51-.81.81-1.96 1.31-3.26 1.31s-2.45-.5-3.26-1.31z',
  },
  {
    id: 'youtube',
    label: 'YouTube',
    icon: 'M22.54 6.42a2.78 2.78 0 00-1.94-2C18.88 4 12 4 12 4s-6.88 0-8.6.46a2.78 2.78 0 00-1.94 2A29.94 29.94 0 001 12a29.94 29.94 0 00.46 5.58 2.78 2.78 0 001.94 2C5.12 20 12 20 12 20s6.88 0 8.6-.46a2.78 2.78 0 001.94-2A29.94 29.94 0 0023 12a29.94 29.94 0 00-.46-5.58zM9.75 15.02V8.98L15.5 12l-5.75 3.02z',
  },
  {
    id: 'retrieve',
    label: 'Retrieve',
    icon: 'M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4',
  },
  {
    id: 'debug',
    label: 'Debug',
    icon: 'M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z',
  },
  {
    id: 'dedupe',
    label: 'Dedupe',
    icon: 'M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16',
  },
  {
    id: 'screenshot',
    label: 'Screenshot',
    icon: 'M3 9a2 2 0 012-2h.93a2 2 0 001.664-.89l.812-1.22A2 2 0 0110.07 4h3.86a2 2 0 011.664.89l.812 1.22A2 2 0 0018.07 7H19a2 2 0 012 2v9a2 2 0 01-2 2H5a2 2 0 01-2-2V9z M15 13a3 3 0 11-6 0 3 3 0 016 0z',
  },
```

**Step 3: Update NO_INPUT_MODES**

Add `dedupe` to `NO_INPUT_MODES`:

```typescript
export const NO_INPUT_MODES = new Set([
  'stats',
  'status',
  'doctor',
  'domains',
  'sources',
  'suggest',
  'debug',
  'sessions',
  'dedupe',
] as const)
```

**Step 4: Verify lint**

Run: `cd apps/web && pnpm lint 2>&1 | tail -10`

**Step 5: Commit**

```bash
git add apps/web/lib/ws-protocol.ts
git commit -m "feat(web): add stdout_json/stdout_line types + missing modes to picker"
```

---

### Task 4: Accumulate stdout in frontend state

**Files:**
- Modify: `apps/web/hooks/use-ws-messages.ts`

**What:** Add `stdoutLines` (raw strings) and `stdoutObjects` (parsed JSON) arrays to the message state. Handle the two new message types. Also fix `target` and `lines` in recent runs.

**Step 1: Add state + types**

Add to `WsMessagesContextValue` interface (after `errorMessage`):

```typescript
/** Raw stdout text lines (non-JSON) */
stdoutLines: string[]
/** Parsed JSON objects from stdout */
stdoutObjects: Record<string, unknown>[]
```

Add state declarations in `useWsMessagesProvider`:

```typescript
const [stdoutLines, setStdoutLines] = useState<string[]>([])
const [stdoutObjects, setStdoutObjects] = useState<Record<string, unknown>[]>([])
const targetRef = useRef('')
const lineCountRef = useRef(0)
```

**Step 2: Handle new message types in the subscribe effect**

Add cases to the switch statement:

```typescript
case 'stdout_json':
  setStdoutObjects((prev) => [...prev, msg.data])
  lineCountRef.current += 1
  break
case 'stdout_line':
  setStdoutLines((prev) => [...prev, msg.line])
  lineCountRef.current += 1
  break
```

Also increment `lineCountRef` in the `log` case:

```typescript
case 'log':
  setLogLines((prev) => [...prev, { content: msg.line, timestamp: Date.now() }])
  lineCountRef.current += 1
  break
```

**Step 3: Fix target + lines in recent runs**

In both `done` and `error` handlers, replace `target: ''` with `target: targetRef.current` and `lines: 0` with `lines: lineCountRef.current`.

**Step 4: Update startExecution to accept and store target**

Change the signature and body:

```typescript
const startExecution = useCallback((mode: string, target?: string) => {
  currentModeRef.current = mode
  targetRef.current = target ?? ''
  lineCountRef.current = 0
  setCurrentMode(mode)
  setMarkdownContent('')
  setLogLines([])
  setErrorMessage('')
  setStdoutLines([])
  setStdoutObjects([])
  setIsProcessing(true)
  setHasResults(true)
  setCrawlFiles([])
  setSelectedFile(null)
}, [])
```

Update `WsMessagesContextValue.startExecution` type:

```typescript
startExecution: (mode: string, target?: string) => void
```

**Step 5: Return new state from the hook**

Add `stdoutLines` and `stdoutObjects` to the return object.

**Step 6: Update omnibox to pass target**

In `apps/web/components/omnibox.tsx`, change `startExecution(execMode)` (line 72) to:

```typescript
startExecution(execMode, execInput.trim())
```

**Step 7: Verify lint + build**

Run: `cd apps/web && pnpm lint && pnpm build 2>&1 | tail -20`

**Step 8: Commit**

```bash
git add apps/web/hooks/use-ws-messages.ts apps/web/components/omnibox.tsx
git commit -m "feat(web): accumulate stdout state + fix recent runs target/lines"
```

---

## Phase B: Renderer Components

Five renderers, each handling a `renderIntent` from `AXON_COMMAND_SPECS`. Each is a pure component that receives data arrays and renders them.

---

### Task 5: Create the render intent dispatcher

**Files:**
- Create: `apps/web/lib/render-dispatch.ts`

**What:** A function that takes a mode ID, looks up its `renderIntent` from `AXON_COMMAND_SPECS`, and returns it. This is what makes `axon-command-map.ts` the live source of truth.

**Step 1: Write the module**

```typescript
import { AXON_COMMAND_SPECS, type AxonRenderIntent } from '@/lib/axon-command-map'

const specMap = new Map(AXON_COMMAND_SPECS.map((s) => [s.id, s]))

export function getRenderIntent(mode: string): AxonRenderIntent {
  return specMap.get(mode)?.renderIntent ?? 'raw-fallback'
}

export function getCommandSpec(mode: string) {
  return specMap.get(mode) ?? null
}

export function isAsyncMode(mode: string): boolean {
  return specMap.get(mode)?.asyncByDefault ?? false
}
```

**Step 2: Verify lint**

Run: `cd apps/web && pnpm lint 2>&1 | tail -5`

**Step 3: Commit**

```bash
git add apps/web/lib/render-dispatch.ts
git commit -m "feat(web): render intent dispatcher from command specs"
```

---

### Task 6: Build the raw fallback renderer

**Files:**
- Create: `apps/web/components/renderers/raw-renderer.tsx`

**What:** The safety net. Renders raw stdout lines as a scrollable monospace block. Used when no specialized renderer matches or when JSON parse confidence is low.

**Step 1: Write the component**

```tsx
'use client'

interface RawRendererProps {
  lines: string[]
  objects: Record<string, unknown>[]
}

export function RawRenderer({ lines, objects }: RawRendererProps) {
  if (lines.length === 0 && objects.length === 0) {
    return (
      <div className="flex h-32 items-center justify-center text-sm text-[#8787af]">
        Waiting for output...
      </div>
    )
  }

  return (
    <div className="space-y-1 font-mono text-xs leading-relaxed">
      {objects.map((obj, i) => (
        <pre
          key={`obj-${i}`}
          className="overflow-x-auto whitespace-pre-wrap rounded-md border border-[rgba(175,215,255,0.08)] bg-[rgba(10,18,35,0.4)] p-3 text-[#dce6f0]"
        >
          {JSON.stringify(obj, null, 2)}
        </pre>
      ))}
      {lines.map((line, i) => (
        <div key={`line-${i}`} className="text-[#8787af]">
          {line}
        </div>
      ))}
    </div>
  )
}
```

**Step 2: Commit**

```bash
git add apps/web/components/renderers/raw-renderer.tsx
git commit -m "feat(web): raw fallback renderer component"
```

---

### Task 7: Build the table renderer

**Files:**
- Create: `apps/web/components/renderers/table-renderer.tsx`

**What:** Renders JSON objects as a sortable table. Used by `sources`, `domains`, `map`, `retrieve`, `suggest`, `status`. Auto-detects columns from the first object's keys.

**Step 1: Write the component**

```tsx
'use client'

import { useMemo, useState } from 'react'

interface TableRendererProps {
  objects: Record<string, unknown>[]
  /** Override column order. If omitted, derived from first object keys. */
  columns?: string[]
}

export function TableRenderer({ objects, columns: columnOverride }: TableRendererProps) {
  const [sortCol, setSortCol] = useState<string | null>(null)
  const [sortAsc, setSortAsc] = useState(true)

  const columns = useMemo(() => {
    if (columnOverride) return columnOverride
    if (objects.length === 0) return []
    return Object.keys(objects[0])
  }, [objects, columnOverride])

  const sorted = useMemo(() => {
    if (!sortCol) return objects
    return [...objects].sort((a, b) => {
      const av = a[sortCol]
      const bv = b[sortCol]
      if (typeof av === 'number' && typeof bv === 'number') {
        return sortAsc ? av - bv : bv - av
      }
      const as = String(av ?? '')
      const bs = String(bv ?? '')
      return sortAsc ? as.localeCompare(bs) : bs.localeCompare(as)
    })
  }, [objects, sortCol, sortAsc])

  const toggleSort = (col: string) => {
    if (sortCol === col) {
      setSortAsc((prev) => !prev)
    } else {
      setSortCol(col)
      setSortAsc(true)
    }
  }

  if (objects.length === 0) {
    return (
      <div className="flex h-32 items-center justify-center text-sm text-[#8787af]">
        Waiting for output...
      </div>
    )
  }

  return (
    <div className="overflow-x-auto">
      <table className="w-full border-collapse font-mono text-xs">
        <thead>
          <tr className="text-[10px] uppercase tracking-wider text-[#8787af]">
            {columns.map((col) => (
              <th
                key={col}
                onClick={() => toggleSort(col)}
                className="cursor-pointer border-b border-[rgba(175,215,255,0.15)] px-3 pb-2 text-left transition-colors hover:text-[#afd7ff]"
              >
                {col.replace(/_/g, ' ')}
                {sortCol === col && (
                  <span className="ml-1">{sortAsc ? '\u2191' : '\u2193'}</span>
                )}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {sorted.map((row, i) => (
            <tr
              key={i}
              className="border-b border-[rgba(175,215,255,0.05)] hover:bg-[rgba(175,215,255,0.03)]"
            >
              {columns.map((col) => (
                <td key={col} className="max-w-[400px] truncate px-3 py-2 text-[#dce6f0]">
                  {formatCell(row[col])}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
      <div className="mt-2 text-right text-[10px] text-[#5f87af]">
        {objects.length} {objects.length === 1 ? 'row' : 'rows'}
      </div>
    </div>
  )
}

function formatCell(value: unknown): string {
  if (value === null || value === undefined) return '—'
  if (typeof value === 'number') return value.toLocaleString()
  if (typeof value === 'boolean') return value ? 'yes' : 'no'
  if (Array.isArray(value)) return `[${value.length}]`
  if (typeof value === 'object') return JSON.stringify(value)
  return String(value)
}
```

**Step 2: Commit**

```bash
git add apps/web/components/renderers/table-renderer.tsx
git commit -m "feat(web): table renderer with auto-columns and sorting"
```

---

### Task 8: Build the cards renderer

**Files:**
- Create: `apps/web/components/renderers/cards-renderer.tsx`

**What:** Renders search/query results as vertical cards with title, snippet, URL, and score. Used by `query`, `search`, `extract`.

**Step 1: Write the component**

```tsx
'use client'

interface CardsRendererProps {
  objects: Record<string, unknown>[]
}

export function CardsRenderer({ objects }: CardsRendererProps) {
  if (objects.length === 0) {
    return (
      <div className="flex h-32 items-center justify-center text-sm text-[#8787af]">
        Waiting for output...
      </div>
    )
  }

  return (
    <div className="space-y-3">
      {objects.map((obj, i) => (
        <Card key={i} data={obj} rank={i + 1} />
      ))}
    </div>
  )
}

function Card({ data, rank }: { data: Record<string, unknown>; rank: number }) {
  const title = str(data.title) || str(data.url) || str(data.name) || `Result ${rank}`
  const snippet =
    str(data.snippet) || str(data.content) || str(data.text) || str(data.description) || ''
  const url = str(data.url) || str(data.source) || ''
  const score = num(data.score) ?? num(data.rank) ?? num(data.position)

  return (
    <div
      className="rounded-lg border border-[rgba(175,215,255,0.1)] p-4 transition-colors hover:border-[rgba(175,215,255,0.2)]"
      style={{ background: 'rgba(10, 18, 35, 0.4)' }}
    >
      <div className="mb-1.5 flex items-start gap-3">
        <span className="shrink-0 font-mono text-[10px] font-bold text-[#5f87af]">
          {score !== null ? `#${score}` : `#${rank}`}
        </span>
        <div className="min-w-0 flex-1">
          <div className="text-sm font-medium text-[#afd7ff]">{title}</div>
          {url && (
            <div className="mt-0.5 truncate font-mono text-[10px] text-[#5f87af]">{url}</div>
          )}
        </div>
      </div>
      {snippet && (
        <p className="mt-2 text-xs leading-relaxed text-[#8787af]">
          {snippet.length > 500 ? `${snippet.slice(0, 500)}...` : snippet}
        </p>
      )}
    </div>
  )
}

function str(v: unknown): string {
  return typeof v === 'string' ? v : ''
}

function num(v: unknown): number | null {
  return typeof v === 'number' ? v : null
}
```

**Step 2: Commit**

```bash
git add apps/web/components/renderers/cards-renderer.tsx
git commit -m "feat(web): cards renderer for search/query results"
```

---

### Task 9: Build the report renderer

**Files:**
- Create: `apps/web/components/renderers/report-renderer.tsx`

**What:** Renders long-form text output — `ask` answers, `evaluate` judgments, `research` reports, `doctor` diagnostics, `debug` analysis. Detects an `answer` or `report` field and renders it as styled prose; appends sources/metadata below.

**Step 1: Write the component**

```tsx
'use client'

import { useMemo } from 'react'

interface ReportRendererProps {
  objects: Record<string, unknown>[]
  lines: string[]
}

export function ReportRenderer({ objects, lines }: ReportRendererProps) {
  // Merge all objects — report commands typically emit one big JSON
  const merged = useMemo(() => {
    if (objects.length === 0) return null
    if (objects.length === 1) return objects[0]
    return Object.assign({}, ...objects)
  }, [objects])

  if (!merged && lines.length === 0) {
    return (
      <div className="flex h-32 items-center justify-center text-sm text-[#8787af]">
        Waiting for output...
      </div>
    )
  }

  // Extract the main body text from common field names
  const body =
    str(merged?.answer) ||
    str(merged?.report) ||
    str(merged?.result) ||
    str(merged?.analysis) ||
    str(merged?.output) ||
    str(merged?.text) ||
    str(merged?.content) ||
    lines.join('\n')

  // Extract metadata fields (everything that isn't the body)
  const bodyKeys = new Set(['answer', 'report', 'result', 'analysis', 'output', 'text', 'content'])
  const meta = merged
    ? Object.entries(merged).filter(([k]) => !bodyKeys.has(k))
    : []

  // Sources list (if present)
  const sources = extractSources(merged)

  return (
    <div className="space-y-4">
      {/* Main body */}
      <div className="whitespace-pre-wrap text-sm leading-[1.8] text-[#dce6f0]">{body}</div>

      {/* Sources */}
      {sources.length > 0 && (
        <div className="border-t border-[rgba(175,215,255,0.1)] pt-3">
          <div className="mb-2 text-[10px] font-semibold uppercase tracking-wider text-[#5f87af]">
            Sources
          </div>
          <div className="space-y-1">
            {sources.map((src, i) => (
              <div key={i} className="font-mono text-[11px] text-[#8787af]">
                <span className="mr-2 text-[#5f87af]">[{i + 1}]</span>
                {src}
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Metadata */}
      {meta.length > 0 && (
        <div className="border-t border-[rgba(175,215,255,0.1)] pt-3">
          <div className="mb-2 text-[10px] font-semibold uppercase tracking-wider text-[#5f87af]">
            Details
          </div>
          <div className="grid grid-cols-2 gap-x-6 gap-y-1 font-mono text-[11px]">
            {meta.map(([key, val]) => (
              <div key={key} className="contents">
                <span className="text-[#5f87af]">{key.replace(/_/g, ' ')}</span>
                <span className="truncate text-[#8787af]">{formatValue(val)}</span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}

function str(v: unknown): string {
  return typeof v === 'string' ? v : ''
}

function extractSources(obj: Record<string, unknown> | null): string[] {
  if (!obj) return []
  const sources = obj.sources ?? obj.chunks ?? obj.references ?? obj.urls
  if (Array.isArray(sources)) {
    return sources.map((s) => {
      if (typeof s === 'string') return s
      if (typeof s === 'object' && s !== null) {
        return (s as Record<string, unknown>).url as string ?? JSON.stringify(s)
      }
      return String(s)
    })
  }
  return []
}

function formatValue(v: unknown): string {
  if (v === null || v === undefined) return '—'
  if (typeof v === 'number') return v.toLocaleString()
  if (typeof v === 'boolean') return v ? 'yes' : 'no'
  if (Array.isArray(v)) return `[${v.length} items]`
  if (typeof v === 'object') return JSON.stringify(v)
  return String(v)
}
```

**Step 2: Commit**

```bash
git add apps/web/components/renderers/report-renderer.tsx
git commit -m "feat(web): report renderer for ask/evaluate/research/doctor"
```

---

### Task 10: Build the status-summary renderer

**Files:**
- Create: `apps/web/components/renderers/status-renderer.tsx`

**What:** Renders operational commands like `stats`, `status`, `doctor`, `embed`, `dedupe` as a key-value card grid. Doctor gets special treatment — service health as colored badges.

**Step 1: Write the component**

```tsx
'use client'

interface StatusRendererProps {
  objects: Record<string, unknown>[]
  lines: string[]
  mode: string
}

export function StatusRenderer({ objects, lines, mode }: StatusRendererProps) {
  const merged = objects.length > 0 ? Object.assign({}, ...objects) : null

  if (!merged && lines.length === 0) {
    return (
      <div className="flex h-32 items-center justify-center text-sm text-[#8787af]">
        Waiting for output...
      </div>
    )
  }

  // Doctor mode: render services as health badges
  if (mode === 'doctor' && merged?.services) {
    return <DoctorView data={merged} />
  }

  // Status mode: render job queues
  if (mode === 'status' && merged) {
    return <JobStatusView data={merged} />
  }

  // Generic key-value display
  if (merged) {
    return <KeyValueGrid data={merged} />
  }

  // Lines fallback
  return (
    <div className="space-y-1 font-mono text-xs">
      {lines.map((line, i) => (
        <div key={i} className="text-[#8787af]">{line}</div>
      ))}
    </div>
  )
}

function DoctorView({ data }: { data: Record<string, unknown> }) {
  const services = data.services as Record<string, unknown>[] | undefined
  const allOk = data.all_ok as boolean | undefined

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2">
        <span
          className={`size-2.5 rounded-full ${
            allOk
              ? 'bg-[#4ade80] shadow-[0_0_8px_rgba(74,222,128,0.5)]'
              : 'bg-[#ef4444] shadow-[0_0_8px_rgba(239,68,68,0.5)]'
          }`}
        />
        <span className="text-sm font-medium text-[#dce6f0]">
          {allOk ? 'All services healthy' : 'Some services degraded'}
        </span>
      </div>
      {Array.isArray(services) && (
        <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
          {services.map((svc, i) => {
            const name = String((svc as Record<string, unknown>).name ?? `Service ${i + 1}`)
            const ok = (svc as Record<string, unknown>).ok as boolean | undefined
            const latency = (svc as Record<string, unknown>).latency_ms as number | undefined
            return (
              <div
                key={i}
                className="flex items-center gap-2.5 rounded-lg border border-[rgba(175,215,255,0.08)] p-3"
                style={{ background: 'rgba(10, 18, 35, 0.4)' }}
              >
                <span
                  className={`size-2 shrink-0 rounded-full ${
                    ok
                      ? 'bg-[#4ade80] shadow-[0_0_6px_rgba(74,222,128,0.4)]'
                      : 'bg-[#ef4444] shadow-[0_0_6px_rgba(239,68,68,0.4)]'
                  }`}
                />
                <span className="flex-1 text-xs font-medium text-[#dce6f0]">{name}</span>
                {latency !== undefined && (
                  <span className="font-mono text-[10px] tabular-nums text-[#5f87af]">
                    {latency}ms
                  </span>
                )}
              </div>
            )
          })}
        </div>
      )}
      <KeyValueGrid
        data={Object.fromEntries(
          Object.entries(data).filter(([k]) => k !== 'services' && k !== 'all_ok')
        )}
      />
    </div>
  )
}

function JobStatusView({ data }: { data: Record<string, unknown> }) {
  const queues = Object.entries(data).filter(([, v]) => Array.isArray(v))

  return (
    <div className="space-y-4">
      {queues.map(([name, jobs]) => (
        <div key={name}>
          <div className="mb-2 text-[10px] font-semibold uppercase tracking-wider text-[#5f87af]">
            {name.replace(/_/g, ' ')}
            <span className="ml-1.5 text-[#8787af]">{(jobs as unknown[]).length}</span>
          </div>
          {(jobs as Record<string, unknown>[]).map((job, i) => (
            <div
              key={i}
              className="mb-1 flex items-center gap-3 rounded-md border border-[rgba(175,215,255,0.06)] px-3 py-2 font-mono text-[11px]"
              style={{ background: 'rgba(10, 18, 35, 0.3)' }}
            >
              <StatusBadge status={String(job.status ?? 'unknown')} />
              <span className="flex-1 truncate text-[#dce6f0]">
                {String(job.url ?? job.id ?? '')}
              </span>
              {job.created_at && (
                <span className="text-[10px] text-[#475569]">{String(job.created_at)}</span>
              )}
            </div>
          ))}
        </div>
      ))}
      {queues.length === 0 && <KeyValueGrid data={data} />}
    </div>
  )
}

function StatusBadge({ status }: { status: string }) {
  const colors: Record<string, string> = {
    completed: 'bg-[#4ade80] text-[#052e16]',
    running: 'bg-[#60a5fa] text-[#172554]',
    pending: 'bg-[#fbbf24] text-[#451a03]',
    failed: 'bg-[#ef4444] text-white',
    canceled: 'bg-[#8787af] text-[#1e1b4b]',
  }
  return (
    <span
      className={`shrink-0 rounded-full px-2 py-0.5 text-[9px] font-bold uppercase tracking-wider ${
        colors[status] ?? 'bg-[#334155] text-[#94a3b8]'
      }`}
    >
      {status}
    </span>
  )
}

function KeyValueGrid({ data }: { data: Record<string, unknown> }) {
  const entries = Object.entries(data).filter(
    ([, v]) => !Array.isArray(v) && typeof v !== 'object'
  )
  if (entries.length === 0) return null

  return (
    <div className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-1.5 font-mono text-[11px]">
      {entries.map(([key, val]) => (
        <div key={key} className="contents">
          <span className="text-[#5f87af]">{key.replace(/_/g, ' ')}</span>
          <span className="truncate text-[#dce6f0]">{formatVal(val)}</span>
        </div>
      ))}
    </div>
  )
}

function formatVal(v: unknown): string {
  if (v === null || v === undefined) return '—'
  if (typeof v === 'number') return v.toLocaleString()
  if (typeof v === 'boolean') return v ? 'yes' : 'no'
  return String(v)
}
```

**Step 2: Commit**

```bash
git add apps/web/components/renderers/status-renderer.tsx
git commit -m "feat(web): status/doctor/job-queue renderer with health badges"
```

---

## Phase C: Wire Renderers into Results Panel

### Task 11: Integrate renderer dispatch into the Content tab

**Files:**
- Modify: `apps/web/components/results-panel.tsx`

**What:** When the current mode is NOT `scrape` or `crawl`, use `getRenderIntent()` to select and render the appropriate component. Keep the existing Plate editor + crawl file explorer for scrape/crawl exactly as-is.

**Step 1: Add imports**

At the top of `results-panel.tsx`:

```typescript
import { getRenderIntent } from '@/lib/render-dispatch'
import { RawRenderer } from '@/components/renderers/raw-renderer'
import { TableRenderer } from '@/components/renderers/table-renderer'
import { CardsRenderer } from '@/components/renderers/cards-renderer'
import { ReportRenderer } from '@/components/renderers/report-renderer'
import { StatusRenderer } from '@/components/renderers/status-renderer'
```

**Step 2: Pull new state from context**

Add to the destructured `useWsMessages()` call:

```typescript
const {
  markdownContent,
  logLines,
  errorMessage,
  recentRuns,
  isProcessing,
  hasResults,
  currentMode,
  crawlFiles,
  selectedFile,
  selectFile,
  stdoutLines,
  stdoutObjects,
} = useWsMessages()
```

**Step 3: Add render intent logic**

After `const hasCrawlFiles = crawlFiles.length > 0`, add:

```typescript
const isFileMode = currentMode === 'scrape' || currentMode === 'crawl'
const renderIntent = getRenderIntent(currentMode)
```

**Step 4: Replace the content tab body**

Replace the `{activeTab === 'content' && (...)}` block with:

```tsx
{activeTab === 'content' && (
  <div
    className="flex max-h-[72vh] overflow-hidden rounded-[10px] border border-[rgba(175,215,255,0.1)]"
    style={{ background: 'rgba(3, 7, 18, 0.25)' }}
  >
    {/* Crawl file explorer sidebar (only for crawl mode) */}
    {hasCrawlFiles && (
      <CrawlFileExplorer
        files={crawlFiles}
        selectedFile={selectedFile}
        onSelectFile={selectFile}
      />
    )}

    {/* Main content area */}
    <div className="flex-1 overflow-y-auto p-3 text-sm leading-[1.75] text-[#dce6f0] sm:p-4 md:p-6">
      {/* Crawl progress bar */}
      {isCrawlMode && isProcessing && (
        <CrawlProgress logLines={logLines} isProcessing={isProcessing} />
      )}

      {/* File-producing modes: use Plate editor */}
      {isFileMode && (
        <ContentViewer
          markdown={markdownContent}
          isProcessing={isProcessing}
          errorMessage={errorMessage}
        />
      )}

      {/* Non-file modes: renderer dispatch */}
      {!isFileMode && (
        <CommandOutput
          intent={renderIntent}
          mode={currentMode}
          objects={stdoutObjects}
          lines={stdoutLines}
          isProcessing={isProcessing}
          errorMessage={errorMessage}
        />
      )}
    </div>
  </div>
)}
```

**Step 5: Add the CommandOutput component**

At the bottom of `results-panel.tsx` (before the `LogViewer` component):

```tsx
function CommandOutput({
  intent,
  mode,
  objects,
  lines,
  isProcessing,
  errorMessage,
}: {
  intent: string
  mode: string
  objects: Record<string, unknown>[]
  lines: string[]
  isProcessing: boolean
  errorMessage: string
}) {
  if (errorMessage) {
    return (
      <div className="flex items-center gap-2 rounded-lg border border-[rgba(239,68,68,0.2)] bg-[rgba(239,68,68,0.05)] p-4 text-sm text-[#ef4444]">
        <span className="size-2 shrink-0 rounded-full bg-[#ef4444]" />
        {errorMessage}
      </div>
    )
  }

  if (isProcessing && objects.length === 0 && lines.length === 0) {
    return (
      <div className="flex h-32 items-center justify-center">
        <div className="flex items-center gap-3 text-sm text-[#8787af]">
          <span className="inline-block size-3 animate-spin rounded-full border-2 border-[rgba(255,135,175,0.2)] border-t-[#ff87af]" />
          Processing...
        </div>
      </div>
    )
  }

  switch (intent) {
    case 'table':
      return <TableRenderer objects={objects} />
    case 'cards':
      return <CardsRenderer objects={objects} />
    case 'report':
      return <ReportRenderer objects={objects} lines={lines} />
    case 'status-summary':
      return <StatusRenderer objects={objects} lines={lines} mode={mode} />
    case 'job-lifecycle':
      return <StatusRenderer objects={objects} lines={lines} mode={mode} />
    default:
      return <RawRenderer lines={lines} objects={objects} />
  }
}
```

**Step 6: Verify lint + build**

Run: `cd apps/web && pnpm lint && pnpm build 2>&1 | tail -20`

**Step 7: Commit**

```bash
git add apps/web/components/results-panel.tsx
git commit -m "feat(web): wire renderer dispatch into content tab by render intent"
```

---

## Phase D: Bug Fixes

### Task 12: Fix cancel — send actual job ID

**Files:**
- Modify: `crates/web/execute.rs` (add job_id to done message)
- Modify: `apps/web/lib/ws-protocol.ts` (add job_id to done type)
- Modify: `apps/web/hooks/use-ws-messages.ts` (store job_id)
- Modify: `apps/web/components/omnibox.tsx` (send job_id on cancel)

**What:** The backend knows the job ID (it's in the CLI JSON output for async modes). Propagate it to the frontend so cancel actually works.

**Step 1: Capture job_id from stdout in execute.rs**

This is the trickiest part. For async modes, the CLI stdout often contains `{"id": "uuid", ...}`. We need to capture it. The simplest approach: in the stdout streaming task, watch for a `job_id` field and send it as a separate message.

Add after the `stdout_json` message send in the stdout task (Task 1's code):

```rust
// If this JSON has a job ID, send it separately for cancel support
if let Some(id) = parsed.get("id").and_then(|v| v.as_str()) {
    let _ = stdout_tx
        .send(json!({"type": "job_id", "id": id}).to_string())
        .await;
}
```

**Step 2: Add `job_id` to frontend types**

In `ws-protocol.ts`, add to `WsServerMsg`:

```typescript
| { type: 'job_id'; id: string }
```

**Step 3: Track job ID in message state**

In `use-ws-messages.ts`:
- Add `const [jobId, setJobId] = useState<string | null>(null)`
- Add case: `case 'job_id': setJobId(msg.id); break`
- Reset in `startExecution`: `setJobId(null)`
- Add `jobId` to context value + return

**Step 4: Use real job ID in omnibox cancel**

In `omnibox.tsx`, pull `jobId` from `useWsMessages()` and change the cancel handler:

```typescript
const { startExecution, jobId } = useWsMessages()

const cancel = useCallback(() => {
  if (!isProcessing) return
  const cancelId = jobId ?? String(execIdRef.current)
  send({ type: 'cancel', id: cancelId })
  setIsProcessing(false)
  const elapsed = Date.now() - startTimeRef.current
  const secs = (elapsed / 1000).toFixed(1)
  setStatusText(`${secs}s \u00b7 cancelled`)
  setStatusType('error')
}, [isProcessing, jobId, send])
```

**Step 5: Verify lint + build**

Run: `cargo check --lib && cd apps/web && pnpm lint && pnpm build`

**Step 6: Commit**

```bash
git add crates/web/execute.rs apps/web/lib/ws-protocol.ts apps/web/hooks/use-ws-messages.ts apps/web/components/omnibox.tsx
git commit -m "fix(web): propagate job ID for cancel support"
```

---

## Phase E: Verification

### Task 13: Manual smoke test — all command classes

**What:** Run through each renderer class with a real command to verify end-to-end flow.

**Step 1: Build backend**

Run: `cargo build --bin axon`

**Step 2: Start services**

Run: `docker compose up -d && ./target/debug/axon serve`

**Step 3: Open browser and test each renderer class**

| Test | Command | Expected Renderer | Verify |
|------|---------|-------------------|--------|
| Table | `sources` (no-input auto-execute) | TableRenderer | Rows with url + chunk_count columns |
| Table | `domains` (no-input) | TableRenderer | Domain names + stats |
| Cards | `query` → "rust async" | CardsRenderer | Ranked cards with snippets |
| Report | `ask` → "what is Axon?" | ReportRenderer | Answer prose + sources list |
| Report | `doctor` (no-input) | StatusRenderer | Health badges per service |
| Status | `status` (no-input) | StatusRenderer | Job queue lists |
| Raw | Any command with unexpected output shape | RawRenderer | JSON blocks |
| File | `scrape` → any URL | ContentViewer (Plate) | Markdown rendered (no regression) |
| File | `crawl` → any URL | File explorer + Plate | Sidebar + markdown (no regression) |

**Step 4: Check recent runs tab**

Verify that `target` column shows the URL/query you entered and `lines` column shows non-zero counts.

**Step 5: Test cancel**

Run a `crawl` command, click Cancel. Verify the cancel request sends a UUID (check browser dev tools Network tab → WS messages), not an integer.

---

## Execution Order Summary

| Task | Phase | Files Changed | Estimated Effort |
|------|-------|--------------|-----------------|
| 1 | A | `execute.rs` | Backend: stdout streaming + `--json` injection |
| 2 | A | `execute.rs` | Backend: expand flag whitelist |
| 3 | A | `ws-protocol.ts` | Frontend: new types + modes |
| 4 | A | `use-ws-messages.ts`, `omnibox.tsx` | Frontend: accumulate stdout + fix recent runs |
| 5 | B | `render-dispatch.ts` | Frontend: command spec lookup |
| 6 | B | `raw-renderer.tsx` | Frontend: fallback renderer |
| 7 | B | `table-renderer.tsx` | Frontend: table renderer |
| 8 | B | `cards-renderer.tsx` | Frontend: cards renderer |
| 9 | B | `report-renderer.tsx` | Frontend: report renderer |
| 10 | B | `status-renderer.tsx` | Frontend: status/doctor renderer |
| 11 | C | `results-panel.tsx` | Frontend: wire everything together |
| 12 | D | 4 files across Rust + TS | Fix cancel with real job IDs |
| 13 | E | none | Manual smoke test |

**Phases A+B are fully parallelizable** — backend (Tasks 1-2) and frontend renderer components (Tasks 6-10) have no dependencies on each other. Tasks 3-4 depend on Task 1 conceptually but can be coded in parallel since the types are known. Task 11 depends on all of Tasks 5-10. Task 12 is independent. Task 13 depends on everything.

---

## What This Plan Does NOT Do (Intentional Scope Cuts)

1. **No flag UI** — Flags can now be passed (Task 2 expands the whitelist) but there's no dropdown/form for them yet. That's Phase E from the original `rest-of-commands.md`. The omnibox still sends `flags: {}`. Users who need flags can use the CLI directly.

2. **No job subcommand UI** — `crawl status <id>`, `crawl list`, etc. are not exposed in the web UI. The job ID propagation (Task 12) is groundwork for this.

3. **No streaming incremental rendering** — Objects accumulate and the renderer re-renders on each new object. For commands that emit hundreds of lines (like `sources` with 100k URLs), this could be slow. Virtualization (react-window) is a follow-up.

4. **No Plate rendering for non-scrape markdown** — `ask` and `research` answers are rendered as prose text, not through Plate. Adding markdown parsing for report bodies is a follow-up.

5. **No scrape/crawl changes** — Existing behavior is untouched. The `file_content` and `crawl_files` paths are preserved exactly.
