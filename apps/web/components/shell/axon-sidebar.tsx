'use client'

import { ChevronDown, PanelLeft, Plus, Search } from 'lucide-react'
import Image from 'next/image'
import React from 'react'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { ScrollArea } from '@/components/ui/scroll-area'
import type { FileEntry } from '@/components/workspace/file-tree'
import { FileTree } from '@/components/workspace/file-tree'
import type { SessionSummary } from '@/hooks/use-recent-sessions'
import {
  formatLastMessageTime,
  formatSessionSubtitle,
  formatSessionTitle,
} from './axon-sidebar-utils'
import { RAIL_MODES, type RailMode } from './axon-ui-config'

const AGENT_LOGO: Record<string, { src: string; alt: string; title: string }> = {
  claude: { src: '/logos/anthropic.svg', alt: 'Anthropic logo', title: 'Anthropic (Claude)' },
  codex: { src: '/logos/openai.svg', alt: 'OpenAI logo', title: 'OpenAI (Codex)' },
  gemini: { src: '/logos/google.svg', alt: 'Google logo', title: 'Google (Gemini)' },
}
const DEFAULT_AGENT_LOGO = {
  src: '/logos/anthropic.svg',
  alt: 'Anthropic logo',
  title: 'Anthropic (Claude)',
} as const

function AgentLogo({ agent }: { agent?: string }) {
  const normalized = (agent ?? 'claude').toLowerCase()
  const logo = AGENT_LOGO[normalized] ?? DEFAULT_AGENT_LOGO
  return (
    <span
      className="inline-flex h-[18px] w-[18px] shrink-0 items-center justify-center rounded-[5px] bg-[rgba(255,255,255,0.94)] ring-1 ring-[rgba(5,10,20,0.3)] shadow-[0_1px_4px_rgba(0,0,0,0.2)]"
      title={logo.title}
    >
      <Image
        src={logo.src}
        alt={logo.alt}
        width={12}
        height={12}
        className="h-3 w-3 object-contain"
        draggable={false}
        unoptimized
      />
    </span>
  )
}

function railItemClass(isActive: boolean) {
  return isActive
    ? 'border-[rgba(175,215,255,0.56)] bg-[linear-gradient(140deg,rgba(135,175,255,0.18),rgba(135,175,255,0.06))] text-[var(--text-primary)] shadow-[0_0_18px_rgba(135,175,255,0.16)]'
    : 'border-transparent bg-[rgba(6,11,24,0.34)] text-[var(--text-secondary)] hover:border-[rgba(175,215,255,0.24)] hover:bg-[rgba(175,215,255,0.08)] hover:text-[var(--text-primary)]'
}

const RailContent = React.memo(function RailContent({
  mode,
  sessions,
  activeSessionId,
  onSelectSession,
  assistantSessions,
  activeAssistantSessionId,
  onSelectAssistantSession,
  fileEntries,
  fileLoading,
  selectedFilePath,
  onSelectFile,
  query,
  onNewSession,
  compact = false,
}: {
  mode: RailMode
  sessions: SessionSummary[]
  activeSessionId: string | null
  onSelectSession: (sessionId: string) => void
  assistantSessions: SessionSummary[]
  activeAssistantSessionId: string | null
  onSelectAssistantSession: (sessionId: string) => void
  fileEntries: FileEntry[]
  fileLoading: boolean
  selectedFilePath: string | null
  onSelectFile: (entry: FileEntry) => void
  query: string
  onNewSession?: () => void
  compact?: boolean
}) {
  const normalizedQuery = query.trim().toLowerCase()
  const rowClass = compact ? 'py-1.5' : 'py-2.5'

  if (mode === 'sessions') {
    const filteredSessions = sessions.filter((session) => {
      if (!normalizedQuery) return true
      return (
        session.preview?.toLowerCase().includes(normalizedQuery) ||
        session.project?.toLowerCase().includes(normalizedQuery) ||
        session.repo?.toLowerCase().includes(normalizedQuery) ||
        session.branch?.toLowerCase().includes(normalizedQuery)
      )
    })

    return (
      <ul className="mt-1 space-y-0.5">
        {filteredSessions.length === 0 ? (
          <li className="px-3 py-4">
            <div className="rounded-lg border border-dashed border-[rgba(175,215,255,0.26)] bg-[rgba(175,215,255,0.06)] p-3 text-left">
              <p className="text-[12px] font-medium text-[var(--text-primary)]">No sessions yet</p>
              <p className="mt-1 text-[11px] text-[var(--text-dim)]">
                Start one from the + button above.
              </p>
              {onNewSession ? (
                <Button
                  type="button"
                  variant="ghost"
                  onClick={onNewSession}
                  className="mt-2 inline-flex h-auto items-center gap-1.5 rounded-md border border-[rgba(175,215,255,0.32)] bg-[linear-gradient(145deg,rgba(135,175,255,0.24),rgba(135,175,255,0.1))] px-2.5 py-1 text-[11px] font-semibold text-[var(--text-primary)]"
                >
                  <Plus className="size-3" />
                  New session
                </Button>
              ) : null}
            </div>
          </li>
        ) : null}
        {filteredSessions.map((session) => {
          const isActive = session.id === activeSessionId
          const title = formatSessionTitle(session.preview, session.project)
          const meta = formatSessionSubtitle(session.repo, session.project, session.branch)
          return (
            <li key={session.id}>
              <button
                type="button"
                onClick={() => onSelectSession(session.id)}
                aria-current={isActive ? 'true' : undefined}
                className={`axon-sidebar-item w-full rounded-md border-l-2 px-0 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--focus-ring-color)] focus-visible:ring-offset-0 ${rowClass} ${railItemClass(isActive)}`}
                title={title}
              >
                <div className="px-3">
                  <div className="grid grid-cols-[auto_minmax(0,1fr)_auto] items-start gap-2">
                    <div className="pt-0.5">
                      <AgentLogo agent={session.agent} />
                    </div>
                    <div className="min-w-0">
                      <span className="block truncate text-[12px] leading-[1.3] font-medium">
                        {title}
                      </span>
                      {meta ? (
                        <div
                          className="mt-px truncate text-[10px] leading-[1.25] text-[var(--text-muted)]"
                          title={`Workspace context: ${meta}`}
                        >
                          {meta}
                        </div>
                      ) : null}
                    </div>
                    <span className="whitespace-nowrap pt-px text-[10px] tabular-nums text-[rgba(175,215,255,0.68)]">
                      {formatLastMessageTime(session.mtimeMs)}
                    </span>
                  </div>
                </div>
              </button>
            </li>
          )
        })}
      </ul>
    )
  }

  if (mode === 'files') {
    const filteredFileEntries = !normalizedQuery
      ? fileEntries
      : fileEntries.filter((entry) => entry.path.toLowerCase().includes(normalizedQuery))

    if (fileLoading) {
      return <div className="px-3 py-4 text-xs text-[var(--text-dim)]">Loading workspace…</div>
    }

    return (
      <div className="pt-1">
        <FileTree
          entries={filteredFileEntries}
          selectedPath={selectedFilePath}
          onSelect={onSelectFile}
        />
      </div>
    )
  }

  if (mode === 'assistant') {
    const filteredSessions = assistantSessions.filter((session) => {
      if (!normalizedQuery) return true
      return session.preview?.toLowerCase().includes(normalizedQuery) ?? false
    })

    return (
      <ul className="mt-1 space-y-0.5">
        {filteredSessions.length === 0 ? (
          <li className="px-3 py-4">
            <div className="rounded-lg border border-dashed border-[rgba(255,135,175,0.28)] bg-[rgba(255,135,175,0.06)] p-3 text-left">
              <p className="text-[12px] font-medium text-[var(--text-primary)]">
                No assistant chats yet
              </p>
              <p className="mt-1 text-[11px] text-[var(--text-dim)]">
                Kick one off with the + button above.
              </p>
              {onNewSession ? (
                <Button
                  type="button"
                  variant="ghost"
                  onClick={onNewSession}
                  className="mt-2 inline-flex h-auto items-center gap-1.5 rounded-md border border-[rgba(255,135,175,0.34)] bg-[linear-gradient(145deg,rgba(255,135,175,0.24),rgba(255,135,175,0.08))] px-2.5 py-1 text-[11px] font-semibold text-[var(--text-primary)]"
                >
                  <Plus className="size-3" />
                  New assistant chat
                </Button>
              ) : null}
            </div>
          </li>
        ) : null}
        {filteredSessions.map((session) => {
          const isActive = session.id === activeAssistantSessionId
          const title = formatSessionTitle(session.preview, undefined)
          const meta = formatSessionSubtitle(session.repo, session.project, session.branch)
          return (
            <li key={session.id}>
              <button
                type="button"
                onClick={() => onSelectAssistantSession(session.id)}
                aria-current={isActive ? 'true' : undefined}
                className={`axon-sidebar-item w-full rounded-md border-l-2 px-0 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--focus-ring-color)] focus-visible:ring-offset-0 ${rowClass} ${railItemClass(isActive)}`}
                title={session.preview ?? title}
              >
                <div className="px-3">
                  <div className="grid grid-cols-[auto_minmax(0,1fr)_auto] items-start gap-2">
                    <div className="pt-0.5">
                      <AgentLogo agent={session.agent} />
                    </div>
                    <div className="min-w-0">
                      <span className="block truncate text-[12px] leading-[1.3] font-medium">
                        {title}
                      </span>
                      {meta ? (
                        <div
                          className="mt-px truncate text-[10px] leading-[1.25] text-[var(--text-muted)]"
                          title={`Workspace context: ${meta}`}
                        >
                          {meta}
                        </div>
                      ) : null}
                    </div>
                    <span className="whitespace-nowrap pt-px text-[10px] tabular-nums text-[rgba(175,215,255,0.68)]">
                      {formatLastMessageTime(session.mtimeMs)}
                    </span>
                  </div>
                </div>
              </button>
            </li>
          )
        })}
      </ul>
    )
  }

  return null
})

export const AxonSidebar = React.memo(function AxonSidebar({
  variant,
  sessions,
  railMode,
  onRailModeChange,
  railQuery,
  onRailQueryChange,
  activeSessionId,
  activeSessionRepo,
  onSelectSession,
  assistantSessions,
  activeAssistantSessionId,
  onSelectAssistantSession,
  fileEntries,
  fileLoading,
  selectedFilePath,
  onSelectFile,
  onCollapse,
  onNewSession,
}: {
  variant: 'mobile' | 'desktop'
  sessions: SessionSummary[]
  railMode: RailMode
  onRailModeChange: (mode: RailMode) => void
  railQuery: string
  onRailQueryChange: (query: string) => void
  activeSessionId: string | null
  activeSessionRepo: string
  onSelectSession: (sessionId: string) => void
  assistantSessions: SessionSummary[]
  activeAssistantSessionId: string | null
  onSelectAssistantSession: (sessionId: string) => void
  fileEntries: FileEntry[]
  fileLoading: boolean
  selectedFilePath: string | null
  onSelectFile: (entry: FileEntry) => void
  onCollapse?: () => void
  onNewSession?: () => void
}) {
  const activeMode = RAIL_MODES.find((mode) => mode.id === railMode) ?? RAIL_MODES[0]!
  const ActiveModeIcon = activeMode.icon
  const isDesktop = variant === 'desktop'
  const toolbarH = isDesktop ? 'h-8' : 'h-10'
  const searchH = isDesktop ? 'h-7 text-[12px]' : 'h-8 text-[13px]'
  const subtitle =
    railMode === 'sessions'
      ? activeSessionRepo
      : railMode === 'assistant'
        ? 'assistant'
        : 'workspace root'

  return (
    <div className="axon-glass-panel flex h-full min-h-0 flex-col animate-fade-in">
      <div className={`axon-toolbar flex ${toolbarH} items-center justify-between px-2`}>
        <div className="flex items-center gap-1">
          {isDesktop && onCollapse ? (
            <>
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                className="h-6 w-6 border border-transparent text-[var(--text-secondary)] hover:border-[rgba(175,215,255,0.2)] hover:bg-[rgba(175,215,255,0.07)] hover:text-[var(--text-primary)]"
                onClick={onCollapse}
              >
                <PanelLeft className="size-4" />
                <span className="sr-only">Collapse sidebar</span>
              </Button>
              <div className="h-4 w-px bg-[var(--border-subtle)]" />
            </>
          ) : null}
          {isDesktop ? (
            <span className="axon-wordmark axon-sidebar-title select-none pl-1 text-sm font-extrabold tracking-[3px]">
              AXON
            </span>
          ) : null}
          {!isDesktop ? (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  type="button"
                  variant="ghost"
                  className="flex h-auto items-center gap-1.5 pl-1 text-[13px] font-medium text-[var(--text-primary)] transition-colors hover:text-[var(--axon-primary)]"
                >
                  <ActiveModeIcon className="size-3.5 text-[var(--axon-primary)]" />
                  <span>{activeMode.label}</span>
                  <span className="text-[11px] font-normal text-[var(--text-dim)]">{subtitle}</span>
                  <ChevronDown className="size-3 text-[var(--text-dim)]" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent
                align="start"
                className="min-w-[220px] border-[var(--border-subtle)] bg-[var(--glass-overlay)] text-[var(--text-primary)] backdrop-blur-xl"
              >
                {RAIL_MODES.map((mode) => {
                  const Icon = mode.icon
                  return (
                    <DropdownMenuItem
                      key={mode.id}
                      onClick={() => onRailModeChange(mode.id)}
                      className="gap-2 text-sm"
                    >
                      <Icon className="size-4 text-[var(--axon-primary)]" />
                      {mode.label}
                    </DropdownMenuItem>
                  )
                })}
              </DropdownMenuContent>
            </DropdownMenu>
          ) : null}
        </div>
        <div className="flex items-center gap-1">
          {isDesktop ? (
            <Button
              type="button"
              variant="ghost"
              onClick={onNewSession}
              className="inline-flex h-7 items-center gap-1.5 rounded-md border border-[rgba(175,215,255,0.34)] bg-[linear-gradient(145deg,rgba(135,175,255,0.28),rgba(135,175,255,0.1))] px-2.5 text-[11px] font-semibold uppercase tracking-[0.08em] text-[var(--text-primary)] shadow-[0_0_14px_rgba(135,175,255,0.2)] transition-colors hover:border-[rgba(175,215,255,0.48)] hover:bg-[linear-gradient(145deg,rgba(135,175,255,0.36),rgba(135,175,255,0.14))]"
            >
              <Plus className="size-3.5" />
              New
            </Button>
          ) : (
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              onClick={onNewSession}
              className="inline-flex size-8 items-center justify-center rounded-md border border-[rgba(175,215,255,0.34)] bg-[linear-gradient(145deg,rgba(135,175,255,0.28),rgba(135,175,255,0.1))] text-[var(--text-primary)] shadow-[0_0_14px_rgba(135,175,255,0.2)]"
            >
              <Plus className="size-3.5" />
              <span className="sr-only">New session</span>
            </Button>
          )}
        </div>
      </div>

      {isDesktop ? (
        <div className="axon-toolbar flex items-center justify-between px-2 py-1">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                type="button"
                variant="ghost"
                className="flex h-auto items-center gap-1.5 pl-1 text-[13px] font-medium text-[var(--text-primary)] transition-colors hover:text-[var(--axon-primary)]"
              >
                <ActiveModeIcon className="size-3.5 text-[var(--axon-primary)]" />
                <span>{activeMode.label}</span>
                <span className="text-[11px] font-normal text-[var(--text-dim)]">{subtitle}</span>
                <ChevronDown className="size-3 text-[var(--text-dim)]" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent
              align="start"
              className="min-w-[220px] border-[var(--border-subtle)] bg-[var(--glass-overlay)] text-[var(--text-primary)] backdrop-blur-xl"
            >
              {RAIL_MODES.map((mode) => {
                const Icon = mode.icon
                return (
                  <DropdownMenuItem
                    key={mode.id}
                    onClick={() => onRailModeChange(mode.id)}
                    className="gap-2 text-sm"
                  >
                    <Icon className="size-4 text-[var(--axon-primary)]" />
                    {mode.label}
                  </DropdownMenuItem>
                )
              })}
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      ) : null}

      <div className="axon-sidebar-search-container border-b border-[var(--border-subtle)] px-2 py-1.5">
        <div className="relative">
          <Search className="axon-sidebar-search-icon pointer-events-none absolute left-2 top-1/2 size-3.5 -translate-y-1/2 text-[var(--text-dim)]" />
          <input
            value={railQuery}
            onChange={(event) => onRailQueryChange(event.target.value)}
            placeholder={
              railMode === 'sessions'
                ? 'Search sessions…'
                : railMode === 'assistant'
                  ? 'Search assistant chats…'
                  : 'Search files…'
            }
            aria-label={`Search ${activeMode.label.toLowerCase()}`}
            className={`axon-sidebar-search-input axon-input ${searchH} w-full rounded-md pl-7 pr-2 font-sans`}
          />
        </div>
      </div>

      <ScrollArea className="min-h-0 flex-1 bg-[rgba(6,11,24,0.2)] px-2 py-1">
        <RailContent
          mode={railMode}
          sessions={sessions}
          activeSessionId={activeSessionId}
          onSelectSession={onSelectSession}
          assistantSessions={assistantSessions}
          activeAssistantSessionId={activeAssistantSessionId}
          onSelectAssistantSession={onSelectAssistantSession}
          fileEntries={fileEntries}
          fileLoading={fileLoading}
          selectedFilePath={selectedFilePath}
          onSelectFile={onSelectFile}
          query={railQuery}
          onNewSession={onNewSession}
          compact={!isDesktop}
        />
      </ScrollArea>
    </div>
  )
})
