'use client'

import {
  BracesIcon,
  ChevronDownIcon,
  FileCode2Icon,
  SearchIcon,
  SparklesIcon,
  TerminalSquareIcon,
} from 'lucide-react'
import type { ComponentProps } from 'react'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible'
import { cn } from '@/lib/utils'

export type ToolKind = 'terminal' | 'mcp' | 'file' | 'skill' | 'search' | 'tool'

const TOOL_KIND_LABEL: Record<ToolKind, string> = {
  terminal: 'TERMINAL',
  mcp: 'MCP',
  file: 'FILE',
  skill: 'SKILL',
  search: 'SEARCH',
  tool: 'TOOL',
}

function matchesPattern(value: string, patterns: RegExp[]): boolean {
  return patterns.some((pattern) => pattern.test(value))
}

export function inferToolKind(toolName: string): ToolKind {
  const normalized = toolName.trim().toLowerCase()
  if (!normalized) return 'tool'

  if (normalized.startsWith('mcp__') || /(^|[._:-])mcp([._:-]|$)/.test(normalized)) return 'mcp'
  if (matchesPattern(normalized, [/(^|[._:-])(exec_command|write_stdin)([._:-]|$)/])) {
    return 'terminal'
  }
  if (matchesPattern(normalized, [/(^|[._:-])(read|write|edit)_file([._:-]|$)/, /apply_patch/])) {
    return 'file'
  }
  if (normalized.includes(':')) return 'skill'
  if (matchesPattern(normalized, [/(^|[._:-])(search|query|crawl|scrape)([._:-]|$)/])) {
    return 'search'
  }
  return 'tool'
}

export function toolKindLabel(kind: ToolKind): string {
  return TOOL_KIND_LABEL[kind]
}

function ToolKindIcon({ kind }: { kind: ToolKind }) {
  if (kind === 'mcp') return <BracesIcon className="size-4 shrink-0 text-[var(--axon-primary)]" />
  if (kind === 'file')
    return <FileCode2Icon className="size-4 shrink-0 text-[var(--axon-primary)]" />
  if (kind === 'skill')
    return <SparklesIcon className="size-4 shrink-0 text-[var(--axon-primary)]" />
  if (kind === 'search')
    return <SearchIcon className="size-4 shrink-0 text-[var(--axon-primary)]" />
  return <TerminalSquareIcon className="size-4 shrink-0 text-[var(--axon-primary)]" />
}

export function Tool({
  className,
  defaultOpen = true,
  ...props
}: ComponentProps<typeof Collapsible>) {
  return (
    <Collapsible
      className={cn(
        'overflow-hidden rounded-xl border border-[var(--border-subtle)] bg-[rgba(7,12,26,0.8)] shadow-[var(--shadow-md)]',
        className,
      )}
      defaultOpen={defaultOpen}
      {...props}
    />
  )
}

export function ToolHeader({
  className,
  title,
  description,
  kind,
  status,
  meta,
  badges,
}: {
  className?: string
  title: string
  description?: string
  kind?: ToolKind
  status?: string
  meta?: string
  badges?: string[]
}) {
  const displayKind = kind ?? inferToolKind(title)
  const normalizedStatus = (status ?? '').toLowerCase()
  const statusTone =
    normalizedStatus === 'completed' || normalizedStatus === 'success'
      ? 'bg-[rgba(64,196,128,0.12)] text-[rgba(128,220,160,0.92)]'
      : normalizedStatus === 'failed' || normalizedStatus === 'error'
        ? 'bg-[rgba(255,135,175,0.12)] text-[rgba(255,170,196,0.86)]'
        : 'bg-[rgba(175,215,255,0.08)] text-[var(--text-dim)]'

  return (
    <CollapsibleTrigger
      className={cn(
        'group flex w-full items-center justify-between gap-2 border-b border-[var(--border-subtle)] text-left px-3 py-2',
        className,
      )}
    >
      <div className="flex min-w-0 items-center gap-2">
        <ToolKindIcon kind={displayKind} />
        <div className="min-w-0">
          <div className="flex min-w-0 items-center gap-1.5">
            <p className={cn('truncate font-medium text-[var(--text-primary)] text-[13px]')}>
              {title}
            </p>
            <span className="shrink-0 rounded border border-[var(--border-subtle)] px-1.5 py-0.5 font-mono text-[10px] text-[var(--text-dim)]">
              {toolKindLabel(displayKind)}
            </span>
            {status ? (
              <span
                className={`shrink-0 rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.12em] ${statusTone}`}
              >
                {status}
              </span>
            ) : null}
          </div>
          {description ? (
            <p className="text-xs text-[var(--text-secondary)]">{description}</p>
          ) : null}
          {badges?.length ? (
            <div className="mt-1 flex flex-wrap items-center gap-1">
              {badges.map((badge) => (
                <span
                  key={badge}
                  className="rounded border border-[var(--border-subtle)] bg-[rgba(255,255,255,0.02)] px-1.5 py-0.5 text-[10px] leading-none text-[var(--text-dim)]"
                >
                  {badge}
                </span>
              ))}
            </div>
          ) : null}
          {meta ? <p className="text-[11px] text-[var(--text-dim)]">{meta}</p> : null}
        </div>
      </div>
      <ChevronDownIcon
        className={cn(
          'size-4 shrink-0 text-[var(--text-dim)] transition-transform',
          'group-data-[state=closed]:-rotate-90',
        )}
      />
    </CollapsibleTrigger>
  )
}

export function ToolContent({ className, ...props }: ComponentProps<typeof CollapsibleContent>) {
  return <CollapsibleContent className={cn('min-h-0', className)} {...props} />
}
