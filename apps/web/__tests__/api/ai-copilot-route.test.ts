import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@ai-sdk/gateway', () => ({
  createGateway: vi.fn(() => vi.fn((model: string) => model)),
}))

vi.mock('ai', () => ({
  generateText: vi.fn(),
}))

describe('POST /api/ai/copilot', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    delete process.env.AI_GATEWAY_API_KEY
  })

  afterEach(() => {
    vi.resetModules()
  })

  it('returns 400 for unsupported model', async () => {
    process.env.AI_GATEWAY_API_KEY = 'key'
    const { POST } = await import('@/app/api/ai/copilot/route')

    const res = await POST(
      new Request('http://localhost/api/ai/copilot', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ model: 'bad-model', prompt: 'hello' }),
      }) as never,
    )

    expect(res.status).toBe(400)
  })

  it('returns 401 when AI Gateway key is missing', async () => {
    const { POST } = await import('@/app/api/ai/copilot/route')

    const res = await POST(
      new Request('http://localhost/api/ai/copilot', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ prompt: 'Continue this sentence' }),
      }) as never,
    )

    expect(res.status).toBe(401)
    await expect(res.json()).resolves.toMatchObject({ code: 'copilot_no_key' })
  })
})
