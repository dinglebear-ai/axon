import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

// ---------------------------------------------------------------------------
// apiFetch tests
//
// api-fetch.ts reads NEXT_PUBLIC_AXON_API_TOKEN at module load time (top-level
// const). We use vi.resetModules() + dynamic import to reload the module with
// different env values in each describe block.
// ---------------------------------------------------------------------------

// Helper: build a minimal Response-like object that satisfies the fetch contract
function makeResponse(
  body: string,
  init: { status?: number; headers?: Record<string, string> } = {},
): Response {
  return new Response(body, {
    status: init.status ?? 200,
    headers: init.headers ?? { 'content-type': 'application/json' },
  })
}

// ---------------------------------------------------------------------------
// Suite A: no API token set
// ---------------------------------------------------------------------------

describe('apiFetch — no API token configured', () => {
  let apiFetch: (input: string | URL | Request, init?: RequestInit) => Promise<Response>

  beforeEach(async () => {
    vi.resetModules()
    delete process.env.NEXT_PUBLIC_AXON_API_TOKEN

    // Mock fetch before importing the module under test
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(makeResponse(JSON.stringify({ ok: true }))))

    const mod = await import('@/lib/api-fetch')
    apiFetch = mod.apiFetch
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('passes request through to fetch unchanged when no token is set', async () => {
    await apiFetch('/api/foo')
    expect(globalThis.fetch).toHaveBeenCalledOnce()
    // When no token, the call is fetch(input, init) — second arg is undefined / original init
    const [calledInput, calledInit] = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0]!
    expect(calledInput).toBe('/api/foo')
    // No x-api-key should be injected (we pass through as-is)
    if (calledInit?.headers) {
      const h = new Headers(calledInit.headers)
      expect(h.has('x-api-key')).toBe(false)
    }
  })

  it('returns the fetch Response directly', async () => {
    const resp = await apiFetch('/api/bar')
    expect(resp).toBeInstanceOf(Response)
    expect(resp.status).toBe(200)
  })

  it('propagates 4xx response without throwing', async () => {
    ;(globalThis.fetch as ReturnType<typeof vi.fn>).mockResolvedValueOnce(
      makeResponse('{"error":"Not Found"}', { status: 404 }),
    )
    const resp = await apiFetch('/api/missing')
    expect(resp.status).toBe(404)
  })

  it('propagates 5xx response without throwing', async () => {
    ;(globalThis.fetch as ReturnType<typeof vi.fn>).mockResolvedValueOnce(
      makeResponse('{"error":"Internal"}', { status: 500 }),
    )
    const resp = await apiFetch('/api/boom')
    expect(resp.status).toBe(500)
  })

  it('re-throws network errors from fetch', async () => {
    ;(globalThis.fetch as ReturnType<typeof vi.fn>).mockRejectedValueOnce(
      new TypeError('Failed to fetch'),
    )
    await expect(apiFetch('/api/net-err')).rejects.toThrow('Failed to fetch')
  })
})

// ---------------------------------------------------------------------------
// Suite B: API token set — token injection
// ---------------------------------------------------------------------------

describe('apiFetch — with API token', () => {
  let apiFetch: (input: string | URL | Request, init?: RequestInit) => Promise<Response>

  beforeEach(async () => {
    vi.resetModules()
    process.env.NEXT_PUBLIC_AXON_API_TOKEN = 'test-secret-token'

    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(makeResponse(JSON.stringify({ ok: true }))))

    const mod = await import('@/lib/api-fetch')
    apiFetch = mod.apiFetch
  })

  afterEach(() => {
    vi.unstubAllGlobals()
    delete process.env.NEXT_PUBLIC_AXON_API_TOKEN
  })

  it('injects x-api-key header for /api/ path strings', async () => {
    await apiFetch('/api/cortex/stats')

    const [, calledInit] = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0]!
    const headers = new Headers(calledInit.headers)
    expect(headers.get('x-api-key')).toBe('test-secret-token')
  })

  it('does not inject x-api-key if caller already set it', async () => {
    await apiFetch('/api/cortex/stats', {
      headers: { 'x-api-key': 'caller-provided-key' },
    })

    const [, calledInit] = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0]!
    const headers = new Headers(calledInit.headers)
    // The caller's key must be preserved, not overwritten
    expect(headers.get('x-api-key')).toBe('caller-provided-key')
  })

  it('merges caller init headers with injected x-api-key', async () => {
    await apiFetch('/api/cortex/stats', {
      headers: { 'content-type': 'application/json' },
    })

    const [, calledInit] = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0]!
    const headers = new Headers(calledInit.headers)
    expect(headers.get('x-api-key')).toBe('test-secret-token')
    expect(headers.get('content-type')).toBe('application/json')
  })

  it('merges init headers over Request object headers', async () => {
    const req = new Request('http://localhost/api/foo', {
      headers: { 'x-custom': 'from-request', authorization: 'from-request' },
    })
    await apiFetch(req, { headers: { authorization: 'from-init' } })

    const [, calledInit] = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0]!
    const headers = new Headers(calledInit.headers)
    expect(headers.get('authorization')).toBe('from-init')
    expect(headers.get('x-custom')).toBe('from-request')
    expect(headers.get('x-api-key')).toBe('test-secret-token')
  })

  it('injects x-api-key when input is a URL object pointing at /api/', async () => {
    // In node environment location is undefined so the URL constructor will throw
    // for relative paths — use an absolute URL with the /api/ prefix
    await apiFetch(new URL('http://localhost/api/jobs'))

    const [, calledInit] = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0]!
    const headers = new Headers(calledInit.headers)
    expect(headers.get('x-api-key')).toBe('test-secret-token')
  })

  it('injects x-api-key when input is a Request object for /api/ path', async () => {
    const req = new Request('http://localhost/api/mcp')
    await apiFetch(req)

    const [, calledInit] = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0]!
    const headers = new Headers(calledInit.headers)
    expect(headers.get('x-api-key')).toBe('test-secret-token')
  })

  it('preserves additional fetch init options (method, body)', async () => {
    await apiFetch('/api/pulse/save', {
      method: 'POST',
      body: JSON.stringify({ title: 'My Doc' }),
      headers: { 'content-type': 'application/json' },
    })

    const [, calledInit] = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0]!
    expect(calledInit.method).toBe('POST')
    expect(calledInit.body).toBe(JSON.stringify({ title: 'My Doc' }))
    const headers = new Headers(calledInit.headers)
    expect(headers.get('x-api-key')).toBe('test-secret-token')
  })

  it('re-throws network errors even when token is set', async () => {
    ;(globalThis.fetch as ReturnType<typeof vi.fn>).mockRejectedValueOnce(
      new TypeError('Network failure'),
    )
    await expect(apiFetch('/api/anything')).rejects.toThrow('Network failure')
  })

  it('propagates 401 response without throwing', async () => {
    ;(globalThis.fetch as ReturnType<typeof vi.fn>).mockResolvedValueOnce(
      makeResponse('{"error":"Unauthorized"}', { status: 401 }),
    )
    const resp = await apiFetch('/api/protected')
    expect(resp.status).toBe(401)
  })

  it('propagates 403 response without throwing', async () => {
    ;(globalThis.fetch as ReturnType<typeof vi.fn>).mockResolvedValueOnce(
      makeResponse('{"error":"Forbidden"}', { status: 403 }),
    )
    const resp = await apiFetch('/api/admin')
    expect(resp.status).toBe(403)
  })
})

// ---------------------------------------------------------------------------
// Suite C: shouldInjectToken path coverage (edge cases for URL parsing)
// ---------------------------------------------------------------------------

describe('apiFetch — shouldInjectToken edge cases', () => {
  let apiFetch: (input: string | URL | Request, init?: RequestInit) => Promise<Response>

  beforeEach(async () => {
    vi.resetModules()
    process.env.NEXT_PUBLIC_AXON_API_TOKEN = 'edge-token'

    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(makeResponse(JSON.stringify({}))))

    const mod = await import('@/lib/api-fetch')
    apiFetch = mod.apiFetch
  })

  afterEach(() => {
    vi.unstubAllGlobals()
    delete process.env.NEXT_PUBLIC_AXON_API_TOKEN
  })

  it('injects token for string starting with /api/ (relative URL catch block)', async () => {
    // In node, `new URL('/api/foo', undefined)` throws — exercises the catch branch
    await apiFetch('/api/foo')

    const [, calledInit] = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0]!
    const headers = new Headers(calledInit.headers)
    expect(headers.get('x-api-key')).toBe('edge-token')
  })

  it('does not inject token for non-/api/ relative path string', async () => {
    // shouldInjectToken returns false for '/other/path' in the catch branch
    await apiFetch('/other/path')

    const [, calledInit] = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0]!
    // Headers object will be present but x-api-key should not be injected
    const headers = new Headers(calledInit?.headers ?? {})
    expect(headers.get('x-api-key')).toBeNull()
  })
})
