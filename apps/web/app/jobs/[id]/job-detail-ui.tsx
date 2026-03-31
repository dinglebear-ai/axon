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
import type { JobDetail } from '@/app/api/jobs/[id]/route'
import { Button } from '@/components/ui/button'
import { KV, Section, ShowMoreList, Stat, StatusBadge, TypeBadge } from './job-detail-components'
import {
  flattenJsonEntries,
  fmtDate,
  fmtDuration,
  getRefreshSummaryRows,
} from './job-detail-helpers'

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

        {job.type === 'crawl' && job.wafDiagnostics && (
          <Section title="WAF Recovery" icon={AlertCircle}>
            <div className="space-y-0">
              <KV label="Status" value={job.wafDiagnostics.status} mono />
              <KV
                label="Attempted Recovery"
                value={job.wafDiagnostics.attemptedRecovery ? 'yes' : 'no'}
              />
              <KV label="Detected Pages" value={job.wafDiagnostics.detectedPages} />
              <KV label="Recovered Pages" value={job.wafDiagnostics.recoveredPages} />
              <KV label="Remaining Pages" value={job.wafDiagnostics.remainingPages} />
            </div>
          </Section>
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
              <ShowMoreList
                title="WAF Remaining URLs"
                items={job.wafDiagnostics?.remainingUrls ?? []}
                emptyText="No unrecovered WAF URL list recorded."
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
