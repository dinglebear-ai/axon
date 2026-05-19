# Web Frontend Test Coverage Audit

**Date:** 2026-03-12
**Scope:** `/home/jmagar/workspace/axon_rust/apps/web/`
**Test Runner:** Vitest 4, node environment
**Current State:** 76 test files, 800 tests, all passing (4.95s runtime)
**Total Test Code:** ~18,300 lines

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Test Pyramid Analysis](#test-pyramid-analysis)
3. [Security-Critical Path Coverage](#security-critical-path-coverage)
4. [Performance-Critical Path Coverage](#performance-critical-path-coverage)
5. [Core Functionality Coverage](#core-functionality-coverage)
6. [Test Quality Assessment](#test-quality-assessment)
7. [Findings by Severity](#findings-by-severity)

---

## Executive Summary

The `apps/web/` test suite is strong for a self-hosted dev project: 800 tests, fast runtime, zero flakes, and good coverage of pure logic modules. The SSRF validation and MCP route tests are particularly thorough. However, there are critical gaps in security-sensitive areas (shell server auth, rate limiter, doc endpoint path validation) and no tests at all for the entire Zustand shell state layer (8 files). The test pyramid is bottom-heavy by design (node environment, no jsdom/browser), which is appropriate for a logic-first frontend, but it means UI integration paths are untested.

**Strengths:**
- SSRF validation: 25+ tests covering IPv4/IPv6/mapped/ULA/link-local/multicast
- MCP route: Full CRUD coverage with Zod schema validation, CSRF header gate, path traversal, and SSRF on server URLs
- API route coverage: Every API route directory has at least partial test coverage
- Pure logic extraction: Security-critical functions are exported and tested independently of HTTP plumbing
- Test isolation: Proper `vi.clearAllMocks()`, `vi.resetModules()`, and fake timers
- Zero snapshot abuse: Only 1 snapshot test file (omnibox-snapshot)

**Weaknesses:**
- Shell server (`shell-server.mjs`): Zero tests for auth, origin validation, PTY env filtering
- Rate limiter (`lib/server/rate-limit.ts`): Zero direct tests for IP spoofing, eviction, counter overflow
- Doc endpoint (`api/pulse/doc/route.ts`): No path traversal tests (relies on `storage.ts` `path.basename()`)
- Shell state (8 `axon-shell-state-*.ts` files): Zero tests
- 13 hooks untested, 28 lib files untested
- No E2E or integration tests that exercise the full request pipeline

---

## Test Pyramid Analysis

| Layer | Count | Notes |
|-------|-------|-------|
| **Unit (pure logic)** | ~750 | Excellent coverage of parsers, validators, normalizers |
| **Unit (mocked I/O)** | ~50 | MCP route, pulse-save, session-cache, cortex routes |
| **Integration** | 0 | No tests exercise real HTTP handlers end-to-end |
| **E2E** | 0 | No Playwright or browser-based tests |

The pyramid is intentionally bottom-heavy. The `node` environment (not jsdom) means React component tests are limited to snapshot/type-level checks. This is reasonable for a single-developer homelab project, but it means the integration layer between components is untested.

---

## Security-Critical Path Coverage

### 1. Shell Server Token Comparison -- CRITICAL

**File:** `shell-server.mjs:120`
**Issue:** Uses `===` for token comparison instead of `crypto.timingSafeEqual()`
**Test Coverage:** Zero. No test file exists for `shell-server.mjs`.

This is the highest-severity gap. The shell server grants PTY access -- a full shell on the host machine. Token comparison via `===` is vulnerable to timing attacks that leak the token byte-by-byte. There are also no tests for origin validation, the env allowlist, or the upgrade handshake rejection paths.

**Recommendation:**

```typescript
// __tests__/shell-server.test.ts
import { describe, expect, it } from 'vitest'

// Extract pure functions from shell-server.mjs for testability:
// isAuthorized, isAllowedOrigin, getAuthToken, buildShellEnv

describe('shell-server auth', () => {
  it('rejects empty token when TOKEN is set', () => {
    // isAuthorized({headers:{}}) with TOKEN='secret' => false
  })

  it('rejects wrong token', () => {
    // isAuthorized with wrong token => false
  })

  it('accepts correct token via Bearer header', () => {
    // isAuthorized with Authorization: Bearer <TOKEN> => true
  })

  it('accepts correct token via x-api-key header', () => {
    // isAuthorized with x-api-key: <TOKEN> => true
  })

  it('accepts correct token via query param', () => {
    // isAuthorized with ?token=<TOKEN> => true
  })

  // Timing-safe comparison test:
  it('uses constant-time comparison for token', () => {
    // After fix: verify timingSafeEqual is called
  })
})

describe('shell-server env filtering', () => {
  it('only includes SAFE_ENV_KEYS in PTY child env', () => {
    // buildShellEnv() should not include SECRET_KEY, DATABASE_URL, etc.
  })

  it('always sets TERM and COLORTERM', () => {
    // buildShellEnv().TERM === 'xterm-256color'
  })

  it('excludes AXON_WEB_API_TOKEN from child env', () => {
    // Verify the token is never passed to spawned shells
  })
})

describe('shell-server origin validation', () => {
  it('rejects cross-origin requests when ALLOWED_ORIGINS is set', () => {
    // isAllowedOrigin with mismatched origin => false
  })

  it('allows same-host origin via x-forwarded-host fallback', () => {
    // isAllowedOrigin with matching forwarded host => true
  })

  it('blocks missing origin only when not configured', () => {
    // Test the null-origin path
  })
})
```

**Prerequisite:** The shell server is a standalone `.mjs` file. To test it, extract the pure functions (`isAuthorized`, `isAllowedOrigin`, `getAuthToken`, `buildShellEnv`) into a separate module that can be imported by both the server and tests.

---

### 2. Subprocess Env Filtering -- HIGH

**File:** `app/api/pulse/source/route.ts:29`
**Issue:** `spawn(commandPath, args, { env: process.env })` passes the full process environment to the child subprocess, including `AXON_WEB_API_TOKEN`, `OPENAI_API_KEY`, database credentials, and any other secrets in the server's environment.
**Test Coverage:** Zero direct tests for the spawn call. SSRF validation of URLs is tested separately.

**Recommendation:**

```typescript
// __tests__/api/pulse-source-env.test.ts
import { describe, expect, it, vi } from 'vitest'

// Mock spawn to capture the env argument
const spawnMock = vi.fn()
vi.mock('node:child_process', () => ({ spawn: spawnMock }))

describe('pulse source subprocess', () => {
  it('does not pass AXON_WEB_API_TOKEN to child process', () => {
    // After fix: verify spawn is called with filtered env
    // The env should include PATH, HOME, QDRANT_URL, TEI_URL
    // but NOT AXON_WEB_API_TOKEN, OPENAI_API_KEY, etc.
  })
})
```

**Code fix needed first:** Create an env allowlist similar to `shell-server.mjs:SAFE_ENV_KEYS`, then pass `env: buildSafeSubprocessEnv()` to `spawn()`.

---

### 3. SQL Parameterization (statusWhere) -- LOW (mitigated)

**File:** `app/api/jobs/route.ts:80-97`
**Issue:** `statusWhere()` interpolates strings into SQL via template literal `WHERE ${where}`.
**Actual Risk:** LOW. The `statusWhere` function uses a `switch` statement on the `StatusFilter` type union, which only produces hardcoded SQL fragments (`status = 'pending'`, etc.). User input is validated against `VALID_STATUSES` Set before reaching `statusWhere`. The `jobs-route.test.ts` file tests this validation.

**Test Coverage:** Partial. Tests validate the Set membership check but do not test `statusWhere()` directly (it's not exported).

**Recommendation:**

```typescript
// Export statusWhere for direct testing, or test via the route handler with mocked PG pool
describe('statusWhere SQL generation', () => {
  it('never produces user-controlled SQL fragments', () => {
    // Verify that statusWhere only returns hardcoded strings
    // for each valid StatusFilter value
  })

  it('returns "1=1" for default/unknown filter', () => {
    // Verify the fallback case
  })
})
```

---

### 4. Rate Limiter IP Resolution -- HIGH

**File:** `lib/server/rate-limit.ts:27-36`
**Issue:** `getClientIp()` trusts `x-forwarded-for` header directly. Behind a reverse proxy this is fine, but if the app is exposed directly, attackers can spoof IPs to bypass rate limits.
**Test Coverage:** Zero. The rate limiter is mocked out in `pulse-save-perf.test.ts` rather than tested directly.

**Recommendation:**

```typescript
// __tests__/rate-limit.test.ts
import { describe, expect, it } from 'vitest'
import { enforceRateLimit } from '@/lib/server/rate-limit'

describe('enforceRateLimit', () => {
  function makeReq(headers: Record<string, string> = {}): Request {
    return { headers: { get: (k: string) => headers[k] ?? null } } as unknown as Request
  }

  it('returns null when under limit', () => {
    const result = enforceRateLimit('test-1', makeReq(), { max: 5, windowMs: 60_000 })
    expect(result).toBeNull()
  })

  it('returns 429 when limit exceeded', () => {
    const req = makeReq({ 'x-forwarded-for': '1.2.3.4' })
    for (let i = 0; i < 3; i++) {
      enforceRateLimit('test-2', req, { max: 3, windowMs: 60_000 })
    }
    const blocked = enforceRateLimit('test-2', req, { max: 3, windowMs: 60_000 })
    expect(blocked?.status).toBe(429)
  })

  it('treats different IPs independently', () => {
    const req1 = makeReq({ 'x-forwarded-for': '1.1.1.1' })
    const req2 = makeReq({ 'x-forwarded-for': '2.2.2.2' })
    for (let i = 0; i < 3; i++) {
      enforceRateLimit('test-3', req1, { max: 3, windowMs: 60_000 })
    }
    const result = enforceRateLimit('test-3', req2, { max: 3, windowMs: 60_000 })
    expect(result).toBeNull()
  })

  it('rejects new IPs when counter map is at capacity (spoofed IP flood)', () => {
    // Verify the MAX_COUNTER_KEYS guard works
  })

  it('evicts expired entries to free capacity', () => {
    // Verify evictExpired reclaims slots
  })

  it('uses first IP from x-forwarded-for chain', () => {
    const req = makeReq({ 'x-forwarded-for': '1.2.3.4, 10.0.0.1, 172.16.0.1' })
    // Verify getClientIp extracts '1.2.3.4'
  })

  it('falls back to x-real-ip when x-forwarded-for is absent', () => {
    const req = makeReq({ 'x-real-ip': '5.6.7.8' })
    // Verify '5.6.7.8' is used
  })

  it('returns "unknown" when no IP headers present', () => {
    // All unknown clients share one bucket -- potential DoS vector
  })
})
```

---

### 5. MCP Config Write Auth -- LOW (mitigated)

**File:** `app/api/mcp/route.ts:68`
**Issue:** Auth gate uses only `X-Pulse-Request: 1` header check.
**Test Coverage:** Good. `mcp/route.test.ts` tests the 403 response when the header is absent for both PUT and DELETE. Also tests Zod validation, path traversal in commands, and SSRF on server URLs.

**Assessment:** The `X-Pulse-Request` header is a CSRF mitigation (custom headers cannot be sent cross-origin by default). Combined with the app's same-origin architecture, this is adequate for a self-hosted tool. No additional tests needed.

---

### 6. Doc Endpoint Path Validation -- MEDIUM

**File:** `app/api/pulse/doc/route.ts:10` calls `loadPulseDoc(filename)` with user-supplied `filename` query param.
**Mitigation:** `storage.ts:178` applies `path.basename(filename)` which strips directory traversal.
**Test Coverage:** `pulse-storage.test.ts` tests load/save/update/list but does not test path traversal defense.

**Recommendation:**

```typescript
// Add to __tests__/pulse-storage.test.ts
describe('path traversal protection', () => {
  it('strips directory traversal from filename on load', async () => {
    const saved = await savePulseDoc({ title: 'Safe', markdown: 'content' })
    // Attempt to read with path traversal
    const traversal = await loadPulseDoc(`../../etc/passwd`)
    expect(traversal).toBeNull() // path.basename strips to 'passwd', which doesn't exist

    // Verify basename stripping preserves valid filenames
    const loaded = await loadPulseDoc(`../../../${saved.filename}`)
    // path.basename extracts just the filename, so this should work
    expect(loaded?.title).toBe('Safe')
  })

  it('strips directory traversal from filename on update', async () => {
    const saved = await savePulseDoc({ title: 'Safe', markdown: 'v1' })
    const result = await updatePulseDoc(`../../${saved.filename}`, {
      title: 'Updated',
      markdown: 'v2',
    })
    expect(result.filename).toBe(saved.filename) // basename stripped
  })
})
```

---

## Performance-Critical Path Coverage

### 1. Message Merge at Scale -- MEDIUM

**File:** `components/shell/live-message-sync.ts`
**Issue:** `mergeHistoricalMessages` uses O(n*m) matching: for each of n historical messages, it scans up to m live messages with `findIndex`. At 200+ messages with string normalization, this could cause frame drops.
**Test Coverage:** Good for correctness (sourceMessageId matching, whitespace normalization, role mismatch). No performance test.

**Recommendation:**

```typescript
// Add to __tests__/live-message-sync.test.ts
describe('mergeHistoricalMessages performance', () => {
  it('completes merge of 500 messages in <50ms', () => {
    const historical = Array.from({ length: 500 }, (_, i) => ({
      id: `h${i}`,
      role: 'assistant' as const,
      content: `Message content ${i} with some extra text to make it realistic`,
      timestamp: i,
    }))
    const live = Array.from({ length: 500 }, (_, i) => ({
      id: `l${i}`,
      role: 'assistant' as const,
      content: `Message content ${i} with some extra text to make it realistic`,
      timestamp: i,
      toolUses: [{ name: 'exec_command', input: {} }],
    }))

    const start = performance.now()
    mergeHistoricalMessages(historical, live)
    const elapsed = performance.now() - start

    expect(elapsed).toBeLessThan(50)
  })
})
```

If this fails, the fix is to build a `Map<sourceMessageId, index>` for live messages instead of linear scanning.

---

### 2. Streaming Delta Batching -- LOW

**File:** `hooks/use-axon-acp.ts`
**Test Coverage:** `use-axon-acp-editor.test.ts` covers the `handleEditorMsg` and `createClientMessageId` exports. The streaming delta accumulation logic is inside the hook (React state) and not directly testable in a node environment.

**Assessment:** This is a browser-only concern. Would need jsdom or Playwright to test meaningfully. Not a gap given the current test infrastructure.

---

### 3. Shell State Re-render Frequency -- MEDIUM

**Files:** 8 `axon-shell-state-*.ts` files (Zustand stores)
**Test Coverage:** Zero. None of the shell state files have any tests.

These files manage the entire shell UI state: messages, sessions, layout, settings, tools, actions. They contain business logic (message deduplication, session switching, tool preference merging) that should be unit-testable without React.

**Recommendation:** Extract pure reducer/selector functions from the Zustand stores and test them independently:

```typescript
// __tests__/axon-shell-state-messages.test.ts
import { describe, expect, it } from 'vitest'
// Import pure functions extracted from the store

describe('shell state message management', () => {
  it('deduplicates messages by id', () => { /* ... */ })
  it('preserves message order on append', () => { /* ... */ })
  it('clears messages on session switch', () => { /* ... */ })
  it('updates streaming flag correctly', () => { /* ... */ })
})
```

---

## Core Functionality Coverage

### Well-Tested Modules

| Module | Test File | Tests | Quality |
|--------|-----------|-------|---------|
| SSRF validation | `url-validation.test.ts` | 25 | Excellent -- IPv4/IPv6/mapped/ULA |
| MCP CRUD + auth | `mcp/route.test.ts` | 32 | Excellent -- schema, CSRF, traversal |
| Pulse storage | `pulse-storage.test.ts` | 13 | Good -- CRUD, round-trip, metadata |
| Live message sync | `live-message-sync.test.ts` | 10 | Good -- guard logic + merge |
| Omnibox | `omnibox.test.ts` + `omnibox-utils.test.ts` | 57 | Excellent |
| Session parsers | `sessions/*.test.ts` | 6 files, ~60 | Thorough |
| WS protocol | `ws-protocol*.test.ts` | 3 files, ~30 | Good type coverage |
| Stream parser | `stream-parser.test.ts` | 12 | Good |
| Pulse chat API | `pulse-chat-api-lib.test.ts` | 47 | Thorough |
| Workspace persistence | `workspace-persistence.test.ts` | 48 | Thorough |
| Terminal history | `terminal-history.test.ts` | 24 | Good |
| AI command utils | `ai-command-utils.test.ts` | 40 | Thorough |
| Structured text | `structured-text.test.ts` | 31 | Good |

### Untested Modules

| Module | Risk | Reason |
|--------|------|--------|
| `shell-server.mjs` | Critical | PTY access, auth, env filtering |
| `lib/server/rate-limit.ts` | High | Rate limit bypass via IP spoofing |
| `axon-shell-state-*.ts` (8 files) | Medium | Core UI state management |
| `hooks/use-axon-ws.ts` | Medium | WebSocket reconnect, message queue |
| `hooks/use-shell-session.ts` | Medium | PTY session lifecycle |
| `hooks/use-adaptive-polling.ts` | Low | Polling frequency logic |
| `lib/pulse/copilot-validation.ts` | Low | Input validation |
| `lib/server/redis-client.ts` | Low | Connection wrapper |
| `lib/sessions/claude-jsonl-parser.ts` | Low | Tested via scanner tests |
| `lib/pulse/workspace-root.ts` | Low | Simple path resolution |

---

## Test Quality Assessment

### Patterns (Positive)

1. **Pure function extraction:** Security-critical logic (`validateUrlForSsrf`, `statusWhere`, `isHighRiskOperationSet`) is extracted from route handlers and tested independently. This is the right pattern.

2. **Proper mock isolation:** Tests use `vi.clearAllMocks()` in `beforeEach`, `vi.resetModules()` for module cache isolation, and dynamic imports for modules with side effects.

3. **Behavioral testing:** Tests check outcomes ("rejects unknown type values") rather than implementation details ("calls Set.has()"). The `pulse-chat-api-lib.test.ts` with 47 tests is a model.

4. **Edge cases covered:** Empty arrays, null bodies, malformed JSON, boundary values (limit clamping 0 and 999), and non-existent files are all tested where they appear.

5. **No test interdependence:** Tests run in any order. No shared mutable state between test files.

### Patterns (Negative)

1. **Duplicated validation logic:** `jobs-route.test.ts` recreates the `VALID_TYPES` and `VALID_STATUSES` Sets locally instead of importing them from the route or testing via the actual handler. If the route adds a new type, the test will still pass but not actually verify the route accepts it.

2. **Mocking depth:** `pulse-save-perf.test.ts` mocks 4 modules (rate-limit, storage, server-env, next/server) to test the route. While necessary, this many mocks risk testing the mock wiring rather than the actual behavior.

3. **No negative security tests for some routes:** The `api/pulse/doc/route.ts` and `api/logs/route.ts` handlers accept user input but have no tests for malicious input beyond what the underlying library (`path.basename`) provides.

---

## Findings by Severity

### Critical

| # | Finding | File | Recommendation |
|---|---------|------|----------------|
| 1 | Shell server has zero tests for auth, origin validation, env filtering | `shell-server.mjs` | Extract pure functions into importable module, write 15-20 tests. Fix timing-safe comparison first. |
| 2 | Token comparison uses `===` instead of `timingSafeEqual` | `shell-server.mjs:120` | Replace `getAuthToken(req) === TOKEN` with `crypto.timingSafeEqual(Buffer.from(token), Buffer.from(TOKEN))` plus length check |

### High

| # | Finding | File | Recommendation |
|---|---------|------|----------------|
| 3 | Rate limiter has zero tests | `lib/server/rate-limit.ts` | Write 8-10 tests covering limit enforcement, IP extraction, spoofed IP flood, eviction, counter overflow |
| 4 | Subprocess passes full `process.env` | `api/pulse/source/route.ts:29` | Create env allowlist, write tests verifying secrets are excluded |
| 5 | No integration tests for full request pipeline | All API routes | Add at least smoke tests for critical auth and validation paths through the actual Next.js handler |

### Medium

| # | Finding | File | Recommendation |
|---|---------|------|----------------|
| 6 | Doc endpoint has no path traversal tests | `api/pulse/doc/route.ts` | Add traversal tests to `pulse-storage.test.ts` |
| 7 | Shell state layer (8 files) is entirely untested | `components/shell/axon-shell-state-*.ts` | Extract pure state logic, write unit tests |
| 8 | Message merge has O(n*m) complexity without perf test | `live-message-sync.ts` | Add benchmark test at 500 messages |
| 9 | WebSocket hook (`use-axon-ws.ts`) untested | `hooks/use-axon-ws.ts` | Extract reconnect logic into testable pure functions |
| 10 | `jobs-route.test.ts` duplicates validation sets | `__tests__/jobs-route.test.ts` | Import from source to keep tests in sync |

### Low

| # | Finding | File | Recommendation |
|---|---------|------|----------------|
| 11 | 13 hooks have no tests | `hooks/use-*.ts` | Prioritize `use-shell-session.ts` and `use-adaptive-polling.ts` |
| 12 | 28 lib files have no tests | `lib/**/*.ts` | Most are thin wrappers or type files; prioritize `copilot-validation.ts` and `redis-client.ts` |
| 13 | No E2E tests | N/A | Add Playwright smoke test for auth gate and omnibox flow |
| 14 | `getStatusCounts` interpolates table names | `api/jobs/route.ts:217` | Table names are hardcoded strings, not user input -- no real risk, but document the pattern |

---

## Summary of Recommended Actions (Priority Order)

1. **Fix `shell-server.mjs` timing-safe comparison** and extract pure functions for testing (Critical, security)
2. **Write rate-limit tests** covering IP spoofing and capacity overflow (High, security)
3. **Add env filtering to pulse source subprocess** and test the allowlist (High, security)
4. **Add path traversal tests to pulse-storage** (Medium, security)
5. **Write shell state unit tests** for at least `axon-shell-state-messages.ts` (Medium, correctness)
6. **Add merge performance benchmark** for live-message-sync (Medium, performance)
7. **Fix `jobs-route.test.ts`** to import validation sets from source (Low, maintainability)
