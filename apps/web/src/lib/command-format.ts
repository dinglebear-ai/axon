import type { ArtifactHandle, CommandResultView, PanelCommandResponse } from './panel-types';

export const commandExamples = [
  'scrape code.claude.com',
  'crawl code.claude.com',
  'ask How do I create claude code hooks?',
  'extract all the prices from https://example.com/products'
];

export function formatBytes(bytes: number): string {
  if (bytes >= 1_048_576) return `${(bytes / 1_048_576).toFixed(1)} MB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${bytes} B`;
}

export function asRecord(value: unknown): Record<string, unknown> | null {
  if (value && typeof value === 'object' && !Array.isArray(value)) return value as Record<string, unknown>;
  return null;
}

export function extractTerminalJob(result: Record<string, unknown> | null): Record<string, unknown> | null {
  if (!result) return null;
  const job = asRecord(result.job);
  if (!job || typeof job.status !== 'string') return null;
  if (!['completed', 'failed', 'canceled', 'cancelled'].includes(job.status)) return null;
  return job;
}

export function extractArtifactHandle(result: Record<string, unknown> | null): ArtifactHandle | null {
  if (!result) return null;
  const handle = asRecord(result.artifact_handle);
  if (!handle || typeof handle.relative_path !== 'string') return null;
  return {
    relative_path: handle.relative_path,
    bytes: typeof handle.bytes === 'number' ? handle.bytes : undefined,
    kind: typeof handle.kind === 'string' ? handle.kind : 'file',
    display_path: typeof handle.display_path === 'string' ? handle.display_path : handle.relative_path,
    line_count: typeof handle.line_count === 'number' ? handle.line_count : undefined
  };
}

export function extractArtifactHandles(result: Record<string, unknown> | null): ArtifactHandle[] {
  if (!result) return [];
  const candidates: Record<string, unknown>[] = [];
  const single = result.artifact_handle;
  if (single && typeof single === 'object' && !Array.isArray(single)) {
    candidates.push({ artifact_handle: single });
  }
  for (const key of ['predicted_artifact_handles', 'output_file_handles', 'artifact_handles']) {
    const arr = result[key];
    if (Array.isArray(arr)) {
      candidates.push(
        ...arr
          .filter((item) => item && typeof item === 'object' && !Array.isArray(item))
          .map((item) => ({ artifact_handle: item }))
      );
    }
  }
  return candidates
    .map((candidate) => extractArtifactHandle(candidate))
    .filter((handle): handle is ArtifactHandle => handle !== null);
}

const RASTER_IMAGE_KINDS = new Set(['screenshot', 'image', 'image/png', 'image/jpeg', 'image/jpg', 'image/webp', 'image/gif', 'image/avif']);
const RASTER_IMAGE_EXTENSIONS = new Set(['png', 'jpg', 'jpeg', 'webp', 'gif', 'avif']);
export const MAX_INLINE_ARTIFACT_BYTES = 8 * 1024 * 1024;

export function isPreviewableRasterArtifact(handle: ArtifactHandle): boolean {
  if (typeof handle.bytes === 'number' && handle.bytes > MAX_INLINE_ARTIFACT_BYTES) return false;
  const kind = handle.kind.toLowerCase();
  if (RASTER_IMAGE_KINDS.has(kind)) return true;
  if (kind.startsWith('image/')) return false;
  const leaf = handle.relative_path.split('/').pop() ?? '';
  const extension = leaf.includes('.') ? leaf.split('.').pop()?.toLowerCase() : undefined;
  return extension ? RASTER_IMAGE_EXTENSIONS.has(extension) : false;
}

export function panelArtifactUrl(relativePath: string): string {
  return `/api/panel/artifact/${relativePath.split('/').map(encodeArtifactSegment).join('/')}`;
}

function encodeArtifactSegment(segment: string): string {
  return encodeURIComponent(segment).replaceAll('.', '%2E');
}

export function arrayField(record: Record<string, unknown>, key: string): unknown[] {
  return Array.isArray(record[key]) ? record[key] : [];
}

export function stringArrayField(record: Record<string, unknown>, key: string): string[] {
  return arrayField(record, key).filter((item): item is string => typeof item === 'string');
}

export function firstStringField(record: Record<string, unknown> | null, keys: string[]): string | undefined {
  if (!record) return undefined;
  for (const key of keys) {
    const value = record[key];
    if (typeof value === 'string' && value.trim()) return value;
  }
  return undefined;
}

export function addStringRow(
  rows: Array<{ label: string; value: string }>,
  label: string,
  value: unknown,
  transform: (value: string) => string = (item) => item
) {
  if (typeof value === 'string' && value.trim()) rows.push({ label, value: transform(value) });
}

export function addNumberRow(rows: Array<{ label: string; value: string }>, label: string, value: unknown) {
  if (typeof value === 'number') rows.push({ label, value: value.toLocaleString() });
}

export function stringifyScalar(value: unknown): string {
  if (typeof value === 'number') return value.toLocaleString();
  if (typeof value === 'string') return value;
  if (typeof value === 'boolean') return value ? 'Yes' : 'No';
  return '';
}

export function compactRows(rows: Array<{ label: string; value: string }>): Array<{ label: string; value: string }> {
  const seen = new Set<string>();
  return rows.filter((row) => {
    if (!row.value) return false;
    const key = `${row.label}:${row.value}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

export function hasJobIds(result: Record<string, unknown> | null): boolean {
  return Boolean(result && stringArrayField(result, 'job_ids').length > 0);
}

export function titleLabel(value: string): string {
  return value
    .split(/[\s_-]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ');
}

export function compactOutputArtifact(path: string): string {
  const parts = path.split('/').filter(Boolean);
  if (parts.length >= 3) {
    const domain = parts[0];
    const leaf = parts.at(-1);
    return `artifact: ${domain}${leaf ? `/${leaf}` : ''}`;
  }
  return `artifact: domains/${path}`;
}

export function compactJobTarget(value: string): string {
  if (value.startsWith('/home/axon/.axon/output/domains/')) {
    return compactOutputArtifact(value.replace('/home/axon/.axon/output/domains/', ''));
  }
  if (value.startsWith('/home/axon/.axon/')) return value.replace('/home/axon/.axon/', '~/.axon/');
  if (value.startsWith('/home/jmagar/.axon/')) return value.replace('/home/jmagar/.axon/', '~/.axon/');

  try {
    const url = new URL(value);
    const path = url.pathname === '/' ? '' : url.pathname.replace(/\/$/, '');
    return `${url.hostname}${path}`;
  } catch {
    return value;
  }
}

export function commandVerb(command: string): string {
  return command.trim().split(/\s+/, 1)[0]?.toLowerCase() || 'command';
}

export function commandResultTitle(action: string, result: Record<string, unknown> | null): string {
  const job = extractTerminalJob(result);
  if (job) {
    const status = String(job.status);
    if (status === 'completed') return `${titleLabel(action)} complete`;
    if (status === 'failed') return `${titleLabel(action)} failed`;
    return `${titleLabel(action)} ${status}`;
  }
  if (action === 'ask') return 'Answer ready';
  if (action === 'status') return 'Status loaded';
  if (action === 'crawl') return hasJobIds(result) ? 'Crawl queued' : 'Crawl complete';
  if (action === 'scrape') return 'Scrape complete';
  if (action === 'screenshot') return 'Screenshot captured';
  if (action === 'extract') return hasJobIds(result) ? 'Extract queued' : 'Extract complete';
  return `${titleLabel(action)} complete`;
}

export function commandResultSubtitle(action: string, result: Record<string, unknown> | null): string {
  const target = firstStringField(result, ['url', 'target', 'query', 'question', 'output_dir']);
  if (target) return compactJobTarget(target);
  if (action === 'status') return 'Current SQLite queue state';
  return 'Axon returned a successful response';
}

export function terminalJobRows(action: string, job: Record<string, unknown>): Array<{ label: string; value: string }> {
  const rows: Array<{ label: string; value: string }> = [];
  const resultJson = asRecord(job.result_json);

  if (job.status === 'failed' && typeof job.error_text === 'string' && job.error_text) {
    rows.push({ label: 'Error', value: job.error_text });
  }

  for (const key of ['pages_crawled', 'docs_embedded', 'chunks_embedded', 'md_created']) {
    const val = resultJson?.[key];
    if (typeof val === 'number' && val > 0) {
      rows.push({ label: titleLabel(key.replaceAll('_', ' ')), value: val.toLocaleString() });
    }
  }

  const elapsedMs = resultJson?.elapsed_ms;
  if (typeof elapsedMs === 'number' && elapsedMs >= 1000) {
    rows.push({ label: 'Elapsed', value: `${(elapsedMs / 1000).toFixed(1)}s` });
  }

  const target = firstStringField(job, ['url', 'target']);
  if (target) {
    rows.push({ label: action === 'ingest' ? 'Source' : 'URL', value: compactJobTarget(target) });
  }

  return rows;
}

export function commandResultRows(action: string, result: Record<string, unknown> | null): Array<{ label: string; value: string }> {
  if (!result) return [];

  if (action === 'status') {
    const totals = asRecord(result.totals);
    return ['crawl', 'extract', 'embed', 'ingest']
      .map((key) => ({ label: titleLabel(key), value: stringifyScalar(totals?.[key]) }))
      .filter((row) => row.value);
  }

  // Terminal job result (from polling path) — show metrics, not raw job IDs
  const job = extractTerminalJob(result);
  if (job) return terminalJobRows(action, job);

  const rows: Array<{ label: string; value: string }> = [];
  const jobIds = stringArrayField(result, 'job_ids');
  const jobs = arrayField(result, 'jobs');
  const urls = stringArrayField(result, 'urls');
  const outputFiles = stringArrayField(result, 'output_files');
  const predictedPaths = stringArrayField(result, 'predicted_paths');
  const hasArtifact = Boolean(extractArtifactHandle(result));

  if (jobIds.length > 0) rows.push({ label: jobIds.length === 1 ? 'Job ID' : 'Jobs', value: jobIds.join(', ') });
  if (jobs.length > 0) rows.push({ label: 'Jobs', value: String(jobs.length) });
  if (urls.length > 0) rows.push({ label: urls.length === 1 ? 'URL' : 'URLs', value: urls.map(compactJobTarget).join(', ') });
  addStringRow(rows, 'Status', result.status);
  addStringRow(rows, 'Collection', result.collection);
  addStringRow(rows, 'Output', result.output_dir, compactJobTarget);
  addStringRow(rows, 'File', result.output_file, compactJobTarget);
  addNumberRow(rows, 'Pages', result.pages);
  addNumberRow(rows, 'Chunks', result.chunks);
  addNumberRow(rows, 'Count', result.count);

  if (outputFiles.length > 0) rows.push({ label: 'Files', value: outputFiles.map(compactJobTarget).slice(0, 3).join(', ') });
  // Only show predicted paths when there are no real output files and no rendered artifact
  if (predictedPaths.length > 0 && outputFiles.length === 0 && !hasArtifact) {
    rows.push({ label: 'Predicted files', value: predictedPaths.map(compactJobTarget).slice(0, 3).join(', ') });
  }

  return rows;
}

export function commandResultBody(action: string, result: Record<string, unknown> | null): string | undefined {
  if (!result) return undefined;
  if (action === 'ask') return firstStringField(result, ['answer', 'response', 'text', 'summary']);
  if (action === 'status') return firstStringField(result, ['text']);
  if (action === 'screenshot') {
    const handle = extractArtifactHandle(result);
    if (handle?.bytes) return formatBytes(handle.bytes);
  }
  return firstStringField(result, ['message', 'summary', 'detail']);
}

export function shouldShowRawResult(action: string, result: Record<string, unknown> | null): boolean {
  if (!result) return true;
  if (action === 'status' || action === 'ask') return false;
  if (extractTerminalJob(result)) return false;
  if (extractArtifactHandle(result)) return false;
  return commandResultRows(action, result).length <= 2 && !commandResultBody(action, result);
}

export function formatCommandResponse(response: PanelCommandResponse): CommandResultView {
  const action = commandVerb(response.command);
  const result = asRecord(response.result);
  const rows: Array<{ label: string; value: string }> = [
    { label: 'Command', value: response.command },
    { label: 'Action', value: titleLabel(action) }
  ];

  const title = commandResultTitle(action, result);
  const subtitle = commandResultSubtitle(action, result);
  rows.push(...commandResultRows(action, result));
  const artifacts = extractArtifactHandles(result);

  const handle = extractArtifactHandle(result);
  const imageArtifact = handle && isPreviewableRasterArtifact(handle) ? handle : undefined;
  const imageUrl = imageArtifact ? panelArtifactUrl(imageArtifact.relative_path) : undefined;

  return {
    ok: true,
    title,
    subtitle,
    rows: compactRows(rows),
    body: commandResultBody(action, result),
    artifacts: artifacts.length > 0 ? artifacts : undefined,
    raw: shouldShowRawResult(action, result) ? JSON.stringify(response.result, null, 2) : undefined,
    imageUrl,
    imageArtifact
  };
}
