import { useVirtualizer } from '@tanstack/react-virtual'
import { CheckCircle, Clock, RefreshCw, XCircle } from 'lucide-react'
import type React from 'react'
import { useRef, useState } from 'react'
import type { JobDetail } from '@/app/api/jobs/[id]/route'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'

export const STATUS_CONFIG = {
  pending: {
    label: 'Pending',
    color: 'text-[var(--text-dim)]',
    bg: 'bg-[rgba(135,175,255,0.08)]',
    icon: Clock,
  },
  running: {
    label: 'Running',
    color: 'text-[var(--axon-primary)]',
    bg: 'bg-[rgba(135,175,255,0.12)]',
    icon: RefreshCw,
  },
  completed: {
    label: 'Completed',
    color: 'text-[var(--axon-success,#87d7af)]',
    bg: 'bg-[rgba(135,215,175,0.12)]',
    icon: CheckCircle,
  },
  failed: {
    label: 'Failed',
    color: 'text-[var(--axon-secondary)]',
    bg: 'bg-[rgba(255,135,175,0.12)]',
    icon: XCircle,
  },
  canceled: {
    label: 'Canceled',
    color: 'text-[var(--text-muted)]',
    bg: 'bg-[rgba(135,135,175,0.1)]',
    icon: XCircle,
  },
} as const

export const TYPE_COLORS: Record<string, string> = {
  crawl: 'text-[var(--axon-primary)]   bg-[rgba(135,175,255,0.1)]',
  embed: 'text-[var(--axon-secondary)] bg-[rgba(255,135,175,0.1)]',
  extract: 'text-[#d7af87]               bg-[rgba(215,175,135,0.1)]',
  ingest: 'text-[#87d7d7]               bg-[rgba(135,215,215,0.1)]',
  refresh: 'text-[#34d399]              bg-[rgba(52,211,153,0.1)]',
}

export function StatusBadge({ status }: { status: JobDetail['status'] }) {
  const cfg = STATUS_CONFIG[status] ?? STATUS_CONFIG.pending
  const Icon = cfg.icon
  return (
    <Badge
      variant="outline"
      className={`gap-1.5 rounded-full border-transparent text-xs font-medium ${cfg.color} ${cfg.bg}`}
    >
      <Icon className={`size-3.5 ${status === 'running' ? 'animate-spin' : ''}`} />
      {cfg.label}
    </Badge>
  )
}

export function TypeBadge({ type }: { type: string }) {
  return (
    <Badge
      variant="outline"
      className={`rounded-full border-transparent text-xs font-semibold uppercase tracking-wider ${TYPE_COLORS[type] ?? ''}`}
    >
      {type}
    </Badge>
  )
}

export function Stat({
  label,
  value,
  icon: Icon,
}: {
  label: string
  value: string | number | null
  icon?: React.ElementType
}) {
  return (
    <div className="flex flex-col gap-1 rounded border border-[var(--border-subtle)] bg-[rgba(10,18,35,0.6)] px-4 py-3">
      <div className="flex items-center gap-1.5 text-[10px] font-semibold uppercase tracking-wider text-[var(--text-dim)]">
        {Icon && <Icon className="size-3" />}
        {label}
      </div>
      <div className="font-mono text-lg font-bold text-[var(--text-primary)]">{value ?? '—'}</div>
    </div>
  )
}

export function Section({
  title,
  icon: Icon,
  children,
}: {
  title: string
  icon: React.ElementType
  children: React.ReactNode
}) {
  return (
    <div className="rounded border border-[var(--border-subtle)] bg-[rgba(10,18,35,0.5)]">
      <div className="flex items-center gap-2 border-b border-[var(--border-subtle)] px-4 py-2.5">
        <Icon className="size-4 text-[var(--text-dim)]" />
        <span className="text-xs font-semibold uppercase tracking-wider text-[var(--text-dim)]">
          {title}
        </span>
      </div>
      <div className="p-4">{children}</div>
    </div>
  )
}

export function KV({
  label,
  value,
  mono = false,
}: {
  label: string
  value: React.ReactNode
  mono?: boolean
}) {
  return (
    <div className="flex items-start gap-3 border-b border-[var(--border-subtle)] py-1.5 last:border-0">
      <span className="w-36 flex-shrink-0 text-[11px] text-[var(--text-dim)]">{label}</span>
      <span
        className={`min-w-0 break-all text-[11px] text-[var(--text-secondary)] ${mono ? 'font-mono' : ''}`}
      >
        {value ?? '—'}
      </span>
    </div>
  )
}

export function ShowMoreList<T>({
  title,
  items,
  emptyText,
  initial = 100,
  step = 100,
  renderItem,
}: {
  title: string
  items: T[]
  emptyText: string
  initial?: number
  step?: number
  renderItem: (item: T, index: number) => React.ReactNode
}) {
  const [visible, setVisible] = useState(initial)
  const shown = items.slice(0, visible)
  const hasMore = visible < items.length
  const scrollRef = useRef<HTMLDivElement>(null)

  const rowVirtualizer = useVirtualizer({
    count: shown.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 32,
    overscan: 15,
  })

  const useVirtual = shown.length > 100

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <div className="text-[11px] font-semibold uppercase tracking-wider text-[var(--text-dim)]">
          {title}
        </div>
        <div className="font-mono text-[10px] text-[var(--text-muted)]">
          {items.length.toLocaleString()} total
        </div>
      </div>
      {items.length === 0 ? (
        <div className="rounded border border-[var(--border-subtle)] bg-[rgba(10,18,35,0.35)] px-3 py-2 text-[11px] text-[var(--text-muted)]">
          {emptyText}
        </div>
      ) : useVirtual ? (
        <div
          ref={scrollRef}
          className="max-h-[400px] overflow-y-auto rounded border border-[var(--border-subtle)] bg-[rgba(10,18,35,0.35)] p-2"
        >
          <div style={{ height: `${rowVirtualizer.getTotalSize()}px`, position: 'relative' }}>
            {rowVirtualizer.getVirtualItems().map((virtualRow) => (
              <div
                key={virtualRow.key}
                data-index={virtualRow.index}
                ref={rowVirtualizer.measureElement}
                style={{
                  position: 'absolute',
                  top: 0,
                  left: 0,
                  width: '100%',
                  transform: `translateY(${virtualRow.start}px)`,
                }}
              >
                {renderItem(shown[virtualRow.index]!, virtualRow.index)}
              </div>
            ))}
          </div>
        </div>
      ) : (
        <ul className="max-h-[400px] space-y-1 overflow-y-auto rounded border border-[var(--border-subtle)] bg-[rgba(10,18,35,0.35)] p-2">
          {shown.map((item, idx) => (
            <li key={idx}>{renderItem(item, idx)}</li>
          ))}
        </ul>
      )}
      {hasMore && (
        <Button
          variant="outline"
          size="sm"
          onClick={() => setVisible((prev) => prev + step)}
          className="border-[var(--border-subtle)] text-[11px] text-[var(--axon-primary)] hover:bg-[rgba(135,175,255,0.1)]"
        >
          Show {Math.min(step, items.length - visible).toLocaleString()} more
        </Button>
      )}
    </div>
  )
}
