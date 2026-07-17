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
import { useEffect, useRef, useState } from 'react';
import type { LucideIcon } from 'lucide-react';
import type {
  ArtifactHandle,
  CheckSummary,
  CommandResultView,
  DoctorService,
  StackCheck,
  StackUrlCheck
} from './panel-types';
import {
  MAX_INLINE_ARTIFACT_BYTES,
  compactJobTarget,
  formatBytes,
  isPreviewableRasterArtifact,
  panelArtifactUrl,
  titleLabel
} from './command-format';
import { normalizeJobStatus, jobTargetFromUrls, jobKindLabel } from '../features/jobs/job-helpers';
import type { ServiceJob, SourceListEntry } from './panel-types';
import type { SourceResult } from '../api/axon-client';
import {
  sourceEntryAdapterName,
  sourceEntryChunkCount,
  sourceEntryFamily,
  sourceEntryLabel
} from '../features/sources/source-helpers';

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

const PREVIEWABLE_RASTER_TYPES = new Set(['image/png', 'image/jpeg', 'image/gif', 'image/webp', 'image/avif']);
const MAX_OPEN_ARTIFACT_BYTES = 64 * 1024 * 1024;

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

type CommandResultCardProps = {
  result: CommandResultView;
  panelToken: string;
};

export function CommandResultCard({ result, panelToken }: CommandResultCardProps) {
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
      {result.imageUrl && result.imageArtifact && (
        <AuthenticatedPanelArtifactImage
          alt={result.imageArtifact.artifact_id}
          panelToken={panelToken}
          src={result.imageUrl}
        />
      )}
      {result.body && <p className="palette-result-body">{result.body}</p>}
      {result.artifacts && result.artifacts.length > 0 && (
        <div className="artifact-list">
          <p className="artifact-list-label">Artifacts</p>
          {result.artifacts.map((a) => (
            <ArtifactRow key={a.artifact_id} artifact={a} panelToken={panelToken} />
          ))}
        </div>
      )}
      {result.raw && <pre className="palette-result-raw">{result.raw}</pre>}
    </section>
  );
}

function AuthenticatedPanelArtifactImage({
  alt,
  panelToken,
  src
}: {
  alt: string;
  panelToken: string;
  src: string;
}) {
  const [objectUrl, setObjectUrl] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Single source of truth for the live blob URL so the effect cleanup and the
  // <img> onError handler revoke it exactly once (no double-revoke).
  const activeUrlRef = useRef<string | null>(null);

  function revokeActiveUrl() {
    if (activeUrlRef.current) {
      URL.revokeObjectURL(activeUrlRef.current);
      activeUrlRef.current = null;
    }
  }

  useEffect(() => {
    let cancelled = false;

    setObjectUrl(null);
    setError(null);

    async function loadArtifactImage() {
      try {
        const response = await fetch(src, { headers: { 'x-axon-panel-token': panelToken } });
        if (!response.ok) throw new Error(`artifact fetch failed with ${response.status}`);
        const blob = await previewableRasterBlob(response);
        const blobUrl = URL.createObjectURL(blob);
        if (cancelled) {
          URL.revokeObjectURL(blobUrl);
          return;
        }
        activeUrlRef.current = blobUrl;
        setObjectUrl(blobUrl);
      } catch (err) {
        if (!cancelled) setError(errorMessage(err));
      }
    }

    void loadArtifactImage();

    return () => {
      cancelled = true;
      revokeActiveUrl();
    };
  }, [panelToken, src]);

  if (error) return <p className="palette-result-body">Preview unavailable: {error}</p>;
  if (!objectUrl) return null;
  return (
    <img
      className="palette-result-image"
      src={objectUrl}
      alt={alt}
      onError={() => {
        // The img is being replaced by the error text, so revoke its blob now
        // instead of waiting for the next effect run / unmount.
        revokeActiveUrl();
        setObjectUrl(null);
        setError('image decode failed');
      }}
    />
  );
}

type ArtifactRowProps = {
  artifact: ArtifactHandle;
  panelToken: string;
};

export function ArtifactRow({ artifact, panelToken }: ArtifactRowProps) {
  const isImage = isPreviewableRasterArtifact(artifact);
  const src = panelArtifactUrl(artifact.artifact_id);
  const name = artifact.artifact_id;
  const meta = formatBytes(artifact.bytes ?? 0) + (artifact.line_count ? ` · ${artifact.line_count.toLocaleString()} lines` : '');
  const actionLabel = isImage ? '↗' : '↓';
  const [error, setError] = useState<string | null>(null);

  return (
    <>
      <div className={`artifact-row${isImage ? ' artifact-row-image' : ''}`}>
        <span className="artifact-kind">{artifact.artifact_kind}</span>
        <span className="artifact-name" title={artifact.artifact_id}>{name}</span>
        <span className="artifact-meta">{meta}</span>
        <button
          type="button"
          onClick={() => {
            setError(null);
            void openPanelArtifact(src, panelToken, name).catch((err) => {
              setError(`Could not open ${name}: ${errorMessage(err)}`);
            });
          }}
          className="artifact-download"
        >
          {actionLabel}
        </button>
      </div>
      {error && <p className="artifact-error">{error}</p>}
    </>
  );
}

async function openPanelArtifact(src: string, panelToken: string, filename: string) {
  const response = await fetch(src, { headers: { 'x-axon-panel-token': panelToken } });
  if (!response.ok) throw new Error(`artifact fetch failed with ${response.status}`);
  const blob = await cappedResponseBlob(
    response,
    MAX_OPEN_ARTIFACT_BYTES,
    'artifact is too large to open in the panel'
  );
  const objectUrl = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = objectUrl;
  link.download = filename;
  link.target = '_blank';
  document.body.appendChild(link);
  link.click();
  link.remove();
  window.setTimeout(() => URL.revokeObjectURL(objectUrl), 30_000);
}

async function previewableRasterBlob(response: Response): Promise<Blob> {
  const type = response.headers.get('content-type')?.split(';')[0]?.trim().toLowerCase() ?? '';
  if (!PREVIEWABLE_RASTER_TYPES.has(type)) {
    throw new Error(`artifact is ${type || 'unknown type'}, not a previewable image`);
  }

  return cappedResponseBlob(response, MAX_INLINE_ARTIFACT_BYTES, 'artifact is too large to preview');
}

async function cappedResponseBlob(response: Response, maxBytes: number, tooLargeMessage: string): Promise<Blob> {
  const length = Number(response.headers.get('content-length') ?? 0);
  if (Number.isFinite(length) && length > maxBytes) {
    throw new Error(tooLargeMessage);
  }

  const contentType = response.headers.get('content-type') ?? 'application/octet-stream';
  if (response.body) {
    const reader = response.body.getReader();
    const chunks: BlobPart[] = [];
    let total = 0;
    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        total += value.byteLength;
        if (total > maxBytes) {
          throw new Error(tooLargeMessage);
        }
        const chunk = new Uint8Array(value.byteLength);
        chunk.set(value);
        chunks.push(chunk.buffer);
      }
    } finally {
      reader.releaseLock();
    }
    return new Blob(chunks, { type: contentType });
  }

  const blob = await response.blob();
  if (blob.size > maxBytes) {
    throw new Error(tooLargeMessage);
  }
  return blob;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
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

export function SourceListRow({ entry }: { entry: SourceListEntry }) {
  const label = sourceEntryLabel(entry);

  return (
    <div className={`job-row ${entry.status ?? ''}`}>
      <div className="job-row-main">
        <strong title={label}>{label}</strong>
        <small className="job-row-meta">
          <span>{sourceEntryFamily(entry)}</span>
          <span>{sourceEntryAdapterName(entry)}</span>
          <span>{sourceEntryChunkCount(entry)} chunks</span>
        </small>
      </div>
      {entry.status && <StatusBadge status={normalizeJobStatus(entry.status)} />}
    </div>
  );
}

export function SourceSubmitResultCard({ result }: { result: SourceResult }) {
  const status = normalizeJobStatus(result.status ?? '');
  const counts = result.counts;

  return (
    <div className={`check-card ${status}`}>
      <span>
        <StatusGlyph status={status} />
        {result.canonical_uri}
      </span>
      <StatusBadge status={status} />
      <p>
        source {result.source_id} · job {result.job_id} · kind {result.source_kind}
      </p>
      {counts && (
        <p>
          {counts.items_total} items · {counts.documents_total} docs · {counts.chunks_total} chunks ·{' '}
          {counts.vector_points_total} points
        </p>
      )}
      {result.warnings?.map((warning, index) => (
        <p className="error" key={`${warning.code}-${index}`}>
          {warning.severity}: {warning.message}
        </p>
      ))}
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
