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
      root.render(<MemoryTab />);
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
      root.render(<MemoryTab />);
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
      '/v1/memories/search',
      expect.objectContaining({ method: 'POST' })
    );
    expect(host.textContent).toContain('Qdrant runs on tootie');
    expect(host.textContent).toContain('Second memory');
    expect(host.textContent).toContain('2 matches for "qdrant"');
  });

  it('shows the empty state when a search returns no memories', async () => {
    vi.mocked(fetch).mockResolvedValue(jsonResponse({ memories: [] }));

    await act(async () => {
      root.render(<MemoryTab />);
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
      root.render(<MemoryTab />);
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
      root.render(<MemoryTab />);
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
      '/v1/memories',
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
      root.render(<MemoryTab />);
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
      root.render(<MemoryTab />);
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

    expect(fetch).toHaveBeenCalledWith('/v1/memories/mem-1', expect.anything());
    expect(host.textContent).toContain('The vector store lives on the NAS.');

    await act(async () => {
      (host.querySelector('[title="Delete memory"]') as HTMLButtonElement).click();
      await flush();
    });

    expect(window.confirm).toHaveBeenCalled();
    expect(fetch).toHaveBeenCalledWith('/v1/memories/mem-1', expect.objectContaining({ method: 'DELETE' }));
  });
});
