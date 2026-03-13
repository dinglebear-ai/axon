// @vitest-environment jsdom
import { renderHook, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useAxonSession } from '@/hooks/use-axon-session'

describe('useAxonSession', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn())
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('returns empty messages for null sessionId', () => {
    const { result } = renderHook(() => useAxonSession(null))
    expect(result.current.messages).toEqual([])
    expect(result.current.loading).toBe(false)
  })

  it('fetches and converts messages for a real sessionId', async () => {
    vi.mocked(fetch).mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        project: 'axon',
        filename: 'session-abc',
        sessionId: 'abc-123',
        messages: [
          { role: 'user', content: 'hello' },
          { role: 'assistant', content: 'hi there' },
        ],
      }),
    } as Response)

    const { result } = renderHook(() => useAxonSession('abc-123'))

    await waitFor(() => expect(result.current.loading).toBe(false))
    expect(result.current.messages).toHaveLength(2)
    expect(result.current.messages[0]!.role).toBe('user')
    expect(result.current.messages[0]!.content).toBe('hello')
  })

  it('strips inline system wrapper from user messages', async () => {
    vi.mocked(fetch).mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        project: 'assistant',
        filename: 'session-abc',
        sessionId: 'abc-123',
        messages: [
          {
            role: 'user',
            content:
              '[System context — Axon editor integration] guidance text [User message] Hello there',
          },
          { role: 'assistant', content: 'Hi' },
        ],
      }),
    } as Response)

    const { result } = renderHook(() => useAxonSession('abc-123'))
    await waitFor(() => expect(result.current.loading).toBe(false))
    expect(result.current.messages[0]!.content).toBe('Hello there')
  })

  it('strips newline marker wrapper from user messages', async () => {
    vi.mocked(fetch).mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        project: 'assistant',
        filename: 'session-abc',
        sessionId: 'abc-123',
        messages: [
          {
            role: 'user',
            content:
              '[System context — Axon editor integration]\\n[User message]\\nBuild me a changelog',
          },
        ],
      }),
    } as Response)

    const { result } = renderHook(() => useAxonSession('abc-123'))
    await waitFor(() => expect(result.current.loading).toBe(false))
    expect(result.current.messages[0]!.content).toBe('Build me a changelog')
  })

  it('sets error on fetch failure', async () => {
    vi.mocked(fetch).mockResolvedValueOnce({
      ok: false,
      status: 404,
    } as Response)

    const { result } = renderHook(() => useAxonSession('bad-id'))

    await waitFor(() => expect(result.current.loading).toBe(false))
    expect(result.current.error).not.toBeNull()
    expect(result.current.messages).toEqual([])
  })

  it('clears stale messages immediately when sessionId changes', async () => {
    let resolveSecond: ((value: any) => void) | null = null
    const secondResponse = new Promise<Response>((resolve) => {
      resolveSecond = resolve
    })

    vi.mocked(fetch)
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({
          project: 'assistant',
          filename: 'session-1',
          sessionId: 'session-1',
          messages: [{ role: 'assistant', content: 'from first session' }],
        }),
      } as Response)
      .mockImplementationOnce(() => secondResponse)

    const { result, rerender } = renderHook(({ sessionId }) => useAxonSession(sessionId), {
      initialProps: { sessionId: 'session-1' },
    })

    await waitFor(() => expect(result.current.loading).toBe(false))
    expect(result.current.messages.map((m) => m.content)).toEqual(['from first session'])

    rerender({ sessionId: 'session-2' })
    expect(result.current.loading).toBe(true)
    expect(result.current.messages).toEqual([])

    ;(resolveSecond as any)?.({
      ok: true,
      json: async () => ({
        project: 'assistant',
        filename: 'session-2',
        sessionId: 'session-2',
        messages: [{ role: 'assistant', content: 'from second session' }],
      }),
    } as any as Response)

    await waitFor(() => expect(result.current.loading).toBe(false))
    expect(result.current.messages.map((m) => m.content)).toEqual(['from second session'])
  })
})
