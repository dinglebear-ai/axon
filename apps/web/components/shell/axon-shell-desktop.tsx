'use client'

import { memo } from 'react'
import { AxonPaneHandle } from './axon-pane-handle'
import { AxonShellConversationPane } from './axon-shell-conversation-pane'
import { AxonShellResizeDivider } from './axon-shell-resize-divider'
import { AxonShellRightPane } from './axon-shell-right-pane'
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

type AxonShellDesktopProps = {
  composer: AxonShellComposerState
  conversation: AxonShellConversationState
  editor: AxonShellEditorState
  layoutActions: AxonShellLayoutActions
  layoutState: AxonShellLayoutState
  settings: AxonShellSettingsState
  sidebar: AxonShellSidebarState
}

export const AxonShellDesktop = memo(function AxonShellDesktop({
  composer,
  conversation,
  editor,
  layoutActions,
  layoutState,
  settings,
  sidebar,
}: AxonShellDesktopProps) {
  return (
    <section ref={layoutState.sectionRef} className="flex min-h-0 flex-1">
      <AxonShellSidebarPane
        layoutActions={layoutActions}
        layoutState={layoutState}
        sidebar={sidebar}
        variant="desktop"
      />

      {layoutState.sidebarOpen && layoutState.chatOpen ? (
        <AxonShellResizeDivider
          onDragStart={layoutActions.startSidebarResize}
          onReset={layoutActions.resetSidebarWidth}
          onNudge={layoutActions.nudgeSidebar}
        />
      ) : layoutState.sidebarOpen && !layoutState.chatOpen ? (
        <div className="w-px shrink-0 bg-[var(--border-subtle)]" />
      ) : null}

      {layoutState.chatOpen ? (
        <AxonShellConversationPane
          composer={composer}
          conversation={conversation}
          editor={editor}
          layoutActions={layoutActions}
          layoutState={layoutState}
          variant="desktop"
        />
      ) : (
        <AxonPaneHandle
          label="Chat"
          side="left"
          onClick={() => layoutActions.persistChatOpen(true)}
        />
      )}

      {layoutState.chatOpen && layoutState.editorOpen ? (
        <AxonShellResizeDivider
          onDragStart={layoutActions.startChatResize}
          onReset={layoutActions.resetChatFlex}
          onNudge={layoutActions.nudgeChatFlex}
        />
      ) : null}

      {layoutState.rightPane ? (
        <AxonShellRightPane
          editor={editor}
          layoutActions={layoutActions}
          layoutState={layoutState}
          settings={settings}
        />
      ) : (
        <AxonPaneHandle
          label="Editor"
          side="right"
          onClick={() => layoutActions.persistRightPane('editor')}
        />
      )}
    </section>
  )
})
