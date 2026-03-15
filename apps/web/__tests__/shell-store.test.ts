import { beforeEach, describe, expect, it } from 'vitest'
import { useShellStore } from '@/lib/shell-store'

describe('shell-store', () => {
  beforeEach(() => {
    useShellStore.setState({
      liveMessages: [],
      liveMessagesHydrated: false,
      isStreaming: false,
      connected: false,
      activeSessionId: null,
      activeAssistantSessionId: null,
      sessionKey: 0,
      pendingHandoffContext: null,
      sessionMode: '',
      editorMarkdown: '# New document\n',
      activeFile: '',
      railMode: 'sessions',
      mobilePane: 'chat',
      sidebarOpen: true,
      chatOpen: true,
      rightPane: 'editor',
      density: 'high',
      canvasProfile: 'current',
      sidebarWidth: 260,
      chatFlex: 1,
      isDragging: false,
      layoutRestored: false,
      railQuery: '',
      enableFs: true,
      enableTerminal: true,
      permissionTimeoutSecs: null,
      adapterTimeoutSecs: null,
      workspaceMode: null,
      workspacePrompt: null,
      workspacePromptVersion: 0,
      workspaceResumeSessionId: null,
      workspaceResumeVersion: 0,
      workspaceContext: null,
      pulseAgent: 'claude',
      pulseModel: 'sonnet',
      pulsePermissionLevel: 'accept-edits',
      acpConfigOptions: [],
    })
  })

  it('updates message slice independently', () => {
    const before = useShellStore.getState()

    before.setLiveMessages([{ role: 'user', content: 'hello' } as never])
    before.setLiveMessagesHydrated(true)

    const after = useShellStore.getState()
    expect(after.liveMessagesHydrated).toBe(true)
    expect(after.liveMessages).toHaveLength(1)
    expect(after.sidebarWidth).toBe(260)
  })

  it('increments session key via action', () => {
    const state = useShellStore.getState()
    expect(state.sessionKey).toBe(0)

    state.incrementSessionKey()
    expect(useShellStore.getState().sessionKey).toBe(1)
  })

  it('updates layout fields with functional setters', () => {
    const state = useShellStore.getState()

    state.setSidebarWidth((prev) => prev + 40)
    state.setChatFlex((prev) => prev + 0.5)
    state.setRailQuery('jobs')

    const after = useShellStore.getState()
    expect(after.sidebarWidth).toBe(300)
    expect(after.chatFlex).toBe(1.5)
    expect(after.railQuery).toBe('jobs')
  })

  it('updates pulse settings through dedicated actions', () => {
    const state = useShellStore.getState()

    state.setPulseAgent('codex')
    state.setPulseModel('opus')
    state.setPulsePermissionLevel('bypass-permissions')

    const after = useShellStore.getState()
    expect(after.pulseAgent).toBe('codex')
    expect(after.pulseModel).toBe('opus')
    expect(after.pulsePermissionLevel).toBe('bypass-permissions')
  })

  it('owns workspace pulse state through dedicated actions', () => {
    const state = useShellStore.getState()

    state.setWorkspaceMode('pulse')
    state.setWorkspacePrompt('Summarize the crawl results')
    state.bumpWorkspacePromptVersion()
    state.setWorkspaceResumeSessionId('session-123')
    state.bumpWorkspaceResumeVersion()
    state.setWorkspaceContext({
      turns: 2,
      sourceCount: 3,
      threadSourceCount: 1,
      contextCharsTotal: 1200,
      contextBudgetChars: 8000,
      lastLatencyMs: 450,
      agent: 'claude',
      model: 'sonnet',
      permissionLevel: 'accept-edits',
    })

    const after = useShellStore.getState()
    expect(after.workspaceMode).toBe('pulse')
    expect(after.workspacePrompt).toBe('Summarize the crawl results')
    expect(after.workspacePromptVersion).toBe(1)
    expect(after.workspaceResumeSessionId).toBe('session-123')
    expect(after.workspaceResumeVersion).toBe(1)
    expect(after.workspaceContext).toMatchObject({
      turns: 2,
      sourceCount: 3,
      agent: 'claude',
      model: 'sonnet',
    })
  })
})
