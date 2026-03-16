'use client'

import { PanelLeft } from 'lucide-react'
import { memo } from 'react'
import { Button } from '@/components/ui/button'
import type {
  AxonShellLayoutActions,
  AxonShellLayoutState,
  AxonShellSidebarState,
} from './axon-shell-state'
import { AxonSidebar } from './axon-sidebar'
import { RAIL_MODES } from './axon-ui-config'

type AxonShellSidebarPaneProps = {
  layoutActions: AxonShellLayoutActions
  layoutState: AxonShellLayoutState
  sidebar: AxonShellSidebarState
  variant: 'desktop' | 'mobile'
}

export const AxonShellSidebarPane = memo(function AxonShellSidebarPane({
  layoutActions,
  layoutState,
  sidebar,
  variant,
}: AxonShellSidebarPaneProps) {
  if (variant === 'mobile') {
    return (
      <AxonSidebar
        variant="mobile"
        {...sidebar.sidebarProps}
        onSelectSession={sidebar.handleMobileSelectSession}
        onSelectFile={sidebar.handleMobileFileSelect}
        onNewSession={sidebar.handleMobileNewSession}
      />
    )
  }

  if (layoutState.sidebarOpen) {
    return (
      <aside
        className={`h-full min-h-0 shrink-0 overflow-hidden ${layoutState.transitionClass}`}
        style={{ width: layoutState.sidebarWidth }}
      >
        <AxonSidebar
          variant="desktop"
          {...sidebar.sidebarProps}
          onSelectSession={sidebar.handleSelectSession}
          onSelectFile={sidebar.handleSidebarFileSelect}
          onCollapse={() => layoutActions.persistSidebarOpen(false)}
        />
      </aside>
    )
  }

  return (
    <div className="flex h-full w-11 shrink-0 flex-col items-center border-r border-[var(--border-subtle)] bg-[linear-gradient(180deg,rgba(9,17,35,0.82),rgba(6,12,26,0.9))] pt-2">
      <Button
        type="button"
        variant="ghost"
        size="icon-sm"
        onClick={() => layoutActions.persistSidebarOpen(true)}
        aria-label="Expand sidebar"
        className="axon-icon-btn flex size-8 items-center justify-center"
      >
        <PanelLeft className="size-4" />
      </Button>
      <div className="my-1.5 w-5 border-t border-[var(--border-subtle)]" />
      {RAIL_MODES.map((mode) => {
        const Icon = mode.icon
        const isActive = layoutState.railMode === mode.id
        return (
          <Button
            key={mode.id}
            type="button"
            variant="ghost"
            size="icon-sm"
            onClick={() => {
              layoutActions.setRailModeTracked(mode.id)
              layoutActions.persistSidebarOpen(true)
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
  )
})
