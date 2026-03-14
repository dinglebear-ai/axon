import { afterEach, describe, expect, it, vi } from 'vitest'
import { TerminalHistory } from '@/lib/terminal-history'

const HISTORY_KEY = 'axon.web.terminal.history'
const LEGACY_HISTORY_KEY = 'axon.terminal.history'

// TerminalHistory checks `typeof window === 'undefined'` for SSR safety.
// In Vitest's node environment, window doesn't exist, so we must stub both
// window and localStorage for the class to engage its storage path.

function makeStorage(initial: Record<string, string> = {}) {
  const data = { ...initial }
  return {
    data,
    mock: {
      getItem: vi.fn((key: string) => data[key] ?? null),
      setItem: vi.fn((key: string, value: string) => {
        data[key] = value
      }),
      removeItem: vi.fn((key: string) => {
        delete data[key]
      }),
    },
  }
}

function stubWindow(storage: ReturnType<typeof makeStorage>) {
  vi.stubGlobal('window', { localStorage: storage.mock })
  vi.stubGlobal('localStorage', storage.mock)
}

describe('TerminalHistory', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('starts empty', () => {
    const s = makeStorage()
    stubWindow(s)
    const h = new TerminalHistory()
    expect(h.getAll()).toEqual([])
  })

  it('push adds a command', () => {
    const s = makeStorage()
    stubWindow(s)
    const h = new TerminalHistory()
    h.push('ls')
    expect(h.getAll()).toEqual(['ls'])
  })

  it('push trims whitespace', () => {
    const s = makeStorage()
    stubWindow(s)
    const h = new TerminalHistory()
    h.push('  ls  ')
    expect(h.getAll()).toEqual(['ls'])
  })

  it('push skips empty strings', () => {
    const s = makeStorage()
    stubWindow(s)
    const h = new TerminalHistory()
    h.push('')
    h.push('   ')
    expect(h.getAll()).toEqual([])
  })

  it('push deduplicates consecutive identical commands', () => {
    const s = makeStorage()
    stubWindow(s)
    const h = new TerminalHistory()
    h.push('ls')
    h.push('ls')
    h.push('ls')
    expect(h.getAll()).toEqual(['ls'])
  })

  it('push allows non-consecutive duplicates', () => {
    const s = makeStorage()
    stubWindow(s)
    const h = new TerminalHistory()
    h.push('ls')
    h.push('cd')
    h.push('ls')
    expect(h.getAll()).toEqual(['ls', 'cd', 'ls'])
  })

  it('push caps at MAX_HISTORY (500)', () => {
    const s = makeStorage()
    stubWindow(s)
    const h = new TerminalHistory()
    for (let i = 0; i < 510; i++) {
      h.push(`cmd-${i}`)
    }
    const all = h.getAll()
    expect(all).toHaveLength(500)
    expect(all[0]).toBe('cmd-10')
    expect(all[499]).toBe('cmd-509')
  })

  it('push persists to localStorage', () => {
    const s = makeStorage()
    stubWindow(s)
    const h = new TerminalHistory()
    h.push('git status')
    expect(s.mock.setItem).toHaveBeenCalledWith(HISTORY_KEY, expect.any(String))
    const saved = JSON.parse(s.data[HISTORY_KEY]!)
    expect(saved).toEqual(['git status'])
  })

  it('constructor loads from localStorage', () => {
    const s = makeStorage({ [HISTORY_KEY]: JSON.stringify(['old-cmd']) })
    stubWindow(s)
    const h = new TerminalHistory()
    expect(h.getAll()).toEqual(['old-cmd'])
  })

  it('constructor handles corrupt localStorage gracefully', () => {
    const s = makeStorage({ [HISTORY_KEY]: 'not-json' })
    stubWindow(s)
    const h = new TerminalHistory()
    expect(h.getAll()).toEqual([])
  })

  it('constructor filters non-string items', () => {
    const s = makeStorage({
      [HISTORY_KEY]: JSON.stringify(['valid', 42, null, 'also-valid']),
    })
    stubWindow(s)
    const h = new TerminalHistory()
    expect(h.getAll()).toEqual(['valid', 'also-valid'])
  })

  it('constructor migrates legacy storage key', () => {
    const s = makeStorage({ [LEGACY_HISTORY_KEY]: JSON.stringify(['legacy-cmd']) })
    stubWindow(s)
    const h = new TerminalHistory()
    expect(h.getAll()).toEqual(['legacy-cmd'])
    expect(s.data[HISTORY_KEY]).toBe(JSON.stringify(['legacy-cmd']))
    expect(s.data[LEGACY_HISTORY_KEY]).toBeUndefined()
  })

  describe('cursor navigation', () => {
    it('prev returns undefined on empty history', () => {
      const s = makeStorage()
      stubWindow(s)
      const h = new TerminalHistory()
      expect(h.prev()).toBeUndefined()
    })

    it('prev walks backward through entries', () => {
      const s = makeStorage()
      stubWindow(s)
      const h = new TerminalHistory()
      h.push('a')
      h.push('b')
      h.push('c')
      expect(h.prev()).toBe('c')
      expect(h.prev()).toBe('b')
      expect(h.prev()).toBe('a')
    })

    it('prev stops at oldest entry', () => {
      const s = makeStorage()
      stubWindow(s)
      const h = new TerminalHistory()
      h.push('only')
      expect(h.prev()).toBe('only')
      expect(h.prev()).toBe('only')
    })

    it('next returns undefined when at end', () => {
      const s = makeStorage()
      stubWindow(s)
      const h = new TerminalHistory()
      h.push('a')
      expect(h.next()).toBeUndefined()
    })

    it('next walks forward after prev', () => {
      const s = makeStorage()
      stubWindow(s)
      const h = new TerminalHistory()
      h.push('a')
      h.push('b')
      h.push('c')
      h.prev() // c
      h.prev() // b
      h.prev() // a
      expect(h.next()).toBe('b')
      expect(h.next()).toBe('c')
      expect(h.next()).toBeUndefined() // past end
    })

    it('reset moves cursor to end', () => {
      const s = makeStorage()
      stubWindow(s)
      const h = new TerminalHistory()
      h.push('a')
      h.push('b')
      h.prev() // b
      h.prev() // a
      h.reset()
      expect(h.prev()).toBe('b')
    })

    it('push resets cursor', () => {
      const s = makeStorage()
      stubWindow(s)
      const h = new TerminalHistory()
      h.push('a')
      h.push('b')
      h.prev() // b
      h.prev() // a
      h.push('c')
      expect(h.prev()).toBe('c')
    })
  })

  describe('SSR safety', () => {
    it('handles missing window gracefully', () => {
      // Don't stub window — leave it undefined (node default)
      const h = new TerminalHistory()
      expect(h.getAll()).toEqual([])
      h.push('test')
      expect(h.getAll()).toEqual(['test'])
    })
  })
})
