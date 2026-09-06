import { describe, expect, it, vi } from 'vitest';

import { PanelRequestError, requestPanelJson } from './panel-request';

describe('requestPanelJson', () => {
  it('returns decoded JSON for a successful response', async () => {
    const fetcher = vi.fn().mockResolvedValue(new Response('{"ok":true}', { status: 200 }));
    await expect(requestPanelJson<{ ok: boolean }>('/ok', {}, fetcher)).resolves.toEqual({ ok: true });
  });

  it('preserves a text error response', async () => {
    const fetcher = vi.fn().mockResolvedValue(new Response('proxy unavailable', { status: 502 }));
    await expect(requestPanelJson('/bad', {}, fetcher)).rejects.toEqual(
      expect.objectContaining({ message: 'proxy unavailable', status: 502 })
    );
  });

  it('reports malformed success JSON', async () => {
    const fetcher = vi.fn().mockResolvedValue(new Response('not-json', { status: 200 }));
    await expect(requestPanelJson('/bad-json', {}, fetcher)).rejects.toThrow('returned invalid JSON');
  });

  it('normalizes transport failures', async () => {
    const fetcher = vi.fn().mockRejectedValue(new TypeError('connection reset'));
    await expect(requestPanelJson('/offline', {}, fetcher)).rejects.toBeInstanceOf(PanelRequestError);
    await expect(requestPanelJson('/offline', {}, fetcher)).rejects.toThrow('connection reset');
  });
});
