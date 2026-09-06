// @vitest-environment jsdom

import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { usePanelData } from './use-panel-data';

function Probe() {
  const panel = usePanelData();
  return (
    <div>
      <input
        aria-label="password"
        value={panel.password}
        onChange={(event) => panel.setPassword(event.target.value)}
      />
      <button onClick={() => void panel.login()}>login</button>
      <output>{panel.message}</output>
    </div>
  );
}

function jsonResponse(body: unknown, init: ResponseInit = {}): Response {
  return new Response(JSON.stringify(body), { ...init });
}

async function flush(times = 5): Promise<void> {
  for (let index = 0; index < times; index += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
}

describe('usePanelData login state', () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(async () => {
    window.sessionStorage.clear();
    host = document.createElement('div');
    document.body.appendChild(host);
    root = createRoot(host);
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse({ setup_required: false })));
    await act(async () => {
      root.render(<Probe />);
      await flush();
    });
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it.each([
    ['a network failure', () => Promise.reject(new TypeError('connection reset')), 'connection reset'],
    ['an HTTP text failure', () => Promise.resolve(new Response('proxy unavailable', { status: 502 })), 'proxy unavailable'],
    ['malformed success JSON', () => Promise.resolve(new Response('not-json', { status: 200 })), 'invalid JSON']
  ])('renders %s without an unhandled rejection', async (_label, response, expected) => {
    vi.mocked(fetch).mockImplementationOnce(response);

    await act(async () => {
      (host.querySelector('button') as HTMLButtonElement).click();
      await flush();
    });

    expect(host.querySelector('output')?.textContent).toContain('Login failed');
    expect(host.querySelector('output')?.textContent).toContain(expected);
    expect(window.sessionStorage.length).toBe(0);
  });
});
