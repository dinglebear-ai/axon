import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('platejs', () => ({
  createSlateEditor: vi.fn(() => ({
    api: {
      isExpanded: () => false,
    },
  })),
  nanoid: () => 'test-id',
}))

vi.mock('@/components/editor/editor-base-kit', () => ({
  BaseEditorKit: [],
}))

describe('POST /api/ai/command', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    delete process.env.AI_GATEWAY_API_KEY
  })

  afterEach(() => {
    vi.resetModules()
  })

  it('returns 400 for invalid payload', async () => {
    const { POST } = await import('@/app/api/ai/command/route')

    const res = await POST(
      new Request('http://localhost/api/ai/command', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ ctx: {}, messages: [] }),
      }) as never,
    )

    expect(res.status).toBe(400)
  })

  it('returns 401 when AI Gateway key is missing', async () => {
    const { POST } = await import('@/app/api/ai/command/route')

    const res = await POST(
      new Request('http://localhost/api/ai/command', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          ctx: {
            children: [{ type: 'p', children: [{ text: 'Hello' }] }],
            selection: null,
          },
          messages: [{ role: 'user', content: 'Help me rewrite this' }],
        }),
      }) as never,
    )

    expect(res.status).toBe(401)
    await expect(res.json()).resolves.toMatchObject({ code: 'ai_command_no_key' })
  })
})
