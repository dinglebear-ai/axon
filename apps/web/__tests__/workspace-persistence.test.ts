import { describe, expect, it } from 'vitest'
import {
  buildPersistedPayload,
  clampSplit,
  PULSE_WORKSPACE_STATE_KEY,
  parsePersistedWorkspaceState,
} from '@/lib/pulse/workspace-persistence'

describe('PULSE_WORKSPACE_STATE_KEY', () => {
  it('is the expected localStorage key', () => {
    expect(PULSE_WORKSPACE_STATE_KEY).toBe('axon.web.pulse.workspace-state.v2')
  })
})

describe('clampSplit', () => {
  it('returns value when within range', () => {
    expect(clampSplit(50, 20, 80)).toBe(50)
  })

  it('clamps to min', () => {
    expect(clampSplit(10, 20, 80)).toBe(20)
  })

  it('clamps to max', () => {
    expect(clampSplit(90, 20, 80)).toBe(80)
  })

  it('handles exact boundaries', () => {
    expect(clampSplit(20, 20, 80)).toBe(20)
    expect(clampSplit(80, 20, 80)).toBe(80)
  })
})

describe('parsePersistedWorkspaceState', () => {
  const validState = {
    permissionLevel: 'accept-edits',
    agent: 'claude',
    model: 'sonnet',
    documentMarkdown: '# Hello',
    chatHistory: [{ role: 'user', content: 'hi' }],
    documentTitle: 'Test Doc',
    currentDocFilename: 'test.md',
    chatSessionId: 'session-1',
    indexedSources: ['https://example.com'],
    activeThreadSources: ['https://example.com'],
    desktopSplitPercent: 62,
    mobileSplitPercent: 56,
    lastResponseLatencyMs: 1500,
    lastResponseModel: 'sonnet',
    showChat: true,
    rightPanel: 'editor',
    savedAt: Date.now(),
  }

  it('returns null for null input', () => {
    expect(parsePersistedWorkspaceState(null)).toBeNull()
  })

  it('returns null for empty string', () => {
    expect(parsePersistedWorkspaceState('')).toBeNull()
  })

  it('returns null for invalid JSON', () => {
    expect(parsePersistedWorkspaceState('not json')).toBeNull()
  })

  it('returns null for JSON that is not an object', () => {
    expect(parsePersistedWorkspaceState('"string"')).toBeNull()
    expect(parsePersistedWorkspaceState('42')).toBeNull()
    expect(parsePersistedWorkspaceState('true')).toBeNull()
  })

  it('returns null when documentTitle is missing', () => {
    const { documentTitle: _, ...partial } = validState
    expect(parsePersistedWorkspaceState(JSON.stringify(partial))).toBeNull()
  })

  it('returns null when documentMarkdown is missing', () => {
    const { documentMarkdown: _, ...partial } = validState
    expect(parsePersistedWorkspaceState(JSON.stringify(partial))).toBeNull()
  })

  it('parses valid state correctly', () => {
    const result = parsePersistedWorkspaceState(JSON.stringify(validState))
    expect(result).not.toBeNull()
    expect(result!.model).toBe('sonnet')
    expect(result!.permissionLevel).toBe('accept-edits')
    expect(result!.agent).toBe('claude')
    expect(result!.documentMarkdown).toBe('# Hello')
    expect(result!.documentTitle).toBe('Test Doc')
    expect(result!.showChat).toBe(true)
    expect(result!.rightPanel).toBe('editor')
  })

  it('accepts freeform model values', () => {
    const state = { ...validState, model: 'gpt-4' }
    const result = parsePersistedWorkspaceState(JSON.stringify(state))
    expect(result!.model).toBe('gpt-4')
  })

  it('defaults permissionLevel to bypass-permissions for unknown value', () => {
    const state = { ...validState, permissionLevel: 'unknown' }
    const result = parsePersistedWorkspaceState(JSON.stringify(state))
    expect(result!.permissionLevel).toBe('bypass-permissions')
  })

  it('defaults agent to claude for unknown value', () => {
    const state = { ...validState, agent: 'unknown' }
    const result = parsePersistedWorkspaceState(JSON.stringify(state))
    expect(result!.agent).toBe('claude')
  })

  it('accepts common model ids', () => {
    for (const model of ['sonnet', 'opus', 'haiku', 'o3']) {
      const state = { ...validState, model }
      const result = parsePersistedWorkspaceState(JSON.stringify(state))
      expect(result!.model).toBe(model)
    }
  })

  it('accepts all valid permission levels', () => {
    for (const level of ['plan', 'accept-edits', 'bypass-permissions']) {
      const state = { ...validState, permissionLevel: level }
      const result = parsePersistedWorkspaceState(JSON.stringify(state))
      expect(result!.permissionLevel).toBe(level)
    }
  })

  it('caps chatHistory at 250 entries', () => {
    const history = Array.from({ length: 300 }, (_, i) => ({
      role: 'user' as const,
      content: `msg-${i}`,
    }))
    const state = { ...validState, chatHistory: history }
    const result = parsePersistedWorkspaceState(JSON.stringify(state))
    expect(result!.chatHistory).toHaveLength(250)
    // Should keep the last 250
    expect(result!.chatHistory[0]!.content).toBe('msg-50')
  })

  it('caps indexedSources at 50 entries', () => {
    const sources = Array.from({ length: 100 }, (_, i) => `https://example.com/${i}`)
    const state = { ...validState, indexedSources: sources }
    const result = parsePersistedWorkspaceState(JSON.stringify(state))
    expect(result!.indexedSources).toHaveLength(50)
  })

  it('caps activeThreadSources at 50 entries', () => {
    const sources = Array.from({ length: 100 }, (_, i) => `https://example.com/${i}`)
    const state = { ...validState, activeThreadSources: sources }
    const result = parsePersistedWorkspaceState(JSON.stringify(state))
    expect(result!.activeThreadSources).toHaveLength(50)
  })

  it('clamps desktopSplitPercent to [20, 80]', () => {
    const low = { ...validState, desktopSplitPercent: 5 }
    expect(parsePersistedWorkspaceState(JSON.stringify(low))!.desktopSplitPercent).toBe(20)

    const high = { ...validState, desktopSplitPercent: 95 }
    expect(parsePersistedWorkspaceState(JSON.stringify(high))!.desktopSplitPercent).toBe(80)
  })

  it('clamps mobileSplitPercent to [35, 70]', () => {
    const low = { ...validState, mobileSplitPercent: 10 }
    expect(parsePersistedWorkspaceState(JSON.stringify(low))!.mobileSplitPercent).toBe(35)

    const high = { ...validState, mobileSplitPercent: 90 }
    expect(parsePersistedWorkspaceState(JSON.stringify(high))!.mobileSplitPercent).toBe(70)
  })

  it('defaults NaN desktopSplitPercent to 62', () => {
    const state = { ...validState, desktopSplitPercent: 'not a number' }
    const result = parsePersistedWorkspaceState(JSON.stringify(state))
    expect(result!.desktopSplitPercent).toBe(62)
  })

  it('defaults null currentDocFilename', () => {
    const state = { ...validState, currentDocFilename: 42 }
    const result = parsePersistedWorkspaceState(JSON.stringify(state))
    expect(result!.currentDocFilename).toBeNull()
  })

  it('defaults null chatSessionId for non-string', () => {
    const state = { ...validState, chatSessionId: 42 }
    const result = parsePersistedWorkspaceState(JSON.stringify(state))
    expect(result!.chatSessionId).toBeNull()
  })

  it('defaults lastResponseLatencyMs to null for non-number', () => {
    const state = { ...validState, lastResponseLatencyMs: 'fast' }
    const result = parsePersistedWorkspaceState(JSON.stringify(state))
    expect(result!.lastResponseLatencyMs).toBeNull()
  })

  it('accepts freeform lastResponseModel', () => {
    const state = { ...validState, lastResponseModel: 'gpt-4' }
    const result = parsePersistedWorkspaceState(JSON.stringify(state))
    expect(result!.lastResponseModel).toBe('gpt-4')
  })

  it('handles missing chatHistory gracefully', () => {
    const { chatHistory: _, ...state } = validState
    const result = parsePersistedWorkspaceState(JSON.stringify(state))
    expect(result!.chatHistory).toEqual([])
  })

  it('handles missing indexedSources gracefully', () => {
    const { indexedSources: _, ...state } = validState
    const result = parsePersistedWorkspaceState(JSON.stringify(state))
    expect(result!.indexedSources).toEqual([])
  })

  it('ensures chat visible when rightPanel is null — forces showChat true', () => {
    const state = { ...validState, showChat: false, rightPanel: null }
    const result = parsePersistedWorkspaceState(JSON.stringify(state))
    expect(result!.showChat).toBe(true)
    expect(result!.rightPanel).toBeNull()
  })

  it('migrates old showEditor=true to rightPanel=editor', () => {
    const oldState = {
      ...validState,
      rightPanel: undefined,
      showEditor: true,
    }
    const result = parsePersistedWorkspaceState(JSON.stringify(oldState))
    expect(result!.rightPanel).toBe('editor')
  })

  it('migrates old showEditor=false to rightPanel=null', () => {
    const oldState = {
      ...validState,
      rightPanel: undefined,
      showEditor: false,
    }
    const result = parsePersistedWorkspaceState(JSON.stringify(oldState))
    expect(result!.rightPanel).toBeNull()
  })
})

describe('buildPersistedPayload', () => {
  const baseInput = {
    permissionLevel: 'accept-edits' as const,
    agent: 'claude' as const,
    model: 'sonnet' as const,
    documentMarkdown: '# Hello',
    chatHistory: [{ role: 'user' as const, content: 'hi' }],
    documentTitle: 'Test Doc',
    currentDocFilename: 'test.md',
    chatSessionId: 'session-1',
    indexedSources: ['https://example.com'],
    activeThreadSources: ['https://example.com'],
    desktopSplitPercent: 62,
    mobileSplitPercent: 56,
    lastResponseLatencyMs: 1500,
    lastResponseModel: 'sonnet' as const,
    showChat: true,
    rightPanel: 'editor' as const,
  }

  it('adds savedAt timestamp', () => {
    const before = Date.now()
    const result = buildPersistedPayload(baseInput)
    expect(result.savedAt).toBeGreaterThanOrEqual(before)
    expect(result.savedAt).toBeLessThanOrEqual(Date.now())
  })

  it('caps chatHistory at 250', () => {
    const history = Array.from({ length: 300 }, (_, i) => ({
      role: 'user' as const,
      content: `msg-${i}`,
    }))
    const result = buildPersistedPayload({ ...baseInput, chatHistory: history })
    expect(result.chatHistory).toHaveLength(250)
  })

  it('caps indexedSources at 50', () => {
    const sources = Array.from({ length: 100 }, (_, i) => `https://example.com/${i}`)
    const result = buildPersistedPayload({ ...baseInput, indexedSources: sources })
    expect(result.indexedSources).toHaveLength(50)
  })

  it('caps activeThreadSources at 50', () => {
    const sources = Array.from({ length: 100 }, (_, i) => `https://example.com/${i}`)
    const result = buildPersistedPayload({ ...baseInput, activeThreadSources: sources })
    expect(result.activeThreadSources).toHaveLength(50)
  })

  it('preserves all fields from input', () => {
    const result = buildPersistedPayload(baseInput)
    expect(result.permissionLevel).toBe('accept-edits')
    expect(result.agent).toBe('claude')
    expect(result.model).toBe('sonnet')
    expect(result.documentMarkdown).toBe('# Hello')
    expect(result.documentTitle).toBe('Test Doc')
    expect(result.currentDocFilename).toBe('test.md')
    expect(result.showChat).toBe(true)
    expect(result.rightPanel).toBe('editor')
  })

  it('roundtrips through parse correctly', () => {
    const payload = buildPersistedPayload(baseInput)
    const parsed = parsePersistedWorkspaceState(JSON.stringify(payload))
    expect(parsed).not.toBeNull()
    expect(parsed!.model).toBe(payload.model)
    expect(parsed!.documentTitle).toBe(payload.documentTitle)
    expect(parsed!.chatHistory).toEqual(payload.chatHistory)
  })
})
