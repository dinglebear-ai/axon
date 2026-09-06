// @vitest-environment jsdom

import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { MemoryTab } from './memory-tab';
import type { MemoryItem } from '../../lib/panel-types';

function memoryItem(overrides: Partial<MemoryItem> = {}): MemoryItem {
  return {
    id: 'mem-1',
    memory_type: 'fact',
    title: 'Qdrant runs on tootie',
    body: 'The vector store lives on the NAS.',
    project: 'axon',
    repo: null,
    file: null,
    confidence: 1,
    status: 'active',
    created_at: 1_700_000_000_000,
    updated_at: 1_700_000_000_000,
    last_seen_at: 1_700_000_000_000,
    access_count: 0,
    score: 0.87,
    ...overrides
  };
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

/**
 * Sets a controlled React input/textarea's value via the native value
 * setter (bypassing the React-patched setter) then dispatches an `input`
 * event, so React's onChange handler observes the new value. Directly
 * assigning `.value` and dispatching `input` is a no-op for controlled
 * elements because React's setter tracks the "previous value" itself.
 */
function setInputValue(element: HTMLInputElement | HTMLTextAreaElement, value: string): void {
  const prototype = element instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  const setter = Object.getOwnPropertyDescriptor(prototype, 'value')?.set;
  setter?.call(element, value);
  element.dispatchEvent(new Event('input', { bubbles: true }));
}

describe('MemoryTab', () => {
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

  it('does not search until the user submits a query', async () => {
    await act(async () => {
      root.render(<MemoryTab token="panel-token" />);
      await flush();
    });

    expect(fetch).not.toHaveBeenCalled();
    expect(host.textContent).toContain('No search run yet');
  });

  it('renders search results from POST /v1/memories/search', async () => {
    vi.mocked(fetch).mockResolvedValue(
      jsonResponse({ memories: [memoryItem(), memoryItem({ id: 'mem-2', title: 'Second memory' })] })
    );

    await act(async () => {
      root.render(<MemoryTab token="panel-token" />);
      await flush();
    });

    const input = host.querySelector('input[placeholder="leave blank to list recent memories"]') as HTMLInputElement;
    await act(async () => {
      setInputValue(input, 'qdrant');
    });
    await act(async () => {
      (host.querySelector('button') as HTMLButtonElement).click();
      await flush();
    });

    expect(fetch).toHaveBeenCalledWith(
      '/api/panel/memories/search',
      expect.objectContaining({
        method: 'POST',
        headers: expect.objectContaining({})
      })
    );
    expect(host.textContent).toContain('Qdrant runs on tootie');
    expect(host.textContent).toContain('Second memory');
    expect(host.textContent).toContain('2 matches for "qdrant"');
  });

  it('shows the empty state when a search returns no memories', async () => {
    vi.mocked(fetch).mockResolvedValue(jsonResponse({ memories: [] }));

    await act(async () => {
      root.render(<MemoryTab token="panel-token" />);
      await flush();
    });

    await act(async () => {
      (host.querySelector('button') as HTMLButtonElement).click();
      await flush();
    });

    expect(host.textContent).toContain('No memories found.');
  });

  it('shows an error message when the search request fails', async () => {
    vi.mocked(fetch).mockResolvedValue(new Response('boom', { status: 500 }));

    await act(async () => {
      root.render(<MemoryTab token="panel-token" />);
      await flush();
    });

    await act(async () => {
      (host.querySelector('button') as HTMLButtonElement).click();
      await flush();
    });

    expect(host.querySelector('p.error')?.textContent).toContain('HTTP 500');
  });

  it('remembers a new memory via POST /v1/memories and resets the form', async () => {
    vi.mocked(fetch).mockResolvedValue(jsonResponse({ memory: memoryItem({ id: 'mem-new', title: 'New memory' }) }));

    await act(async () => {
      root.render(<MemoryTab token="panel-token" />);
      await flush();
    });

    const bodyField = host.querySelector('textarea') as HTMLTextAreaElement;
    await act(async () => {
      setInputValue(bodyField, 'Remember this fact.');
    });

    const rememberButton = Array.from(host.querySelectorAll('button')).find((button) =>
      button.textContent?.includes('Remember')
    ) as HTMLButtonElement;

    await act(async () => {
      rememberButton.click();
      await flush();
    });

    expect(fetch).toHaveBeenCalledWith(
      '/api/panel/memories',
      expect.objectContaining({
        method: 'POST',
        body: expect.stringContaining('Remember this fact.')
      })
    );
    expect(host.textContent).toContain('Saved memory mem-new');
    expect(bodyField.value).toBe('');
  });

  it('rejects an empty body without calling the API', async () => {
    await act(async () => {
      root.render(<MemoryTab token="panel-token" />);
      await flush();
    });

    const rememberButton = Array.from(host.querySelectorAll('button')).find((button) =>
      button.textContent?.includes('Remember')
    ) as HTMLButtonElement;

    expect(rememberButton.disabled).toBe(true);
    expect(fetch).not.toHaveBeenCalled();
  });

  it('views a memory detail via GET /v1/memories/{id} and deletes it with confirmation', async () => {
    vi.stubGlobal('confirm', vi.fn(() => true));
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse({ memories: [memoryItem()] }))
      .mockResolvedValueOnce(jsonResponse({ memory: memoryItem() }))
      .mockResolvedValueOnce(jsonResponse({ memory: memoryItem() }));

    await act(async () => {
      root.render(<MemoryTab token="panel-token" />);
      await flush();
    });

    await act(async () => {
      (host.querySelector('button') as HTMLButtonElement).click();
      await flush();
    });

    await act(async () => {
      (host.querySelector('[title="View memory"]') as HTMLButtonElement).click();
      await flush();
    });

    expect(fetch).toHaveBeenCalledWith('/api/panel/memories/mem-1', expect.anything());
    expect(host.textContent).toContain('The vector store lives on the NAS.');

    await act(async () => {
      (host.querySelector('[title="Delete memory"]') as HTMLButtonElement).click();
      await flush();
    });

    expect(window.confirm).toHaveBeenCalled();
    expect(fetch).toHaveBeenCalledWith('/api/panel/memories/mem-1', expect.objectContaining({ method: 'DELETE' }));
  });

  it('ignores stale reordered detail responses and deletes the currently selected memory', async () => {
    vi.stubGlobal('confirm', vi.fn(() => true));
    let resolveFirst!: (response: Response) => void;
    let resolveSecond!: (response: Response) => void;
    const firstDetail = new Promise<Response>((resolve) => { resolveFirst = resolve; });
    const secondDetail = new Promise<Response>((resolve) => { resolveSecond = resolve; });
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse({ memories: [
        memoryItem({ id: 'mem-a', title: 'Memory A', body: 'body A' }),
        memoryItem({ id: 'mem-b', title: 'Memory B', body: 'body B' })
      ] }))
      .mockImplementationOnce(() => firstDetail)
      .mockImplementationOnce(() => secondDetail)
      .mockResolvedValueOnce(jsonResponse({ memory: memoryItem({ id: 'mem-b' }) }));

    await act(async () => {
      root.render(<MemoryTab token="panel-token" />);
      await flush();
    });
    await act(async () => {
      (host.querySelector('button') as HTMLButtonElement).click();
      await flush();
    });

    const viewButtons = host.querySelectorAll<HTMLButtonElement>('[title="View memory"]');
    await act(async () => {
      viewButtons[0].click();
      viewButtons[1].click();
      resolveSecond(jsonResponse({ memory: memoryItem({ id: 'mem-b', title: 'Memory B', body: 'body B' }) }));
      await flush();
    });
    await act(async () => {
      resolveFirst(jsonResponse({ memory: memoryItem({ id: 'mem-a', title: 'Memory A', body: 'body A' }) }));
      await flush();
    });

    expect(host.textContent).toContain('body B');
    expect(host.textContent).not.toContain('body A');
    const detailDelete = Array.from(host.querySelectorAll<HTMLButtonElement>('[title="Delete memory"]')).at(-1)!;
    await act(async () => {
      detailDelete.click();
      await flush();
    });
    expect(fetch).toHaveBeenCalledWith('/api/panel/memories/mem-b', expect.objectContaining({ method: 'DELETE' }));
  });

  it('does not publish a detail response after the detail is closed', async () => {
    let resolveDetail!: (response: Response) => void;
    const detail = new Promise<Response>((resolve) => { resolveDetail = resolve; });
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse({ memories: [memoryItem()] }))
      .mockImplementationOnce(() => detail);

    await act(async () => {
      root.render(<MemoryTab token="panel-token" />);
      await flush();
    });
    await act(async () => {
      (host.querySelector('button') as HTMLButtonElement).click();
      await flush();
    });
    await act(async () => {
      (host.querySelector('[title="View memory"]') as HTMLButtonElement).click();
      await flush();
    });
    await act(async () => {
      (host.querySelector('[title="Close detail"]') as HTMLButtonElement).click();
      resolveDetail(jsonResponse({ memory: memoryItem({ body: 'late body' }) }));
      await flush();
    });

    expect(host.textContent).not.toContain('late body');
    expect(host.querySelector('[title="Close detail"]')).toBeNull();
  });
});
