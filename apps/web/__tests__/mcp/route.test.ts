/**
 * Tests for app/api/mcp/route.ts (GET / PUT / DELETE handlers).
 *
 * node:fs/promises and node:os are mocked so tests run without touching disk.
 * next/server is mocked to return a plain object with .json() and .status.
 */

import type { NextRequest } from 'next/server'
import { beforeEach, describe, expect, it, vi } from 'vitest'

// ── Mocks ─────────────────────────────────────────────────────────────────────

vi.mock('node:os', () => ({ default: { homedir: () => '/home/testuser' } }))

const fsMock = {
  readFile: vi.fn(),
  writeFile: vi.fn(),
  mkdir: vi.fn(),
}
vi.mock('node:fs/promises', () => ({ default: fsMock }))

vi.mock('next/server', () => ({
  NextResponse: {
    json: (data: unknown, init?: { status?: number }) => ({
      _data: data,
      status: init?.status ?? 200,
      json: async () => data,
    }),
  },
}))

// Mocks required by status/route.ts (execFile + promisify)
vi.mock('node:child_process', () => ({ execFile: vi.fn() }))
vi.mock('node:util', () => ({ promisify: (fn: unknown) => fn }))

// ── Import after mocks are registered ────────────────────────────────────────

// Dynamic import so that the module picks up the mocked dependencies.
async function loadRoute() {
  return import('@/app/api/mcp/route')
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/** Simulate a Next.js Request with a JSON body and optional headers. */
function makeRequest(body: unknown, headers: Record<string, string> = {}): NextRequest {
  return {
    json: async () => body,
    headers: {
      get: (key: string) => headers[key.toLowerCase()] ?? null,
    },
  } as unknown as NextRequest
}

// ── Tests ─────────────────────────────────────────────────────────────────────

describe('GET /api/mcp', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    fsMock.writeFile.mockResolvedValue(undefined)
    fsMock.mkdir.mockResolvedValue(undefined)
  })

  it('returns parsed config when file exists with valid JSON', async () => {
    const config = { mcpServers: { 'my-server': { command: 'node', args: ['server.js'] } } }
    fsMock.readFile.mockResolvedValue(JSON.stringify(config))

    const { GET } = await loadRoute()
    const response = await GET()

    expect(response.status).toBe(200)
    expect(await response.json()).toEqual(config)
  })

  it('returns { mcpServers: {} } when file does not exist (ENOENT)', async () => {
    const enoent = Object.assign(new Error('ENOENT'), { code: 'ENOENT' })
    fsMock.readFile.mockRejectedValue(enoent)

    const { GET } = await loadRoute()
    const response = await GET()

    expect(response.status).toBe(200)
    expect(await response.json()).toEqual({ mcpServers: {} })
  })

  it('returns 500 when file contains invalid JSON', async () => {
    fsMock.readFile.mockResolvedValue('not-json{{{')

    const { GET } = await loadRoute()
    const response = await GET()

    expect(response.status).toBe(500)
    const body = await response.json()
    expect(body).toHaveProperty('error')
  })

  it('returns { mcpServers: {} } when parsed JSON lacks mcpServers', async () => {
    fsMock.readFile.mockResolvedValue(JSON.stringify({ something: 'else' }))

    const { GET } = await loadRoute()
    const response = await GET()

    // readMcpConfig normalises a missing/invalid mcpServers to {}
    expect(response.status).toBe(200)
    expect(await response.json()).toEqual({ mcpServers: {} })
  })
})

describe('PUT /api/mcp', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    fsMock.writeFile.mockResolvedValue(undefined)
    fsMock.mkdir.mockResolvedValue(undefined)
  })

  it('writes pretty-printed JSON and returns { ok: true } for valid body', async () => {
    const config = { mcpServers: { 'my-server': { command: 'node' } } }
    const req = makeRequest(config)

    const { PUT } = await loadRoute()
    const response = await PUT(req)

    expect(response.status).toBe(200)
    expect(await response.json()).toEqual({ ok: true })

    // Verify writeFile was called with pretty-printed JSON
    expect(fsMock.writeFile).toHaveBeenCalledOnce()
    const written = fsMock.writeFile.mock.calls[0]?.[1] as string
    expect(written).toBe(JSON.stringify(config, null, 2))
  })

  it('returns 400 when body is missing mcpServers field', async () => {
    const req = makeRequest({ notMcpServers: {} })

    const { PUT } = await loadRoute()
    const response = await PUT(req)

    expect(response.status).toBe(400)
    const body = await response.json()
    expect(body).toHaveProperty('error')
  })

  it('returns 400 when body is null', async () => {
    const req = makeRequest(null)

    const { PUT } = await loadRoute()
    const response = await PUT(req)

    expect(response.status).toBe(400)
  })

  it('returns 400 when mcpServers is not an object', async () => {
    const req = makeRequest({ mcpServers: 'not-an-object' })

    const { PUT } = await loadRoute()
    const response = await PUT(req)

    expect(response.status).toBe(400)
  })

  it('returns 400 when command contains path traversal (../../bin/bash)', async () => {
    const config = { mcpServers: { 'evil-server': { command: '../../bin/bash' } } }
    const req = makeRequest(config)

    const { PUT } = await loadRoute()
    const response = await PUT(req)

    expect(response.status).toBe(400)
    const body = await response.json()
    expect(body).toHaveProperty('error')
  })

  it('returns 400 when command contains shell metacharacters', async () => {
    const config = { mcpServers: { 'evil-server': { command: 'node; rm -rf /' } } }
    const req = makeRequest(config)

    const { PUT } = await loadRoute()
    const response = await PUT(req)

    expect(response.status).toBe(400)
  })

  it('returns 400 when command contains path separators', async () => {
    const config = { mcpServers: { 'evil-server': { command: '/usr/bin/node' } } }
    const req = makeRequest(config)

    const { PUT } = await loadRoute()
    const response = await PUT(req)

    expect(response.status).toBe(400)
  })

  it('returns 400 when env key does not match uppercase convention', async () => {
    const config = {
      mcpServers: {
        'my-server': {
          command: 'node',
          env: { 'bad-key': 'value' },
        },
      },
    }
    const req = makeRequest(config)

    const { PUT } = await loadRoute()
    const response = await PUT(req)

    expect(response.status).toBe(400)
  })

  it('returns 200 for a fully valid config with url-type server', async () => {
    const config = {
      mcpServers: {
        'http-server': { url: 'https://mcp.example.com/sse' },
        'stdio-server': {
          command: 'npx',
          args: ['-y', '@modelcontextprotocol/server-filesystem', '/tmp'],
          env: { API_KEY: 'secret' },
        },
      },
    }
    const req = makeRequest(config)

    const { PUT } = await loadRoute()
    const response = await PUT(req)

    expect(response.status).toBe(200)
    expect(await response.json()).toEqual({ ok: true })
  })
})

describe('DELETE /api/mcp', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    fsMock.writeFile.mockResolvedValue(undefined)
    fsMock.mkdir.mockResolvedValue(undefined)
  })

  it('removes an existing server and returns { ok: true }', async () => {
    const existing = {
      mcpServers: {
        'keep-me': { command: 'node' },
        'delete-me': { command: 'python' },
      },
    }
    fsMock.readFile.mockResolvedValue(JSON.stringify(existing))

    const req = makeRequest({ name: 'delete-me' })
    const { DELETE } = await loadRoute()
    const response = await DELETE(req)

    expect(response.status).toBe(200)
    expect(await response.json()).toEqual({ ok: true })

    // Verify the written config no longer contains the deleted server
    const written = JSON.parse(fsMock.writeFile.mock.calls[0]?.[1] as string) as {
      mcpServers: Record<string, unknown>
    }
    expect(Object.keys(written.mcpServers)).toEqual(['keep-me'])
    expect(written.mcpServers).not.toHaveProperty('delete-me')
  })

  it('returns { ok: true } even when server name does not exist (filter is a no-op)', async () => {
    // The implementation filters + writes regardless — no 404 is raised.
    const existing = { mcpServers: { 'keep-me': { command: 'node' } } }
    fsMock.readFile.mockResolvedValue(JSON.stringify(existing))

    const req = makeRequest({ name: 'nonexistent' })
    const { DELETE } = await loadRoute()
    const response = await DELETE(req)

    expect(response.status).toBe(200)
    expect(await response.json()).toEqual({ ok: true })
  })

  it('returns 400 when body is missing name field', async () => {
    const req = makeRequest({ notName: 'something' })

    const { DELETE } = await loadRoute()
    const response = await DELETE(req)

    expect(response.status).toBe(400)
    const body = await response.json()
    expect(body).toHaveProperty('error')
  })

  it('returns 400 when name is not a string', async () => {
    const req = makeRequest({ name: 42 })

    const { DELETE } = await loadRoute()
    const response = await DELETE(req)

    expect(response.status).toBe(400)
  })

  it('returns 400 when body is null', async () => {
    const req = makeRequest(null)

    const { DELETE } = await loadRoute()
    const response = await DELETE(req)

    expect(response.status).toBe(400)
  })
})

describe('SSRF protection — validateStatusUrl', () => {
  // Import the real function so tests stay in sync with the implementation.
  let validateStatusUrl: (url: string) => boolean

  beforeEach(async () => {
    const mod = await import('@/app/api/mcp/status/route')
    validateStatusUrl = mod.validateStatusUrl
  })

  const blockedUrls = [
    'http://localhost/sse',
    'http://127.0.0.1/sse',
    'http://0.0.0.0/sse',
    'http://[::1]/sse',
    'http://10.0.0.1/sse',
    'http://172.16.0.1/sse',
    'http://192.168.1.100/sse',
    'http://169.254.169.254/sse', // AWS metadata endpoint
    'ftp://example.com/sse',
    'file:///etc/passwd',
  ]

  const allowedUrls = [
    'http://mcp.example.com/sse',
    'https://api.example.org/mcp',
    'https://203.0.113.1/sse', // TEST-NET-3, public
  ]

  for (const url of blockedUrls) {
    it(`blocks ${url}`, () => {
      expect(validateStatusUrl(url)).toBe(false)
    })
  }

  for (const url of allowedUrls) {
    it(`allows ${url}`, () => {
      expect(validateStatusUrl(url)).toBe(true)
    })
  }

  it('blocks malformed URLs that cannot be parsed', () => {
    expect(validateStatusUrl('not a url')).toBe(false)
    expect(validateStatusUrl('')).toBe(false)
    expect(validateStatusUrl('://broken')).toBe(false)
  })

  // IPv4-mapped IPv6 and trailing-dot bypass cases
  it('blocks http://[::ffff:127.0.0.1]/sse (IPv4-mapped loopback)', () => {
    expect(validateStatusUrl('http://[::ffff:127.0.0.1]/sse')).toBe(false)
  })

  it('blocks http://[::ffff:10.0.0.1]/sse (IPv4-mapped RFC-1918 10.x)', () => {
    expect(validateStatusUrl('http://[::ffff:10.0.0.1]/sse')).toBe(false)
  })

  it('blocks http://[::ffff:192.168.1.1]/sse (IPv4-mapped RFC-1918 192.168.x)', () => {
    expect(validateStatusUrl('http://[::ffff:192.168.1.1]/sse')).toBe(false)
  })

  it('blocks http://localhost./sse (trailing-dot localhost)', () => {
    expect(validateStatusUrl('http://localhost./sse')).toBe(false)
  })

  it('blocks http://[fe80::1]/sse (link-local IPv6)', () => {
    expect(validateStatusUrl('http://[fe80::1]/sse')).toBe(false)
  })
})
