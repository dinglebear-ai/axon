import { AxonApiError } from '../../api/axon-client';
import type { WatchPage, WatchSchedule, WatchSummary } from '../../lib/panel-types';

export function normalizeWatchEntries(result: WatchPage | null): WatchSummary[] {
  if (!result) return [];
  return result.items ?? [];
}

export function watchEntryKey(entry: WatchSummary, index: number): string {
  return entry.watch_id || String(index);
}

export function watchScheduleLabel(schedule: WatchSchedule | null | undefined): string {
  if (!schedule) return 'No schedule';
  if (schedule.cron) return `cron ${schedule.cron}`;
  return `every ${formatDuration(schedule.every_seconds)}`;
}

export function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return `${seconds}s`;
  if (seconds % 86400 === 0) return `${seconds / 86400}d`;
  if (seconds % 3600 === 0) return `${seconds / 3600}h`;
  if (seconds % 60 === 0) return `${seconds / 60}m`;
  return `${seconds}s`;
}

export function watchStatusLabel(entry: WatchSummary): string {
  if (!entry.enabled) return 'paused';
  return entry.last_status ?? 'scheduled';
}

export function watchSummaryLabel(result: WatchPage | null): string {
  if (!result) return 'No data loaded';
  const entries = normalizeWatchEntries(result);
  const total = result.total ?? entries.length;
  return `${entries.length} shown of ${total}`;
}

export function watchErrorMessage(error: unknown): string {
  if (error instanceof AxonApiError) {
    if (error.status === 401 || error.status === 403) {
      return 'Requires an Axon API bearer token or OAuth (see the server env settings) — the panel session token does not grant /v1 access.';
    }
    return error.message;
  }
  return error instanceof Error ? error.message : String(error);
}

export function parseScheduleSeconds(value: string): number | null {
  const trimmed = value.trim();
  if (!trimmed) return null;
  const parsed = Number(trimmed);
  if (!Number.isFinite(parsed) || parsed <= 0) return null;
  return Math.floor(parsed);
}
