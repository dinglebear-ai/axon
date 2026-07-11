import { AxonApiError } from '../../api/axon-client';
import type { MemoryItem } from '../../lib/panel-types';

export function memoryEntryKey(entry: MemoryItem, index: number): string {
  return entry.id || String(index);
}

export function memoryScopeLabel(entry: MemoryItem): string {
  const parts = [entry.file, entry.repo, entry.project].filter((value): value is string => Boolean(value));
  return parts.length ? parts[0] : 'global';
}

export function memoryResultsSummaryLabel(entries: MemoryItem[] | null, query: string): string {
  if (!entries) return 'No search run yet';
  if (!query.trim()) return `${entries.length} shown`;
  return `${entries.length} match${entries.length === 1 ? '' : 'es'} for "${query.trim()}"`;
}

export function memoryTimestampLabel(epochMs: number): string {
  if (!Number.isFinite(epochMs) || epochMs <= 0) return 'unknown';
  return new Date(epochMs).toLocaleString();
}

export function memoryErrorMessage(error: unknown): string {
  if (error instanceof AxonApiError) {
    if (error.status === 401 || error.status === 403) {
      return 'Requires an Axon API token (AXON_HTTP_TOKEN or OAuth) — the panel session token does not grant /v1 access.';
    }
    return error.message;
  }
  return error instanceof Error ? error.message : String(error);
}

export function parseConfidence(value: string): number | null {
  const trimmed = value.trim();
  if (!trimmed) return null;
  const parsed = Number(trimmed);
  if (!Number.isFinite(parsed) || parsed < 0 || parsed > 1) return null;
  return parsed;
}
