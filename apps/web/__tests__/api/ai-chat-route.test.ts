import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@/lib/pulse/server-env', () => ({
  ensureRepoRootEnvLoaded: vi.fn(),
}))

describe('POST /api/ai/chat', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    delete process.env.OPENAI_BASE_URL
    delete process.env.OPENAI_API_KEY
    delete process.env.OPENAI_MODEL
  })

  afterEach(() => {
    vi.restoreAllMocks()
    vi.resetModules()
  })

  it('returns 503 when OPENAI env is missing', async () => {
    const { POST } = await import('@/app/api/ai/chat/route')

    const res = await POST(
      new Request('http://localhost/api/ai/chat', {
        method: 'POST',
        body: JSON.stringify({ prompt: 'hi' }),
        headers: { 'Content-Type': 'application/json' },
      }),
    )

    expect(res.status).toBe(503)
    const body = (await res.json()) as { code?: string }
    expect(body.code).toBe('ai_chat_config')
  })

  it('returns 400 for invalid payload', async () => {
    process.env.OPENAI_BASE_URL = 'http://llm.local/v1'
    process.env.OPENAI_API_KEY = 'key'

    const { POST } = await import('@/app/api/ai/chat/route')

    const res = await POST(
      new Request('http://localhost/api/ai/chat', {
        method: 'POST',
        body: JSON.stringify({ prompt: '' }),
        headers: { 'Content-Type': 'application/json' },
      }),
    )

    expect(res.status).toBe(400)
  })

  it('returns 502 when upstream response is not ok', async () => {
    process.env.OPENAI_BASE_URL = 'http://llm.local/v1'
    process.env.OPENAI_API_KEY = 'key'

    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: false,
        status: 500,
      }),
    )

    const { POST } = await import('@/app/api/ai/chat/route')

    const res = await POST(
      new Request('http://localhost/api/ai/chat', {
        method: 'POST',
        body: JSON.stringify({ prompt: 'hello world' }),
        headers: { 'Content-Type': 'application/json' },
      }),
    )

    expect(res.status).toBe(502)
    const body = (await res.json()) as { code?: string }
    expect(body.code).toBe('ai_chat_upstream')
  })
})
