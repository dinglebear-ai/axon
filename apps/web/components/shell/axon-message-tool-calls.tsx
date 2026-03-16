'use client'

import { FileCode2 } from 'lucide-react'
import { useState } from 'react'
import {
  ChainOfThought,
  ChainOfThoughtContent,
  ChainOfThoughtHeader,
  ChainOfThoughtStep,
} from '@/components/ai-elements/chain-of-thought'
import { buildToolHeader } from '@/components/shell/tool-call-metadata'
import type { PulseToolUse } from '@/lib/pulse/types'

export function ToolStepDetail({ tool }: { tool: PulseToolUse }) {
  const [expanded, setExpanded] = useState(false)
  const hasInput = tool.input && Object.keys(tool.input).length > 0
  const hasOutput = Boolean(tool.content)
  if (!hasInput && !hasOutput && !tool.locations?.length) return null

  const preview = tool.content ? tool.content.slice(0, 160).replace(/\n+/g, ' ').trim() : null
  const isTruncated = tool.content && tool.content.length > 160

  return (
    <div className="mt-0.5 space-y-0.5">
      {hasInput ? (
        <pre className="max-h-20 overflow-auto rounded bg-[rgba(0,0,0,0.3)] px-2 py-1 font-mono text-[9px] leading-[1.3] text-[var(--text-secondary)]">
          {JSON.stringify(tool.input, null, 2)}
        </pre>
      ) : null}
      {tool.locations?.length ? (
        <div className="flex flex-wrap gap-1">
          {tool.locations.map((loc) => (
            <span
              key={loc}
              className="inline-flex items-center gap-1 rounded border border-[var(--axon-primary-bg)] bg-[var(--axon-primary-bg)] px-1 py-0.5 font-mono text-[9px] text-[var(--axon-primary-strong)]"
            >
              <FileCode2 className="size-2.5" />
              {loc}
            </span>
          ))}
        </div>
      ) : null}
      {hasOutput ? (
        <pre
          className="cursor-pointer overflow-x-auto rounded bg-[rgba(0,0,0,0.3)] px-2 py-1 font-mono text-[9px] leading-[1.3] text-[var(--text-secondary)] whitespace-pre-wrap"
          onClick={() => setExpanded((v) => !v)}
        >
          {expanded ? tool.content : preview}
          {!expanded && isTruncated ? (
            <span className="text-[var(--text-dim)]">… (click to expand)</span>
          ) : null}
        </pre>
      ) : null}
    </div>
  )
}

export function ToolCallsGroup({ tools }: { tools: PulseToolUse[] }) {
  const hasRunning = tools.some((t) => t.status === 'running' || t.status === 'pending')
  const firstTool = tools[0]
  const firstTitle = firstTool ? buildToolHeader(firstTool).title : 'tool'
  const headerLabel = tools.length === 1 ? firstTitle : `${tools.length} tool calls`

  return (
    <ChainOfThought
      className="mt-0.5 space-y-0 rounded border border-[rgba(135,175,255,0.1)] bg-[rgba(7,12,26,0.5)] px-2 py-1"
      defaultOpen={hasRunning}
    >
      <ChainOfThoughtHeader
        hideIcon
        className="text-[10px] md:text-[10px] text-[var(--text-muted)]"
      >
        {headerLabel}
      </ChainOfThoughtHeader>
      <ChainOfThoughtContent className="mt-0.5 space-y-0.5">
        {tools.map((tool, i) => {
          const { title, description } = buildToolHeader(tool)
          const isDone = tool.status === 'completed' || tool.status === 'success'
          const isFailed = tool.status === 'failed' || tool.status === 'error'
          const stepStatus = isDone ? 'complete' : isFailed ? 'pending' : 'active'
          const statusColor = isDone
            ? 'text-[rgba(128,220,160,0.7)]'
            : isFailed
              ? 'text-[rgba(255,135,175,0.7)]'
              : 'text-[var(--axon-info)]'

          return (
            <ChainOfThoughtStep
              key={tool.toolCallId ?? i}
              status={stepStatus}
              className="text-[10px] md:text-[10px]"
              label={
                <span className="flex items-baseline gap-1.5 text-[10px]">
                  <span className="text-[var(--text-secondary)]">{title}</span>
                  {description ? (
                    <span className="text-[9px] text-[var(--text-dim)]">{description}</span>
                  ) : null}
                  <span
                    className={`text-[9px] font-semibold uppercase tracking-wide ${statusColor}`}
                  >
                    {isDone ? '✓' : isFailed ? '✗' : '…'}
                  </span>
                </span>
              }
              description={<ToolStepDetail tool={tool} />}
            />
          )
        })}
      </ChainOfThoughtContent>
    </ChainOfThought>
  )
}
