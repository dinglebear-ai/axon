'use client'

import { PanelLeft } from 'lucide-react'
import dynamic from 'next/dynamic'
import { memo } from 'react'
import { Button } from '@/components/ui/button'
import { AxonCortexPane } from './axon-cortex-pane'
import { AxonLogsPane } from './axon-logs-pane'
import { AxonMcpPane } from './axon-mcp-pane'
import { AxonMobilePaneSwitcher } from './axon-mobile-pane-switcher'
import { AxonSettingsPane } from './axon-settings-pane'
import { AxonShellConversationPane } from './axon-shell-conversation-pane'
import { AxonShellSidebarPane } from './axon-shell-sidebar-pane'
import type {
  AxonShellComposerState,
  AxonShellConversationState,
  AxonShellEditorState,
  AxonShellLayoutActions,
  AxonShellLayoutState,
  AxonShellSettingsState,
  AxonShellSidebarState,
} from './axon-shell-state'
import { AxonTerminalPane } from './axon-terminal-pane'

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

type AxonShellMobileProps = {
  composer: AxonShellComposerState
  conversation: AxonShellConversationState
  editor: AxonShellEditorState
  layoutActions: AxonShellLayoutActions
  layoutState: AxonShellLayoutState
  settings: AxonShellSettingsState
  sidebar: AxonShellSidebarState
}

export const AxonShellMobile = memo(function AxonShellMobile({
  composer,
  conversation,
  editor,
  layoutActions,
  layoutState,
  settings,
  sidebar,
}: AxonShellMobileProps) {
  return (
    <section className="flex min-h-0 flex-1 flex-col">
      <div className="axon-toolbar flex h-14 items-center justify-between bg-[rgba(7,12,26,0.62)] px-3">
        <span className="axon-wordmark select-none text-sm font-extrabold tracking-[3px]">
          AXON
        </span>
        <div className="flex items-center gap-1.5">
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            onClick={() => layoutActions.setMobilePaneTracked('sidebar')}
            aria-label="Sidebar pane"
            aria-pressed={layoutState.mobilePane === 'sidebar'}
            className={`inline-flex size-7 items-center justify-center rounded border transition-colors ${
              layoutState.mobilePane === 'sidebar'
                ? 'border-[rgba(175,215,255,0.48)] bg-[linear-gradient(145deg,rgba(135,175,255,0.34),rgba(135,175,255,0.14))] text-[var(--text-primary)] shadow-[0_0_14px_rgba(135,175,255,0.2)]'
                : 'border-[var(--border-subtle)] bg-[var(--surface-input)] text-[var(--text-dim)] hover:border-[rgba(175,215,255,0.24)] hover:text-[var(--text-primary)]'
            }`}
          >
            <PanelLeft className="size-3.5" />
          </Button>
          <AxonMobilePaneSwitcher
            mobilePane={layoutState.mobilePane === 'sidebar' ? 'chat' : layoutState.mobilePane}
            onMobilePaneChange={(pane) => layoutActions.setMobilePaneTracked(pane)}
          />
        </div>
      </div>

      <div className="flex min-h-0 flex-1 flex-col">
        {layoutState.mobilePane === 'sidebar' ? (
          <AxonShellSidebarPane
            layoutActions={layoutActions}
            layoutState={layoutState}
            sidebar={sidebar}
            variant="mobile"
          />
        ) : layoutState.mobilePane === 'chat' ? (
          <AxonShellConversationPane
            composer={composer}
            conversation={conversation}
            editor={editor}
            layoutActions={layoutActions}
            layoutState={layoutState}
            variant="mobile"
          />
        ) : layoutState.mobilePane === 'editor' ? (
          <div className="axon-glass-shell flex h-full min-h-0 flex-col border-0 rounded-none">
            <div className="min-h-0 flex-1 overflow-hidden">
              <EditorPane
                markdown={editor.editorMarkdown}
                onMarkdownChange={editor.setEditorMarkdown}
                scrollStorageKey="axon.web.shell.editor-scroll"
              />
            </div>
          </div>
        ) : layoutState.mobilePane === 'terminal' ? (
          <div className="axon-glass-shell flex h-full min-h-0 flex-col border-0 rounded-none">
            <AxonTerminalPane />
          </div>
        ) : layoutState.mobilePane === 'logs' ? (
          <div className="axon-glass-shell flex h-full min-h-0 flex-col border-0 rounded-none">
            <AxonLogsPane />
          </div>
        ) : layoutState.mobilePane === 'mcp' ? (
          <div className="axon-glass-shell flex h-full min-h-0 flex-col border-0 rounded-none">
            <AxonMcpPane />
          </div>
        ) : layoutState.mobilePane === 'settings' ? (
          <div className="axon-glass-shell flex h-full min-h-0 flex-col border-0 rounded-none">
            <AxonSettingsPane
              canvasProfile={layoutState.canvasProfile}
              onCanvasProfileChange={layoutActions.handleCanvasProfileChange}
              enableFs={settings.enableFs}
              onEnableFsChange={settings.setEnableFs}
              enableTerminal={settings.enableTerminal}
              onEnableTerminalChange={settings.setEnableTerminal}
              permissionTimeoutSecs={settings.permissionTimeoutSecs}
              onPermissionTimeoutSecsChange={settings.setPermissionTimeoutSecs}
              adapterTimeoutSecs={settings.adapterTimeoutSecs}
              onAdapterTimeoutSecsChange={settings.setAdapterTimeoutSecs}
            />
          </div>
        ) : layoutState.mobilePane === 'cortex' ? (
          <div className="axon-glass-shell flex h-full min-h-0 flex-col border-0 rounded-none">
            <AxonCortexPane />
          </div>
        ) : null}
      </div>
    </section>
  )
})
