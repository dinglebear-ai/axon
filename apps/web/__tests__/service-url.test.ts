import { describe, expect, it } from 'vitest'
import { normalizeLocalServiceUrl } from '@/lib/server/service-url'

describe('normalizeLocalServiceUrl', () => {
  it('rewrites known docker service hosts when running outside Docker', () => {
    expect(
      normalizeLocalServiceUrl('redis://:secret@axon-redis:6379', {
        runningInDocker: false,
      }),
    ).toBe('redis://:secret@127.0.0.1:53379')

    expect(
      normalizeLocalServiceUrl('http://axon-tei:80', {
        runningInDocker: false,
      }),
    ).toBe('http://127.0.0.1:52000')
  })

  it('keeps docker service hosts unchanged when running inside Docker', () => {
    expect(
      normalizeLocalServiceUrl('redis://:secret@axon-redis:6379', {
        runningInDocker: true,
      }),
    ).toBe('redis://:secret@axon-redis:6379')
  })

  it('returns malformed values unchanged', () => {
    expect(
      normalizeLocalServiceUrl('not-a-url', {
        runningInDocker: false,
      }),
    ).toBe('not-a-url')
  })
})
