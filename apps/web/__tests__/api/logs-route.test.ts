import { NextRequest } from 'next/server'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const enforceRateLimitMock = vi.fn()
const logErrorMock = vi.fn()
const logWarnMock = vi.fn()

vi.mock('@/lib/server/rate-limit', () => ({
  enforceRateLimit: (...args: unknown[]) => enforceRateLimitMock(...args),
}))

vi.mock('@/lib/server/logger', () => ({
  logError: (...args: unknown[]) => logErrorMock(...args),
  logWarn: (...args: unknown[]) => logWarnMock(...args),
}))

const logsMock = vi.fn()
const demuxStreamMock = vi.fn()

vi.mock('dockerode', () => {
  return {
    default: class Dockerode {
      modem = { demuxStream: (...args: unknown[]) => demuxStreamMock(...args) }
      getContainer() {
        return { logs: (...args: unknown[]) => logsMock(...args) }
      }
    },
  }
})

function makeRequest(url: string) {
  return new NextRequest(url)
}

describe('GET /api/logs', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    enforceRateLimitMock.mockReturnValue(null)
    process.env.AXON_WEB_ENABLE_DOCKER_SOCKET_LOGS = 'false'
  })

  afterEach(() => {
    delete process.env.AXON_WEB_ENABLE_DOCKER_SOCKET_LOGS
    delete process.env.AXON_WEB_DOCKER_SOCKET_PATH
    vi.resetModules()
  })

  it('returns 503 when docker socket logs are disabled', async () => {
    const { GET } = await import('@/app/api/logs/route')

    const res = await GET(makeRequest('http://localhost/api/logs'))

    expect(res.status).toBe(503)
    expect(await res.text()).toMatch(/disabled/i)
  })

  it('returns 400 for invalid service when enabled', async () => {
    process.env.AXON_WEB_ENABLE_DOCKER_SOCKET_LOGS = 'true'
    const { GET } = await import('@/app/api/logs/route')

    const res = await GET(makeRequest('http://localhost/api/logs?service=bad-service'))

    expect(res.status).toBe(400)
    expect(await res.text()).toMatch(/invalid service/i)
  })

  it('returns 400 for invalid tail when enabled', async () => {
    process.env.AXON_WEB_ENABLE_DOCKER_SOCKET_LOGS = 'true'
    const { GET } = await import('@/app/api/logs/route')

    const res = await GET(makeRequest('http://localhost/api/logs?tail=0'))

    expect(res.status).toBe(400)
    expect(await res.text()).toMatch(/invalid tail/i)
  })

  it('returns rate-limit response when enforced', async () => {
    enforceRateLimitMock.mockReturnValue(new Response('Rate limited', { status: 429 }))
    const { GET } = await import('@/app/api/logs/route')

    const res = await GET(makeRequest('http://localhost/api/logs'))

    expect(res.status).toBe(429)
    expect(await res.text()).toBe('Rate limited')
  })
})
