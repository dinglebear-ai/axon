import { describe, expect, it, vi } from 'vitest';

import { AxonClient } from './axon-client';

describe('AxonClient panel transport', () => {
  it('maps REST paths to the panel proxy and sends only the panel header', async () => {
    const fetcher = vi.fn().mockResolvedValue(new Response('{"items":[]}', { status: 200 }));
    const client = new AxonClient({
      pathPrefix: '/api/panel',
      headers: { 'x-axon-panel-token': 'panel-secret' },
      fetch: fetcher
    });

    await client.listWatches({ limit: 50 });

    expect(fetcher).toHaveBeenCalledWith(
      '/api/panel/watches?limit=50',
      expect.objectContaining({ method: 'GET' })
    );
    const headers = fetcher.mock.calls[0][1].headers as Headers;
    expect(headers.get('x-axon-panel-token')).toBe('panel-secret');
    expect(headers.has('authorization')).toBe(false);
  });

  it('uses the panel proxy credential for sources, watches, and memories', async () => {
    const fetcher = vi.fn().mockImplementation(() => Promise.resolve(new Response('{}', { status: 200 })));
    const client = new AxonClient({
      pathPrefix: '/api/panel',
      headers: { 'x-axon-panel-token': 'panel-secret' },
      fetch: fetcher
    });

    await Promise.all([
      client.sources({ limit: 1 }),
      client.submitSource({ source: 'https://example.com' }),
      client.listWatches({ limit: 1 }),
      client.updateWatch('watch 1', {}),
      client.pauseWatch('watch 1'),
      client.resumeWatch('watch 1'),
      client.deleteWatch('watch 1'),
      client.searchMemories({ query: 'needle' }),
      client.rememberMemory({ body: 'fact' }),
      client.showMemory('memory 1'),
      client.deleteMemory('memory 1')
    ]);

    expect(fetcher).toHaveBeenCalledTimes(11);
    for (const [url, init] of fetcher.mock.calls) {
      expect(url).toMatch(/^\/api\/panel\/(sources|watches|memories)/);
      const headers = init.headers as Headers;
      expect(headers.get('x-axon-panel-token')).toBe('panel-secret');
      expect(headers.has('authorization')).toBe(false);
    }
  });
});
