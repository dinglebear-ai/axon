import { AxonApiError } from '../../api/axon-client';
import type { SourceListEntry, SourcesListResult } from '../../lib/panel-types';

// Options for the `adapter` field of `SourceRequest`. `adapter` is a free
// string resolved server-side by the source resolver — there is no enum in
// the OpenAPI schema — so this mirrors `SourceKind`
// (crates/axon-api/src/source/enums.rs) as the closest concept to a
// selectable "family". "auto" omits the field so the resolver picks it.
export const SOURCE_FAMILY_OPTIONS = [
  { value: 'auto', label: 'Auto-detect' },
  { value: 'web', label: 'Web' },
  { value: 'local', label: 'Local path' },
  { value: 'git', label: 'Git repo' },
  { value: 'registry', label: 'Package registry' },
  { value: 'feed', label: 'RSS/Atom feed' },
  { value: 'reddit', label: 'Reddit' },
  { value: 'youtube', label: 'YouTube' },
  { value: 'session', label: 'AI session' }
] as const;

export type MaxPagesParseResult =
  | { ok: true; value: number | null }
  | { ok: false; message: string };

export function parseMaxPages(value: string): MaxPagesParseResult {
  const trimmed = value.trim();
  if (!trimmed) return { ok: true, value: null };
  const parsed = Number(trimmed);
  if (!Number.isSafeInteger(parsed) || parsed < 0) {
    return { ok: false, message: 'Max pages must be a non-negative safe integer.' };
  }
  return { ok: true, value: parsed };
}

export function normalizeSourceEntries(result: SourcesListResult | null): SourceListEntry[] {
  if (!result) return [];
  return result.urls.flatMap((entry): SourceListEntry[] => {
    if (typeof entry === 'string') return [{ url: entry }];
    const [url, chunks] = entry;
    return typeof url === 'string' && typeof chunks === 'number' ? [{ url, chunks }] : [];
  });
}

export function sourceEntryKey(entry: SourceListEntry, index: number): string {
  return entry.canonical_uri ?? entry.url ?? String(index);
}

export function sourceEntryLabel(entry: SourceListEntry): string {
  return entry.canonical_uri ?? entry.url ?? 'Unknown source';
}

export function sourceEntryFamily(entry: SourceListEntry): string {
  return entry.source_kind ?? '—';
}

export function sourceEntryAdapterName(entry: SourceListEntry): string {
  if (!entry.adapter) return '—';
  return typeof entry.adapter === 'string' ? entry.adapter : (entry.adapter.name ?? '—');
}

export function sourceEntryChunkCount(entry: SourceListEntry): number {
  return entry.counts?.chunks_total ?? entry.chunks ?? 0;
}

export function sourcesSummaryLabel(result: SourcesListResult | null): string {
  if (!result) return 'No data loaded';
  const entries = normalizeSourceEntries(result);
  return `${entries.length} shown of ${result.count}`;
}

export function sourceErrorMessage(error: unknown): string {
  if (error instanceof AxonApiError) {
    if (error.status === 401 || error.status === 403) {
      return 'Requires an Axon API bearer token or OAuth (see the server env settings) — the panel session token does not grant /v1 access.';
    }
    return error.message;
  }
  return error instanceof Error ? error.message : String(error);
}
