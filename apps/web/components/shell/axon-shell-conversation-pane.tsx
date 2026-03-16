'use client'

import {
  Brain,
  MessageSquareText,
  PanelRight,
  ScrollText,
  Settings2,
  TerminalSquare,
} from 'lucide-react'
import { memo } from 'react'
import { Conversation, ConversationScrollButton } from '@/components/ai-elements/conversation'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { AxonMessageList } from './axon-message-list'
import { AxonPromptComposer } from './axon-prompt-composer'
import type {
  AxonShellComposerState,
  AxonShellConversationState,
  AxonShellEditorState,
  AxonShellLayoutActions,
  AxonShellLayoutState,
} from './axon-shell-state'
import { McpIcon } from './mcp-config'

const DESKTOP_TOOL_BUTTON_CLASS =
  'h-8 w-8 rounded-md border border-transparent text-[var(--text-secondary)] hover:border-[rgba(175,215,255,0.26)] hover:bg-[rgba(175,215,255,0.08)] data-[active=true]:border-[rgba(175,215,255,0.46)] data-[active=true]:bg-[linear-gradient(145deg,rgba(135,175,255,0.28),rgba(135,175,255,0.1))] data-[active=true]:text-[var(--text-primary)]'

type AxonShellConversationPaneProps = {
  composer: AxonShellComposerState
  conversation: AxonShellConversationState
  editor: AxonShellEditorState
  layoutActions: AxonShellLayoutActions
  layoutState: AxonShellLayoutState
  variant: 'desktop' | 'mobile'
}

export const AxonShellConversationPane = memo(function AxonShellConversationPane({
  composer,
  conversation,
  editor,
  layoutActions,
  layoutState,
  variant,
}: AxonShellConversationPaneProps) {
  if (variant === 'mobile') {
    return (
      <div className="axon-glass-shell flex h-full min-h-0 flex-col border-0 rounded-none">
        <Conversation key={conversation.sessionKey} className="w-full flex-1 px-2 py-2">
          <AxonMessageList
            messages={conversation.displayMessages}
            agentName={conversation.agentLabel}
            sessionKey={conversation.sessionKey}
            copiedId={conversation.copiedId}
            copyMessage={conversation.copyMessage}
            onOpenFile={conversation.handleMobileOpenFile}
            isTyping={conversation.isStreaming}
            variant="mobile"
            loading={conversation.sessionLoading}
            error={conversation.sessionError}
            onRetry={conversation.reloadSession}
            onEditorContent={editor.onEditorUpdate}
            onEdit={conversation.handleEditMessage}
            onRetryMessage={conversation.handleRetryMessage}
          />
          <ConversationScrollButton className="animate-scale-in" />
        </Conversation>

        <div className="axon-toolbar border-t border-b-0 px-2 py-2">
          <AxonPromptComposer compact {...composer.composerProps} />
        </div>
      </div>
    )
  }

  return (
    <div
      className={`axon-glass-shell h-full min-h-0 overflow-hidden rounded-none border-0 animate-fade-in ${layoutState.transitionClass}`}
      style={{ flex: `${layoutState.chatFlex} ${layoutState.chatFlex} 0%`, minWidth: 320 }}
    >
      <div className="axon-toolbar flex h-9 items-center justify-between px-3 xl:px-4">
        <div className="min-w-0">
          <div className="truncate text-[12px] font-semibold leading-snug tracking-[-0.01em] text-[var(--text-primary)] xl:text-[13px]">
            {conversation.chatTitle}
          </div>
          <div className="mt-0.5 flex items-center gap-1 font-mono text-[10px] uppercase tracking-[0.09em] text-[var(--text-dim)]">
            <span>{conversation.agentLabel}</span>
            <span className="opacity-40">·</span>
            <span>{conversation.liveMessages.length} msg</span>
            {conversation.connected ? null : (
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
                data-active={layoutState.rightPane === 'cortex'}
                onClick={() =>
                  layoutActions.persistRightPane(
                    layoutState.rightPane === 'cortex' ? null : 'cortex',
                  )
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
                data-active={layoutState.rightPane === 'terminal'}
                onClick={() =>
                  layoutActions.persistRightPane(
                    layoutState.rightPane === 'terminal' ? null : 'terminal',
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
                data-active={layoutState.rightPane === 'logs'}
                onClick={() =>
                  layoutActions.persistRightPane(layoutState.rightPane === 'logs' ? null : 'logs')
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
                data-active={layoutState.rightPane === 'mcp'}
                onClick={() =>
                  layoutActions.persistRightPane(layoutState.rightPane === 'mcp' ? null : 'mcp')
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
                data-active={layoutState.rightPane === 'settings'}
                onClick={() =>
                  layoutActions.persistRightPane(
                    layoutState.rightPane === 'settings' ? null : 'settings',
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
                data-active={layoutState.chatOpen}
                onClick={() => layoutActions.persistChatOpen(!layoutState.chatOpen)}
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
                data-active={layoutState.rightPane === 'editor'}
                onClick={() =>
                  layoutActions.persistRightPane(
                    layoutState.rightPane === 'editor' ? null : 'editor',
                  )
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

      <div className="flex h-[calc(100%-36px)] min-h-0 flex-col">
        <Conversation className="w-full flex-1 px-2 py-1 xl:px-3">
          <AxonMessageList
            messages={conversation.displayMessages}
            agentName={conversation.agentLabel}
            sessionKey={conversation.sessionKey}
            copiedId={conversation.copiedId}
            copyMessage={conversation.copyMessage}
            onOpenFile={conversation.openFile}
            isTyping={conversation.isStreaming}
            variant="desktop"
            loading={conversation.sessionLoading}
            error={conversation.sessionError}
            onRetry={conversation.reloadSession}
            onEditorContent={editor.onEditorUpdate}
            onEdit={conversation.handleEditMessage}
            onRetryMessage={conversation.handleRetryMessage}
          />
          <ConversationScrollButton className="animate-scale-in" />
        </Conversation>

        <div className="axon-toolbar border-t border-b-0 px-2 py-1.5 xl:px-3">
          <AxonPromptComposer compact {...composer.composerProps} />
        </div>
      </div>
    </div>
  )
})
