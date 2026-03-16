'use client'

import {
  Brain,
  MessageSquareText,
  PanelLeft,
  PanelRight,
  ScrollText,
  Settings2,
  TerminalSquare,
} from 'lucide-react'
import dynamic from 'next/dynamic'
import { memo } from 'react'
import { Conversation, ConversationScrollButton } from '@/components/ai-elements/conversation'
import { DockerStats } from '@/components/docker-stats'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { useIsMobile } from '@/hooks/use-is-mobile'
import { AxonCortexPane } from './axon-cortex-pane'
import { AxonFrame } from './axon-frame'
import { AxonLogsPane } from './axon-logs-pane'
import { AxonMcpPane } from './axon-mcp-pane'
import { AxonMessageList } from './axon-message-list'
import { AxonPaneHandle } from './axon-pane-handle'
import { AxonPromptComposer } from './axon-prompt-composer'
import { AxonSettingsPane } from './axon-settings-pane'
import { AxonShellMobile } from './axon-shell-mobile'
import { AxonShellResizeDivider } from './axon-shell-resize-divider'
import {
  PANE_WIDTH_MIN,
  shouldReloadSessionOnTurnComplete,
  useAxonShellState,
} from './axon-shell-state'
import { AxonSidebar } from './axon-sidebar'
import { AxonTerminalPane } from './axon-terminal-pane'
import { RAIL_MODES } from './axon-ui-config'
import { McpIcon } from './mcp-config'

const EditorPane = dynamic(
  () => import('@/components/editor/editor-pane').then((m) => ({ default: m.PulseEditorPane })),
  {
    ssr: false,
    loading: () => (
      <div className="flex h-full w-full flex-col">
        <div className="h-12 w-full border-b border-[rgba(175,215,255,0.08)] bg-[linear-gradient(180deg,rgba(10,18,35,0.64),rgba(4,9,20,0.68))]" />
        <div className="flex-1 bg-transparent" />
      </div>
    ),
  },
)

const DESKTOP_TOOL_BUTTON_CLASS =
  'h-8 w-8 rounded-md border border-transparent text-[var(--text-secondary)] hover:border-[rgba(175,215,255,0.26)] hover:bg-[rgba(175,215,255,0.08)] data-[active=true]:border-[rgba(175,215,255,0.46)] data-[active=true]:bg-[linear-gradient(145deg,rgba(135,175,255,0.28),rgba(135,175,255,0.1))] data-[active=true]:text-[var(--text-primary)]'

export { shouldReloadSessionOnTurnComplete }

export const AxonShell = memo(function AxonShell() {
  const shellState = useAxonShellState()
  const {
    canvasRef,
    layoutState,
    layoutActions,
    settings,
    conversation,
    composer,
    sidebar,
    editor,
  } = shellState
  const shell = {
    canvasRef,
    ...layoutState,
    ...layoutActions,
    ...settings,
    ...conversation,
    ...composer,
    ...sidebar,
    ...editor,
  }
  // Mount only the active layout tree — avoids reconciling both mobile and
  // desktop subtrees simultaneously. Initialises to false (SSR-safe) and
  // flips after the first paint, so there is no flash of wrong layout.
  const isMobile = useIsMobile()

  return (
    <AxonFrame canvasRef={shell.canvasRef} canvasProfile={shell.canvasProfile}>
      {/* DockerStats is always CSS-hidden here; visible=false prevents 500ms re-renders */}
      <div className="hidden">
        <DockerStats onStats={shell.handleStats} visible={false} />
      </div>
      <div className="flex h-dvh min-h-dvh flex-col">
        {isMobile ? (
          <AxonShellMobile
            composer={shellState.composer}
            conversation={shellState.conversation}
            editor={shellState.editor}
            layoutActions={shellState.layoutActions}
            layoutState={shellState.layoutState}
            settings={shellState.settings}
            sidebar={shellState.sidebar}
          />
        ) : (
          <section ref={shell.sectionRef} className="flex min-h-0 flex-1">
            {shell.sidebarOpen ? (
              <aside
                className={`h-full min-h-0 shrink-0 overflow-hidden ${shell.transitionClass}`}
                style={{ width: shell.sidebarWidth }}
              >
                <AxonSidebar
                  variant="desktop"
                  {...shell.sidebarProps}
                  onSelectSession={shell.handleSelectSession}
                  onSelectFile={shell.handleSidebarFileSelect}
                  onCollapse={() => shell.persistSidebarOpen(false)}
                />
              </aside>
            ) : (
              <div className="flex h-full w-11 shrink-0 flex-col items-center border-r border-[var(--border-subtle)] bg-[linear-gradient(180deg,rgba(9,17,35,0.82),rgba(6,12,26,0.9))] pt-2">
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-sm"
                  onClick={() => shell.persistSidebarOpen(true)}
                  aria-label="Expand sidebar"
                  className="axon-icon-btn flex size-8 items-center justify-center"
                >
                  <PanelLeft className="size-4" />
                </Button>
                <div className="my-1.5 w-5 border-t border-[var(--border-subtle)]" />
                {RAIL_MODES.map((mode) => {
                  const Icon = mode.icon
                  const isActive = shell.railMode === mode.id
                  return (
                    <Button
                      key={mode.id}
                      type="button"
                      variant="ghost"
                      size="icon-sm"
                      onClick={() => {
                        shell.setRailModeTracked(mode.id)
                        shell.persistSidebarOpen(true)
                      }}
                      aria-label={mode.label}
                      title={mode.label}
                      className={`flex size-8 items-center justify-center rounded transition-colors ${
                        isActive
                          ? 'border border-[rgba(175,215,255,0.42)] bg-[linear-gradient(145deg,rgba(135,175,255,0.26),rgba(135,175,255,0.08))] text-[var(--text-primary)]'
                          : 'text-[var(--text-dim)] hover:bg-[rgba(175,215,255,0.06)] hover:text-[var(--text-primary)]'
                      }`}
                    >
                      <Icon className="size-4" />
                    </Button>
                  )
                })}
              </div>
            )}

            {shell.sidebarOpen && shell.chatOpen ? (
              <AxonShellResizeDivider
                onDragStart={shell.startSidebarResize}
                onReset={shell.resetSidebarWidth}
                onNudge={shell.nudgeSidebar}
              />
            ) : shell.sidebarOpen && !shell.chatOpen ? (
              <div className="w-px shrink-0 bg-[var(--border-subtle)]" />
            ) : null}

            {shell.chatOpen ? (
              <div
                className={`axon-glass-shell h-full min-h-0 overflow-hidden rounded-none border-0 animate-fade-in ${shell.transitionClass}`}
                style={{ flex: `${shell.chatFlex} ${shell.chatFlex} 0%`, minWidth: PANE_WIDTH_MIN }}
              >
                <div className="axon-toolbar flex h-12 items-center justify-between px-3 xl:px-4">
                  <div className="min-w-0">
                    <div className="truncate text-[14px] font-semibold leading-snug tracking-[-0.01em] text-[var(--text-primary)] xl:text-[15px]">
                      {shell.chatTitle}
                    </div>
                    <div className="mt-0.5 flex items-center gap-1 font-mono text-[10px] uppercase tracking-[0.09em] text-[var(--text-dim)]">
                      <span>{shell.agentLabel}</span>
                      <span className="opacity-40">·</span>
                      <span>{shell.liveMessages.length} msg</span>
                      {shell.connected ? null : (
                        <>
                          <span className="opacity-40">·</span>
                          <span className="text-[var(--axon-secondary)]">disconnected</span>
                        </>
                      )}
                    </div>
                  </div>
                  <div className="flex items-center gap-1">
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon-sm"
                          className={DESKTOP_TOOL_BUTTON_CLASS}
                          data-active={shell.rightPane === 'cortex'}
                          onClick={() =>
                            shell.persistRightPane(shell.rightPane === 'cortex' ? null : 'cortex')
                          }
                        >
                          <Brain className="size-4" />
                          <span className="sr-only">Cortex</span>
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent side="bottom">Cortex</TooltipContent>
                    </Tooltip>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon-sm"
                          className={DESKTOP_TOOL_BUTTON_CLASS}
                          data-active={shell.rightPane === 'terminal'}
                          onClick={() =>
                            shell.persistRightPane(
                              shell.rightPane === 'terminal' ? null : 'terminal',
                            )
                          }
                        >
                          <TerminalSquare className="size-4" />
                          <span className="sr-only">Terminal</span>
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent side="bottom">Terminal</TooltipContent>
                    </Tooltip>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon-sm"
                          className={DESKTOP_TOOL_BUTTON_CLASS}
                          data-active={shell.rightPane === 'logs'}
                          onClick={() =>
                            shell.persistRightPane(shell.rightPane === 'logs' ? null : 'logs')
                          }
                        >
                          <ScrollText className="size-4" />
                          <span className="sr-only">Logs</span>
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent side="bottom">Logs</TooltipContent>
                    </Tooltip>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon-sm"
                          className={DESKTOP_TOOL_BUTTON_CLASS}
                          data-active={shell.rightPane === 'mcp'}
                          onClick={() =>
                            shell.persistRightPane(shell.rightPane === 'mcp' ? null : 'mcp')
                          }
                        >
                          <McpIcon className="size-4" />
                          <span className="sr-only">MCP Servers</span>
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent side="bottom">MCP Servers</TooltipContent>
                    </Tooltip>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon-sm"
                          className={DESKTOP_TOOL_BUTTON_CLASS}
                          data-active={shell.rightPane === 'settings'}
                          onClick={() =>
                            shell.persistRightPane(
                              shell.rightPane === 'settings' ? null : 'settings',
                            )
                          }
                        >
                          <Settings2 className="size-4" />
                          <span className="sr-only">Settings</span>
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent side="bottom">Settings</TooltipContent>
                    </Tooltip>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon-sm"
                          className={DESKTOP_TOOL_BUTTON_CLASS}
                          data-active={shell.chatOpen}
                          onClick={() => shell.persistChatOpen(!shell.chatOpen)}
                        >
                          <MessageSquareText className="size-4" />
                          <span className="sr-only">Chat</span>
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent side="bottom">Chat</TooltipContent>
                    </Tooltip>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon-sm"
                          className={DESKTOP_TOOL_BUTTON_CLASS}
                          data-active={shell.rightPane === 'editor'}
                          onClick={() =>
                            shell.persistRightPane(shell.rightPane === 'editor' ? null : 'editor')
                          }
                        >
                          <PanelRight className="size-4" />
                          <span className="sr-only">Toggle Panel</span>
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent side="bottom">Toggle Panel</TooltipContent>
                    </Tooltip>
                  </div>
                </div>

                <div className="flex h-[calc(100%-48px)] min-h-0 flex-col">
                  <Conversation className="w-full flex-1 px-3 py-2.5 xl:px-4">
                    <AxonMessageList
                      messages={shell.displayMessages}
                      agentName={shell.agentLabel}
                      sessionKey={shell.sessionKey}
                      copiedId={shell.copiedId}
                      copyMessage={shell.copyMessage}
                      onOpenFile={shell.openFile}
                      isTyping={shell.isStreaming}
                      variant="desktop"
                      loading={shell.sessionLoading}
                      error={shell.sessionError}
                      onRetry={shell.reloadSession}
                      onEditorContent={shell.onEditorUpdate}
                      onEdit={shell.handleEditMessage}
                      onRetryMessage={shell.handleRetryMessage}
                    />
                    <ConversationScrollButton className="animate-scale-in" />
                  </Conversation>

                  <div className="axon-toolbar border-t border-b-0 px-3 py-2 xl:px-4">
                    <AxonPromptComposer compact {...shell.composerProps} />
                  </div>
                </div>
              </div>
            ) : (
              <AxonPaneHandle
                label="Chat"
                side="left"
                onClick={() => shell.persistChatOpen(true)}
              />
            )}

            {shell.chatOpen && shell.editorOpen ? (
              <AxonShellResizeDivider
                onDragStart={shell.startChatResize}
                onReset={shell.resetChatFlex}
                onNudge={shell.nudgeChatFlex}
              />
            ) : null}

            {shell.rightPane ? (
              <aside
                className={`axon-glass-shell h-full min-h-0 overflow-hidden rounded-none border-0 animate-fade-in ${shell.transitionClass}`}
                style={{ flex: '1 1 0%', minWidth: PANE_WIDTH_MIN }}
              >
                {shell.rightPane === 'editor' && (
                  <EditorPane
                    markdown={shell.editorMarkdown}
                    onMarkdownChange={shell.setEditorMarkdown}
                    scrollStorageKey="axon.web.shell.editor-scroll"
                  />
                )}
                {shell.rightPane === 'cortex' && <AxonCortexPane />}
                {shell.rightPane === 'terminal' && <AxonTerminalPane />}
                {shell.rightPane === 'logs' && <AxonLogsPane />}
                {shell.rightPane === 'mcp' && <AxonMcpPane />}
                {shell.rightPane === 'settings' && (
                  <AxonSettingsPane
                    canvasProfile={shell.canvasProfile}
                    onCanvasProfileChange={shell.handleCanvasProfileChange}
                    enableFs={shell.enableFs}
                    onEnableFsChange={shell.setEnableFs}
                    enableTerminal={shell.enableTerminal}
                    onEnableTerminalChange={shell.setEnableTerminal}
                    permissionTimeoutSecs={shell.permissionTimeoutSecs}
                    onPermissionTimeoutSecsChange={shell.setPermissionTimeoutSecs}
                    adapterTimeoutSecs={shell.adapterTimeoutSecs}
                    onAdapterTimeoutSecsChange={shell.setAdapterTimeoutSecs}
                  />
                )}
              </aside>
            ) : (
              <AxonPaneHandle
                label="Editor"
                side="right"
                onClick={() => shell.persistRightPane('editor')}
              />
            )}
          </section>
        )}
      </div>
    </AxonFrame>
  )
})

AxonShell.displayName = 'AxonShell'
