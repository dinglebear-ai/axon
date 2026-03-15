'use client'

import { useCallback, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { AXON_COMMAND_OPTIONS, type AxonOptionSpec, getCommandSpec } from '@/lib/axon-command-map'
import type { CommandOptionValues } from '@/lib/command-options'

interface CommandOptionsPanelProps {
  mode: string
  values: CommandOptionValues
  onChange: (values: CommandOptionValues) => void
}

function parseEnumValues(notes?: string): string[] {
  if (!notes) return []
  // Match patterns like "hot|top|new|rising" or "hour|day|week|month|year|all"
  const pipeMatch = notes.match(/:\s*([a-zA-Z0-9_|]+)$/)
  if (pipeMatch) {
    // pipeMatch[1] is always defined when the match succeeds (capture group 1)
    return pipeMatch[1]!.split('|').filter(Boolean)
  }
  return []
}

function getOptionSpec(key: string): AxonOptionSpec | undefined {
  return AXON_COMMAND_OPTIONS.find((o) => o.key === key)
}

function OptionControl({
  optionKey,
  spec,
  value,
  onUpdate,
}: {
  optionKey: string
  spec: AxonOptionSpec
  value: string | boolean | number | undefined
  onUpdate: (key: string, val: string | boolean | number) => void
}) {
  const label = optionKey.replace(/_/g, ' ')

  switch (spec.value) {
    case 'bool':
      return (
        <div className="flex cursor-pointer items-center gap-2.5 rounded-lg px-3 py-2 transition-colors hover:bg-[var(--surface-float)]">
          <Checkbox
            checked={!!value}
            onCheckedChange={(checked) => onUpdate(optionKey, !!checked)}
            aria-label={label}
          />
          <span className="text-xs text-[var(--text-muted)]">{label}</span>
        </div>
      )

    case 'number':
      return (
        <div className="flex items-center gap-2.5 rounded-lg px-3 py-2">
          <span className="shrink-0 text-xs text-[var(--text-muted)]">{label}</span>
          <Input
            type="number"
            value={value !== undefined ? String(value) : ''}
            onChange={(e) => {
              const n = Number.parseInt(e.target.value, 10)
              if (!Number.isNaN(n)) onUpdate(optionKey, n)
            }}
            aria-label={label}
            placeholder="--"
            className="h-7 w-20 border-[var(--border-subtle)] bg-[rgba(10,18,35,0.5)] font-mono text-xs text-[var(--axon-secondary)] placeholder:text-[var(--text-dim)]"
          />
        </div>
      )

    case 'enum': {
      const options = parseEnumValues(spec.notes)
      return (
        <div className="flex items-center gap-2.5 rounded-lg px-3 py-2">
          <span className="shrink-0 text-xs text-[var(--text-muted)]">{label}</span>
          <Select
            value={value !== undefined ? String(value) : ''}
            onValueChange={(v) => onUpdate(optionKey, v)}
          >
            <SelectTrigger
              size="sm"
              aria-label={label}
              className="h-7 w-auto min-w-[80px] border-[var(--border-subtle)] bg-[rgba(10,18,35,0.5)] font-mono text-xs text-[var(--axon-secondary)]"
            >
              <SelectValue placeholder="default" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="">default</SelectItem>
              {options.map((opt) => (
                <SelectItem key={opt} value={opt}>
                  {opt}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      )
    }

    case 'string':
    case 'list':
      return (
        <div className="flex items-center gap-2.5 rounded-lg px-3 py-2">
          <span className="shrink-0 text-xs text-[var(--text-muted)]">{label}</span>
          <Input
            type="text"
            value={value !== undefined ? String(value) : ''}
            onChange={(e) => onUpdate(optionKey, e.target.value)}
            aria-label={label}
            placeholder={spec.value === 'list' ? 'comma-separated' : '--'}
            className="h-7 w-40 border-[var(--border-subtle)] bg-[rgba(10,18,35,0.5)] font-mono text-xs text-[var(--axon-secondary)] placeholder:text-[var(--text-dim)]"
          />
        </div>
      )

    default:
      return null
  }
}

export function CommandOptionsPanel({ mode, values, onChange }: CommandOptionsPanelProps) {
  const [expanded, setExpanded] = useState(false)

  // useCallback must be called unconditionally (Rules of Hooks)
  const handleUpdate = useCallback(
    (key: string, val: string | boolean | number) => {
      onChange({ ...values, [key]: val })
    },
    [values, onChange],
  )

  const spec = getCommandSpec(mode)
  const optionKeys = spec?.commandOptions ?? []
  if (optionKeys.length === 0) return null

  const resolvedOptions = optionKeys
    .map((key) => ({ key, spec: getOptionSpec(key) }))
    .filter((o): o is { key: string; spec: AxonOptionSpec } => o.spec !== undefined)

  if (resolvedOptions.length === 0) return null

  return (
    <div
      className="overflow-hidden rounded-lg border border-[var(--border-subtle)] transition-all duration-200"
      style={{ background: 'rgba(10, 18, 35, 0.45)' }}
    >
      <Button
        variant="ghost"
        size="sm"
        onClick={() => setExpanded((prev) => !prev)}
        aria-expanded={expanded}
        className="flex w-full items-center gap-2 px-3 py-2 text-left hover:bg-[var(--surface-float)]"
      >
        <svg
          className={`size-3 shrink-0 text-[var(--text-dim)] transition-transform duration-200 ${expanded ? 'rotate-90' : ''}`}
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
        </svg>
        <span className="text-[10px] font-semibold uppercase tracking-wider text-[var(--text-dim)]">
          Options
        </span>
        <span className="text-[10px] text-[var(--text-dim)]">({resolvedOptions.length})</span>
      </Button>

      {expanded && (
        <div className="flex flex-wrap gap-x-2 gap-y-0.5 border-t border-[var(--border-subtle)] px-1 pb-2 pt-1">
          {resolvedOptions.map(({ key, spec: optSpec }) => (
            <OptionControl
              key={key}
              optionKey={key}
              spec={optSpec}
              value={values[key]}
              onUpdate={handleUpdate}
            />
          ))}
        </div>
      )}
    </div>
  )
}
