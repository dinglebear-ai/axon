// @vitest-environment jsdom

import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ArtifactRow, CommandResultCard, EmptyState, SourceListRow, SourceSubmitResultCard } from './panel-components';
import type { ArtifactHandle, CommandResultView, SourceListEntry } from './panel-types';
import type { SourceResult } from '../api/axon-client';

const artifact: ArtifactHandle = {
  artifact_id: 'art_screenshot_123',
  bytes: 32,
  artifact_kind: 'screenshot'
};

function commandResult(overrides: Partial<CommandResultView> = {}): CommandResultView {
  return {
    ok: true,
    title: 'Screenshot captured',
    subtitle: 'art_screenshot_123',
    rows: [],
    imageUrl: '/api/panel/artifacts/art_screenshot_123/content',
    imageArtifact: artifact,
    artifacts: [artifact],
    ...overrides
  };
}

describe('panel artifact rendering', () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement('div');
    document.body.appendChild(host);
    root = createRoot(host);
    vi.stubGlobal('fetch', vi.fn());
    vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:artifact');
    vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => undefined);
  });

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    host.remove();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it('keeps the artifact row visible while loading the authenticated preview', async () => {
    vi.mocked(fetch).mockResolvedValue(
      new Response('png', {
        headers: { 'content-type': 'image/png', 'content-length': '3' }
      })
    );

    await act(async () => {
      root.render(<CommandResultCard result={commandResult()} panelToken="panel-token" />);
    });

    expect(fetch).toHaveBeenCalledWith('/api/panel/artifacts/art_screenshot_123/content', {
      headers: { 'x-axon-panel-token': 'panel-token' }
    });
    expect(host.querySelector('.artifact-name')?.textContent).toBe('art_screenshot_123');
    expect(host.querySelector('img')?.getAttribute('src')).toBe('blob:artifact');
  });

  it('shows preview errors when the server returns non-raster content', async () => {
    vi.mocked(fetch).mockResolvedValue(
      new Response('{}', {
        headers: { 'content-type': 'application/json', 'content-length': '2' }
      })
    );

    await act(async () => {
      root.render(<CommandResultCard result={commandResult()} panelToken="panel-token" />);
    });

    expect(host.textContent).toContain('Preview unavailable: artifact is application/json, not a previewable image');
    expect(host.querySelector('.artifact-name')?.textContent).toBe('art_screenshot_123');
  });

  it('rejects oversized previews via blob size when content-length is absent', async () => {
    // No content-length header, so the header-based cap is bypassed; the
    // streaming body cap is the real defense and must still trip.
    const oversized = new Response(new Uint8Array(9 * 1024 * 1024), {
      headers: { 'content-type': 'image/png' }
    });
    vi.mocked(fetch).mockResolvedValue(oversized);

    await act(async () => {
      root.render(<CommandResultCard result={commandResult()} panelToken="panel-token" />);
    });

    expect(host.textContent).toContain('Preview unavailable: artifact is too large to preview');
    expect(host.querySelector('img')).toBeNull();
  });

  it('revokes the object URL and shows an error when the image fails to decode', async () => {
    vi.mocked(fetch).mockResolvedValue(
      new Response('png', {
        headers: { 'content-type': 'image/png', 'content-length': '3' }
      })
    );

    await act(async () => {
      root.render(<CommandResultCard result={commandResult()} panelToken="panel-token" />);
    });

    const img = host.querySelector('img');
    expect(img?.getAttribute('src')).toBe('blob:artifact');

    await act(async () => {
      img?.dispatchEvent(new Event('error'));
    });

    expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:artifact');
    expect(host.querySelector('img')).toBeNull();
    expect(host.textContent).toContain('Preview unavailable: image decode failed');
  });

  it('shows artifact row errors when download/open fails', async () => {
    vi.mocked(fetch).mockResolvedValue(new Response('missing', { status: 404 }));

    await act(async () => {
      root.render(<ArtifactRow artifact={artifact} panelToken="panel-token" />);
    });

    const button = host.querySelector('button');
    expect(button).not.toBeNull();
    await act(async () => {
      button?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    });

    expect(host.textContent).toContain('Could not open art_screenshot_123: artifact fetch failed with 404');
    expect(fetch).toHaveBeenCalledWith('/api/panel/artifacts/art_screenshot_123/content', {
      headers: { 'x-axon-panel-token': 'panel-token' }
    });
  });
});

describe('Sources tab rendering', () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement('div');
    document.body.appendChild(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    host.remove();
  });

  it('renders a source list row with family, adapter, and chunk count', () => {
    const entry: SourceListEntry = {
      canonical_uri: 'https://example.com/docs',
      source_kind: 'web',
      adapter: { name: 'web', version: '1' },
      status: 'completed',
      counts: { chunks_total: 42 }
    };

    act(() => {
      root.render(<SourceListRow entry={entry} />);
    });

    expect(host.textContent).toContain('https://example.com/docs');
    expect(host.textContent).toContain('web');
    expect(host.textContent).toContain('42 chunks');
  });

  it('falls back to legacy url/chunks fields when ledger fields are absent', () => {
    const entry: SourceListEntry = { url: 'https://legacy.example.com', chunks: 7 };

    act(() => {
      root.render(<SourceListRow entry={entry} />);
    });

    expect(host.textContent).toContain('https://legacy.example.com');
    expect(host.textContent).toContain('7 chunks');
  });

  it('shows an empty state with no sources indexed', () => {
    act(() => {
      root.render(<EmptyState loading={false} text="No sources indexed yet." />);
    });

    expect(host.textContent).toContain('No sources indexed yet.');
  });

  it('shows a loading empty state while sources are being fetched', () => {
    act(() => {
      root.render(<EmptyState loading text="No sources indexed yet." />);
    });

    expect(host.textContent).toContain('Checking...');
  });

  it('renders a successful source submission result with counts', () => {
    const result: SourceResult = {
      job_id: 'job-1',
      source_id: 'source-1',
      canonical_uri: 'https://example.com/docs',
      source_kind: 'web',
      adapter: { name: 'web', version: '1' },
      scope: 'site',
      status: 'completed',
      ledger: { source_id: 'source-1', generation: 'gen-1', status: 'completed', counts: emptyCounts() },
      graph: { nodes_upserted: 0, edges_upserted: 0, evidence_records: 0, degraded: false },
      counts: { ...emptyCounts(), items_total: 3, documents_total: 3, chunks_total: 12, vector_points_total: 12 },
      warnings: []
    };

    act(() => {
      root.render(<SourceSubmitResultCard result={result} />);
    });

    expect(host.textContent).toContain('https://example.com/docs');
    expect(host.textContent).toContain('source source-1');
    expect(host.textContent).toContain('3 items');
    expect(host.textContent).toContain('12 chunks');
  });

  it('surfaces submission warnings as error rows', () => {
    const result: SourceResult = {
      job_id: 'job-2',
      source_id: 'source-2',
      canonical_uri: 'https://example.com/degraded',
      source_kind: 'web',
      adapter: { name: 'web', version: '1' },
      scope: 'site',
      status: 'completed_degraded',
      ledger: { source_id: 'source-2', generation: 'gen-1', status: 'completed_degraded', counts: emptyCounts() },
      graph: { nodes_upserted: 0, edges_upserted: 0, evidence_records: 0, degraded: true },
      counts: emptyCounts(),
      warnings: [{ code: 'thin_page', severity: 'warning', message: 'Page content was too thin to index', retryable: true }]
    };

    act(() => {
      root.render(<SourceSubmitResultCard result={result} />);
    });

    expect(host.textContent).toContain('warning: Page content was too thin to index');
  });
});

function emptyCounts() {
  return {
    items_total: 0,
    items_changed: 0,
    documents_total: 0,
    chunks_total: 0,
    vector_points_total: 0,
    bytes_total: 0
  };
}
