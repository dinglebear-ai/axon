import { useVirtualizer } from '@tanstack/react-virtual'
import {
  AlertCircle,
  ArrowLeft,
  CheckCircle,
  Clock,
  Database,
  ExternalLink,
  FileText,
  Globe,
  Layers,
  RefreshCw,
  Settings,
  XCircle,
} from 'lucide-react'
import Link from 'next/link'
import type React from 'react'
import { useRef, useState } from 'react'
import type { JobDetail } from '@/app/api/jobs/[id]/route'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  flattenJsonEntries,
  fmtDate,
  fmtDuration,
  getRefreshSummaryRows,
} from './job-detail-helpers'

const STATUS_CONFIG = {
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

const TYPE_COLORS: Record<string, string> = {
  crawl: 'text-[var(--axon-primary)]   bg-[rgba(135,175,255,0.1)]',
  embed: 'text-[var(--axon-secondary)] bg-[rgba(255,135,175,0.1)]',
  extract: 'text-[#d7af87]               bg-[rgba(215,175,135,0.1)]',
  ingest: 'text-[#87d7d7]               bg-[rgba(135,215,215,0.1)]',
  refresh: 'text-[#34d399]              bg-[rgba(52,211,153,0.1)]',
}

function StatusBadge({ status }: { status: JobDetail['status'] }) {
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

function TypeBadge({ type }: { type: string }) {
  return (
    <Badge
      variant="outline"
      className={`rounded-full border-transparent text-xs font-semibold uppercase tracking-wider ${TYPE_COLORS[type] ?? ''}`}
    >
      {type}
    </Badge>
  )
}

function Stat({
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

function Section({
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

function KV({
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

function ShowMoreList<T>({
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

export function JobDetailLoadingState() {
  return (
    <div className="flex h-full items-center justify-center text-[var(--text-dim)]">
      <RefreshCw className="mr-2 size-5 animate-spin" />
      Loading job…
    </div>
  )
}

export function JobDetailErrorState({ error }: { error: string | null }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-4 text-[var(--text-muted)]">
      <AlertCircle className="size-10 text-[var(--axon-secondary)]" />
      <p className="text-sm">{error ?? 'Job not found'}</p>
      <Button variant="ghost" size="sm" asChild>
        <Link
          href="/"
          className="gap-1.5 text-xs text-[var(--axon-primary)] hover:bg-[rgba(135,175,255,0.1)]"
        >
          <ArrowLeft className="size-3.5" />
          Back to Dashboard
        </Link>
      </Button>
    </div>
  )
}

export function JobDetailView({ job }: { job: JobDetail }) {
  const duration = fmtDuration(job.elapsedMs, job.startedAt, job.finishedAt)
  const isUrl = job.type === 'crawl' || (job.type === 'embed' && job.target.startsWith('http'))
  const resultJsonFlat = flattenJsonEntries(job.resultJson)
  const configJsonFlat = flattenJsonEntries(job.configJson)

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <div className="flex flex-shrink-0 items-center gap-3 border-b border-[var(--border-subtle)] px-5 py-3">
        <Button variant="ghost" size="sm" asChild>
          <Link
            href="/"
            className="gap-1.5 text-xs text-[var(--text-muted)] hover:text-[var(--text-secondary)]"
          >
            <ArrowLeft className="size-3.5" />
            Dashboard
          </Link>
        </Button>
        <span className="text-[var(--text-dim)]">/</span>
        <TypeBadge type={job.type} />
        <StatusBadge status={job.status} />
        {job.status === 'running' && (
          <span className="ml-auto animate-pulse text-[10px] text-[var(--text-dim)]">
            Auto-refreshing…
          </span>
        )}
      </div>

      <div className="flex-1 space-y-5 overflow-y-auto px-5 py-5">
        <div className="rounded border border-[var(--border-subtle)] bg-[rgba(10,18,35,0.6)] px-4 py-3">
          <div className="flex items-start gap-2">
            <Globe className="mt-0.5 size-4 flex-shrink-0 text-[var(--text-dim)]" />
            <div className="min-w-0 flex-1">
              <div className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-[var(--text-dim)]">
                Target
              </div>
              {isUrl ? (
                <a
                  href={job.target}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="flex items-center gap-1.5 break-all font-mono text-sm text-[var(--axon-primary)] hover:underline"
                >
                  {job.target}
                  <ExternalLink className="size-3 flex-shrink-0" />
                </a>
              ) : (
                <span className="break-all font-mono text-sm text-[var(--text-secondary)]">
                  {job.target}
                </span>
              )}
            </div>
          </div>
        </div>

        <div className="flex items-center gap-2 rounded border border-[var(--border-subtle)] bg-[rgba(10,18,35,0.4)] px-4 py-2.5">
          <span className="w-16 flex-shrink-0 text-[10px] font-semibold uppercase tracking-wider text-[var(--text-dim)]">
            Job ID
          </span>
          <code className="break-all font-mono text-[11px] text-[var(--text-secondary)]">
            {job.id}
          </code>
        </div>

        {job.type === 'crawl' && (
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
            <Stat label="Pages Crawled" value={job.pagesCrawled} icon={Globe} />
            <Stat label="Pages Discovered" value={job.pagesDiscovered} icon={Globe} />
            <Stat label="Markdown Created" value={job.mdCreated} icon={FileText} />
            <Stat label="Thin Skipped" value={job.thinMd} icon={FileText} />
            <Stat label="Filtered URLs" value={job.filteredUrls} icon={Globe} />
            <Stat label="Error Pages" value={job.errorPages} icon={AlertCircle} />
            <Stat label="WAF Blocked" value={job.wafBlockedPages} icon={AlertCircle} />
            <Stat
              label="Success"
              value={job.success == null ? 'running' : job.success ? 'yes' : 'no'}
              icon={job.success == null ? Clock : job.success ? CheckCircle : XCircle}
            />
          </div>
        )}

        {job.type === 'embed' && (
          <div className="grid grid-cols-2 gap-3">
            <Stat label="Docs Embedded" value={job.docsEmbedded} icon={FileText} />
            <Stat label="Chunks Embedded" value={job.chunksEmbedded} icon={Database} />
          </div>
        )}

        {job.type === 'extract' && job.urls && job.urls.length > 0 && (
          <Section title="URLs" icon={Globe}>
            <ul className="space-y-1">
              {job.urls.map((u) => {
                const isSafeUrl = /^https?:\/\//i.test(u)
                return (
                  <li key={u}>
                    {isSafeUrl ? (
                      <a
                        href={u}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="flex items-center gap-1 font-mono text-[11px] text-[var(--axon-primary)] hover:underline"
                      >
                        {u} <ExternalLink className="size-3 flex-shrink-0" />
                      </a>
                    ) : (
                      <span className="font-mono text-[11px] text-[var(--axon-primary)]">{u}</span>
                    )}
                  </li>
                )
              })}
            </ul>
          </Section>
        )}

        {job.type === 'refresh' && (
          <Section title="Refresh Summary" icon={RefreshCw}>
            <div className="space-y-0">
              {getRefreshSummaryRows(job).map((row) => (
                <KV
                  key={row.label}
                  label={row.label}
                  value={row.value}
                  mono={row.label === 'Manifest Path'}
                />
              ))}
            </div>
            {job.urls && job.urls.length > 0 && (
              <div className="mt-3">
                <ShowMoreList
                  title="Refresh URLs"
                  items={job.urls}
                  emptyText="No refresh URLs recorded."
                  renderItem={(url) => {
                    const isSafeUrl = /^https?:\/\//i.test(url)
                    return isSafeUrl ? (
                      <a
                        href={url}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="flex items-center gap-1 break-all font-mono text-[11px] text-[var(--axon-primary)] hover:underline"
                      >
                        {url}
                        <ExternalLink className="size-3 flex-shrink-0" />
                      </a>
                    ) : (
                      <span className="break-all font-mono text-[11px] text-[var(--text-secondary)]">
                        {url}
                      </span>
                    )
                  }}
                />
              </div>
            )}
          </Section>
        )}

        {job.errorText && (
          <div className="rounded border border-[rgba(255,135,175,0.3)] bg-[rgba(255,135,175,0.06)] px-4 py-3">
            <div className="mb-2 flex items-center gap-2">
              <XCircle className="size-4 text-[var(--axon-secondary)]" />
              <span className="text-xs font-semibold text-[var(--axon-secondary)]">Error</span>
            </div>
            <pre className="whitespace-pre-wrap font-mono text-[11px] text-[var(--text-secondary)]">
              {job.errorText}
            </pre>
          </div>
        )}

        <Section title="Timing" icon={Clock}>
          <div className="space-y-0">
            <KV label="Created" value={fmtDate(job.createdAt)} />
            <KV label="Started" value={fmtDate(job.startedAt)} />
            <KV label="Finished" value={fmtDate(job.finishedAt)} />
            <KV label="Duration" value={duration} mono />
          </div>
        </Section>

        <Section title="Configuration" icon={Settings}>
          <div className="space-y-0">
            {job.collection && <KV label="Collection" value={job.collection} mono />}
            {job.renderMode && <KV label="Render Mode" value={job.renderMode} mono />}
            {job.maxDepth != null && <KV label="Max Depth" value={job.maxDepth} />}
            {job.maxPages != null && (
              <KV label="Max Pages" value={job.maxPages === 0 ? 'unlimited' : job.maxPages} />
            )}
            {job.embed != null && <KV label="Auto-embed" value={job.embed ? 'yes' : 'no'} />}
            {job.cacheHit != null && <KV label="Cache Hit" value={job.cacheHit ? 'yes' : 'no'} />}
            {job.outputDir && <KV label="Output Dir" value={job.outputDir} mono />}
            {job.staleUrlsDeleted != null && (
              <KV label="Stale URLs Deleted" value={job.staleUrlsDeleted} />
            )}
            {typeof job.resultJson?.manifest_path === 'string' && (
              <KV label="Manifest Path" value={job.resultJson.manifest_path} mono />
            )}
            {typeof job.resultJson?.audit_report_path === 'string' && (
              <KV label="Audit Report Path" value={job.resultJson.audit_report_path} mono />
            )}
          </div>
        </Section>

        {job.type === 'crawl' && (
          <Section title="Crawl Artifacts" icon={FileText}>
            <div className="space-y-4">
              <ShowMoreList
                title="Observed URLs"
                items={job.observedUrls ?? []}
                emptyText="No URL artifacts found."
                renderItem={(url) => (
                  <a
                    href={url}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="flex items-center gap-1 break-all font-mono text-[11px] text-[var(--axon-primary)] hover:underline"
                  >
                    {url}
                    <ExternalLink className="size-3 flex-shrink-0" />
                  </a>
                )}
              />
              <ShowMoreList
                title="Markdown Files Created"
                items={job.markdownFiles ?? []}
                emptyText="No manifest file entries found."
                renderItem={(entry) => (
                  <div className="space-y-0.5 rounded border border-[var(--border-subtle)] bg-[rgba(0,0,0,0.2)] px-2 py-1.5">
                    <div className="break-all font-mono text-[11px] text-[var(--text-secondary)]">
                      {entry.relativePath}
                    </div>
                    <div className="break-all font-mono text-[10px] text-[var(--text-muted)]">
                      {entry.url}
                    </div>
                    <div className="text-[10px] text-[var(--text-dim)]">
                      {entry.markdownChars.toLocaleString()} chars
                    </div>
                  </div>
                )}
              />
              <ShowMoreList
                title="Thin URLs"
                items={job.thinUrls ?? []}
                emptyText="No thin URL list recorded."
                renderItem={(url) => (
                  <span className="break-all font-mono text-[11px] text-[var(--text-secondary)]">
                    {url}
                  </span>
                )}
              />
              <ShowMoreList
                title="WAF Blocked URLs"
                items={job.wafBlockedUrls ?? []}
                emptyText="No WAF-blocked URL list recorded."
                renderItem={(url) => (
                  <span className="break-all font-mono text-[11px] text-[var(--text-secondary)]">
                    {url}
                  </span>
                )}
              />
            </div>
          </Section>
        )}

        <Section title="Result JSON Metadata" icon={Layers}>
          <div className="space-y-0">
            {resultJsonFlat.length === 0 ? (
              <KV label="result_json" value="—" />
            ) : (
              resultJsonFlat.map((entry) => (
                <KV key={entry.key} label={entry.key} value={entry.value} mono />
              ))
            )}
          </div>
        </Section>

        <Section title="Config JSON Metadata" icon={Settings}>
          <div className="space-y-0">
            {configJsonFlat.length === 0 ? (
              <KV label="config_json" value="—" />
            ) : (
              configJsonFlat.map((entry) => (
                <KV key={entry.key} label={entry.key} value={entry.value} mono />
              ))
            )}
          </div>
        </Section>

        {job.resultJson && Object.keys(job.resultJson).length > 0 && (
          <Section title="Result Data" icon={Layers}>
            <pre className="overflow-x-auto rounded bg-[rgba(0,0,0,0.3)] p-3 font-mono text-[10px] leading-relaxed text-[var(--text-dim)]">
              {JSON.stringify(job.resultJson, null, 2)}
            </pre>
          </Section>
        )}
      </div>
    </div>
  )
}
