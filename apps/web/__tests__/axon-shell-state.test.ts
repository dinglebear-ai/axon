// @vitest-environment jsdom

import { act, renderHook } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

const {
  actionState,
  copyState,
  layoutState,
  messageState,
  sessionState,
  settingsState,
  shellState,
} = vi.hoisted(() => ({
  copyState: {
    copiedId: 'copy-1',
    copy: vi.fn(),
  },
  layoutState: {
    canvasProfile: 'current',
    chatFlex: 1,
    chatOpen: true,
    editorOpen: true,
    handleCanvasProfileChange: vi.fn(),
    isDragging: false,
    layoutRestored: true,
    mobilePane: 'chat',
    nudgeChatFlex: vi.fn(),
    nudgeSidebar: vi.fn(),
    persistChatOpen: vi.fn(),
    persistRightPane: vi.fn(),
    persistSidebarOpen: vi.fn(),
    railMode: 'sessions',
    resetChatFlex: vi.fn(),
    resetSidebarWidth: vi.fn(),
    rightPane: 'editor',
    sectionRef: { current: null },
    setMobilePaneTracked: vi.fn(),
    setRailModeTracked: vi.fn(),
    sidebarOpen: true,
    sidebarWidth: 260,
    startChatResize: vi.fn(),
    startSidebarResize: vi.fn(),
    transitionClass: 'transition',
    density: 'high',
    setDensityTracked: vi.fn(),
  },
  sessionState: {
    activeSessionId: 'session-1',
    setActiveSessionId: vi.fn(),
    activeAssistantSessionId: null,
    setActiveAssistantSessionId: vi.fn(),
    chatSessionId: 'session-1',
    historicalMessages: [{ id: 'm-1', role: 'assistant', content: 'hi' }],
    sessionLoading: false,
    sessionError: null,
    reloadSession: vi.fn(),
    rawSessions: [{ id: 'session-1', preview: 'Preview title', project: 'axon_rust' }],
    reloadSessions: vi.fn(),
    assistantSessions: [],
    reloadAssistantSessions: vi.fn(),
    onSessionIdChange: vi.fn(),
  },
  messageState: {
    liveMessages: [{ id: 'm-live', role: 'user', content: 'hello' }],
    liveMessagesHydrated: true,
    setLiveMessages: vi.fn(),
    setLiveMessagesHydrated: vi.fn(),
    onMessagesChange: vi.fn(),
    persistMessages: vi.fn(),
  },
  settingsState: {
    enableFs: true,
    setEnableFs: vi.fn(),
    enableTerminal: true,
    setEnableTerminal: vi.fn(),
    permissionTimeoutSecs: 30,
    setPermissionTimeoutSecs: vi.fn(),
    adapterTimeoutSecs: 60,
    setAdapterTimeoutSecs: vi.fn(),
  },
  actionState: {
    composerProps: { onSubmit: vi.fn(), files: [] },
    handleEditMessage: vi.fn(),
    handleMobileFileSelect: vi.fn(),
    handleMobileNewSession: vi.fn(),
    handleMobileOpenFile: vi.fn(),
    handleMobileSelectSession: vi.fn(),
    handleRetryMessage: vi.fn(),
    handleSelectSession: vi.fn(),
    handleSidebarFileSelect: vi.fn(),
    handleStats: vi.fn(),
    openFile: vi.fn(),
    sidebarProps: { sessions: [], railMode: 'sessions' },
  },
  shellState: {
    sessionKey: 2,
    pendingHandoffContext: null,
    setPendingHandoffContext: vi.fn(),
    sessionMode: 'accept-edits',
    setSessionMode: vi.fn(),
    pulseAgent: 'claude',
    pulseModel: 'sonnet',
    pulsePermissionLevel: 'accept-edits',
    acpConfigOptions: [],
    setPulseAgent: vi.fn(),
    setPulseModel: vi.fn(),
    setPulsePermissionLevel: vi.fn(),
    setAcpConfigOptions: vi.fn(),
    editorMarkdown: '# hello',
    setEditorMarkdown: vi.fn(),
    setActiveFile: vi.fn(),
    activeFile: '',
    railQuery: '',
    setRailQuery: vi.fn(),
  },
}))

vi.mock('@/hooks/use-copy-feedback', () => ({
  useCopyFeedback: () => copyState,
}))

vi.mock('@/hooks/use-mcp-servers', () => ({
  useMcpServers: () => ({
    enabledMcpServers: ['filesystem'],
    mcpServers: ['filesystem'],
    mcpStatusByServer: { filesystem: 'online' },
    setEnabledMcpServers: vi.fn(),
    toggleMcpServer: vi.fn(),
  }),
}))

vi.mock('@/hooks/use-workspace-files', () => ({
  useWorkspaceFiles: () => ({
    fileEntries: [],
    fileLoading: false,
    selectedFilePath: null,
    setSelectedFilePath: vi.fn(),
  }),
}))

vi.mock('@/hooks/use-axon-ws', () => ({
  useAxonWs: () => ({
    send: vi.fn(),
    subscribeByTypes: vi.fn(() => vi.fn()),
  }),
}))

vi.mock('@/hooks/use-axon-acp', () => ({
  useAxonAcp: () => ({
    submitPrompt: vi.fn(),
    isStreaming: false,
    connected: true,
  }),
}))

vi.mock('@/lib/api-fetch', () => ({
  apiFetch: vi.fn(),
}))

vi.mock('@/lib/shell-store', () => ({
  useShellStore: Object.assign(
    (selector: (state: typeof shellState) => unknown) => selector(shellState),
    {
      getState: () => shellState,
      setState: (partial: Partial<typeof shellState>) => Object.assign(shellState, partial),
    },
  ),
}))

vi.mock('@/components/shell/axon-shell-state-layout', () => ({
  useAxonShellLayoutControls: () => layoutState,
}))

vi.mock('@/components/shell/axon-shell-state-session', () => ({
  useAxonShellSession: () => sessionState,
}))

vi.mock('@/components/shell/axon-shell-state-messages', () => ({
  useAxonShellMessages: () => messageState,
}))

vi.mock('@/components/shell/axon-shell-state-settings', () => ({
  useAxonShellSettings: () => settingsState,
}))

vi.mock('@/components/shell/axon-shell-state-tools', () => ({
  useToolPreferenceState: () => ({
    enabledMcpTools: null,
    handleCommandsUpdate: vi.fn(),
    mcpToolsByServer: { filesystem: ['read_file'] },
    setEnabledMcpTools: vi.fn(),
    setToolPresets: vi.fn(),
    toolPrefsHydrated: true,
    toolPresets: [],
  }),
}))

vi.mock('@/components/shell/axon-shell-state-actions', () => ({
  useAxonShellActions: () => actionState,
}))

import { useAxonShellState } from '@/components/shell/axon-shell-state'

describe('useAxonShellState', () => {
  it('returns grouped layoutState, layoutActions, settings, conversation, composer, sidebar, and editor sections', () => {
    const { result } = renderHook(() => useAxonShellState())

    expect(result.current.layoutState).toMatchObject({
      canvasProfile: 'current',
      chatFlex: 1,
      mobilePane: 'chat',
      chatOpen: true,
      editorOpen: true,
      isDragging: false,
      layoutRestored: true,
      railMode: 'sessions',
      rightPane: 'editor',
      sidebarOpen: true,
      sidebarWidth: 260,
      sectionRef: layoutState.sectionRef,
      transitionClass: 'transition',
      density: 'high',
    })
    expect(result.current.layoutActions).toMatchObject({
      handleCanvasProfileChange: layoutState.handleCanvasProfileChange,
      nudgeChatFlex: layoutState.nudgeChatFlex,
      nudgeSidebar: layoutState.nudgeSidebar,
      persistChatOpen: layoutState.persistChatOpen,
      persistRightPane: layoutState.persistRightPane,
      persistSidebarOpen: layoutState.persistSidebarOpen,
      resetChatFlex: layoutState.resetChatFlex,
      resetSidebarWidth: layoutState.resetSidebarWidth,
      setMobilePaneTracked: layoutState.setMobilePaneTracked,
      setRailModeTracked: layoutState.setRailModeTracked,
      startChatResize: layoutState.startChatResize,
      startSidebarResize: layoutState.startSidebarResize,
      setDensityTracked: layoutState.setDensityTracked,
    })
    expect(result.current.settings).toMatchObject({
      enableFs: true,
      enableTerminal: true,
      permissionTimeoutSecs: 30,
      adapterTimeoutSecs: 60,
    })
    expect(result.current.conversation).toMatchObject({
      agentLabel: 'Claude',
      chatTitle: 'Preview title',
      displayMessages: [{ id: 'm-live', role: 'user', content: 'hello' }],
      liveMessages: [{ id: 'm-live', role: 'user', content: 'hello' }],
      sessionLoading: false,
      sessionError: null,
    })
    expect(result.current.composer).toMatchObject({
      composerProps: actionState.composerProps,
    })
    expect(result.current.sidebar).toMatchObject({
      sidebarProps: actionState.sidebarProps,
      handleSelectSession: actionState.handleSelectSession,
      handleSidebarFileSelect: actionState.handleSidebarFileSelect,
      handleMobileSelectSession: actionState.handleMobileSelectSession,
      handleMobileFileSelect: actionState.handleMobileFileSelect,
      handleMobileNewSession: actionState.handleMobileNewSession,
    })
    expect(result.current.editor).toMatchObject({
      editorMarkdown: '# hello',
    })
    expect(typeof result.current.editor.setEditorMarkdown).toBe('function')
    expect(typeof result.current.editor.onEditorUpdate).toBe('function')
    expect('agentLabel' in result.current).toBe(false)
    expect('canvasProfile' in result.current).toBe(false)
    expect('chatTitle' in result.current).toBe(false)
    expect('chat' in result.current).toBe(false)
    expect('composerProps' in result.current).toBe(false)
    expect('displayMessages' in result.current).toBe(false)
    expect('editorMarkdown' in result.current).toBe(false)
    expect('enableFs' in result.current).toBe(false)
    expect('layout' in result.current).toBe(false)
    expect('layoutState' in result.current).toBe(true)
    expect('layoutActions' in result.current).toBe(true)
    expect('rightPane' in result.current).toBe(false)
    expect('sidebarProps' in result.current).toBe(false)
  })

  it('keeps unrelated grouped sections referentially stable when only editor state changes', () => {
    const { result, rerender } = renderHook(() => useAxonShellState())

    const initialLayoutState = result.current.layoutState
    const initialLayoutActions = result.current.layoutActions
    const initialConversation = result.current.conversation
    const initialComposer = result.current.composer
    const initialSidebar = result.current.sidebar
    const initialSettings = result.current.settings
    const initialEditor = result.current.editor

    act(() => {
      shellState.editorMarkdown = '# updated'
    })
    rerender()

    expect(result.current.layoutState).toBe(initialLayoutState)
    expect(result.current.layoutActions).toBe(initialLayoutActions)
    expect(result.current.conversation).toBe(initialConversation)
    expect(result.current.composer).toBe(initialComposer)
    expect(result.current.sidebar).toBe(initialSidebar)
    expect(result.current.settings).toBe(initialSettings)
    expect(result.current.editor).not.toBe(initialEditor)
    expect(result.current.editor.editorMarkdown).toBe('# updated')
  })
})
