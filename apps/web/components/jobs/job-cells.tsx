'use client'

import {
  AlertCircle,
  ArrowDown,
  ArrowUp,
  ArrowUpDown,
  Ban,
  CheckCircle2,
  Clock,
  ExternalLink,
} from 'lucide-react'
import Link from 'next/link'
import type { Job, JobStatus, JobType, StatusCounts } from '@/app/api/jobs/route'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'

// ── Helpers ───────────────────────────────────────────────────────────────────

// 3. RELATIVE TIMESTAMPS with absolute on hover
export function formatRelativeTime(isoString: string | null): {
  relative: string
  absolute: string
} {
  if (!isoString) return { relative: '—', absolute: '—' }
  const date = new Date(isoString)
  const diff = Date.now() - date.getTime()
  const absolute = date.toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
  if (diff < 60_000) return { relative: 'just now', absolute }
  if (diff < 3_600_000) return { relative: `${Math.floor(diff / 60_000)}m ago`, absolute }
  if (diff < 86_400_000) return { relative: `${Math.floor(diff / 3_600_000)}h ago`, absolute }
  return { relative: `${Math.floor(diff / 86_400_000)}d ago`, absolute }
}

// 4. SMART URL TRUNCATION — show last 2 path segments
export function smartTruncate(target: string): string {
  if (target.startsWith('http')) {
    try {
      const url = new URL(target)
      const parts = url.pathname.split('/').filter(Boolean)
      if (parts.length >= 2) return `…/${parts.slice(-2).join('/')}`
      if (parts.length === 1) return `${url.hostname}/…/${parts[0]}`
      return url.hostname
    } catch {
      // fall through to path logic
    }
  }
  const parts = target.split(/[/\\]/).filter(Boolean)
  if (parts.length >= 2) return `…/${parts.slice(-2).join('/')}`
  return target
}

// ── 1. COLOR-CODED TYPE CHIP ─────────────────────────────────────────────────

const TYPE_STYLES: Record<JobType, { chip: string; dot: string }> = {
  crawl: {
    chip: 'text-[#38bdf8] bg-[rgba(56,189,248,0.1)] border border-[rgba(56,189,248,0.25)]',
    dot: '#38bdf8',
  },
  embed: {
    chip: 'text-[#fbbf24] bg-[rgba(251,191,36,0.1)] border border-[rgba(251,191,36,0.25)]',
    dot: '#fbbf24',
  },
  extract: {
    chip: 'text-[#a78bfa] bg-[rgba(167,139,250,0.1)] border border-[rgba(167,139,250,0.25)]',
    dot: '#a78bfa',
  },
  ingest: {
    chip: 'text-[#fb7185] bg-[rgba(251,113,133,0.1)] border border-[rgba(251,113,133,0.25)]',
    dot: '#fb7185',
  },
  refresh: {
    chip: 'text-[#34d399] bg-[rgba(52,211,153,0.1)] border border-[rgba(52,211,153,0.25)]',
    dot: '#34d399',
  },
}

export function TypeChip({ type }: { type: JobType }) {
  const s = TYPE_STYLES[type]
  return (
    <Badge
      variant="outline"
      className={`gap-1 rounded-full text-[9px] font-bold uppercase tracking-widest ${s.chip}`}
    >
      <span className="size-1.5 flex-shrink-0 rounded-full" style={{ background: s.dot }} />
      {type}
    </Badge>
  )
}

// ── 2. RICHER STATUS BADGE ────────────────────────────────────────────────────

export function StatusBadge({ status }: { status: JobStatus }) {
  if (status === 'running') {
    return (
      <Badge variant="outline" className="gap-2 rounded-full border-transparent bg-transparent">
        <span className="relative flex size-2.5 flex-shrink-0">
          <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-[#38bdf8] opacity-60" />
          <span className="relative inline-flex size-2.5 rounded-full bg-[#38bdf8]" />
        </span>
        <span className="text-[10px] font-semibold text-[#38bdf8]">Active</span>
        {/* 8. RUNNING PROGRESS -- indeterminate shimmer bar */}
        <span className="h-1 w-10 overflow-hidden rounded-full bg-[rgba(56,189,248,0.15)]">
          <span className="block h-full w-full animate-shimmer bg-[linear-gradient(90deg,transparent_25%,rgba(56,189,248,0.7)_50%,transparent_75%)] [background-size:200%_100%]" />
        </span>
      </Badge>
    )
  }
  if (status === 'pending') {
    return (
      <Badge variant="outline" className="gap-1.5 rounded-full border-transparent bg-transparent">
        <Clock className="size-3 text-[var(--text-dim)]" />
        <span className="text-[10px] font-medium text-[var(--text-dim)]">Pending</span>
      </Badge>
    )
  }
  if (status === 'completed') {
    return (
      <Badge
        variant="outline"
        className="gap-1.5 rounded-full border-transparent bg-transparent opacity-40"
      >
        <CheckCircle2 className="size-3 text-[var(--axon-success)]" />
        <span className="text-[10px] font-medium text-[var(--axon-success)]">Done</span>
      </Badge>
    )
  }
  if (status === 'failed') {
    return (
      <Badge variant="outline" className="gap-1.5 rounded-full border-transparent bg-transparent">
        <AlertCircle className="size-3 text-red-400" />
        <span className="text-[10px] font-semibold text-red-400">Failed</span>
      </Badge>
    )
  }
  return (
    <Badge
      variant="outline"
      className="gap-1.5 rounded-full border-transparent bg-transparent opacity-60"
    >
      <Ban className="size-3 text-yellow-400" />
      <span className="text-[10px] font-medium text-yellow-400">Canceled</span>
    </Badge>
  )
}

// ── 6. STATS SUMMARY BAR ──────────────────────────────────────────────────────

function StatPip({
  color,
  label,
  count,
  pulse,
}: {
  color: string
  label: string
  count: number
  pulse?: boolean
}) {
  return (
    <span
      className={`flex items-center gap-1.5 text-[10px] transition-opacity ${count === 0 ? 'opacity-30' : ''}`}
    >
      <span
        className={`size-1.5 flex-shrink-0 rounded-full ${pulse && count > 0 ? 'animate-pulse' : ''}`}
        style={{ background: color }}
      />
      <span className="font-mono font-semibold tabular-nums" style={{ color }}>
        {count.toLocaleString()}
      </span>
      <span className="text-[var(--text-dim)]">{label}</span>
    </span>
  )
}

export function StatsBar({ counts }: { counts: StatusCounts | undefined }) {
  if (!counts) return null
  const total = counts.running + counts.pending + counts.completed + counts.failed
  if (total === 0) return null
  return (
    <div className="StatsBar mb-3 flex flex-wrap items-center gap-5 rounded-lg border border-[var(--border-subtle)] bg-[rgba(10,18,35,0.4)] px-4 py-2.5">
      <StatPip color="#38bdf8" label="active" count={counts.running} pulse />
      <StatPip color="var(--text-dim)" label="pending" count={counts.pending} />
      <StatPip color="var(--axon-success)" label="done" count={counts.completed} />
      <StatPip color="#f87171" label="failed" count={counts.failed} />
    </div>
  )
}

// ── 7. SORTABLE COLUMN HEADER ─────────────────────────────────────────────────

export type SortColumn = 'type' | 'target' | 'collection' | 'status' | 'started'
export type SortDir = 'asc' | 'desc'

export function SortableHeader({
  column,
  label,
  sort,
  onSort,
}: {
  column: SortColumn
  label: string
  sort: { column: SortColumn; dir: SortDir }
  onSort: (col: SortColumn) => void
}) {
  const active = sort.column === column
  return (
    <Button
      variant="ghost"
      size="xs"
      onClick={() => onSort(column)}
      className={`gap-1 text-[10px] font-semibold uppercase tracking-widest ${
        active ? 'text-[var(--axon-primary)]' : 'text-[var(--text-dim)]'
      } hover:text-[var(--text-secondary)]`}
    >
      {label}
      {active ? (
        sort.dir === 'asc' ? (
          <ArrowUp className="size-3" />
        ) : (
          <ArrowDown className="size-3" />
        )
      ) : (
        <ArrowUpDown className="size-3 opacity-30" />
      )}
    </Button>
  )
}

// ── Skeleton row ──────────────────────────────────────────────────────────────

export function SkeletonRow() {
  const shimmer =
    'animate-shimmer rounded bg-[linear-gradient(90deg,rgba(135,175,255,0.04)_25%,rgba(135,175,255,0.09)_50%,rgba(135,175,255,0.04)_75%)] [background-size:200%_100%]'
  return (
    <tr className="border-b border-[var(--border-subtle)]">
      <td className="px-3 py-2.5">
        <div className={`${shimmer} h-3 w-14`} />
      </td>
      <td className="px-3 py-2.5">
        <div className={`${shimmer} h-3 w-full max-w-[280px]`} />
      </td>
      <td className="px-3 py-2.5">
        <div className={`${shimmer} h-3 w-16`} />
      </td>
      <td className="px-3 py-2.5">
        <div className={`${shimmer} h-4 w-20 rounded-full`} />
      </td>
      <td className="px-3 py-2.5">
        <div className={`${shimmer} h-3 w-16`} />
      </td>
    </tr>
  )
}

// ── 5. JOB ROW with hover actions ────────────────────────────────────────────

export function JobRow({ job }: { job: Job }) {
  const { relative, absolute } = formatRelativeTime(job.startedAt)
  const truncated = smartTruncate(job.target)

  return (
    <tr
      className="JobRow group border-b border-[var(--border-subtle)] transition-colors duration-100 hover:bg-[rgba(135,175,255,0.04)]"
      title={job.errorText ?? undefined}
    >
      <td className="px-3 py-2.5">
        <TypeChip type={job.type} />
      </td>
      <td className="max-w-[280px] px-3 py-2.5">
        <Link
          href={`/jobs/${job.id}`}
          className="group/link flex min-w-0 items-center gap-1.5"
          title={job.target}
        >
          <span className="block truncate font-mono text-[11px] text-[var(--text-secondary)] transition-colors group-hover/link:text-[var(--axon-primary)]">
            {truncated}
          </span>
          <ExternalLink className="size-3 flex-shrink-0 text-[var(--text-dim)] opacity-0 transition-opacity group-hover/link:opacity-100" />
        </Link>
      </td>
      <td className="px-3 py-2.5">
        {job.collection ? (
          <span className="whitespace-nowrap rounded bg-[rgba(135,175,255,0.07)] px-1.5 py-0.5 font-mono text-[10px] text-[var(--text-dim)]">
            {job.collection}
          </span>
        ) : (
          <span className="text-[10px] text-[var(--text-dim)]">—</span>
        )}
      </td>
      <td className="px-3 py-2.5">
        <StatusBadge status={job.status} />
      </td>
      <td
        className="whitespace-nowrap px-3 py-2.5 font-mono text-[10px] text-[var(--text-dim)]"
        title={absolute}
        suppressHydrationWarning
      >
        {relative}
      </td>
    </tr>
  )
}
