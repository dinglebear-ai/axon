import { EventEmitter } from 'node:events'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@/lib/pulse/server-env', () => ({
  ensureRepoRootEnvLoaded: vi.fn(),
}))

vi.mock('@/lib/pulse/workspace-root', () => ({
  getWorkspaceRoot: () => '/workspace',
}))

const validateUrlsForSsrfMock = vi.fn()
vi.mock('@/lib/server/url-validation', () => ({
  validateUrlsForSsrf: (...args: unknown[]) => validateUrlsForSsrfMock(...args),
}))

const spawnMock = vi.fn()
vi.mock('node:child_process', () => ({
  spawn: (...args: unknown[]) => spawnMock(...args),
}))

class MockChild extends EventEmitter {
  stdout = new EventEmitter()
  stderr = new EventEmitter()
  kill = vi.fn()
}

describe('POST /api/pulse/source', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    validateUrlsForSsrfMock.mockReturnValue({ valid: true })
  })

  afterEach(() => {
    vi.resetModules()
  })

  it('returns 400 for invalid JSON body', async () => {
    const { POST } = await import('@/app/api/pulse/source/route')

    const req = new Request('http://localhost/api/pulse/source', {
      method: 'POST',
      body: 'not-json',
      headers: { 'Content-Type': 'application/json' },
    })

    const res = await POST(req)

    expect(res.status).toBe(400)
  })

  it('returns 400 with ssrf_blocked when URL is rejected', async () => {
    validateUrlsForSsrfMock.mockReturnValue({
      valid: false,
      reason: 'private network',
      url: 'http://127.0.0.1',
    })

    const { POST } = await import('@/app/api/pulse/source/route')

    const req = new Request('http://localhost/api/pulse/source', {
      method: 'POST',
      body: JSON.stringify({ urls: ['http://127.0.0.1'] }),
      headers: { 'Content-Type': 'application/json' },
    })

    const res = await POST(req)

    expect(res.status).toBe(400)
    const body = (await res.json()) as { code?: string }
    expect(body.code).toBe('ssrf_blocked')
  })

  it('returns 502 when scrape subprocess reports failure', async () => {
    const child = new MockChild()
    spawnMock.mockImplementation(() => {
      queueMicrotask(() => {
        child.emit('close', 1, null)
      })
      return child
    })

    const { POST } = await import('@/app/api/pulse/source/route')

    const res = await POST(
      new Request('http://localhost/api/pulse/source', {
        method: 'POST',
        body: JSON.stringify({ urls: ['https://example.com'] }),
        headers: { 'Content-Type': 'application/json' },
      }),
    )

    expect(res.status).toBe(502)
    expect(spawnMock).toHaveBeenCalled()
  })
})
