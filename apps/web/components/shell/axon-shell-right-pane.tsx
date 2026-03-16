'use client'

import dynamic from 'next/dynamic'
import { memo } from 'react'
import { AxonCortexPane } from './axon-cortex-pane'
import { AxonLogsPane } from './axon-logs-pane'
import { AxonMcpPane } from './axon-mcp-pane'
import { AxonSettingsPane } from './axon-settings-pane'
import type {
  AxonShellEditorState,
  AxonShellLayoutActions,
  AxonShellLayoutState,
  AxonShellSettingsState,
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

type AxonShellRightPaneProps = {
  editor: AxonShellEditorState
  layoutActions: AxonShellLayoutActions
  layoutState: AxonShellLayoutState
  settings: AxonShellSettingsState
}

export const AxonShellRightPane = memo(function AxonShellRightPane({
  editor,
  layoutActions,
  layoutState,
  settings,
}: AxonShellRightPaneProps) {
  if (!layoutState.rightPane) {
    return null
  }

  return (
    <aside
      className={`axon-glass-shell h-full min-h-0 overflow-hidden rounded-none border-0 animate-fade-in ${layoutState.transitionClass}`}
      style={{ flex: '1 1 0%', minWidth: 320 }}
    >
      {layoutState.rightPane === 'editor' && (
        <EditorPane
          markdown={editor.editorMarkdown}
          onMarkdownChange={editor.setEditorMarkdown}
          scrollStorageKey="axon.web.shell.editor-scroll"
        />
      )}
      {layoutState.rightPane === 'cortex' && <AxonCortexPane />}
      {layoutState.rightPane === 'terminal' && <AxonTerminalPane />}
      {layoutState.rightPane === 'logs' && <AxonLogsPane />}
      {layoutState.rightPane === 'mcp' && <AxonMcpPane />}
      {layoutState.rightPane === 'settings' && (
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
      )}
    </aside>
  )
})
