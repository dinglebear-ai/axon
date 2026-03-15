import { NextRequest } from 'next/server'
import { afterEach, describe, expect, it, vi } from 'vitest'

type ProxyModule = {
  proxy: (req: NextRequest) => Response
}

const ORIGINAL_ENV = { ...process.env }

async function loadProxyWithEnv(env: Record<string, string | undefined>): Promise<ProxyModule> {
  process.env = { ...ORIGINAL_ENV }
  for (const [key, value] of Object.entries(env)) {
    if (value === undefined) {
      delete process.env[key]
    } else {
      process.env[key] = value
    }
  }
  vi.resetModules()
  return import('@/proxy')
}

function makeApiRequest(url = 'http://localhost:49010/api/jobs', headers: HeadersInit = {}) {
  return new NextRequest(url, { headers })
}

afterEach(() => {
  process.env = { ...ORIGINAL_ENV }
  vi.resetModules()
})

describe('proxy auth + origin gate', () => {
  it('authorizes with AXON_WEB_API_TOKEN', async () => {
    const { proxy } = await loadProxyWithEnv({
      AXON_WEB_API_TOKEN: 'web-token',
      AXON_WEB_BROWSER_API_TOKEN: undefined,
      AXON_WEB_ALLOW_INSECURE_DEV: 'false',
      AXON_WEB_ALLOWED_ORIGINS: '',
    })

    const res = proxy(makeApiRequest(undefined, { authorization: 'Bearer web-token' }))

    expect(res.status).toBe(200)
    expect(res.headers.get('X-Frame-Options')).toBe('DENY')
  })

  it('authorizes with AXON_WEB_BROWSER_API_TOKEN', async () => {
    const { proxy } = await loadProxyWithEnv({
      AXON_WEB_API_TOKEN: 'web-token',
      AXON_WEB_BROWSER_API_TOKEN: 'browser-token',
      AXON_WEB_ALLOW_INSECURE_DEV: 'false',
      AXON_WEB_ALLOWED_ORIGINS: '',
    })

    const res = proxy(makeApiRequest(undefined, { 'x-api-key': 'browser-token' }))

    expect(res.status).toBe(200)
    expect(res.headers.get('X-Content-Type-Options')).toBe('nosniff')
  })

  it('rejects invalid token', async () => {
    const { proxy } = await loadProxyWithEnv({
      AXON_WEB_API_TOKEN: 'web-token',
      AXON_WEB_BROWSER_API_TOKEN: 'browser-token',
      AXON_WEB_ALLOW_INSECURE_DEV: 'false',
      AXON_WEB_ALLOWED_ORIGINS: '',
    })

    const res = proxy(makeApiRequest(undefined, { authorization: 'Bearer wrong-token' }))

    expect(res.status).toBe(401)
  })

  it('rejects disallowed origin', async () => {
    const { proxy } = await loadProxyWithEnv({
      AXON_WEB_API_TOKEN: 'web-token',
      AXON_WEB_BROWSER_API_TOKEN: undefined,
      AXON_WEB_ALLOW_INSECURE_DEV: 'false',
      AXON_WEB_ALLOWED_ORIGINS: 'https://good.example.com',
    })

    const res = proxy(
      makeApiRequest(undefined, {
        authorization: 'Bearer web-token',
        origin: 'https://evil.example.com',
      }),
    )

    expect(res.status).toBe(403)
  })

  it('returns 503 when auth is not configured', async () => {
    const { proxy } = await loadProxyWithEnv({
      AXON_WEB_API_TOKEN: undefined,
      AXON_WEB_BROWSER_API_TOKEN: undefined,
      AXON_WEB_ALLOW_INSECURE_DEV: 'false',
      AXON_WEB_ALLOWED_ORIGINS: '',
    })

    const res = proxy(makeApiRequest())

    expect(res.status).toBe(503)
  })
})
