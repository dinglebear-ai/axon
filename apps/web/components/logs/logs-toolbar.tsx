'use client'

import { Pause, Play, Trash2, WrapText } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'

export const SERVICES = [
  'axon-workers',
  'axon-web',
  'axon-postgres',
  'axon-redis',
  'axon-rabbitmq',
  'axon-qdrant',
  'axon-chrome',
] as const

export type IndividualService = (typeof SERVICES)[number]
export type ServiceName = IndividualService | 'all'

export const TAIL_OPTIONS = [50, 100, 200, 500, 1000] as const
export type TailLines = (typeof TAIL_OPTIONS)[number]

interface LogsToolbarProps {
  service: ServiceName
  tailLines: TailLines
  filter: string
  autoScroll: boolean
  compact: boolean
  wrapLines: boolean
  isConnected: boolean
  onServiceChange: (s: ServiceName) => void
  onTailChange: (t: TailLines) => void
  onFilterChange: (f: string) => void
  onAutoScrollToggle: () => void
  onCompactToggle: () => void
  onWrapToggle: () => void
  onClear: () => void
}

export function LogsToolbar({
  service,
  tailLines,
  filter,
  autoScroll,
  compact,
  wrapLines,
  isConnected,
  onServiceChange,
  onTailChange,
  onFilterChange,
  onAutoScrollToggle,
  onCompactToggle,
  onWrapToggle,
  onClear,
}: LogsToolbarProps) {
  return (
    <div className="flex flex-wrap items-center gap-2">
      {/* Service selector */}
      <div className="flex items-center gap-1.5">
        <span className="text-[10px] font-semibold uppercase tracking-widest text-[var(--text-dim)]">
          Service
        </span>
        <Select value={service} onValueChange={(v) => onServiceChange(v as ServiceName)}>
          <SelectTrigger
            size="sm"
            className="h-7 w-auto min-w-[120px] border-[var(--border-subtle)] bg-[rgba(10,18,35,0.7)] text-[11px] font-medium text-[var(--text-secondary)]"
            aria-label="Select service"
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">All services</SelectItem>
            {SERVICES.map((s) => (
              <SelectItem key={s} value={s}>
                {s}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      {/* Tail selector */}
      <div className="flex items-center gap-1.5">
        <span className="text-[10px] font-semibold uppercase tracking-widest text-[var(--text-dim)]">
          Tail
        </span>
        <Select
          value={String(tailLines)}
          onValueChange={(v) => onTailChange(Number(v) as TailLines)}
        >
          <SelectTrigger
            size="sm"
            className="h-7 w-auto min-w-[70px] border-[var(--border-subtle)] bg-[rgba(10,18,35,0.7)] text-[11px] font-medium text-[var(--text-secondary)]"
            aria-label="Select tail lines"
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {TAIL_OPTIONS.map((n) => (
              <SelectItem key={n} value={String(n)}>
                {n}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      {/* Filter input */}
      <Input
        type="text"
        placeholder="Filter logs..."
        value={filter}
        onChange={(e) => onFilterChange(e.target.value)}
        className="h-7 min-w-[140px] border-[var(--border-subtle)] bg-[rgba(10,18,35,0.7)] text-[11px] text-[var(--text-secondary)] placeholder:text-[var(--text-dim)]"
        aria-label="Filter log lines"
      />

      {/* Auto-scroll toggle */}
      <Button
        variant="outline"
        size="sm"
        onClick={onAutoScrollToggle}
        className={`h-7 gap-1.5 text-[11px] font-medium ${
          autoScroll
            ? 'border-[var(--border-standard)] bg-[rgba(135,175,255,0.12)] text-[var(--axon-primary)]'
            : 'border-[var(--border-subtle)] bg-[rgba(10,18,35,0.7)] text-[var(--text-dim)]'
        }`}
        aria-pressed={autoScroll}
        title={autoScroll ? 'Pause auto-scroll' : 'Resume auto-scroll'}
      >
        {autoScroll ? <Pause className="size-3" /> : <Play className="size-3" />}
        Auto-scroll
      </Button>

      <Button
        variant="outline"
        size="sm"
        onClick={onCompactToggle}
        className={`h-7 text-[11px] font-medium ${
          compact
            ? 'border-[var(--border-standard)] bg-[rgba(135,175,255,0.12)] text-[var(--axon-primary)]'
            : 'border-[var(--border-subtle)] bg-[rgba(10,18,35,0.7)] text-[var(--text-dim)]'
        }`}
        aria-pressed={compact}
        title={compact ? 'Switch to comfortable spacing' : 'Switch to compact spacing'}
      >
        Compact
      </Button>

      <Button
        variant="outline"
        size="sm"
        onClick={onWrapToggle}
        className={`h-7 gap-1.5 text-[11px] font-medium ${
          wrapLines
            ? 'border-[var(--border-standard)] bg-[rgba(135,175,255,0.12)] text-[var(--axon-primary)]'
            : 'border-[var(--border-subtle)] bg-[rgba(10,18,35,0.7)] text-[var(--text-dim)]'
        }`}
        aria-pressed={wrapLines}
        title={wrapLines ? 'Disable line wrapping' : 'Enable line wrapping'}
      >
        <WrapText className="size-3" />
        Wrap
      </Button>

      <Button
        variant="outline"
        size="sm"
        onClick={onClear}
        className="h-7 gap-1.5 border-[var(--border-subtle)] bg-[rgba(10,18,35,0.7)] text-[11px] font-medium text-[var(--text-dim)]"
        title="Clear visible log buffer"
      >
        <Trash2 className="size-3" />
        Clear
      </Button>

      {/* Connection status */}
      <div className="ml-auto flex items-center gap-1.5">
        <span
          className="inline-block size-2 rounded-full"
          style={{
            background: isConnected ? 'var(--axon-success)' : '#ef4444',
            boxShadow: isConnected
              ? '0 0 6px rgba(130,217,160,0.6)'
              : '0 0 6px rgba(239,68,68,0.6)',
          }}
          aria-hidden="true"
        />
        <span className="text-[10px] font-medium text-[var(--text-dim)]">
          {isConnected ? 'Live' : 'Disconnected'}
        </span>
      </div>
    </div>
  )
}
