'use client';

import {
  Ban,
  Bot,
  CheckCircle2,
  Cpu,
  Database,
  ExternalLink,
  FileCog,
  Globe2,
  HelpCircle,
  Server,
  ShieldCheck,
  TriangleAlert,
  XCircle
} from 'lucide-react';
import type { LucideIcon } from 'lucide-react';
import type {
  ArtifactHandle,
  CheckSummary,
  CommandResultView,
  DoctorService,
  StackCheck,
  StackUrlCheck
} from './panel-types';
import { compactJobTarget, formatBytes, titleLabel } from './command-format';
import { normalizeJobStatus, jobTargetFromUrls, jobKindLabel } from './job-helpers';
import type { ServiceJob } from './panel-types';

// ---------------------------------------------------------------------------
// Icon maps (used by UrlCard and CheckCard)
// ---------------------------------------------------------------------------

const checkIcons: Record<string, LucideIcon> = {
  Chrome: Globe2,
  'Compose assets': FileCog,
  Docker: Server,
  'Docker Compose': Server,
  'Gemini CLI': Bot,
  'MCP/API token': ShieldCheck,
  'NVIDIA runtime': Cpu,
  'OAuth / lab-auth': ShieldCheck,
  Qdrant: Database,
  'TEI / Qwen3': Cpu
};

const urlIcons: Record<string, LucideIcon> = {
  'Chrome control': Globe2,
  'MCP endpoint': ShieldCheck,
  'Panel / readyz': Server,
  'Public URL': Globe2,
  'Qdrant readyz': Database,
  'TEI health': Cpu
};

// ---------------------------------------------------------------------------
// Pure check/status utility functions
// ---------------------------------------------------------------------------

export function summarizeChecks(checks: StackCheck[]): CheckSummary {
  return checks.reduce(
    (summary, check) => {
      if (check.status === 'ok') summary.ok += 1;
      else if (check.status === 'warn') summary.warn += 1;
      else if (check.status === 'error') summary.error += 1;
      else if (check.status === 'skipped') summary.skipped += 1;

      summary.total += 1;
      return summary;
    },
    { ok: 0, warn: 0, error: 0, skipped: 0, total: 0 }
  );
}

export function mergeStatus(summaries: CheckSummary[]): string {
  if (summaries.some((summary) => summary.error > 0)) return 'error';
  if (summaries.some((summary) => summary.warn > 0)) return 'warn';
  if (summaries.some((summary) => summary.ok > 0)) return 'ok';
  return 'skipped';
}

export function overallStatusLabel(status: string): string {
  if (status === 'ok') return 'Operational';
  if (status === 'warn') return 'Needs attention';
  if (status === 'error') return 'Degraded';
  return 'Pending checks';
}

export function statusIcon(status: string): LucideIcon {
  if (status === 'ok') return CheckCircle2;
  if (status === 'warn') return TriangleAlert;
  if (status === 'error') return XCircle;
  if (status === 'skipped') return Ban;
  if (status === 'oauth') return ShieldCheck;
  if (status === 'agent') return Bot;
  return HelpCircle;
}

export function statusLabel(status: string): string {
  if (status === 'ok') return 'Online';
  if (status === 'warn') return 'Degraded';
  if (status === 'error') return 'Offline';
  if (status === 'skipped') return 'Skipped';
  return status;
}

export function describeEndpoint(url: string): { protocol: string; host: string; path: string } {
  if (!url) return { protocol: 'unset', host: 'Not configured', path: '' };

  try {
    const parsed = new URL(url);
    const path = `${parsed.pathname}${parsed.search}` || '/';
    return {
      protocol: parsed.protocol.replace(':', '').toUpperCase(),
      host: parsed.host,
      path
    };
  } catch {
    return { protocol: 'custom', host: url, path: '' };
  }
}

export function compactReachabilityDetail(detail: string): string {
  return detail.replace(/^reachable;\s*/i, '');
}

export function summarizeConfig(raw: string): { lines: number; characters: number } {
  return {
    lines: raw ? raw.split(/\r\n|\r|\n/).length : 0,
    characters: raw.length
  };
}

// ---------------------------------------------------------------------------
// React components
// ---------------------------------------------------------------------------

export function SummaryPill({ label, summary }: { label: string; summary: CheckSummary }) {
  const status = summary.error > 0 ? 'error' : summary.warn > 0 ? 'warn' : summary.ok > 0 ? 'ok' : 'skipped';
  const Icon = statusIcon(status);
  const parts = [
    `${summary.ok} ok`,
    summary.warn ? `${summary.warn} warn` : '',
    summary.error ? `${summary.error} error` : '',
    summary.skipped ? `${summary.skipped} skipped` : ''
  ].filter(Boolean);

  return (
    <div className={`summary-pill ${status}`}>
      <Icon aria-hidden="true" className="status-icon" />
      <span>{label}</span>
      <strong>{summary.total ? parts.join(' · ') : 'pending'}</strong>
    </div>
  );
}

export function SubsectionTitle({ icon: Icon, title, note }: { icon: LucideIcon; title: string; note: string }) {
  return (
    <div className="subsection-heading">
      <h3>
        <Icon aria-hidden="true" className="heading-icon" />
        {title}
      </h3>
      <p>{note}</p>
    </div>
  );
}

export function UrlCard({ check }: { check: StackUrlCheck }) {
  const endpoint = describeEndpoint(check.url);
  const Icon = urlIcons[check.label] ?? Globe2;

  return (
    <div className={`url-card ${check.status}`}>
      <div className="url-service">
        <span>
          <Icon aria-hidden="true" className="card-icon" />
          {check.label}
        </span>
        <small>{endpoint.protocol}</small>
      </div>
      <div className="url-target">
        <strong>{check.url ? endpoint.host : 'Not configured'}</strong>
        {check.url && <code>{endpoint.path}</code>}
      </div>
      <div className="url-state">
        <StatusBadge status={check.status} />
        <p>
          {compactReachabilityDetail(check.detail)}
          {check.url && <ExternalLink aria-hidden="true" className="inline-icon" />}
        </p>
      </div>
    </div>
  );
}

export function CheckCard({ check }: { check: StackCheck }) {
  const Icon = checkIcons[check.label] ?? statusIcon(check.status);

  return (
    <div className={`check-card ${check.status}`}>
      <span>
        <Icon aria-hidden="true" className="card-icon" />
        {check.label}
      </span>
      <StatusBadge status={check.status} />
      <p>{check.detail}</p>
    </div>
  );
}

export function StatusBadge({ status }: { status: string }) {
  const Icon = statusIcon(status);

  return (
    <strong className={`status-badge ${status}`}>
      <Icon aria-hidden="true" className="status-icon" />
      {statusLabel(status)}
    </strong>
  );
}

export function StatusGlyph({ status }: { status: string }) {
  const Icon = statusIcon(status);
  return <Icon aria-hidden="true" className="status-glyph" />;
}

export function EmptyState({ loading, text }: { loading: boolean; text: string }) {
  return <p className="empty-state">{loading ? 'Checking...' : text}</p>;
}

export function CommandResultCard({ result }: { result: CommandResultView }) {
  return (
    <section className={`palette-result ${result.ok ? 'ok' : 'error'}`} aria-live="polite">
      <div className="palette-result-heading">
        <div>
          <p className="eyebrow">{result.ok ? 'Command complete' : 'Command error'}</p>
          <h3>{result.title}</h3>
          <span>{result.subtitle}</span>
        </div>
        <StatusBadge status={result.ok ? 'ok' : 'error'} />
      </div>
      {result.rows.length > 0 && (
        <dl className="palette-result-grid">
          {result.rows.map((row) => (
            <div key={`${row.label}-${row.value}`}>
              <dt>{row.label}</dt>
              <dd>{row.value}</dd>
            </div>
          ))}
        </dl>
      )}
      {result.imageUrl && (
        <img className="palette-result-image" src={result.imageUrl} alt="Screenshot" />
      )}
      {result.body && <p className="palette-result-body">{result.body}</p>}
      {result.artifacts && result.artifacts.length > 0 && (
        <div className="artifact-list">
          <p className="artifact-list-label">Artifacts</p>
          {result.artifacts.map((a) => (
            <ArtifactRow key={a.relative_path} artifact={a} />
          ))}
        </div>
      )}
      {result.raw && <pre className="palette-result-raw">{result.raw}</pre>}
    </section>
  );
}

export function ArtifactRow({ artifact }: { artifact: ArtifactHandle }) {
  const isScreenshot = artifact.kind === 'screenshot';
  const isImage = isScreenshot || artifact.kind.startsWith('image');
  const src = `/api/panel/artifact/${artifact.relative_path}`;
  const name = artifact.display_path.split('/').pop() ?? artifact.display_path;
  const meta = formatBytes(artifact.bytes ?? 0) + (artifact.line_count ? ` · ${artifact.line_count.toLocaleString()} lines` : '');

  if (isImage) {
    return (
      <div className="artifact-row artifact-row-image">
        <span className="artifact-kind">{artifact.kind}</span>
        <a href={src} target="_blank" rel="noreferrer" className="artifact-preview-link">
          <img src={src} alt={artifact.display_path} className="artifact-preview" />
        </a>
        <span className="artifact-name" title={artifact.display_path}>{name}</span>
        <span className="artifact-meta">{meta}</span>
      </div>
    );
  }

  return (
    <div className="artifact-row">
      <span className="artifact-kind">{artifact.kind}</span>
      <span className="artifact-name" title={artifact.display_path}>{name}</span>
      <span className="artifact-meta">{meta}</span>
      <a href={src} target="_blank" rel="noreferrer" download={name} className="artifact-download">↓</a>
    </div>
  );
}

export function DoctorCard({ service }: { service: DoctorService & { name: string } }) {
  const status = service.ok === false ? 'error' : 'ok';
  const detail = service.detail ?? service.model ?? service.vector_mode ?? service.command ?? service.path ?? 'ready';
  const target = service.url ?? service.collection ?? service.path ?? service.command ?? '';

  return (
    <div className={`doctor-card ${status}`}>
      <span>
        <StatusGlyph status={status} />
        {titleLabel(service.name)}
      </span>
      <StatusBadge status={status} />
      {target && <strong>{target}</strong>}
      <p>{detail}</p>
    </div>
  );
}

export function JobRow({ job }: { job: ServiceJob }) {
  const rawTarget = job.url ?? job.target ?? jobTargetFromUrls(job.urls_json) ?? job.id;
  const target = compactJobTarget(rawTarget);
  const updatedAt = new Date(job.updated_at).toLocaleTimeString();

  return (
    <div className={`job-row ${job.status}`}>
      <div className="job-row-main">
        <strong title={rawTarget}>{target}</strong>
        <small className="job-row-meta">
          <span>{jobKindLabel(job.kind)}</span>
          <span>{updatedAt}</span>
        </small>
      </div>
      <StatusBadge status={normalizeJobStatus(job.status)} />
    </div>
  );
}
