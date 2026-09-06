import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { startCompletionDrivenPolling } from './panel-polling';

describe('startCompletionDrivenPolling', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('never overlaps polling generations', async () => {
    let resolvePoll!: () => void;
    const poll = vi.fn(() => new Promise<void>((resolve) => { resolvePoll = resolve; }));
    const stop = startCompletionDrivenPolling({ poll, intervalMs: 5_000, isVisible: () => true });

    await vi.advanceTimersByTimeAsync(15_000);
    expect(poll).toHaveBeenCalledTimes(1);
    resolvePoll();
    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(4_999);
    expect(poll).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(poll).toHaveBeenCalledTimes(2);
    stop();
  });

  it('does not poll while the document is hidden', async () => {
    let visible = false;
    const poll = vi.fn().mockResolvedValue(undefined);
    const stop = startCompletionDrivenPolling({ poll, intervalMs: 5_000, isVisible: () => visible });

    await vi.advanceTimersByTimeAsync(15_000);
    expect(poll).not.toHaveBeenCalled();
    visible = true;
    await vi.advanceTimersByTimeAsync(5_000);
    expect(poll).toHaveBeenCalledTimes(1);
    stop();
  });

  it('does not schedule another request after stop', async () => {
    const poll = vi.fn().mockResolvedValue(undefined);
    const stop = startCompletionDrivenPolling({ poll, intervalMs: 5_000, isVisible: () => true });
    stop();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(poll).not.toHaveBeenCalled();
  });
});
