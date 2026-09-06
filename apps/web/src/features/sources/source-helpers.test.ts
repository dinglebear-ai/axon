import { describe, expect, it } from 'vitest';

import { normalizeSourceEntries, parseMaxPages, sourcesSummaryLabel } from './source-helpers';

describe('parseMaxPages', () => {
  it.each([
    ['', null],
    ['   ', null],
    ['0', 0],
    ['1', 1],
    ['42', 42]
  ])('accepts server-representable input %j', (value, expected) => {
    expect(parseMaxPages(value)).toEqual({ ok: true, value: expected });
  });

  it.each(['1.5', '-1', 'NaN', 'Infinity', String(Number.MAX_SAFE_INTEGER + 1)])(
    'rejects non-u64-style input %j',
    (value) => {
      expect(parseMaxPages(value)).toEqual({
        ok: false,
        message: 'Max pages must be a non-negative safe integer.'
      });
    }
  );
});

describe('generated SourcesResponse projection', () => {
  it('projects indexed source tuples into display entries', () => {
    const response = { count: 2, limit: 50, offset: 0, urls: [['https://a.test', 7], ['https://b.test', 3]] };
    expect(normalizeSourceEntries(response)).toEqual([
      { url: 'https://a.test', chunks: 7 },
      { url: 'https://b.test', chunks: 3 }
    ]);
    expect(sourcesSummaryLabel(response)).toBe('2 shown of 2');
  });

  it('projects domain source strings into display entries', () => {
    const response = {
      domain: 'example.com', count: 1, limit: 50, cursor: null, next_cursor: null,
      truncated: false, urls: ['https://example.com/docs']
    };
    expect(normalizeSourceEntries(response)).toEqual([{ url: 'https://example.com/docs' }]);
  });
});
