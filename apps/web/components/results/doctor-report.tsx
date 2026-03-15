'use client'

import { AlertTriangle, CheckCircle2 } from 'lucide-react'
import type React from 'react'
import { Badge } from '@/components/ui/badge'
import type { DoctorResult, DoctorServiceStatus } from '@/lib/result-types'
import { fmtMs as formatDurationMs } from './shared'

function fmtProbeMs(ms: number | undefined): string {
  if (ms === undefined) return '--'
  return formatDurationMs(ms)
}

function cleanServiceMeta(text: string | undefined): string | undefined {
  if (!text) return undefined
  const cleaned = text
    .replace(/\bhttp\s+\d{3}\b/gi, '')
    .replace(/\bstatus\s*:\s*\d{3}\b/gi, '')
    .replace(/\s{2,}/g, ' ')
    .trim()
  return cleaned.length > 0 ? cleaned : undefined
}

function SectionHeader({ children }: { children: React.ReactNode }) {
  return <h3 className="ui-label mb-2 text-[var(--axon-primary-strong)]">{children}</h3>
}

function MetricTile({
  label,
  value,
  status,
  size = 'small',
}: {
  label: string
  value: string
  status: 'ok' | 'error' | 'info' | 'warn'
  size?: 'large' | 'small'
}) {
  const colorClass =
    status === 'error'
      ? 'text-[var(--axon-secondary)] border-[var(--border-accent)] bg-[rgba(255,135,175,0.08)]'
      : status === 'ok'
        ? 'text-[var(--axon-success)] border-[rgba(130,217,160,0.28)] bg-[var(--axon-success-bg)]'
        : status === 'warn'
          ? 'text-[var(--axon-warning)] border-[rgba(255,192,134,0.28)] bg-[var(--axon-warning-bg)]'
          : 'text-[var(--axon-primary)] border-[var(--border-subtle)] bg-[var(--surface-elevated)]'

  return (
    <div className={`rounded-lg border px-3 py-2.5 animate-fade-in-up ${colorClass}`}>
      <div className="ui-label text-[var(--text-muted)]">{label}</div>
      <div
        className={`mt-1 ui-mono font-semibold leading-none ${size === 'large' ? 'text-[22px]' : 'text-[16px]'}`}
      >
        {value}
      </div>
    </div>
  )
}

function StatusPill({ ok }: { ok: boolean }) {
  return (
    <Badge
      variant="outline"
      className={`gap-1 rounded-full border-transparent text-[10px] ${
        ok
          ? 'bg-[var(--axon-success-bg)] text-[var(--axon-success)]'
          : 'bg-[rgba(255,135,175,0.2)] text-[var(--axon-secondary)]'
      }`}
    >
      <span className="text-[length:var(--text-2xs)]">{'\u25CF'}</span>
      {ok ? 'ok' : 'fail'}
    </Badge>
  )
}

function ServiceRows({ entries }: { entries: Array<[string, DoctorServiceStatus]> }) {
  return (
    <div className="rounded-lg border border-[var(--border-subtle)] bg-[var(--surface-elevated)]">
      <div className="grid grid-cols-[110px_130px_1fr] gap-3 border-b border-[var(--border-subtle)] px-3 py-2 ui-label">
        <span>Status</span>
        <span>Latency</span>
        <span>Service</span>
      </div>
      <div className="max-h-[48vh] overflow-auto">
        {entries.map(([name, svc]) => {
          const detail = cleanServiceMeta(svc.detail)
          const summary = cleanServiceMeta(svc.summary)
          return (
            <div
              key={name}
              className="border-b border-[var(--border-subtle)] px-3 py-2.5 last:border-b-0"
            >
              <div className="grid grid-cols-[110px_130px_1fr] gap-3">
                <div className="pt-0.5">
                  <StatusPill ok={svc.ok} />
                </div>
                <div className="pt-1 ui-mono text-[var(--text-secondary)]">
                  {fmtProbeMs(svc.latency_ms)}
                </div>
                <div>
                  <div className="text-[length:var(--text-sm)] font-semibold text-[var(--text-secondary)]">
                    {name}
                  </div>
                  {svc.model && (
                    <div className="mt-0.5 break-words ui-meta text-[var(--axon-primary)]">
                      {svc.model}
                    </div>
                  )}
                  {svc.url && (
                    <div className="mt-0.5 break-all ui-mono text-[var(--axon-primary)]">
                      {svc.url}
                    </div>
                  )}
                  {detail && (
                    <div className="mt-1 whitespace-pre-wrap break-words ui-meta ui-dim-contrast">
                      {detail}
                    </div>
                  )}
                  {summary && (
                    <div className="mt-1 whitespace-pre-wrap break-words ui-meta ui-dim-contrast">
                      {summary}
                    </div>
                  )}
                </div>
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}

export interface DoctorReportProps {
  data: DoctorResult
}

export function DoctorReport({ data }: DoctorReportProps) {
  const allEntries = Object.entries(data.services)
  const failedEntries = allEntries.filter(([, s]) => !s.ok)
  const healthyEntries = allEntries.filter(([, s]) => s.ok)

  const pipelineEntries = Object.entries(data.pipelines ?? {})
  const queueEntries = Object.entries(data.queue_names ?? {})

  const healthyPipelines = pipelineEntries.filter(([, ok]) => ok).length
  const unhealthyPipelines = pipelineEntries.length - healthyPipelines
  const observedAt = data.observed_at_utc ? new Date(data.observed_at_utc).toLocaleString() : null
  const timingRows = [
    ['crawl report', data.timing_ms?.crawl_report],
    ['extract report', data.timing_ms?.extract_report],
    ['embed report', data.timing_ms?.embed_report],
    ['ingest report', data.timing_ms?.ingest_report],
    ['stale/pending query', data.timing_ms?.stale_pending],
  ] as const

  return (
    <div className="space-y-4">
      <div
        className="rounded-xl border px-3 py-3"
        style={{
          borderColor: data.all_ok ? 'rgba(135,215,135,0.24)' : 'rgba(255,135,175,0.34)',
          background:
            'linear-gradient(120deg, rgba(14,24,44,0.74) 0%, rgba(11,20,38,0.92) 52%, rgba(17,26,48,0.74) 100%)',
        }}
      >
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
          <span
            className={`inline-block size-2.5 rounded-full ${
              data.all_ok
                ? 'bg-[var(--axon-success)] shadow-[0_0_8px_rgba(130,217,160,0.65)]'
                : 'bg-[var(--axon-secondary)] shadow-[0_0_8px_rgba(255,135,175,0.65)]'
            }`}
          />
          <span className="text-[length:var(--text-sm)] font-semibold text-[var(--text-secondary)]">
            {data.all_ok ? 'System Health: Stable' : 'System Health: Attention Needed'}
          </span>
          {observedAt && <span className="ui-meta">Observed {observedAt}</span>}
        </div>
      </div>

      {/* Asymmetric metric grid */}
      <div className="grid gap-4 md:grid-cols-3">
        <div className="md:col-span-2">
          <MetricTile
            label="System Status"
            value={failedEntries.length === 0 ? 'Healthy' : `${failedEntries.length} down`}
            status={failedEntries.length === 0 ? 'ok' : 'error'}
            size="large"
          />
        </div>
        <div className="flex flex-col gap-3">
          <MetricTile
            label="Services Up"
            value={`${healthyEntries.length}`}
            status="ok"
            size="small"
          />
          <MetricTile
            label="Services Down"
            value={`${failedEntries.length}`}
            status={failedEntries.length > 0 ? 'error' : 'ok'}
            size="small"
          />
        </div>
      </div>

      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        <MetricTile label="Pending Jobs" value={`${data.pending_jobs}`} status="info" />
        <MetricTile label="Stale Jobs" value={`${data.stale_jobs}`} status="warn" />
        <MetricTile label="Pipelines Up" value={`${healthyPipelines}`} status="ok" />
        <MetricTile
          label="Pipelines Down"
          value={`${unhealthyPipelines}`}
          status={unhealthyPipelines > 0 ? 'error' : 'ok'}
        />
      </div>

      <div className="grid gap-4 lg:grid-cols-[1.7fr_1fr]">
        <div>
          <SectionHeader>Services</SectionHeader>
          {/* Failure-first grouping */}
          <div className="space-y-3">
            {failedEntries.length > 0 && (
              <div>
                <div className="mb-2 flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-[var(--axon-secondary)]">
                  <AlertTriangle className="size-3" />
                  {failedEntries.length} {failedEntries.length === 1 ? 'service' : 'services'} down
                </div>
                <ServiceRows entries={failedEntries} />
              </div>
            )}
            {failedEntries.length > 0 && healthyEntries.length > 0 && (
              <div className="border-t border-[var(--border-subtle)] my-3" />
            )}
            {healthyEntries.length > 0 && (
              <div>
                <div className="mb-2 flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-[var(--axon-success)]">
                  <CheckCircle2 className="size-3" />
                  {healthyEntries.length} {healthyEntries.length === 1 ? 'service' : 'services'}{' '}
                  healthy
                </div>
                <ServiceRows entries={healthyEntries} />
              </div>
            )}
          </div>
        </div>

        <div className="space-y-4">
          {pipelineEntries.length > 0 && (
            <div className="rounded-lg border border-[var(--border-subtle)] p-3 bg-[var(--surface-elevated)]">
              <SectionHeader>Pipelines</SectionHeader>
              <div className="space-y-1.5">
                {pipelineEntries.map(([name, ok]) => (
                  <div
                    key={name}
                    className="flex items-center justify-between rounded-md border border-[var(--border-subtle)] bg-[var(--surface-base)] px-2 py-1.5 text-[length:var(--text-sm)]"
                  >
                    <span className="text-[var(--text-secondary)]">{name}</span>
                    <span
                      className={ok ? 'text-[var(--axon-success)]' : 'text-[var(--axon-secondary)]'}
                    >
                      {ok ? 'up' : 'down'}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          )}

          {queueEntries.length > 0 && (
            <div className="rounded-lg border border-[var(--border-subtle)] p-3 bg-[var(--surface-elevated)]">
              <SectionHeader>Queue Names</SectionHeader>
              <div className="space-y-1.5 ui-mono">
                {queueEntries.map(([key, val]) => (
                  <div
                    key={key}
                    className="grid grid-cols-[minmax(0,1fr)_minmax(0,1.5fr)] gap-3 rounded-md border border-[var(--border-subtle)] bg-[var(--surface-base)] px-2 py-1.5"
                  >
                    <span className="break-words text-[var(--text-dim)]">{key}</span>
                    <span className="break-all text-right text-[var(--axon-primary)]">{val}</span>
                  </div>
                ))}
              </div>
            </div>
          )}

          {data.browser_runtime && (
            <div className="rounded-lg border border-[var(--border-subtle)] p-3 bg-[var(--surface-elevated)]">
              <SectionHeader>Browser Runtime</SectionHeader>
              <div className="flex items-center justify-between text-[length:var(--text-sm)]">
                <span className="text-[var(--text-dim)]">Selection</span>
                <span className="ui-mono text-[var(--axon-primary)]">
                  {data.browser_runtime.selection}
                </span>
              </div>
            </div>
          )}

          {data.timing_ms && (
            <div className="rounded-lg border border-[var(--border-subtle)] p-3 bg-[var(--surface-elevated)]">
              <SectionHeader>Probe Timing</SectionHeader>
              <div className="space-y-1.5 ui-mono">
                {timingRows.map(([label, value]) => (
                  <div key={label} className="flex justify-between">
                    <span className="text-[var(--text-dim)]">{label}</span>
                    <span className="text-[var(--axon-primary)]">{fmtProbeMs(value)}</span>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
