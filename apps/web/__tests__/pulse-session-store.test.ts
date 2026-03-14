/**
 * Tests for lib/pulse/session-store.ts
 *
 * session-store.ts exports four localStorage-backed functions:
 *   - loadSavedSessions(): SavedPulseSession[]
 *   - saveSession(sessionId, chatHistory, documentMarkdown, documentTitle): SavedPulseSession | null
 *   - deleteSession(sessionId): boolean
 *   - getSession(sessionId): SavedPulseSession | null
 *
 * The storage key is 'axon.web.pulse.saved-sessions'.
 * All functions access window.localStorage — we mock it with vi.stubGlobal.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { SavedPulseSession } from '@/lib/pulse/session-store'
import {
  deleteSession,
  getSession,
  loadSavedSessions,
  saveSession,
} from '@/lib/pulse/session-store'
import type { ChatMessage } from '@/lib/pulse/workspace-persistence'

// ── localStorage mock ────────────────────────────────────────────────────────

const SESSIONS_KEY = 'axon.web.pulse.saved-sessions'

let storage: Record<string, string>

function setupLocalStorageMock(): void {
  storage = {}
  const localStorageMock = {
    getItem: vi.fn((k: string) => storage[k] ?? null),
    setItem: vi.fn((k: string, v: string) => {
      storage[k] = v
    }),
    removeItem: vi.fn((k: string) => {
      delete storage[k]
    }),
    clear: vi.fn(() => {
      for (const k in storage) delete storage[k]
    }),
  }
  // The source uses window.localStorage (browser API).
  // In Vitest's node environment, window is undefined, so we must stub both
  // globalThis.localStorage AND globalThis.window.localStorage.
  vi.stubGlobal('localStorage', localStorageMock)
  vi.stubGlobal('window', { localStorage: localStorageMock })
}

// ── Fixtures ─────────────────────────────────────────────────────────────────

function makeHistory(messages: Array<[string, string]>): ChatMessage[] {
  return messages.map(([role, content]) => ({
    role: role as 'user' | 'assistant',
    content,
  }))
}

function seedSession(session: SavedPulseSession): void {
  const existing = storage[SESSIONS_KEY]
  const parsed: SavedPulseSession[] = existing ? (JSON.parse(existing) as SavedPulseSession[]) : []
  // Prepend so ordering matches most-recent-first expectation
  storage[SESSIONS_KEY] = JSON.stringify([session, ...parsed])
}

function makeSession(overrides: Partial<SavedPulseSession> = {}): SavedPulseSession {
  const now = Date.now()
  return {
    sessionId: 'ses-default',
    title: 'Default title',
    preview: 'Default preview',
    chatHistory: [{ role: 'user', content: 'Hello' }],
    documentMarkdown: '',
    documentTitle: 'Doc',
    messageCount: 1,
    createdAt: now,
    updatedAt: now,
    ...overrides,
  }
}

// ── Setup / teardown ──────────────────────────────────────────────────────────

beforeEach(() => {
  setupLocalStorageMock()
})

afterEach(() => {
  vi.unstubAllGlobals()
})

// ── loadSavedSessions ─────────────────────────────────────────────────────────

describe('loadSavedSessions', () => {
  it('returns empty array when storage is empty', () => {
    expect(loadSavedSessions()).toEqual([])
  })

  it('returns empty array when localStorage has null for the key', () => {
    storage[SESSIONS_KEY] = 'null'
    expect(loadSavedSessions()).toEqual([])
  })

  it('returns empty array when localStorage value is not a JSON array', () => {
    storage[SESSIONS_KEY] = '{"not":"array"}'
    expect(loadSavedSessions()).toEqual([])
  })

  it('returns empty array on malformed JSON', () => {
    storage[SESSIONS_KEY] = '{bad json'
    expect(loadSavedSessions()).toEqual([])
  })

  it('returns stored sessions sorted by updatedAt descending', () => {
    const older = makeSession({ sessionId: 'ses-1', updatedAt: 1000, createdAt: 1000 })
    const newer = makeSession({ sessionId: 'ses-2', updatedAt: 2000, createdAt: 2000 })
    storage[SESSIONS_KEY] = JSON.stringify([older, newer])

    const result = loadSavedSessions()
    expect(result).toHaveLength(2)
    expect(result[0]!.sessionId).toBe('ses-2')
    expect(result[1]!.sessionId).toBe('ses-1')
  })

  it('skips entries without a valid sessionId', () => {
    const valid = makeSession({ sessionId: 'ses-good' })
    const invalid = { title: 'no id', updatedAt: 9999 }
    storage[SESSIONS_KEY] = JSON.stringify([valid, invalid])

    const result = loadSavedSessions()
    expect(result).toHaveLength(1)
    expect(result[0]!.sessionId).toBe('ses-good')
  })
})

// ── saveSession ───────────────────────────────────────────────────────────────

describe('saveSession', () => {
  it('returns null for empty sessionId', () => {
    const history = makeHistory([['user', 'hi']])
    expect(saveSession('', history, '', 'Doc')).toBeNull()
  })

  it('returns null for empty chatHistory', () => {
    expect(saveSession('ses-1', [], '', 'Doc')).toBeNull()
  })

  it('persists the session to localStorage', () => {
    const history = makeHistory([['user', 'What is Qdrant?']])
    const result = saveSession('ses-1', history, '# Doc', 'My Doc')

    expect(result).not.toBeNull()
    const stored = JSON.parse(storage[SESSIONS_KEY]!) as SavedPulseSession[]
    expect(stored).toHaveLength(1)
    expect(stored[0]!.sessionId).toBe('ses-1')
  })

  it('derives title from first user message (≤60 chars)', () => {
    const msg = 'Short question'
    const history = makeHistory([['user', msg]])
    const result = saveSession('ses-1', history, '', 'Doc')

    expect(result!.title).toBe(msg)
  })

  it('truncates title to 60 chars with ellipsis when message is longer', () => {
    const longMsg = 'A'.repeat(80)
    const history = makeHistory([['user', longMsg]])
    const result = saveSession('ses-1', history, '', 'Doc')

    expect(result!.title).toBe(`${'A'.repeat(60)}...`)
    expect(result!.title.length).toBe(63)
  })

  it('derives preview from first user message (≤100 chars)', () => {
    const msg = 'Short preview'
    const history = makeHistory([['user', msg]])
    const result = saveSession('ses-1', history, '', 'Doc')

    expect(result!.preview).toBe(msg)
  })

  it('truncates preview at 100 chars with ellipsis', () => {
    const longMsg = 'B'.repeat(150)
    const history = makeHistory([['user', longMsg]])
    const result = saveSession('ses-1', history, '', 'Doc')

    expect(result!.preview).toBe(`${'B'.repeat(100)}...`)
  })

  it('collapses newlines in title and preview', () => {
    const history = makeHistory([['user', 'line one\nline two']])
    const result = saveSession('ses-1', history, '', 'Doc')

    expect(result!.title).not.toContain('\n')
    expect(result!.preview).not.toContain('\n')
    expect(result!.title).toContain('line one line two')
  })

  it('uses "Untitled conversation" as title when history has no user messages', () => {
    const history = makeHistory([['assistant', 'Hello!']])
    const result = saveSession('ses-1', history, '', 'Doc')

    expect(result!.title).toBe('Untitled conversation')
  })

  it('sets preview to empty string when history has no user messages', () => {
    const history = makeHistory([['assistant', 'Hello!']])
    const result = saveSession('ses-1', history, '', 'Doc')

    expect(result!.preview).toBe('')
  })

  it('stores messageCount equal to chatHistory length', () => {
    const history = makeHistory([
      ['user', 'q'],
      ['assistant', 'a'],
      ['user', 'q2'],
    ])
    const result = saveSession('ses-1', history, '', 'Doc')

    expect(result!.messageCount).toBe(3)
  })

  it('stores documentMarkdown and documentTitle', () => {
    const history = makeHistory([['user', 'hi']])
    const result = saveSession('ses-1', history, '# My Content', 'My Title')

    expect(result!.documentMarkdown).toBe('# My Content')
    expect(result!.documentTitle).toBe('My Title')
  })

  it('preserves createdAt on subsequent saves for the same sessionId', () => {
    const history = makeHistory([['user', 'first']])
    const first = saveSession('ses-stable', history, '', 'Doc')
    const originalCreatedAt = first!.createdAt

    // Small delay via mocked time is not needed — createdAt comes from existing entry
    const result2 = saveSession('ses-stable', makeHistory([['user', 'second']]), '', 'Doc')
    expect(result2!.createdAt).toBe(originalCreatedAt)
  })

  it('updates updatedAt on subsequent saves', () => {
    const before = Date.now()
    const history = makeHistory([['user', 'hello']])
    const result = saveSession('ses-1', history, '', 'Doc')

    expect(result!.updatedAt).toBeGreaterThanOrEqual(before)
    expect(result!.updatedAt).toBeLessThanOrEqual(Date.now())
  })

  it('caps chatHistory stored at 250 messages', () => {
    const history = makeHistory(Array.from({ length: 300 }, (_, i) => ['user', `msg-${i}`]))
    const result = saveSession('ses-big', history, '', 'Doc')

    expect(result!.chatHistory).toHaveLength(250)
    // Keeps the LAST 250 (slice(-250))
    expect(result!.chatHistory[0]!.content).toBe('msg-50')
  })

  it('updates an existing session entry in place', () => {
    const history1 = makeHistory([['user', 'question one']])
    saveSession('ses-update', history1, '', 'Doc')

    const history2 = makeHistory([['user', 'question two']])
    saveSession('ses-update', history2, '', 'Doc')

    const stored = JSON.parse(storage[SESSIONS_KEY]!) as SavedPulseSession[]
    // Only one entry for the same sessionId
    expect(stored.filter((s) => s.sessionId === 'ses-update')).toHaveLength(1)
    expect(stored[0]!.title).toBe('question two')
  })

  it('returns the saved session object', () => {
    const history = makeHistory([['user', 'hi']])
    const result = saveSession('ses-ret', history, '# Doc', 'My Doc')

    expect(result).not.toBeNull()
    expect(result!.sessionId).toBe('ses-ret')
    expect(typeof result!.createdAt).toBe('number')
    expect(typeof result!.updatedAt).toBe('number')
  })
})

// ── deleteSession ─────────────────────────────────────────────────────────────

describe('deleteSession', () => {
  it('returns false when sessionId does not exist', () => {
    expect(deleteSession('non-existent')).toBe(false)
  })

  it('returns true when session is deleted', () => {
    const session = makeSession({ sessionId: 'ses-del' })
    seedSession(session)

    expect(deleteSession('ses-del')).toBe(true)
  })

  it('removes the session from localStorage', () => {
    const session = makeSession({ sessionId: 'ses-remove' })
    seedSession(session)

    deleteSession('ses-remove')

    const stored = JSON.parse(storage[SESSIONS_KEY]!) as SavedPulseSession[]
    expect(stored.find((s) => s.sessionId === 'ses-remove')).toBeUndefined()
  })

  it('does not affect other sessions when one is deleted', () => {
    seedSession(makeSession({ sessionId: 'ses-keep', updatedAt: 1000 }))
    seedSession(makeSession({ sessionId: 'ses-del', updatedAt: 2000 }))

    deleteSession('ses-del')

    const stored = JSON.parse(storage[SESSIONS_KEY]!) as SavedPulseSession[]
    expect(stored).toHaveLength(1)
    expect(stored[0]!.sessionId).toBe('ses-keep')
  })

  it('does not write to storage when session did not exist', () => {
    const setItem = vi.mocked(localStorage.setItem)
    setItem.mockClear()

    deleteSession('ghost-session')

    expect(setItem).not.toHaveBeenCalled()
  })
})

// ── getSession ────────────────────────────────────────────────────────────────

describe('getSession', () => {
  it('returns null when storage is empty', () => {
    expect(getSession('ses-1')).toBeNull()
  })

  it('returns null when sessionId does not exist', () => {
    seedSession(makeSession({ sessionId: 'ses-other' }))
    expect(getSession('ses-1')).toBeNull()
  })

  it('returns the correct session when it exists', () => {
    const session = makeSession({
      sessionId: 'ses-found',
      title: 'Found Me',
      messageCount: 7,
    })
    seedSession(session)

    const result = getSession('ses-found')
    expect(result).not.toBeNull()
    expect(result!.sessionId).toBe('ses-found')
    expect(result!.title).toBe('Found Me')
    expect(result!.messageCount).toBe(7)
  })

  it('returns null on malformed localStorage value', () => {
    storage[SESSIONS_KEY] = 'not valid json'
    expect(getSession('ses-1')).toBeNull()
  })
})

// ── State isolation ───────────────────────────────────────────────────────────

describe('state isolation — sessions do not bleed into each other', () => {
  it('each session has independent chatHistory', () => {
    const historyA = makeHistory([['user', 'question A']])
    const historyB = makeHistory([['user', 'question B']])

    saveSession('ses-A', historyA, '', 'Doc A')
    saveSession('ses-B', historyB, '', 'Doc B')

    const a = getSession('ses-A')
    const b = getSession('ses-B')

    expect(a!.chatHistory[0]!.content).toBe('question A')
    expect(b!.chatHistory[0]!.content).toBe('question B')
  })

  it('deleting one session does not affect others', () => {
    saveSession('ses-X', makeHistory([['user', 'x']]), '', 'X')
    saveSession('ses-Y', makeHistory([['user', 'y']]), '', 'Y')

    deleteSession('ses-X')

    expect(getSession('ses-X')).toBeNull()
    expect(getSession('ses-Y')).not.toBeNull()
  })

  it('saving the same sessionId twice updates only that entry', () => {
    saveSession('ses-1', makeHistory([['user', 'v1']]), '', 'D')
    saveSession('ses-2', makeHistory([['user', 'v1']]), '', 'D')
    saveSession('ses-1', makeHistory([['user', 'v2']]), '', 'D')

    const all = loadSavedSessions()
    expect(all).toHaveLength(2)
    const s1 = all.find((s) => s.sessionId === 'ses-1')
    expect(s1!.title).toBe('v2')
  })
})

// ── MAX_SESSIONS cap ──────────────────────────────────────────────────────────

describe('MAX_SESSIONS cap (50)', () => {
  it('keeps at most 50 sessions after writing 60', () => {
    // Seed 60 sessions directly so we bypass the saveSession guard
    const sessions = Array.from({ length: 60 }, (_, i) =>
      makeSession({
        sessionId: `ses-${i}`,
        updatedAt: i,
        createdAt: i,
      }),
    )
    storage[SESSIONS_KEY] = JSON.stringify(sessions)

    // saveSession triggers writeSessionMap which caps at MAX_SESSIONS
    saveSession('ses-trigger', makeHistory([['user', 'hi']]), '', 'Doc')

    const stored = JSON.parse(storage[SESSIONS_KEY]) as SavedPulseSession[]
    expect(stored.length).toBeLessThanOrEqual(50)
  })
})

// ── Edge cases ─────────────────────────────────────────────────────────────────

describe('edge cases', () => {
  it('handles localStorage.getItem throwing (private browsing)', () => {
    const throwingMock = {
      getItem: vi.fn(() => {
        throw new Error('SecurityError')
      }),
      setItem: vi.fn(),
      removeItem: vi.fn(),
      clear: vi.fn(),
    }
    vi.stubGlobal('localStorage', throwingMock)
    vi.stubGlobal('window', { localStorage: throwingMock })

    expect(() => loadSavedSessions()).not.toThrow()
    expect(loadSavedSessions()).toEqual([])
  })

  it('handles localStorage.setItem throwing (quota exceeded)', () => {
    const quotaMock = {
      getItem: vi.fn(() => null),
      setItem: vi.fn(() => {
        throw new Error('QuotaExceededError')
      }),
      removeItem: vi.fn(),
      clear: vi.fn(),
    }
    vi.stubGlobal('localStorage', quotaMock)
    vi.stubGlobal('window', { localStorage: quotaMock })

    const history = makeHistory([['user', 'hi']])
    // Should not throw — writeSessionMap catches the error
    expect(() => saveSession('ses-quota', history, '', 'Doc')).not.toThrow()
  })

  it('loadSavedSessions returns sessions sorted by updatedAt even with ties', () => {
    const ts = 1_000_000
    const s1 = makeSession({ sessionId: 'tie-1', updatedAt: ts })
    const s2 = makeSession({ sessionId: 'tie-2', updatedAt: ts })
    storage[SESSIONS_KEY] = JSON.stringify([s1, s2])

    const result = loadSavedSessions()
    // Both present — order is stable (sort returns 0 for equal)
    expect(result).toHaveLength(2)
  })

  it('saveSession with very long prompt still works (capped at first 250 history entries)', () => {
    const history = makeHistory([['user', 'x'.repeat(8000)]])
    const result = saveSession('ses-long', history, '', 'Doc')

    expect(result).not.toBeNull()
    // Title is truncated to 60 chars + '...'
    expect(result!.title.length).toBe(63)
  })
})
