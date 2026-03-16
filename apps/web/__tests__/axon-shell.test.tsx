// @vitest-environment jsdom

import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

const { viewportState } = vi.hoisted(() => ({
  viewportState: {
    isMobile: false,
  },
}))

vi.mock('next/dynamic', () => ({
  default: () => () => <div data-testid="editor-pane" />,
}))

vi.mock('@/hooks/use-is-mobile', () => ({
  useIsMobile: () => viewportState.isMobile,
}))

const persistRightPane = vi.fn()
const persistChatOpen = vi.fn()
const persistSidebarOpen = vi.fn()
const setRailModeTracked = vi.fn()

vi.mock('@/components/shell/axon-shell-state', () => ({
  PANE_WIDTH_MIN: 320,
  shouldReloadSessionOnTurnComplete: vi.fn(),
  useAxonShellState: () => ({
    canvasRef: { current: null },
    layoutState: {
      canvasProfile: 'current',
      chatFlex: 1,
      chatOpen: true,
      editorOpen: true,
      isDragging: false,
      layoutRestored: true,
      mobilePane: 'chat',
      railMode: 'sessions',
      rightPane: 'settings',
      sectionRef: { current: null },
      sidebarOpen: true,
      sidebarWidth: 260,
      transitionClass: 'transition',
      density: 'high',
    },
    layoutActions: {
      handleCanvasProfileChange: vi.fn(),
      nudgeChatFlex: vi.fn(),
      nudgeSidebar: vi.fn(),
      persistChatOpen,
      persistRightPane,
      persistSidebarOpen,
      resetChatFlex: vi.fn(),
      resetSidebarWidth: vi.fn(),
      setMobilePaneTracked: vi.fn(),
      setRailModeTracked,
      startChatResize: vi.fn(),
      startSidebarResize: vi.fn(),
      setDensityTracked: vi.fn(),
    },
    settings: {
      enableFs: true,
      setEnableFs: vi.fn(),
      enableTerminal: true,
      setEnableTerminal: vi.fn(),
      permissionTimeoutSecs: 30,
      setPermissionTimeoutSecs: vi.fn(),
      adapterTimeoutSecs: 60,
      setAdapterTimeoutSecs: vi.fn(),
    },
    conversation: {
      agentLabel: 'Claude',
      chatTitle: 'Preview title',
      connected: true,
      copiedId: null,
      copyMessage: vi.fn(),
      displayMessages: [{ id: 'm-1', role: 'assistant', content: 'hello' }],
      handleEditMessage: vi.fn(),
      handleMobileOpenFile: vi.fn(),
      handleRetryMessage: vi.fn(),
      handleStats: vi.fn(),
      isStreaming: false,
      liveMessages: [{ id: 'm-1', role: 'assistant', content: 'hello' }],
      openFile: vi.fn(),
      reloadSession: vi.fn(),
      sessionError: null,
      sessionKey: 1,
      sessionLoading: false,
    },
    composer: {
      composerProps: { onSubmit: vi.fn(), files: [] },
    },
    sidebar: {
      handleMobileFileSelect: vi.fn(),
      handleMobileNewSession: vi.fn(),
      handleMobileSelectSession: vi.fn(),
      handleSelectSession: vi.fn(),
      handleSidebarFileSelect: vi.fn(),
      sidebarProps: {
        sessions: [],
        railMode: 'sessions',
        onRailModeChange: setRailModeTracked,
        railQuery: '',
        onRailQueryChange: vi.fn(),
        activeSessionId: 'session-1',
        activeSessionRepo: 'axon_rust',
        assistantSessions: [],
        activeAssistantSessionId: null,
        onSelectAssistantSession: vi.fn(),
        fileEntries: [],
        fileLoading: false,
        selectedFilePath: null,
        onNewSession: vi.fn(),
      },
    },
    editor: {
      editorMarkdown: '# hello',
      onEditorUpdate: vi.fn(),
      setEditorMarkdown: vi.fn(),
    },
  }),
}))

vi.mock('@/components/shell/axon-frame', () => ({
  AxonFrame: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
}))

vi.mock('@/components/docker-stats', () => ({
  DockerStats: () => <div data-testid="docker-stats" />,
}))

vi.mock('@/components/shell/axon-sidebar', () => ({
  AxonSidebar: ({ variant }: { variant: string }) => <div>sidebar-{variant}</div>,
}))

vi.mock('@/components/shell/axon-message-list', () => ({
  AxonMessageList: ({ messages }: { messages: Array<{ content: string }> }) => (
    <div>messages-{messages.length}</div>
  ),
}))

vi.mock('@/components/shell/axon-prompt-composer', () => ({
  AxonPromptComposer: () => <div>prompt-composer</div>,
}))

vi.mock('@/components/shell/axon-settings-pane', () => ({
  AxonSettingsPane: (props: {
    enableFs: boolean
    enableTerminal: boolean
    permissionTimeoutSecs: number
    adapterTimeoutSecs: number
  }) => (
    <div>
      settings-{String(props.enableFs)}-{String(props.enableTerminal)}-{props.permissionTimeoutSecs}
      -{props.adapterTimeoutSecs}
    </div>
  ),
}))

vi.mock('@/components/shell/axon-shell-resize-divider', () => ({
  AxonShellResizeDivider: () => <div data-testid="resize-divider" />,
}))

vi.mock('@/components/shell/axon-pane-handle', () => ({
  AxonPaneHandle: ({ label }: { label: string }) => <div>{label}</div>,
}))

vi.mock('@/components/shell/axon-mobile-pane-switcher', () => ({
  AxonMobilePaneSwitcher: () => <div>mobile-switcher</div>,
}))

vi.mock('@/components/shell/axon-cortex-pane', () => ({
  AxonCortexPane: () => <div>cortex-pane</div>,
}))

vi.mock('@/components/shell/axon-logs-pane', () => ({
  AxonLogsPane: () => <div>logs-pane</div>,
}))

vi.mock('@/components/shell/axon-mcp-pane', () => ({
  AxonMcpPane: () => <div>mcp-pane</div>,
}))

vi.mock('@/components/shell/axon-terminal-pane', () => ({
  AxonTerminalPane: () => <div>terminal-pane</div>,
}))

vi.mock('@/components/shell/axon-shell-mobile', () => ({
  AxonShellMobile: () => <div>mobile-shell</div>,
}))

vi.mock('@/components/shell/axon-shell-desktop', () => ({
  AxonShellDesktop: () => <div>desktop-shell</div>,
}))

vi.mock('@/components/shell/axon-shell-sidebar-pane', () => ({
  AxonShellSidebarPane: () => <div>sidebar-pane</div>,
}))

vi.mock('@/components/shell/axon-shell-conversation-pane', () => ({
  AxonShellConversationPane: () => <div>conversation-pane</div>,
}))

vi.mock('@/components/shell/axon-shell-right-pane', () => ({
  AxonShellRightPane: () => <div>right-pane</div>,
}))

vi.mock('@/components/ai-elements/conversation', () => ({
  Conversation: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  ConversationScrollButton: () => <div>scroll-button</div>,
}))

vi.mock('@/components/ui/button', () => ({
  Button: ({ children, ...props }: React.ButtonHTMLAttributes<HTMLButtonElement>) => (
    <button {...props}>{children}</button>
  ),
}))

vi.mock('@/components/ui/tooltip', () => ({
  Tooltip: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  TooltipContent: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  TooltipTrigger: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
}))

import { AxonShell } from '@/components/shell/axon-shell'

describe('AxonShell', () => {
  it('renders from extracted desktop and pane subtrees', () => {
    viewportState.isMobile = false

    render(<AxonShell />)

    expect(screen.getByText('desktop-shell')).toBeTruthy()
  })

  it('renders from the extracted mobile subtree when mobile is active', () => {
    viewportState.isMobile = true

    render(<AxonShell />)

    expect(screen.getByText('mobile-shell')).toBeTruthy()
  })
})
