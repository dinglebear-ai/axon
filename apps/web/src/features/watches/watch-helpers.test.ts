import { describe, expect, it } from 'vitest';

import {
  formatDuration,
  normalizeWatchEntries,
  parseScheduleSeconds,
  watchEntryKey,
  watchErrorMessage,
  watchScheduleLabel,
  watchStatusLabel,
  watchSummaryLabel
} from './watch-helpers';
import type { WatchPage, WatchSummary } from '../../lib/panel-types';
import { AxonApiError } from '../../api/axon-client';

function watchSummary(overrides: Partial<WatchSummary> = {}): WatchSummary {
  return {
    watch_id: 'watch-1',
    source_id: 'src-1',
    enabled: true,
    schedule: { every_seconds: 3600 },
    next_run_at: '2026-07-10T00:00:00Z',
    last_job_id: null,
    last_status: 'completed',
    ...overrides
  };
}

describe('normalizeWatchEntries', () => {
  it('returns an empty array for a null result', () => {
    expect(normalizeWatchEntries(null)).toEqual([]);
  });

  it('returns the items array from a page', () => {
    const page: WatchPage = { items: [watchSummary()], next_cursor: null, limit: 50 };
    expect(normalizeWatchEntries(page)).toHaveLength(1);
  });
});

describe('watchEntryKey', () => {
  it('prefers watch_id', () => {
    expect(watchEntryKey(watchSummary(), 3)).toBe('watch-1');
  });

  it('falls back to index when watch_id is empty', () => {
    expect(watchEntryKey(watchSummary({ watch_id: '' }), 3)).toBe('3');
  });
});

describe('formatDuration', () => {
  it('formats whole days/hours/minutes compactly', () => {
    expect(formatDuration(86400)).toBe('1d');
    expect(formatDuration(3600)).toBe('1h');
    expect(formatDuration(120)).toBe('2m');
    expect(formatDuration(45)).toBe('45s');
  });
});

describe('watchScheduleLabel', () => {
  it('prefers a cron expression when present', () => {
    expect(watchScheduleLabel({ every_seconds: 3600, cron: '0 * * * *' })).toBe('cron 0 * * * *');
  });

  it('falls back to an interval label', () => {
    expect(watchScheduleLabel({ every_seconds: 3600 })).toBe('every 1h');
  });

  it('handles a missing schedule', () => {
    expect(watchScheduleLabel(null)).toBe('No schedule');
  });
});

describe('watchStatusLabel', () => {
  it('reports paused for a disabled watch', () => {
    expect(watchStatusLabel(watchSummary({ enabled: false }))).toBe('paused');
  });

  it('reports the last run status for an enabled watch', () => {
    expect(watchStatusLabel(watchSummary({ last_status: 'failed' }))).toBe('failed');
  });

  it('falls back to scheduled when no run has happened yet', () => {
    expect(watchStatusLabel(watchSummary({ last_status: null }))).toBe('scheduled');
  });
});

describe('watchSummaryLabel', () => {
  it('reports no data when the result is null', () => {
    expect(watchSummaryLabel(null)).toBe('No data loaded');
  });

  it('reports shown vs total counts', () => {
    const page: WatchPage = { items: [watchSummary()], next_cursor: null, limit: 50, total: 5 };
    expect(watchSummaryLabel(page)).toBe('1 shown of 5');
  });
});

describe('watchErrorMessage', () => {
  it('explains 401/403 as missing an API token', () => {
    expect(watchErrorMessage(new AxonApiError(401, {}))).toContain('AXON_HTTP_TOKEN');
  });

  it('passes through other AxonApiError messages', () => {
    expect(watchErrorMessage(new AxonApiError(500, { message: 'boom' }))).toBe('boom');
  });

  it('stringifies plain errors', () => {
    expect(watchErrorMessage(new Error('oops'))).toBe('oops');
    expect(watchErrorMessage('raw')).toBe('raw');
  });
});

describe('parseScheduleSeconds', () => {
  it('parses a positive integer string', () => {
    expect(parseScheduleSeconds('3600')).toBe(3600);
  });

  it('floors fractional input', () => {
    expect(parseScheduleSeconds('90.7')).toBe(90);
  });

  it('rejects blank, zero, negative, or non-numeric input', () => {
    expect(parseScheduleSeconds('')).toBeNull();
    expect(parseScheduleSeconds('0')).toBeNull();
    expect(parseScheduleSeconds('-5')).toBeNull();
    expect(parseScheduleSeconds('nope')).toBeNull();
  });
});
