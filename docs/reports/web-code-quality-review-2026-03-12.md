# Axon Web Frontend -- Code Quality Review

**Date:** 2026-03-12
**Scope:** `apps/web/` -- Next.js 16 frontend for the Axon RAG system
**Reviewer:** Opus 4.6 code review agent

---

## Executive Summary

The `apps/web/` codebase is a well-structured Next.js application with thoughtful state management decomposition, solid SSRF protection, and good separation of concerns between hooks, components, and API routes. The shell state split into focused modules (`-layout`, `-messages`, `-session`, `-settings`, `-tools`, `-actions`) is the right architectural choice for this complexity level.

That said, there are concrete issues across six categories: code complexity in the ACP hook, duplicated patterns in job queries, SQL injection risk in dynamic query construction, an unbounded in-memory cache, missing error propagation in key paths, and a reducer/handler divergence risk. None are show-stoppers, but several are the kind of technical debt that compounds quietly.

**Findings by severity:**
- Critical: 2
- High: 5
- Medium: 8
- Low: 5

---

## Table of Contents

1. [Critical Findings](#critical-findings)
2. [High Severity Findings](#high-severity-findings)
3. [Medium Severity Findings](#medium-severity-findings)
4. [Low Severity Findings](#low-severity-findings)

---

## Critical Findings

### C-1: SQL Injection via String Interpolation in Job Queries

**File:** `/home/jmagar/workspace/axon_rust/apps/web/app/api/jobs/route.ts`, lines 80-97 and 288-311
**Severity:** Critical

The `statusWhere()` function returns a raw SQL string that is interpolated directly into queries via template literals (`WHERE ${where}`). While the input is currently constrained by `VALID_STATUSES`, the `statusWhere` function itself accepts a `StatusFilter` string type and returns arbitrary SQL. If this function is ever reused or modified, or if the validation gate is bypassed, the interpolated SQL would be injectable.

More critically, the `type === 'all'` UNION query on lines 288-311 interpolates `statusWhere()` five separate times into a single query string. This pattern is fragile -- a single change to `statusWhere` that accepts untrusted input would affect five injection points simultaneously.

```typescript
// Current pattern (lines 290-311)
const unionResult = await getJobsPgPool().query(
  `WITH combined AS (
    SELECT ... FROM axon_crawl_jobs WHERE ${where}    // <-- interpolated
    UNION ALL
    SELECT ... FROM axon_extract_jobs WHERE ${where}  // <-- interpolated
    ...
  )
  ...`,
  [limit, offset],
)
```

**Recommendation:** Use parameterized status filtering. Replace `statusWhere()` with a parameterized approach:

```typescript
function statusValues(filter: StatusFilter): string[] {
  switch (filter) {
    case 'active': return ['pending', 'running']
    case 'failed': return ['failed', 'canceled']
    case 'all': return []
    default: return [filter]
  }
}

// In query:
const statuses = statusValues(safeStatusFilter)
const statusClause = statuses.length > 0
  ? `status = ANY($3::text[])`
  : '1=1'
// Pass statuses as a query parameter
```

This removes the string interpolation entirely and makes the query immune to injection regardless of how `statusValues` evolves.

---

### C-2: PgPool Created Without Connection Limits or Timeouts

**File:** `/home/jmagar/workspace/axon_rust/apps/web/lib/server/pg-pool.ts`, lines 11-17
**Severity:** Critical

The Postgres pool is created with zero configuration beyond the connection string:

```typescript
function createPool(): Pool {
  const connectionString =
    process.env.AXON_PG_URL ?? process.env.AXON_PG_MCP_URL ?? DEFAULT_AXON_PG_URL
  return new Pool({ connectionString })
}
```

The `pg` library defaults are:
- `max`: 10 connections (reasonable for a single process, but can exhaust under load)
- `idleTimeoutMillis`: 10000 (10s -- connections reclaimed quickly)
- `connectionTimeoutMillis`: 0 (infinite -- a hung DB will hang the entire request indefinitely)
- No `statement_timeout` -- a slow query will hold a connection forever

In the job dashboard UNION query (C-1), a single request already runs two queries (status counts + jobs). Under concurrent load, this pool can be exhausted. More importantly, the missing `connectionTimeoutMillis` means a network partition or unresponsive DB will cause all requests to queue indefinitely until the process is killed.

**Recommendation:**

```typescript
function createPool(): Pool {
  const connectionString =
    process.env.AXON_PG_URL ?? process.env.AXON_PG_MCP_URL ?? DEFAULT_AXON_PG_URL
  return new Pool({
    connectionString,
    max: 5,
    connectionTimeoutMillis: 5000,
    idleTimeoutMillis: 30000,
    statement_timeout: 15000,
  })
}
```

---

## High Severity Findings

### H-1: useAxonAcp Subscribe Effect Has Excessive Dependency Array

**File:** `/home/jmagar/workspace/axon_rust/apps/web/hooks/use-axon-acp.ts`, lines 200-518
**Severity:** High

The main `useEffect` in `useAxonAcp` (lines 200-518) is a 318-line effect that subscribes to WebSocket messages and dispatches them through a switch statement. Its dependency array includes `agent`, `model`, and `sessionMode` (line 515-517), which means the entire subscription is torn down and rebuilt every time the user changes the model or session mode.

This is problematic because:
1. The `flushTimerRef` and `streamingTimeoutRef` are cleared in the cleanup, meaning an in-progress stream's buffered content can be lost during a model change.
2. The subscribe/unsubscribe cycle creates a brief window where messages can be missed.
3. These values (`agent`, `model`, `sessionMode`) are only used in the `result` telemetry logging (line 315-323), which is dev-only.

**Recommendation:** Move the telemetry values into refs instead of closing over them:

```typescript
const agentRef = useRef(agent)
const modelRef = useRef(model)
const sessionModeRef = useRef(sessionMode)
useEffect(() => { agentRef.current = agent }, [agent])
useEffect(() => { modelRef.current = model }, [model])
useEffect(() => { sessionModeRef.current = sessionMode }, [sessionMode])
```

Then remove `agent`, `model`, `sessionMode` from the effect's dependency array and read from the refs in the telemetry block.

---

### H-2: Duplicated Job Query Functions -- Identical Structure, Different Tables

**File:** `/home/jmagar/workspace/axon_rust/apps/web/app/api/jobs/route.ts`, lines 99-212
**Severity:** High

Functions `queryCrawl`, `queryExtract`, `queryEmbed`, `queryIngest`, and `queryRefresh` share near-identical structure. Each:
1. Takes `(statusFilter, limit, offset)`
2. Runs `SELECT ... FROM {table} WHERE ${where} ORDER BY created_at DESC LIMIT $1 OFFSET $2`
3. Maps rows to `Job[]` with the same field mapping

The only differences are the table name, the target column (url vs urls_json vs input_text vs source_type+target), and whether collection is present. This is five copies of the same pattern, and any change to the query structure (e.g., adding a new column, changing the sort order) must be replicated five times.

**Recommendation:** Extract a generic query function:

```typescript
interface JobQueryConfig {
  table: string
  type: JobType
  targetExpression: string  // SQL expression for the target column
  collectionExpression: string | null  // SQL expression or null
}

const JOB_TABLES: Record<string, JobQueryConfig> = {
  crawl: { table: 'axon_crawl_jobs', type: 'crawl', targetExpression: 'url', collectionExpression: "config_json->>'collection'" },
  extract: { table: 'axon_extract_jobs', type: 'extract', targetExpression: 'urls_json::text', collectionExpression: null },
  // ...
}

async function queryJobTable(config: JobQueryConfig, statusFilter: StatusFilter, limit: number, offset: number) {
  const where = statusWhere(statusFilter)
  const collectionSelect = config.collectionExpression ? `, ${config.collectionExpression} AS collection` : ''
  const rows = await getJobsPgPool().query(
    `SELECT id, ${config.targetExpression} AS target, status, created_at, started_at, finished_at, error_text${collectionSelect},
            COUNT(*) OVER() AS total
     FROM ${config.table}
     WHERE ${where}
     ORDER BY created_at DESC
     LIMIT $1 OFFSET $2`,
    [limit, offset],
  )
  // ... single row mapper
}
```

---

### H-3: Unbounded Replay Cache in Pulse Chat Route

**File:** `/home/jmagar/workspace/axon_rust/apps/web/app/api/pulse/chat/route.ts`, lines 104-106 and replay-cache imports
**Severity:** High

The `replayCache` (imported from `replay-cache.ts`) is an in-memory `Map` used for reconnect replay. Each chat turn stores its full event buffer in memory. While `pruneReplayCache` exists, it is time-based (evicts entries older than some TTL). Under sustained load -- many concurrent chat sessions, each producing hundreds of events -- this cache can grow without bound until the Next.js process runs out of memory.

There is no maximum entry count guard, no maximum total bytes guard, and no LRU eviction.

**Recommendation:** Add an entry count cap to `pruneReplayCache` or switch to an LRU map:

```typescript
const MAX_REPLAY_ENTRIES = 200

export function pruneReplayCache(now: number): void {
  // Time-based eviction (existing)
  for (const [key, entry] of replayCache) {
    if (now - entry.createdAt > REPLAY_TTL_MS) replayCache.delete(key)
  }
  // Size-based eviction (new)
  if (replayCache.size > MAX_REPLAY_ENTRIES) {
    const sorted = [...replayCache.entries()].sort((a, b) => a[1].createdAt - b[1].createdAt)
    const excess = replayCache.size - MAX_REPLAY_ENTRIES
    for (let i = 0; i < excess; i++) {
      replayCache.delete(sorted[i][0])
    }
  }
}
```

---

### H-4: Reducer/Handler Divergence Risk in WS Message Processing

**File:** `/home/jmagar/workspace/axon_rust/apps/web/hooks/ws-messages/runtime.ts`, lines 98-157
**Severity:** High

`reduceRuntimeState` (the pure reducer used in tests) and `handleWsMessage` (the imperative handler used in production) process the same message types but are maintained separately. The comment on line 101-103 explicitly warns:

```typescript
// IMPORTANT: When updating message handling in handlers.ts, update the
// matching cases here to prevent divergence.
```

This is a textbook maintenance hazard. When someone adds a new message type to `handleWsMessage`, they must remember to also update `reduceRuntimeState`. If they don't, tests pass (because the reducer is what tests exercise) but production behavior diverges.

Today, the two are already partially divergent:
- `handleCommandOutputJson` in `handlers.ts` handles scrape/extract virtual file creation (lines 113-156), but `reduceRuntimeState` only tracks `stdoutJson` and `currentJobId`.
- `handleCommandDone` in `handlers.ts` handles workspace handoff and recent runs, but `reduceRuntimeState` doesn't handle `command.done` at all (no matching case).

**Recommendation:** Either:
1. Make `handleWsMessage` call `reduceRuntimeState` internally and then apply side effects on top of the reduced state, or
2. Delete `reduceRuntimeState` and test the handler directly using mock setters (which is what the handler already accepts).

The current dual-maintenance approach guarantees they will diverge further.

---

### H-5: `process.env` Read at Module Scope Breaks Hot Reload

**File:** `/home/jmagar/workspace/axon_rust/apps/web/lib/axon-ws-exec.ts`, lines 9-14 and `/home/jmagar/workspace/axon_rust/apps/web/lib/api-fetch.ts`, lines 6-7
**Severity:** High

Both files read `process.env` at module scope:

```typescript
// axon-ws-exec.ts
const WORKERS_WS_URL = process.env.AXON_WORKERS_WS_URL ?? ...
const WORKERS_WS_TOKEN = process.env.AXON_WEB_API_TOKEN?.trim() ?? ''

// api-fetch.ts
const API_TOKEN = process.env.NEXT_PUBLIC_AXON_BROWSER_API_TOKEN ?? process.env.NEXT_PUBLIC_AXON_API_TOKEN
```

In `axon-ws-exec.ts`, these are server-side env vars used in API routes. Module-scope reads are cached when the module is first imported, so:
1. Changing env vars at runtime (e.g., via `.env` hot reload in development) has no effect until the process restarts.
2. In serverless/edge deployments, this is fine. In the long-running Docker dev server, this is a source of confusion when debugging connection issues.

The `api-fetch.ts` case is acceptable since `NEXT_PUBLIC_*` vars are inlined at build time by Next.js. But `AXON_WEB_API_TOKEN` in `axon-ws-exec.ts` is a server-side secret that should be read at call time.

**Recommendation:** Change `axon-ws-exec.ts` to read env vars lazily:

```typescript
const getWorkersWsUrl = () =>
  process.env.AXON_WORKERS_WS_URL ??
  process.env.NEXT_PUBLIC_AXON_WS_URL ??
  process.env.AXON_BACKEND_URL?.replace(/^http/i, 'ws').replace(/\/$/, '').concat('/ws') ??
  `ws://127.0.0.1:${process.env.NEXT_PUBLIC_AXON_PORT || '49000'}/ws`

const getWorkersWsToken = () => process.env.AXON_WEB_API_TOKEN?.trim() ?? ''
```

---

## Medium Severity Findings

### M-1: useAxonShellState Returns 60+ Properties as a Flat Object

**File:** `/home/jmagar/workspace/axon_rust/apps/web/components/shell/axon-shell-state.ts`, lines 380-442
**Severity:** Medium

The `useAxonShellState` hook returns a flat object with 60+ properties. This makes it impossible for consumers to understand which group of state a property belongs to, and every property change triggers a re-render of every consumer.

The hook already groups state internally (`layout`, `session`, `messages`, `settings`), but then destructures everything into a flat namespace. `AxonShell.tsx` consumes this via `const shell = useAxonShellState()` and accesses `shell.chatFlex`, `shell.sessionError`, etc. without any indication of which subsystem owns each property.

**Recommendation:** Preserve the grouped structure in the return value:

```typescript
return {
  layout: layout,
  session: { ...session, activeSession, chatTitle },
  messages: { displayMessages, liveMessages: messages.liveMessages },
  settings,
  actions: { composerProps, sidebarProps, handleEditMessage, ... },
  canvas: { canvasRef, handleStats, isStreaming },
  // ...
}
```

This also enables more granular memoization -- a component that only needs layout state won't re-render when messages change.

---

### M-2: Duplicated `createClientId` / `createClientMessageId` Functions

**Files:**
- `/home/jmagar/workspace/axon_rust/apps/web/components/shell/axon-shell-state-helpers.ts`, lines 107-116 (`createClientId`)
- `/home/jmagar/workspace/axon_rust/apps/web/hooks/use-axon-acp.ts`, lines 22-40 (`createFallbackClientId` + `createClientMessageId`)
**Severity:** Medium

These are the same function with slightly different signatures. Both handle the `crypto.randomUUID()` unavailability fallback for non-secure origins. The logic is identical: try `crypto.randomUUID()`, catch, fall back to `Date.now()` + `Math.random()`.

**Recommendation:** Extract to a single utility in `lib/`:

```typescript
// lib/client-id.ts
export function createClientId(prefix?: string): string {
  try {
    if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
      const uuid = crypto.randomUUID()
      return prefix ? `${prefix}-${uuid}` : uuid
    }
  } catch { /* fall through */ }
  const fallback = `${Date.now()}-${Math.random().toString(16).slice(2, 10)}`
  return prefix ? `${prefix}-${fallback}` : fallback
}
```

---

### M-3: `onEditorUpdate` Duplicates localStorage Write

**File:** `/home/jmagar/workspace/axon_rust/apps/web/components/shell/axon-shell-state.ts`, lines 102-118
**Severity:** Medium

The `onEditorUpdate` callback calls both `layout.persistRightPane('editor')` (which internally writes to localStorage) AND then immediately writes to localStorage again on line 107:

```typescript
const onEditorUpdate = useCallback(
  (content: string, operation: 'replace' | 'append') => {
    setEditorMarkdown(...)
    layout.persistRightPane('editor')          // writes to localStorage
    try {
      window.localStorage.setItem(RIGHT_PANE_STORAGE_KEY, 'editor')  // redundant write
    } catch { /* ignore */ }
    layout.setMobilePaneTracked('editor')      // writes to localStorage
    try {
      window.localStorage.setItem(AXON_MOBILE_PANE_STORAGE_KEY, 'editor')  // redundant write
    } catch { /* ignore */ }
  },
  [layout],
)
```

Lines 107-108 and 113-114 are redundant -- they duplicate what `persistRightPane` and `setMobilePaneTracked` already do.

**Recommendation:** Remove the redundant localStorage writes:

```typescript
const onEditorUpdate = useCallback(
  (content: string, operation: 'replace' | 'append') => {
    setEditorMarkdown((prev) => (operation === 'append' ? `${prev}\n${content}` : content))
    layout.persistRightPane('editor')
    layout.setMobilePaneTracked('editor')
  },
  [layout],
)
```

---

### M-4: `mergeHistoricalMessages` Has O(n*m) Worst Case

**File:** `/home/jmagar/workspace/axon_rust/apps/web/components/shell/live-message-sync.ts`, lines 34-102
**Severity:** Medium

The merge function performs a three-pass matching strategy for each historical message:
1. Linear scan by `sourceMessageId` (O(m) per historical message)
2. Index-based comparison (O(1))
3. Fallback linear scan by content equality (O(m) per historical message)

For a conversation with 200 messages (the storage cap), this is fine. But the `normalize()` and `isSemanticallySameContent()` calls involve regex replacement on every comparison in pass 3, and pass 1 also does a `findIndex`. The worst case is O(n*m) where n and m are both 200.

**Recommendation:** Build a lookup map for `sourceMessageId` before iterating:

```typescript
const liveBySourceId = new Map<string, number>()
live.forEach((m, idx) => {
  if (m.sourceMessageId && !liveBySourceId.has(m.sourceMessageId)) {
    liveBySourceId.set(m.sourceMessageId, idx)
  }
})
```

Then pass 1 becomes O(1) per message instead of O(m).

---

### M-5: `handleCommandsUpdate` Uses `.includes()` on Unsorted Array

**File:** `/home/jmagar/workspace/axon_rust/apps/web/components/shell/axon-shell-state-tools.ts`, lines 110-113
**Severity:** Medium

```typescript
setEnabledMcpTools((current) => {
  if (current === null) return allTools
  return current.filter((toolName) => allTools.includes(toolName))
})
```

`allTools` can contain hundreds of tool names across MCP servers. `current.filter(... allTools.includes(...))` is O(n*m). The same pattern appears in the `composerProps.onToggleMcpTool` handler in `axon-shell-state-actions.ts` line 388.

**Recommendation:** Use a `Set` for the lookup:

```typescript
setEnabledMcpTools((current) => {
  if (current === null) return allTools
  const allToolsSet = new Set(allTools)
  return current.filter((toolName) => allToolsSet.has(toolName))
})
```

---

### M-6: `recordAssistantDelta` Pushes Every Delta as a Separate Block

**File:** `/home/jmagar/workspace/axon_rust/apps/web/app/api/pulse/chat/route-helpers.ts`, lines 105-113
**Severity:** Medium

```typescript
export function recordAssistantDelta(parserState, delta, startedAt): void {
  parserState.blocks.push({ type: 'text', content: delta })
  parserState.deltaCount += 1
  parserState.firstDeltaMs ??= Date.now() - startedAt
}
```

Every individual delta token (often 1-5 characters) is pushed as a separate block. A typical assistant response might produce 200-500 deltas, creating 200-500 block entries. These blocks are serialized into the `done` event and cached in the replay buffer.

Compare with `recordThinking`, which correctly appends to the last thinking block when the type matches (line 119-124).

**Recommendation:** Apply the same coalescing pattern:

```typescript
export function recordAssistantDelta(parserState, delta, startedAt): void {
  const lastBlock = parserState.blocks[parserState.blocks.length - 1]
  if (lastBlock?.type === 'text') {
    lastBlock.content += delta
  } else {
    parserState.blocks.push({ type: 'text', content: delta })
  }
  parserState.deltaCount += 1
  parserState.firstDeltaMs ??= Date.now() - startedAt
}
```

This reduces memory usage and serialization overhead by 100-500x for the blocks array.

---

### M-7: Pulse Source Route Spawns Binary via `scripts/axon` Instead of WS Bridge

**File:** `/home/jmagar/workspace/axon_rust/apps/web/app/api/pulse/source/route.ts`, lines 19-87
**Severity:** Medium

The `/api/pulse/source` route spawns the axon binary directly via `child_process.spawn`, while every other command route uses the WS bridge (`runAxonCommandWsStream`). This means:
1. The binary must exist on the Next.js host filesystem (breaks when running web separately from workers).
2. The `process.env` is passed wholesale to the child (`env: process.env`), which leaks all server env vars to the subprocess.
3. There is no abort signal integration -- if the client disconnects, the scrape subprocess runs to completion.
4. The `scripts/axon` path is resolved relative to `getWorkspaceRoot()`, which may not be correct in production.

**Recommendation:** Migrate to the WS bridge pattern used by `/api/pulse/chat`:

```typescript
const result = await runAxonCommandWs('scrape', SOURCE_INDEX_TIMEOUT_MS, urls.join(' '), { json: 'true' })
```

This eliminates the binary dependency, subprocess env leakage, and inconsistency with the rest of the API layer.

---

### M-8: Rate Limiter Uses `x-forwarded-for` Without Trust Verification

**File:** `/home/jmagar/workspace/axon_rust/apps/web/lib/server/rate-limit.ts`, lines 27-36
**Severity:** Medium

```typescript
function getClientIp(request: Request): string {
  const forwarded = request.headers.get('x-forwarded-for')
  if (forwarded) {
    const first = forwarded.split(',')[0]?.trim()
    if (first) return first
  }
  const real = request.headers.get('x-real-ip')?.trim()
  if (real) return real
  return 'unknown'
}
```

The `x-forwarded-for` header is trivially spoofable by any client. Without a reverse proxy that strips/overwrites this header, an attacker can bypass rate limiting by sending a different `x-forwarded-for` value with each request. For a self-hosted setup with Docker Compose, this is the expected scenario -- the Next.js process receives requests directly.

**Recommendation:** Since this is self-hosted and likely behind a known proxy (or direct), add a comment documenting the trust model. If running behind a reverse proxy, strip untrusted headers at the proxy layer. If running directly, fall back to the socket IP (not available via `Request`, but available via Next.js `headers()` or middleware):

```typescript
// IMPORTANT: x-forwarded-for is trusted ONLY when behind a reverse proxy
// that overwrites this header. In direct-access deployments, this provides
// no protection against spoofed IPs.
```

---

## Low Severity Findings

### L-1: Deprecated Type Aliases Without Removal Timeline

**Files:**
- `/home/jmagar/workspace/axon_rust/apps/web/hooks/use-axon-session.ts`, line 28 (`MessageItem`)
- `/home/jmagar/workspace/axon_rust/apps/web/hooks/ws-messages/types.ts`, lines 61-65 (`PulseWorkspaceModel`, `PulseWorkspacePermission`, `PulseWorkspaceAgent`)
**Severity:** Low

Four `@deprecated` type aliases exist with no removal date or migration tracking. These create import confusion -- new code may import the deprecated alias instead of the canonical type.

**Recommendation:** Add a comment with a target removal date (e.g., `@deprecated Since 2026-03-10 -- use AxonMessage directly. Remove after 2026-04-01.`) and grep the codebase for remaining usages.

---

### L-2: Magic Number 200 for Message Storage Cap

**File:** `/home/jmagar/workspace/axon_rust/apps/web/components/shell/axon-shell-state-messages.ts`, line 50
**Severity:** Low

```typescript
const payload = { messages: messages.slice(-200) }
```

The `200` limit is not named or documented. It's also not coordinated with the `MAX_LOG_LINES` (5000) or `MAX_STDOUT_ITEMS` (5000) constants in `runtime.ts`.

**Recommendation:** Extract to a named constant:

```typescript
const MAX_PERSISTED_MESSAGES = 200
```

---

### L-3: `handleStats` in Actions is a No-Op

**File:** `/home/jmagar/workspace/axon_rust/apps/web/components/shell/axon-shell-state-actions.ts`, lines 444-446
**Severity:** Low

```typescript
const handleStats = useCallback((_data: unknown) => {
  // DockerStats manages its own state; this hook is reserved for future use
}, [])
```

This is dead code that's still wired into the return value (line 458) and consumed in `axon-shell-state.ts`. The actual `handleStats` that drives the neural canvas is defined in `axon-shell-state.ts` (line 166-179) and shadows this one. The one from actions is never used.

**Recommendation:** Remove the dead `handleStats` from `useAxonShellActions` and its return value.

---

### L-4: `POST` Handler for `/api/jobs` Returns 200 Instead of 501

**File:** `/home/jmagar/workspace/axon_rust/apps/web/app/api/jobs/route.ts`, lines 356-361
**Severity:** Low

```typescript
export async function POST(): Promise<NextResponse> {
  return NextResponse.json(
    { ok: false, message: 'Cancel not yet supported from UI' },
    { status: 200 },
  )
}
```

A `200 OK` response for an unimplemented endpoint is misleading. The client has no way to distinguish this from a successful response without parsing the body.

**Recommendation:** Use `501 Not Implemented`:

```typescript
export async function POST(): Promise<NextResponse> {
  return apiError(501, 'Cancel not yet supported from UI', { code: 'not_implemented' })
}
```

---

### L-5: Mobile Pane Switcher Validates Pane Names via Long If-Chain

**File:** `/home/jmagar/workspace/axon_rust/apps/web/components/shell/axon-shell-state-layout.ts`, lines 76-88
**Severity:** Low

```typescript
if (
  saved === 'sidebar' ||
  saved === 'chat' ||
  saved === 'editor' ||
  saved === 'terminal' ||
  saved === 'logs' ||
  saved === 'mcp' ||
  saved === 'settings' ||
  saved === 'cortex'
) {
  setMobilePane(saved as AxonMobilePane)
}
```

The `AxonMobilePane` type and `VALID_RIGHT_PANES` set already exist in the helpers file. This validation is hand-rolled instead of reusing those definitions.

**Recommendation:** Create a `VALID_MOBILE_PANES` set in the helpers file and use it:

```typescript
const VALID_MOBILE_PANES = new Set<string>([
  'sidebar', 'chat', 'editor', 'terminal', 'logs', 'mcp', 'settings', 'cortex',
])

// In layout:
if (saved && VALID_MOBILE_PANES.has(saved)) {
  setMobilePane(saved as AxonMobilePane)
}
```

---

## Architectural Observations

These are not bugs or debt, but structural observations worth noting for future planning.

### The Shell State Split is Good -- Protect It

The decomposition of `useAxonShellState` into `layout`, `session`, `messages`, `settings`, `tools`, and `actions` modules is the right call. It keeps each module under 150 lines and separable. The risk is that `axon-shell-state.ts` (the orchestrator) continues to grow as it wires everything together. At 443 lines, it's approaching the point where it needs its own split -- perhaps separating the "effects" (lines 146-299) from the "composition" (lines 37-142 and 300-442).

### WebSocket Architecture is Sound

The `useAxonWs` -> `useWsMessages` -> `handleWsMessage` pipeline is clean. The separation between the transport layer (connect/reconnect/queue), the message routing layer (subscribe/dispatch), and the state update layer (setters) is well-defined. The pending message queue with cap (MAX_PENDING_MESSAGES = 100) prevents unbounded memory growth during disconnects.

### SSRF Protection is Thorough

`url-validation.ts` handles IPv4, IPv6, IPv6-mapped IPv4, ULA, link-local, and multicast ranges. The DNS rebinding limitation is documented. This is above-average for application-level SSRF protection.

### The Session Cache is Well-Designed

`session-cache.ts` implements stale-while-revalidate with thundering-herd protection via in-flight promise deduplication. The pattern is correct and the implementation is clean.
