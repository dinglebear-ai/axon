// @vitest-environment jsdom

import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { WatchListRow, WatchesTab } from './watches-tab';
import type { WatchPage, WatchSummary } from './panel-types';

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

function watchPage(items: WatchSummary[], overrides: Partial<WatchPage> = {}): WatchPage {
  return { items, next_cursor: null, limit: 50, total: items.length, ...overrides };
}

function jsonResponse(body: unknown, init: ResponseInit = {}): Response {
  return new Response(JSON.stringify(body), {
    headers: { 'content-type': 'application/json' },
    ...init
  });
}

async function flush(times = 5): Promise<void> {
  for (let i = 0; i < times; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
}

describe('WatchListRow', () => {
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

  it('renders schedule and status for an enabled watch', () => {
    act(() => {
      root.render(
        <WatchListRow entry={watchSummary()} busy={false} onPause={vi.fn()} onResume={vi.fn()} onEdit={vi.fn()} onDelete={vi.fn()} />
      );
    });

    expect(host.textContent).toContain('src-1');
    expect(host.textContent).toContain('every 1h');
    expect(host.textContent).toContain('completed');
  });

  it('shows a resume control and paused meta for a disabled watch', () => {
    act(() => {
      root.render(
        <WatchListRow
          entry={watchSummary({ enabled: false, last_status: null })}
          busy={false}
          onPause={vi.fn()}
          onResume={vi.fn()}
          onEdit={vi.fn()}
          onDelete={vi.fn()}
        />
      );
    });

    expect(host.textContent).toContain('paused');
    expect(host.querySelector('[title="Resume watch"]')).not.toBeNull();
    expect(host.querySelector('[title="Pause watch"]')).toBeNull();
  });

  it('wires pause/resume/edit/delete button clicks to their handlers', () => {
    const onPause = vi.fn();
    const onEdit = vi.fn();
    const onDelete = vi.fn();

    act(() => {
      root.render(
        <WatchListRow entry={watchSummary()} busy={false} onPause={onPause} onResume={vi.fn()} onEdit={onEdit} onDelete={onDelete} />
      );
    });

    (host.querySelector('[title="Pause watch"]') as HTMLButtonElement).click();
    (host.querySelector('[title="Edit watch"]') as HTMLButtonElement).click();
    (host.querySelector('[title="Delete watch"]') as HTMLButtonElement).click();

    expect(onPause).toHaveBeenCalledWith('watch-1');
    expect(onEdit).toHaveBeenCalledWith('watch-1');
    expect(onDelete).toHaveBeenCalledWith('watch-1');
  });

  it('disables action buttons while busy', () => {
    act(() => {
      root.render(
        <WatchListRow entry={watchSummary()} busy onPause={vi.fn()} onResume={vi.fn()} onEdit={vi.fn()} onDelete={vi.fn()} />
      );
    });

    host.querySelectorAll('button').forEach((button) => {
      expect(button.disabled).toBe(true);
    });
  });
});

describe('WatchesTab', () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement('div');
    document.body.appendChild(host);
    root = createRoot(host);
    vi.stubGlobal('fetch', vi.fn());
  });

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    host.remove();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it('renders a list of watches from GET /v1/watches', async () => {
    vi.mocked(fetch).mockResolvedValue(jsonResponse(watchPage([watchSummary(), watchSummary({ watch_id: 'watch-2', source_id: 'src-2' })])));

    await act(async () => {
      root.render(<WatchesTab token="panel-token" active />);
      await flush();
    });

    expect(fetch).toHaveBeenCalledWith('/v1/watches?limit=50', expect.anything());
    expect(host.textContent).toContain('src-1');
    expect(host.textContent).toContain('src-2');
    expect(host.textContent).toContain('2 shown of 2');
  });

  it('shows the empty state when no watches are configured', async () => {
    vi.mocked(fetch).mockResolvedValue(jsonResponse(watchPage([])));

    await act(async () => {
      root.render(<WatchesTab token="panel-token" active />);
      await flush();
    });

    expect(host.textContent).toContain('No watches configured yet.');
  });

  it('shows an error message when the request fails', async () => {
    vi.mocked(fetch).mockResolvedValue(new Response('boom', { status: 500 }));

    await act(async () => {
      root.render(<WatchesTab token="panel-token" active />);
      await flush();
    });

    expect(host.querySelector('p.error')?.textContent).toContain('HTTP 500');
  });

  it('does not fetch when the tab is not active', async () => {
    await act(async () => {
      root.render(<WatchesTab token="panel-token" active={false} />);
      await flush();
    });

    expect(fetch).not.toHaveBeenCalled();
  });

  it('wires the pause action to POST /v1/watches/{id}/pause and refreshes the list', async () => {
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse(watchPage([watchSummary()])))
      .mockResolvedValueOnce(jsonResponse({ ...watchSummary(), enabled: false }))
      .mockResolvedValueOnce(jsonResponse(watchPage([watchSummary({ enabled: false, last_status: null })])));

    await act(async () => {
      root.render(<WatchesTab token="panel-token" active />);
      await flush();
    });

    await act(async () => {
      (host.querySelector('[title="Pause watch"]') as HTMLButtonElement).click();
      await flush();
    });

    expect(fetch).toHaveBeenCalledWith('/v1/watches/watch-1/pause', expect.objectContaining({ method: 'POST' }));
    expect(fetch).toHaveBeenCalledTimes(3);
  });

  it('wires the resume action to POST /v1/watches/{id}/resume', async () => {
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse(watchPage([watchSummary({ enabled: false, last_status: null })])))
      .mockResolvedValueOnce(jsonResponse(watchSummary()))
      .mockResolvedValueOnce(jsonResponse(watchPage([watchSummary()])));

    await act(async () => {
      root.render(<WatchesTab token="panel-token" active />);
      await flush();
    });

    await act(async () => {
      (host.querySelector('[title="Resume watch"]') as HTMLButtonElement).click();
      await flush();
    });

    expect(fetch).toHaveBeenCalledWith('/v1/watches/watch-1/resume', expect.objectContaining({ method: 'POST' }));
  });

  it('confirms before deleting and wires DELETE /v1/watches/{id}', async () => {
    vi.stubGlobal('confirm', vi.fn(() => true));
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse(watchPage([watchSummary()])))
      .mockResolvedValueOnce(jsonResponse({ watch_id: 'watch-1', deleted: true }))
      .mockResolvedValueOnce(jsonResponse(watchPage([])));

    await act(async () => {
      root.render(<WatchesTab token="panel-token" active />);
      await flush();
    });

    await act(async () => {
      (host.querySelector('[title="Delete watch"]') as HTMLButtonElement).click();
      await flush();
    });

    expect(window.confirm).toHaveBeenCalled();
    expect(fetch).toHaveBeenCalledWith('/v1/watches/watch-1', expect.objectContaining({ method: 'DELETE' }));
  });

  it('skips the delete request when the confirmation is declined', async () => {
    vi.stubGlobal('confirm', vi.fn(() => false));
    vi.mocked(fetch).mockResolvedValueOnce(jsonResponse(watchPage([watchSummary()])));

    await act(async () => {
      root.render(<WatchesTab token="panel-token" active />);
      await flush();
    });

    await act(async () => {
      (host.querySelector('[title="Delete watch"]') as HTMLButtonElement).click();
      await flush();
    });

    expect(fetch).toHaveBeenCalledTimes(1);
  });
});
