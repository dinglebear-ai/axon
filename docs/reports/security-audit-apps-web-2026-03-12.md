# Security Audit: apps/web (Axon Next.js Frontend)

**Date:** 2026-03-12
**Auditor:** Security Audit (DevSecOps)
**Scope:** `/home/jmagar/workspace/axon_rust/apps/web/`
**Application:** Next.js 16.1.6 App Router frontend for Axon RAG system
**Branch:** `feat/github-code-aware-chunking`

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Critical Findings](#critical-findings)
3. [High Severity Findings](#high-severity-findings)
4. [Medium Severity Findings](#medium-severity-findings)
5. [Low Severity Findings](#low-severity-findings)
6. [Positive Security Controls](#positive-security-controls)
7. [Remediation Priority Matrix](#remediation-priority-matrix)

---

## Executive Summary

The `apps/web/` frontend demonstrates several mature security practices: constant-time token comparison, SSRF validation with IPv6 coverage, CSP headers, rate limiting with anti-spoofing protections, Zod validation on API boundaries, and an allowlisted shell environment. However, the audit identified **3 critical**, **4 high**, **6 medium**, and **5 low** severity findings that should be addressed.

The most urgent issues are: subprocess environment leakage in the Pulse source route, the SQL interpolation pattern in the jobs API (mitigated but fragile), and the shell server's non-constant-time token comparison which is exploitable on the same network.

**Overall Risk Rating:** **Medium-High** -- The application has solid foundations but several gaps that would be exploitable by an attacker with network access to the self-hosted deployment.

---

## Critical Findings

### C-1: Subprocess Inherits Full `process.env` (CWE-200: Exposure of Sensitive Information)

**Severity:** Critical | **CVSS 3.1:** 7.5 (High)
**File:** `app/api/pulse/source/route.ts:28`

The `runAxonScrape()` function spawns a child process with `env: process.env`, passing the entire server environment -- including `AXON_WEB_API_TOKEN`, database credentials (`AXON_PG_URL`), Redis credentials (`AXON_REDIS_URL`), `OPENAI_API_KEY`, and any other secret loaded via `ensureRepoRootEnvLoaded()`.

```typescript
const child = spawn(commandPath, args, {
  cwd: repoRoot,
  env: process.env,  // <-- leaks ALL secrets to subprocess
  stdio: ['ignore', 'pipe', 'pipe'],
})
```

**Attack scenario:** If the `axon scrape` binary has a vulnerability (or if the scraped URL returns content that triggers a code path writing to stdout/stderr), secrets could leak through the subprocess output. The subprocess output is returned to the HTTP client on line 125: `output: result.output.slice(0, 6000)`. A compromised or malicious binary at the `scripts/axon` path would have full access to all credentials.

**Remediation:**
```typescript
const SAFE_CHILD_ENV_KEYS = [
  'HOME', 'PATH', 'SHELL', 'LANG', 'TERM', 'USER',
  'QDRANT_URL', 'TEI_URL', 'AXON_PG_URL', 'AXON_REDIS_URL',
  'AXON_AMQP_URL', 'AXON_COLLECTION', 'AXON_CHROME_REMOTE_URL',
  // Only keys the axon binary actually needs
]

function buildChildEnv(): Record<string, string> {
  const env: Record<string, string> = {}
  for (const key of SAFE_CHILD_ENV_KEYS) {
    if (process.env[key]) env[key] = process.env[key]!
  }
  return env
}

const child = spawn(commandPath, args, {
  cwd: repoRoot,
  env: buildChildEnv(),
  stdio: ['ignore', 'pipe', 'pipe'],
})
```

---

### C-2: Shell Server Token Comparison is Timing-Vulnerable (CWE-208: Observable Timing Discrepancy)

**Severity:** Critical | **CVSS 3.1:** 7.4 (High)
**File:** `shell-server.mjs:120`

The shell server uses JavaScript's `===` operator for token comparison, which is vulnerable to timing attacks. An attacker on the same network can measure response times to progressively discover the token character by character.

```javascript
function isAuthorized(req) {
  if (TOKEN) return getAuthToken(req) === TOKEN  // <-- timing-vulnerable
  // ...
}
```

This is particularly dangerous because the shell server provides full PTY access -- successful authentication gives arbitrary command execution on the host.

**Remediation:**
```javascript
import { timingSafeEqual } from 'node:crypto'

function constantTimeEqual(a, b) {
  if (typeof a !== 'string' || typeof b !== 'string') return false
  if (a.length !== b.length) return false
  return timingSafeEqual(Buffer.from(a, 'utf-8'), Buffer.from(b, 'utf-8'))
}

function isAuthorized(req) {
  if (TOKEN) return constantTimeEqual(getAuthToken(req), TOKEN)
  // ...
}
```

---

### C-3: Subprocess Output Returned to Client Contains Internal Details (CWE-209: Generation of Error Message Containing Sensitive Information)

**Severity:** Critical | **CVSS 3.1:** 6.5 (Medium)
**File:** `app/api/pulse/source/route.ts:117-125`

Both the error path and success path return raw subprocess output (up to 6000 chars) to the HTTP client. This output includes stderr, which may contain stack traces, file paths, database connection strings, and internal infrastructure details.

```typescript
// Error path:
return apiError(502, 'Source indexing failed', {
  detail: result.output.slice(0, 6000),  // includes stderr
})

// Success path:
return NextResponse.json({
  indexed: urls,
  command: `./scripts/axon scrape ${urls.join(' ')} --json`, // exposes internal script path
  output: result.output.slice(0, 6000),  // includes combined stdout+stderr
  markdownBySrc: result.markdownBySrc,
})
```

**Attack scenario:** An attacker submits a URL that causes the scraper to fail. The error output reveals internal file paths, dependency versions, network topology (e.g., `axon-postgres:5432`), or even partial credentials from misconfigured logging.

**Remediation:**
```typescript
// Never return raw stderr to the client
if (!result.ok) {
  console.error('[pulse/source] scrape failed:', result.output.slice(0, 2000))
  return apiError(502, 'Source indexing failed', {
    code: 'source_index_failed',
  })
}

return NextResponse.json({
  indexed: urls,
  markdownBySrc: result.markdownBySrc,
  // Omit: command, raw output
})
```

---

## High Severity Findings

### H-1: SQL Interpolation Pattern is Fragile (CWE-89: SQL Injection)

**Severity:** High | **CVSS 3.1:** 6.3 (Medium)
**File:** `app/api/jobs/route.ts:80-97, 217-224`

The `statusWhere()` function returns raw SQL fragments that are string-interpolated into queries at 7 call sites. While the current implementation uses a switch statement with hardcoded SQL strings (making injection impossible today), the pattern is architecturally fragile. The `getStatusCounts()` function on line 224 also interpolates table names directly:

```typescript
const countSql = (table: string) =>
  getJobsPgPool().query<...>(
    `SELECT ... FROM ${table}`,  // table name interpolated
  )
```

The table names are hardcoded constants on lines 226-231, so this is not exploitable today. However, a future developer adding a new job type or status filter could introduce SQL injection if they follow the established pattern without understanding the implicit safety contract.

**Remediation:** Use parameterized queries throughout. For the status filter, pass the status values as query parameters instead of interpolating SQL fragments:

```typescript
function statusParams(filter: StatusFilter): { clause: string; values: string[] } {
  switch (filter) {
    case 'active':
      return { clause: 'status = ANY($3::text[])', values: ['pending', 'running'] }
    case 'failed':
      return { clause: 'status = ANY($3::text[])', values: ['failed', 'canceled'] }
    case 'all':
      return { clause: 'TRUE', values: [] }
    default:
      return { clause: 'status = $3', values: [filter] }
  }
}
```

---

### H-2: PgPool Has No Connection Limits or Timeouts (CWE-400: Uncontrolled Resource Consumption)

**Severity:** High | **CVSS 3.1:** 6.5 (Medium)
**File:** `lib/server/pg-pool.ts:11-17`

The PostgreSQL pool is created with only a connection string -- no connection limit, idle timeout, connection timeout, or statement timeout. The `pg` library defaults to 10 max connections with no timeouts.

```typescript
function createPool(): Pool {
  const connectionString =
    process.env.AXON_PG_URL ?? process.env.AXON_PG_MCP_URL ?? DEFAULT_AXON_PG_URL
  return new Pool({
    connectionString,
    // No max, connectionTimeoutMillis, idleTimeoutMillis, or statement_timeout
  })
}
```

**Attack scenario:** A slow or malicious query on the jobs API (e.g., requesting `type=all` with a large UNION across 5 tables) could exhaust all pool connections, blocking all other database operations until the pool's internal queue fills up.

**Remediation:**
```typescript
function createPool(): Pool {
  return new Pool({
    connectionString,
    max: 5,
    connectionTimeoutMillis: 5_000,
    idleTimeoutMillis: 30_000,
    statement_timeout: 15_000,
    query_timeout: 30_000,
  })
}
```

---

### H-3: MCP Config Write Protected Only by Trivially Spoofable Header (CWE-287: Improper Authentication)

**Severity:** High | **CVSS 3.1:** 6.5 (Medium)
**File:** `app/api/mcp/route.ts:68, 102`

The MCP config PUT and DELETE endpoints are protected by checking `X-Pulse-Request: 1` header, which is trivially spoofable by any client that can reach the API:

```typescript
if (request.headers.get('X-Pulse-Request') !== '1') {
  return NextResponse.json({ error: 'Forbidden' }, { status: 403 })
}
```

While `proxy.ts` enforces token auth on all `/api/*` routes (so unauthenticated users cannot reach this endpoint), the `X-Pulse-Request` check provides zero additional security beyond what `proxy.ts` already provides. Any authenticated user can modify the MCP server configuration, including:
- Adding a malicious MCP server command that will be executed by the ACP adapter
- Modifying environment variables passed to MCP server processes
- Pointing MCP URLs to attacker-controlled servers

**Attack scenario:** An authenticated user (or an attacker who obtains the API token) can register a malicious MCP server. When the ACP adapter connects to it, the attacker gains code execution within the adapter's context.

**Remediation:** The `X-Pulse-Request` header check should be removed (it provides false security) or replaced with a proper CSRF protection mechanism:
```typescript
// Option 1: Remove the header check entirely (proxy.ts auth is sufficient for single-user)
// Option 2: Add a proper CSRF token if multi-user support is planned
// Option 3: Add command validation against a stricter allowlist
```

For the MCP server `command` field, consider restricting to a known set of safe binaries rather than accepting arbitrary paths.

---

### H-4: Pulse Doc Endpoint Vulnerable to Path Traversal (CWE-22: Path Traversal)

**Severity:** High | **CVSS 3.1:** 5.3 (Medium)
**File:** `app/api/pulse/doc/route.ts:9-11`

The `/api/pulse/doc?filename=...` endpoint passes the filename query parameter directly to `loadPulseDoc()` without validation:

```typescript
const filename = url.searchParams.get('filename')
if (filename) {
  const doc = await loadPulseDoc(filename)
```

The `loadPulseDoc()` function in `lib/pulse/storage.ts:178` does use `path.basename()`:
```typescript
export async function loadPulseDoc(filename: string): Promise<StoredDoc | null> {
  const safeName = path.basename(filename)
  const fullPath = path.join(PULSE_DIR, safeName)
```

The `path.basename()` call strips directory components, which prevents `../../etc/passwd` traversal. However, the defense is buried in the storage layer rather than at the API boundary. The `/api/pulse/save` route validates filenames with a Zod regex (`/^[a-z0-9-]+-\d+\.md$/`), but the doc GET route has no such validation.

**Remediation:** Add input validation at the API boundary:
```typescript
const SAFE_FILENAME_RE = /^[a-z0-9-]+-\d+\.md$/
const filename = url.searchParams.get('filename')
if (filename) {
  if (!SAFE_FILENAME_RE.test(filename)) {
    return NextResponse.json({ error: 'Invalid filename' }, { status: 400 })
  }
  const doc = await loadPulseDoc(filename)
```

---

## Medium Severity Findings

### M-1: Rate Limiting Not Applied to Most Routes (CWE-770: Allocation of Resources Without Limits)

**Severity:** Medium | **CVSS 3.1:** 5.3 (Medium)
**File:** Various API routes

Rate limiting is applied to only 4 of 14+ API routes:
- `/api/pulse/chat` -- 40/min
- `/api/pulse/save` -- 20/min
- `/api/sessions/list` -- 30/min
- `/api/sessions/[id]` -- 60/min

The following routes have **no rate limiting**:
- `/api/jobs` -- Direct database queries (UNION across 5 tables)
- `/api/cortex/stats` -- Backend WS bridge call
- `/api/cortex/domains` -- Backend WS bridge call
- `/api/cortex/sources` -- Backend WS bridge call
- `/api/cortex/doctor` -- Infrastructure health probe
- `/api/cortex/status` -- Backend WS bridge call
- `/api/cortex/suggest` -- Backend WS bridge call
- `/api/pulse/doc` -- File system read
- `/api/pulse/source` -- Spawns subprocess + blocks for up to 8 minutes
- `/api/pulse/config` -- Spawns ACP adapter lifecycle
- `/api/ai/command` -- LLM API call ($$)
- `/api/ai/copilot` -- LLM API call ($$)
- `/api/mcp` -- File system read/write + network probes (status)

The `/api/pulse/source` and `/api/ai/command` routes are particularly concerning: the source route spawns a blocking subprocess for up to 8 minutes, and the AI routes incur external API costs.

**Remediation:** Apply rate limiting to all API routes, with stricter limits on expensive operations:
```typescript
// In each route handler:
const limited = enforceRateLimit('api.pulse.source', request, { max: 5, windowMs: 60_000 })
if (limited) return limited

const limited = enforceRateLimit('api.ai.command', request, { max: 20, windowMs: 60_000 })
if (limited) return limited
```

---

### M-2: `x-forwarded-for` Trusted Without Verification (CWE-346: Origin Validation Error)

**Severity:** Medium | **CVSS 3.1:** 5.3 (Medium)
**File:** `lib/server/rate-limit.ts:27-36`

The rate limiter trusts the first IP in the `x-forwarded-for` header without any verification that the request actually passed through a trusted reverse proxy:

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

**Attack scenario:** An attacker sends requests with a rotating `X-Forwarded-For` header, each containing a different IP address. This completely bypasses rate limiting because each request appears to come from a unique client.

Since this is self-hosted behind Docker, the mitigation depends on the deployment architecture:

**Remediation:** If behind a reverse proxy (e.g., Caddy, Traefik):
```typescript
function getClientIp(request: Request): string {
  // Only trust x-forwarded-for if the request came from a trusted proxy
  // For self-hosted single-user: just use a fixed key
  const forwarded = request.headers.get('x-forwarded-for')
  if (forwarded) {
    // Take the LAST IP in the chain (closest to the reverse proxy)
    // or better: use the connecting IP if available
    const parts = forwarded.split(',')
    const clientIp = parts[parts.length - 1]?.trim()
    if (clientIp) return clientIp
  }
  return request.headers.get('x-real-ip')?.trim() ?? 'unknown'
}
```

---

### M-3: CSP Allows `'unsafe-inline'` for Scripts in Production (CWE-79: Cross-Site Scripting)

**Severity:** Medium | **CVSS 3.1:** 4.7 (Medium)
**File:** `next.config.ts:47`, `proxy.ts:56-57`

Both the `next.config.ts` headers and `proxy.ts` CSP include `'unsafe-inline'` for script-src in production:

```typescript
// next.config.ts
`script-src 'self' 'unsafe-inline'${isDev ? " 'unsafe-eval'" : ''}`,

// proxy.ts
IS_DEV
  ? "script-src 'self' 'unsafe-inline' 'unsafe-eval'"
  : "script-src 'self' 'unsafe-inline'",
```

While `'unsafe-inline'` is often necessary for Next.js (inline script tags for hydration data), it weakens XSS protection by allowing attacker-injected inline scripts to execute.

**Remediation:** Use nonce-based CSP. Next.js 16 supports this natively:
```typescript
// In next.config.ts experimental section:
experimental: {
  cspNonce: true,
}
// Then use 'nonce-{nonce}' instead of 'unsafe-inline' for script-src
```

---

### M-4: Next.js Image Optimization Allows All Remote Hosts (CWE-918: SSRF)

**Severity:** Medium | **CVSS 3.1:** 4.3 (Medium)
**File:** `next.config.ts:79-88`

The Next.js image optimization configuration allows loading images from any hostname over HTTP or HTTPS:

```typescript
images: {
  remotePatterns: [
    { protocol: 'https', hostname: '**' },
    { protocol: 'http', hostname: '**' },
  ],
},
```

This makes the Next.js image optimization endpoint (`/_next/image?url=...`) an open SSRF proxy. An attacker can use it to probe internal services by requesting images from internal hostnames (e.g., `/_next/image?url=http://axon-postgres:5432/&w=1&q=1`).

**Remediation:** Restrict to known domains or disable entirely if unused:
```typescript
images: {
  remotePatterns: [],
  // Or if remote images are needed:
  // remotePatterns: [{ protocol: 'https', hostname: 'your-cdn.example.com' }],
},
```

---

### M-5: Wire Format Trust in ACP Event Handling (CWE-20: Improper Input Validation)

**Severity:** Medium | **CVSS 3.1:** 4.3 (Medium)
**File:** `hooks/use-axon-acp.ts:201-488`

Most ACP event types are consumed via raw `as` casts without Zod validation. The `editor_update` event (line 440-442) is properly validated via the `EditorUpdateSchema`, but all other event types trust the wire format:

```typescript
case 'assistant_delta': {
  const delta = (msg.delta as string) ?? ''       // raw cast
  const usage = msg.usage as WsUsageStats | undefined  // raw cast
  const locations = msg.tool_locations as string[] | undefined  // raw cast
  // ...
}

case 'tool_use': {
  const toolCallId = (msg.tool_call_id as string) ?? ''   // raw cast
  const toolName = (msg.tool_name as string) ?? 'unknown'  // raw cast
  const toolInput = (msg.tool_input as Record<string, unknown>) ?? {}  // raw cast
  // ...
}
```

While this is client-side code (limiting the blast radius to the user's own browser), a compromised or malfunctioning backend WebSocket could inject unexpected data types that cause runtime errors or unexpected UI behavior.

**Remediation:** Add Zod schemas for all ACP event types, mirroring the pattern established for `editor_update`:
```typescript
const AssistantDeltaSchema = z.object({
  type: z.literal('assistant_delta'),
  delta: z.string().default(''),
  usage: WsUsageStatsSchema.optional(),
  tool_locations: z.array(z.string()).optional(),
})
```

---

### M-6: Duplicate CSP Definitions (CWE-1188: Insecure Default Initialization)

**Severity:** Medium | **CVSS 3.1:** 3.7 (Low)
**File:** `next.config.ts:34-57` and `proxy.ts:41-62`

CSP headers are defined in two separate locations (`next.config.ts` headers and `proxy.ts` security headers) with slightly different values:

- `next.config.ts` CSP includes `"img-src 'self' data: blob: https:"` (allows https images)
- `proxy.ts` CSP includes `"img-src 'self' data: blob:"` (no https images)
- `next.config.ts` CSP includes `"form-action 'self'"` -- `proxy.ts` does not
- `next.config.ts` applies to all routes (`/:path*`), `proxy.ts` only to `/api/:path*`

When both are active, the browser receives two `Content-Security-Policy` headers. Per the CSP spec, the browser enforces the **intersection** (most restrictive) of both policies. This is likely unintentional and creates confusing behavior.

**Remediation:** Consolidate CSP into a single location. Since `proxy.ts` handles `/api/*` routes and `next.config.ts` handles all routes, remove the CSP from whichever is less appropriate (likely `proxy.ts`, since API responses don't render HTML).

---

## Low Severity Findings

### L-1: Module-Scope Secret Caching (CWE-798: Use of Hard-coded Credentials)

**Severity:** Low | **CVSS 3.1:** 3.7 (Low)
**File:** `lib/axon-ws-exec.ts:9-14`, `proxy.ts:9-10`, `lib/api-fetch.ts:6-7`

Multiple files cache secrets at module scope (import time). While this is standard Next.js practice (environment variables are read once at startup), it means:
1. Token rotation requires a full server restart
2. If a module is accidentally included in a client bundle, the token would leak

```typescript
// axon-ws-exec.ts
const WORKERS_WS_TOKEN = process.env.AXON_WEB_API_TOKEN?.trim() ?? ''

// proxy.ts
const API_TOKEN = process.env.AXON_WEB_API_TOKEN?.trim() || null

// api-fetch.ts (client-side -- intentional)
const API_TOKEN = process.env.NEXT_PUBLIC_AXON_BROWSER_API_TOKEN ?? process.env.NEXT_PUBLIC_AXON_API_TOKEN
```

The `api-fetch.ts` case is intentional (client-side token injection). The server-side cases in `axon-ws-exec.ts` and `proxy.ts` are acceptable for the current single-process deployment but would need refactoring for token rotation support.

**Remediation:** Document that token changes require server restart. For future-proofing, consider reading tokens lazily:
```typescript
function getApiToken(): string {
  return process.env.AXON_WEB_API_TOKEN?.trim() ?? ''
}
```

---

### L-2: WebSocket Token in URL Query String (CWE-598: Use of GET Request Method With Sensitive Query Strings)

**Severity:** Low | **CVSS 3.1:** 3.1 (Low)
**File:** `hooks/use-axon-ws.ts:89`, `lib/axon-ws-exec.ts:20-21`

Authentication tokens are passed as URL query parameters in WebSocket connections:

```typescript
// use-axon-ws.ts (client)
const wsUrl = wsToken ? `${base}?token=${encodeURIComponent(wsToken)}` : base

// axon-ws-exec.ts (server)
url.searchParams.set('token', WORKERS_WS_TOKEN)
```

Tokens in URL query strings can appear in browser history, server access logs, proxy logs, and HTTP Referer headers. However, for WebSocket upgrade requests, the URL is used only in the initial HTTP upgrade handshake and is not logged by most proxies.

**Remediation:** This is a known limitation of the WebSocket protocol (you cannot set custom headers on the browser `WebSocket` constructor). The current approach is the standard workaround. Ensure that any reverse proxy logs are configured to redact query parameters.

---

### L-3: Redis Client Has No Authentication Timeout (CWE-400: Uncontrolled Resource Consumption)

**Severity:** Low | **CVSS 3.1:** 3.1 (Low)
**File:** `lib/server/redis-client.ts:17-41`

The Redis client is created without connection timeout, retry strategy, or TLS configuration:

```typescript
client = createClient({ url: redisUrl })
```

If the Redis server is unreachable, the client will hang indefinitely on the first connection attempt, potentially blocking request processing.

**Remediation:**
```typescript
client = createClient({
  url: redisUrl,
  socket: {
    connectTimeout: 5_000,
    reconnectStrategy: (retries) => Math.min(retries * 500, 5_000),
  },
})
```

---

### L-4: Insecure Dev Bypass Flag (CWE-489: Active Debug Code)

**Severity:** Low | **CVSS 3.1:** 2.4 (Low)
**File:** `proxy.ts:15`, `shell-server.mjs:41`

The `AXON_WEB_ALLOW_INSECURE_DEV=true` flag completely bypasses authentication for localhost requests. While this is documented and defaults to `false`, if accidentally set in production, it would expose the full API and shell server without authentication.

**Remediation:** Add a startup warning:
```typescript
if (ALLOW_INSECURE_LOCAL_DEV) {
  console.warn(
    '[SECURITY] AXON_WEB_ALLOW_INSECURE_DEV=true — authentication is DISABLED for localhost. ' +
    'Do NOT use this setting in production.'
  )
}
```

---

### L-5: Config Probe Cache Has No Size Bound (CWE-400: Uncontrolled Resource Consumption)

**Severity:** Low | **CVSS 3.1:** 2.4 (Low)
**File:** `app/api/pulse/config/route.ts:19-23`

The `CONFIG_CACHE` Map and `IN_FLIGHT` Map have TTL-based eviction (60s, on access), but no maximum entry count:

```typescript
const CONFIG_CACHE = new Map<string, { options: ...; expires: number }>()
```

While the cache key space is limited by the `agent:model:sessionId` combination, a large number of unique session IDs could grow this map. The TTL eviction on line 108-110 only fires during cache writes, not on reads.

**Remediation:** Add a size cap similar to the replay cache:
```typescript
const CONFIG_CACHE_MAX_ENTRIES = 100
// In the eviction loop:
while (CONFIG_CACHE.size > CONFIG_CACHE_MAX_ENTRIES) {
  const oldest = CONFIG_CACHE.keys().next()
  if (oldest.done) break
  CONFIG_CACHE.delete(oldest.value)
}
```

---

## Positive Security Controls

The audit identified several well-implemented security measures that should be maintained:

1. **Constant-time token comparison** (`proxy.ts:135-143`) -- Uses `crypto.timingSafeEqual` with proper length pre-check. This is textbook correct.

2. **SSRF validation with IPv6 coverage** (`lib/server/url-validation.ts`) -- Thorough implementation covering IPv4 private ranges, IPv6 ULA/link-local/multicast, IPv6-mapped IPv4, and known loopback hostnames. The documented DNS rebinding caveat is honest and appropriate.

3. **Zod validation on API boundaries** -- `PulseChatRequestSchema`, `PulseSourceRequestSchema`, `SaveRequestSchema`, `McpServerConfigSchema`, `PulseConfigProbeRequestSchema` all use Zod with appropriate size limits (`max(8000)`, `max(100_000)`, `max(200)`).

4. **Rate limiter with anti-spoofing** (`lib/server/rate-limit.ts:54`) -- The `MAX_COUNTER_KEYS` cap with spoofed-IP rejection at capacity is a smart defense against IP rotation attacks exhausting the counter map.

5. **Replay cache with bounded memory** (`app/api/pulse/chat/replay-cache.ts`) -- Multiple bounds: `REPLAY_BUFFER_LIMIT=512` events, `REPLAY_CACHE_MAX_ENTRIES=64`, `REPLAY_CACHE_MAX_TOTAL_BYTES=8MB`, and `REPLAY_CACHE_TTL_MS=2min`. This effectively prevents the DoS vector identified in the Phase 1 review.

6. **Shell server environment allowlist** (`shell-server.mjs:42-54`) -- Only `HOME`, `PATH`, `SHELL`, `LANG`, `LC_ALL`, `LC_CTYPE`, `TZ`, `TMPDIR`, `PWD`, `USER`, `USERNAME` are passed to the PTY child. Secrets are excluded.

7. **Path traversal prevention** (`lib/pulse/storage.ts:128,178`) -- `path.basename()` is used consistently in `updatePulseDoc` and `loadPulseDoc` to strip directory components.

8. **CSP headers** -- Frame-ancestors `'none'`, object-src `'none'`, base-uri `'self'`, X-Frame-Options DENY, X-Content-Type-Options nosniff, HSTS in production.

9. **Session ID validation** -- `app/api/sessions/[id]/route.ts:23` validates IDs with `/^[\w.@:-]{1,255}$/` before any filesystem operation.

10. **MCP command validation** -- `app/api/mcp/route.ts:10-12` rejects path traversal in MCP server commands via `/^(?!.*\.\.)([/a-zA-Z0-9._-]+)$/`.

---

## Remediation Priority Matrix

| Priority | Finding | Effort | Impact |
|----------|---------|--------|--------|
| **P0 (Now)** | C-2: Shell server timing attack | 15 min | Prevents token theft for remote code execution |
| **P0 (Now)** | C-1: Subprocess env leakage | 30 min | Prevents credential exposure via subprocess output |
| **P1 (This sprint)** | C-3: Raw output to client | 20 min | Prevents information disclosure |
| **P1 (This sprint)** | H-2: PgPool limits | 10 min | Prevents connection exhaustion DoS |
| **P1 (This sprint)** | H-4: Doc endpoint path validation | 10 min | Defense-in-depth for path traversal |
| **P2 (Next sprint)** | H-1: SQL interpolation refactor | 2 hrs | Eliminates fragile SQL pattern |
| **P2 (Next sprint)** | M-1: Rate limiting gaps | 1 hr | Prevents DoS on expensive routes |
| **P2 (Next sprint)** | M-2: x-forwarded-for trust | 30 min | Prevents rate limit bypass |
| **P2 (Next sprint)** | M-4: Image SSRF | 5 min | Closes SSRF via image optimizer |
| **P3 (Backlog)** | H-3: MCP header auth | 1 hr | Improves MCP config security |
| **P3 (Backlog)** | M-3: CSP unsafe-inline | 2 hrs | Strengthens XSS protection |
| **P3 (Backlog)** | M-5: ACP wire validation | 3 hrs | Improves client-side robustness |
| **P3 (Backlog)** | M-6: Duplicate CSP | 30 min | Reduces confusion |
| **P4 (Opportunistic)** | L-1 through L-5 | Various | Minor hardening |

---

*End of security audit report.*
